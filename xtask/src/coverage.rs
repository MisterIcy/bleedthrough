//! `cargo xtask coverage`: line coverage per crate and file from `cargo llvm-cov`, the lines a change adds
//! that no test executes, a comparison with another report, and the floors of the domain crates
//! (docs/testing.md, specs `coverage-reporting`).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;

use proc_macro2::{Span, TokenStream};
use quote::ToTokens;
use serde_json::{Value, json};
use syn::{Attribute, Item};

use crate::declaration::{Declaration, Layer};
use crate::meta::Package;

const CONFIG: &str = "testing/coverage.toml";
const FLOORS: &str = "testing/coverage-floors.toml";
const DEFAULT_OUT: &str = "target/coverage";

// ---- lcov ----

/// Execution counts per executable line, per file.
pub type Lines = BTreeMap<u32, u64>;
pub type Files = BTreeMap<String, Lines>;

/// Parses lcov text into per-file line counts; paths are made relative to `root` when they start with it.
/// Returns the files and whether any branch data (`BRDA`) was present.
pub fn parse_lcov(text: &str, root: &str) -> (Files, bool) {
    let mut files = Files::new();
    let mut current: Option<String> = None;
    let mut branches = false;
    let prefix = format!("{}/", root.trim_end_matches('/'));
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("SF:") {
            let path = path.strip_prefix(&prefix).unwrap_or(path).to_string();
            files.entry(path.clone()).or_default();
            current = Some(path);
        } else if let Some(data) = line.strip_prefix("DA:") {
            let mut parts = data.split(',');
            if let (Some(file), Some(number), Some(count)) = (&current, parts.next(), parts.next())
                && let (Ok(number), Ok(count)) = (number.parse::<u32>(), count.parse::<u64>())
            {
                let slot = files
                    .entry(file.clone())
                    .or_default()
                    .entry(number)
                    .or_insert(0);
                *slot = (*slot).max(count);
            }
        } else if line.starts_with("BRDA:") {
            branches = true;
        } else if line == "end_of_record" {
            current = None;
        }
    }
    (files, branches)
}

// ---- exclusions ----

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Exclusions {
    pub paths: Vec<String>,
    pub crates: Vec<String>,
    pub vendored: Vec<String>,
    pub generated: Vec<String>,
}

impl Exclusions {
    pub fn parse(text: &str) -> Result<Self, String> {
        let root: toml::Table = text.parse().map_err(|e| format!("{CONFIG}: {e}"))?;
        let section = root
            .get("exclude")
            .and_then(toml::Value::as_table)
            .ok_or_else(|| format!("{CONFIG}: missing [exclude]"))?;
        let list = |key: &str| -> Result<Vec<String>, String> {
            match section.get(key) {
                None => Ok(Vec::new()),
                Some(value) => value
                    .as_array()
                    .ok_or_else(|| format!("{CONFIG}: `{key}` must be a list"))?
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_string)
                            .ok_or_else(|| format!("{CONFIG}: `{key}` holds a non-string"))
                    })
                    .collect(),
            }
        };
        Ok(Self {
            paths: list("paths")?,
            crates: list("crates")?,
            vendored: list("vendored")?,
            generated: list("generated")?,
        })
    }

    /// Whether the repository-relative `path` is excluded.
    pub fn excludes(&self, path: &str) -> bool {
        let rooted = format!("/{path}");
        self.paths
            .iter()
            .chain(&self.vendored)
            .chain(&self.generated)
            .any(|p| rooted.contains(p.as_str()) || path.starts_with(p.as_str()))
            || self
                .crates
                .iter()
                .any(|c| path.starts_with(&format!("{c}/")))
    }

    pub fn describe(&self) -> Vec<String> {
        self.paths
            .iter()
            .chain(&self.vendored)
            .chain(&self.generated)
            .cloned()
            .chain(self.crates.iter().map(|c| format!("crate {c}")))
            .collect()
    }
}

fn is_cfg_test(attr: &Attribute) -> bool {
    attr.path().is_ident("cfg")
        && attr
            .to_token_stream()
            .to_string()
            .replace(' ', "")
            .contains("(test)")
}

fn is_test_attr(attr: &Attribute) -> bool {
    attr.path().is_ident("test")
}

fn span_lines(stream: TokenStream) -> Option<(u32, u32)> {
    let tokens: Vec<_> = stream.into_iter().collect();
    let line = |span: Span, end: bool| {
        let lc = if end { span.end() } else { span.start() };
        u32::try_from(lc.line).ok()
    };
    Some((
        line(tokens.first()?.span(), false)?,
        line(tokens.last()?.span(), true)?,
    ))
}

fn attrs_of(item: &Item) -> &[Attribute] {
    match item {
        Item::Fn(i) => &i.attrs,
        Item::Mod(i) => &i.attrs,
        Item::Impl(i) => &i.attrs,
        Item::Const(i) => &i.attrs,
        Item::Static(i) => &i.attrs,
        Item::Struct(i) => &i.attrs,
        Item::Enum(i) => &i.attrs,
        Item::Use(i) => &i.attrs,
        Item::Type(i) => &i.attrs,
        Item::Trait(i) => &i.attrs,
        _ => &[],
    }
}

fn collect_test_ranges(items: &[Item], out: &mut Vec<(u32, u32)>) {
    for item in items {
        let attrs = attrs_of(item);
        if attrs.iter().any(|a| is_cfg_test(a) || is_test_attr(a)) {
            if let Some(range) = span_lines(item.to_token_stream()) {
                out.push(range);
            }
        } else if let Item::Mod(m) = item
            && let Some((_, inner)) = &m.content
        {
            collect_test_ranges(inner, out);
        } else if let Item::Impl(i) = item {
            for member in &i.items {
                if let syn::ImplItem::Fn(f) = member
                    && f.attrs.iter().any(|a| is_cfg_test(a) || is_test_attr(a))
                    && let Some(range) = span_lines(f.to_token_stream())
                {
                    out.push(range);
                }
            }
        }
    }
}

/// The line ranges (inclusive) of test code in `source`: `#[cfg(test)]` items and `#[test]` functions.
pub fn test_ranges(source: &str) -> Vec<(u32, u32)> {
    let Ok(file) = syn::parse_file(source) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    collect_test_ranges(&file.items, &mut out);
    out
}

// ---- aggregation ----

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Counts {
    pub total: u64,
    pub covered: u64,
}

impl Counts {
    /// Percent with two decimals, or `None` when there are no executable lines.
    pub fn percent(&self) -> Option<f64> {
        (self.total > 0)
            .then(|| (self.covered as f64 * 10_000.0 / self.total as f64).round() / 100.0)
    }
    fn add(&mut self, other: &Self) {
        self.total += other.total;
        self.covered += other.covered;
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Report {
    /// crate name -> file -> counts
    pub crates: BTreeMap<String, BTreeMap<String, Counts>>,
    /// repository-relative file -> uncovered executable lines (after exclusions)
    pub uncovered: BTreeMap<String, Vec<u32>>,
    /// repository-relative file -> every executable line, covered or not
    pub executable: BTreeMap<String, BTreeSet<u32>>,
    pub branch_available: bool,
}

impl Report {
    pub fn crate_counts(&self, name: &str) -> Counts {
        let mut total = Counts::default();
        if let Some(files) = self.crates.get(name) {
            files.values().for_each(|c| total.add(c));
        }
        total
    }
    pub fn total(&self) -> Counts {
        let mut total = Counts::default();
        self.crates
            .keys()
            .for_each(|k| total.add(&self.crate_counts(k)));
        total
    }
}

/// Maps a repository-relative file to the crate whose directory contains it (longest match).
fn crate_of<'a>(file: &str, dirs: &'a [(String, String)]) -> Option<&'a str> {
    dirs.iter()
        .filter(|(dir, _)| file.starts_with(&format!("{dir}/")))
        .max_by_key(|(dir, _)| dir.len())
        .map(|(_, name)| name.as_str())
}

/// Returns a file's source by repository-relative path.
type SourceReader<'a> = &'a dyn Fn(&str) -> Option<String>;

/// Builds the report from lcov data: drops excluded files and test code, groups by crate.
/// `read` returns a file's source (for test-code detection); a file it cannot read keeps all its lines.
pub fn build_report(
    files: &Files,
    branch_available: bool,
    exclusions: &Exclusions,
    crate_dirs: &[(String, String)],
    read: SourceReader<'_>,
) -> Report {
    let mut report = Report {
        branch_available,
        ..Report::default()
    };
    for (file, lines) in files {
        if exclusions.excludes(file) {
            continue;
        }
        let Some(krate) = crate_of(file, crate_dirs) else {
            continue;
        };
        let ranges = read(file).map(|s| test_ranges(&s)).unwrap_or_default();
        let kept: Lines = lines
            .iter()
            .filter(|(line, _)| !ranges.iter().any(|(a, b)| a <= *line && *line <= b))
            .map(|(l, c)| (*l, *c))
            .collect();
        if kept.is_empty() {
            continue;
        }
        let counts = Counts {
            total: kept.len() as u64,
            covered: kept.values().filter(|c| **c > 0).count() as u64,
        };
        report
            .crates
            .entry(krate.to_string())
            .or_default()
            .insert(file.clone(), counts);
        let uncovered: Vec<u32> = kept
            .iter()
            .filter(|(_, c)| **c == 0)
            .map(|(l, _)| *l)
            .collect();
        if !uncovered.is_empty() {
            report.uncovered.insert(file.clone(), uncovered);
        }
        report
            .executable
            .insert(file.clone(), kept.keys().copied().collect());
    }
    report
}

// ---- rendering ----

fn fmt_percent(p: Option<f64>) -> String {
    p.map_or_else(|| "n/a".to_string(), |p| format!("{p:.2}"))
}

pub fn to_json(report: &Report, exclusions: &Exclusions) -> Value {
    let crates: BTreeMap<&str, Value> = report
        .crates
        .iter()
        .map(|(name, files)| {
            let totals = report.crate_counts(name);
            let files: BTreeMap<&str, Value> = files
                .iter()
                .map(|(f, c)| {
                    (
                        f.as_str(),
                        json!({"lines": c.total, "covered": c.covered, "percent": c.percent()}),
                    )
                })
                .collect();
            (
                name.as_str(),
                json!({"lines": totals.total, "covered": totals.covered, "percent": totals.percent(), "files": files}),
            )
        })
        .collect();
    let total = report.total();
    json!({
        "schema": 1,
        "branch_coverage": if report.branch_available { "available" } else { "unavailable" },
        "exclusions": exclusions.describe(),
        "total": {"lines": total.total, "covered": total.covered, "percent": total.percent()},
        "crates": crates,
    })
}

pub fn summary(report: &Report, exclusions: &Exclusions) -> String {
    let mut out = String::from(
        "Line coverage per crate (advisory: coverage shows what tests run, not what they check)\n\n",
    );
    out.push_str("| crate | lines | covered | line % |\n|---|---:|---:|---:|\n");
    for name in report.crates.keys() {
        let c = report.crate_counts(name);
        out.push_str(&format!(
            "| {name} | {} | {} | {} |\n",
            c.total,
            c.covered,
            fmt_percent(c.percent())
        ));
    }
    let t = report.total();
    out.push_str(&format!(
        "| **total** | {} | {} | {} |\n\n",
        t.total,
        t.covered,
        fmt_percent(t.percent())
    ));
    out.push_str(&if report.branch_available {
        "Branch coverage: available (see the JSON report).\n".to_string()
    } else {
        "Branch coverage: unavailable on the pinned stable toolchain (it needs `-Z coverage-options=branch`, nightly only); it is not reported as zero.\n".to_string()
    });
    out.push_str(&format!(
        "\nExcluded: {}\n",
        exclusions.describe().join(", ")
    ));
    out
}

/// Per-crate comparison with an earlier JSON report (the target branch's).
pub fn comparison(report: &Report, base: &Value) -> String {
    let mut out = String::from("| crate | base % | now % | change |\n|---|---:|---:|---:|\n");
    let mut names: BTreeSet<String> = report.crates.keys().cloned().collect();
    if let Some(map) = base["crates"].as_object() {
        names.extend(map.keys().cloned());
    }
    for name in names {
        let now = report.crate_counts(&name).percent();
        let before = base["crates"][&name]["percent"].as_f64();
        let change = match (before, now) {
            (Some(b), Some(n)) => format!("{:+.2}", n - b),
            _ => "n/a".to_string(),
        };
        out.push_str(&format!(
            "| {name} | {} | {} | {change} |\n",
            fmt_percent(before),
            fmt_percent(now)
        ));
    }
    out
}

// ---- changed lines ----

/// Lines added per file, from `git diff --unified=0` output.
pub fn added_lines(diff: &str) -> BTreeMap<String, Vec<u32>> {
    let mut out: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    let mut file: Option<String> = None;
    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ ") {
            file = path.strip_prefix("b/").map(str::to_string);
        } else if line.starts_with("@@")
            && let Some(file) = &file
            && let Some(range) = line.split_whitespace().find(|p| p.starts_with('+'))
        {
            let range = range.trim_start_matches('+');
            let (start, count) = range.split_once(',').unwrap_or((range, "1"));
            if let (Ok(start), Ok(count)) = (start.parse::<u32>(), count.parse::<u32>()) {
                out.entry(file.clone())
                    .or_default()
                    .extend(start..start + count);
            }
        }
    }
    out
}

/// Added lines that are executable and never executed, by file.
pub fn uncovered_changed(
    report: &Report,
    added: &BTreeMap<String, Vec<u32>>,
) -> BTreeMap<String, Vec<u32>> {
    let mut out = BTreeMap::new();
    for (file, lines) in added {
        let Some(uncovered) = report.uncovered.get(file) else {
            continue;
        };
        let hit: Vec<u32> = lines
            .iter()
            .copied()
            .filter(|l| uncovered.contains(l))
            .collect();
        if !hit.is_empty() {
            out.insert(file.clone(), hit);
        }
    }
    out
}

fn ranges(lines: &[u32]) -> String {
    let mut parts = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let mut j = i;
        while j + 1 < lines.len() && lines[j + 1] == lines[j] + 1 {
            j += 1;
        }
        parts.push(if i == j {
            lines[i].to_string()
        } else {
            format!("{}-{}", lines[i], lines[j])
        });
        i = j + 1;
    }
    parts.join(", ")
}

pub fn changed_lines_text(changed: &BTreeMap<String, Vec<u32>>, base: &str) -> String {
    if changed.is_empty() {
        return format!("Lines added since {base} that no test executes: none\n");
    }
    let mut out = format!("Lines added since {base} that no test executes:\n");
    for (file, lines) in changed {
        out.push_str(&format!("  {file}: {}\n", ranges(lines)));
    }
    out
}

// ---- floors ----

#[derive(Debug, Clone, PartialEq)]
pub struct Floors {
    pub margin: f64,
    pub floors: BTreeMap<String, (f64, String)>,
}

impl Floors {
    pub fn parse(text: &str) -> Result<Self, String> {
        let root: toml::Table = text.parse().map_err(|e| format!("{FLOORS}: {e}"))?;
        let margin = root
            .get("margin")
            .and_then(toml::Value::as_float)
            .ok_or_else(|| format!("{FLOORS}: `margin` (percentage points) is required"))?;
        let mut floors = BTreeMap::new();
        for (name, value) in root
            .get("crate")
            .and_then(toml::Value::as_table)
            .into_iter()
            .flatten()
        {
            let table = value
                .as_table()
                .ok_or_else(|| format!("{FLOORS}: [crate.{name}] is not a table"))?;
            let floor = table
                .get("floor")
                .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
                .ok_or_else(|| format!("{FLOORS}: [crate.{name}] needs a numeric `floor`"))?;
            let reason = table
                .get("reason")
                .and_then(toml::Value::as_str)
                .filter(|r| !r.trim().is_empty())
                .ok_or_else(|| format!("{FLOORS}: [crate.{name}] needs a `reason`"))?;
            floors.insert(name.clone(), (floor, reason.to_string()));
        }
        Ok(Self { margin, floors })
    }
}

/// Checks the measured coverage of `domain` crates against their floors.
pub fn check_floors(floors: &Floors, report: &Report, domain: &[String]) -> Vec<String> {
    let mut errors = Vec::new();
    for name in domain {
        let measured = report.crate_counts(name).percent();
        let Some((floor, _)) = floors.floors.get(name) else {
            errors.push(format!("{name}: no floor recorded in {FLOORS}"));
            continue;
        };
        match measured {
            // A crate without executable lines has nothing to measure; its floor must be 0.
            None if *floor > 0.0 => errors.push(format!(
                "{name}: no executable lines are measured but the floor is {floor:.2}"
            )),
            None => {}
            Some(actual) if actual < *floor => errors.push(format!(
                "{name}: line coverage {actual:.2}% is below its floor {floor:.2}%"
            )),
            Some(actual) if actual > *floor + floors.margin => errors.push(format!(
                "{name}: line coverage {actual:.2}% is more than {:.2} points above its floor {floor:.2}%; raise the floor in {FLOORS} with a reason",
                floors.margin
            )),
            Some(_) => {}
        }
    }
    for name in floors.floors.keys().filter(|n| !domain.contains(n)) {
        errors.push(format!(
            "{name}: {FLOORS} has a floor for a crate that is not a domain crate"
        ));
    }
    errors
}

// ---- command ----

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

pub const HELP: &str = "\
usage: cargo xtask coverage [options]

  --out DIR        where to write coverage.json, summary.md, lcov.info, changed-lines.txt (default target/coverage)
  --from-lcov F    use an existing lcov file instead of running the instrumented tests
  --base REF       also list lines added since REF (git diff REF...HEAD plus the working tree) that no test runs
  --compare FILE   compare per-crate figures with an earlier coverage.json (the target branch's)
  --check          enforce testing/coverage-floors.toml for the domain crates
  --update-floors  rewrite testing/coverage-floors.toml from this run (a reviewed edit; reasons are kept or set)

Needs cargo-llvm-cov and the llvm-tools component (docs/testing.md).";

/// Runs (or reads) the instrumented tests and builds the report with the configured exclusions.
fn measure(
    root: &Path,
    packages: &[Package],
    args: &[String],
    out_dir: &Path,
) -> Result<(Report, Exclusions), String> {
    let lcov_path = match option(args, "--from-lcov") {
        Some(path) => path.into(),
        None => {
            let target = out_dir.join("lcov.info");
            let status = Command::new("cargo")
                .args([
                    "llvm-cov",
                    "--workspace",
                    "--locked",
                    "--lcov",
                    "--output-path",
                ])
                .arg(&target)
                .current_dir(root)
                .status()
                .map_err(|e| {
                    format!("cannot run cargo llvm-cov (install it, see docs/testing.md): {e}")
                })?;
            if !status.success() {
                return Err("the instrumented test run failed (see above)".to_string());
            }
            target
        }
    };
    let lcov =
        fs::read_to_string(&lcov_path).map_err(|e| format!("{}: {e}", lcov_path.display()))?;
    let (files, branch) = parse_lcov(&lcov, &root.display().to_string());
    let exclusions = Exclusions::parse(
        &fs::read_to_string(root.join(CONFIG)).map_err(|e| format!("{CONFIG}: {e}"))?,
    )?;
    let crate_dirs: Vec<(String, String)> = packages
        .iter()
        .filter_map(|p| {
            let rel = p.dir().strip_prefix(root).ok()?.display().to_string();
            Some((rel, p.name.clone()))
        })
        .collect();
    let report = build_report(&files, branch, &exclusions, &crate_dirs, &|f| {
        fs::read_to_string(root.join(f)).ok()
    });
    Ok((report, exclusions))
}

/// Writes the JSON report and the summary (with the optional comparison and changed lines); returns the summary.
fn publish(
    root: &Path,
    report: &Report,
    exclusions: &Exclusions,
    args: &[String],
    out_dir: &Path,
) -> Result<String, String> {
    let json = to_json(report, exclusions);
    fs::write(
        out_dir.join("coverage.json"),
        serde_json::to_string_pretty(&json).unwrap_or_default() + "\n",
    )
    .map_err(|e| e.to_string())?;
    let mut text = summary(report, exclusions);
    if let Some(path) = option(args, "--compare") {
        let base: Value =
            serde_json::from_str(&fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?)
                .map_err(|e| format!("{path}: {e}"))?;
        text.push_str("\nChange against the target branch:\n\n");
        text.push_str(&comparison(report, &base));
    }
    if let Some(base) = option(args, "--base") {
        let diff = git(root, &["diff", "--unified=0", "--no-color", &base])?;
        let changed = uncovered_changed(report, &added_lines(&diff));
        let listing = changed_lines_text(&changed, &base);
        fs::write(out_dir.join("changed-lines.txt"), &listing).map_err(|e| e.to_string())?;
        text.push('\n');
        text.push_str(&listing);
    }
    fs::write(out_dir.join("summary.md"), &text).map_err(|e| e.to_string())?;
    Ok(text)
}

pub fn run(
    root: &Path,
    declaration: &Declaration,
    packages: &[Package],
    args: &[String],
) -> Result<Vec<String>, String> {
    let out_dir = root.join(option(args, "--out").unwrap_or_else(|| DEFAULT_OUT.to_string()));
    fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let (report, exclusions) = measure(root, packages, args, &out_dir)?;
    println!("{}", publish(root, &report, &exclusions, args, &out_dir)?);

    let domain: Vec<String> = declaration
        .crates
        .iter()
        .filter(|(_, d)| d.layer == Layer::Domain)
        .map(|(n, _)| n.clone())
        .collect();
    if args.iter().any(|a| a == "--update-floors") {
        write_floors(root, &report, &domain)?;
        println!("{FLOORS} rewritten; review the reasons before committing");
    }
    if args.iter().any(|a| a == "--check") {
        let floors = Floors::parse(
            &fs::read_to_string(root.join(FLOORS)).map_err(|e| format!("{FLOORS}: {e}"))?,
        )?;
        return Ok(check_floors(&floors, &report, &domain));
    }
    Ok(Vec::new())
}

fn write_floors(root: &Path, report: &Report, domain: &[String]) -> Result<(), String> {
    let path = root.join(FLOORS);
    let previous = fs::read_to_string(&path)
        .ok()
        .and_then(|t| Floors::parse(&t).ok());
    let margin = previous.as_ref().map_or(3.0, |f| f.margin);
    let mut text = String::from(
        "# Minimum line coverage of each domain crate, checked by `cargo xtask coverage --check`.\n\
         # A crate below its floor fails; a crate more than `margin` points above it also fails until the\n\
         # floor is raised, so the floor tracks reality. Edit a floor only with a stated reason.\n\
         # Coverage shows what tests execute, not what they check: mutation testing measures that.\n\n",
    );
    text.push_str(&format!("margin = {margin:.1}\n"));
    for name in domain {
        let measured = report.crate_counts(name).percent().unwrap_or(0.0);
        let floor = (measured.floor() - 1.0).max(0.0);
        let reason = previous
            .as_ref()
            .and_then(|f| f.floors.get(name))
            .filter(|(old, _)| (*old - floor).abs() < f64::EPSILON)
            .map_or_else(
                || format!("measured {measured:.2}% when recorded; floor one point below"),
                |(_, r)| r.clone(),
            );
        text.push_str(&format!(
            "\n[crate.{name}]\nfloor = {floor:.1}\nreason = {reason:?}\n"
        ));
    }
    fs::write(&path, text).map_err(|e| format!("{FLOORS}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LCOV: &str = "SF:/repo/crates/a/src/lib.rs\nDA:1,3\nDA:2,0\nDA:3,0\nDA:10,5\nend_of_record\nSF:/repo/crates/a/tests/t.rs\nDA:1,1\nend_of_record\nSF:/repo/xtask/src/main.rs\nDA:1,0\nend_of_record\n";

    fn dirs() -> Vec<(String, String)> {
        vec![
            ("crates/a".into(), "a".into()),
            ("xtask".into(), "xtask".into()),
        ]
    }

    fn exclusions() -> Exclusions {
        Exclusions {
            paths: vec!["/tests/".into()],
            crates: vec!["xtask".into()],
            ..Exclusions::default()
        }
    }

    #[test]
    fn lcov_is_parsed_with_relative_paths_and_branch_detection() {
        let (files, branch) = parse_lcov(LCOV, "/repo");
        assert!(!branch);
        assert_eq!(files["crates/a/src/lib.rs"][&2], 0);
        let (_, branch) = parse_lcov("SF:/x\nBRDA:1,0,0,1\nend_of_record\n", "/repo");
        assert!(branch);
    }

    #[test]
    fn exclusions_drop_tests_and_the_xtask_crate() {
        let e = exclusions();
        assert!(e.excludes("crates/a/tests/t.rs"));
        assert!(e.excludes("xtask/src/main.rs"));
        assert!(!e.excludes("crates/a/src/lib.rs"));
    }

    #[test]
    fn test_modules_and_test_functions_are_found_by_line() {
        let source = "fn a() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {}\n}\n#[test]\nfn loose() {}\n";
        assert_eq!(test_ranges(source), vec![(2, 6), (7, 8)]);
    }

    #[test]
    fn the_report_groups_by_crate_and_skips_test_code() {
        let (files, _) = parse_lcov(LCOV, "/repo");
        let source = "fn x() {}\n#[cfg(test)]\nmod t {\n}\nfn f() {}\n".to_string()
            + &"\n".repeat(5)
            + "fn g() {}\n";
        let report = build_report(&files, false, &exclusions(), &dirs(), &|f| {
            (f == "crates/a/src/lib.rs").then(|| source.clone())
        });
        let counts = report.crate_counts("a");
        // Lines 2 and 3 fall inside the test module (lines 2-4), leaving lines 1 and 10.
        assert_eq!(
            counts,
            Counts {
                total: 2,
                covered: 2
            }
        );
        assert!(!report.crates.contains_key("xtask"));
        assert_eq!(report.total().percent(), Some(100.0));
    }

    #[test]
    fn uncovered_lines_are_kept_when_the_source_has_no_test_code() {
        let (files, _) = parse_lcov(LCOV, "/repo");
        let report = build_report(&files, false, &exclusions(), &dirs(), &|_| {
            Some("fn x() {}\n".into())
        });
        assert_eq!(
            report.crate_counts("a"),
            Counts {
                total: 4,
                covered: 2
            }
        );
        assert_eq!(report.uncovered["crates/a/src/lib.rs"], vec![2, 3]);
        assert_eq!(report.crate_counts("a").percent(), Some(50.0));
    }

    #[test]
    fn added_lines_are_read_from_a_zero_context_diff() {
        let diff = "diff --git a/f.rs b/f.rs\n--- a/f.rs\n+++ b/f.rs\n@@ -1,0 +2,2 @@\n+x\n+y\n@@ -9 +11 @@\n-a\n+b\n@@ -20,2 +22,0 @@\n-gone\n-gone\n";
        assert_eq!(added_lines(diff)["f.rs"], vec![2, 3, 11]);
    }

    #[test]
    fn changed_lines_that_no_test_runs_are_listed_by_file_and_line() {
        let (files, _) = parse_lcov(LCOV, "/repo");
        let report = build_report(&files, false, &exclusions(), &dirs(), &|_| None);
        let added = BTreeMap::from([("crates/a/src/lib.rs".to_string(), vec![1, 2, 3, 4])]);
        let changed = uncovered_changed(&report, &added);
        assert_eq!(changed["crates/a/src/lib.rs"], vec![2, 3]);
        assert!(changed_lines_text(&changed, "main").contains("crates/a/src/lib.rs: 2-3"));
        assert!(changed_lines_text(&BTreeMap::new(), "main").contains("none"));
    }

    fn floors(a: f64) -> Floors {
        Floors::parse(&format!(
            "margin = 3.0\n[crate.a]\nfloor = {a}\nreason = \"first run\"\n"
        ))
        .unwrap()
    }

    fn report_at(percent_of_ten: u64) -> Report {
        let mut report = Report::default();
        report.crates.entry("a".into()).or_default().insert(
            "f.rs".into(),
            Counts {
                total: 10,
                covered: percent_of_ten,
            },
        );
        report
    }

    #[test]
    fn erosion_names_the_crate_the_floor_and_the_figure() {
        let errors = check_floors(&floors(80.0), &report_at(7), &["a".to_string()]);
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].contains("a:")
                && errors[0].contains("70.00%")
                && errors[0].contains("80.00%"),
            "{errors:?}"
        );
    }

    #[test]
    fn a_stale_floor_fails_until_it_is_raised() {
        let errors = check_floors(&floors(60.0), &report_at(7), &["a".to_string()]);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("raise the floor"), "{errors:?}");
        assert!(check_floors(&floors(68.0), &report_at(7), &["a".to_string()]).is_empty());
        assert!(check_floors(&floors(70.0), &report_at(7), &["a".to_string()]).is_empty());
    }

    #[test]
    fn a_missing_or_foreign_floor_is_reported_and_a_floor_needs_a_reason() {
        assert_eq!(
            check_floors(
                &floors(70.0),
                &report_at(7),
                &["a".to_string(), "b".to_string()]
            )
            .len(),
            1
        );
        assert_eq!(check_floors(&floors(70.0), &report_at(7), &[]).len(), 1);
        assert!(Floors::parse("margin = 1.0\n[crate.a]\nfloor = 1.0\n").is_err());
    }

    #[test]
    fn the_comparison_shows_the_change_per_crate() {
        let base = json!({"crates": {"a": {"percent": 60.0}}});
        let text = comparison(&report_at(7), &base);
        assert!(text.contains("| a | 60.00 | 70.00 | +10.00 |"), "{text}");
    }

    #[test]
    fn branch_coverage_is_reported_unavailable_not_zero() {
        let text = summary(&report_at(7), &exclusions());
        assert!(text.contains("Branch coverage: unavailable"));
        assert!(to_json(&report_at(7), &exclusions())["branch_coverage"] == "unavailable");
    }
}
