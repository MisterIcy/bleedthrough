//! Behaviour of the Mago backend that does not need the shared corpus.

use bleedthrough_syntax::{
    DiagnosticCode, NodeRef, ParseOptions, ParseQuality, ParserBackend, RecoveryMode, StmtKind,
    check_invariants,
};
use bleedthrough_syntax_mago::MagoBackend;

fn parse(source: &str, options: &ParseOptions) -> bleedthrough_syntax::ParsedFile {
    MagoBackend::new().parse("t.php", source.as_bytes(), options)
}

#[test]
fn oversized_input_is_not_parsed() {
    let mut options = ParseOptions::default();
    options.max_bytes = 10;
    let parsed = parse("<?php echo 'a long enough line';", &options);
    assert_eq!(parsed.quality(), ParseQuality::Failed);
    assert_eq!(parsed.diagnostics()[0].code, DiagnosticCode::InputTooLarge);
    assert!(parsed.tree().statements().is_empty());
}

#[test]
fn input_at_the_limit_is_parsed() {
    let source = "<?php echo 1;";
    let mut options = ParseOptions::default();
    options.max_bytes = u32::try_from(source.len()).unwrap();
    assert_eq!(parse(source, &options).quality(), ParseQuality::Clean);
}

#[test]
fn clean_code_is_clean() {
    let parsed = parse(
        "<?php\nnamespace A;\nfunction f(int $a): int { return $a + 1; }\n",
        &ParseOptions::default(),
    );
    assert_eq!(parsed.quality(), ParseQuality::Clean);
    assert!(parsed.diagnostics().is_empty());
}

#[test]
fn dropped_statements_in_nested_lists_become_error_nodes() {
    let source = "<?php\nnamespace A;\nfunction f() { return 1; }\nclass C { function bad( { } }\nfinally { }\nfunction g() {}\n";
    let parsed = parse(source, &ParseOptions::default());
    assert_ne!(parsed.quality(), ParseQuality::Clean);
    assert!(parsed.error_count() > 0);
    assert!(check_invariants(parsed.tree(), source.as_bytes()).is_empty());
}

#[test]
fn a_broken_function_body_keeps_its_good_statements_and_covers_the_rest() {
    let source = "<?php\nfunction f() {\n  $a = 1;\n  @@@ ;\n  $b = 2;\n}\n";
    let parsed = parse(source, &ParseOptions::default());
    assert!(check_invariants(parsed.tree(), source.as_bytes()).is_empty());
    assert!(
        parsed
            .diagnostics()
            .iter()
            .all(|d| d.span.end as usize <= source.len())
    );
}

fn function_body(parsed: &bleedthrough_syntax::ParsedFile, name: &str) -> Vec<&'static str> {
    for stmt in parsed.tree().statements() {
        if let StmtKind::Function(f) = &stmt.kind
            && &*f.name == name
        {
            let body = f.body.as_ref().expect("a body");
            return body.iter().map(|s| NodeRef::Stmt(s).kind_name()).collect();
        }
    }
    panic!("no function {name}");
}

#[test]
fn a_dropped_statement_in_a_body_is_an_error_after_its_good_neighbour() {
    let source = "<?php\nfunction f() {\n  $a = 1;\n  if ( { }\n  $b = 2;\n}\n";
    let parsed = parse(source, &ParseOptions::default());
    let body = function_body(&parsed, "f");
    assert_eq!(body.first(), Some(&"Expr"), "{body:?}");
    assert!(body.contains(&"Error"), "{body:?}");
}

#[test]
fn junk_between_class_members_becomes_a_member_error() {
    for (keyword, source) in [
        (
            "class",
            "<?php\nclass C { function a() {} @@@ function b() {} }\n",
        ),
        (
            "interface",
            "<?php\ninterface I { function a(); @@@ function b(); }\n",
        ),
        (
            "trait",
            "<?php\ntrait T { function a() {} @@@ function b() {} }\n",
        ),
        ("enum", "<?php\nenum E { case A; @@@ case B; }\n"),
        (
            "anonymous class",
            "<?php\n$x = new class { function a() {} @@@ function b() {} };\n",
        ),
    ] {
        let parsed = parse(source, &ParseOptions::default());
        assert_ne!(parsed.quality(), ParseQuality::Clean, "{keyword}");
        assert!(
            check_invariants(parsed.tree(), source.as_bytes()).is_empty(),
            "{keyword}"
        );
        let mut found = false;
        bleedthrough_syntax::visit_tree(parsed.tree(), &mut |node| {
            if let NodeRef::Member(m) = node {
                found |= matches!(m.kind, bleedthrough_syntax::MemberKind::Error { .. });
            }
        });
        assert!(found, "{keyword}: no member error node");
    }
}

#[test]
fn editor_mode_keeps_an_unfinished_tail() {
    let source = "<?php\nfunction f() {\n  $a = 1;\n";
    let mut options = ParseOptions::default();
    options.recovery = RecoveryMode::Editor;
    let parsed = parse(source, &options);
    assert!(matches!(
        parsed.tree().statements().last().map(|s| &s.kind),
        Some(StmtKind::Function(_))
    ));
    assert!(!parsed.diagnostics().is_empty());
    assert!(check_invariants(parsed.tree(), source.as_bytes()).is_empty());
}

#[test]
fn deep_nesting_is_bounded() {
    let source = format!("<?php $a = {}1{};", "(".repeat(5000), ")".repeat(5000));
    let parsed = parse(&source, &ParseOptions::default());
    assert!(!parsed.diagnostics().is_empty());
}

#[test]
fn arbitrary_bytes_do_not_panic() {
    let bytes: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
    let parsed = MagoBackend::new().parse("t.php", &bytes, &ParseOptions::default());
    assert!(check_invariants(parsed.tree(), &bytes).is_empty());
}

#[test]
fn reported_identity_matches_the_locked_version() {
    let lock =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.lock")).unwrap();
    let marker = "name = \"mago-syntax\"\nversion = \"";
    let start = lock.find(marker).expect("mago-syntax is locked") + marker.len();
    let locked = &lock[start..start + lock[start..].find('"').unwrap()];
    let info = MagoBackend::new().info();
    assert_eq!(info.name, "mago-syntax");
    assert_eq!(info.version, locked);
}

#[test]
fn concurrent_parses_agree() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<MagoBackend>();

    let sources = [
        "<?php\nclass A { public function f(int $x): int { return $x * 2; } }\n",
        "<?php\nnamespace N;\nuse Foo\\Bar;\nfunction g(?Bar $b = null) { return $b?->x ?? [1, 2, 3]; }\n",
        "<?php\nclass C { function bad( { } }\nfinally { }\nfunction ok() {}\n",
        "<html><?= $a ?></html>\n<?php if ($a): ?>x<?php endif; ?>\n",
        "<?php\nenum S: string { case A = 'a'; public function label(): string { return ucfirst($this->value); } }\n",
    ];
    let backend = MagoBackend::new();
    let options = ParseOptions::default();
    let dump = |source: &str| {
        format!(
            "{:?}",
            backend.parse("t.php", source.as_bytes(), &options).tree()
        )
    };
    let reference: Vec<String> = sources.iter().map(|s| dump(s)).collect();
    std::thread::scope(|scope| {
        for worker in 0..8 {
            let (backend, options, reference, sources) = (&backend, &options, &reference, &sources);
            scope.spawn(move || {
                for round in 0..40 {
                    let index = (worker + round) % sources.len();
                    let parsed = backend.parse("t.php", sources[index].as_bytes(), options);
                    assert_eq!(format!("{:?}", parsed.tree()), reference[index]);
                }
            });
        }
    });
}

#[test]
fn constructs_without_an_owned_node_are_counted_as_unsupported() {
    let source = "<?php\nf(1, ?);\n";
    let parsed = parse(source, &ParseOptions::default());
    assert!(parsed.unsupported_count() > 0);
    assert_eq!(parsed.quality(), ParseQuality::Recovered);
    assert!(check_invariants(parsed.tree(), source.as_bytes()).is_empty());
}

#[test]
fn a_single_stray_token_between_statements_is_an_error_node() {
    let source = "<?php\n$a = 1;\n} \n$b = 2;\n";
    let parsed = parse(source, &ParseOptions::default());
    assert!(parsed.error_count() > 0);
    assert!(check_invariants(parsed.tree(), source.as_bytes()).is_empty());
}

#[test]
fn an_error_with_no_matching_diagnostic_still_gets_one() {
    // Whatever Mago reports, every error node must point at a diagnostic that exists.
    let source = "<?php\nclass C { function bad( { } }\nfinally { }\n";
    let parsed = parse(source, &ParseOptions::default());
    for stmt in parsed.tree().statements() {
        if let StmtKind::Error {
            diagnostic: Some(index),
        } = &stmt.kind
        {
            assert!(*index < parsed.diagnostics().len());
        }
    }
}

fn assert_clean(source: &str) {
    let parsed = parse(source, &ParseOptions::default());
    assert_eq!(
        parsed.quality(),
        ParseQuality::Clean,
        "{:?}",
        parsed.diagnostics()
    );
    assert!(check_invariants(parsed.tree(), source.as_bytes()).is_empty());
}

#[test]
fn colon_delimited_if_with_elseifs_and_else_is_clean() {
    assert_clean(
        "<?php\nif ($a): echo 1;\nelseif ($b): echo 2;\nelseif ($c): echo 3;\nelse: echo 4;\nendif;\n",
    );
}

#[test]
fn colon_delimited_loops_and_switch_are_clean() {
    assert_clean(
        "<?php\nwhile ($a): echo 1; endwhile;\nfor ($i = 0; $i < 2; $i++): echo 1; endfor;\nforeach ($a as $b): echo 1; endforeach;\nswitch ($a): case 1: echo 1; break; default: echo 2; endswitch;\ndeclare(ticks=1): echo 1; enddeclare;\n",
    );
}

#[test]
fn a_long_valid_concatenation_is_clean() {
    let terms = vec!["'a'"; 500].join(" . ");
    assert_clean(&format!("<?php\n$s = {terms};\n"));
}

#[test]
fn an_extremely_long_chain_is_bounded_and_reported() {
    let terms = vec!["'a'"; 5000].join(" . ");
    let source = format!("<?php\n$s = {terms};\n");
    let parsed = parse(&source, &ParseOptions::default());
    assert_ne!(parsed.quality(), ParseQuality::Clean);
    assert!(
        parsed
            .diagnostics()
            .iter()
            .any(|d| d.code == DiagnosticCode::RecursionLimit)
    );
    assert!(check_invariants(parsed.tree(), source.as_bytes()).is_empty());
    // The tree is safe to clone, compare and drop on a small stack.
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            let copy = parsed.tree().clone();
            assert!(copy == *parsed.tree());
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn a_long_valid_else_if_chain_is_clean() {
    let mut source = String::from("<?php\nif ($a) { echo 0; }");
    for i in 1..300 {
        source.push_str(&format!(" elseif ($a == {i}) {{ echo {i}; }}"));
    }
    assert_clean(&source);
}

fn assert_well_formed_when_broken(source: &str) {
    for mode in [RecoveryMode::Strict, RecoveryMode::Editor] {
        let mut options = ParseOptions::default();
        options.recovery = mode;
        let parsed = parse(source, &options);
        assert_ne!(parsed.quality(), ParseQuality::Clean, "{source:?}");
        let violations = check_invariants(parsed.tree(), source.as_bytes());
        assert!(
            violations.is_empty(),
            "{source:?} ({mode:?}): {violations:?}"
        );
    }
}

#[test]
fn a_missing_closer_reported_over_the_next_token_does_not_overlap_it() {
    // Mago gives the missing `)` the range of the following token; the call must still end
    // before the statement that starts there.
    assert_well_formed_when_broken("<?php\nfunction adn add(int $a, float $b) { }\n");
    assert_well_formed_when_broken("<?php\n$x = foo(1, [2, 3\n$y = 4;\n");
}

#[test]
fn a_missing_terminator_in_a_list_item_does_not_overlap_its_successor() {
    assert_well_formed_when_broken(
        "<?php\nclass C { use T { foo as final bar as protected baz; } }\n",
    );
    assert_well_formed_when_broken("<?php\nswitch ($a) { case 1: echo 1 case 2: echo 2; }\n");
}

#[test]
fn attributes_on_constants_are_kept_and_not_reported_as_dropped_source() {
    let source = "<?php\n#[\\Deprecated(since: '8.4')]\nconst A = UNKNOWN;\nclass C {\n    #[\\Deprecated]\n    public const int X = 1;\n    public const int Y = 2;\n}\n";
    let parsed = parse(source, &ParseOptions::default());
    assert_eq!(parsed.quality(), ParseQuality::Clean);
    assert!(check_invariants(parsed.tree(), source.as_bytes()).is_empty());
    let mut attributes = 0;
    bleedthrough_syntax::visit_tree(parsed.tree(), &mut |node| {
        if matches!(node, NodeRef::Attribute(_)) {
            attributes += 1;
        }
    });
    assert_eq!(attributes, 2);
}
