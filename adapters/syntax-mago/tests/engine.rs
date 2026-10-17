//! The whole `fixtures/vuln` corpus through lowering, the call graph and summaries.

use std::collections::BTreeMap;

use bleedthrough_engine_suite::engine::{Gate, RuleIndex};
use bleedthrough_engine_suite::rules::RuleSet;
use bleedthrough_engine_suite::{Project, workspace_root};
use bleedthrough_syntax_mago::MagoBackend;

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

fn rule_of(cwe: &str) -> &'static str {
    match cwe {
        "22" => "BT9003",
        "78" => "BT9002",
        "89" => "BT9001",
        "502" => "BT9004",
        "601" => "BT9005",
        "918" => "BT9006",
        other => panic!("no test rule for CWE-{other}"),
    }
}

fn php_files(dir: &std::path::Path, prefix: &str, out: &mut Vec<(String, String)>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if path.is_dir() {
            php_files(&path, &format!("{prefix}{name}/"), out);
        } else if name.ends_with(".php") {
            out.push((
                format!("{prefix}{name}"),
                std::fs::read_to_string(&path).unwrap(),
            ));
        }
    }
}

/// What the engine does with each CWE's cases: vulnerable cases found, safe twins flagged.
///
/// The twins of CWE-502, 601 and 918 are made safe by a guard (`in_array`, `str_starts_with`) or a
/// sink option (`allowed_classes`); recognising either is a follow-up change, so every one of them
/// is a known false positive and is asserted as one: the test fails if the gap closes, so the
/// expectation gets tightened.
const EXPECTED: [(&str, u32, u32); 6] = [
    // (CWE number, vulnerable cases found, safe twins flagged)
    ("22", 9, 0),
    ("78", 9, 0),
    ("89", 9, 0),
    ("502", 9, 9),
    ("601", 9, 9),
    ("918", 9, 9),
];

#[test]
fn the_corpus_matches_the_recorded_expectations() {
    let root = workspace_root().join("fixtures/vuln");
    let index = RuleIndex::build(&test_rules(), &Gate::default());
    let mut found: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    let mut problems: Vec<String> = Vec::new();
    let mut cwes: Vec<_> = std::fs::read_dir(&root).unwrap().flatten().collect();
    cwes.sort_by_key(std::fs::DirEntry::file_name);
    for cwe_dir in cwes {
        let name = cwe_dir.file_name().to_string_lossy().into_owned();
        let Some(number) = name.strip_prefix("cwe-") else {
            continue;
        };
        let mut cases: Vec<_> = std::fs::read_dir(cwe_dir.path())
            .unwrap()
            .flatten()
            .collect();
        cases.sort_by_key(std::fs::DirEntry::file_name);
        for case in cases {
            let id = case.file_name().to_string_lossy().into_owned();
            let mut files = Vec::new();
            php_files(&case.path(), "", &mut files);
            let refs: Vec<(&str, &str)> = files
                .iter()
                .map(|(p, t)| (p.as_str(), t.as_str()))
                .collect();
            let project = Project::load(&MagoBackend::new(), &refs);
            let rule = rule_of(number);
            let flows: Vec<String> = project
                .project_flows(&index)
                .into_iter()
                .filter(|f| f.starts_with(rule))
                .collect();
            let tally = found.entry(number.to_string()).or_default();
            if id.ends_with("-safe") {
                if !flows.is_empty() {
                    tally.1 += 1;
                }
            } else if flows.is_empty() {
                problems.push(format!("{id}: no flow found"));
            } else {
                tally.0 += 1;
            }
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
    for (cwe, tp, fp) in EXPECTED {
        assert_eq!(found.get(cwe), Some(&(tp, fp)), "CWE-{cwe}");
    }
}
