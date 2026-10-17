//! Properties of the whole pipeline over generated programs: irrelevant edits change nothing,
//! sanitising never adds findings, adding a source never removes any, and mutated input never panics.

use std::collections::BTreeSet;

use bleedthrough_engine_suite::engine::{Gate, Limits, RuleIndex};
use bleedthrough_engine_suite::rules::RuleSet;
use bleedthrough_engine_suite::{Project, workspace_root};
use bleedthrough_syntax_mago::MagoBackend;
use bleedthrough_testkit::proptest::prelude::*;
use bleedthrough_testkit::{Settings, check_with, mutated, settings};

/// The engine's test rule sets, loaded from `crates/engine/tests/rules`.
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

/// Every case parses and analyses a program, so the default case count is capped.
fn bounded() -> Settings {
    let wanted = settings();
    Settings {
        cases: wanted.cases.min(24),
        ..wanted
    }
}

/// One statement of a generated program over the variables `$v0`..`$v3`.
#[derive(Debug, Clone)]
enum Stmt {
    Source(usize, usize),
    Literal(usize),
    Copy(usize, usize),
    Concat(usize, usize),
    Sanitise(usize, usize),
    Helper(usize, usize),
    Sink(usize),
}

fn stmt() -> impl Strategy<Value = Stmt> {
    prop_oneof![
        (0usize..4, 0usize..3).prop_map(|(v, s)| Stmt::Source(v, s)),
        (0usize..4).prop_map(Stmt::Literal),
        (0usize..4, 0usize..4).prop_map(|(a, b)| Stmt::Copy(a, b)),
        (0usize..4, 0usize..4).prop_map(|(a, b)| Stmt::Concat(a, b)),
        (0usize..4, 0usize..4).prop_map(|(a, b)| Stmt::Sanitise(a, b)),
        (0usize..4, 0usize..4).prop_map(|(a, b)| Stmt::Helper(a, b)),
        (0usize..4).prop_map(Stmt::Sink),
    ]
}

fn program() -> impl Strategy<Value = Vec<Stmt>> {
    prop::collection::vec(stmt(), 1..10)
}

const SOURCES: [&str; 3] = ["$_GET['a']", "$_POST['b']", "$_COOKIE['c']"];

fn render_stmt(s: &Stmt, literal_as_source: bool) -> String {
    match s {
        Stmt::Source(v, k) => format!("$v{v} = {};", SOURCES[*k]),
        Stmt::Literal(v) if literal_as_source => format!("$v{v} = $_REQUEST['added'];"),
        Stmt::Literal(v) => format!("$v{v} = 'text';"),
        Stmt::Copy(a, b) => format!("$v{a} = $v{b};"),
        Stmt::Concat(a, b) => format!("$v{a} = $v{b} . 'k';"),
        Stmt::Sanitise(a, b) => format!("$v{a} = mysqli_real_escape_string($l, $v{b});"),
        Stmt::Helper(a, b) => format!("$v{a} = helper($v{b});"),
        Stmt::Sink(v) => format!("mysqli_query($l, $v{v});"),
    }
}

const HEADER: &str =
    "<?php\nfunction helper($p) { return $p . '!'; }\n$v0 = ''; $v1 = ''; $v2 = ''; $v3 = '';\n";

fn render(stmts: &[Stmt]) -> String {
    let mut out = String::from(HEADER);
    for s in stmts {
        out.push_str(&render_stmt(s, false));
        out.push('\n');
    }
    out
}

/// The fingerprints of the findings of `php`.
fn fingerprints(php: &str) -> BTreeSet<String> {
    let rules = test_rules();
    let index = RuleIndex::build(&rules, &Gate::default());
    Project::load(&MagoBackend::new(), &[("a.php", php)])
        .findings(&rules, &index, Limits::default())
        .iter()
        .map(|f| f.fingerprint().as_str().to_string())
        .collect()
}

fn run(
    strategy: impl Strategy<Value = (Vec<Stmt>, usize)>,
    test: impl Fn((Vec<Stmt>, usize)) -> Result<(), TestCaseError>,
) {
    if let Err(message) = check_with(bounded(), Some(file!()), strategy, test) {
        panic!("{message}");
    }
}

/// spec: finding-production/Results are deterministic and independent of scheduling
#[test]
fn a_dead_assignment_or_an_unrelated_function_changes_nothing() {
    run((program(), 0usize..12), |(stmts, at)| {
        let base = fingerprints(&render(&stmts));
        let mut lines: Vec<String> = stmts.iter().map(|s| render_stmt(s, false)).collect();
        let at = at % (lines.len() + 1);
        lines.insert(at, "$dead = $dead2 . 'x';".to_string());
        lines.push("function unrelated($z) { return $z . $z; }".to_string());
        let edited = format!("{HEADER}{}\n", lines.join("\n"));
        prop_assert_eq!(fingerprints(&edited), base);
        Ok(())
    });
}

/// spec: finding-production/Results are deterministic and independent of scheduling
#[test]
fn sanitising_an_assignment_never_adds_findings() {
    run((program(), 0usize..12), |(stmts, at)| {
        let base = fingerprints(&render(&stmts));
        let candidates: Vec<usize> = stmts
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                matches!(
                    s,
                    Stmt::Source(..) | Stmt::Copy(..) | Stmt::Concat(..) | Stmt::Helper(..)
                )
            })
            .map(|(i, _)| i)
            .collect();
        prop_assume!(!candidates.is_empty());
        let target = candidates[at % candidates.len()];
        let lines: Vec<String> = stmts
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let line = render_stmt(s, false);
                if i != target {
                    return line;
                }
                let (var, rhs) = line.split_once(" = ").expect("an assignment");
                let rhs = rhs.trim_end_matches(';');
                format!("{var} = mysqli_real_escape_string($l, {rhs});")
            })
            .collect();
        let after = fingerprints(&format!("{HEADER}{}\n", lines.join("\n")));
        prop_assert!(after.is_subset(&base), "{after:?} not within {base:?}");
        Ok(())
    });
}

/// spec: finding-production/Results are deterministic and independent of scheduling
#[test]
fn adding_a_source_never_removes_a_finding() {
    run((program(), 0usize..12), |(stmts, _)| {
        let base = fingerprints(&render(&stmts));
        let mut out = String::from(HEADER);
        for s in &stmts {
            out.push_str(&render_stmt(s, true));
            out.push('\n');
        }
        let after = fingerprints(&out);
        prop_assert!(base.is_subset(&after), "{base:?} not within {after:?}");
        Ok(())
    });
}

/// spec: finding-production/Rule problems and internal faults do not abort analysis
#[test]
fn mutated_programs_never_panic() {
    let strategy = mutated(program().prop_map(|stmts| render(&stmts)));
    let result = check_with(bounded(), Some(file!()), strategy, |bytes| {
        let php = String::from_utf8_lossy(&bytes).into_owned();
        let rules = test_rules();
        let index = RuleIndex::build(&rules, &Gate::default());
        let project = Project::load(&MagoBackend::new(), &[("a.php", php.as_str())]);
        let _ = project.findings(&rules, &index, Limits::default());
        let (coverage, _) = project.coverage(&index, &rules, "8.3");
        prop_assert!(
            coverage.skipped.is_empty(),
            "an internal fault was swallowed: {:?}",
            coverage.skipped
        );
        Ok(())
    });
    if let Err(message) = result {
        panic!("{message}");
    }
}
