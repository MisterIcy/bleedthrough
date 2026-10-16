//! Symbol-layer scenarios and goldens, run through the Mago backend.

use std::path::Path;

use bleedthrough_symbols_suite::{run_golden_dir, summary_text, workspace_root};
use bleedthrough_syntax_mago::MagoBackend;

fn assert_goldens(relative: &str, render: &bleedthrough_symbols_suite::Render<'_>) {
    let dir = workspace_root().join(relative);
    let failures = run_golden_dir(Path::new(&dir), render);
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn summary_goldens() {
    let backend = MagoBackend::new();
    assert_goldens("fixtures/symbols/summary", &|name, source| {
        summary_text(&backend, name, source)
    });
}

fn summarise(source: &str) -> bleedthrough_symbols_suite::FileSummary {
    bleedthrough_symbols_suite::summarize_source(&MagoBackend::new(), "t.php", source)
}

#[test]
fn a_body_only_change_keeps_the_fingerprint() {
    let a = summarise("<?php\nnamespace A;\nclass C { function f(int $x): int { return 1; } }\n");
    let b = summarise(
        "<?php\n\n// comment\nnamespace A;\nclass C {\n    function f(int $x): int { return strlen('abc') + $x; }\n}\n",
    );
    assert_eq!(a.fingerprint(), b.fingerprint());
    assert_ne!(a.referenced_functions, b.referenced_functions);
}

#[test]
fn a_signature_change_alters_the_fingerprint() {
    let a = summarise("<?php\nclass C { function f(int $x): int { return 1; } }\n");
    let b = summarise("<?php\nclass C { function f(string $x): int { return 1; } }\n");
    assert_ne!(a.fingerprint(), b.fingerprint());
}

#[test]
fn a_docblock_change_that_alters_a_type_alters_the_fingerprint() {
    let a = summarise(
        "<?php\nclass C {\n/** @return list<int> */\nfunction f(): array { return []; } }\n",
    );
    let b = summarise(
        "<?php\nclass C {\n/** @return list<string> */\nfunction f(): array { return []; } }\n",
    );
    assert_ne!(a.fingerprint(), b.fingerprint());
}

#[test]
fn promoted_constructor_parameters_become_properties() {
    let s = summarise(
        "<?php\nclass P { function __construct(private readonly int $a, public ?string $b = null, $plain) {} }\n",
    );
    let names: Vec<_> = s.classes[0].properties.iter().map(|p| &*p.name).collect();
    assert_eq!(names, ["a", "b"]);
    assert!(s.classes[0].properties[0].is_readonly);
    assert_eq!(s.classes[0].properties[1].ty.ty.to_string(), "string|null");
}

#[test]
fn a_file_with_an_error_node_keeps_its_intact_declarations() {
    let s = summarise(
        &std::fs::read_to_string(workspace_root().join("fixtures/symbols/summary/broken.php"))
            .unwrap(),
    );
    assert!(s.incomplete);
    let names: Vec<_> = s.classes.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"Broken\\Intact"), "{names:?}");
}

#[test]
fn a_class_referenced_only_in_a_body_is_listed() {
    let s = summarise("<?php\nnamespace N;\nclass A { function f() { return new Hidden(); } }\n");
    assert!(
        s.referenced_classes
            .iter()
            .any(|c| c.as_str() == "N\\Hidden")
    );
}

#[test]
fn summaries_do_not_depend_on_anything_but_the_file() {
    let source =
        std::fs::read_to_string(workspace_root().join("fixtures/symbols/summary/basic.php"))
            .unwrap();
    let first = summarise(&source);
    assert_eq!(first, summarise(&source));
    assert!(first.referenced_classes.is_sorted());
    assert!(first.referenced_functions.is_sorted());
}
