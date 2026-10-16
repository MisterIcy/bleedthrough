//! Spans, trees, parse results, docblock lookup and the invariant checker, through the public
//! API only. Written against the surviving mutants of `cargo xtask mutants`.

use bleedthrough_syntax::*;

fn int(start: u32, end: u32) -> Expr {
    Node::new(
        Span::new(start, end),
        ExprKind::Literal(Literal::Int { raw: "1".into() }),
    )
}

fn expr_stmt(start: u32, end: u32, e: Expr) -> Stmt {
    Node::new(Span::new(start, end), StmtKind::Expr(e))
}

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

fn violations(tree: &SyntaxTree, source: &[u8], kind: InvariantKind) -> Vec<Violation> {
    check_invariants(tree, source)
        .into_iter()
        .filter(|v| v.kind == kind)
        .collect()
}

/// spec: syntax-model/Spans are byte offsets with consistent nesting
#[test]
fn a_span_knows_whether_it_is_empty() {
    assert!(Span::new(3, 3).is_empty());
    assert!(Span::new(4, 3).is_empty());
    assert!(!Span::new(3, 4).is_empty());
    assert_eq!(Span::new(3, 7).len(), 4);
    assert_eq!(Span::new(7, 3).len(), 0);
}

#[test]
fn a_tree_knows_the_length_of_its_source() {
    assert_eq!(SyntaxTree::new(vec![], vec![], 17).source_len(), 17);
    assert_eq!(SyntaxTree::empty(5).source_len(), 5);
}

/// spec: syntax-model/Diagnostics and parse quality describe each file
#[test]
fn a_parse_result_counts_unsupported_and_error_nodes() {
    let error = Node::new(Span::new(0, 2), StmtKind::Error { diagnostic: None });
    let unsupported = Node::new(Span::new(2, 4), StmtKind::Unsupported { kind: "X".into() });
    let other = Node::new(Span::new(4, 6), StmtKind::Unsupported { kind: "Y".into() });
    let second_error = Node::new(Span::new(6, 8), StmtKind::Error { diagnostic: None });
    let tree = SyntaxTree::new(vec![error, unsupported, other, second_error], vec![], 8);
    let info = BackendInfo {
        name: "test".into(),
        version: "1".into(),
    };
    let parsed = ParsedFile::new(tree, vec![], info);
    assert_eq!(parsed.unsupported_count(), 2);
    assert_eq!(parsed.error_count(), 2);
}

/// spec: syntax-model/Comments and docblocks are retained raw
#[test]
fn only_an_adjacent_docblock_belongs_to_a_declaration() {
    let comment = |kind, start, end, text: &str| Comment {
        kind,
        span: Span::new(start, end),
        text: text.into(),
    };
    let source = b"/** d */ function f() {}";
    let doc = comment(CommentKind::Doc, 0, 8, "/** d */");
    assert_eq!(
        docblock_before(std::slice::from_ref(&doc), source, 9),
        Some(&doc)
    );

    // comments after the declaration are not candidates
    let later = comment(CommentKind::Block, 24, 31, "/* e */");
    let with_later = b"/** d */ function f() {} /* e */";
    assert_eq!(
        docblock_before(&[doc.clone(), later], with_later, 9),
        Some(&doc)
    );

    // a plain block comment is not a docblock, even when adjacent
    let block_source = b"/* d */ function f() {}";
    let block = comment(CommentKind::Block, 0, 7, "/* d */");
    assert_eq!(docblock_before(&[block], block_source, 8), None);

    // code between the docblock and the declaration separates them
    let separated = b"/** d */ ; function f() {}";
    let doc = comment(CommentKind::Doc, 0, 8, "/** d */");
    assert_eq!(docblock_before(&[doc], separated, 11), None);
}

// ---- invariants ----

/// spec: syntax-model/Spans are byte offsets with consistent nesting
#[test]
fn an_empty_span_is_in_bounds_but_a_reversed_one_is_not() {
    let empty = Node::new(Span::new(1, 1), StmtKind::Noop);
    let tree = SyntaxTree::new(vec![empty], vec![], 2);
    assert!(violations(&tree, b"  ", InvariantKind::Bounds).is_empty());

    let reversed = Node::new(Span::new(2, 1), StmtKind::Noop);
    let tree = SyntaxTree::new(vec![reversed], vec![], 3);
    assert!(!violations(&tree, b"   ", InvariantKind::Bounds).is_empty());

    let at_the_end = Node::new(Span::new(0, 4), StmtKind::Noop);
    let tree = SyntaxTree::new(vec![at_the_end], vec![], 4);
    assert!(violations(&tree, b"1+2;", InvariantKind::Bounds).is_empty());
}

#[test]
fn touching_siblings_are_in_order_and_overlapping_ones_are_not() {
    let touching = binary(int(0, 1), int(1, 2), Span::new(0, 2));
    let tree = SyntaxTree::new(vec![expr_stmt(0, 3, touching)], vec![], 3);
    assert!(violations(&tree, b"12;", InvariantKind::SiblingOrder).is_empty());

    let overlapping = binary(int(0, 2), int(1, 3), Span::new(0, 3));
    let tree = SyntaxTree::new(vec![expr_stmt(0, 4, overlapping)], vec![], 4);
    assert_eq!(
        violations(&tree, b"1+2;", InvariantKind::SiblingOrder).len(),
        1
    );
}

#[test]
fn an_uncovered_byte_is_reported_at_its_offset() {
    // "1  +;": the statement covers byte 0 only; the first significant byte after it is the `+`
    let tree = SyntaxTree::new(vec![expr_stmt(0, 1, int(0, 1))], vec![], 5);
    let found = violations(&tree, b"1  +;", InvariantKind::Coverage);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].span, Span::new(3, 4));
}

#[test]
fn a_dropped_byte_between_list_items_is_reported_at_its_offset() {
    // "{1;  @2;}": the `@` at offset 5 sits between the two statements of the block
    let inner = vec![expr_stmt(1, 3, int(1, 2)), expr_stmt(6, 8, int(6, 7))];
    let block = Node::new(Span::new(0, 9), StmtKind::Block(inner));
    let tree = SyntaxTree::new(vec![block], vec![], 9);
    let found = violations(&tree, b"{1;  @2;}", InvariantKind::Coverage);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].span, Span::new(5, 6));
}
