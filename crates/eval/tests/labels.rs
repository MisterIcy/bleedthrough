//! The evaluator scores a generated tree through its label file.

mod common;

use bleedthrough_bench::synth::{Generator, Params};
use bleedthrough_eval::corpus::{Corpus, Label};
use bleedthrough_eval::matching::score;
use bleedthrough_eval::report;
use common::scratch;

fn generated(name: &str) -> (std::path::PathBuf, usize) {
    let dir = scratch(name);
    let params = Params {
        seed: 4,
        files: 300,
        vuln_pm: 100,
        ..Params::default()
    };
    let generator = Generator::new(params);
    generator.write(&dir).expect("tree written");
    let planted = generator.labels().matches("label = \"vulnerable\"").count();
    (dir, planted)
}

#[test]
fn the_evaluator_loads_every_planted_pattern() {
    let (dir, planted) = generated("labels-load");
    assert!(planted > 10);
    let corpus = Corpus::load_named(&dir, "labels.toml").expect("loads");
    assert_eq!(
        corpus
            .cases
            .iter()
            .filter(|c| c.label == Label::Vulnerable)
            .count(),
        planted
    );
    assert!(
        corpus.check_labels().is_empty(),
        "{:?}",
        corpus.check_labels()
    );
    // The plain check would call every unlabelled generated file an orphan.
    assert!(
        corpus
            .check()
            .iter()
            .any(|e| e.contains("is not listed by any case"))
    );
}

/// spec: accuracy-evaluation/The scorer is itself tested
#[test]
fn a_perfect_stand_in_report_scores_every_planted_pattern() {
    let (dir, planted) = generated("labels-score");
    let corpus = Corpus::load_named(&dir, "labels.toml").expect("loads");
    let findings: Vec<String> = corpus
        .cases
        .iter()
        .flat_map(|c| &c.expect)
        .map(|e| {
            format!(
                r#"{{"rule":"r","message":"m","location":{{"path":"{}","start":{{"line":{l},"column":1}},"end":{{"line":{l},"column":2}},"byte_start":0,"byte_end":1}},"taxonomy":{{"cwe":["{}"],"owasp":[{{"edition":2021,"id":"A03"}}]}},"severity":{{"level":"high","score":null,"vector":null}},"confidence":{{"tier":"high","reasons":[]}},"fingerprint":"bt1:{:032x}","paranoid":false,"trace":null,"fix":null}}"#,
                e.range.file, e.cwe, u64::from(e.range.start) ^ (e.range.file.len() as u64), l = e.range.start
            )
        })
        .collect();
    let text = format!(
        r#"{{"schema_version":"1.0","tool":{{"name":"bleedthrough","version":"0"}},"options":{{"confidence_threshold":"low","paranoid":false,"php_version":"8.4","ruleset_id":"x"}},"findings":[{}],"coverage":{{"files_clean":1,"files_recovered":0,"files_failed":0,"skipped":[],"limits_hit":[],"unresolved_includes":0,"unresolved_calls":0,"packs":[],"php_version":"8.4","cache":null}},"suppressed":{{"below_threshold_low":0,"below_threshold_medium":0,"baselined":0}}}}"#,
        findings.join(",")
    );
    let scored = score(&corpus, &report::read(&text).expect("valid"));
    let totals = scored.totals();
    assert_eq!((totals.tp as usize, totals.fp, totals.fn_), (planted, 0, 0));
}
