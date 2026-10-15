//! Tests of the conformance suite itself: stub backends that misbehave in one way each, and
//! deliberately broken temporary corpora. A suite that cannot fail proves nothing.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use bleedthrough_syntax::{
    BackendInfo, Diagnostic, DiagnosticCode, FunctionLike, Node, ParseOptions, ParsedFile,
    ParserBackend, Span, Stmt, StmtKind, SyntaxTree,
};
use bleedthrough_syntax_conformance::corpus::{VERSIONS, render_ledger};
use bleedthrough_syntax_conformance::mutate::{RobustnessOptions, run_robustness};
use bleedthrough_syntax_conformance::{
    Allowances, Corpus, Failure, Ledger, SuiteOptions, Verdict, VersionInfo, compare, run_suite,
};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A directory removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "bleedthrough-suite-self-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, relative: &str, bytes: &[u8]) {
        let target = self.0.join(relative);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, bytes).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const VALID: &[u8] = b"<?php\nfunction one() {}\n";
const BROKEN: &[u8] = b"<?php\nfunction ok_before() {}\nBROKEN line;\nfunction ok_after() {}\n";

fn ledger_text(paths: &[(&str, Verdict)], unverified: &[&str]) -> String {
    let mut ledger = Ledger {
        flags: "php -l".to_string(),
        ..Ledger::default()
    };
    for version in VERSIONS {
        let verified = !unverified.contains(&version);
        ledger.versions.insert(
            version.to_string(),
            VersionInfo {
                interpreter: if verified {
                    "php".into()
                } else {
                    String::new()
                },
                recorded: if verified {
                    "2026-01-01".into()
                } else {
                    String::new()
                },
                verified,
                reason: if verified {
                    String::new()
                } else {
                    "no image".into()
                },
            },
        );
        for (path, verdict) in paths {
            ledger
                .verdicts
                .entry((*path).to_string())
                .or_default()
                .insert(version.to_string(), *verdict);
        }
    }
    render_ledger(&ledger)
}

const MANIFEST: &str = r#"
breakage_classes = ["invalid-token"]

[[feature]]
id = "f1"
version = "7.0"
doc = "https://www.php.net/manual/en/x.php"
summary = "a feature"
fixtures = ["v7.0/one.php"]

[[fixture]]
path = "v7.0/one.php"
group = "valid"
provenance = "original"
licence = "MIT"

[[fixture]]
path = "malformed/broken.php"
group = "malformed"
provenance = "original"
licence = "MIT"

[fixture.expect]
class = "invalid-token"
codes = ["unexpected_token"]
lines = [3, 3]
survivors = ["ok_before", "ok_after"]
"#;

/// A complete, consistent two-fixture corpus.
fn good_corpus() -> TempDir {
    let dir = TempDir::new();
    dir.write("manifest.toml", MANIFEST.as_bytes());
    dir.write("v7.0/one.php", VALID);
    dir.write("malformed/broken.php", BROKEN);
    dir.write(
        "ledger.toml",
        ledger_text(
            &[
                ("v7.0/one.php", Verdict::Valid),
                ("malformed/broken.php", Verdict::Invalid),
            ],
            &[],
        )
        .as_bytes(),
    );
    dir
}

fn load(dir: &TempDir) -> Corpus {
    Corpus::load(dir.path()).expect("the corpus loads")
}

// ---- the stub backend ----

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fault {
    None,
    DropStatement,
    OverlapSiblings,
    ParseMalformedClean,
    LoseSurvivor,
    Panic,
    WrongSpanOnSecond,
}

struct Stub {
    fault: Fault,
    name: &'static str,
}

impl Stub {
    fn new(fault: Fault) -> Self {
        Self {
            fault,
            name: "stub",
        }
    }
}

fn function(name: &str, span: Span) -> Stmt {
    Node::new(
        span,
        StmtKind::Function(Box::new(FunctionLike {
            name: name.into(),
            attributes: vec![],
            modifiers: vec![],
            by_ref: false,
            params: vec![],
            return_type: None,
            body: Some(vec![]),
        })),
    )
}

/// A line-based mini parser: `<?php`, `function NAME() {}` lines, and `BROKEN` lines.
fn mini_parse(source: &[u8], fault: Fault) -> ParsedFile {
    let text = String::from_utf8_lossy(source);
    let mut statements = vec![Node::new(
        Span::new(0, 5),
        StmtKind::OpenTag { short: false },
    )];
    let mut diagnostics = Vec::new();
    let mut offset = 0u32;
    let mut seen_functions = 0;
    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += u32::try_from(line.len()).unwrap();
        let trimmed = line.trim_end();
        let span = Span::new(start, start + u32::try_from(trimmed.len()).unwrap());
        if let Some(rest) = trimmed.strip_prefix("function ") {
            seen_functions += 1;
            let name = rest.split('(').next().unwrap();
            let skip = (fault == Fault::DropStatement && seen_functions == 1)
                || (fault == Fault::LoseSurvivor && name == "ok_after");
            if fault == Fault::LoseSurvivor && name == "ok_after" {
                let diagnostic = diagnostics.len();
                statements.push(Node::new(
                    span,
                    StmtKind::Error {
                        diagnostic: Some(diagnostic),
                    },
                ));
                diagnostics.push(Diagnostic::new(DiagnosticCode::UnexpectedToken, "x", span));
            } else if !skip {
                let mut at = span;
                if fault == Fault::OverlapSiblings && seen_functions == 1 {
                    at.end += 3;
                }
                if fault == Fault::WrongSpanOnSecond && seen_functions == 2 {
                    at.start += 1;
                }
                statements.push(function(name, at));
            }
        } else if trimmed.starts_with("BROKEN") {
            if fault == Fault::ParseMalformedClean {
                continue_as_expression(&mut statements, span);
            } else {
                let diagnostic = diagnostics.len();
                diagnostics.push(Diagnostic::new(DiagnosticCode::UnexpectedToken, "x", span));
                statements.push(Node::new(
                    span,
                    StmtKind::Error {
                        diagnostic: Some(diagnostic),
                    },
                ));
            }
        }
    }
    let info = BackendInfo {
        name: "stub".into(),
        version: "1".into(),
    };
    ParsedFile::new(
        SyntaxTree::new(statements, vec![], offset),
        diagnostics,
        info,
    )
}

fn continue_as_expression(statements: &mut Vec<Stmt>, span: Span) {
    use bleedthrough_syntax::{ExprKind, Literal};
    let literal = Node::new(span, ExprKind::Literal(Literal::Int { raw: "1".into() }));
    statements.push(Node::new(span, StmtKind::Expr(literal)));
}

impl ParserBackend for Stub {
    fn info(&self) -> BackendInfo {
        BackendInfo {
            name: self.name.into(),
            version: "1".into(),
        }
    }

    fn parse(&self, _path: &str, source: &[u8], _options: &ParseOptions) -> ParsedFile {
        assert!(self.fault != Fault::Panic, "the stub was told to panic");
        mini_parse(source, self.fault)
    }
}

fn run(dir: &TempDir, backend: &dyn ParserBackend) -> Vec<Failure> {
    let goldens = dir.path().join("../goldens-unused");
    let mut options = SuiteOptions::new(&goldens);
    options.bless = true;
    let corpus = load(dir);
    run_suite(backend, &corpus, &options).failures
}

fn checks(failures: &[Failure]) -> Vec<&'static str> {
    failures.iter().map(|f| f.check).collect()
}

// ---- 4.2 / 4.4: the suite flags each misbehaviour ----

#[test]
fn a_faithful_backend_passes() {
    let dir = good_corpus();
    let failures = run(&dir, &Stub::new(Fault::None));
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn a_dropped_statement_is_flagged() {
    let dir = good_corpus();
    let failures = run(&dir, &Stub::new(Fault::DropStatement));
    assert!(checks(&failures).contains(&"invariants"), "{failures:?}");
}

#[test]
fn overlapping_siblings_are_flagged() {
    let dir = good_corpus();
    let failures = run(&dir, &Stub::new(Fault::OverlapSiblings));
    assert!(checks(&failures).contains(&"invariants"), "{failures:?}");
}

#[test]
fn a_malformed_fixture_parsed_clean_is_flagged() {
    let dir = good_corpus();
    let failures = run(&dir, &Stub::new(Fault::ParseMalformedClean));
    assert!(
        checks(&failures).contains(&"malformed-reported"),
        "{failures:?}"
    );
}

#[test]
fn a_lost_survivor_is_flagged_by_name() {
    let dir = good_corpus();
    let failures = run(&dir, &Stub::new(Fault::LoseSurvivor));
    let survivor: Vec<_> = failures
        .iter()
        .filter(|f| f.check == "malformed-survivor")
        .collect();
    assert_eq!(survivor.len(), 1, "{failures:?}");
    assert!(survivor[0].message.contains("ok_after"));
}

#[test]
fn a_panicking_backend_is_reported_not_propagated() {
    let dir = good_corpus();
    let failures = run(&dir, &Stub::new(Fault::Panic));
    assert!(checks(&failures).contains(&"resilience"), "{failures:?}");
}

#[test]
fn a_wrong_error_region_is_flagged() {
    let dir = good_corpus();
    let manifest = MANIFEST.replace("lines = [3, 3]", "lines = [1, 1]");
    dir.write("manifest.toml", manifest.as_bytes());
    let failures = run(&dir, &Stub::new(Fault::None));
    assert!(
        checks(&failures).contains(&"malformed-error-node"),
        "{failures:?}"
    );
}

#[test]
fn an_unreported_code_family_is_flagged() {
    let dir = good_corpus();
    let manifest = MANIFEST.replace(
        "codes = [\"unexpected_token\"]",
        "codes = [\"unclosed_string\"]",
    );
    dir.write("manifest.toml", manifest.as_bytes());
    let failures = run(&dir, &Stub::new(Fault::None));
    assert!(
        checks(&failures).contains(&"malformed-code"),
        "{failures:?}"
    );
}

// ---- goldens ----

#[test]
fn a_changed_dump_fails_against_its_golden_and_a_missing_golden_is_named() {
    let dir = good_corpus();
    let goldens = TempDir::new();
    let corpus = load(&dir);
    let mut options = SuiteOptions::new(goldens.path());
    let missing = run_suite(&Stub::new(Fault::None), &corpus, &options).failures;
    assert!(
        checks(&missing).iter().all(|c| *c == "golden"),
        "{missing:?}"
    );
    assert!(missing.iter().all(|f| f.message.contains("BLESS=1")));

    options.bless = true;
    assert!(
        run_suite(&Stub::new(Fault::None), &corpus, &options)
            .failures
            .is_empty()
    );
    options.bless = false;
    assert!(
        run_suite(&Stub::new(Fault::None), &corpus, &options)
            .failures
            .is_empty()
    );

    let changed = run_suite(&Stub::new(Fault::WrongSpanOnSecond), &corpus, &options).failures;
    assert!(checks(&changed).contains(&"golden"), "{changed:?}");
}

// ---- ledger agreement ----

#[test]
fn a_backend_accepting_what_every_verified_interpreter_rejects_is_flagged() {
    let dir = good_corpus();
    // The "valid" fixture is recorded as rejected everywhere, yet the stub parses it clean.
    dir.write(
        "ledger.toml",
        ledger_text(
            &[
                ("v7.0/one.php", Verdict::Invalid),
                ("malformed/broken.php", Verdict::Invalid),
            ],
            &[],
        )
        .as_bytes(),
    );
    let failures = run(&dir, &Stub::new(Fault::None));
    assert!(
        checks(&failures).contains(&"ledger-agreement"),
        "{failures:?}"
    );
}

#[test]
fn unverified_versions_are_reported_and_do_not_decide_agreement() {
    let dir = good_corpus();
    let all: Vec<&str> = VERSIONS.to_vec();
    dir.write(
        "ledger.toml",
        ledger_text(
            &[
                ("v7.0/one.php", Verdict::Invalid),
                ("malformed/broken.php", Verdict::Invalid),
            ],
            &all,
        )
        .as_bytes(),
    );
    let corpus = load(&dir);
    let mut options = SuiteOptions::new(dir.path().join("../goldens-unused"));
    options.bless = true;
    let report = run_suite(&Stub::new(Fault::None), &corpus, &options);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.unverified.len(), VERSIONS.len());
    assert!(
        report
            .unverified
            .iter()
            .all(|(_, reason)| reason == "no image")
    );
}

// ---- 4.3: the loader and the completeness checks ----

fn problems(dir: &TempDir) -> Vec<String> {
    load(dir).check()
}

#[test]
fn a_complete_corpus_has_no_problems() {
    assert!(problems(&good_corpus()).is_empty());
}

#[test]
fn an_orphan_fixture_is_named() {
    let dir = good_corpus();
    dir.write("v7.0/orphan.php", b"<?php\n");
    assert!(
        problems(&dir)
            .iter()
            .any(|p| p.contains("orphan") && p.contains("v7.0/orphan.php"))
    );
}

#[test]
fn a_missing_file_is_named() {
    let dir = good_corpus();
    std::fs::remove_file(dir.path().join("v7.0/one.php")).unwrap();
    assert!(
        problems(&dir)
            .iter()
            .any(|p| p.contains("v7.0/one.php") && p.contains("does not exist"))
    );
}

#[test]
fn a_feature_without_a_fixture_is_named() {
    let dir = good_corpus();
    let manifest = format!(
        "{MANIFEST}\n[[feature]]\nid = \"lonely\"\nversion = \"7.1\"\ndoc = \"https://www.php.net/x\"\nsummary = \"s\"\nfixtures = []\n"
    );
    dir.write("manifest.toml", manifest.as_bytes());
    assert!(
        problems(&dir)
            .iter()
            .any(|p| p.contains("lonely") && p.contains("no fixture"))
    );
}

#[test]
fn a_valid_fixture_mapped_to_no_feature_is_named() {
    let dir = good_corpus();
    let manifest = MANIFEST.replace(
        "fixtures = [\"v7.0/one.php\"]",
        "fixtures = [\"malformed/broken.php\"]",
    );
    dir.write("manifest.toml", manifest.as_bytes());
    assert!(
        problems(&dir)
            .iter()
            .any(|p| p.contains("v7.0/one.php") && p.contains("no feature"))
    );
}

#[test]
fn an_oversized_fixture_is_named_but_a_realistic_one_may_be_large() {
    let dir = good_corpus();
    let long = "<?php\n".to_string() + &"echo 1;\n".repeat(250);
    dir.write("v7.0/one.php", long.as_bytes());
    assert!(
        problems(&dir)
            .iter()
            .any(|p| p.contains("v7.0/one.php") && p.contains("lines"))
    );

    let manifest = format!(
        "{}\n[[fixture]]\npath = \"realistic/big.php\"\ngroup = \"realistic\"\nprovenance = \"original\"\nlicence = \"MIT\"\npurpose = \"a large app file\"\n",
        MANIFEST
    );
    let dir = good_corpus();
    dir.write("manifest.toml", manifest.as_bytes());
    dir.write("realistic/big.php", long.as_bytes());
    let mut ledger = ledger_text(
        &[
            ("v7.0/one.php", Verdict::Valid),
            ("malformed/broken.php", Verdict::Invalid),
            ("realistic/big.php", Verdict::Valid),
        ],
        &[],
    );
    ledger.push('\n');
    dir.write("ledger.toml", ledger.as_bytes());
    assert!(problems(&dir).is_empty(), "{:?}", problems(&dir));
}

#[test]
fn a_fixture_without_provenance_or_with_a_disallowed_licence_is_named() {
    let dir = good_corpus();
    let manifest = MANIFEST
        .replacen("provenance = \"original\"", "provenance = \"\"", 1)
        .replacen("licence = \"MIT\"", "licence = \"GPL-3.0-only\"", 2);
    dir.write("manifest.toml", manifest.as_bytes());
    let found = problems(&dir);
    assert!(found.iter().any(|p| p.contains("provenance")), "{found:?}");
    assert!(found.iter().any(|p| p.contains("licence")), "{found:?}");
}

#[test]
fn a_malformed_fixture_without_an_expectation_is_named() {
    let dir = good_corpus();
    let cut = MANIFEST.find("[fixture.expect]").unwrap();
    dir.write("manifest.toml", &MANIFEST.as_bytes()[..cut]);
    let found = problems(&dir);
    assert!(
        found
            .iter()
            .any(|p| p.contains("malformed/broken.php") && p.contains("expectation")),
        "{found:?}"
    );
}

#[test]
fn a_breakage_class_without_a_fixture_is_named() {
    let dir = good_corpus();
    let manifest = MANIFEST.replace(
        "breakage_classes = [\"invalid-token\"]",
        "breakage_classes = [\"invalid-token\", \"unterminated-heredoc\"]",
    );
    dir.write("manifest.toml", manifest.as_bytes());
    assert!(
        problems(&dir)
            .iter()
            .any(|p| p.contains("unterminated-heredoc"))
    );
}

#[test]
fn a_doc_that_is_not_on_php_net_is_named() {
    let dir = good_corpus();
    let manifest = MANIFEST.replace(
        "https://www.php.net/manual/en/x.php",
        "https://example.com/x",
    );
    dir.write("manifest.toml", manifest.as_bytes());
    assert!(problems(&dir).iter().any(|p| p.contains("php.net")));
}

#[test]
fn a_fixture_without_a_ledger_entry_is_named() {
    let dir = good_corpus();
    dir.write(
        "ledger.toml",
        ledger_text(&[("v7.0/one.php", Verdict::Valid)], &[]).as_bytes(),
    );
    assert!(
        problems(&dir)
            .iter()
            .any(|p| p.contains("malformed/broken.php") && p.contains("ledger"))
    );
}

#[test]
fn an_unverified_version_needs_a_reason() {
    let dir = good_corpus();
    let text = ledger_text(
        &[
            ("v7.0/one.php", Verdict::Valid),
            ("malformed/broken.php", Verdict::Invalid),
        ],
        &["8.5"],
    )
    .replace("reason = \"no image\"\n", "");
    dir.write("ledger.toml", text.as_bytes());
    assert!(
        problems(&dir)
            .iter()
            .any(|p| p.contains("8.5") && p.contains("reason"))
    );
}

#[test]
fn a_malformed_manifest_is_an_error_not_a_panic() {
    let dir = good_corpus();
    dir.write("manifest.toml", b"this is = not [valid");
    assert!(Corpus::load(dir.path()).is_err());
}

// ---- 4.7: the differential harness ----

#[test]
fn identical_backends_do_not_differ() {
    let dir = good_corpus();
    let a = Stub::new(Fault::None);
    let b = Stub {
        fault: Fault::None,
        name: "other",
    };
    let found = compare(
        &[("a", &a as &dyn ParserBackend), ("b", &b)],
        &load(&dir),
        &Allowances::none(),
        &ParseOptions::default(),
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn differing_backends_fail_unless_the_fixture_is_allowed_with_a_reason() {
    let dir = good_corpus();
    let a = Stub::new(Fault::None);
    let b = Stub {
        fault: Fault::WrongSpanOnSecond,
        name: "other",
    };
    let pair: [(&str, &dyn ParserBackend); 2] = [("a", &a), ("b", &b)];
    let corpus = load(&dir);
    let found = compare(
        &pair,
        &corpus,
        &Allowances::none(),
        &ParseOptions::default(),
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].subject, "malformed/broken.php");
    assert!(found[0].message.contains("differs"));

    let allowed = Allowances::parse(
        "[[allowance]]\nfixture = \"malformed/broken.php\"\nreason = \"spans differ by design\"\n",
    )
    .unwrap();
    assert!(compare(&pair, &corpus, &allowed, &ParseOptions::default()).is_empty());
    assert!(Allowances::parse("[[allowance]]\nfixture = \"malformed/broken.php\"\n").is_err());
}

// ---- 4.5: reproducibility of the robustness sweep ----

struct Recorder {
    seen: std::sync::Mutex<Vec<Vec<u8>>>,
}

impl ParserBackend for Recorder {
    fn info(&self) -> BackendInfo {
        BackendInfo {
            name: "recorder".into(),
            version: "1".into(),
        }
    }

    fn parse(&self, _path: &str, source: &[u8], _options: &ParseOptions) -> ParsedFile {
        self.seen.lock().unwrap().push(source.to_vec());
        mini_parse(source, Fault::None)
    }
}

fn inputs_seen(seed: u64) -> Vec<Vec<u8>> {
    let dir = good_corpus();
    let recorder = Recorder {
        seen: std::sync::Mutex::new(Vec::new()),
    };
    let mut options = RobustnessOptions::default();
    options.seed = seed;
    options.mutations = 6;
    options.prefix_stride = 7;
    options.slow_parse = Duration::from_secs(60);
    let _ = run_robustness(&recorder, &load(&dir), &options);
    let seen = recorder.seen.lock().unwrap();
    seen.clone()
}

#[test]
fn the_same_seed_replays_the_same_inputs_and_another_seed_does_not() {
    let first = inputs_seen(42);
    assert!(!first.is_empty());
    assert_eq!(first, inputs_seen(42));
    assert_ne!(first, inputs_seen(43));
}

#[test]
fn a_hanging_parse_is_reported_with_its_seed() {
    struct Slow;
    impl ParserBackend for Slow {
        fn info(&self) -> BackendInfo {
            BackendInfo {
                name: "slow".into(),
                version: "1".into(),
            }
        }
        fn parse(&self, _path: &str, source: &[u8], _options: &ParseOptions) -> ParsedFile {
            std::thread::sleep(Duration::from_millis(30));
            mini_parse(source, Fault::None)
        }
    }
    let dir = good_corpus();
    let mut options = RobustnessOptions::default();
    options.mutations = 1;
    options.prefix_stride = 1000;
    options.slow_parse = Duration::from_millis(5);
    let failures = run_robustness(&Slow, &load(&dir), &options);
    assert!(
        failures
            .iter()
            .any(|f| f.check == "resilience" && f.message.contains("took")),
        "{failures:?}"
    );
    assert!(
        failures.iter().any(|f| f.subject.contains("seed=")),
        "{failures:?}"
    );
}
