//! Integration through the public `app` port only: resolve a `Format`, get an emitter, emit,
//! inspect the outcome. Nothing from `emit` is used except `emitter_for`.

mod common;

use bleedthrough_app::{EmitContext, EmitError, Format};

#[test]
fn every_format_name_resolves_and_emits() {
    for name in Format::NAMES {
        let format: Format = name.parse().unwrap();
        assert!(
            bleedthrough_emit::emitter_for(format).is_some(),
            "format {name} is not wired to an emitter"
        );
    }
}

#[test]
fn emitters_report_the_format_they_were_resolved_for() {
    for format in Format::ALL {
        assert_eq!(
            bleedthrough_emit::emitter_for(format).unwrap().format(),
            format
        );
    }
}

#[test]
fn jsonl_is_byte_identical_to_ndjson() {
    let alias: Format = "jsonl".parse().unwrap();
    for (name, report) in common::all() {
        assert_eq!(
            common::emit(alias, &report).0,
            common::emit(Format::Ndjson, &report).0,
            "{name}"
        );
    }
}

#[test]
fn unknown_format_error_lists_the_six_names() {
    let message = "xml".parse::<Format>().unwrap_err().to_string();
    assert!(
        Format::NAMES.iter().all(|n| message.contains(n)),
        "{message}"
    );
}

#[test]
fn incomplete_runs_yield_a_summary_for_tsv_and_diff() {
    for format in [Format::Tsv, Format::Diff] {
        let (_, outcome) = common::emit(format, &common::incomplete());
        let summary = outcome.coverage_summary.expect("summary line");
        assert!(
            summary.contains("not analysed") && !summary.contains('\n'),
            "{summary}"
        );
    }
    let (_, outcome) = common::emit(Format::Tsv, &common::mixed());
    assert!(
        outcome
            .coverage_summary
            .unwrap()
            .contains("suppressed: 12 low and 3 medium confidence, 7 by baseline")
    );
}

#[test]
fn formats_with_a_native_place_embed_coverage_instead() {
    let (json, outcome) = common::emit(Format::Json, &common::incomplete());
    assert!(outcome.coverage_summary.is_none());
    assert!(
        String::from_utf8(json)
            .unwrap()
            .contains("\"files_failed\": 2")
    );
    let (gha, _) = common::emit(Format::GithubActions, &common::incomplete());
    assert!(
        String::from_utf8(gha)
            .unwrap()
            .contains("2 failed to parse")
    );
}

#[test]
fn conflicting_fix_is_skipped_with_a_reason() {
    let (_, outcome) = common::emit(Format::Diff, &common::mixed());
    assert_eq!(outcome.skipped_fixes.len(), 1);
    assert!(outcome.skipped_fixes[0].reason().contains("overlaps"));
}

#[test]
fn stale_source_skips_the_fix() {
    let mut src = common::sources();
    src.0.insert(
        common::DB.into(),
        format!(
            "{}// edited\n",
            common::DB_TEXT.replace("mysqli_query", "mysqli_exec")
        ),
    );
    let ctx = EmitContext::new().with_sources(&src);
    let mut out = Vec::new();
    let outcome = bleedthrough_emit::emitter_for(Format::Diff)
        .unwrap()
        .emit(&common::mixed(), &ctx, &mut out)
        .unwrap();
    assert!(
        outcome
            .skipped_fixes
            .iter()
            .any(|s| s.reason().contains("changed since"))
    );
    assert!(!String::from_utf8(out).unwrap().contains("src/Db.php"));
}

#[test]
fn diff_without_sources_is_an_error_not_an_empty_patch() {
    let mut out = Vec::new();
    let result = bleedthrough_emit::emitter_for(Format::Diff).unwrap().emit(
        &common::mixed(),
        &EmitContext::new(),
        &mut out,
    );
    assert!(matches!(result, Err(EmitError::Unavailable(_))));
}

#[test]
fn zero_findings_give_valid_empty_results() {
    let report = common::empty();
    assert!(common::emit(Format::Diff, &report).0.is_empty());
    let (tsv, _) = common::emit(Format::Tsv, &report);
    assert_eq!(
        String::from_utf8(tsv).unwrap().lines().count(),
        1,
        "header only"
    );
    let (sarif, _) = common::emit(Format::Sarif, &report);
    let log: serde_json::Value = serde_json::from_slice(&sarif).unwrap();
    assert_eq!(log["runs"][0]["results"].as_array().unwrap().len(), 0);
    let (gha, _) = common::emit(Format::GithubActions, &report);
    assert!(!String::from_utf8(gha).unwrap().contains("::error"));
}

#[test]
fn tsv_header_can_be_omitted() {
    let ctx = EmitContext::new().with_header(false);
    let mut out = Vec::new();
    bleedthrough_emit::emitter_for(Format::Tsv)
        .unwrap()
        .emit(&common::mixed(), &ctx, &mut out)
        .unwrap();
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text.lines().count(), 6);
    assert!(text.lines().all(|l| l.split('\t').count() == 12));
}

#[test]
fn tsv_escapes_keep_one_row_and_twelve_columns() {
    let (out, _) = common::emit(Format::Tsv, &common::hostile());
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text.lines().count(), 2);
    assert!(text.lines().all(|l| l.split('\t').count() == 12));
    assert!(text.contains("unsafe:\\tcolumn\\nnext line\\r\\\\"));
}

#[test]
fn github_actions_cap_keeps_the_most_severe_and_counts_the_rest() {
    let ctx = EmitContext::new().with_annotation_cap(2);
    let mut out = Vec::new();
    bleedthrough_emit::emitter_for(Format::GithubActions)
        .unwrap()
        .emit(&common::mixed(), &ctx, &mut out)
        .unwrap();
    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines[0].starts_with("::error") && lines[0].contains("title=php.sqli::"),
        "{lines:?}"
    );
    assert!(
        lines[1].starts_with("::error") && lines[1].contains("title=php.cmdi"),
        "{lines:?}"
    );
    assert!(
        lines[2].starts_with("::notice") && lines[2].contains("4 more finding(s) omitted"),
        "{lines:?}"
    );
    assert_eq!(
        lines.len(),
        4,
        "two annotations, omitted notice, coverage notice"
    );
}
