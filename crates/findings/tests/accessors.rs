//! The public vocabulary of the findings model: wire names, accessors and the completeness
//! judgement, checked through the public API only.

use bleedthrough_findings::{
    Applicability, Confidence, ConfidenceTier, Coverage, Edit, Finding, FindingParts, Fingerprint,
    FingerprintInput, Fix, LimitHit, LimitKind, Location, Message, PackRef, Position, RelPath,
    Report, RuleId, RunInfo, RunOptions, SkipReason, SkippedFile, Step, StepKind, Suppressed,
    Trace,
};
use bleedthrough_taxonomy::{Cwe, OwaspRef, Severity, SeverityLevel, Taxonomy};

fn path(p: &str) -> RelPath {
    RelPath::new(p).unwrap()
}

fn location() -> Location {
    let p = Position::new(1, 1).unwrap();
    Location::new(path("a.php"), p, p, 0, 0).unwrap()
}

fn edit() -> Edit {
    Edit::new(path("a.php"), 3, 8, "hello", "world!").unwrap()
}

fn finding(paranoid: bool, trace: Option<Trace>, fix: Option<Fix>) -> Finding {
    let rule = RuleId::new("BT0001").unwrap();
    Finding::new(FindingParts {
        fingerprint: Fingerprint::compute(&FingerprintInput::new(rule.clone(), "f", vec![], 0)),
        rule,
        message: Message::new("tainted value").unwrap(),
        location: location(),
        taxonomy: Taxonomy::new(
            vec![Cwe::new(89).unwrap()],
            vec![OwaspRef::new(2021, "A03").unwrap()],
        )
        .unwrap(),
        severity: Severity::new(SeverityLevel::High),
        confidence: Confidence::new(ConfidenceTier::High, vec![]).unwrap(),
        paranoid,
        trace,
        fix,
    })
}

/// spec: finding-model/Confidence is a tier with documented reasons
#[test]
fn wire_names_are_stable_lowercase_words() {
    assert_eq!(ConfidenceTier::Low.as_str(), "low");
    assert_eq!(ConfidenceTier::Medium.as_str(), "medium");
    assert_eq!(ConfidenceTier::High.as_str(), "high");

    let skips = [
        (SkipReason::Excluded, "excluded"),
        (SkipReason::TooLarge, "too_large"),
        (SkipReason::Unreadable, "unreadable"),
        (SkipReason::NotUtf8, "not_utf8"),
        (SkipReason::TimeLimit, "time_limit"),
        (SkipReason::InternalFault, "internal_fault"),
    ];
    for (reason, name) in skips {
        assert_eq!(reason.as_str(), name);
    }

    let limits = [
        (LimitKind::PerFileTime, "per_file_time"),
        (LimitKind::FileSize, "file_size"),
        (LimitKind::CallDepth, "call_depth"),
        (LimitKind::TotalTime, "total_time"),
        (LimitKind::BlockLimit, "block_limit"),
        (LimitKind::InstructionLimit, "instruction_limit"),
        (LimitKind::StepBudget, "step_budget"),
        (LimitKind::BlockVisits, "block_visits"),
        (LimitKind::OriginCap, "origin_cap"),
        (LimitKind::Iterations, "iterations"),
        (LimitKind::SummaryDepth, "summary_depth"),
        (LimitKind::Candidates, "candidates"),
        (LimitKind::ExpressionDepth, "expression_depth"),
        (LimitKind::ComponentSize, "component_size"),
    ];
    for (kind, name) in limits {
        assert_eq!(kind.as_str(), name);
    }

    assert_eq!(Applicability::Mechanical.as_str(), "mechanical");
    assert_eq!(Applicability::NeedsReview.as_str(), "needs_review");

    let steps = [
        (StepKind::Source, "source"),
        (StepKind::Propagation, "propagation"),
        (StepKind::Call, "call"),
        (StepKind::SanitizerConsidered, "sanitizer_considered"),
        (StepKind::Sink, "sink"),
    ];
    for (kind, name) in steps {
        assert_eq!(kind.as_str(), name);
    }
}

#[test]
fn a_message_keeps_its_text() {
    assert_eq!(
        Message::new("tainted value").unwrap().as_str(),
        "tainted value"
    );
}

#[test]
fn a_limit_hit_reports_its_file_only_when_it_has_one() {
    let with = LimitHit::new(LimitKind::PerFileTime, Some(path("src/a.php")));
    assert_eq!(with.kind(), LimitKind::PerFileTime);
    assert_eq!(with.path(), Some(&path("src/a.php")));
    assert_eq!(LimitHit::new(LimitKind::TotalTime, None).path(), None);
}

#[test]
fn a_pack_reference_keeps_name_and_version() {
    let pack = PackRef::new("symfony", "1.2.3");
    assert_eq!(pack.name(), "symfony");
    assert_eq!(pack.version(), "1.2.3");
}

/// spec: finding-model/Coverage states what was and was not analysed
#[test]
fn each_unexamined_thing_alone_makes_coverage_incomplete() {
    assert!(!Coverage::new("8.3").is_incomplete());

    let mut failed = Coverage::new("8.3");
    failed.files_failed = 1;
    assert!(failed.is_incomplete());

    let mut recovered = Coverage::new("8.3");
    recovered.files_recovered = 1;
    assert!(recovered.is_incomplete());

    let mut skipped = Coverage::new("8.3");
    skipped.skipped = vec![SkippedFile::new(path("a.php"), SkipReason::Excluded)];
    assert!(skipped.is_incomplete());

    let mut limited = Coverage::new("8.3");
    limited.limits_hit = vec![LimitHit::new(LimitKind::CallDepth, None)];
    assert!(limited.is_incomplete());

    // clean files, unresolved calls and includes, and cache use do not make a run incomplete
    let mut clean = Coverage::new("8.3");
    clean.files_clean = 10;
    clean.unresolved_calls = 4;
    clean.unresolved_includes = 2;
    assert!(!clean.is_incomplete());
}

#[test]
fn an_edit_exposes_its_range_and_texts() {
    let e = edit();
    assert_eq!(e.path(), &path("a.php"));
    assert_eq!((e.start(), e.end()), (3, 8));
    assert_eq!(e.expected(), "hello");
    assert_eq!(e.replacement(), "world!");
}

/// spec: finding-model/Fixes are optional, explicit and non-overlapping
#[test]
fn a_fix_exposes_description_applicability_and_edits() {
    let fix = Fix::new(
        "use a prepared statement",
        Applicability::NeedsReview,
        vec![edit()],
    )
    .unwrap();
    assert_eq!(fix.description(), "use a prepared statement");
    assert_eq!(fix.applicability(), Applicability::NeedsReview);
    assert_eq!(fix.edits().len(), 1);
}

/// spec: finding-model/A finding carries identity, evidence and classification
#[test]
fn a_finding_exposes_paranoid_trace_and_fix() {
    let bare = finding(false, None, None);
    assert!(!bare.paranoid());
    assert!(bare.trace().is_none() && bare.fix().is_none());

    let trace = Trace::new(
        vec![
            Step::new(location(), StepKind::Source, "input"),
            Step::new(location(), StepKind::Sink, "query"),
        ],
        10,
    )
    .unwrap();
    let fix = Fix::new("d", Applicability::Mechanical, vec![edit()]).unwrap();
    let full = finding(true, Some(trace.clone()), Some(fix.clone()));
    assert!(full.paranoid());
    assert_eq!(full.trace(), Some(&trace));
    assert_eq!(full.fix(), Some(&fix));
}

/// spec: finding-model/Suppressed counts are always reported
#[test]
fn a_report_exposes_run_information_and_suppressed_counts() {
    let options = RunOptions::new(ConfidenceTier::Medium, true, "8.3", "rules-1");
    let run = RunInfo::new("bleedthrough", "1.4.2", options);
    assert_eq!(run.tool_name(), "bleedthrough");
    assert_eq!(run.tool_version(), "1.4.2");

    let mut suppressed = Suppressed::default();
    suppressed.below_threshold_low = 12;
    suppressed.below_threshold_medium = 3;
    suppressed.baselined = 7;
    let report = Report::new(run, vec![], Coverage::new("8.3"), suppressed);
    assert_eq!(report.suppressed(), &suppressed);
    assert_eq!(report.suppressed().baselined, 7);
}
