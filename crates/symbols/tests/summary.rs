//! File summaries: the golden dump of a rich file, the incompleteness flag and the fingerprint.

mod common;
use common::*;

use bleedthrough_symbols::{dump_summary, summarize};
use bleedthrough_syntax::{ExprKind, Node, Span, StmtKind, SyntaxTree};

const SOURCE: &str = include_str!("golden/summary.php");
const GOLDEN: &str = include_str!("golden/summary.txt");

/// spec: symbol-table/Declarations are collected per file as summaries
#[test]
fn the_dump_of_a_rich_file_is_the_golden_text() {
    let summary = summarize("a.php", &parse(SOURCE), SOURCE);
    let dump = dump_summary(&summary);
    assert_eq!(dump.trim_end(), GOLDEN.trim_end());
}

/// spec: symbol-table/Declarations are collected per file as summaries
#[test]
fn a_tree_with_holes_is_incomplete() {
    let whole = summarize("a.php", &parse("function f() {}"), "function f() {}");
    assert!(!whole.incomplete);
    assert!(!dump_summary(&whole).contains("incomplete"));
    let hole = Node::new(Span::new(0, 1), ExprKind::Error { diagnostic: None });
    let tree = SyntaxTree::new(
        vec![Node::new(Span::new(0, 1), StmtKind::Expr(hole))],
        Vec::new(),
        1,
    );
    let broken = summarize("a.php", &tree, "x");
    assert!(broken.incomplete);
    assert!(dump_summary(&broken).starts_with("file a.php\nincomplete\n"));
}

/// spec: symbol-table/Tables expose a stable fingerprint of summaries
#[test]
fn the_fingerprint_covers_declarations_and_nothing_else() {
    let fp = |src: &str, path: &str| summarize(path, &parse(src), src).fingerprint();
    let base = "class A { public int $x = 1; function m(int $a): string { return 's'; } }";
    let reference = fp(base, "a.php");
    // whitespace, comments, bodies and the path do not matter
    assert_eq!(
        reference,
        fp(
            "\n\n// note\nclass A  {  public int $x=1;\n function m(int $a) : string { return 'other body'; } }",
            "b.php"
        )
    );
    // declarations do
    for changed in [
        "class A { public int $x = 1; function m(int $a): int { return 's'; } }",
        "class A { public int $x = 1; function m(int $b): string { return 's'; } }",
        "class A { public string $x = 1; function m(int $a): string { return 's'; } }",
        "class B { public int $x = 1; function m(int $a): string { return 's'; } }",
        "class A { public int $x = 1; }",
    ] {
        assert_ne!(reference, fp(changed, "a.php"), "{changed}");
    }
    assert_eq!(reference.to_string().len(), 64);
    assert_eq!(reference.as_bytes().len(), 32);
    assert!(reference.as_bytes().iter().any(|b| *b != 0));
    assert_ne!(reference.as_bytes(), fp("class Z {}", "a.php").as_bytes());
    let hex: String = reference
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(reference.to_string(), hex);
}

/// spec: symbol-table/Declarations are collected per file as summaries
#[test]
fn diagnostic_codes_have_stable_spellings() {
    use bleedthrough_symbols::SymbolDiagnosticCode as C;
    for (code, text) in [
        (C::DuplicateImport, "duplicate_import"),
        (C::DocblockSyntax, "docblock_syntax"),
        (C::DocblockUnsupported, "docblock_unsupported"),
        (C::DocblockContradiction, "docblock_contradiction"),
        (C::DocblockUnverified, "docblock_unverified"),
        (C::TypeConversion, "type_conversion"),
        (C::BuiltinData, "builtin_data"),
    ] {
        assert_eq!(code.as_str(), text);
    }
}

/// spec: type-model/Native types are converted exactly
#[test]
fn true_and_false_are_native_types_of_their_own() {
    let src = "function f(true $a, false $b, ?false $c): true|false {}";
    let dump = dump_summary(&summarize("a.php", &parse(src), src));
    assert!(
        dump.contains("function f($a: true (native); $b: false (native); $c: false|null (native)) -> bool (native)"),
        "{dump}"
    );
}
