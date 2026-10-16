//! Emitter output against the published JSON Schema.

mod common;

use bleedthrough_app::Format;
use serde_json::Value;

type Mutation = fn(&mut Value);

fn schema() -> Value {
    let path = common::golden_dir().join("../../../../schemas/report.schema.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn the_schema_is_itself_a_valid_2020_12_schema() {
    jsonschema::meta::validate(&schema()).expect("schema validates against the metaschema");
}

/// spec: report-emitters/Output conforms to the published schema and golden files
#[test]
fn json_documents_and_ndjson_records_validate() {
    let validator = jsonschema::validator_for(&schema()).unwrap();
    for (name, report) in common::all() {
        let (json, _) = common::emit(Format::Json, &report);
        let doc: Value = serde_json::from_slice(&json).unwrap();
        assert!(
            validator.is_valid(&doc),
            "{name}.json: {:?}",
            validator.iter_errors(&doc).next()
        );
        let (nd, _) = common::emit(Format::Ndjson, &report);
        let lines: Vec<&str> = std::str::from_utf8(&nd).unwrap().lines().collect();
        assert_eq!(lines.len(), report.findings().len() + 2);
        for line in lines {
            let record: Value = serde_json::from_str(line).unwrap();
            assert!(validator.is_valid(&record), "{name}.ndjson: {line}");
        }
    }
}

#[test]
fn the_harness_rejects_malformed_documents() {
    let validator = jsonschema::validator_for(&schema()).unwrap();
    let (json, _) = common::emit(Format::Json, &common::full());
    let good: Value = serde_json::from_slice(&json).unwrap();
    assert!(validator.is_valid(&good));
    let mutations: [(&str, Mutation); 5] = [
        ("missing coverage", |v| {
            v.as_object_mut().unwrap().remove("coverage");
        }),
        ("bad fingerprint", |v| {
            v["findings"][0]["fingerprint"] = "x".into()
        }),
        ("unknown reason code", |v| {
            v["findings"][0]["confidence"]["reasons"][0]["code"] = "made_up".into()
        }),
        ("absolute path", |v| {
            v["findings"][0]["location"]["path"] = "/etc/passwd".into()
        }),
        ("extra field", |v| v["findings"][0]["extra"] = true.into()),
    ];
    for (what, mutate) in mutations {
        let mut bad = good.clone();
        mutate(&mut bad);
        assert!(!validator.is_valid(&bad), "accepted: {what}");
    }
}

#[test]
fn the_schema_enumerates_every_reason_code() {
    use bleedthrough_findings::ReasonCode;
    let schema = schema();
    let listed: Vec<&str> = schema["$defs"]["reason_code"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let known: Vec<&str> = ReasonCode::ALL.iter().map(ReasonCode::as_str).collect();
    assert_eq!(listed, known);
}

#[test]
fn schema_version_matches_the_report() {
    let (json, _) = common::emit(Format::Json, &common::empty());
    let doc: Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(
        doc["schema_version"],
        bleedthrough_findings::REPORT_SCHEMA_VERSION
    );
    assert!(
        schema()["title"]
            .as_str()
            .unwrap()
            .ends_with(bleedthrough_findings::REPORT_SCHEMA_VERSION)
    );
}

#[test]
fn every_finding_field_reaches_the_json_output() {
    let (json, _) = common::emit(Format::Json, &common::full());
    let doc: Value = serde_json::from_slice(&json).unwrap();
    let finding = doc["findings"][0].as_object().unwrap();
    let keys: Vec<&str> = finding.keys().map(String::as_str).collect();
    for key in [
        "rule",
        "message",
        "location",
        "taxonomy",
        "severity",
        "confidence",
        "fingerprint",
        "paranoid",
        "trace",
        "fix",
    ] {
        assert!(keys.contains(&key), "missing {key}");
        assert!(
            !finding[key].is_null(),
            "{key} is null for the full-featured finding"
        );
    }
    assert_eq!(finding.len(), 10);
    assert_eq!(finding["severity"]["score"], 9.5);
}
