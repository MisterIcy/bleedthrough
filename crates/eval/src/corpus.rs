//! The vulnerability fixture corpus: manifest loading and the completeness checks.
//!
//! The corpus is plain data under `fixtures/vuln/`; nothing here names a rule, an engine or a pack.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use bleedthrough_taxonomy::Cwe;
use toml::{Table, Value};

/// Licences a case may carry; the same list as the syntax corpus and the dependency policy.
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

/// A case file is at most this many lines.
pub const MAX_CASE_LINES: usize = 100;

/// Confidence tiers, lowest first.
pub const TIERS: [&str; 3] = ["low", "medium", "high"];

/// Whether a case is expected to be reported or not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    /// A real weakness: the expectations must be found.
    Vulnerable,
    /// No finding of the case's CWE is expected in the protected ranges.
    Safe,
}

/// A line range (1-based, inclusive) in one file of a case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    /// The file, relative to the corpus root.
    pub file: String,
    /// First line.
    pub start: u32,
    /// Last line.
    pub end: u32,
}

/// One finding a vulnerable case expects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expect {
    /// Where the finding must be.
    pub range: Range,
    /// The weakness class, `CWE-<n>`.
    pub cwe: String,
    /// The lowest confidence tier that counts.
    pub min_confidence: String,
}

/// A known gap: a case the tool is known to get wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownGap {
    /// Why the tool gets it wrong.
    pub reason: String,
    /// The tracking record.
    pub tracking: String,
}

/// One case of the corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    /// Unique identifier.
    pub id: String,
    /// The weakness class, `CWE-<n>`.
    pub cwe: String,
    /// Vulnerable or safe.
    pub label: Label,
    /// The data flow in a sentence.
    pub description: String,
    /// The PHP versions the case applies to, for example `8.0+`.
    pub php: String,
    /// The flow shapes the case covers.
    pub shapes: Vec<String>,
    /// The counterpart case.
    pub twin: Option<String>,
    /// What the safe twin changes (vulnerable cases).
    pub fix: Option<String>,
    /// The reason a safe case has no twin.
    pub standalone: Option<String>,
    /// Where the content comes from.
    pub provenance: String,
    /// The licence of the content.
    pub licence: String,
    /// Files of the case, relative to the corpus root.
    pub files: Vec<String>,
    /// The known-gap marker.
    pub known_gap: Option<KnownGap>,
    /// Expected findings (vulnerable cases).
    pub expect: Vec<Expect>,
    /// Protected ranges (safe cases); empty means every file as a whole.
    pub protect: Vec<Range>,
}

impl Case {
    /// The protected ranges of a safe case: the declared ones, else each file whole.
    pub fn protected(&self) -> Vec<Range> {
        if self.protect.is_empty() {
            self.files
                .iter()
                .map(|file| Range {
                    file: file.clone(),
                    start: 1,
                    end: u32::MAX,
                })
                .collect()
        } else {
            self.protect.clone()
        }
    }
}

/// A weakness class for which some flow shapes do not exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassNote {
    /// The weakness class.
    pub cwe: String,
    /// Shapes the class does not admit.
    pub not_applicable: Vec<String>,
    /// Why.
    pub reason: String,
}

/// The loaded corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Corpus {
    /// The directory holding the manifest.
    pub root: PathBuf,
    /// The closed vocabulary of flow shapes.
    pub shapes: Vec<String>,
    /// Classes with shapes that do not apply.
    pub classes: Vec<ClassNote>,
    /// The cases, in manifest order.
    pub cases: Vec<Case>,
}

fn text(table: &Table, key: &str) -> Option<String> {
    table.get(key).and_then(Value::as_str).map(str::to_string)
}

fn strings(table: &Table, key: &str) -> Result<Vec<String>, String> {
    match table.get(key) {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| format!("`{key}` must hold strings"))
            })
            .collect(),
        Some(_) => Err(format!("`{key}` must be an array")),
    }
}

fn lines(table: &Table) -> Result<(u32, u32), String> {
    let Some(Value::Array(pair)) = table.get("lines") else {
        return Err("`lines` must be a two-element array".to_string());
    };
    let number = |v: &Value| {
        v.as_integer()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| "`lines` must hold positive integers".to_string())
    };
    match pair.as_slice() {
        [a, b] => Ok((number(a)?, number(b)?)),
        _ => Err("`lines` must be a two-element array".to_string()),
    }
}

fn parse_range(table: &Table) -> Result<Range, String> {
    let file = text(table, "file").ok_or("missing `file`")?;
    let (start, end) = lines(table)?;
    Ok(Range { file, start, end })
}

fn tables<'a>(table: &'a Table, key: &str) -> Result<Vec<&'a Table>, String> {
    match table.get(key) {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| {
                v.as_table()
                    .ok_or_else(|| format!("`{key}` entries must be tables"))
            })
            .collect(),
        Some(_) => Err(format!("`{key}` must be an array of tables")),
    }
}

fn parse_case(table: &Table) -> Result<Case, String> {
    let id = text(table, "id").ok_or("a case has no `id`")?;
    let wrap = |e: String| format!("case `{id}`: {e}");
    let required = |key: &str| text(table, key).ok_or_else(|| wrap(format!("missing `{key}`")));
    let cwe = required("cwe")?;
    let label = match required("label")?.as_str() {
        "vulnerable" => Label::Vulnerable,
        "safe" => Label::Safe,
        other => {
            return Err(wrap(format!(
                "label `{other}` is neither vulnerable nor safe"
            )));
        }
    };
    let mut expect = Vec::new();
    for entry in tables(table, "expect").map_err(wrap)? {
        let range = parse_range(entry).map_err(wrap)?;
        expect.push(Expect {
            range,
            cwe: text(entry, "cwe").unwrap_or_else(|| cwe.clone()),
            min_confidence: text(entry, "min_confidence")
                .ok_or_else(|| wrap("an expectation has no `min_confidence`".to_string()))?,
        });
    }
    let mut protect = Vec::new();
    for entry in tables(table, "protect").map_err(wrap)? {
        protect.push(parse_range(entry).map_err(wrap)?);
    }
    let known_gap = match table.get("known_gap") {
        None => None,
        Some(Value::Table(gap)) => Some(KnownGap {
            reason: text(gap, "reason").unwrap_or_default(),
            tracking: text(gap, "tracking").unwrap_or_default(),
        }),
        Some(_) => return Err(wrap("`known_gap` must be a table".to_string())),
    };
    Ok(Case {
        description: required("description")?,
        php: required("php")?,
        shapes: strings(table, "shapes").map_err(wrap)?,
        twin: text(table, "twin"),
        fix: text(table, "fix"),
        standalone: text(table, "standalone"),
        provenance: text(table, "provenance").unwrap_or_default(),
        licence: text(table, "licence").unwrap_or_default(),
        files: strings(table, "files").map_err(wrap)?,
        known_gap,
        expect,
        protect,
        id,
        cwe,
        label,
    })
}

impl Corpus {
    /// Parses a manifest. Only the structure is checked here; [`Corpus::check`] checks the rest.
    pub fn parse(root: &Path, manifest: &str) -> Result<Self, String> {
        let table: Table = manifest
            .parse()
            .map_err(|e| format!("manifest.toml: {e}"))?;
        let shapes = strings(&table, "shapes")?;
        let mut classes = Vec::new();
        for entry in tables(&table, "class")? {
            classes.push(ClassNote {
                cwe: text(entry, "cwe").ok_or("a `class` entry has no `cwe`")?,
                not_applicable: strings(entry, "not_applicable")?,
                reason: text(entry, "reason").unwrap_or_default(),
            });
        }
        let cases = tables(&table, "case")?
            .into_iter()
            .map(parse_case)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            root: root.to_path_buf(),
            shapes,
            classes,
            cases,
        })
    }

    /// Reads `<root>/manifest.toml`.
    pub fn load(root: &Path) -> Result<Self, String> {
        Self::load_named(root, "manifest.toml")
    }

    /// Reads a manifest-format file `<root>/<name>`, for example a generated `labels.toml`.
    pub fn load_named(root: &Path, name: &str) -> Result<Self, String> {
        let path = root.join(name);
        let manifest = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(root, &manifest)
    }

    /// The case with this identifier.
    pub fn case(&self, id: &str) -> Option<&Case> {
        self.cases.iter().find(|c| c.id == id)
    }

    /// Every problem with the corpus: files, labels, pairing, taxonomy, licences and sizes. Needs no
    /// scan command and no network.
    pub fn check(&self) -> Vec<String> {
        self.check_with(true)
    }

    /// The checks of [`Corpus::check`] for a generated label file: files that no case lists are
    /// allowed (a synthetic tree is mostly unlabelled code) and so are files over the size limit,
    /// which exists to keep hand-written cases focused.
    pub fn check_labels(&self) -> Vec<String> {
        self.check_with(false)
    }

    fn check_with(&self, strict: bool) -> Vec<String> {
        let mut errors = Vec::new();
        let mut ids = BTreeSet::new();
        for case in &self.cases {
            if !ids.insert(case.id.as_str()) {
                errors.push(format!("duplicate case id `{}`", case.id));
            }
        }
        let mut listed = BTreeSet::new();
        let mut line_counts = BTreeMap::new();
        for case in &self.cases {
            for file in &case.files {
                listed.insert(file.clone());
                if let Some(count) = self.read_lines(case, file, strict, &mut errors) {
                    line_counts.insert(file.clone(), count);
                }
            }
        }
        for case in &self.cases {
            self.check_case(case, &line_counts, &mut errors);
        }
        for class in &self.classes {
            if let Err(e) = Cwe::parse(&class.cwe) {
                errors.push(format!("class note `{}`: {e}", class.cwe));
            }
            for shape in &class.not_applicable {
                if !self.shapes.contains(shape) {
                    errors.push(format!(
                        "class note `{}`: unknown shape `{shape}`",
                        class.cwe
                    ));
                }
            }
            if !class.not_applicable.is_empty() && class.reason.trim().is_empty() {
                errors.push(format!("class note `{}` has no reason", class.cwe));
            }
        }
        if strict {
            errors.extend(self.orphans(&listed));
        }
        errors
    }

    fn read_lines(
        &self,
        case: &Case,
        file: &str,
        strict: bool,
        errors: &mut Vec<String>,
    ) -> Option<usize> {
        let unsafe_path = file.starts_with('/')
            || Path::new(file)
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir));
        if unsafe_path || !file.ends_with(".php") {
            errors.push(format!(
                "case `{}`: `{file}` must be a relative .php path inside the corpus",
                case.id
            ));
            return None;
        }
        let Ok(bytes) = fs::read(self.root.join(file)) else {
            errors.push(format!("case `{}`: file `{file}` is missing", case.id));
            return None;
        };
        let content = String::from_utf8_lossy(&bytes);
        let count = content.lines().count();
        if strict && count > MAX_CASE_LINES {
            errors.push(format!(
                "case `{}`: `{file}` has {count} lines; the limit is {MAX_CASE_LINES}",
                case.id
            ));
        }
        if content.contains("vendor/autoload") || content.contains("composer") {
            errors.push(format!(
                "case `{}`: `{file}` depends on an installed package; write a stub into the case",
                case.id
            ));
        }
        Some(count)
    }

    fn check_case(&self, case: &Case, counts: &BTreeMap<String, usize>, errors: &mut Vec<String>) {
        let id = &case.id;
        let mut say = |message: String| errors.push(format!("case `{id}`: {message}"));
        self.check_header(case, &mut say);
        let check_range = |range: &Range, what: &str, say: &mut dyn FnMut(String)| {
            let Some(&count) = counts
                .get(&range.file)
                .filter(|_| case.files.contains(&range.file))
            else {
                say(format!(
                    "{what} names `{}`, which is not a file of the case",
                    range.file
                ));
                return;
            };
            let outside = range.start == 0
                || range.start > range.end
                || range.start as usize > count
                || (range.end != u32::MAX && range.end as usize > count);
            if outside {
                say(format!(
                    "{what} lines {}-{} are outside `{}` ({count} lines)",
                    range.start, range.end, range.file
                ));
            }
        };
        match case.label {
            Label::Vulnerable => check_vulnerable(case, &check_range, &mut say),
            Label::Safe => {
                if !case.expect.is_empty() {
                    say("is safe but declares expected findings".to_string());
                }
                for range in &case.protect {
                    check_range(range, "protected range", &mut say);
                }
            }
        }
        self.check_twin(case, &mut say);
    }

    fn check_header(&self, case: &Case, say: &mut dyn FnMut(String)) {
        if let Err(e) = Cwe::parse(&case.cwe) {
            say(format!("weakness `{}`: {e}", case.cwe));
        }
        let missing = [
            (case.description.trim().is_empty(), "has no description"),
            (case.provenance.trim().is_empty(), "has no provenance"),
            (case.files.is_empty(), "lists no files"),
            (case.shapes.is_empty(), "records no flow shape"),
        ];
        for (is_missing, message) in missing {
            if is_missing {
                say(message.to_string());
            }
        }
        if !valid_php_range(&case.php) {
            say(format!(
                "php `{}` is not like `8.0+` or `7.4-8.3`",
                case.php
            ));
        }
        if !ALLOWED_LICENCES.contains(&case.licence.as_str()) {
            say(format!(
                "licence `{}` is not on the allow-list",
                case.licence
            ));
        }
        for shape in case.shapes.iter().filter(|s| !self.shapes.contains(s)) {
            say(format!("unknown shape `{shape}`"));
        }
        if let Some(gap) = &case.known_gap
            && (gap.reason.trim().is_empty() || gap.tracking.trim().is_empty())
        {
            say("known gap needs a reason and a tracking reference".to_string());
        }
    }

    fn check_twin(&self, case: &Case, say: &mut dyn FnMut(String)) {
        match (&case.twin, &case.standalone) {
            (Some(twin), _) => {
                let Some(other) = self.case(twin) else {
                    say(format!("twin `{twin}` does not exist"));
                    return;
                };
                if other.label == case.label {
                    say(format!("twin `{twin}` has the same label"));
                }
                if other.cwe != case.cwe {
                    say(format!("twin `{twin}` has another weakness class"));
                }
                if other.twin.as_deref() != Some(case.id.as_str()) {
                    say(format!("twin `{twin}` does not reference it back"));
                }
            }
            (None, Some(reason)) => {
                if case.label == Label::Safe && reason.trim().is_empty() {
                    say("standalone negative has no reason".to_string());
                }
            }
            (None, None) => say("has no twin and is not a standalone negative".to_string()),
        }
    }

    fn orphans(&self, listed: &BTreeSet<String>) -> Vec<String> {
        let mut found = Vec::new();
        walk(&self.root, &self.root, &mut found);
        found
            .into_iter()
            .filter(|f| !listed.contains(f))
            .map(|f| format!("file `{f}` is not listed by any case"))
            .collect()
    }
}

type RangeCheck<'a> = dyn Fn(&Range, &str, &mut dyn FnMut(String)) + 'a;

fn check_vulnerable(case: &Case, check_range: &RangeCheck<'_>, say: &mut dyn FnMut(String)) {
    if case.expect.is_empty() {
        say("is vulnerable but declares no expected finding".to_string());
    }
    if !case.protect.is_empty() {
        say("is vulnerable but declares protected ranges".to_string());
    }
    for expect in &case.expect {
        check_range(&expect.range, "expected finding", say);
        if let Err(e) = Cwe::parse(&expect.cwe) {
            say(format!("expected finding `{}`: {e}", expect.cwe));
        }
        if !TIERS.contains(&expect.min_confidence.as_str()) {
            say(format!(
                "min_confidence `{}` is not low, medium or high",
                expect.min_confidence
            ));
        }
    }
    if case.standalone.is_some() {
        say("is vulnerable and cannot be standalone".to_string());
    }
    if case.fix.as_deref().is_none_or(|f| f.trim().is_empty()) {
        say("does not state what its safe twin's fix is".to_string());
    }
}

fn valid_php_range(text: &str) -> bool {
    let version = |v: &str| {
        v.split_once('.')
            .is_some_and(|(a, b)| a.parse::<u8>().is_ok() && b.parse::<u8>().is_ok())
    };
    match text.strip_suffix('+') {
        Some(from) => version(from),
        None => text
            .split_once('-')
            .is_some_and(|(a, b)| version(a) && version(b)),
    }
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            walk(root, &path, out);
        } else if path.extension().is_some_and(|e| e == "php")
            && let Ok(relative) = path.strip_prefix(root)
        {
            out.push(
                relative
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/"),
            );
        }
    }
}
