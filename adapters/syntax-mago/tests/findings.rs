//! Findings from sink hits: traces, de-duplication, confidence, classification and fingerprints.

use bleedthrough_engine_suite::engine::{Gate, Limits, RuleIndex};
use bleedthrough_engine_suite::findings::{Finding, StepKind};
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

fn findings_with(files: &[(&str, &str)], limits: Limits) -> Vec<Finding> {
    let rules = test_rules();
    let index = RuleIndex::build(&rules, &Gate::default());
    Project::load(&MagoBackend::new(), files).findings(&rules, &index, limits)
}

fn findings(php: &str) -> Vec<Finding> {
    findings_with(&[("a.php", php)], Limits::default())
}

fn kinds(finding: &Finding) -> Vec<StepKind> {
    finding
        .trace()
        .map(|t| t.steps().iter().map(|s| s.kind()).collect())
        .unwrap_or_default()
}

// ---- 6.1 findings and traces -------------------------------------------------------------

/// spec: finding-production/Each flow becomes one finding with a trace
#[test]
fn two_sources_reaching_one_sink_give_two_findings() {
    let found = findings("<?php\nmysqli_query($l, $_GET['a'] . $_POST['b']);\n");
    assert_eq!(found.len(), 2);
    let mut labels: Vec<&str> = found
        .iter()
        .map(|f| f.trace().unwrap().steps()[0].label())
        .collect();
    labels.sort_unstable();
    assert_eq!(labels, ["source: $_GET", "source: $_POST"]);
}

/// spec: finding-production/Each flow becomes one finding with a trace
#[test]
fn one_flow_over_two_paths_is_one_finding() {
    let php = "<?php\n$v = $_GET['a'];\nif ($c) { $x = $v . 'a'; } else { $x = $v . 'b'; }\nmysqli_query($l, $x);\n";
    assert_eq!(findings(php).len(), 1);
}

/// spec: finding-production/Each flow becomes one finding with a trace
#[test]
fn a_trace_runs_from_the_source_through_the_call_to_the_sink() {
    let php = "<?php\nfunction h($x) {\n  mysqli_query($l, $x . '!');\n}\nh($_GET['a']);\n";
    let found = findings(php);
    assert_eq!(found.len(), 1);
    let finding = &found[0];
    assert_eq!(finding.location().start().line(), 3);
    let steps = kinds(finding);
    assert_eq!(steps.first(), Some(&StepKind::Source));
    assert_eq!(steps.last(), Some(&StepKind::Sink));
    assert!(steps.contains(&StepKind::Call));
    assert!(steps.contains(&StepKind::Propagation));
    let trace = finding.trace().unwrap();
    assert!(!trace.truncated());
    // The source is in the caller, the sink in the callee.
    assert_eq!(trace.steps()[0].location().start().line(), 5);
}

/// spec: finding-production/Each flow becomes one finding with a trace
#[test]
fn a_wrong_sanitizer_is_a_considered_step() {
    let php = "<?php\n$x = escapeshellarg($_GET['a']);\nmysqli_query($l, $x);\n";
    let found = findings(php);
    assert_eq!(found.len(), 1);
    assert!(kinds(&found[0]).contains(&StepKind::SanitizerConsidered));
}

/// spec: finding-production/Each flow becomes one finding with a trace
#[test]
fn a_long_trace_keeps_both_ends_and_is_marked() {
    let mut php = String::from("<?php\n$x = $_GET['a'];\n");
    for _ in 0..30 {
        php.push_str("$x = $x . 'y';\n");
    }
    php.push_str("mysqli_query($l, $x);\n");
    let found = findings(&php);
    assert_eq!(found.len(), 1);
    let trace = found[0].trace().unwrap();
    assert!(trace.truncated());
    assert_eq!(trace.steps().len(), Limits::default().trace_steps);
    assert_eq!(trace.steps().first().unwrap().kind(), StepKind::Source);
    assert_eq!(trace.steps().last().unwrap().kind(), StepKind::Sink);
}

// ---- 6.2 confidence, classification, fingerprints ----------------------------------------

fn reasons(finding: &Finding) -> Vec<&'static str> {
    finding
        .confidence()
        .reasons()
        .iter()
        .map(|r| r.code().as_str())
        .collect()
}

/// spec: finding-production/Confidence is derived from stated reasons
#[test]
fn a_direct_resolved_flow_is_high_confidence_with_no_reasons() {
    let found = findings("<?php\nmysqli_query($l, $_GET['a']);\n");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].confidence().tier().as_str(), "high");
    assert!(reasons(&found[0]).is_empty());
}

/// spec: finding-production/Confidence is derived from stated reasons
#[test]
fn an_inferred_source_alone_is_medium() {
    let php = "<?php\nclass A {\n  private $p;\n  function set() { $this->p = $_GET['a']; }\n  function run() { mysqli_query($l, $this->p); }\n}\n";
    let found = findings(php);
    assert_eq!(found.len(), 1);
    assert_eq!(reasons(&found[0]), ["source_inferred"]);
    assert_eq!(found[0].confidence().tier().as_str(), "medium");
}

/// spec: finding-production/Confidence is derived from stated reasons
#[test]
fn several_conditions_give_every_reason_and_the_lowest_tier() {
    let php = "<?php\ninterface I { function m($x); }\nclass A implements I { function m($x) { return $x; } }\nclass B implements I { function m($x) { return $x; } }\nfunction f(I $i) {\n  mysqli_query($l, unknown_fn($i->m($_GET['a'])));\n}\n";
    let found = findings(php);
    assert_eq!(found.len(), 1);
    let got = reasons(&found[0]);
    assert!(
        got.contains(&"dynamic_dispatch_several_candidates"),
        "{got:?}"
    );
    assert!(
        got.contains(&"unresolved_callee_assumed_propagate"),
        "{got:?}"
    );
    assert_eq!(found[0].confidence().tier().as_str(), "low");
}

/// spec: finding-production/Confidence is derived from stated reasons
#[test]
fn a_recovery_parsed_file_lowers_the_tier() {
    let found = findings("<?php\nmysqli_query($l, $_GET['a']);\nfunction broken( {\n");
    assert!(
        found
            .iter()
            .all(|f| reasons(f).contains(&"parse_recovered")
                && f.confidence().tier().as_str() == "low"),
        "{found:?}"
    );
}

/// spec: finding-production/Severity and classification come from the rule
#[test]
fn severity_and_taxonomy_do_not_vary_with_the_flow() {
    let php = "<?php\nmysqli_query($l, $_GET['a']);\nfunction h($x) { mysqli_query($l, unknown_fn($x . 'z')); }\nh($_POST['b']);\n";
    let found = findings(php);
    assert_eq!(found.len(), 2);
    assert_ne!(found[0].confidence(), found[1].confidence());
    assert_eq!(found[0].severity(), found[1].severity());
    assert_eq!(found[0].taxonomy(), found[1].taxonomy());
    assert_eq!(found[0].paranoid(), found[1].paranoid());
}

/// spec: finding-production/Fingerprints are stable and meaningful
#[test]
fn inserting_lines_above_moves_the_finding_but_not_its_fingerprint() {
    let php = "<?php\nmysqli_query($l, $_GET['a']);\n";
    let moved = "<?php\n// a comment\n$unrelated = 1;\n\nmysqli_query( $l,  $_GET['a'] );\n";
    let (a, b) = (findings(php), findings(moved));
    assert_eq!(a.len(), 1);
    assert_eq!(b.len(), 1);
    assert_ne!(a[0].location(), b[0].location());
    assert_eq!(a[0].fingerprint(), b[0].fingerprint());
}

/// spec: finding-production/Fingerprints are stable and meaningful
#[test]
fn changing_the_sink_or_the_source_changes_the_fingerprint() {
    let base = findings("<?php\nmysqli_query($l, $_GET['a']);\n");
    let sink = findings("<?php\nmysqli_query($m, $_GET['a']);\n");
    let source = findings("<?php\nmysqli_query($l, $_GET['b']);\n");
    assert_ne!(base[0].fingerprint(), sink[0].fingerprint());
    assert_ne!(base[0].fingerprint(), source[0].fingerprint());
}

/// spec: finding-production/Fingerprints are stable and meaningful
#[test]
fn the_same_sink_in_two_files_has_two_fingerprints() {
    let php = "<?php\nmysqli_query($l, $_GET['a']);\n";
    let found = findings_with(&[("a.php", php), ("b.php", php)], Limits::default());
    assert_eq!(found.len(), 2);
    assert_ne!(found[0].fingerprint(), found[1].fingerprint());
}

/// spec: finding-production/Fingerprints are stable and meaningful
#[test]
fn identical_sinks_in_one_unit_are_told_apart_by_occurrence() {
    let php = "<?php\nmysqli_query($l, $_GET['a']);\nmysqli_query($l, $_GET['a']);\n";
    let found = findings(php);
    assert_eq!(found.len(), 2);
    assert_ne!(found[0].fingerprint(), found[1].fingerprint());
}

// ---- pattern findings and coverage -------------------------------------------------------

fn pattern_findings(files: &[(&str, &str)]) -> Vec<Finding> {
    findings_with(files, Limits::default())
        .into_iter()
        .filter(|f| f.rule().as_str() == "BT9203")
        .collect()
}

/// spec: pattern-matching/Pattern findings
#[test]
fn a_pattern_finding_has_no_trace_and_high_confidence_when_resolved() {
    let found = pattern_findings(&[("a.php", "<?php\nhash('md5', $data);\n")]);
    assert_eq!(found.len(), 1);
    assert!(found[0].trace().is_none());
    assert_eq!(found[0].confidence().tier().as_str(), "high");
    assert_eq!(found[0].location().start().line(), 2);
}

/// spec: pattern-matching/Pattern findings
#[test]
fn a_pattern_matched_by_name_alone_carries_the_unresolved_reason() {
    let found = findings_with(
        &[(
            "a.php",
            "<?php
create_function('$a', 'return 1;');
",
        )],
        Limits::default(),
    );
    let found: Vec<_> = found
        .iter()
        .filter(|f| f.rule().as_str() == "BT9205")
        .collect();
    assert_eq!(found.len(), 1);
    assert_eq!(reasons(found[0]), ["unresolved_callee_assumed_propagate"]);
    assert_eq!(found[0].confidence().tier().as_str(), "low");
}

/// spec: pattern-matching/Pattern findings
#[test]
fn a_pattern_in_a_recovery_parsed_file_carries_the_reason() {
    let found = pattern_findings(&[("a.php", "<?php\nhash('md5', $data);\nfunction broken( {\n")]);
    assert_eq!(found.len(), 1);
    assert_eq!(reasons(&found[0]), ["parse_recovered"]);
}

/// spec: finding-production/Coverage is reported honestly
#[test]
fn unresolved_calls_and_recovered_files_are_counted() {
    let rules = test_rules();
    let index = RuleIndex::build(&rules, &Gate::default());
    let project = Project::load(
        &MagoBackend::new(),
        &[
            (
                "a.php",
                "<?php\nnot_defined_anywhere(1);\n$f = $_GET['f'];\n$f();\n",
            ),
            ("b.php", "<?php\nfunction broken( {\n"),
            ("c.php", "<?php\nstrlen('x');\n"),
        ],
    );
    let (coverage, calls) = project.coverage(&index, &rules, "8.3");
    assert_eq!(coverage.files_recovered, 1);
    assert_eq!(coverage.files_clean, 2);
    assert_eq!(coverage.unresolved_calls, calls.total_unresolved_calls());
    assert!(coverage.unresolved_calls >= 2, "{calls:?}");
    assert!(calls.unresolved_calls.len() >= 2, "by reason: {calls:?}");
}
