//! The examples in `docs/output-formats.md` are the golden files, verbatim.

mod common;

fn docs() -> String {
    let path = common::golden_dir().join("../../../../docs/output-formats.md");
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn documented_examples_match_the_golden_files() {
    let docs = docs();
    for name in ["incomplete.tsv", "mixed.gha.txt", "mixed.patch"] {
        let golden = std::fs::read_to_string(common::golden_dir().join(name)).unwrap();
        assert!(
            docs.contains(&golden),
            "docs/output-formats.md is out of date with {name}"
        );
    }
    let ndjson = std::fs::read_to_string(common::golden_dir().join("mixed.ndjson")).unwrap();
    let lines: Vec<&str> = ndjson.lines().collect();
    assert!(docs.contains(lines[0]) && docs.contains(lines[lines.len() - 1]));
}

#[test]
fn docs_name_the_golden_directory_the_schema_version_and_every_reason() {
    let docs = docs();
    assert!(docs.contains("crates/emit/tests/golden/"));
    assert!(docs.contains(&format!(
        "**{}**",
        bleedthrough_findings::REPORT_SCHEMA_VERSION
    )));
    for code in bleedthrough_findings::ReasonCode::ALL {
        assert!(
            docs.contains(&format!("`{}`", code.as_str())),
            "{}",
            code.as_str()
        );
    }
}
