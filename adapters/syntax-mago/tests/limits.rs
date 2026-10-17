//! Limits: each bound is reached, widens conservatively, is counted by kind and marks findings;
//! adversarial input terminates.

use bleedthrough_engine_suite::engine::{CallGraphConfig, Gate, Limits, RuleIndex};
use bleedthrough_engine_suite::findings::Finding;
use bleedthrough_engine_suite::ir::LowerConfig;
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

struct Outcome {
    kinds: Vec<&'static str>,
    recovered: u64,
    findings: Vec<Finding>,
}

impl Outcome {
    fn widened(&self) -> bool {
        !self.findings.is_empty()
            && self.findings.iter().all(|f| {
                f.confidence()
                    .reasons()
                    .iter()
                    .any(|r| r.code().as_str() == "analysis_widened")
            })
    }
}

fn run_with(php: &str, lower: &LowerConfig, calls: CallGraphConfig, limits: Limits) -> Outcome {
    let rules = test_rules();
    let index = RuleIndex::build(&rules, &Gate::default());
    let mut project = Project::load_with(&MagoBackend::new(), &[("a.php", php)], lower);
    project.call_config = calls;
    let (coverage, _) = project.coverage_with(&index, &rules, "8.3", limits);
    let mut kinds: Vec<&'static str> = coverage
        .limits_hit
        .iter()
        .map(|h| h.kind().as_str())
        .collect();
    kinds.sort_unstable();
    kinds.dedup();
    Outcome {
        kinds,
        recovered: coverage.files_recovered,
        findings: project.findings(&rules, &index, limits),
    }
}

fn run(php: &str, limits: Limits) -> Outcome {
    run_with(
        php,
        &LowerConfig::default(),
        CallGraphConfig::default(),
        limits,
    )
}

const SINK_AFTER: &str = "mysqli_query($l, $x);\n";

// ---- one test per limit kind -------------------------------------------------------------

/// spec: analysis-limits/Limit behaviour is tested
#[test]
fn expression_depth() {
    let php = "<?php\n$x = strtolower(strtolower(strtolower(strtolower(strtolower(strtolower($_GET['a']))))));\nmysqli_query($l, $x);\n";
    let lower = LowerConfig {
        max_depth: 3,
        ..LowerConfig::default()
    };
    let out = run_with(php, &lower, CallGraphConfig::default(), Limits::default());
    assert_eq!(out.kinds, ["expression_depth"]);
    assert!(out.widened(), "{:?}", out.findings);
}

/// spec: analysis-limits/Limit behaviour is tested
#[test]
fn block_limit() {
    let php = format!(
        "<?php\n$x = $_GET['a'];\nif ($a) {{}} if ($b) {{}} if ($c) {{}} if ($d) {{}}\n{SINK_AFTER}"
    );
    let lower = LowerConfig {
        max_blocks: 3,
        ..LowerConfig::default()
    };
    let out = run_with(&php, &lower, CallGraphConfig::default(), Limits::default());
    assert!(out.kinds.contains(&"block_limit"), "{:?}", out.kinds);
}

/// spec: analysis-limits/Limit behaviour is tested
#[test]
fn instruction_limit() {
    let php = format!("<?php\n$x = $_GET['a'];\n$y = 1; $y = 2; $y = 3; $y = 4;\n{SINK_AFTER}");
    let lower = LowerConfig {
        max_instructions: 4,
        ..LowerConfig::default()
    };
    let out = run_with(&php, &lower, CallGraphConfig::default(), Limits::default());
    assert!(out.kinds.contains(&"instruction_limit"), "{:?}", out.kinds);
}

/// spec: analysis-limits/Limit behaviour is tested
#[test]
fn step_budget() {
    let php = format!("<?php\n$x = $_GET['a'];\n$y = 1;\n$y = 2;\n$y = 3;\n{SINK_AFTER}");
    let limits = Limits {
        step_budget: 3,
        ..Limits::default()
    };
    assert!(run(&php, limits).kinds.contains(&"step_budget"));
}

/// spec: analysis-limits/Limit behaviour is tested
#[test]
fn block_visits() {
    let php = "<?php\n$x = '';\nwhile ($c) { $x = $x . $_GET['a']; }\nmysqli_query($l, $x);\n";
    let limits = Limits {
        block_visits: 1,
        ..Limits::default()
    };
    assert!(run(php, limits).kinds.contains(&"block_visits"));
}

/// spec: analysis-limits/Origin set exceeds the cap
#[test]
fn origin_cap() {
    let php = "<?php\n$x = $_GET['a'] . $_POST['b'];\nmysqli_query($l, $x);\n";
    let limits = Limits {
        max_origins: 1,
        ..Limits::default()
    };
    let out = run(php, limits);
    assert_eq!(out.kinds, ["origin_cap"]);
    assert_eq!(
        out.findings.len(),
        1,
        "one origin is kept, the loss is reported"
    );
    assert!(out.widened());
}

/// spec: analysis-limits/Fixpoint does not converge
#[test]
fn iterations() {
    let php = "<?php\nfunction a($x) { return b($x); }\nfunction b($x) { return c($x); }\nfunction c($x) { if ($x) { return a($x); } return $x; }\nmysqli_query($l, c($_GET['a']));\n";
    let limits = Limits {
        iterations: 1,
        ..Limits::default()
    };
    let out = run(php, limits);
    assert!(out.kinds.contains(&"iterations"), "{:?}", out.kinds);
    assert!(out.widened(), "{:?}", out.findings);
}

/// spec: analysis-limits/Limit behaviour is tested
#[test]
fn summary_depth() {
    let php = "<?php\n$f = function ($x) { mysqli_query($l, $x); };\n$f($_GET['a']);\n";
    let limits = Limits {
        closure_depth: 0,
        ..Limits::default()
    };
    assert!(run(php, limits).kinds.contains(&"summary_depth"));
}

/// spec: analysis-limits/Limit behaviour is tested
#[test]
fn candidates() {
    let php = "<?php\ninterface I { function m($x); }\nclass A implements I { function m($x) { return $x; } }\nclass B implements I { function m($x) { return $x; } }\nclass C implements I { function m($x) { return $x; } }\nfunction f(I $i) { mysqli_query($l, $i->m($_GET['a'])); }\n";
    let out = run_with(
        php,
        &LowerConfig::default(),
        CallGraphConfig { max_candidates: 2 },
        Limits::default(),
    );
    assert!(out.kinds.contains(&"candidates"), "{:?}", out.kinds);
}

/// spec: analysis-limits/Limit behaviour is tested
#[test]
fn component_size() {
    let php = "<?php\nfunction a($x) { return b($x); }\nfunction b($x) { if ($x) { return a($x); } return $x; }\nmysqli_query($l, a($_GET['a']));\n";
    let limits = Limits {
        component_size: 1,
        ..Limits::default()
    };
    let out = run(php, limits);
    assert!(out.kinds.contains(&"component_size"), "{:?}", out.kinds);
    assert!(out.widened());
}

/// spec: analysis-limits/Widening never hides a reachable sink silently
#[test]
fn defaults_report_no_limit_on_ordinary_code() {
    let php = "<?php\nfunction h($x) { return $x . 'a'; }\nmysqli_query($l, h($_GET['a']));\n";
    let out = run(php, Limits::default());
    assert!(out.kinds.is_empty(), "{:?}", out.kinds);
    assert!(!out.widened());
    assert_eq!(out.findings.len(), 1);
}

// ---- adversarial input -------------------------------------------------------------------

/// spec: analysis-limits/Deeply nested expression
#[test]
fn nesting_past_the_depth_bound_is_flattened_and_reported() {
    let depth = 300;
    let php = format!(
        "<?php\n$x = {}$_GET['a']{};\nmysqli_query($l, $x);\n",
        "strtolower(".repeat(depth),
        ")".repeat(depth)
    );
    let out = run(&php, Limits::default());
    assert!(out.kinds.contains(&"expression_depth"), "{:?}", out.kinds);
    assert_eq!(
        out.findings.len(),
        1,
        "the source inside the cut is not lost"
    );
    assert!(out.widened());
}

/// spec: analysis-limits/Deeply nested expression
#[test]
fn a_thousand_levels_of_nesting_terminate_and_the_file_is_reported_incomplete() {
    let depth = 1000;
    let php = format!(
        "<?php\n$x = {}$_GET['a']{};\nmysqli_query($l, $x);\n",
        "strtolower(".repeat(depth),
        ")".repeat(depth)
    );
    // The parser gives up on nesting this deep and returns an error node; the file is counted as
    // recovered, so the report is not clean.
    let out = run(&php, Limits::default());
    assert!(out.recovered == 1 || out.kinds.contains(&"expression_depth"));
}

/// spec: analysis-limits/Adversarial input terminates within bounded resources
#[test]
fn a_very_long_concatenation_chain_terminates() {
    let php = format!(
        "<?php\n$x = $_GET['a']{};\nmysqli_query($l, $x);\n",
        " . 'y'".repeat(5000)
    );
    // Past the parser's nesting bound the chain is an error node: the file is reported recovered.
    let out = run(&php, Limits::default());
    assert!(out.recovered == 1 || out.findings.len() == 1);
}

/// spec: analysis-limits/Adversarial input terminates within bounded resources
#[test]
fn a_long_chain_of_assignments_terminates() {
    let mut php = String::from("<?php\n$v0 = $_GET['a'];\n");
    for n in 1..5000 {
        php.push_str(&format!("$v{n} = $v{};\n", n - 1));
    }
    php.push_str("mysqli_query($l, $v4999);\n");
    assert_eq!(run(&php, Limits::default()).findings.len(), 1);
}

/// spec: analysis-limits/Large mutually recursive set
#[test]
fn a_thousand_functions_in_a_cycle_terminate() {
    let n = 1000;
    let mut php = String::from("<?php\n");
    for i in 0..n {
        php.push_str(&format!(
            "function f{i}($x) {{ return f{}($x); }}\n",
            (i + 1) % n
        ));
    }
    php.push_str("mysqli_query($l, f0($_GET['a']));\n");
    let out = run(&php, Limits::default());
    assert!(out.kinds.contains(&"component_size") || out.kinds.is_empty());
}

/// spec: analysis-limits/Adversarial input terminates within bounded resources
#[test]
fn many_blocks_and_a_wide_array_terminate() {
    let mut php = String::from("<?php\n$x = $_GET['a'];\n");
    for _ in 0..3000 {
        php.push_str("if ($c) { $x = $x . 'a'; }\n");
    }
    php.push_str(&format!(
        "$m = [{}];\nmysqli_query($l, $x);\n",
        "1,".repeat(20000)
    ));
    let out = run(&php, Limits::default());
    assert_eq!(out.findings.len(), 1);
}
