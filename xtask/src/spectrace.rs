//! `cargo xtask spec-trace`: an advisory report of which specified requirements have a marked test
//! (docs/testing.md, spec `testing-strategy`). Never fails the build.
//!
//! A test covers a requirement by carrying the doc line `/// spec: <capability>/<Requirement title>`.
//! Requirements are the `### Requirement:` headings of `openspec/specs/*/spec.md`; the scenarios under each
//! are listed for the ones nothing covers. Titles of requirements still pending in an active change are
//! known (their markers are not stale) but are reported apart. A requirement verified by hand is declared by
//! a line `manual: <capability>/<Requirement title> - <reason and procedure>` in a change's tasks.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use proc_macro2::{TokenStream, TokenTree};
use quote::ToTokens;
use serde_json::{Value, json};
use syn::{Attribute, Expr, ExprLit, Item, Lit, Meta};

const MARKER: &str = "spec:";
const ROOTS: [&str; 3] = ["crates", "adapters", "xtask"];

/// A requirement and the scenarios under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    pub capability: String,
    pub title: String,
    pub scenarios: Vec<String>,
    /// Defined only in an active change, not yet in the main specifications.
    pub pending: bool,
}

/// Requirements of one specification text.
pub fn parse_spec(capability: &str, text: &str, pending: bool) -> Vec<Requirement> {
    let mut out: Vec<Requirement> = Vec::new();
    for line in text.lines() {
        if let Some(title) = line.strip_prefix("### Requirement:") {
            out.push(Requirement {
                capability: capability.to_string(),
                title: title.trim().to_string(),
                scenarios: Vec::new(),
                pending,
            });
        } else if let Some(name) = line.strip_prefix("#### Scenario:")
            && let Some(last) = out.last_mut()
        {
            last.scenarios.push(name.trim().to_string());
        }
    }
    out
}

/// A test carrying a marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marked {
    pub test: String,
    pub capability: String,
    pub title: String,
    /// The test exercises generated inputs (`check(..)` or `proptest!`), not an example.
    pub property: bool,
}

fn doc_lines(attrs: &[Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter(|a| a.path().is_ident("doc"))
        .filter_map(|a| match &a.meta {
            Meta::NameValue(nv) => match &nv.value {
                Expr::Lit(ExprLit {
                    lit: Lit::Str(s), ..
                }) => Some(s.value()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn mentions_property(stream: TokenStream) -> bool {
    stream.into_iter().any(|tree| match tree {
        TokenTree::Ident(i) => matches!(
            i.to_string().as_str(),
            "check" | "check_with" | "proptest" | "prop_assert" | "prop_assert_eq"
        ),
        TokenTree::Group(g) => mentions_property(g.stream()),
        _ => false,
    })
}

fn walk(items: &[Item], file: &str, out: &mut Vec<Marked>) {
    for item in items {
        match item {
            Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    walk(inner, file, out);
                }
            }
            Item::Fn(f) if f.attrs.iter().any(|a| a.path().is_ident("test")) => {
                let property = mentions_property(f.block.to_token_stream());
                for line in doc_lines(&f.attrs) {
                    if let Some((capability, title)) = line
                        .trim()
                        .strip_prefix(MARKER)
                        .and_then(|r| r.trim().split_once('/'))
                    {
                        out.push(Marked {
                            test: format!("{file}::{}", f.sig.ident),
                            capability: capability.trim().to_string(),
                            title: title.trim().to_string(),
                            property,
                        });
                    }
                }
            }
            _ => {}
        }
    }
}

/// The markers of one Rust source.
pub fn parse_markers(file: &str, source: &str) -> Vec<Marked> {
    let mut out = Vec::new();
    if let Ok(parsed) = syn::parse_file(source) {
        walk(&parsed.items, file, &mut out);
    }
    out
}

/// A requirement declared as verified by hand: `manual: <capability>/<title> - <note>`.
pub fn parse_manual(text: &str) -> Vec<(String, String, String)> {
    text.lines()
        .filter_map(|line| {
            let rest = line.split_once("manual:")?.1.trim();
            let (key, note) = rest.split_once(" - ").unwrap_or((rest, ""));
            let (capability, title) = key.trim().trim_matches('`').split_once('/')?;
            Some((
                capability.trim().to_string(),
                title.trim().to_string(),
                note.trim().to_string(),
            ))
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Covered,
    PropertyOnly,
    Manual,
    Uncovered,
}

impl Status {
    fn word(self) -> &'static str {
        match self {
            Self::Covered => "covered",
            Self::PropertyOnly => "property-only",
            Self::Manual => "manual",
            Self::Uncovered => "uncovered",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub requirement: Requirement,
    pub status: Status,
    pub tests: Vec<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Trace {
    pub rows: Vec<Row>,
    /// Markers naming a requirement that exists nowhere.
    pub stale: Vec<Marked>,
}

pub fn trace(
    requirements: &[Requirement],
    markers: &[Marked],
    manual: &[(String, String, String)],
) -> Trace {
    let known = |cap: &str, title: &str| {
        requirements
            .iter()
            .any(|r| r.capability == cap && r.title == title)
    };
    let mut rows = Vec::new();
    for requirement in requirements {
        let hits: Vec<&Marked> = markers
            .iter()
            .filter(|m| m.capability == requirement.capability && m.title == requirement.title)
            .collect();
        let verified_by_hand = manual
            .iter()
            .any(|(c, t, _)| *c == requirement.capability && *t == requirement.title);
        let status = if hits.iter().any(|m| !m.property) {
            Status::Covered
        } else if !hits.is_empty() {
            Status::PropertyOnly
        } else if verified_by_hand {
            Status::Manual
        } else {
            Status::Uncovered
        };
        let mut tests: Vec<String> = hits.iter().map(|m| m.test.clone()).collect();
        tests.sort();
        rows.push(Row {
            requirement: requirement.clone(),
            status,
            tests,
        });
    }
    rows.sort_by(|a, b| {
        (&a.requirement.capability, &a.requirement.title)
            .cmp(&(&b.requirement.capability, &b.requirement.title))
    });
    let mut stale: Vec<Marked> = markers
        .iter()
        .filter(|m| !known(&m.capability, &m.title))
        .cloned()
        .collect();
    stale.sort_by(|a, b| a.test.cmp(&b.test));
    Trace { rows, stale }
}

pub fn to_json(trace: &Trace) -> Value {
    let mut capabilities: BTreeMap<&str, Vec<Value>> = BTreeMap::new();
    for row in &trace.rows {
        capabilities
            .entry(&row.requirement.capability)
            .or_default()
            .push(json!({
                "requirement": row.requirement.title,
                "status": row.status.word(),
                "pending": row.requirement.pending,
                "scenarios": row.requirement.scenarios,
                "tests": row.tests,
            }));
    }
    let count = |s: Status| {
        trace
            .rows
            .iter()
            .filter(|r| !r.requirement.pending && r.status == s)
            .count()
    };
    json!({
        "schema": 1,
        "summary": {
            "covered": count(Status::Covered),
            "property_only": count(Status::PropertyOnly),
            "manual": count(Status::Manual),
            "uncovered": count(Status::Uncovered),
        },
        "capabilities": capabilities,
        "stale_markers": trace.stale.iter().map(|m| json!({"test": m.test, "marker": format!("{}/{}", m.capability, m.title)})).collect::<Vec<_>>(),
    })
}

pub fn to_text(trace: &Trace) -> String {
    let main: Vec<&Row> = trace
        .rows
        .iter()
        .filter(|r| !r.requirement.pending)
        .collect();
    let count = |s: Status| main.iter().filter(|r| r.status == s).count();
    let mut out = format!(
        "Spec traceability (advisory): {} requirements of the main specifications: {} covered, {} property-only, {} manual, {} without a marked test\n",
        main.len(),
        count(Status::Covered),
        count(Status::PropertyOnly),
        count(Status::Manual),
        count(Status::Uncovered)
    );
    let mut current = "";
    for row in main.iter().filter(|r| {
        matches!(
            r.status,
            Status::Uncovered | Status::PropertyOnly | Status::Manual
        )
    }) {
        if row.requirement.capability != current {
            current = &row.requirement.capability;
            out.push_str(&format!("\n{current}\n"));
        }
        match row.status {
            Status::Uncovered => {
                out.push_str(&format!("  no marked test: {}\n", row.requirement.title));
                row.requirement
                    .scenarios
                    .iter()
                    .for_each(|s| out.push_str(&format!("    scenario: {s}\n")));
            }
            Status::PropertyOnly => out.push_str(&format!(
                "  only properties, no example test: {} ({})\n",
                row.requirement.title,
                row.tests.join(", ")
            )),
            _ => out.push_str(&format!("  manual: {}\n", row.requirement.title)),
        }
    }
    let pending: Vec<&Row> = trace
        .rows
        .iter()
        .filter(|r| r.requirement.pending && !r.tests.is_empty())
        .collect();
    if !pending.is_empty() {
        out.push_str("\nMarked tests for requirements still pending in an active change:\n");
        pending.iter().for_each(|r| {
            out.push_str(&format!(
                "  {}/{}: {}\n",
                r.requirement.capability,
                r.requirement.title,
                r.tests.join(", ")
            ))
        });
    }
    if trace.stale.is_empty() {
        out.push_str("\nStale markers: none\n");
    } else {
        out.push_str("\nStale markers (the requirement does not exist):\n");
        trace.stale.iter().for_each(|m| {
            out.push_str(&format!(
                "  {}: spec: {}/{}\n",
                m.test, m.capability, m.title
            ))
        });
    }
    out
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = read.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            if entry.file_name() != "target" {
                rust_files(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn spec_dirs(root: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    if let Ok(read) = fs::read_dir(root) {
        for entry in read.filter_map(Result::ok) {
            let spec = entry.path().join("spec.md");
            if spec.is_file() {
                out.push((entry.file_name().to_string_lossy().into_owned(), spec));
            }
        }
    }
    out.sort();
    out
}

pub const HELP: &str = "\
usage: cargo xtask spec-trace [--json FILE]

Reports which requirements of openspec/specs have a test marked `/// spec: <capability>/<Requirement>`,
which are covered only by properties, which are verified by hand, which markers are stale. Advisory: the exit
status is always 0. Writes target/spec-trace/report.json and report.txt.";

pub fn run(root: &Path, args: &[String]) -> Result<Vec<String>, String> {
    let mut requirements = Vec::new();
    for (capability, path) in spec_dirs(&root.join("openspec/specs")) {
        requirements.extend(parse_spec(
            &capability,
            &fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?,
            false,
        ));
    }
    let mut manual = Vec::new();
    if let Ok(read) = fs::read_dir(root.join("openspec/changes")) {
        for change in read.filter_map(Result::ok) {
            let path = change.path();
            if path.file_name().is_some_and(|n| n == "archive") {
                continue;
            }
            for (capability, spec) in spec_dirs(&path.join("specs")) {
                let text =
                    fs::read_to_string(&spec).map_err(|e| format!("{}: {e}", spec.display()))?;
                for req in parse_spec(&capability, &text, true) {
                    if !requirements.iter().any(|r: &Requirement| {
                        r.capability == req.capability && r.title == req.title
                    }) {
                        requirements.push(req);
                    }
                }
            }
            if let Ok(tasks) = fs::read_to_string(path.join("tasks.md")) {
                manual.extend(parse_manual(&tasks));
            }
        }
    }
    let mut files = Vec::new();
    for dir in ROOTS {
        rust_files(&root.join(dir), &mut files);
    }
    let mut markers = Vec::new();
    for file in files {
        let label = file
            .strip_prefix(root)
            .unwrap_or(&file)
            .display()
            .to_string();
        if let Ok(source) = fs::read_to_string(&file) {
            markers.extend(parse_markers(&label, &source));
        }
    }
    let result = trace(&requirements, &markers, &manual);
    let out = root.join("target/spec-trace");
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&to_json(&result)).unwrap_or_default() + "\n";
    let text = to_text(&result);
    fs::write(out.join("report.json"), &json).map_err(|e| e.to_string())?;
    fs::write(out.join("report.txt"), &text).map_err(|e| e.to_string())?;
    if let Some(i) = args.iter().position(|a| a == "--json")
        && let Some(path) = args.get(i + 1)
    {
        fs::write(path, &json).map_err(|e| format!("{path}: {e}"))?;
    }
    print!("{text}");
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = "## Purpose\n\n### Requirement: Alpha\n\n#### Scenario: one\n#### Scenario: two\n\n### Requirement: Beta\n\n#### Scenario: three\n\n### Requirement: Gamma\n";

    const SOURCE: &str = r#"
/// spec: cap/Alpha
#[test]
fn example() { assert_eq!(1, 1); }

/// spec: cap/Beta
#[test]
fn generated() { check(strategy(), |v| Ok(())); }

/// spec: cap/Removed
#[test]
fn old() {}

#[test]
fn unmarked() {}
"#;

    fn markers() -> Vec<Marked> {
        parse_markers("t.rs", SOURCE)
    }

    #[test]
    fn requirements_and_their_scenarios_are_read_from_a_spec() {
        let reqs = parse_spec("cap", SPEC, false);
        assert_eq!(reqs.len(), 3);
        assert_eq!(reqs[0].scenarios, vec!["one", "two"]);
        assert!(reqs[2].scenarios.is_empty());
    }

    #[test]
    fn markers_are_read_from_doc_lines_and_properties_are_recognised() {
        let m = markers();
        assert_eq!(m.len(), 3);
        assert!(!m[0].property && m[0].test == "t.rs::example");
        assert!(m[1].property);
    }

    #[test]
    fn the_report_lists_uncovered_property_only_and_stale() {
        let reqs = parse_spec("cap", SPEC, false);
        let result = trace(&reqs, &markers(), &[]);
        let status: Vec<Status> = result.rows.iter().map(|r| r.status).collect();
        assert_eq!(
            status,
            vec![Status::Covered, Status::PropertyOnly, Status::Uncovered]
        );
        assert_eq!(result.stale.len(), 1);
        assert_eq!(result.stale[0].test, "t.rs::old");
        let text = to_text(&result);
        assert!(text.contains("no marked test: Gamma"), "{text}");
        assert!(
            text.contains("only properties, no example test: Beta"),
            "{text}"
        );
        assert!(text.contains("t.rs::old: spec: cap/Removed"), "{text}");
        let json = to_json(&result);
        assert_eq!(json["summary"]["uncovered"], 1);
        assert_eq!(json["stale_markers"][0]["marker"], "cap/Removed");
    }

    #[test]
    fn uncovered_scenarios_are_listed_under_their_capability() {
        let reqs = parse_spec(
            "cap",
            "### Requirement: Delta\n#### Scenario: s1\n#### Scenario: s2\n",
            false,
        );
        let text = to_text(&trace(&reqs, &[], &[]));
        assert!(
            text.contains("cap\n  no marked test: Delta\n    scenario: s1\n    scenario: s2"),
            "{text}"
        );
    }

    #[test]
    fn a_manual_declaration_makes_the_requirement_manual() {
        let manual = parse_manual(
            "- [ ] 9.9 manual: cap/Gamma - verified by running the binary on a Windows host\n",
        );
        assert_eq!(manual[0].0, "cap");
        let result = trace(&parse_spec("cap", SPEC, false), &markers(), &manual);
        assert_eq!(result.rows[2].status, Status::Manual);
        assert!(to_text(&result).contains("manual: Gamma"));
    }

    #[test]
    fn pending_requirements_are_known_but_reported_apart() {
        let mut reqs = parse_spec("cap", SPEC, false);
        reqs.extend(parse_spec("cap", "### Requirement: Removed\n", true));
        let result = trace(&reqs, &markers(), &[]);
        assert!(result.stale.is_empty());
        let text = to_text(&result);
        assert!(text.contains("still pending in an active change"), "{text}");
        assert!(
            text.contains("3 requirements of the main specifications"),
            "{text}"
        );
    }
}
