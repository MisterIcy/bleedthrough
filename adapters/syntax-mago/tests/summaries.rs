//! Inter-procedural taint: summaries, recursion, multi-target calls, locations and includes.

use bleedthrough_engine_suite::engine::{Gate, Limits, RuleIndex};
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

fn index() -> RuleIndex {
    RuleIndex::build(&test_rules(), &Gate::default())
}

fn project(files: &[(&str, &str)]) -> Project {
    Project::load(&MagoBackend::new(), files)
}

fn flows(files: &[(&str, &str)]) -> Vec<String> {
    project(files).project_flows(&index())
}

fn sql(source: &str, sink: &str) -> String {
    format!("BT9001 sql: $_GET@{source} -> mysqli_query@{sink}")
}

// ---- 4.1 summaries -----------------------------------------------------------------------

/// spec: taint-analysis/Calls are summarised and composed across functions
#[test]
fn a_helper_that_reaches_a_sink_is_a_flow() {
    let php = "<?php\nfunction h($x) {\n  mysqli_query($l, $x);\n}\nh($_GET['a']);\n";
    assert_eq!(flows(&[("a.php", php)]), [sql("a.php:5", "a.php:3")]);
}

/// spec: taint-analysis/Calls are summarised and composed across functions
#[test]
fn a_sanitising_helper_clears_the_flow() {
    let php = "<?php\nfunction clean($x) {\n  return mysqli_real_escape_string($l, $x);\n}\nmysqli_query($l, clean($_GET['a']));\n";
    assert_eq!(flows(&[("a.php", php)]), Vec::<String>::new());
}

/// spec: taint-analysis/Calls are summarised and composed across functions
#[test]
fn a_helper_passes_taint_through_its_return_value() {
    let php = "<?php\nfunction id($x) { return $x; }\nmysqli_query($l, id($_GET['a']));\n";
    assert_eq!(flows(&[("a.php", php)]), [sql("a.php:3", "a.php:3")]);
}

/// spec: taint-analysis/Calls are summarised and composed across functions
#[test]
fn a_wrapper_around_a_source_introduces_it() {
    let php = "<?php\nfunction src() { return $_GET['a']; }\nmysqli_query($l, src());\n";
    assert_eq!(flows(&[("a.php", php)]), [sql("a.php:2", "a.php:3")]);
}

/// spec: taint-analysis/Calls are summarised and composed across functions
#[test]
fn a_helper_that_ignores_its_argument_blocks_the_flow() {
    let php = "<?php\nfunction k($x) { return 'k'; }\nmysqli_query($l, k($_GET['a']));\n";
    assert_eq!(flows(&[("a.php", php)]), Vec::<String>::new());
}

/// spec: taint-analysis/Calls are summarised and composed across functions
#[test]
fn a_method_receives_its_receiver_and_arguments() {
    let php = "<?php\nclass R { function q($x) { mysqli_query($l, $x); } }\nfunction go(R $r) { $r->q($_GET['a']); }\n";
    assert_eq!(flows(&[("a.php", php)]), [sql("a.php:3", "a.php:2")]);
}

/// spec: taint-analysis/Calls are summarised and composed across functions
#[test]
fn a_by_reference_parameter_carries_taint_back() {
    let php =
        "<?php\nfunction fill(&$out) { $out = $_GET['a']; }\nfill($v);\nmysqli_query($l, $v);\n";
    assert_eq!(flows(&[("a.php", php)]), [sql("a.php:2", "a.php:4")]);
}

/// spec: taint-analysis/Calls are summarised and composed across functions
#[test]
fn sanitising_before_the_helper_still_counts() {
    let php = "<?php\nfunction h($x) { mysqli_query($l, $x); }\nh(mysqli_real_escape_string($l, $_GET['a']));\n";
    assert_eq!(flows(&[("a.php", php)]), Vec::<String>::new());
}

/// spec: taint-analysis/Calls are summarised and composed across functions
#[test]
fn sanitising_for_another_class_does_not_count() {
    let php = "<?php\nfunction h($x) { mysqli_query($l, $x); }\nh(escapeshellarg($_GET['a']));\n";
    assert_eq!(flows(&[("a.php", php)]), [sql("a.php:3", "a.php:2")]);
}

/// spec: finding-production/Traces
#[test]
fn a_hit_remembers_the_calls_it_went_through() {
    let php = "<?php\nfunction inner($x) { mysqli_query($l, $x); }\nfunction outer($y) { inner($y); }\nouter($_GET['a']);\n";
    let p = project(&[("a.php", php)]);
    let (_, result) = p.analyse(&index(), Limits::default());
    assert_eq!(result.hits.len(), 1);
    let lines: Vec<usize> = result.hits[0]
        .calls
        .iter()
        .map(|(_, span)| p.line("a.php", span.start))
        .collect();
    assert_eq!(lines, [4, 3]);
}

// ---- 4.2 recursion -----------------------------------------------------------------------

/// spec: taint-analysis/Calls are summarised and composed across functions
#[test]
fn a_recursive_function_returning_its_parameter_is_a_flow() {
    let php = "<?php\nfunction r($n, $x) {\n  if ($n > 0) { return r($n - 1, $x); }\n  return $x;\n}\nmysqli_query($l, r(3, $_GET['a']));\n";
    assert_eq!(flows(&[("a.php", php)]), [sql("a.php:6", "a.php:6")]);
}

const MUTUAL: &str = "<?php\nfunction f($n, $x) { return $n > 0 ? g($n - 1, $x) : $x; }\nfunction g($n, $x) { return h($n, $x); }\nfunction h($n, $x) { return f($n, $x); }\nmysqli_query($l, f(3, $_GET['a']));\n";

/// spec: analysis-limits/Recursive components stop iterating
#[test]
fn mutual_recursion_settles_within_the_limit() {
    let p = project(&[("a.php", MUTUAL)]);
    let (_, result) = p.analyse(&index(), Limits::default());
    assert_eq!(result.hits.len(), 1);
    assert!(result.flows.iter().all(|f| !f.limits.iterations));
}

/// spec: analysis-limits/Recursive components stop iterating
#[test]
fn a_component_that_does_not_settle_is_widened_not_dropped() {
    let p = project(&[("a.php", MUTUAL)]);
    let limits = Limits {
        iterations: 1,
        ..Limits::default()
    };
    let (_, result) = p.analyse(&index(), limits);
    assert!(result.flows.iter().any(|f| f.limits.iterations));
    assert_eq!(result.hits.len(), 1);
}

// ---- 4.3 multi-target and unresolved calls -----------------------------------------------

const SHAPES: &str = "<?php\ninterface Q { function run($x); }\nclass A implements Q { function run($x) { mysqli_query($l, $x); } }\nclass B implements Q { function run($x) { } }\nfunction go(Q $q) { $q->run($_GET['a']); }\n";

/// spec: finding-production/Confidence reasons
#[test]
fn several_candidates_mark_the_flow() {
    let p = project(&[("a.php", SHAPES)]);
    let (_, result) = p.analyse(&index(), Limits::default());
    assert_eq!(result.hits.len(), 1);
    assert!(result.hits[0].label.marks.several >= 2);
}

/// spec: finding-production/Confidence reasons
#[test]
fn an_unresolved_callee_marks_the_flow() {
    let php = "<?php\n$a = mystery($_GET['a']);\nmysqli_query($l, $a);\n";
    let p = project(&[("a.php", php)]);
    let (_, result) = p.analyse(&index(), Limits::default());
    assert_eq!(result.hits.len(), 1);
    assert!(result.hits[0].label.marks.assumed_unresolved);
}

/// spec: finding-production/Confidence reasons
#[test]
fn a_clean_flow_carries_no_marks() {
    let php = "<?php\nmysqli_query($l, $_GET['a']);\n";
    let p = project(&[("a.php", php)]);
    let (_, result) = p.analyse(&index(), Limits::default());
    let marks = result.hits[0].label.marks;
    assert!(!marks.assumed_unresolved && !marks.inferred_source && marks.several == 0);
}

// ---- 4.4 locations and includes ----------------------------------------------------------

/// spec: taint-analysis/Properties are flow-insensitive locations
#[test]
fn a_property_written_in_one_method_reaches_a_sink_in_another() {
    let php = "<?php\nclass C {\n  public $p;\n  function set() { $this->p = $_GET['a']; }\n  function run() { mysqli_query($l, $this->p); }\n}\n";
    let p = project(&[("a.php", php)]);
    let (_, result) = p.analyse(&index(), Limits::default());
    assert_eq!(result.hits.len(), 1);
    assert!(result.hits[0].label.marks.inferred_source);
    assert!(result.settled);
}

/// spec: taint-analysis/Properties are flow-insensitive locations
#[test]
fn the_location_rounds_are_deterministic() {
    let php = "<?php\nclass C {\n  public $p;\n  function set() { $this->p = $_GET['a']; }\n  function run() { mysqli_query($l, $this->p); }\n}\n";
    let first = project(&[("a.php", php)])
        .analyse(&index(), Limits::default())
        .1;
    let second = project(&[("a.php", php)])
        .analyse(&index(), Limits::default())
        .1;
    assert_eq!(first, second);
}

/// spec: taint-analysis/Includes share the including scope
#[test]
fn an_included_file_sees_the_including_variables() {
    let a = "<?php\n$x = $_GET['a'];\ninclude 'b.php';\n";
    let b = "<?php\nmysqli_query($l, $x);\n";
    assert_eq!(
        flows(&[("a.php", a), ("b.php", b)]),
        [sql("a.php:2", "b.php:2")]
    );
}

/// spec: taint-analysis/Includes share the including scope
#[test]
fn an_included_file_defines_variables_for_the_includer() {
    let a = "<?php\ninclude 'b.php';\nmysqli_query($l, $y);\n";
    let b = "<?php\n$y = $_GET['a'];\n";
    assert_eq!(
        flows(&[("a.php", a), ("b.php", b)]),
        [sql("b.php:2", "a.php:3")]
    );
}

/// spec: taint-analysis/Includes share the including scope
#[test]
fn a_dynamic_include_is_not_followed() {
    let a = "<?php\n$x = $_GET['a'];\ninclude $_GET['f'];\n";
    let b = "<?php\nmysqli_query($l, $x);\n";
    assert_eq!(flows(&[("a.php", a), ("b.php", b)]), Vec::<String>::new());
}
