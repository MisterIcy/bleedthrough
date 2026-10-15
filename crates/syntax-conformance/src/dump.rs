//! The stable textual dump of an owned syntax tree, used for golden files and for comparing
//! backends. One node per line, two spaces of indent per level, source order, ASCII only.

use std::fmt::Write as _;

use bleedthrough_syntax::{Comment, CommentKind, NodeRef, SyntaxTree};

/// Escapes every non-ASCII character as `\u{..}`, so a dump never depends on terminal or font
/// support and never changes with the Unicode tables of a toolchain.
fn ascii_only(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let _ = write!(out, "\\u{{{:x}}}", u32::from(c));
        }
    }
    out
}

fn comment_kind(kind: CommentKind) -> &'static str {
    match kind {
        CommentKind::Line => "Line",
        CommentKind::Hash => "Hash",
        CommentKind::Block => "Block",
        CommentKind::Doc => "Doc",
        _ => "Other",
    }
}

fn node(out: &mut String, node: NodeRef<'_>, depth: usize) {
    let span = node.span();
    let _ = write!(
        out,
        "{:indent$}{} {}..{}",
        "",
        node.kind_name(),
        span.start,
        span.end,
        indent = depth * 2
    );
    if let Some(label) = node.label() {
        let _ = write!(out, " {}", ascii_only(&label));
    }
    out.push('\n');
    for child in node.children() {
        self::node(out, child, depth + 1);
    }
}

fn comment(out: &mut String, c: &Comment) {
    let _ = writeln!(
        out,
        "Comment {} {}..{} {}",
        comment_kind(c.kind),
        c.span.start,
        c.span.end,
        ascii_only(&format!("{:?}", c.text))
    );
}

/// Renders the tree: its statements, then its comments, then a closing line with the source length.
pub fn dump_tree(tree: &SyntaxTree) -> String {
    let mut out = String::new();
    for statement in tree.statements() {
        node(&mut out, NodeRef::Stmt(statement), 0);
    }
    for c in tree.comments() {
        comment(&mut out, c);
    }
    let _ = writeln!(out, "End {}", tree.source_len());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bleedthrough_syntax::{
        Expr, ExprKind, Literal, Node, QuoteKind, Span, Stmt, StmtKind, SyntaxTree,
    };

    fn string(start: u32, end: u32, raw: &str) -> Expr {
        Node::new(
            Span::new(start, end),
            ExprKind::Literal(Literal::String {
                raw: raw.into(),
                quote: QuoteKind::Double,
            }),
        )
    }

    fn statement(e: Expr) -> Stmt {
        Node::new(Span::new(e.span.start, e.span.end + 1), StmtKind::Expr(e))
    }

    #[test]
    fn a_plain_tree_dumps_in_source_order_with_indent() {
        let tree = SyntaxTree::new(vec![statement(string(0, 3, "ab"))], vec![], 4);
        let dump = dump_tree(&tree);
        let lines: Vec<&str> = dump.lines().collect();
        assert_eq!(lines[0], "Expr 0..4");
        assert!(lines[1].starts_with("  Literal 0..3 "), "{dump}");
        assert_eq!(lines.last(), Some(&"End 4"));
    }

    #[test]
    fn quotes_newlines_and_non_ascii_are_escaped_on_one_line() {
        let tree = SyntaxTree::new(
            vec![statement(string(0, 20, "a\"b\nc \u{e9}\u{1f600}"))],
            vec![],
            21,
        );
        let dump = dump_tree(&tree);
        let literal = dump.lines().nth(1).unwrap();
        assert!(literal.contains("\\\"b"), "{literal}");
        assert!(literal.contains("\\n"), "{literal}");
        assert!(literal.contains("\\u{e9}"), "{literal}");
        assert!(literal.contains("\\u{1f600}"), "{literal}");
        assert!(dump.is_ascii());
        assert_eq!(dump.lines().count(), 3);
    }

    #[test]
    fn comments_follow_statements() {
        let comment = Comment {
            kind: CommentKind::Doc,
            span: Span::new(5, 12),
            text: "/** é */".into(),
        };
        let tree = SyntaxTree::new(vec![], vec![comment], 12);
        assert_eq!(
            dump_tree(&tree),
            "Comment Doc 5..12 \"/** \\u{e9} */\"\nEnd 12\n"
        );
    }

    #[test]
    fn the_dump_is_deterministic() {
        let tree = SyntaxTree::new(vec![statement(string(0, 3, "x"))], vec![], 4);
        assert_eq!(dump_tree(&tree), dump_tree(&tree.clone()));
    }
}
