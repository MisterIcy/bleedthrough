//! The fixed report set shared by every integration test, built from the public API only.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use bleedthrough_app::SourceText;
use bleedthrough_findings::*;
use bleedthrough_taxonomy::*;

pub const DB: &str = "src/Db.php";
pub const SHELL: &str = "lib/Shell.php";
pub const HOSTILE: &str = "views/dir:a,b/é page.php";

pub const DB_TEXT: &str =
    "<?php\n$id = $_GET['id'];\n$q = \"SELECT * FROM t WHERE id = $id\";\nmysqli_query($db, $q);\n";
/// No trailing newline, on purpose.
pub const SHELL_TEXT: &str = "<?php\necho shell_exec($_GET['cmd']);";
pub const HOSTILE_TEXT: &str = "<?php\n// é 日本\necho $_GET['a'];\n";

/// The original text of the analysed files.
pub struct Sources(pub BTreeMap<String, String>);

impl SourceText for Sources {
    fn source(&self, path: &RelPath) -> Option<String> {
        self.0.get(path.as_str()).cloned()
    }
}

pub fn sources() -> Sources {
    let pairs = [(DB, DB_TEXT), (SHELL, SHELL_TEXT), (HOSTILE, HOSTILE_TEXT)];
    Sources(
        pairs
            .into_iter()
            .map(|(p, t)| (p.to_string(), t.to_string()))
            .collect(),
    )
}

fn rel(p: &str) -> RelPath {
    RelPath::new(p).unwrap()
}

/// Location of the first occurrence of `needle` in `text`.
pub fn at(path: &str, text: &str, needle: &str) -> Location {
    let start = text.find(needle).unwrap();
    Location::from_byte_range(rel(path), text, start, start + needle.len()).unwrap()
}

pub fn taxonomy() -> Taxonomy {
    Taxonomy::new(
        vec![Cwe::new(89).unwrap()],
        vec![OwaspRef::new(2021, "A03").unwrap()],
    )
    .unwrap()
}

pub fn parts(
    rule: &str,
    symbol: &str,
    location: Location,
    level: SeverityLevel,
    tier: ConfidenceTier,
) -> FindingParts {
    let rule = RuleId::new(rule).unwrap();
    let reasons = if tier == ConfidenceTier::High {
        vec![]
    } else {
        vec![Reason::new(ReasonCode::TypeUnknown)]
    };
    let input = FingerprintInput::new(rule.clone(), symbol, vec![format!("{symbol}-flow")], 0);
    FindingParts {
        fingerprint: Fingerprint::compute(&input),
        rule,
        message: Message::new(format!("Tainted value reaches a sink in {symbol}")).unwrap(),
        location,
        taxonomy: taxonomy(),
        severity: Severity::new(level),
        confidence: Confidence::new(tier, reasons).unwrap(),
        paranoid: false,
        trace: None,
        fix: None,
    }
}

pub fn edit(path: &str, text: &str, needle: &str, replacement: &str) -> Edit {
    let start = text.find(needle).unwrap();
    Edit::new(rel(path), start, start + needle.len(), needle, replacement).unwrap()
}

fn options() -> RunInfo {
    RunInfo::new(
        "bleedthrough",
        "0.0.0",
        RunOptions::new(ConfidenceTier::Medium, false, "8.3", "core-1"),
    )
}

fn clean_coverage() -> Coverage {
    let mut c = Coverage::new("8.3");
    c.files_clean = 12;
    c.packs = vec![
        PackRef::new("symfony", "1.0.0"),
        PackRef::new("core", "1.0.0"),
    ];
    c
}

pub fn build(findings: Vec<FindingParts>, coverage: Coverage, suppressed: Suppressed) -> Report {
    let findings = findings.into_iter().map(Finding::new).collect();
    Report::new(options(), findings, coverage, suppressed)
}

pub fn empty() -> Report {
    let mut coverage = clean_coverage();
    coverage.cache = Some(CacheStats::new(10, 2, 0));
    build(vec![], coverage, Suppressed::default())
}

fn step(kind: StepKind, needle: &str, label: &str) -> Step {
    Step::new(at(DB, DB_TEXT, needle), kind, label)
}

/// A registry with a second edition and a weakness outside v1, to exercise multi-edition output.
fn two_edition_registry() -> Registry {
    Registry::new(
        vec![
            WeaknessEntry {
                number: 89,
                name: "SQL injection",
                catalog: "test",
                categories: &[(2021, "A03")],
            },
            WeaknessEntry {
                number: 943,
                name: "Data query logic",
                catalog: "test",
                categories: &[(2025, "A05")],
            },
        ],
        vec![
            CategoryEntry {
                edition: 2021,
                id: "A03",
                name: "Injection",
            },
            CategoryEntry {
                edition: 2025,
                id: "A05",
                name: "Injection",
            },
        ],
    )
}

/// One finding with every optional part: trace, fix with two edits, two editions, reasons, score.
pub fn full() -> Report {
    let mut p = parts(
        "php.sqli",
        "Db::find",
        at(DB, DB_TEXT, "mysqli_query($db, $q)"),
        SeverityLevel::Critical,
        ConfidenceTier::Medium,
    );
    let registry = two_edition_registry();
    p.taxonomy = Taxonomy::new(
        vec![
            Cwe::new_in(&registry, 89).unwrap(),
            Cwe::new_in(&registry, 943).unwrap(),
        ],
        vec![
            OwaspRef::new_in(&registry, 2021, "A03").unwrap(),
            OwaspRef::new_in(&registry, 2025, "A05").unwrap(),
        ],
    )
    .unwrap();
    p.severity = Severity::new(SeverityLevel::Critical)
        .with_score_tenths(95)
        .unwrap()
        .with_vector("BT:1/AV:N");
    p.confidence = Confidence::new(
        ConfidenceTier::Medium,
        vec![
            Reason::with_detail(ReasonCode::DynamicDispatchSeveralCandidates, "3"),
            Reason::new(ReasonCode::SourceInferred),
        ],
    )
    .unwrap();
    p.paranoid = true;
    p.trace = Some(
        Trace::new(
            vec![
                step(StepKind::Source, "$_GET['id']", "request parameter"),
                step(StepKind::Propagation, "$id", "interpolated into string"),
                step(StepKind::Sink, "mysqli_query($db, $q)", "executed as SQL"),
            ],
            10,
        )
        .unwrap(),
    );
    p.fix = Some(
        Fix::new(
            "Cast the id to an integer",
            Applicability::NeedsReview,
            vec![
                edit(DB, DB_TEXT, "$_GET['id']", "(int) $_GET['id']"),
                edit(DB, DB_TEXT, "mysqli_query", "mysqli_query"),
            ],
        )
        .unwrap(),
    );
    build(vec![p], clean_coverage(), Suppressed::default())
}

/// The three findings that carry fixes; the second overlaps the first.
fn fixable() -> Vec<FindingParts> {
    let mut crit = parts(
        "php.sqli",
        "Db::find",
        at(DB, DB_TEXT, "mysqli_query($db, $q)"),
        SeverityLevel::Critical,
        ConfidenceTier::High,
    );
    crit.fix = Some(
        Fix::new(
            "Use a prepared statement",
            Applicability::Mechanical,
            vec![edit(
                DB,
                DB_TEXT,
                "mysqli_query($db, $q)",
                "mysqli_execute($db, $q)",
            )],
        )
        .unwrap(),
    );
    let mut clash = parts(
        "php.sqli2",
        "Db::find2",
        at(DB, DB_TEXT, "$db, $q"),
        SeverityLevel::High,
        ConfidenceTier::Medium,
    );
    clash.fix = Some(
        Fix::new(
            "Overlaps the first fix",
            Applicability::Mechanical,
            vec![edit(DB, DB_TEXT, "$db, $q", "$db")],
        )
        .unwrap(),
    );
    let mut shell = parts(
        "php.cmdi",
        "Shell::run",
        at(SHELL, SHELL_TEXT, "shell_exec($_GET['cmd'])"),
        SeverityLevel::High,
        ConfidenceTier::High,
    );
    shell.fix = Some(
        Fix::new(
            "Escape the argument",
            Applicability::Mechanical,
            vec![edit(
                SHELL,
                SHELL_TEXT,
                "$_GET['cmd']",
                "escapeshellarg($_GET['cmd'])",
            )],
        )
        .unwrap(),
    );
    vec![crit, clash, shell]
}

/// Five severities, mixed confidences, three rules, overlapping and plain fixes, suppressed counts.
pub fn mixed() -> Report {
    let mut v = fixable();
    v.push(parts(
        "php.xss",
        "Db::show",
        at(DB, DB_TEXT, "$id = "),
        SeverityLevel::Medium,
        ConfidenceTier::Low,
    ));
    v.push(parts(
        "php.xss",
        "Db::list",
        at(DB, DB_TEXT, "SELECT"),
        SeverityLevel::Low,
        ConfidenceTier::High,
    ));
    v.push(parts(
        "php.info",
        "Shell::note",
        at(SHELL, SHELL_TEXT, "echo"),
        SeverityLevel::Info,
        ConfidenceTier::Medium,
    ));
    let mut suppressed = Suppressed::default();
    suppressed.below_threshold_low = 12;
    suppressed.below_threshold_medium = 3;
    suppressed.baselined = 7;
    build(v, clean_coverage(), suppressed)
}

/// Failed, recovered and skipped files, limits hit and unresolved dynamic constructs.
pub fn incomplete() -> Report {
    let mut coverage = clean_coverage();
    coverage.files_failed = 2;
    coverage.files_recovered = 1;
    coverage.skipped = vec![
        SkippedFile::new(rel("vendor/big.php"), SkipReason::TooLarge),
        SkippedFile::new(rel("src/slow.php"), SkipReason::TimeLimit),
    ];
    coverage.limits_hit = vec![
        LimitHit::new(LimitKind::PerFileTime, Some(rel("src/slow.php"))),
        LimitHit::new(LimitKind::TotalTime, None),
    ];
    coverage.unresolved_includes = 4;
    coverage.unresolved_calls = 9;
    let p = parts(
        "php.cmdi",
        "Shell::run",
        at(SHELL, SHELL_TEXT, "shell_exec($_GET['cmd'])"),
        SeverityLevel::High,
        ConfidenceTier::High,
    );
    build(vec![p], coverage, Suppressed::default())
}

/// Tabs, newlines, percent signs, colons, commas, backslashes and non-ASCII in every free-text field.
pub fn hostile() -> Report {
    let mut p = parts(
        "php.xss",
        "Page::render",
        at(HOSTILE, HOSTILE_TEXT, "echo $_GET['a']"),
        SeverityLevel::Medium,
        ConfidenceTier::High,
    );
    p.message =
        Message::new("100% unsafe:\tcolumn\nnext line\r\\ back, é 日本 \u{1F600} ::set-output")
            .unwrap();
    p.fix = Some(
        Fix::new(
            "Escape: <b>\t\"quoted\"\nline",
            Applicability::Mechanical,
            vec![edit(
                HOSTILE,
                HOSTILE_TEXT,
                "$_GET['a']",
                "htmlspecialchars($_GET['a'])",
            )],
        )
        .unwrap(),
    );
    build(vec![p], clean_coverage(), Suppressed::default())
}

/// Every named report of the set.
pub fn all() -> Vec<(&'static str, Report)> {
    vec![
        ("empty", empty()),
        ("full", full()),
        ("mixed", mixed()),
        ("incomplete", incomplete()),
        ("hostile", hostile()),
    ]
}

pub fn extension(format: bleedthrough_app::Format) -> &'static str {
    use bleedthrough_app::Format::*;
    match format {
        Json => "json",
        Sarif => "sarif",
        Ndjson => "ndjson",
        Tsv => "tsv",
        GithubActions => "gha.txt",
        Diff => "patch",
        _ => "out",
    }
}

pub fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

/// Emits `report` in `format` with the shared sources; returns the bytes and the outcome.
pub fn emit(
    format: bleedthrough_app::Format,
    report: &Report,
) -> (Vec<u8>, bleedthrough_app::EmitOutcome) {
    let src = sources();
    let ctx = bleedthrough_app::EmitContext::new().with_sources(&src);
    let emitter = bleedthrough_emit::emitter_for(format).expect("format is wired");
    let mut out = Vec::new();
    let outcome = emitter.emit(report, &ctx, &mut out).unwrap();
    (out, outcome)
}
