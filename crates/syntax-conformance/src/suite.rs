//! The suite assertions: what a backend must do with each fixture, run through the port only.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use bleedthrough_syntax::{
    LineIndex, MemberKind, NodeRef, ParseOptions, ParseQuality, ParsedFile, ParserBackend, Span,
    StmtKind, check_invariants, visit_tree,
};

use crate::corpus::{Corpus, Expectation, Fixture, Group, Verdict};
use crate::dump::dump_tree;

/// One failed assertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// The fixture, a generated variant such as `path#prefix=12`, or `-` for a corpus-wide check.
    pub subject: String,
    /// Which check failed.
    pub check: &'static str,
    /// What was wrong.
    pub message: String,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}: {}", self.check, self.subject, self.message)
    }
}

pub(crate) fn failure(subject: &str, check: &'static str, message: impl Into<String>) -> Failure {
    Failure {
        subject: subject.to_string(),
        check,
        message: message.into(),
    }
}

/// How the suite behaves.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SuiteOptions {
    /// Where the golden dumps live, mirroring the fixture paths with a `.txt` suffix.
    pub golden_dir: PathBuf,
    /// Rewrite the golden files instead of comparing; set from `BLESS=1` by callers.
    pub bless: bool,
    /// Options for the plain parse of every fixture.
    pub parse: ParseOptions,
}

impl SuiteOptions {
    /// Options comparing against the goldens in `golden_dir`.
    pub fn new(golden_dir: impl Into<PathBuf>) -> Self {
        Self {
            golden_dir: golden_dir.into(),
            bless: false,
            parse: ParseOptions::default(),
        }
    }
}

/// The outcome of a suite run.
#[derive(Debug, Clone, Default)]
pub struct SuiteReport {
    /// Failed assertions.
    pub failures: Vec<Failure>,
    /// PHP versions whose ledger verdicts are not interpreter-verified, with the reason.
    pub unverified: Vec<(String, String)>,
    /// How many fixtures were run.
    pub fixtures: usize,
}

fn declared_names(tree: &bleedthrough_syntax::SyntaxTree) -> Vec<String> {
    let mut names = Vec::new();
    visit_tree(tree, &mut |node| match node {
        NodeRef::Stmt(s) => match &s.kind {
            StmtKind::Function(f) => names.push(f.name.to_string()),
            StmtKind::ClassLike(c) => names.extend(c.name.iter().map(|n| n.to_string())),
            _ => {}
        },
        NodeRef::Member(m) => {
            if let MemberKind::Method(f) = &m.kind {
                names.push(f.name.to_string());
            }
        }
        _ => {}
    });
    names
}

fn error_spans(tree: &bleedthrough_syntax::SyntaxTree) -> Vec<Span> {
    let mut spans = Vec::new();
    visit_tree(tree, &mut |node| {
        if node.is_error() {
            spans.push(node.span());
        }
    });
    spans
}

fn check_clean(fixture: &Fixture, parsed: &ParsedFile, out: &mut Vec<Failure>) {
    let (path, unsupported, errors) = (
        &fixture.path,
        parsed.unsupported_count(),
        parsed.error_count(),
    );
    if parsed.quality() != ParseQuality::Clean || unsupported > 0 || errors > 0 {
        let first = parsed.diagnostics().first().map_or_else(
            || "no diagnostic".to_string(),
            |d| format!("{} at {}..{}", d.code.as_str(), d.span.start, d.span.end),
        );
        out.push(failure(
            path,
            "valid-parses-clean",
            format!(
                "quality {}, {} diagnostics, {unsupported} unsupported, {errors} error nodes (first: {first})",
                parsed.quality().as_str(),
                parsed.diagnostics().len()
            ),
        ));
    }
}

fn check_malformed(
    fixture: &Fixture,
    e: &Expectation,
    bytes: &[u8],
    parsed: &ParsedFile,
    out: &mut Vec<Failure>,
) {
    let path = &fixture.path;
    if parsed.quality() == ParseQuality::Clean {
        out.push(failure(
            path,
            "malformed-reported",
            "the malformed fixture parsed clean",
        ));
        return;
    }
    let reported: Vec<&str> = parsed
        .diagnostics()
        .iter()
        .map(|d| d.code.as_str())
        .collect();
    if !e.codes.iter().any(|c| reported.contains(&c.as_str())) {
        out.push(failure(
            path,
            "malformed-code",
            format!("expected one of {:?}, reported {reported:?}", e.codes),
        ));
    }
    let index = LineIndex::new(bytes);
    let overlaps = |s: Span| {
        let first = index.line_column(s.start).line;
        let last = index.line_column(s.end.saturating_sub(1).max(s.start)).line;
        first <= e.lines.1 && e.lines.0 <= last
    };
    if !error_spans(parsed.tree()).into_iter().any(overlaps) {
        out.push(failure(
            path,
            "malformed-error-node",
            format!("no error node covers lines {}..{}", e.lines.0, e.lines.1),
        ));
    }
    let present = declared_names(parsed.tree());
    for name in &e.survivors {
        if !present.iter().any(|n| n == name) {
            out.push(failure(
                path,
                "malformed-survivor",
                format!("declaration `{name}` is missing from the tree"),
            ));
        }
    }
}

fn check_ledger(fixture: &Fixture, corpus: &Corpus, parsed: &ParsedFile, out: &mut Vec<Failure>) {
    let Some(row) = corpus.ledger.verdicts.get(&fixture.path) else {
        return;
    };
    let verified: Vec<Verdict> = row
        .iter()
        .filter(|(v, _)| corpus.ledger.versions.get(*v).is_some_and(|i| i.verified))
        .map(|(_, verdict)| *verdict)
        .collect();
    if verified.is_empty() {
        return;
    }
    let accepts = parsed.quality() == ParseQuality::Clean;
    let any_valid = verified.contains(&Verdict::Valid);
    if !any_valid && accepts {
        out.push(failure(
            &fixture.path,
            "ledger-agreement",
            "the backend accepts a fixture that every verified interpreter rejects",
        ));
    }
    if any_valid && !accepts && fixture.group != Group::Malformed {
        out.push(failure(
            &fixture.path,
            "ledger-agreement",
            "the backend rejects syntax that a verified interpreter accepts",
        ));
    }
}

fn golden_path(options: &SuiteOptions, fixture: &Fixture) -> PathBuf {
    options.golden_dir.join(format!("{}.txt", fixture.path))
}

fn check_golden(fixture: &Fixture, dump: &str, options: &SuiteOptions, out: &mut Vec<Failure>) {
    let path = golden_path(options, fixture);
    if options.bless {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&path, dump) {
            out.push(failure(
                &fixture.path,
                "golden",
                format!("cannot write {}: {e}", path.display()),
            ));
        }
        return;
    }
    match std::fs::read_to_string(&path) {
        Err(_) => out.push(failure(
            &fixture.path,
            "golden",
            format!("no golden file {} (run with BLESS=1)", path.display()),
        )),
        Ok(expected) if expected != dump => {
            out.push(failure(
                &fixture.path,
                "golden",
                first_difference(&expected, dump),
            ));
        }
        Ok(_) => {}
    }
}

/// Describes where two dumps first differ.
pub(crate) fn first_difference(expected: &str, actual: &str) -> String {
    for (number, (a, b)) in expected.lines().zip(actual.lines()).enumerate() {
        if a != b {
            return format!("line {}: expected `{a}`, found `{b}`", number + 1);
        }
    }
    format!(
        "expected {} lines, found {}",
        expected.lines().count(),
        actual.lines().count()
    )
}

/// Runs every per-fixture assertion for one fixture.
pub fn run_fixture(
    backend: &dyn ParserBackend,
    corpus: &Corpus,
    fixture: &Fixture,
    options: &SuiteOptions,
) -> Vec<Failure> {
    let mut out = Vec::new();
    let bytes = match corpus.read(fixture) {
        Ok(b) => b,
        Err(e) => return vec![failure(&fixture.path, "read", e)],
    };
    let attempt = catch_unwind(AssertUnwindSafe(|| {
        backend.parse(&fixture.path, &bytes, &options.parse)
    }));
    let Ok(parsed) = attempt else {
        return vec![failure(
            &fixture.path,
            "resilience",
            "the backend panicked instead of returning a result",
        )];
    };
    if parsed.quality() != ParseQuality::Failed {
        for v in check_invariants(parsed.tree(), &bytes) {
            out.push(failure(
                &fixture.path,
                "invariants",
                format!("{:?}: {}", v.kind, v.message),
            ));
        }
    }
    match (fixture.group, &fixture.expect) {
        (Group::Malformed, Some(e)) => check_malformed(fixture, e, &bytes, &parsed, &mut out),
        (Group::Malformed, None) => out.push(failure(
            &fixture.path,
            "malformed-expectation",
            "no expectation declared",
        )),
        (Group::Valid | Group::Legacy | Group::Realistic, _) => {
            check_clean(fixture, &parsed, &mut out)
        }
    }
    check_ledger(fixture, corpus, &parsed, &mut out);
    check_golden(fixture, &dump_tree(parsed.tree()), options, &mut out);
    out
}

/// Runs the per-fixture assertions over the whole corpus.
pub fn run_suite(
    backend: &dyn ParserBackend,
    corpus: &Corpus,
    options: &SuiteOptions,
) -> SuiteReport {
    let mut report = SuiteReport::default();
    for problem in corpus.check() {
        report.failures.push(failure("-", "corpus", problem));
    }
    for fixture in &corpus.fixtures {
        report
            .failures
            .extend(run_fixture(backend, corpus, fixture, options));
        report.fixtures += 1;
    }
    for (version, info) in &corpus.ledger.versions {
        if !info.verified {
            report
                .unverified
                .push((version.clone(), info.reason.clone()));
        }
    }
    report
}

/// The goldens directory as a plain path, for callers that only have a corpus root.
pub fn default_golden_dir(crate_dir: &Path) -> PathBuf {
    crate_dir.join("goldens")
}
