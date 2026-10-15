//! Structural assertions on the SARIF 2.1.0 output. The official schema is not vendored: its
//! repository carries OASIS terms rather than an open-source licence.

mod common;

use bleedthrough_app::Format;
use serde_json::Value;

fn log(report: &bleedthrough_findings::Report) -> Value {
    serde_json::from_slice(&common::emit(Format::Sarif, report).0).unwrap()
}

#[test]
fn log_shape_and_level_mapping() {
    let l = log(&common::mixed());
    assert_eq!(l["version"], "2.1.0");
    assert_eq!(l["runs"].as_array().unwrap().len(), 1);
    let results = l["runs"][0]["results"].as_array().unwrap();
    let levels: Vec<(&str, &str)> = results
        .iter()
        .map(|r| (r["ruleId"].as_str().unwrap(), r["level"].as_str().unwrap()))
        .collect();
    assert!(levels.contains(&("php.sqli", "error")) && levels.contains(&("php.cmdi", "error")));
    assert!(levels.contains(&("php.xss", "warning")) && levels.contains(&("php.xss", "note")));
    assert!(levels.contains(&("php.info", "note")));
}

#[test]
fn rules_are_declared_and_indexed_with_taxonomy_relationships() {
    let l = log(&common::full());
    let run = &l["runs"][0];
    let rules = run["tool"]["driver"]["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 1);
    let result = &run["results"][0];
    assert_eq!(
        rules[result["ruleIndex"].as_u64().unwrap() as usize]["id"],
        result["ruleId"]
    );
    let names: Vec<&str> = run["taxonomies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["CWE", "OWASP Top 10 2021", "OWASP Top 10 2025"]);
    for rel in rules[0]["relationships"].as_array().unwrap() {
        let target = &rel["target"];
        let component = target["toolComponent"]["index"].as_u64().unwrap() as usize;
        let taxon = target["index"].as_u64().unwrap() as usize;
        assert_eq!(
            run["taxonomies"][component]["taxa"][taxon]["id"],
            target["id"]
        );
    }
    assert_eq!(rules[0]["properties"]["security-severity"], "9.5");
}

#[test]
fn fingerprint_code_flow_fix_and_properties_are_carried() {
    let report = common::full();
    let r = &log(&report)["runs"][0]["results"][0];
    let finding = &report.findings()[0];
    assert_eq!(
        r["partialFingerprints"]["bleedthrough/v1"],
        finding.fingerprint().as_str()
    );
    let steps = r["codeFlows"][0]["threadFlows"][0]["locations"]
        .as_array()
        .unwrap();
    let trace = finding.trace().unwrap().steps();
    assert_eq!(steps.len(), trace.len());
    for (s, t) in steps.iter().zip(trace) {
        assert_eq!(s["location"]["message"]["text"], t.label());
        assert_eq!(s["kinds"][0], t.kind().as_str());
        assert_eq!(
            s["location"]["physicalLocation"]["region"]["startLine"],
            t.location().start().line()
        );
    }
    assert_eq!(
        r["fixes"][0]["artifactChanges"][0]["replacements"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let p = &r["properties"];
    assert_eq!(
        (
            p["confidence"].as_str(),
            p["paranoid"].as_bool(),
            p["securitySeverity"].as_str()
        ),
        (Some("medium"), Some(true), Some("9.5"))
    );
    assert_eq!(p["confidenceReasons"][1]["detail"], "3");
}

#[test]
fn incomplete_runs_are_visible() {
    let inv = &log(&common::incomplete())["runs"][0]["invocations"][0];
    assert_eq!(inv["executionSuccessful"], false);
    let notes = inv["toolExecutionNotifications"].as_array().unwrap();
    assert!(notes.iter().any(|n| {
        n["message"]["text"]
            .as_str()
            .unwrap()
            .contains("2 file(s) failed to parse")
    }));
    assert!(notes.iter().any(
        |n| n["locations"][0]["physicalLocation"]["artifactLocation"]["uri"] == "src/slow.php"
    ));
    assert_eq!(inv["properties"]["coverage"]["files_failed"], 2);
    assert_eq!(inv["properties"]["suppressed"]["baselined"], 0);
    assert_eq!(
        log(&common::mixed())["runs"][0]["invocations"][0]["executionSuccessful"],
        true
    );
}
