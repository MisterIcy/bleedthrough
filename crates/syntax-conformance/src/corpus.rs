//! The fixture corpus: manifest, ledger, loading, and the completeness checks.
//!
//! The corpus is plain data under `fixtures/php/`; nothing here knows any parser.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use toml::{Table, Value};

/// The fixtures a PHP version can introduce, in order. The corpus covers all of them.
pub const VERSIONS: [&str; 11] = [
    "7.0", "7.1", "7.2", "7.3", "7.4", "8.0", "8.1", "8.2", "8.3", "8.4", "8.5",
];

/// Licences a fixture may carry; the same list as the dependency policy in `deny.toml`, plus the
/// project's own dual licence.
pub const ALLOWED_LICENCES: [&str; 10] = [
    "MIT OR Apache-2.0",
    "MIT",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unicode-3.0",
    "Zlib",
    "CC0-1.0",
];

/// Non-realistic fixtures are at most this many lines.
pub const MAX_FIXTURE_LINES: usize = 200;

/// Which of the four groups a fixture belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Syntax introduced by a PHP version, valid in it.
    Valid,
    /// Syntax valid in some 7.x and rejected by 8.x; parsed leniently.
    Legacy,
    /// Broken code with declared expectations.
    Malformed,
    /// Files resembling real applications.
    Realistic,
}

impl Group {
    fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "valid" => Self::Valid,
            "legacy" => Self::Legacy,
            "malformed" => Self::Malformed,
            "realistic" => Self::Realistic,
            _ => return None,
        })
    }
}

/// What a malformed fixture must satisfy, without naming any parser's messages or recovery shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expectation {
    /// The class of breakage; one of the manifest's `breakage_classes`.
    pub class: String,
    /// At least one of these diagnostic codes must be reported.
    pub codes: Vec<String>,
    /// First and last line (1-based, inclusive) that an error node must cover.
    pub lines: (u32, u32),
    /// Declarations before and after the break that must survive in the tree.
    pub survivors: Vec<String>,
}

/// One fixture file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fixture {
    /// Path relative to the corpus root, with `/` separators.
    pub path: String,
    /// The fixture's group.
    pub group: Group,
    /// Why this file exists (required for realistic fixtures).
    pub purpose: Option<String>,
    /// Where the content comes from; `original` for files written for this project.
    pub provenance: String,
    /// The licence of the content.
    pub licence: String,
    /// The malformed-fixture expectation.
    pub expect: Option<Expectation>,
}

/// One syntax feature with the documentation page that describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feature {
    /// A stable identifier.
    pub id: String,
    /// The PHP version that introduced it.
    pub version: String,
    /// The documentation page.
    pub doc: String,
    /// A short description.
    pub summary: String,
    /// The fixtures exercising it.
    pub fixtures: Vec<String>,
}

/// An interpreter's verdict on one fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The interpreter accepts the syntax.
    Valid,
    /// The interpreter rejects it.
    Invalid,
}

impl Verdict {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "valid" => Some(Self::Valid),
            "invalid" => Some(Self::Invalid),
            _ => None,
        }
    }

    /// The word used in the ledger file.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Invalid => "invalid",
        }
    }
}

/// What is known about the interpreter of one PHP version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionInfo {
    /// The interpreter that produced the verdicts, empty when none was available.
    pub interpreter: String,
    /// The date the verdicts were recorded.
    pub recorded: String,
    /// Whether verdicts exist for this version; `false` versions are unverified.
    pub verified: bool,
    /// Why the version is unverified.
    pub reason: String,
}

/// The per-version verdict ledger.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ledger {
    /// How the interpreters were invoked.
    pub flags: String,
    /// Per-version interpreter information.
    pub versions: BTreeMap<String, VersionInfo>,
    /// Fixture path -> version -> verdict.
    pub verdicts: BTreeMap<String, BTreeMap<String, Verdict>>,
}

/// A loaded corpus.
#[derive(Debug, Clone)]
pub struct Corpus {
    /// The corpus root directory.
    pub root: PathBuf,
    /// The classes of breakage the malformed group must cover.
    pub breakage_classes: Vec<String>,
    /// Features, with their documentation.
    pub features: Vec<Feature>,
    /// Fixtures, in manifest order.
    pub fixtures: Vec<Fixture>,
    /// The ledger.
    pub ledger: Ledger,
}

fn string(table: &Table, key: &str, context: &str) -> Result<String, String> {
    match table.get(key) {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(_) => Err(format!("{context}: `{key}` must be a string")),
        None => Err(format!("{context}: missing `{key}`")),
    }
}

fn optional_string(table: &Table, key: &str) -> Option<String> {
    match table.get(key) {
        Some(Value::String(s)) => Some(s.clone()),
        _ => None,
    }
}

fn strings(table: &Table, key: &str, context: &str) -> Result<Vec<String>, String> {
    match table.get(key) {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| match v {
                Value::String(s) => Ok(s.clone()),
                _ => Err(format!("{context}: `{key}` must hold strings")),
            })
            .collect(),
        Some(_) => Err(format!("{context}: `{key}` must be an array")),
    }
}

fn tables<'a>(root: &'a Table, key: &str) -> Vec<&'a Table> {
    match root.get(key) {
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_table).collect(),
        _ => Vec::new(),
    }
}

fn expectation(table: &Table, context: &str) -> Result<Expectation, String> {
    let lines = match table.get("lines") {
        Some(Value::Array(v)) if v.len() == 2 => {
            let n = |i: usize| {
                v[i].as_integer()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| format!("{context}: `lines` must hold two positive integers"))
            };
            (n(0)?, n(1)?)
        }
        _ => return Err(format!("{context}: `lines` must be [first, last]")),
    };
    Ok(Expectation {
        class: string(table, "class", context)?,
        codes: strings(table, "codes", context)?,
        lines,
        survivors: strings(table, "survivors", context)?,
    })
}

fn fixture(table: &Table) -> Result<Fixture, String> {
    let path = string(table, "path", "fixture")?;
    let context = format!("fixture {path}");
    let group_name = string(table, "group", &context)?;
    let group = Group::parse(&group_name)
        .ok_or_else(|| format!("{context}: unknown group {group_name}"))?;
    let expect = match table.get("expect") {
        Some(Value::Table(t)) => Some(expectation(t, &context)?),
        _ => None,
    };
    Ok(Fixture {
        path,
        group,
        purpose: optional_string(table, "purpose"),
        provenance: optional_string(table, "provenance").unwrap_or_default(),
        licence: optional_string(table, "licence").unwrap_or_default(),
        expect,
    })
}

fn feature(table: &Table) -> Result<Feature, String> {
    let id = string(table, "id", "feature")?;
    let context = format!("feature {id}");
    Ok(Feature {
        version: string(table, "version", &context)?,
        doc: string(table, "doc", &context)?,
        summary: string(table, "summary", &context)?,
        fixtures: strings(table, "fixtures", &context)?,
        id,
    })
}

/// Parses a ledger file's text.
pub fn parse_ledger(text: &str) -> Result<Ledger, String> {
    let root: Table = text.parse().map_err(|e| format!("ledger: {e}"))?;
    let mut ledger = Ledger {
        flags: optional_string(&root, "flags").unwrap_or_default(),
        ..Ledger::default()
    };
    if let Some(Value::Table(versions)) = root.get("versions") {
        for (version, value) in versions {
            let t = value
                .as_table()
                .ok_or_else(|| format!("ledger: versions.{version} must be a table"))?;
            ledger.versions.insert(
                version.clone(),
                VersionInfo {
                    interpreter: optional_string(t, "interpreter").unwrap_or_default(),
                    recorded: optional_string(t, "recorded").unwrap_or_default(),
                    verified: t.get("verified").and_then(Value::as_bool).unwrap_or(false),
                    reason: optional_string(t, "reason").unwrap_or_default(),
                },
            );
        }
    }
    if let Some(Value::Table(verdicts)) = root.get("verdicts") {
        for (path, value) in verdicts {
            let t = value
                .as_table()
                .ok_or_else(|| format!("ledger: verdicts.{path} must be a table"))?;
            let mut row = BTreeMap::new();
            for (version, verdict) in t {
                let word = verdict.as_str().unwrap_or_default();
                let parsed = Verdict::parse(word)
                    .ok_or_else(|| format!("ledger: {path} {version}: unknown verdict {word:?}"))?;
                row.insert(version.clone(), parsed);
            }
            ledger.verdicts.insert(path.clone(), row);
        }
    }
    Ok(ledger)
}

/// Renders a ledger as TOML text, sorted and stable.
pub fn render_ledger(ledger: &Ledger) -> String {
    let mut out = String::from(
        "# Generated by the conformance ledger recorder; see fixtures/php/README.md.\n",
    );
    out.push_str(&format!("flags = {:?}\n\n", ledger.flags));
    for (version, info) in &ledger.versions {
        out.push_str(&format!("[versions.\"{version}\"]\n"));
        out.push_str(&format!("interpreter = {:?}\n", info.interpreter));
        out.push_str(&format!("recorded = {:?}\n", info.recorded));
        out.push_str(&format!("verified = {}\n", info.verified));
        if !info.reason.is_empty() {
            out.push_str(&format!("reason = {:?}\n", info.reason));
        }
        out.push('\n');
    }
    out.push_str("[verdicts]\n");
    for (path, row) in &ledger.verdicts {
        let cells: Vec<String> = row
            .iter()
            .map(|(v, verdict)| format!("\"{v}\" = \"{}\"", verdict.as_str()))
            .collect();
        out.push_str(&format!("{path:?} = {{ {} }}\n", cells.join(", ")));
    }
    out
}

impl Corpus {
    /// Loads `manifest.toml` and `ledger.toml` from `root`.
    pub fn load(root: &Path) -> Result<Self, String> {
        let manifest = std::fs::read_to_string(root.join("manifest.toml"))
            .map_err(|e| format!("manifest.toml: {e}"))?;
        let table: Table = manifest
            .parse()
            .map_err(|e| format!("manifest.toml: {e}"))?;
        let ledger_text = std::fs::read_to_string(root.join("ledger.toml"))
            .map_err(|e| format!("ledger.toml: {e}"))?;
        Ok(Self {
            root: root.to_path_buf(),
            breakage_classes: strings(&table, "breakage_classes", "manifest")?,
            features: tables(&table, "feature")
                .into_iter()
                .map(feature)
                .collect::<Result<_, _>>()?,
            fixtures: tables(&table, "fixture")
                .into_iter()
                .map(fixture)
                .collect::<Result<_, _>>()?,
            ledger: parse_ledger(&ledger_text)?,
        })
    }

    /// The bytes of a fixture.
    pub fn read(&self, fixture: &Fixture) -> Result<Vec<u8>, String> {
        std::fs::read(self.root.join(&fixture.path)).map_err(|e| format!("{}: {e}", fixture.path))
    }

    /// Every problem that makes the corpus incomplete or inconsistent, one line each.
    pub fn check(&self) -> Vec<String> {
        let mut problems = Vec::new();
        self.check_files(&mut problems);
        self.check_features(&mut problems);
        self.check_fixtures(&mut problems);
        self.check_ledger(&mut problems);
        problems
    }

    fn check_files(&self, problems: &mut Vec<String>) {
        let listed: BTreeSet<&str> = self.fixtures.iter().map(|f| f.path.as_str()).collect();
        if listed.len() != self.fixtures.len() {
            problems.push("a fixture is listed more than once in the manifest".to_string());
        }
        for path in php_files(&self.root) {
            if !listed.contains(path.as_str()) {
                problems.push(format!("orphan fixture {path}: not listed in the manifest"));
            }
        }
        for fixture in &self.fixtures {
            let on_disk = self.root.join(&fixture.path);
            match std::fs::read(&on_disk) {
                Err(_) => {
                    problems.push(format!("fixture {}: the file does not exist", fixture.path))
                }
                Ok(bytes) => {
                    let lines = bytes.iter().filter(|b| **b == b'\n').count() + 1;
                    if fixture.group != Group::Realistic && lines > MAX_FIXTURE_LINES {
                        problems.push(format!(
                            "fixture {}: {lines} lines, the limit is {MAX_FIXTURE_LINES}",
                            fixture.path
                        ));
                    }
                }
            }
        }
    }

    fn check_features(&self, problems: &mut Vec<String>) {
        let listed: BTreeSet<&str> = self.fixtures.iter().map(|f| f.path.as_str()).collect();
        let mut mapped: BTreeSet<&str> = BTreeSet::new();
        for feature in &self.features {
            if feature.fixtures.is_empty() {
                problems.push(format!("feature {}: no fixture", feature.id));
            }
            if !VERSIONS.contains(&feature.version.as_str()) {
                problems.push(format!(
                    "feature {}: unknown version {}",
                    feature.id, feature.version
                ));
            }
            if !feature.doc.starts_with("https://www.php.net/") {
                problems.push(format!(
                    "feature {}: `doc` must cite a php.net page",
                    feature.id
                ));
            }
            for path in &feature.fixtures {
                if listed.contains(path.as_str()) {
                    mapped.insert(path.as_str());
                } else {
                    problems.push(format!(
                        "feature {}: fixture {path} is not in the manifest",
                        feature.id
                    ));
                }
            }
        }
        for fixture in &self.fixtures {
            let needs_feature = matches!(fixture.group, Group::Valid | Group::Legacy);
            if needs_feature && !mapped.contains(fixture.path.as_str()) {
                problems.push(format!("fixture {}: mapped to no feature", fixture.path));
            }
        }
    }

    fn check_fixtures(&self, problems: &mut Vec<String>) {
        let classes: BTreeSet<&str> = self.breakage_classes.iter().map(String::as_str).collect();
        let mut covered: BTreeSet<&str> = BTreeSet::new();
        for fixture in &self.fixtures {
            if fixture.provenance.is_empty() {
                problems.push(format!("fixture {}: no provenance", fixture.path));
            }
            if !ALLOWED_LICENCES.contains(&fixture.licence.as_str()) {
                problems.push(format!(
                    "fixture {}: licence {:?} is not on the allow-list",
                    fixture.path, fixture.licence
                ));
            }
            if fixture.group == Group::Realistic && fixture.purpose.is_none() {
                problems.push(format!(
                    "fixture {}: realistic fixtures need a purpose",
                    fixture.path
                ));
            }
            match (&fixture.expect, fixture.group) {
                (None, Group::Malformed) => {
                    problems.push(format!(
                        "fixture {}: malformed fixtures need an expectation",
                        fixture.path
                    ));
                }
                (Some(e), Group::Malformed) => {
                    if classes.contains(e.class.as_str()) {
                        covered.insert(e.class.as_str());
                    } else {
                        problems.push(format!(
                            "fixture {}: unknown breakage class {}",
                            fixture.path, e.class
                        ));
                    }
                    if e.codes.is_empty() {
                        problems.push(format!(
                            "fixture {}: no expected diagnostic code",
                            fixture.path
                        ));
                    }
                }
                (Some(_), _) => {
                    problems.push(format!(
                        "fixture {}: only malformed fixtures carry an expectation",
                        fixture.path
                    ));
                }
                (None, _) => {}
            }
        }
        for class in &self.breakage_classes {
            if !covered.contains(class.as_str()) {
                problems.push(format!("breakage class {class}: no malformed fixture"));
            }
        }
    }

    fn check_ledger(&self, problems: &mut Vec<String>) {
        for fixture in &self.fixtures {
            if !self.ledger.verdicts.contains_key(&fixture.path) {
                problems.push(format!("fixture {}: no ledger entry", fixture.path));
            }
        }
        for path in self.ledger.verdicts.keys() {
            if !self.fixtures.iter().any(|f| &f.path == path) {
                problems.push(format!("ledger entry {path}: no such fixture"));
            }
        }
        for version in VERSIONS {
            match self.ledger.versions.get(version) {
                None => problems.push(format!("ledger: no entry for PHP {version}")),
                Some(info) if !info.verified && info.reason.is_empty() => {
                    problems.push(format!(
                        "ledger: PHP {version} is unverified without a reason"
                    ));
                }
                Some(_) => {}
            }
        }
    }
}

fn php_files(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "php")
                && let Ok(relative) = path.strip_prefix(root)
            {
                out.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    out
}
