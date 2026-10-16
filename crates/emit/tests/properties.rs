//! Emitters over generated reports: byte-identical output for equal reports, independence from finding
//! order, output free of control sequences, and JSON/NDJSON that validate against the published schema.
//! Generators of reports live here, built from the public constructors (docs/testing.md).

mod common;

use bleedthrough_app::Format;
use bleedthrough_findings::{Applicability, ConfidenceTier, Fix, Report, Suppressed};
use bleedthrough_taxonomy::SeverityLevel;
use bleedthrough_testkit::check;
use bleedthrough_testkit::proptest::prelude::*;
use serde_json::Value;

/// Text built from troublesome characters: delimiters, escapes, controls, non-ASCII.
fn hostile_text() -> impl Strategy<Value = String> {
    let alphabet = vec![
        "a",
        "Z",
        "0",
        " ",
        "\t",
        "\n",
        "\r",
        "\\",
        "%",
        ":",
        ",",
        "::",
        "\"",
        "<",
        "é",
        "日",
        "\u{1F600}",
    ];
    prop::collection::vec(prop::sample::select(alphabet), 0..12).prop_map(|parts| parts.concat())
}

fn level() -> impl Strategy<Value = SeverityLevel> {
    prop::sample::select(vec![
        SeverityLevel::Info,
        SeverityLevel::Low,
        SeverityLevel::Medium,
        SeverityLevel::High,
        SeverityLevel::Critical,
    ])
}

fn tier() -> impl Strategy<Value = ConfidenceTier> {
    prop::sample::select(vec![
        ConfidenceTier::Low,
        ConfidenceTier::Medium,
        ConfidenceTier::High,
    ])
}

/// Where a generated finding can point: a path, its text and a snippet that occurs once in it.
const PLACES: [(&str, &str, &str); 4] = [
    (common::DB, common::DB_TEXT, "mysqli_query($db, $q)"),
    (common::DB, common::DB_TEXT, "$_GET['id']"),
    (
        common::SHELL,
        common::SHELL_TEXT,
        "shell_exec($_GET['cmd'])",
    ),
    (common::HOSTILE, common::HOSTILE_TEXT, "echo $_GET['a']"),
];

fn finding() -> impl Strategy<Value = bleedthrough_findings::FindingParts> {
    (
        "[a-z]{1,4}\\.[a-z]{1,6}",
        hostile_text(),
        hostile_text(),
        0..PLACES.len(),
        level(),
        tier(),
        any::<bool>(),
        prop::option::of((hostile_text(), hostile_text(), any::<bool>())),
    )
        .prop_map(
            |(rule, symbol, message, place, level, tier, paranoid, fix)| {
                let (path, text, needle) = PLACES[place];
                let mut parts = common::parts(
                    &rule,
                    &format!("S{symbol}"),
                    common::at(path, text, needle),
                    level,
                    tier,
                );
                parts.message = bleedthrough_findings::Message::new(format!("m{message}")).unwrap();
                parts.paranoid = paranoid;
                if let Some((description, replacement, mechanical)) = fix {
                    let applicability = if mechanical {
                        Applicability::Mechanical
                    } else {
                        Applicability::NeedsReview
                    };
                    parts.fix = Fix::new(
                        format!("f{description}"),
                        applicability,
                        vec![common::edit(path, text, needle, &format!("r{replacement}"))],
                    )
                    .ok();
                }
                parts
            },
        )
}

fn report() -> impl Strategy<Value = Report> {
    (
        prop::collection::vec(finding(), 0..6),
        0u64..50,
        0u64..50,
        0u64..20,
    )
        .prop_map(|(findings, low, medium, baselined)| {
            let mut suppressed = Suppressed::default();
            suppressed.below_threshold_low = low;
            suppressed.below_threshold_medium = medium;
            suppressed.baselined = baselined;
            let mut coverage = bleedthrough_findings::Coverage::new("8.3");
            coverage.files_clean = findings.len() as u64;
            common::build(findings, coverage, suppressed)
        })
}

fn reversed(report: &Report) -> Report {
    let mut findings = report.findings().to_vec();
    findings.reverse();
    Report::new(
        report.run().clone(),
        findings,
        report.coverage().clone(),
        *report.suppressed(),
    )
}

/// spec: report-emitters/Guarantees common to every format
#[test]
fn equal_reports_emit_identical_bytes_in_every_format_whatever_the_finding_order() {
    check(report(), |report| {
        let other = reversed(&report);
        for format in Format::ALL {
            let first = common::emit(format, &report).0;
            prop_assert_eq!(&first, &common::emit(format, &report).0, "{} twice", format);
            prop_assert_eq!(
                &first,
                &common::emit(format, &other).0,
                "{} reordered",
                format
            );
            let text = String::from_utf8(first)
                .map_err(|e| TestCaseError::fail(format!("{format}: {e}")))?;
            prop_assert!(
                !text.contains('\u{1b}'),
                "{format} contains an escape sequence"
            );
        }
        Ok(())
    });
}

fn schema_validator() -> jsonschema::Validator {
    let path = common::golden_dir().join("../../../../schemas/report.schema.json");
    let schema: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    jsonschema::validator_for(&schema).unwrap()
}

/// spec: report-emitters/Output conforms to the published schema and golden files
#[test]
fn generated_json_documents_and_ndjson_records_validate_against_the_schema() {
    let validator = schema_validator();
    check(report(), |report| {
        let json = common::emit(Format::Json, &report).0;
        let doc: Value =
            serde_json::from_slice(&json).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert!(
            validator.is_valid(&doc),
            "json: {:?}",
            validator.iter_errors(&doc).next()
        );
        let nd = common::emit(Format::Ndjson, &report).0;
        let text = String::from_utf8(nd).map_err(|e| TestCaseError::fail(e.to_string()))?;
        for line in text.lines() {
            let record: Value =
                serde_json::from_str(line).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert!(validator.is_valid(&record), "ndjson: {line}");
        }
        Ok(())
    });
}
