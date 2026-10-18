//! Pattern rules: call patterns with argument conditions, use patterns, non-literal includes and
//! gating, over small PHP programs.

use bleedthrough_engine_suite::engine::{Gate, Limits, RuleIndex};
use bleedthrough_engine_suite::rules::{PhpVersion, RuleSet};
use bleedthrough_engine_suite::{Project, workspace_root};
use bleedthrough_syntax_mago::MagoBackend;

/// The engine's test rule sets, loaded from `crates/engine/tests/rules`.
fn test_rules() -> RuleSet {
    let root = workspace_root().join("crates/engine/tests/rules");
    match bleedthrough_fs::load_rules(&root) {
        Ok(loaded) => loaded.set,
        Err(errors) => {
            let lines: Vec<String> = errors.iter().map(ToString::to_string).collect();
            panic!("the test rules are invalid:\n{}", lines.join("\n"));
        }
    }
}

/// `RULE@file:line`, sorted, for every pattern match in `files`.
fn matches_with(gate: &Gate, files: &[(&str, &str)]) -> Vec<String> {
    let index = RuleIndex::build(&test_rules(), gate);
    let project = Project::load(&MagoBackend::new(), files);
    let (graph, result) = project.analyse(&index, Limits::default());
    let mut out: Vec<String> = result
        .patterns
        .iter()
        .map(|hit| {
            let entry = &index.patterns()[hit.pattern as usize];
            let file = &graph.unit(hit.unit).file;
            format!(
                "{}@{}:{}{}",
                index.rules()[entry.rule as usize],
                file,
                project.line(file, hit.span.start),
                if hit.by_name { " (by name)" } else { "" }
            )
        })
        .collect();
    out.sort();
    out
}

fn matches(php: &str) -> Vec<String> {
    matches_with(&Gate::default(), &[("a.php", php)])
}

fn none(php: &str) {
    let found = matches(php);
    assert!(found.is_empty(), "expected no matches, got {found:#?}");
}

// ---- 5.1 call patterns -------------------------------------------------------------------

/// spec: pattern-matching/Call patterns match resolved calls
#[test]
fn a_function_call_with_matching_arguments_is_reported_at_the_call() {
    assert_eq!(
        matches("<?php\nsetcookie($name, 'v');\n"),
        ["BT9201@a.php:2"]
    );
}

/// spec: pattern-matching/Call patterns match resolved calls
#[test]
fn a_call_whose_arguments_fail_the_condition_is_not_reported() {
    none("<?php\nsetcookie('sid', 'v');\n");
}

/// spec: pattern-matching/Call patterns match resolved calls
#[test]
fn a_function_of_the_same_short_name_in_another_namespace_is_not_the_symbol() {
    none(
        "<?php\nnamespace App;\nfunction hash($algo, $data) { return $data; }\nhash('md5', 'x');\n",
    );
}

/// spec: pattern-matching/Call patterns match resolved calls
#[test]
fn an_unresolved_call_matches_by_its_written_name_and_says_so() {
    assert_eq!(
        matches("<?php\ncreate_function('', 'return 1;');\n"),
        ["BT9205@a.php:2 (by name)"]
    );
}

/// spec: pattern-matching/Argument conditions are evaluated exactly
#[test]
fn folded_concatenation_equals_the_literal() {
    assert_eq!(
        matches("<?php\nhash('m' . 'd5', $data);\n"),
        ["BT9203@a.php:2"]
    );
    none("<?php\nhash('sha256', $data);\n");
}

/// spec: pattern-matching/Argument conditions are evaluated exactly
#[test]
fn constants_with_literal_values_fold() {
    assert_eq!(
        matches("<?php\nconst ALGO = 'md5';\nhash(ALGO, $data);\n"),
        ["BT9203@a.php:3"]
    );
}

/// spec: pattern-matching/Argument conditions are evaluated exactly
#[test]
fn an_absent_argument_satisfies_absent() {
    assert_eq!(
        matches("<?php\nhtmlspecialchars($x);\n"),
        ["BT9202@a.php:2"]
    );
    none("<?php\nhtmlspecialchars($x, ENT_QUOTES);\n");
}

/// spec: pattern-matching/Argument conditions are evaluated exactly
#[test]
fn matches_uses_the_linear_subset() {
    assert_eq!(
        matches("<?php\nini_set('display_errors', '1');\n"),
        ["BT9204@a.php:2"]
    );
    none("<?php\nini_set('memory_limit', '1G');\n");
}

/// spec: pattern-matching/Argument conditions are evaluated exactly
#[test]
fn an_argument_that_cannot_be_folded_is_non_literal() {
    assert_eq!(
        matches("<?php\nsetcookie($a . $b, 'v');\n"),
        ["BT9201@a.php:2"]
    );
    none("<?php\nhash($algo, $data);\n");
}

/// spec: pattern-matching/Argument conditions are evaluated exactly
#[test]
fn a_named_argument_is_found_by_its_parameter() {
    none("<?php\nhtmlspecialchars($x, flags: ENT_QUOTES);\n");
}

// ---- 5.2 use patterns and includes -------------------------------------------------------

/// spec: pattern-matching/Use patterns match any use of a symbol
#[test]
fn a_literal_callable_string_is_a_use_at_the_string() {
    assert_eq!(
        matches("<?php\narray_map('create_function', $a);\n"),
        ["BT9205@a.php:2"]
    );
}

/// spec: pattern-matching/Use patterns match any use of a symbol
#[test]
fn a_callable_array_names_its_method() {
    assert_eq!(
        matches(
            "<?php\nclass Util { static function danger() {} }\narray_map(['Util', 'danger'], $a);\n"
        ),
        ["BT9207@a.php:3"]
    );
}

/// spec: pattern-matching/Use patterns match any use of a symbol
#[test]
fn new_and_static_access_are_uses() {
    assert_eq!(
        matches(
            "<?php\nnamespace Legacy { class Mailer { const X = 1; } }\nnamespace {\n$m = new Legacy\\Mailer();\n$x = Legacy\\Mailer::X;\n}\n"
        ),
        ["BT9206@a.php:4", "BT9206@a.php:5"]
    );
    assert_eq!(
        matches("<?php\nclass Util { static function danger() {} }\nUtil::danger();\n"),
        ["BT9207@a.php:3"]
    );
}

/// spec: pattern-matching/Use patterns match any use of a symbol
#[test]
fn a_first_class_callable_is_a_use() {
    assert_eq!(
        matches("<?php\n$f = create_function(...);\n"),
        ["BT9205@a.php:2 (by name)"]
    );
}

/// spec: pattern-matching/Use patterns match any use of a symbol
#[test]
fn a_type_hint_is_not_a_use() {
    none(
        "<?php\nnamespace Legacy { class Mailer {} }\nnamespace {\nfunction f(Legacy\\Mailer $m): Legacy\\Mailer { return $m; }\n}\n",
    );
}

/// spec: pattern-matching/Include-non-literal patterns
#[test]
fn a_variable_include_path_is_reported() {
    assert_eq!(matches("<?php\nrequire $path;\n"), ["BT9208@a.php:2"]);
}

/// spec: pattern-matching/Include-non-literal patterns
#[test]
fn a_foldable_include_path_is_not() {
    none("<?php\nrequire __DIR__ . '/a.php';\ninclude 'b.php';\n");
}

// ---- 5.3 gating --------------------------------------------------------------------------

const GATED: &str = "<?php\ngate_min();\ngate_max();\ngate_paranoid();\n";

/// spec: pattern-matching/Pattern findings use the common output model
#[test]
fn the_version_gate_drops_rules_outside_their_range() {
    let at = |v: &str| Gate {
        php: Some(PhpVersion::parse(v).unwrap()),
        paranoid: true,
    };
    let rules = |gate: &Gate| -> Vec<String> {
        matches_with(gate, &[("a.php", GATED)])
            .into_iter()
            .map(|m| m[..6].to_string())
            .collect()
    };
    assert_eq!(rules(&at("7.4")), ["BT9210", "BT9211"]);
    assert_eq!(rules(&at("8.2")), ["BT9209", "BT9211"]);
    assert_eq!(
        rules(&Gate::default()).len(),
        2,
        "no version: ranges are ignored"
    );
}

/// spec: pattern-matching/Pattern findings use the common output model
#[test]
fn paranoid_rules_run_only_in_paranoid_mode() {
    let off = Gate::default();
    let found = matches_with(&off, &[("a.php", GATED)]);
    assert!(found.iter().all(|m| !m.starts_with("BT9211")), "{found:?}");
    let on = Gate {
        php: None,
        paranoid: true,
    };
    let found = matches_with(&on, &[("a.php", GATED)]);
    assert!(found.iter().any(|m| m.starts_with("BT9211")), "{found:?}");
}

/// spec: taint-analysis/Rules are gated
#[test]
fn gating_applies_to_taint_rules_too() {
    let rules = test_rules();
    let all = RuleIndex::build(&rules, &Gate::default());
    let paranoid = RuleIndex::build(
        &rules,
        &Gate {
            php: None,
            paranoid: true,
        },
    );
    assert_eq!(paranoid.rules().len(), all.rules().len() + 1);
}
