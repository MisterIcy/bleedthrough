//! Loading TOML rule files: errors with file, line and key, determinism and the content hash.

use std::fs;
use std::path::Path;

use bleedthrough_fs::{CheckStatus, check_rules, load_rules};

fn taint_rule(id: &str, title: &str) -> String {
    format!(
        r#"[[rule]]
id = "{id}"
title = "{title}"
description = "Tainted data reaches an SQL query."
remediation = "Use prepared statements."
cwe = ["CWE-89"]
owasp = ["OWASP:2021:A03"]

[rule.metrics]
attack_vector = "network"
attack_complexity = "low"
privileges_required = "none"
user_interaction = "none"
scope = "unchanged"
confidentiality = "high"
integrity = "high"
availability = "high"

[rule.taint]
sources = ["$_GET", "$_POST"]

[[rule.taint.sinks]]
symbol = "mysqli_query"
args = [2]
class = "sql"
"#
    )
}

fn file(dir: &Path, name: &str, body: &str) {
    let path = dir.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, format!("schema = 1\n\n{body}")).unwrap();
}

fn errors(dir: &Path) -> Vec<String> {
    load_rules(dir)
        .unwrap_err()
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn a_valid_directory_loads_in_identifier_order() {
    let d = tempfile::tempdir().unwrap();
    file(d.path(), "b/two.toml", &taint_rule("BT0002", "Two"));
    file(d.path(), "a/one.toml", &taint_rule("BT0001", "One"));
    let loaded = load_rules(d.path()).unwrap();
    let ids: Vec<_> = loaded
        .set
        .iter()
        .map(|r| r.id().as_str().to_string())
        .collect();
    assert_eq!(ids, ["BT0001", "BT0002"]);
    assert_eq!(loaded.hash.len(), 64);
    let rule = loaded.set.iter().next().unwrap();
    assert_eq!(rule.origin().unwrap().file, "a/one.toml");
    assert_eq!(rule.origin().unwrap().line, 3);
    assert_eq!(rule.severity().score_text().unwrap(), "9.8");
}

#[test]
fn a_misspelt_key_reports_file_line_and_key() {
    let d = tempfile::tempdir().unwrap();
    let body = taint_rule("BT0001", "One").replace("remediation =", "remedation =");
    file(d.path(), "r.toml", &body);
    let e = errors(d.path());
    assert!(
        e.iter()
            .any(|m| m.starts_with("r.toml:7: remedation: unknown key `remedation`")),
        "{e:?}"
    );
    assert!(
        e.iter()
            .any(|m| m.contains("remediation") && m.contains("is required")),
        "{e:?}"
    );
}

#[test]
fn every_error_in_a_file_is_reported() {
    let d = tempfile::tempdir().unwrap();
    let body = taint_rule("BT0001", "One")
        .replace("title =", "titel =")
        .replace(r#"cwe = ["CWE-89"]"#, "cwe = 89")
        .replace(r#"class = "sql""#, r#"class = "xss""#);
    file(d.path(), "r.toml", &body);
    let e = errors(d.path());
    let located = e.iter().filter(|m| m.starts_with("r.toml:")).count();
    assert!(located >= 3, "{e:?}");
    assert!(e.iter().any(|m| m.contains("titel")));
    assert!(e.iter().any(|m| m.contains("`cwe` must be an array")));
    assert!(
        e.iter()
            .any(|m| m.contains("\"xss\"") && m.contains("sinks[0].class"))
    );
}

#[test]
fn a_file_with_three_errors_reports_three() {
    let d = tempfile::tempdir().unwrap();
    let body = taint_rule("BT0001", "One")
        .replace(
            "[rule.metrics]",
            "unexpected = 1\nparanoid = \"yes\"\n\n[rule.metrics]",
        )
        .replace(r#"scope = "unchanged""#, "scope = 3");
    file(d.path(), "r.toml", &body);
    let e = errors(d.path());
    assert_eq!(e.len(), 3, "{e:?}");
    assert!(e.iter().all(|m| m.starts_with("r.toml:")));
}

#[test]
fn wrong_types_and_a_declared_score_are_rejected() {
    let d = tempfile::tempdir().unwrap();
    let body =
        taint_rule("BT0001", "One").replace(r#"scope = "unchanged""#, "scope = 3\nscore = 9.8");
    file(d.path(), "r.toml", &body);
    let e = errors(d.path());
    assert!(
        e.iter()
            .any(|m| m.contains("`scope` must be a string, found integer")),
        "{e:?}"
    );
    assert!(e.iter().any(|m| m.contains("unknown key `score`")), "{e:?}");
}

#[test]
fn a_newer_schema_names_both_versions() {
    let d = tempfile::tempdir().unwrap();
    fs::write(d.path().join("r.toml"), "schema = 2\n").unwrap();
    let e = errors(d.path());
    assert_eq!(e.len(), 1);
    assert!(
        e[0].contains("schema 2") && e[0].contains("supports schema 1"),
        "{e:?}"
    );
    fs::write(d.path().join("r.toml"), "[[rule]]\n").unwrap();
    assert!(errors(d.path())[0].contains("schema"));
}

#[test]
fn a_syntax_error_is_located() {
    let d = tempfile::tempdir().unwrap();
    fs::write(d.path().join("r.toml"), "schema = 1\n[[rule]\nid = \n").unwrap();
    let e = errors(d.path());
    assert!(e[0].starts_with("r.toml:2"), "{e:?}");
}

#[test]
fn reordered_files_give_the_same_set_and_hash() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    file(a.path(), "a.toml", &taint_rule("BT0001", "One"));
    file(a.path(), "b.toml", &taint_rule("BT0002", "Two"));
    file(b.path(), "z/first.toml", &taint_rule("BT0002", "Two"));
    file(b.path(), "m.toml", &taint_rule("BT0001", "One"));
    let (la, lb) = (load_rules(a.path()).unwrap(), load_rules(b.path()).unwrap());
    assert_eq!(la.hash, lb.hash);
    let ids = |l: &bleedthrough_fs::LoadedRules| {
        l.set.iter().map(|r| r.id().to_string()).collect::<Vec<_>>()
    };
    assert_eq!(ids(&la), ids(&lb));
}

#[test]
fn a_comment_edit_keeps_the_hash_and_a_content_edit_changes_it() {
    let d = tempfile::tempdir().unwrap();
    let body = taint_rule("BT0001", "One");
    file(d.path(), "r.toml", &body);
    let before = load_rules(d.path()).unwrap().hash;
    file(
        d.path(),
        "r.toml",
        &format!(
            "# a new comment\n{}",
            body.replace("[rule.metrics]", "[rule.metrics] # inline")
        ),
    );
    assert_eq!(load_rules(d.path()).unwrap().hash, before);
    file(d.path(), "r.toml", &taint_rule("BT0001", "Renamed"));
    assert_ne!(load_rules(d.path()).unwrap().hash, before);
}

#[test]
fn duplicate_and_retired_identifiers_fail() {
    let d = tempfile::tempdir().unwrap();
    file(d.path(), "a.toml", &taint_rule("BT0001", "One"));
    file(d.path(), "b.toml", &taint_rule("BT0001", "Again"));
    let e = errors(d.path());
    assert!(
        e.iter()
            .any(|m| m.contains("a.toml:3") && m.contains("b.toml:3")),
        "{e:?}"
    );

    let r = tempfile::tempdir().unwrap();
    file(r.path(), "a.toml", &taint_rule("BT0007", "Seven"));
    fs::write(
        r.path().join("RETIRED.md"),
        "# Retired\n\n- `BT0007` removed in 0.2\n",
    )
    .unwrap();
    assert!(
        errors(r.path())
            .iter()
            .any(|m| m.contains("BT0007") && m.contains("retired"))
    );
}

#[test]
fn rules_check_exits_with_the_tool_error_status_for_invalid_rules() {
    let d = tempfile::tempdir().unwrap();
    file(
        d.path(),
        "r.toml",
        &taint_rule("BT0001", "One").replace("remediation =", "remedation ="),
    );
    let mut out = Vec::new();
    assert_eq!(
        check_rules(d.path(), &mut out).unwrap(),
        CheckStatus::ToolError
    );
    assert!(
        String::from_utf8(out)
            .unwrap()
            .contains("r.toml:7: remedation:")
    );

    let ok = tempfile::tempdir().unwrap();
    file(ok.path(), "r.toml", &taint_rule("BT0001", "One"));
    let mut out = Vec::new();
    assert_eq!(
        check_rules(ok.path(), &mut out).unwrap(),
        CheckStatus::Valid
    );
    assert!(
        String::from_utf8(out)
            .unwrap()
            .starts_with("1 rule(s) valid")
    );
}

/// The worked examples in `docs/rule-schema.md` are loaded by the real loader, so they cannot rot.
fn doc_examples() -> Vec<(String, String)> {
    let doc =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/rule-schema.md"))
            .unwrap();
    let mut out = Vec::new();
    let mut current: Option<(String, String)> = None;
    for line in doc.lines() {
        match (&mut current, line) {
            (None, "```toml") => current = Some((String::new(), String::new())),
            (Some(_), "```") => out.extend(current.take()),
            (Some((name, body)), l) => match l.strip_prefix("# example: ") {
                Some(n) if name.is_empty() => *name = n.to_string(),
                _ => {
                    body.push_str(l);
                    body.push('\n');
                }
            },
            (None, _) => {}
        }
    }
    out
}

#[test]
fn the_documented_examples_load() {
    let examples = doc_examples();
    let names: Vec<_> = examples.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["taint-rule", "pattern-rule"]);
    for (name, body) in examples {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join(format!("{name}.toml")), &body).unwrap();
        let loaded = load_rules(d.path()).unwrap_or_else(|e| panic!("example {name}: {e:?}"));
        assert!(!loaded.set.is_empty(), "{name}");
    }
}
