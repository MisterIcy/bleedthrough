//! The public invariant checker for syntax trees.

use crate::span::Span;
use crate::tree::SyntaxTree;
use crate::walk::{NodeRef, visit_tree};

/// Term: exempt
///
/// Which invariant a [`Violation`] breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum InvariantKind {
    /// A span has `start > end` or reaches past the end of the file.
    Bounds,
    /// A child's span is not inside its parent's span.
    Containment,
    /// Sibling spans overlap or are out of source order.
    SiblingOrder,
    /// A non-whitespace byte is in no statement, comment, error node or unsupported node.
    Coverage,
}

/// Term: exempt
///
/// One broken invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    clippy::exhaustive_structs,
    reason = "a plain record of kind, span and message"
)]
pub struct Violation {
    /// The invariant.
    pub kind: InvariantKind,
    /// The span concerned.
    pub span: Span,
    /// What is wrong.
    pub message: String,
}

fn violation(kind: InvariantKind, span: Span, message: String) -> Violation {
    Violation {
        kind,
        span,
        message,
    }
}

fn check_node(node: NodeRef<'_>, len: u32, out: &mut Vec<Violation>) {
    let span = node.span();
    let name = node.kind_name();
    if span.start > span.end || span.end > len {
        let message = format!(
            "{name} span {}..{} is out of bounds (file length {len})",
            span.start, span.end
        );
        out.push(violation(InvariantKind::Bounds, span, message));
    }
    check_siblings(node.children(), span, name, out);
}

fn check_siblings(children: Vec<NodeRef<'_>>, parent: Span, name: &str, out: &mut Vec<Violation>) {
    let mut previous: Option<Span> = None;
    for child in children {
        let span = child.span();
        if !parent.contains(span) {
            let message = format!(
                "{} {}..{} is not inside its parent {name} {}..{}",
                child.kind_name(),
                span.start,
                span.end,
                parent.start,
                parent.end
            );
            out.push(violation(InvariantKind::Containment, span, message));
        }
        if let Some(prev) = previous.filter(|p| p.end > span.start) {
            let message = format!(
                "{} {}..{} overlaps or precedes its previous sibling {}..{}",
                child.kind_name(),
                span.start,
                span.end,
                prev.start,
                prev.end
            );
            out.push(violation(InvariantKind::SiblingOrder, span, message));
        }
        previous = Some(span);
    }
}

/// Reports significant bytes between neighbours of every statement or member list. The model does
/// not record where a list's delimiters are, so bytes before the first and after the last item of
/// a list cannot be told from the delimiters themselves and are not checked here.
fn check_list_gaps(tree: &SyntaxTree, source: &[u8], out: &mut Vec<Violation>) {
    let mut comments: Vec<Span> = tree.comments().iter().map(|c| c.span).collect();
    comments.sort();
    visit_tree(tree, &mut |node| {
        for list in node.child_lists() {
            for pair in list.windows(2) {
                let gap = Span::new(pair[0].span().end, pair[1].span().start);
                report_uncovered_outside(source, gap, &comments, out);
            }
        }
    });
}

fn report_uncovered_outside(source: &[u8], gap: Span, comments: &[Span], out: &mut Vec<Violation>) {
    if gap.start >= gap.end {
        return;
    }
    let bytes = source
        .get(gap.start as usize..gap.end as usize)
        .unwrap_or(&[]);
    for (offset, byte) in bytes.iter().enumerate() {
        let at = gap.start + u32::try_from(offset).unwrap_or(0);
        let index = comments.partition_point(|c| c.end <= at);
        let in_comment = comments.get(index).is_some_and(|c| c.start <= at);
        if !is_php_whitespace(*byte) && !in_comment {
            let message =
                format!("byte at offset {at} lies between two list items and belongs to neither");
            out.push(violation(
                InvariantKind::Coverage,
                Span::new(at, at + 1),
                message,
            ));
            return;
        }
    }
}

fn is_php_whitespace(byte: u8) -> bool {
    byte.is_ascii_whitespace() || byte == 0x0B
}

fn check_coverage(tree: &SyntaxTree, source: &[u8], out: &mut Vec<Violation>) {
    let mut spans: Vec<Span> = tree.statements().iter().map(|s| s.span).collect();
    spans.extend(tree.comments().iter().map(|c| c.span));
    spans.sort();
    let mut cursor: u32 = 0;
    let end_of_file = u32::try_from(source.len()).unwrap_or(u32::MAX);
    for span in spans
        .into_iter()
        .chain([Span::new(end_of_file, end_of_file)])
    {
        if span.start > cursor {
            report_uncovered(source, Span::new(cursor, span.start.min(end_of_file)), out);
        }
        cursor = cursor.max(span.end);
    }
}

fn report_uncovered(source: &[u8], gap: Span, out: &mut Vec<Violation>) {
    let bytes = source
        .get(gap.start as usize..gap.end as usize)
        .unwrap_or(&[]);
    if let Some(offset) = bytes.iter().position(|b| !is_php_whitespace(*b)) {
        let at = gap.start + u32::try_from(offset).unwrap_or(0);
        let message =
            format!("byte at offset {at} is covered by no statement, comment or error node");
        out.push(violation(
            InvariantKind::Coverage,
            Span::new(at, at + 1),
            message,
        ));
    }
}

/// Checks whole-file coverage, span bounds, parent-child containment and sibling order of `tree`
/// against its `source`. Coverage does not apply to a failed parse; callers skip this check then.
pub fn check_invariants(tree: &SyntaxTree, source: &[u8]) -> Vec<Violation> {
    let mut out = Vec::new();
    let len = u32::try_from(source.len()).unwrap_or(u32::MAX);
    let tops: Vec<NodeRef<'_>> = tree.statements().iter().map(NodeRef::Stmt).collect();
    check_siblings(tops, Span::new(0, len), "file", &mut out);
    visit_tree(tree, &mut |node| check_node(node, len, &mut out));
    check_coverage(tree, source, &mut out);
    check_list_gaps(tree, source, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{BinaryOp, Expr, ExprKind};
    use crate::node::{Literal, Node};
    use crate::stmt::{Stmt, StmtKind};
    use crate::trivia::{Comment, CommentKind};

    fn int(start: u32, end: u32) -> Expr {
        Node::new(
            Span::new(start, end),
            ExprKind::Literal(Literal::Int { raw: "1".into() }),
        )
    }

    fn expr_stmt(start: u32, end: u32, e: Expr) -> Stmt {
        Node::new(Span::new(start, end), StmtKind::Expr(e))
    }

    fn kinds(violations: &[Violation]) -> Vec<InvariantKind> {
        violations.iter().map(|v| v.kind).collect()
    }

    // source: "1+2;" at offsets 0..4
    fn binary(lhs: Expr, rhs: Expr, span: Span) -> Expr {
        Node::new(
            span,
            ExprKind::Binary {
                op: BinaryOp::Add,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
        )
    }

    #[test]
    fn a_dropped_byte_inside_a_list_is_reported() {
        // "{1;@2;}": the `@` sits between the two statements of the block.
        let inner = vec![expr_stmt(1, 3, int(1, 2)), expr_stmt(4, 6, int(4, 5))];
        let block = Node::new(Span::new(0, 7), StmtKind::Block(inner));
        let tree = SyntaxTree::new(vec![block], vec![], 7);
        assert!(kinds(&check_invariants(&tree, b"{1;@2;}")).contains(&InvariantKind::Coverage));
    }

    #[test]
    fn a_comment_between_list_items_is_fine() {
        let inner = vec![expr_stmt(1, 3, int(1, 2)), expr_stmt(8, 10, int(8, 9))];
        let block = Node::new(Span::new(0, 11), StmtKind::Block(inner));
        let comment = Comment {
            kind: CommentKind::Block,
            span: Span::new(3, 8),
            text: "/*x*/".into(),
        };
        let tree = SyntaxTree::new(vec![block], vec![comment], 11);
        assert!(check_invariants(&tree, b"{1;/*x*/2;}").is_empty());
    }

    #[test]
    fn valid_tree_passes() {
        let e = binary(int(0, 1), int(2, 3), Span::new(0, 3));
        let tree = SyntaxTree::new(vec![expr_stmt(0, 4, e)], vec![], 4);
        assert!(check_invariants(&tree, b"1+2;").is_empty());
    }

    #[test]
    fn out_of_bounds_is_reported() {
        let tree = SyntaxTree::new(vec![expr_stmt(0, 9, int(0, 1))], vec![], 4);
        assert!(kinds(&check_invariants(&tree, b"1+2;")).contains(&InvariantKind::Bounds));
    }

    #[test]
    fn child_outside_parent_is_reported() {
        let e = binary(int(0, 1), int(2, 5), Span::new(0, 3));
        let tree = SyntaxTree::new(vec![expr_stmt(0, 4, e)], vec![], 6);
        assert!(kinds(&check_invariants(&tree, b"1+2;  ")).contains(&InvariantKind::Containment));
    }

    #[test]
    fn overlapping_siblings_are_reported() {
        let e = binary(int(0, 2), int(1, 3), Span::new(0, 3));
        let tree = SyntaxTree::new(vec![expr_stmt(0, 4, e)], vec![], 4);
        assert!(kinds(&check_invariants(&tree, b"1+2;")).contains(&InvariantKind::SiblingOrder));
    }

    #[test]
    fn uncovered_byte_is_reported_but_whitespace_and_comments_are_not() {
        let comment = Comment {
            kind: CommentKind::Line,
            span: Span::new(5, 9),
            text: "//ab".into(),
        };
        let tree = SyntaxTree::new(vec![expr_stmt(0, 4, int(0, 1))], vec![comment], 10);
        assert!(check_invariants(&tree, b"1+2; //ab\n").is_empty());
        let uncovered = SyntaxTree::new(vec![expr_stmt(0, 2, int(0, 1))], vec![], 4);
        let found = check_invariants(&uncovered, b"1+2;");
        assert_eq!(kinds(&found), vec![InvariantKind::Coverage]);
    }

    #[test]
    fn holes_are_counted() {
        let error = Node::new(Span::new(0, 2), StmtKind::Error { diagnostic: None });
        let unsupported = Node::new(Span::new(2, 4), StmtKind::Unsupported { kind: "X".into() });
        let nested = expr_stmt(
            4,
            6,
            Node::new(Span::new(4, 5), ExprKind::Unsupported { kind: "Y".into() }),
        );
        let tree = SyntaxTree::new(vec![error, unsupported, nested], vec![], 6);
        assert_eq!(tree.hole_counts(), (2, 1));
    }
}
