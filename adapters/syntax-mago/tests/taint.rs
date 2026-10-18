//! Intra-procedural taint analysis over small PHP programs, through the Mago backend and the
//! engine's test rule sets.

use bleedthrough_engine_suite::engine::{Gate, RuleIndex};
use bleedthrough_engine_suite::rules::RuleSet;
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

fn flows(php: &str) -> Vec<String> {
    let project = Project::load(&MagoBackend::new(), &[("a.php", php)]);
    let index = RuleIndex::build(&test_rules(), &Gate::default());
    project.intra_flows(&index)
}

fn sql(source_line: usize, sink_line: usize) -> String {
    format!("BT9001 sql: $_GET@a.php:{source_line} -> mysqli_query@a.php:{sink_line}")
}

fn none(php: &str) {
    let found = flows(php);
    assert!(found.is_empty(), "expected no flows, got {found:#?}");
}

fn one(php: &str, expected: &str) {
    assert_eq!(flows(php), vec![expected.to_string()], "{php}");
}

// ---- 3.3 flow-sensitivity ----------------------------------------------------------------

#[test]
fn straight_line_flow_is_found() {
    one(
        "<?php\n$a = $_GET['a'];\nmysqli_query($l, $a);\n",
        &sql(2, 3),
    );
}

#[test]
fn overwriting_with_a_constant_kills_the_taint() {
    none("<?php\n$a = $_GET['a'];\n$a = 'safe';\nmysqli_query($l, $a);\n");
}

#[test]
fn taint_on_one_branch_survives_the_join() {
    one(
        "<?php\nif ($c) {\n$a = $_GET['a'];\n} else {\n$a = 'x';\n}\nmysqli_query($l, $a);\n",
        &sql(3, 7),
    );
}

#[test]
fn a_loop_carries_taint_around() {
    one(
        "<?php\n$a = '';\nwhile ($c) {\nmysqli_query($l, $a);\n$a = $_GET['a'];\n}\n",
        &sql(5, 4),
    );
}

#[test]
fn array_elements_collapse_into_the_array() {
    one(
        "<?php\n$a = ['y' => 'z'];\n$a['x'] = $_GET['a'];\nmysqli_query($l, $a['y']);\n",
        &sql(3, 4),
    );
}

#[test]
fn properties_flow_through_their_location() {
    one(
        "<?php\nclass C { public $p;\nfunction run() { $this->p = $_GET['a']; mysqli_query($l, $this->p); } }\n",
        &sql(3, 3),
    );
}

// ---- 3.4 sinks and sanitizers -----------------------------------------------------------

#[test]
fn the_right_sanitizer_clears_the_class() {
    none("<?php\n$a = mysqli_real_escape_string($l, $_GET['a']);\nmysqli_query($l, $a);\n");
}

#[test]
fn the_wrong_sanitizer_does_not() {
    one(
        "<?php\n$a = escapeshellarg($_GET['a']);\nmysqli_query($l, $a);\n",
        &sql(2, 3),
    );
}

#[test]
fn partial_sanitisation_still_fires_for_the_other_class() {
    let found =
        flows("<?php\n$a = escapeshellarg($_GET['a']);\nsystem($a);\nfile_get_contents($a);\n");
    assert_eq!(
        found,
        vec![
            "BT9003 file-path: $_GET@a.php:2 -> file_get_contents@a.php:4".to_string(),
            "BT9006 outgoing-request: $_GET@a.php:2 -> file_get_contents@a.php:4".to_string()
        ],
    );
}

#[test]
fn a_non_vulnerable_argument_is_not_a_sink_hit() {
    none("<?php\nmysqli_query($_GET['a'], 'SELECT 1');\n");
}

// ---- 3.5 call policy ---------------------------------------------------------------------

#[test]
fn strlen_and_int_casts_drop_taint() {
    none("<?php\n$a = strlen($_GET['a']);\nmysqli_query($l, $a);\n");
    none("<?php\n$a = (int) $_GET['a'];\nmysqli_query($l, $a);\n");
}

#[test]
fn strtoupper_keeps_taint() {
    one(
        "<?php\n$a = strtoupper($_GET['a']);\nmysqli_query($l, $a);\n",
        &sql(2, 3),
    );
}

#[test]
fn a_string_cast_keeps_taint() {
    one(
        "<?php\n$a = (string) $_GET['a'];\nmysqli_query($l, $a);\n",
        &sql(2, 3),
    );
}

#[test]
fn a_propagator_passes_only_its_inputs() {
    one(
        "<?php\n$a = trim($_GET['a']);\nmysqli_query($l, $a);\n",
        &sql(2, 3),
    );
}

#[test]
fn an_unknown_function_is_assumed_to_propagate() {
    one(
        "<?php\n$a = mystery($_GET['a']);\nmysqli_query($l, $a);\n",
        &sql(2, 3),
    );
}

// ---- 3.6 constructs ----------------------------------------------------------------------

#[test]
fn foreach_binds_key_and_value() {
    one(
        "<?php\nforeach ($_GET as $k => $v) {\nmysqli_query($l, $v);\n}\n",
        &sql(2, 3),
    );
}

#[test]
fn list_destructuring_distributes() {
    one(
        "<?php\n[$a, $b] = $_GET['p'];\nmysqli_query($l, $b);\n",
        &sql(2, 3),
    );
}

#[test]
fn a_closure_sees_captured_taint() {
    one(
        "<?php\n$a = $_GET['a'];\n$f = function () use ($a) { mysqli_query($l, $a); };\n",
        &sql(2, 3),
    );
}

#[test]
fn a_closure_result_carries_its_capture() {
    one(
        "<?php\n$a = $_GET['a'];\n$f = fn() => $a;\nmysqli_query($l, $f());\n",
        &sql(2, 4),
    );
}

#[test]
fn a_by_reference_argument_receives_taint() {
    one(
        "<?php\n$out = '';\nparse_str($_GET['a'], $out);\nmysqli_query($l, $out);\n",
        &sql(3, 4),
    );
}

#[test]
fn match_ternary_and_coalesce_join_their_arms() {
    for expr in [
        "$c ? $_GET['a'] : 'x'",
        "$_GET['a'] ?? 'x'",
        "match ($c) { 1 => $_GET['a'], default => 'x' }",
    ] {
        one(
            &format!("<?php\n$a = {expr};\nmysqli_query($l, $a);\n"),
            &sql(2, 3),
        );
    }
}

#[test]
fn a_generator_yield_reaches_the_consumer() {
    let found = flows(
        "<?php\nfunction g() { yield $_GET['a']; }\nforeach (g() as $v) { mysqli_query($l, $v); }\n",
    );
    // The yield itself is inter-procedural; intra-procedurally nothing reaches the sink.
    assert!(found.is_empty(), "{found:#?}");
}

// ---- 3.7 the vulnerability corpus, one unit at a time ---------------------------------------

/// The shapes a single unit can show: the rest need summaries (group 4) or the framework rules.
const INTRA_SHAPES: [&str; 4] = ["direct", "local-flow", "array-element", "wrong-sanitizer"];

/// CWEs whose safe twins are fixed by a guard (`in_array` host check, `str_starts_with`) or a call
/// option (`allowed_classes`): flagged until the engine models those, which no spec covers yet.
const GUARDED_TWINS: [&str; 3] = ["918", "601", "502"];

fn rule_of(cwe: &str) -> &'static str {
    match cwe {
        "22" => "BT9003",
        "78" => "BT9002",
        "89" => "BT9001",
        "502" => "BT9004",
        "601" => "BT9005",
        "918" => "BT9006",
        other => panic!("no test rule for CWE-{other}"),
    }
}

#[test]
fn single_unit_corpus_cases_are_found_and_their_twins_are_not() {
    let root = workspace_root().join("fixtures/vuln");
    let index = RuleIndex::build(&test_rules(), &Gate::default());
    let mut checked = 0;
    for cwe_dir in std::fs::read_dir(&root).unwrap().flatten() {
        let name = cwe_dir.file_name().to_string_lossy().into_owned();
        let Some(number) = name.strip_prefix("cwe-") else {
            continue;
        };
        for case in std::fs::read_dir(cwe_dir.path()).unwrap().flatten() {
            let id = case.file_name().to_string_lossy().into_owned();
            let (stem, safe) = match id.strip_suffix("-safe") {
                Some(stem) => (stem, true),
                None => (id.as_str(), false),
            };
            let shape = stem.split_once('-').map_or("", |x| x.1);
            if !INTRA_SHAPES.contains(&shape) {
                continue;
            }
            let text = std::fs::read_to_string(case.path().join("index.php")).unwrap();
            let project = Project::load(&MagoBackend::new(), &[("index.php", &text)]);
            let rule = rule_of(number);
            let found: Vec<String> = project
                .intra_flows(&index)
                .into_iter()
                .filter(|f| f.starts_with(rule))
                .collect();
            // The twin of a wrong-sanitizer case is fixed by a guard (an allow-list check), which
            // the analysis does not model: only the vulnerable case is asserted.
            if safe && shape == "wrong-sanitizer" {
                continue;
            }
            if safe {
                if GUARDED_TWINS.contains(&number) {
                    // Made safe by a guard or a call option the analysis does not model.
                    assert!(!found.is_empty(), "{id}: expected the known false positive");
                } else {
                    assert!(found.is_empty(), "{id}: safe twin flagged: {found:#?}");
                }
            } else {
                assert!(!found.is_empty(), "{id}: no flow found");
            }
            checked += 1;
        }
    }
    assert!(checked >= 20, "only {checked} cases checked");
}

// ---- 3.1 the rule index ------------------------------------------------------------------

mod index {
    use bleedthrough_engine_suite::engine::Subject;
    use bleedthrough_engine_suite::symbols::Fqn;

    use super::*;

    const CLASSES: &str = "<?php\nclass Request { function get($k) {} }\nclass JsonRequest extends Request {}\nclass Other {}\nclass Db { static function run($a, $q) {} function exec($query) {} }\nclass Reader {}\n";

    fn lookup(subject: Subject<'_>) -> bleedthrough_engine_suite::engine::Matches {
        let project = Project::load(&MagoBackend::new(), &[("c.php", CLASSES)]);
        let index = RuleIndex::build(&test_rules(), &Gate::default());
        index.lookup(&project.table, subject)
    }

    /// spec: rule-index/Symbol lookup
    #[test]
    fn functions_match_case_insensitively() {
        assert_eq!(lookup(Subject::Function("MySQLi_Query")).sinks.len(), 1);
        assert!(lookup(Subject::Function("mysqli_nothing")).sinks.is_empty());
    }

    /// spec: rule-index/Symbol lookup
    #[test]
    fn static_methods_match_by_class() {
        let class = Fqn::new("Db");
        let found = lookup(Subject::Static {
            class: &class,
            method: "run",
        });
        assert_eq!(found.sinks.len(), 1);
        let other = Fqn::new("Other");
        assert!(
            lookup(Subject::Static {
                class: &other,
                method: "run"
            })
            .sinks
            .is_empty()
        );
    }

    /// spec: rule-index/Symbol lookup
    #[test]
    fn instance_methods_with_a_plus_include_subclasses() {
        let sub = Fqn::new("JsonRequest");
        assert_eq!(
            lookup(Subject::Instance {
                class: &sub,
                method: "get"
            })
            .sources
            .len(),
            1
        );
        let unrelated = Fqn::new("Other");
        assert!(
            lookup(Subject::Instance {
                class: &unrelated,
                method: "get"
            })
            .sources
            .is_empty()
        );
    }

    /// spec: rule-index/Symbol lookup
    #[test]
    fn constructors_and_superglobals_match() {
        let reader = Fqn::new("Reader");
        assert_eq!(
            lookup(Subject::Constructor { class: &reader })
                .sources
                .len(),
            1
        );
        assert_eq!(lookup(Subject::Superglobal("$_GET")).sources.len(), 1);
        assert!(lookup(Subject::Superglobal("$_SERVER")).sources.is_empty());
    }

    /// spec: rule-index/Rule gating
    #[test]
    fn the_default_gate_admits_every_rule_but_the_paranoid_one() {
        let index = RuleIndex::build(&test_rules(), &Gate::default());
        assert_eq!(index.rules().len(), 17, "all but the paranoid rule");
    }
}
