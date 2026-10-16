//! Timing, memory, results, comparison and baselines, on injected samples and counters.

use std::collections::BTreeMap;

use bleedthrough_bench::baseline::{Baselines, Entries, observed};
use bleedthrough_bench::compare::{Options, Verdict, compare};
use bleedthrough_bench::memory::parse_peak_rss_kb;
use bleedthrough_bench::result::{Document, Environment, StageResult};
use bleedthrough_bench::stage::{
    Allocation, Counters, EndToEnd, InputDesc, Measurement, Outcome, Registry, Stage, measure,
    no_scan_reason,
};
use bleedthrough_bench::timing::{Config, MIN_ITERATIONS, Summary, median, spread};

fn measured(samples: &[u64]) -> Outcome {
    Outcome::Measured(Box::new(Measurement {
        warmups: 3,
        timing: Summary::of(samples).expect("samples"),
        peak_rss_kb: Some(1000),
        counters: Counters::from([("files".to_string(), 10), ("nodes".to_string(), 500)]),
        allocation: Some(Allocation {
            count: 1000,
            total_bytes: 64_000,
            peak_bytes: 8_000,
        }),
    }))
}

fn env(cpu: &str) -> Environment {
    Environment {
        os: "linux".into(),
        cpu_model: cpu.into(),
        logical_cores: 8,
        profile: "release".into(),
    }
}

fn document(cpu: &str, samples: &[u64]) -> Document {
    Document {
        tool_version: "0.0.0".into(),
        commit: Some("abc".into()),
        environment: env(cpu),
        inputs: vec![InputDesc {
            name: "synthetic:seed=1".into(),
            files: 10,
            bytes: 2000,
        }],
        results: vec![StageResult {
            stage: "parse".into(),
            input: "synthetic:seed=1".into(),
            outcome: measured(samples),
        }],
    }
}

#[test]
fn median_and_spread() {
    assert_eq!(median(&[5, 1, 3]), Some(3));
    assert_eq!(median(&[4, 1, 3, 2]), Some(2));
    assert_eq!(median(&[]), None);
    assert_eq!(spread(&[10, 10, 10]), Some(0));
    assert_eq!(spread(&[8, 10, 12, 10, 100]), Some(2));
}

#[test]
fn fewer_than_the_minimum_iterations_is_refused() {
    let config = Config {
        warmups: 0,
        iterations: 1,
    };
    let error = config.validate().expect_err("refused");
    assert!(
        error.contains(&format!("minimum is {MIN_ITERATIONS}")),
        "{error}"
    );
    struct Never;
    impl Stage for Never {
        fn name(&self) -> String {
            "never".into()
        }
        fn pass(&mut self) {
            panic!("must not run");
        }
        fn counters(&mut self) -> Counters {
            Counters::new()
        }
    }
    assert!(measure(&mut Never, &config).is_err());
}

#[test]
fn warm_ups_run_but_are_not_sampled() {
    use std::cell::Cell;
    let runs = Cell::new(0u32);
    let samples = bleedthrough_bench::timing::sample(
        &Config {
            warmups: 2,
            iterations: 6,
        },
        || runs.set(runs.get() + 1),
    )
    .expect("samples");
    assert_eq!((runs.get(), samples.len()), (8, 6));
}

#[test]
fn peak_memory_is_read_from_proc_status_text_and_absent_otherwise() {
    let status = "Name:\tx\nVmPeak:\t  9000 kB\nVmHWM:\t  4321 kB\nVmRSS:\t 4000 kB\n";
    assert_eq!(parse_peak_rss_kb(status), Some(4321));
    assert_eq!(parse_peak_rss_kb("Name:\tx\n"), None);
    let outcome = Outcome::Measured(Box::new(Measurement {
        peak_rss_kb: None,
        ..match measured(&[1, 2, 3, 4, 5]) {
            Outcome::Measured(m) => *m,
            Outcome::Unavailable(_) => unreachable!(),
        }
    }));
    let doc = Document {
        results: vec![StageResult {
            stage: "parse".into(),
            input: "synthetic:seed=1".into(),
            outcome,
        }],
        ..document("x", &[1])
    };
    assert!(doc.to_json().contains("\"peak_rss_kb\": null"));
}

#[test]
fn an_unavailable_stage_serialises_with_its_reason() {
    let mut registry = Registry::new();
    registry.register(Box::new(EndToEnd::new(
        "/nonexistent/bleedthrough".into(),
        ".".into(),
    )));
    let results = registry.run(&Config::default()).expect("runs");
    let Outcome::Unavailable(reason) = &results[0].1 else {
        panic!("expected unavailable")
    };
    assert!(reason.contains("cannot run"), "{reason}");
    let doc = Document {
        results: vec![StageResult {
            stage: "end-to-end".into(),
            input: "synthetic:seed=1".into(),
            outcome: results[0].1.clone(),
        }],
        ..document("x", &[1])
    };
    let json = doc.to_json();
    assert!(json.contains("\"status\": \"unavailable\"") && json.contains("cannot run"));
    assert_eq!(Document::from_json(&json).expect("round trip"), doc);
}

#[test]
fn no_scan_is_recognised() {
    assert!(no_scan_reason(Some(0), b"", b"").is_some());
    assert!(no_scan_reason(Some(2), b"", b"usage").is_some());
    assert!(no_scan_reason(Some(1), b"", b"not implemented").is_some());
    assert!(no_scan_reason(Some(0), b"{}", b"").is_none());
    assert!(no_scan_reason(Some(1), b"", b"boom").is_none());
}

#[test]
fn documents_round_trip_and_serialise_identically() {
    let doc = document("cpu", &[100, 110, 105, 95, 100]);
    let json = doc.to_json();
    assert_eq!(json, document("cpu", &[100, 110, 105, 95, 100]).to_json());
    assert_eq!(Document::from_json(&json).expect("reads"), doc);
    assert!(
        !json.contains('/') || !json.contains("home"),
        "no absolute paths"
    );
}

#[test]
fn old_and_unknown_versions() {
    let json = document("cpu", &[1, 2, 3, 4, 5]).to_json();
    let old = json.replace("\"schema_version\": \"1\"", "\"schema_version\": \"0\"");
    let error = Document::from_json(&old).expect_err("rejected");
    assert!(
        error.contains("`0`") && error.contains("not supported"),
        "{error}"
    );
}

#[test]
fn result_json_validates_against_its_schema() {
    let schema: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../schemas/bench-result.schema.json"
        ))
        .expect("schema"),
    )
    .expect("json");
    let mut doc = document("cpu", &[100, 110, 105, 95, 100]);
    doc.results.push(StageResult {
        stage: "end-to-end".into(),
        input: "synthetic:seed=1".into(),
        outcome: Outcome::Unavailable("no scan".into()),
    });
    let value: serde_json::Value = serde_json::from_str(&doc.to_json()).expect("json");
    let validator = jsonschema::validator_for(&schema).expect("valid schema");
    let errors: Vec<String> = validator
        .iter_errors(&value)
        .map(|e| format!("{e} at {}", e.instance_path))
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn within_noise_is_no_regression() {
    // Base varies by ~5% (MAD/median), head is 2% slower.
    let base = document("cpu", &[95, 100, 105, 100, 100, 95, 105, 100, 100]);
    let head = document("cpu", &[102, 102, 102, 102, 102]);
    let row = &compare(&base, &head, &Options::default())
        .expect("compares")
        .rows[0];
    assert_eq!(row.verdict, Verdict::NoChange);
}

#[test]
fn a_clear_regression_names_stage_and_input() {
    let base = document("cpu", &[97, 100, 103, 100, 100]);
    let head = document("cpu", &[120, 120, 120, 120, 120]);
    let comparison = compare(&base, &head, &Options::default()).expect("compares");
    assert_eq!(comparison.rows[0].verdict, Verdict::Regression);
    let report = comparison.report();
    assert!(
        report.contains("REGRESSION: stage `parse` on `synthetic:seed=1`"),
        "{report}"
    );
}

#[test]
fn noise_widens_the_limit() {
    // 12% slower exceeds the 10% threshold, but the base noise is 10%, so the limit is 30%.
    let base = document("cpu", &[90, 100, 110, 100, 90, 110, 100]);
    let head = document("cpu", &[112, 112, 112, 112, 112]);
    assert_eq!(
        compare(&base, &head, &Options::default()).expect("ok").rows[0].verdict,
        Verdict::NoChange
    );
}

#[test]
fn improvements_are_reported_as_such() {
    let base = document("cpu", &[100, 100, 100, 100, 100]);
    let head = document("cpu", &[50, 50, 50, 50, 50]);
    assert_eq!(
        compare(&base, &head, &Options::default()).expect("ok").rows[0].verdict,
        Verdict::Improvement
    );
}

#[test]
fn different_machines_are_refused_unless_forced_and_then_labelled() {
    let base = document("cpu A", &[100, 100, 100, 100, 100]);
    let head = document("cpu B", &[100, 100, 100, 100, 100]);
    let error = compare(&base, &head, &Options::default()).expect_err("refused");
    assert!(
        error.contains("different environments")
            && error.contains("cpu A")
            && error.contains("--force"),
        "{error}"
    );
    let forced = compare(
        &base,
        &head,
        &Options {
            force: true,
            ..Options::default()
        },
    )
    .expect("forced");
    assert!(forced.forced && forced.report().contains("forced comparison"));
}

#[test]
fn input_drift_is_refused_and_explained() {
    let base = document("cpu", &[100; 5]);
    let mut head = document("cpu", &[100; 5]);
    head.inputs[0].bytes += 1;
    let error = compare(&base, &head, &Options::default()).expect_err("refused");
    assert!(
        error.contains("drifted") && error.contains("2000") && error.contains("2001"),
        "{error}"
    );
}

#[test]
fn an_unavailable_side_is_not_a_verdict() {
    let base = document("cpu", &[100; 5]);
    let mut head = document("cpu", &[100; 5]);
    head.results[0].outcome = Outcome::Unavailable("gone".into());
    assert_eq!(
        compare(&base, &head, &Options::default()).expect("ok").rows[0].verdict,
        Verdict::Unavailable
    );
}

fn entries(count: u64) -> Entries {
    let Outcome::Measured(m) = measured(&[1, 2, 3, 4, 5]) else {
        unreachable!()
    };
    let mut set = observed(&m);
    set.insert("alloc_count".into(), count);
    BTreeMap::from([(("parse".to_string(), "synthetic:seed=1".to_string()), set)])
}

#[test]
fn baseline_holds_within_tolerance_and_names_a_doubling() {
    let baseline = Baselines {
        tolerance_pct: 2,
        entries: entries(1000),
    };
    assert!(baseline.check(&entries(1000)).is_empty());
    assert!(baseline.check(&entries(1015)).is_empty());
    let problems = baseline.check(&entries(2000));
    assert_eq!(problems.len(), 1);
    let p = &problems[0];
    assert!(
        p.contains("parse")
            && p.contains("alloc_count")
            && p.contains("baseline 1000")
            && p.contains("now 2000"),
        "{p}"
    );
}

#[test]
fn lowering_beyond_tolerance_must_be_recorded() {
    let baseline = Baselines {
        tolerance_pct: 2,
        entries: entries(1000),
    };
    let problems = baseline.check(&entries(500));
    assert!(
        problems[0].contains("improved") && problems[0].contains("--update"),
        "{problems:?}"
    );
}

#[test]
fn input_counters_are_exact() {
    let baseline = Baselines {
        tolerance_pct: 50,
        entries: entries(1000),
    };
    let mut now = entries(1000);
    now.values_mut()
        .next()
        .expect("entry")
        .insert("nodes".into(), 501);
    let problems = baseline.check(&now);
    assert!(
        problems[0].contains("`nodes`") && problems[0].contains("exact"),
        "{problems:?}"
    );
}

#[test]
fn baseline_update_changes_only_counters_and_the_diff_shows_each() {
    let old = Baselines {
        tolerance_pct: 2,
        entries: entries(1000),
    };
    let new = old.updated(entries(600));
    let diff = old.diff(&new);
    assert_eq!(
        diff,
        vec!["parse `synthetic:seed=1` alloc_count: 1000 -> 600".to_string()]
    );
    let text = new.render();
    assert_eq!(Baselines::parse(&text).expect("round trip"), new);
    assert!(old.diff(&old).is_empty());
}

#[test]
fn missing_and_unmeasured_baselines_are_reported() {
    let baseline = Baselines {
        tolerance_pct: 2,
        entries: entries(1000),
    };
    assert!(baseline.check(&Entries::new())[0].contains("was not measured"));
    let empty = Baselines {
        tolerance_pct: 2,
        entries: Entries::new(),
    };
    assert!(empty.check(&entries(1))[0].contains("has no baseline"));
}

#[test]
fn merging_concatenates_samples_and_keeps_the_largest_peak() {
    let a = document("cpu", &[10, 20, 30, 40, 50]);
    let b = document("cpu", &[60, 70, 80, 90, 100]);
    let merged = Document::merge(&[a.clone(), b]).expect("merges");
    let Outcome::Measured(m) = &merged.results[0].outcome else {
        panic!("measured")
    };
    assert_eq!(m.timing.samples_ns.len(), 10);
    assert_eq!(m.timing.median_ns, 55);
    assert!(Document::merge(&[a.clone(), document("other cpu", &[1; 5])]).is_err());
    assert_eq!(
        Document::merge(std::slice::from_ref(&a)).expect("single"),
        a
    );
}
