//! `cargo xtask mutants`: mutation testing of the domain crates with `cargo mutants`, a normalised
//! report, and the survivor ledger (`testing/mutants-survivors.toml`) with its ratchet
//! (docs/testing.md, spec `mutation-testing`).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

const CONFIG: &str = "testing/mutants.toml";
const LEDGER: &str = "testing/mutants-survivors.toml";
const CRITICAL: &str = "testing/mutants-critical.toml";
const DEFAULT_OUT: &str = "target/mutants";

// ---- configuration ----

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub crates: Vec<String>,
    pub timeout_secs: u64,
    pub exclude_re: Vec<String>,
    pub exclude_globs: Vec<String>,
    /// Crate to the packages whose tests exercise it, when they are not its own (the engine's
    /// tests need a parser, so they live in the adapter).
    pub test_packages: BTreeMap<String, Vec<String>>,
}

fn string_list(table: &toml::Table, key: &str, file: &str) -> Result<Vec<String>, String> {
    match table.get(key) {
        None => Ok(Vec::new()),
        Some(value) => value
            .as_array()
            .ok_or_else(|| format!("{file}: `{key}` must be a list"))?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| format!("{file}: `{key}` holds a non-string"))
            })
            .collect(),
    }
}

fn test_packages(root: &toml::Table) -> Result<BTreeMap<String, Vec<String>>, String> {
    let Some(value) = root.get("test_packages") else {
        return Ok(BTreeMap::new());
    };
    let table = value
        .as_table()
        .ok_or_else(|| format!("{CONFIG}: `test_packages` must be a table"))?;
    table
        .keys()
        .map(|krate| Ok((krate.clone(), string_list(table, krate, CONFIG)?)))
        .collect()
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, String> {
        let root: toml::Table = text.parse().map_err(|e| format!("{CONFIG}: {e}"))?;
        let timeout = root
            .get("timeout_secs")
            .and_then(toml::Value::as_integer)
            .and_then(|t| u64::try_from(t).ok())
            .filter(|t| *t > 0)
            .ok_or_else(|| format!("{CONFIG}: `timeout_secs` must be a positive integer"))?;
        let crates = string_list(&root, "crates", CONFIG)?;
        if crates.is_empty() {
            return Err(format!("{CONFIG}: `crates` is empty"));
        }
        Ok(Self {
            crates,
            timeout_secs: timeout,
            exclude_re: string_list(&root, "exclude_re", CONFIG)?,
            exclude_globs: string_list(&root, "exclude_globs", CONFIG)?,
            test_packages: test_packages(&root)?,
        })
    }
}

// ---- globs ----

/// Matches a repository-relative path against a glob: `*` within a segment, `**` across segments.
pub fn glob_match(pattern: &str, path: &str) -> bool {
    fn segs(p: &[&str], s: &[&str]) -> bool {
        match p.split_first() {
            None => s.is_empty(),
            Some((&"**", rest)) => (0..=s.len()).any(|skip| segs(rest, &s[skip..])),
            Some((first, rest)) => s
                .split_first()
                .is_some_and(|(head, tail)| seg(first, head) && segs(rest, tail)),
        }
    }
    fn seg(p: &str, s: &str) -> bool {
        match p.split_once('*') {
            None => p == s,
            Some((pre, post)) => s.strip_prefix(pre).is_some_and(|rest| {
                (0..=rest.len()).any(|skip| rest.is_char_boundary(skip) && seg(post, &rest[skip..]))
            }),
        }
    }
    let p: Vec<&str> = pattern.split('/').collect();
    let s: Vec<&str> = path.split('/').collect();
    segs(&p, &s)
}

// ---- outcomes ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Outcome {
    Caught,
    Missed,
    Timeout,
    Unviable,
}

impl Outcome {
    pub fn word(self) -> &'static str {
        match self {
            Self::Caught => "caught",
            Self::Missed => "missed",
            Self::Timeout => "timeout",
            Self::Unviable => "unviable",
        }
    }
}

/// One mutant and what happened to it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Mutant {
    pub krate: String,
    pub file: String,
    /// What was changed, without file, line or column: stable across edits that move code.
    pub description: String,
    pub line: u32,
    pub outcome: Outcome,
}

/// The mutant name without its `file:line:col: ` prefix.
pub fn describe(name: &str) -> String {
    let mut rest = name;
    // `path:line:col: description`
    for _ in 0..3 {
        match rest.split_once(':') {
            Some((_, tail)) => rest = tail,
            None => return name.to_string(),
        }
    }
    rest.trim_start().to_string()
}

/// Reads cargo-mutants' `outcomes.json`; the baseline (unmutated) scenario is skipped.
pub fn read_outcomes(text: &str) -> Result<Vec<Mutant>, String> {
    let root: Value = serde_json::from_str(text).map_err(|e| format!("outcomes.json: {e}"))?;
    let mut out = Vec::new();
    for item in root["outcomes"]
        .as_array()
        .ok_or("outcomes.json has no `outcomes`")?
    {
        let Some(mutant) = item["scenario"].get("Mutant") else {
            continue;
        };
        let outcome = match item["summary"].as_str() {
            Some("CaughtMutant") => Outcome::Caught,
            Some("MissedMutant") => Outcome::Missed,
            Some("Timeout") => Outcome::Timeout,
            Some("Unviable") => Outcome::Unviable,
            other => return Err(format!("unexpected outcome {other:?}")),
        };
        out.push(Mutant {
            krate: mutant["package"].as_str().unwrap_or_default().to_string(),
            file: mutant["file"].as_str().unwrap_or_default().to_string(),
            description: describe(mutant["name"].as_str().unwrap_or_default()),
            line: mutant["span"]["start"]["line"]
                .as_u64()
                .and_then(|l| u32::try_from(l).ok())
                .unwrap_or(0),
            outcome,
        });
    }
    out.sort();
    Ok(out)
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Tally {
    pub caught: u64,
    pub missed: u64,
    pub timeout: u64,
    pub unviable: u64,
}

impl Tally {
    fn add(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Caught => self.caught += 1,
            Outcome::Missed => self.missed += 1,
            Outcome::Timeout => self.timeout += 1,
            Outcome::Unviable => self.unviable += 1,
        }
    }
    /// Caught over caught plus missed, as a percentage with two decimals; timeouts and unviable mutants
    /// are reported but do not count either way.
    pub fn score(&self) -> Option<f64> {
        let denominator = self.caught + self.missed;
        (denominator > 0)
            .then(|| (self.caught as f64 * 10_000.0 / denominator as f64).round() / 100.0)
    }
    fn json(&self) -> Value {
        json!({"caught": self.caught, "missed": self.missed, "timeout": self.timeout, "unviable": self.unviable, "score": self.score()})
    }
}

/// A crate's tally and its tallies per file.
type PerCrate<'a> = (Tally, BTreeMap<&'a str, Tally>);

/// The normalised report: counts per crate and file, and the missed and timed-out mutants with their
/// line and change. No absolute paths, timestamps or durations, so equal runs give equal reports.
pub fn report(mutants: &[Mutant]) -> Value {
    let mut crates: BTreeMap<&str, PerCrate<'_>> = BTreeMap::new();
    for m in mutants {
        let entry = crates.entry(&m.krate).or_default();
        entry.0.add(m.outcome);
        entry.1.entry(&m.file).or_default().add(m.outcome);
    }
    let mut total = Tally::default();
    mutants.iter().for_each(|m| total.add(m.outcome));
    let crates: BTreeMap<&str, Value> = crates
        .into_iter()
        .map(|(name, (tally, files))| {
            let files: BTreeMap<&str, Value> =
                files.into_iter().map(|(f, t)| (f, t.json())).collect();
            let mut value = tally.json();
            value["files"] = json!(files);
            (name, value)
        })
        .collect();
    let listed = |outcome: Outcome| -> Vec<Value> {
        mutants
            .iter()
            .filter(|m| m.outcome == outcome)
            .map(|m| json!({"crate": m.krate, "file": m.file, "line": m.line, "mutant": m.description}))
            .collect()
    };
    json!({
        "schema": 1,
        "total": total.json(),
        "crates": crates,
        "missed": listed(Outcome::Missed),
        "timeout": listed(Outcome::Timeout),
    })
}

/// Every mutant and its outcome, sorted, without line numbers: two runs on one commit must give equal text.
pub fn outcomes_text(mutants: &[Mutant]) -> String {
    let mut lines: Vec<String> = mutants
        .iter()
        .filter(|m| m.outcome != Outcome::Timeout)
        .map(|m| {
            format!(
                "{}\t{}\t{}\t{}",
                m.outcome.word(),
                m.krate,
                m.file,
                m.description
            )
        })
        .collect();
    lines.sort();
    lines.join("\n") + "\n"
}

// ---- ledger ----

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub krate: String,
    pub file: String,
    pub mutant: String,
    pub reason: String,
    pub equivalent: bool,
    pub count: u64,
}

pub fn parse_ledger(text: &str) -> Result<Vec<Entry>, String> {
    let root: toml::Table = text.parse().map_err(|e| format!("{LEDGER}: {e}"))?;
    let mut out = Vec::new();
    for (index, item) in root
        .get("survivor")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let table = item
            .as_table()
            .ok_or_else(|| format!("{LEDGER}: survivor #{index} is not a table"))?;
        let text_of = |key: &str| -> Result<String, String> {
            table
                .get(key)
                .and_then(toml::Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| format!("{LEDGER}: survivor #{index} needs `{key}`"))
        };
        let name = format!(
            "{} `{}`",
            table
                .get("file")
                .and_then(toml::Value::as_str)
                .unwrap_or("?"),
            table
                .get("mutant")
                .and_then(toml::Value::as_str)
                .unwrap_or("?")
        );
        if table.contains_key("baseline") {
            return Err(format!(
                "{LEDGER}: survivor #{index} ({name}) is a `baseline` entry; analyse it: kill it with a test or give a specific reason"
            ));
        }
        let reason = text_of("reason")?;
        if reason.trim().is_empty() {
            return Err(format!("{LEDGER}: survivor #{index} has an empty reason"));
        }
        let lower = reason.to_lowercase();
        if ["not analysed", "not analyzed", "unanalysed", "unanalyzed"]
            .iter()
            .any(|w| lower.contains(w))
        {
            return Err(format!(
                "{LEDGER}: survivor #{index} ({name}) has a reason that admits it was not analysed"
            ));
        }
        out.push(Entry {
            krate: text_of("crate")?,
            file: text_of("file")?,
            mutant: text_of("mutant")?,
            reason,
            equivalent: table
                .get("equivalent")
                .and_then(toml::Value::as_bool)
                .ok_or_else(|| {
                    format!("{LEDGER}: survivor #{index} needs `equivalent = true|false`")
                })?,
            count: table
                .get("count")
                .map_or(Ok(1), |v| {
                    v.as_integer()
                        .and_then(|c| u64::try_from(c).ok())
                        .filter(|c| *c > 0)
                        .ok_or("bad count")
                })
                .map_err(|e| format!("{LEDGER}: survivor #{index}: {e}"))?,
        });
    }
    Ok(out)
}

pub fn parse_critical(text: &str) -> Result<Vec<String>, String> {
    let root: toml::Table = text.parse().map_err(|e| format!("{CRITICAL}: {e}"))?;
    string_list(&root, "globs", CRITICAL)
}

type Key = (String, String, String);

/// Compares the missed mutants with the ledger. `scope` limits the comparison to those crates;
/// `check_stale` is false for runs that covered only part of a crate (a diff).
pub fn compare(
    ledger: &[Entry],
    mutants: &[Mutant],
    critical: &[String],
    scope: &[String],
    check_stale: bool,
) -> Vec<String> {
    let in_scope = |krate: &str| scope.iter().any(|s| s == krate);
    let is_critical = |file: &str| critical.iter().any(|g| glob_match(g, file));
    let mut errors = Vec::new();
    let mut missed: BTreeMap<Key, u64> = BTreeMap::new();
    for m in mutants.iter().filter(|m| m.outcome == Outcome::Missed) {
        *missed
            .entry((m.krate.clone(), m.file.clone(), m.description.clone()))
            .or_default() += 1;
    }
    let mut allowed: BTreeMap<Key, &Entry> = BTreeMap::new();
    for entry in ledger.iter().filter(|e| in_scope(&e.krate)) {
        let key = (
            entry.krate.clone(),
            entry.file.clone(),
            entry.mutant.clone(),
        );
        if allowed.insert(key, entry).is_some() {
            errors.push(format!(
                "ledger lists {} {} `{}` twice",
                entry.krate, entry.file, entry.mutant
            ));
        }
        if is_critical(&entry.file) && !entry.equivalent {
            errors.push(format!(
                "ledger entry in security-critical code must be an equivalent mutant (fix it with a test): {} `{}`",
                entry.file, entry.mutant
            ));
        }
    }
    for (key, count) in &missed {
        let tolerated = allowed.get(key).map_or(0, |e| e.count);
        if *count > tolerated {
            let (krate, file, mutant) = key;
            let hint = if is_critical(file) {
                "security-critical code: kill it with a test, or prove it equivalent in a ledger entry"
            } else {
                "add a test that kills it, or a ledger entry with a reason"
            };
            errors.push(format!(
                "new surviving mutant in {krate} {file}: `{mutant}` ({hint})"
            ));
        }
    }
    if check_stale {
        for (key, entry) in &allowed {
            let now = missed.get(key).copied().unwrap_or(0);
            if now < entry.count {
                let (krate, file, mutant) = key;
                errors.push(format!(
                    "stale ledger entry for {krate} {file}: `{mutant}` ({}); remove or lower its count",
                    if now == 0 { "now caught or gone" } else { "fewer survivors than listed" }
                ));
            }
        }
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

pub const HELP: &str = "\
usage: cargo xtask mutants [options]

  --package P        mutate only crate P (repeatable); default: every crate in testing/mutants.toml
  --in-diff REF      mutate only code changed since REF (the stale-entry check is skipped)
  --shard K/N        run shard K of N (cargo-mutants' sharding)
  --jobs N           parallel mutant builds (default 4)
  --out DIR          output directory (default target/mutants)
  --from-outcomes F  compare an existing cargo-mutants outcomes.json instead of running
  --no-check         report only; do not compare with the survivor ledger

Writes report.json (counts, missed and timed-out mutants) and outcomes.txt to the output directory.
Needs cargo-mutants (docs/testing.md).";

fn git_diff(root: &Path, base: &str) -> Result<String, String> {
    let output = Command::new("git")
        .args(["diff", "--no-color", base])
        .current_dir(root)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git diff {base}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn run_mutants(
    root: &Path,
    config: &Config,
    args: &[String],
    packages: &[String],
    out: &Path,
) -> Result<String, String> {
    let mut command = Command::new("cargo");
    command.arg("mutants").current_dir(root);
    command
        .args(["--no-shuffle", "--timeout"])
        .arg(config.timeout_secs.to_string());
    command
        .arg("--jobs")
        .arg(option(args, "--jobs").unwrap_or_else(|| "4".to_string()));
    command.arg("--output").arg(out);
    for package in packages {
        command.args(["--package", package]);
    }
    let mut tested: Vec<&String> = packages
        .iter()
        .filter_map(|p| config.test_packages.get(p))
        .flatten()
        .collect();
    tested.sort();
    tested.dedup();
    for package in tested {
        command.args(["--test-package", package]);
    }
    for re in &config.exclude_re {
        command.args(["--exclude-re", re]);
    }
    for glob in &config.exclude_globs {
        command.args(["--exclude", glob]);
    }
    if let Some(shard) = option(args, "--shard") {
        command.args(["--shard", &shard]);
    }
    if let Some(base) = option(args, "--in-diff") {
        let diff_path = out.join("changes.diff");
        fs::write(&diff_path, git_diff(root, &base)?).map_err(|e| e.to_string())?;
        command.arg("--in-diff").arg(diff_path);
    }
    // Exit codes 2 (missed) and 3 (timeouts) are results, not failures; the ledger judges them.
    let status = command
        .status()
        .map_err(|e| format!("cannot run cargo mutants (install it, see docs/testing.md): {e}"))?;
    match status.code() {
        Some(0 | 2 | 3) => Ok(out.join("mutants.out/outcomes.json").display().to_string()),
        _ => Err("cargo mutants failed (see above)".to_string()),
    }
}

pub fn run(root: &Path, args: &[String]) -> Result<Vec<String>, String> {
    let config = Config::parse(
        &fs::read_to_string(root.join(CONFIG)).map_err(|e| format!("{CONFIG}: {e}"))?,
    )?;
    let requested: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| *a == "--package")
        .filter_map(|(i, _)| args.get(i + 1).cloned())
        .collect();
    if let Some(unknown) = requested.iter().find(|p| !config.crates.contains(p)) {
        return Err(format!("`{unknown}` is not a mutated crate; see {CONFIG}"));
    }
    let packages = if requested.is_empty() {
        config.crates.clone()
    } else {
        requested
    };
    let out = root.join(option(args, "--out").unwrap_or_else(|| DEFAULT_OUT.to_string()));
    fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let outcomes_path = match option(args, "--from-outcomes") {
        Some(path) => path,
        None => run_mutants(root, &config, args, &packages, &out)?,
    };
    let mutants = read_outcomes(
        &fs::read_to_string(&outcomes_path).map_err(|e| format!("{outcomes_path}: {e}"))?,
    )?;
    let report = report(&mutants);
    fs::write(
        out.join("report.json"),
        serde_json::to_string_pretty(&report).unwrap_or_default() + "\n",
    )
    .map_err(|e| e.to_string())?;
    fs::write(out.join("outcomes.txt"), outcomes_text(&mutants)).map_err(|e| e.to_string())?;
    println!("{}", summary(&report));
    if args.iter().any(|a| a == "--no-check") {
        return Ok(Vec::new());
    }
    let ledger = parse_ledger(
        &fs::read_to_string(root.join(LEDGER)).map_err(|e| format!("{LEDGER}: {e}"))?,
    )?;
    let critical = parse_critical(
        &fs::read_to_string(root.join(CRITICAL)).map_err(|e| format!("{CRITICAL}: {e}"))?,
    )?;
    Ok(compare(
        &ledger,
        &mutants,
        &critical,
        &packages,
        option(args, "--in-diff").is_none() && option(args, "--shard").is_none(),
    ))
}

pub fn summary(report: &Value) -> String {
    let mut out = String::from(
        "| crate | caught | missed | timeout | unviable | score % |\n|---|---:|---:|---:|---:|---:|\n",
    );
    let cell = |v: &Value, key: &str| v[key].as_u64().unwrap_or(0).to_string();
    if let Some(crates) = report["crates"].as_object() {
        for (name, value) in crates {
            let score = value["score"]
                .as_f64()
                .map_or("n/a".to_string(), |s| format!("{s:.2}"));
            out.push_str(&format!(
                "| {name} | {} | {} | {} | {} | {score} |\n",
                cell(value, "caught"),
                cell(value, "missed"),
                cell(value, "timeout"),
                cell(value, "unviable")
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mutant(file: &str, description: &str, outcome: Outcome) -> Mutant {
        Mutant {
            krate: "c".into(),
            file: file.into(),
            description: description.into(),
            line: 1,
            outcome,
        }
    }

    fn entry(file: &str, mutant: &str, equivalent: bool) -> Entry {
        Entry {
            krate: "c".into(),
            file: file.into(),
            mutant: mutant.into(),
            reason: "why".into(),
            equivalent,
            count: 1,
        }
    }

    fn scope() -> Vec<String> {
        vec!["c".to_string()]
    }

    #[test]
    fn names_lose_their_position() {
        assert_eq!(
            describe("crates/a/src/x.rs:43:9: replace f -> bool with true"),
            "replace f -> bool with true"
        );
        assert_eq!(
            describe("crates/a/src/x.rs:43:9: replace == with != in g"),
            "replace == with != in g"
        );
    }

    #[test]
    fn globs_match_segments_and_depth() {
        assert!(glob_match("crates/a/src/x.rs", "crates/a/src/x.rs"));
        assert!(glob_match(
            "crates/a/src/types/**",
            "crates/a/src/types/atom.rs"
        ));
        assert!(glob_match(
            "crates/a/src/types/**",
            "crates/a/src/types/deep/x.rs"
        ));
        assert!(glob_match("**/tests/**", "crates/a/tests/t.rs"));
        assert!(glob_match("crates/*/src/lib.rs", "crates/a/src/lib.rs"));
        assert!(!glob_match("crates/*/src/lib.rs", "crates/a/b/src/lib.rs"));
        assert!(!glob_match("crates/a/src/x.rs", "crates/a/src/y.rs"));
        assert!(glob_match("*.rs", "x.rs"));
        assert!(!glob_match("*.rs", "x.rss"));
    }

    #[test]
    fn a_survivor_with_no_entry_fails_and_is_named() {
        let mutants = [mutant("src/a.rs", "replace f with 0", Outcome::Missed)];
        let errors = compare(&[], &mutants, &[], &scope(), true);
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].contains("new surviving mutant") && errors[0].contains("replace f with 0"),
            "{errors:?}"
        );
    }

    #[test]
    fn a_tolerated_survivor_passes_and_caught_mutants_are_ignored() {
        let mutants = [
            mutant("src/a.rs", "m", Outcome::Missed),
            mutant("src/a.rs", "n", Outcome::Caught),
        ];
        assert!(
            compare(
                &[entry("src/a.rs", "m", false)],
                &mutants,
                &[],
                &scope(),
                true
            )
            .is_empty()
        );
    }

    #[test]
    fn an_entry_whose_mutant_is_now_caught_is_stale() {
        let mutants = [mutant("src/a.rs", "m", Outcome::Caught)];
        let errors = compare(
            &[entry("src/a.rs", "m", false)],
            &mutants,
            &[],
            &scope(),
            true,
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("stale"), "{errors:?}");
        // A partial run (a diff or a shard) cannot judge staleness.
        assert!(
            compare(
                &[entry("src/a.rs", "m", false)],
                &mutants,
                &[],
                &scope(),
                false
            )
            .is_empty()
        );
    }

    #[test]
    fn counts_allow_repeated_identical_survivors_and_go_stale_when_fewer_remain() {
        let two = [
            mutant("src/a.rs", "m", Outcome::Missed),
            mutant("src/a.rs", "m", Outcome::Missed),
        ];
        let mut e = entry("src/a.rs", "m", false);
        assert_eq!(
            compare(std::slice::from_ref(&e), &two, &[], &scope(), true).len(),
            1
        );
        e.count = 2;
        assert!(compare(std::slice::from_ref(&e), &two, &[], &scope(), true).is_empty());
        assert_eq!(compare(&[e], &two[..1], &[], &scope(), true).len(), 1);
    }

    #[test]
    fn a_non_equivalent_entry_in_critical_code_is_refused() {
        let critical = vec!["src/sev*.rs".to_string()];
        let mutants = [mutant("src/severity.rs", "m", Outcome::Missed)];
        let errors = compare(
            &[entry("src/severity.rs", "m", false)],
            &mutants,
            &critical,
            &scope(),
            true,
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("security-critical"), "{errors:?}");
        assert!(
            compare(
                &[entry("src/severity.rs", "m", true)],
                &mutants,
                &critical,
                &scope(),
                true
            )
            .is_empty()
        );
    }

    #[test]
    fn a_baseline_entry_is_refused() {
        let text = "[[survivor]]\ncrate=\"c\"\nfile=\"f\"\nmutant=\"m\"\nreason=\"why\"\nequivalent=false\nbaseline=true\n";
        let error = parse_ledger(text).unwrap_err();
        assert!(
            error.contains("baseline") && error.contains("`m`"),
            "{error}"
        );
    }

    #[test]
    fn a_reason_that_admits_no_analysis_is_refused() {
        let text = "[[survivor]]\ncrate=\"c\"\nfile=\"f\"\nmutant=\"m\"\nreason=\"Not analysed yet\"\nequivalent=false\n";
        let error = parse_ledger(text).unwrap_err();
        assert!(
            error.contains("not analysed") && error.contains("`m`"),
            "{error}"
        );
    }

    #[test]
    fn a_new_survivor_in_critical_code_says_to_kill_it() {
        let critical = vec!["src/sev*.rs".to_string()];
        let errors = compare(
            &[],
            &[mutant("src/severity.rs", "m", Outcome::Missed)],
            &critical,
            &scope(),
            true,
        );
        assert!(errors[0].contains("kill it with a test"), "{errors:?}");
    }

    #[test]
    fn entries_of_crates_outside_the_scope_are_not_judged() {
        let mut other = entry("src/a.rs", "m", false);
        other.krate = "d".into();
        assert!(compare(&[other], &[], &[], &scope(), true).is_empty());
    }

    #[test]
    fn the_report_counts_per_crate_and_file_and_lists_missed_mutants() {
        let mutants = [
            mutant("src/a.rs", "m1", Outcome::Missed),
            mutant("src/a.rs", "m2", Outcome::Caught),
            mutant("src/b.rs", "m3", Outcome::Caught),
            mutant("src/b.rs", "m4", Outcome::Unviable),
            mutant("src/b.rs", "m5", Outcome::Timeout),
        ];
        let r = report(&mutants);
        assert_eq!(r["crates"]["c"]["caught"], 2);
        assert_eq!(r["crates"]["c"]["missed"], 1);
        assert_eq!(r["crates"]["c"]["timeout"], 1);
        assert_eq!(r["crates"]["c"]["unviable"], 1);
        assert_eq!(r["crates"]["c"]["score"], 66.67);
        assert_eq!(r["crates"]["c"]["files"]["src/a.rs"]["missed"], 1);
        assert_eq!(r["missed"][0]["mutant"], "m1");
        assert_eq!(r["missed"][0]["line"], 1);
        assert!(!r.to_string().contains("/home/"));
    }

    #[test]
    fn outcomes_are_read_from_cargo_mutants_json() {
        let json = r#"{"outcomes":[
          {"scenario":"Baseline","summary":"Success"},
          {"scenario":{"Mutant":{"name":"crates/a/src/x.rs:4:9: replace f -> bool with true","package":"a","file":"crates/a/src/x.rs","span":{"start":{"line":4,"column":9}}}},"summary":"MissedMutant"},
          {"scenario":{"Mutant":{"name":"crates/a/src/x.rs:9:9: replace g -> u8 with 0","package":"a","file":"crates/a/src/x.rs","span":{"start":{"line":9,"column":9}}}},"summary":"CaughtMutant"}]}"#;
        let mutants = read_outcomes(json).unwrap();
        assert_eq!(mutants.len(), 2);
        assert_eq!(mutants[0].description, "replace f -> bool with true");
        assert_eq!(mutants[0].outcome, Outcome::Missed);
        assert_eq!(mutants[0].line, 4);
    }

    #[test]
    fn the_outcome_text_is_sorted_and_free_of_line_numbers() {
        let text = outcomes_text(&[
            mutant("b.rs", "z", Outcome::Caught),
            mutant("a.rs", "y", Outcome::Missed),
        ]);
        assert_eq!(text, "caught\tc\tb.rs\tz\nmissed\tc\ta.rs\ty\n");
    }

    #[test]
    fn the_ledger_requires_reasons_and_the_equivalent_flag() {
        let ok =
            "[[survivor]]\ncrate=\"c\"\nfile=\"f\"\nmutant=\"m\"\nreason=\"r\"\nequivalent=true\n";
        assert_eq!(parse_ledger(ok).unwrap().len(), 1);
        assert!(parse_ledger("[[survivor]]\ncrate=\"c\"\nfile=\"f\"\nmutant=\"m\"\nreason=\" \"\nequivalent=true\n").is_err());
        assert!(
            parse_ledger("[[survivor]]\ncrate=\"c\"\nfile=\"f\"\nmutant=\"m\"\nreason=\"r\"\n")
                .is_err()
        );
    }

    #[test]
    fn the_committed_configuration_parses() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let read = |name: &str| fs::read_to_string(root.join(name)).unwrap();
        assert!(!Config::parse(&read(CONFIG)).unwrap().crates.is_empty());
        assert!(!parse_critical(&read(CRITICAL)).unwrap().is_empty());
    }

    #[test]
    fn test_packages_are_read_per_crate() {
        let text = "crates = [\"a\"]\ntimeout_secs = 5\n[test_packages]\na = [\"a\", \"b\"]\n";
        let config = Config::parse(text).unwrap();
        assert_eq!(config.test_packages["a"], ["a", "b"]);
        assert!(
            Config::parse("crates = [\"a\"]\ntimeout_secs = 5\n")
                .unwrap()
                .test_packages
                .is_empty()
        );
        assert!(Config::parse("crates = [\"a\"]\ntimeout_secs = 5\ntest_packages = 3\n").is_err());
    }
}
