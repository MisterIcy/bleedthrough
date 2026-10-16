//! The scorer against hand-written reports whose correct metrics are known.

mod common;

use bleedthrough_eval::corpus::Corpus;
use bleedthrough_eval::expectations::Expectations;
use bleedthrough_eval::matching::score;
use bleedthrough_eval::report::{self, Finding};
use bleedthrough_eval::result::{Verdict, table, to_json};
use bleedthrough_eval::run::{Obtained, ci_exit_code, classify, evaluate, evaluate_report};
use common::*;

/// One vulnerable case `v` (CWE-89, lines 3-3 of a/vuln.php, medium), its safe twin `s`, and a
/// second vulnerable CWE-78 case `w` with its twin `x`.
fn corpus_with_two_classes(name: &str) -> Corpus {
    let root = scratch(name);
    pair_files(&root);
    write(&root, "b/vuln.php", "<?php\n$a = 1;\n$b = 2;\n");
    write(&root, "b/safe.php", "<?php\n$a = 1;\n$b = 2;\n");
    let second = pair(EXPECT)
        .replace("\"v\"", "\"w\"")
        .replace("\"s\"", "\"x\"")
        .replace("CWE-89", "CWE-78")
        .replace("a/", "b/");
    corpus(&root, &format!("{}{}", pair(EXPECT), second))
}

fn finding(path: &str, line: u32, cwe: &str, rule: &str, tier: &str) -> String {
    format!(
        r#"{{"rule":"{rule}","message":"m","location":{{"path":"{path}","start":{{"line":{line},"column":1}},"end":{{"line":{line},"column":9}},"byte_start":0,"byte_end":8}},"taxonomy":{{"cwe":["{cwe}"],"owasp":[{{"edition":2021,"id":"A03"}}]}},"severity":{{"level":"high","score":null,"vector":null}},"confidence":{{"tier":"{tier}","reasons":[]}},"fingerprint":"bt1:{:032x}","paranoid":false,"trace":null,"fix":null}}"#,
        u64::from(line)
    )
}

fn document(findings: &[String]) -> String {
    format!(
        r#"{{"schema_version":"1.0","tool":{{"name":"bleedthrough","version":"0"}},"options":{{"confidence_threshold":"low","paranoid":false,"php_version":"8.4","ruleset_id":"x"}},"findings":[{}],"coverage":{{"files_clean":1,"files_recovered":0,"files_failed":0,"skipped":[],"limits_hit":[],"unresolved_includes":0,"unresolved_calls":0,"packs":[],"php_version":"8.4","cache":null}},"suppressed":{{"below_threshold_low":0,"below_threshold_medium":0,"baselined":0}}}}"#,
        findings.join(",")
    )
}

fn findings(list: &[String]) -> Vec<Finding> {
    report::read(&document(list)).expect("valid report")
}

#[test]
fn correct_finding_is_a_true_positive() {
    let corpus = corpus_with_two_classes("tp");
    let scored = score(
        &corpus,
        &findings(&[finding("a/vuln.php", 3, "CWE-89", "r1", "high")]),
    );
    let c = scored.per_cwe["CWE-89"];
    assert_eq!((c.tp, c.fp, c.fn_), (1, 0, 0));
    assert_eq!(c.precision(), Some(1.0));
    assert_eq!(scored.per_rule["r1"].tp, 1);
}

#[test]
fn missing_vulnerability_is_a_false_negative_naming_the_case() {
    let corpus = corpus_with_two_classes("fn");
    let scored = score(&corpus, &findings(&[]));
    let c = scored.per_cwe["CWE-89"];
    assert_eq!((c.tp, c.fn_), (0, 1));
    assert_eq!(c.recall(), Some(0.0));
    assert!(scored.cases.iter().any(|o| o.id == "v" && !o.passed));
}

#[test]
fn finding_in_a_safe_twin_is_a_false_positive_naming_the_case() {
    let corpus = corpus_with_two_classes("fp");
    let scored = score(
        &corpus,
        &findings(&[finding("a/safe.php", 2, "CWE-89", "r1", "high")]),
    );
    assert_eq!(scored.per_cwe["CWE-89"].fp, 1);
    assert!(
        scored
            .cases
            .iter()
            .any(|o| o.id == "s" && !o.passed && o.counts.fp == 1)
    );
    assert_eq!(scored.per_rule["r1"].fp, 1);
}

#[test]
fn other_cwe_in_a_safe_twin_is_unlabelled() {
    let corpus = corpus_with_two_classes("other-cwe");
    let scored = score(
        &corpus,
        &findings(&[finding("a/safe.php", 2, "CWE-78", "r1", "high")]),
    );
    assert_eq!(scored.per_cwe["CWE-89"].fp, 0);
    assert_eq!(scored.unlabelled.len(), 1);
}

#[test]
fn unlabelled_findings_affect_neither_precision_nor_recall() {
    let corpus = corpus_with_two_classes("unlabelled");
    let scored = score(
        &corpus,
        &findings(&[
            finding("a/vuln.php", 3, "CWE-89", "r1", "high"),
            finding("a/vuln.php", 2, "CWE-89", "r1", "high"),
            finding("nowhere.php", 1, "CWE-89", "r1", "high"),
        ]),
    );
    let c = scored.per_cwe["CWE-89"];
    assert_eq!((c.tp, c.fp, c.fn_), (1, 0, 0));
    assert_eq!(scored.unlabelled.len(), 2);
}

#[test]
fn a_finding_satisfies_at_most_one_expectation_and_the_second_is_unlabelled() {
    let corpus = corpus_with_two_classes("once");
    let scored = score(
        &corpus,
        &findings(&[
            finding("a/vuln.php", 3, "CWE-89", "r1", "high"),
            finding("a/vuln.php", 3, "CWE-89", "r2", "high"),
        ]),
    );
    assert_eq!(scored.per_cwe["CWE-89"].tp, 1);
    assert_eq!(scored.unlabelled.len(), 1);
}

#[test]
fn confidence_below_the_minimum_does_not_match() {
    let corpus = corpus_with_two_classes("tier");
    let low = score(
        &corpus,
        &findings(&[finding("a/vuln.php", 3, "CWE-89", "r", "low")]),
    );
    assert_eq!(low.per_cwe["CWE-89"].tp, 0);
    let medium = score(
        &corpus,
        &findings(&[finding("a/vuln.php", 3, "CWE-89", "r", "medium")]),
    );
    assert_eq!(medium.per_cwe["CWE-89"].tp, 1);
}

#[test]
fn range_must_overlap_and_cwe_must_match() {
    let corpus = corpus_with_two_classes("overlap");
    for (path, line, cwe) in [
        ("a/vuln.php", 2, "CWE-89"),
        ("a/vuln.php", 3, "CWE-78"),
        ("b/vuln.php", 3, "CWE-89"),
    ] {
        let scored = score(&corpus, &findings(&[finding(path, line, cwe, "r", "high")]));
        assert_eq!(scored.per_cwe["CWE-89"].tp, 0, "{path}:{line} {cwe}");
    }
}

#[test]
fn any_cwe_of_the_finding_counts() {
    let corpus = corpus_with_two_classes("multi-cwe");
    let text = document(&[finding("a/vuln.php", 3, "CWE-89", "r", "high")
        .replace(r#""cwe":["CWE-89"]"#, r#""cwe":["CWE-20","CWE-89"]"#)]);
    let scored = score(&corpus, &report::read(&text).expect("valid"));
    assert_eq!(scored.per_cwe["CWE-89"].tp, 1);
}

#[test]
fn order_of_findings_does_not_matter() {
    let corpus = corpus_with_two_classes("order");
    let list = [
        finding("a/vuln.php", 3, "CWE-89", "r1", "high"),
        finding("a/safe.php", 2, "CWE-89", "r2", "low"),
        finding("b/vuln.php", 3, "CWE-78", "r3", "medium"),
        finding("a/vuln.php", 1, "CWE-89", "r4", "high"),
    ];
    let forward = score(&corpus, &findings(&list));
    let mut reversed = list.to_vec();
    reversed.reverse();
    assert_eq!(forward, score(&corpus, &findings(&reversed)));
    let a = to_json(
        &evaluate_report(&corpus, &Expectations::default(), &document(&list)),
        &corpus,
    );
    let b = to_json(
        &evaluate_report(&corpus, &Expectations::default(), &document(&reversed)),
        &corpus,
    );
    assert_eq!(a, b);
}

#[test]
fn undefined_metrics_are_null_not_zero_or_one() {
    let corpus = corpus_with_two_classes("undefined");
    let evaluation = evaluate_report(&corpus, &Expectations::default(), &document(&[]));
    let json: serde_json::Value =
        serde_json::from_str(&to_json(&evaluation, &corpus)).expect("json");
    // Nothing reported: precision is undefined; recall is 0/1.
    assert!(json["per_cwe"]["CWE-89"]["precision"].is_null());
    assert_eq!(json["per_cwe"]["CWE-89"]["recall"], 0.0);
    assert!(table(&evaluation).contains("undefined"));
    // A class with no vulnerable cases has undefined recall.
    let root = scratch("undefined-recall");
    write(&root, "n.php", "<?php\n");
    let negatives = corpus_negatives(&root);
    let scored = score(&negatives, &[]);
    assert_eq!(scored.per_cwe["CWE-89"].recall(), None);
}

fn corpus_negatives(root: &std::path::Path) -> Corpus {
    corpus(
        root,
        "[[case]]\nid = \"n\"\ncwe = \"CWE-89\"\nlabel = \"safe\"\ndescription = \"d\"\nphp = \"8.0+\"\nshapes = [\"direct\"]\nstandalone = \"r\"\nprovenance = \"original\"\nlicence = \"MIT\"\nfiles = [\"n.php\"]\n",
    )
}

#[test]
fn malformed_reports_fail_with_the_validation_error_and_no_metrics() {
    let corpus = corpus_with_two_classes("invalid");
    for text in [
        "not json",
        "{}",
        &document(&[]).replace("\"findings\":[]", "\"findings\":\"x\""),
    ] {
        let e = evaluate_report(&corpus, &Expectations::default(), text);
        assert_eq!(e.verdict, Verdict::Fail);
        assert!(e.scored.is_none());
        assert!(!e.problems.is_empty());
    }
    let bad_tier = document(&[finding("a/vuln.php", 3, "CWE-89", "r", "extreme")]);
    let e = evaluate_report(&corpus, &Expectations::default(), &bad_tier);
    assert!(e.problems[0].contains("does not validate against report.schema.json"));
}

fn expectations(text: &str) -> Expectations {
    Expectations::parse(text).expect("expectations parse")
}

const BASE: &str = "[eval]\nscan-switch = true\n[cwe.\"CWE-89\"]\nmin_tp = 1\nmax_fp = 0\nknown_gaps = []\n[cwe.\"CWE-78\"]\nmin_tp = 1\nmax_fp = 0\nknown_gaps = []\n";

fn perfect() -> Vec<String> {
    vec![
        finding("a/vuln.php", 3, "CWE-89", "r1", "high"),
        finding("b/vuln.php", 3, "CWE-78", "r2", "high"),
    ]
}

#[test]
fn matching_expectations_pass() {
    let corpus = corpus_with_two_classes("pass");
    let e = evaluate_report(&corpus, &expectations(BASE), &document(&perfect()));
    assert_eq!(e.verdict, Verdict::Pass, "{:?}", e.problems);
}

#[test]
fn regression_fails_and_names_case_and_class() {
    let corpus = corpus_with_two_classes("regression");
    let e = evaluate_report(&corpus, &expectations(BASE), &document(&perfect()[1..]));
    assert_eq!(e.verdict, Verdict::Fail);
    assert!(
        e.problems
            .iter()
            .any(|p| p.contains("CWE-89") && p.contains("0 true positives, expected at least 1"))
    );
    assert!(
        e.problems
            .iter()
            .any(|p| p.contains("CWE-89: case v fails"))
    );
}

#[test]
fn false_positive_over_the_maximum_fails() {
    let corpus = corpus_with_two_classes("over-fp");
    let mut list = perfect();
    list.push(finding("a/safe.php", 2, "CWE-89", "r1", "high"));
    let e = evaluate_report(&corpus, &expectations(BASE), &document(&list));
    assert!(
        e.problems
            .iter()
            .any(|p| p.contains("1 false positives, expected at most 0"))
    );
}

#[test]
fn a_class_absent_from_the_expectations_fails() {
    let corpus = corpus_with_two_classes("absent");
    let only89 =
        "[eval]\nscan-switch = true\n[cwe.\"CWE-89\"]\nmin_tp = 1\nmax_fp = 0\nknown_gaps = []\n";
    let e = evaluate_report(&corpus, &expectations(only89), &document(&perfect()));
    assert!(
        e.problems
            .iter()
            .any(|p| p.starts_with("CWE-78: the report has results"))
    );
}

fn with_known_gap(corpus_name: &str) -> (Corpus, Expectations) {
    let root = scratch(corpus_name);
    pair_files(&root);
    let cases = pair(EXPECT).replace(
        "files = [\"a/vuln.php\"]",
        "files = [\"a/vuln.php\"]\nknown_gap = { reason = \"dynamic call\", tracking = \"#12\" }",
    );
    let text = "[eval]\nscan-switch = true\n[cwe.\"CWE-89\"]\nmin_tp = 0\nmax_fp = 0\nknown_gaps = [\"v\"]\n";
    (corpus(&root, &cases), expectations(text))
}

#[test]
fn a_known_gap_that_fails_is_reported_not_failed() {
    let (corpus, exp) = with_known_gap("gap-fails");
    let e = evaluate_report(&corpus, &exp, &document(&[]));
    assert_eq!(e.verdict, Verdict::Pass, "{:?}", e.problems);
    assert_eq!(e.scored.expect("scored").per_cwe["CWE-89"].known_gaps, 1);
    assert!(
        to_json(&evaluate_report(&corpus, &exp, &document(&[])), &corpus).contains("dynamic call")
    );
}

#[test]
fn a_known_gap_that_now_passes_fails_until_recorded() {
    let (corpus, exp) = with_known_gap("gap-passes");
    let e = evaluate_report(
        &corpus,
        &exp,
        &document(&[finding("a/vuln.php", 3, "CWE-89", "r", "high")]),
    );
    assert_eq!(e.verdict, Verdict::Fail);
    assert!(
        e.problems
            .iter()
            .any(|p| p.contains("known gap v now passes"))
    );
}

#[test]
fn update_records_counts_and_the_diff_lists_every_change() {
    let (corpus, _) = with_known_gap("update");
    let old = expectations(BASE);
    let scored = score(
        &corpus,
        &findings(&[finding("a/vuln.php", 3, "CWE-89", "r", "high")]),
    );
    let new = Expectations::recorded(&corpus, &scored, true);
    let diff = old.diff(&new);
    assert!(
        diff.contains(&"CWE-89: known gap added: v".to_string()),
        "{diff:?}"
    );
    assert!(diff.contains(&"CWE-78: removed".to_string()), "{diff:?}");
    assert_eq!(Expectations::parse(&new.render()).expect("round trip"), new);
    assert!(old.diff(&old).is_empty());
}

#[test]
fn output_is_byte_identical_on_repeated_runs() {
    let corpus = corpus_with_two_classes("bytes");
    let run = || {
        to_json(
            &evaluate_report(&corpus, &expectations(BASE), &document(&perfect())),
            &corpus,
        )
    };
    assert_eq!(run(), run());
    let table_a = table(&evaluate_report(
        &corpus,
        &expectations(BASE),
        &document(&perfect()),
    ));
    let table_b = table(&evaluate_report(
        &corpus,
        &expectations(BASE),
        &document(&perfect()),
    ));
    assert_eq!(table_a, table_b);
}

#[test]
fn no_scan_is_not_evaluated_with_a_reason_and_a_distinct_status() {
    let corpus = corpus_with_two_classes("noscan");
    let exp = expectations(BASE);
    for (code, out, err) in [
        (Some(0), "", ""),
        (Some(2), "", "unknown command"),
        (Some(1), "", "not implemented"),
    ] {
        let obtained = classify(code, out, err);
        assert!(matches!(obtained, Obtained::NoScan(_)), "{code:?}");
        let e = evaluate(&corpus, &exp, &obtained);
        assert_eq!(e.verdict, Verdict::NotEvaluated);
        assert!(e.reason.is_some() && e.scored.is_none());
        assert_eq!(e.verdict.exit_code(), 3);
        assert!(!to_json(&e, &corpus).contains("totals"));
    }
    assert!(matches!(classify(Some(1), "", "boom"), Obtained::Failed(_)));
    assert!(matches!(classify(None, "", ""), Obtained::Failed(_)));
}

#[test]
fn the_switch_makes_not_evaluated_fail_ci() {
    let off = expectations("[eval]\nscan-switch = false\n");
    let on = expectations("[eval]\nscan-switch = true\n");
    assert_eq!(ci_exit_code(Verdict::NotEvaluated, &off), 0);
    assert_eq!(ci_exit_code(Verdict::NotEvaluated, &on), 1);
    assert_eq!(ci_exit_code(Verdict::Fail, &off), 1);
    assert_eq!(ci_exit_code(Verdict::Pass, &on), 0);
}

#[test]
fn the_committed_expectations_parse_and_cover_every_class() {
    let text = std::fs::read_to_string(real_root().join("expectations.toml")).expect("read");
    let exp = Expectations::parse(&text).expect("parses");
    let corpus = Corpus::load(&real_root()).expect("loads");
    for case in &corpus.cases {
        assert!(exp.per_cwe.contains_key(&case.cwe), "{}", case.cwe);
    }
    assert_eq!(exp.render(), text, "expectations.toml is in canonical form");
}

#[test]
fn result_json_validates_against_its_schema() {
    let corpus = corpus_with_two_classes("schema");
    let e = evaluate_report(&corpus, &expectations(BASE), &document(&perfect()));
    let schema: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(real_root().join("../../schemas/eval-result.schema.json"))
            .expect("schema"),
    )
    .expect("schema json");
    let validator = jsonschema::validator_for(&schema).expect("valid schema");
    for evaluation in [
        e,
        evaluate(
            &corpus,
            &expectations(BASE),
            &Obtained::NoScan("none".into()),
        ),
    ] {
        let doc: serde_json::Value =
            serde_json::from_str(&to_json(&evaluation, &corpus)).expect("json");
        let errors: Vec<String> = validator.iter_errors(&doc).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{errors:?}");
    }
}
