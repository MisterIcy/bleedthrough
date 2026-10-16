//! The real-world corpus: the pinned project list and its checker, hash-verified fetching with safe
//! extraction, and the parse-failure budget.
//!
//! Archives are untrusted data: they are hashed before they are read, extracted without trusting any
//! path in them, never executed, and never committed.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

/// The size tiers a project can be in.
pub const TIERS: [&str; 3] = ["small", "medium", "large"];
/// The code styles the list must mix.
pub const STYLES: [&str; 3] = ["framework", "non-framework", "legacy"];

/// The most bytes and entries one archive may expand to; a guard against decompression bombs.
const MAX_TOTAL_BYTES: u64 = 4 << 30;
const MAX_ENTRIES: u64 = 500_000;

/// One pinned project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// Short identifier, used in the cache and in the budget.
    pub id: String,
    /// `https://github.com/<owner>/<repo>`.
    pub repository: String,
    /// The exact commit, 40 hex digits.
    pub commit: String,
    /// SHA-256 of the archive of that commit.
    pub sha256: String,
    /// SPDX identifier of the project's licence.
    pub licence: String,
    /// PHP versions the project targets.
    pub php: Vec<String>,
    /// `small`, `medium` or `large`.
    pub tier: String,
    /// `framework`, `non-framework` or `legacy`.
    pub style: String,
    /// Why the project was chosen.
    pub reason: String,
}

impl Project {
    /// Where the archive of the pinned commit is downloaded from.
    pub fn archive_url(&self) -> String {
        let path = self
            .repository
            .trim_end_matches('/')
            .trim_start_matches("https://github.com/");
        format!("https://codeload.github.com/{path}/tar.gz/{}", self.commit)
    }
}

/// The parsed list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
    /// The projects, in file order.
    pub projects: Vec<Project>,
}

fn text(table: &toml::Table, key: &str) -> String {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

impl List {
    /// Parses `corpora/real-world.toml`. Missing fields parse as empty and are reported by [`List::check`],
    /// which names the project.
    pub fn parse(source: &str) -> Result<Self, String> {
        let root: toml::Table = source
            .parse()
            .map_err(|e| format!("real-world list: {e}"))?;
        let items = root
            .get("project")
            .and_then(toml::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let projects = items
            .iter()
            .filter_map(toml::Value::as_table)
            .map(|t| Project {
                id: text(t, "id"),
                repository: text(t, "repository"),
                commit: text(t, "commit"),
                sha256: text(t, "sha256"),
                licence: text(t, "licence"),
                php: t
                    .get("php")
                    .and_then(toml::Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
                tier: text(t, "tier"),
                style: text(t, "style"),
                reason: text(t, "reason"),
            })
            .collect();
        Ok(Self { projects })
    }

    /// Every problem with the list: unpinned or unrecorded projects (named), duplicates, and missing
    /// tier or style coverage.
    pub fn check(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let mut seen = BTreeSet::new();
        for p in &self.projects {
            if !seen.insert(p.id.clone()) {
                errors.push(format!("project `{}` is listed twice", p.id));
            }
            errors.extend(p.check());
        }
        for tier in TIERS {
            if !self.projects.iter().any(|p| p.tier == tier) {
                errors.push(format!("no project in the `{tier}` tier"));
            }
        }
        for style in STYLES {
            if !self.projects.iter().any(|p| p.style == style) {
                errors.push(format!("no `{style}` project"));
            }
        }
        errors
    }
}

impl Project {
    /// The problems with this one entry, each naming the project.
    fn check(&self) -> Vec<String> {
        let name = if self.id.is_empty() {
            "<no id>"
        } else {
            &self.id
        };
        let hex = |s: &str, len: usize| s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit());
        let mut errors = Vec::new();
        if !hex(&self.commit, 40) {
            errors.push(format!(
                "project `{name}` has no exact commit (40 hex digits)"
            ));
        }
        if !hex(&self.sha256, 64) {
            errors.push(format!(
                "project `{name}` has no archive SHA-256 (64 hex digits)"
            ));
        }
        for (field, value) in [
            ("repository", &self.repository),
            ("licence", &self.licence),
            ("reason", &self.reason),
        ] {
            if value.is_empty() {
                errors.push(format!("project `{name}` has no {field}"));
            }
        }
        if !self.repository.starts_with("https://github.com/") {
            errors.push(format!(
                "project `{name}`: repository must be an https://github.com/ URL"
            ));
        }
        if self.php.is_empty() {
            errors.push(format!("project `{name}` records no PHP versions"));
        }
        if !TIERS.contains(&self.tier.as_str()) {
            errors.push(format!(
                "project `{name}` has tier `{}`; expected one of {TIERS:?}",
                self.tier
            ));
        }
        if !STYLES.contains(&self.style.as_str()) {
            errors.push(format!(
                "project `{name}` has style `{}`; expected one of {STYLES:?}",
                self.style
            ));
        }
        errors
    }
}

/// The lower-case hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut s, b| {
            use std::fmt::Write as _;
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Why an archive was refused.
fn refuse(project: &str, why: impl std::fmt::Display) -> String {
    format!("project `{project}`: {why}")
}

/// A relative path with no `..`, root or prefix components, with the archive's top-level directory
/// removed (GitHub archives wrap the tree in `<repo>-<commit>/`).
fn safe_relative(path: &Path) -> Option<PathBuf> {
    let mut parts = path.components();
    let mut out = PathBuf::new();
    let first = parts.next()?;
    if !matches!(first, Component::Normal(_)) {
        return None;
    }
    for part in parts {
        match part {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out)
}

/// Whether a symlink at `link` (relative to the tree root) pointing at `target` stays inside the tree.
fn link_stays_inside(link: &Path, target: &Path) -> bool {
    if target.is_absolute() {
        return false;
    }
    let mut depth = link.parent().map_or(0, |p| p.components().count()) as i64;
    for part in target.components() {
        match part {
            Component::Normal(_) => depth += 1,
            Component::ParentDir => depth -= 1,
            Component::CurDir => {}
            _ => return false,
        }
        if depth < 0 {
            return false;
        }
    }
    true
}

/// Extracts a gzip-compressed tar archive into `dest` (which must not exist), refusing anything that
/// could write outside it: absolute or `..` paths, hard links, device nodes, and symlinks whose target
/// leaves the tree. Nothing is created in `dest` before the whole archive has been checked.
pub fn extract(project: &str, archive: &[u8], dest: &Path) -> Result<u64, String> {
    // Pass 1 validates every entry; pass 2 writes. A refused archive leaves nothing behind.
    let mut entries = 0u64;
    let mut total = 0u64;
    for entry in tar::Archive::new(flate2::read::GzDecoder::new(archive))
        .entries()
        .map_err(|e| refuse(project, e))?
    {
        let entry = entry.map_err(|e| refuse(project, e))?;
        entries += 1;
        total += entry.header().size().unwrap_or(0);
        if entries > MAX_ENTRIES || total > MAX_TOTAL_BYTES {
            return Err(refuse(project, "archive expands beyond the size limits"));
        }
        let raw = entry.path_bytes().into_owned();
        let name = String::from_utf8_lossy(&raw).into_owned();
        let kind = entry.header().entry_type();
        if kind.is_pax_global_extensions() || kind.is_pax_local_extensions() {
            continue;
        }
        let Some(relative) = safe_relative(Path::new(&name)) else {
            return Err(refuse(
                project,
                format!("unsafe path `{name}` in the archive"),
            ));
        };
        match kind {
            t if t.is_dir() || t.is_file() => {}
            t if t.is_symlink() => {
                let target = entry
                    .link_name()
                    .map_err(|e| refuse(project, e))?
                    .unwrap_or_default();
                if !link_stays_inside(&relative, &target) {
                    return Err(refuse(
                        project,
                        format!("symlink `{name}` points outside the tree"),
                    ));
                }
            }
            _ => {
                return Err(refuse(
                    project,
                    format!("entry `{name}` is neither a file, a directory nor a safe symlink"),
                ));
            }
        }
    }
    fs::create_dir_all(dest).map_err(|e| refuse(project, format!("{}: {e}", dest.display())))?;
    let mut files = 0u64;
    for entry in tar::Archive::new(flate2::read::GzDecoder::new(archive))
        .entries()
        .map_err(|e| refuse(project, e))?
    {
        let mut entry = entry.map_err(|e| refuse(project, e))?;
        let kind = entry.header().entry_type();
        let name = String::from_utf8_lossy(&entry.path_bytes()).into_owned();
        let Some(relative) = safe_relative(Path::new(&name)) else {
            continue;
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = dest.join(&relative);
        if kind.is_dir() {
            fs::create_dir_all(&target).map_err(|e| refuse(project, e))?;
        } else if kind.is_file() {
            fs::create_dir_all(target.parent().unwrap_or(dest)).map_err(|e| refuse(project, e))?;
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|e| refuse(project, e))?;
            fs::write(&target, bytes).map_err(|e| refuse(project, e))?;
            files += 1;
        }
        // Symlinks that passed the check are not recreated: the corpus is only ever read as source
        // files, and a link is not a file to parse.
    }
    Ok(files)
}

/// The cache directory: `$BLEEDTHROUGH_CORPUS_DIR`, else `<root>/target/corpus`.
pub fn cache_dir(root: &Path) -> PathBuf {
    std::env::var_os("BLEEDTHROUGH_CORPUS_DIR")
        .map_or_else(|| root.join("target").join("corpus"), PathBuf::from)
}

/// Where a project's verified tree lives inside the cache.
pub fn tree_dir(cache: &Path, project: &Project) -> PathBuf {
    cache.join(&project.id).join("tree")
}

fn marker(cache: &Path, project: &Project) -> PathBuf {
    cache.join(&project.id).join("VERIFIED")
}

/// Whether the project's tree is in the cache and was verified against the pinned hash.
pub fn is_fetched(cache: &Path, project: &Project) -> bool {
    fs::read_to_string(marker(cache, project)).is_ok_and(|m| m.trim() == project.sha256)
        && tree_dir(cache, project).is_dir()
}

/// Verifies `archive` against the pin and extracts it into the cache. A mismatch fails naming the
/// project and both hashes, and leaves anything already verified untouched.
pub fn install(cache: &Path, project: &Project, archive: &[u8]) -> Result<u64, String> {
    let actual = sha256_hex(archive);
    if actual != project.sha256 {
        return Err(refuse(
            &project.id,
            format!(
                "archive hash mismatch: expected {}, got {actual}; the archive was not used",
                project.sha256
            ),
        ));
    }
    let home = cache.join(&project.id);
    let staging = cache.join(format!("{}.staging", project.id));
    let _ = fs::remove_dir_all(&staging);
    let files = extract(&project.id, archive, &staging.join("tree"));
    let files = match files {
        Ok(n) => n,
        Err(e) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
    };
    fs::write(staging.join("VERIFIED"), format!("{}\n", project.sha256))
        .map_err(|e| refuse(&project.id, e))?;
    let _ = fs::remove_dir_all(&home);
    fs::rename(&staging, &home)
        .map_err(|e| refuse(&project.id, format!("{}: {e}", home.display())))?;
    Ok(files)
}

/// Every `.php` file below `dir`, as `/`-separated paths relative to it, in sorted order.
pub fn php_files(dir: &Path) -> Result<Vec<String>, String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
        let mut paths: Vec<PathBuf> = fs::read_dir(dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        paths.sort();
        for path in paths {
            let meta =
                fs::symlink_metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            if meta.is_dir() {
                walk(root, &path, out)?;
            } else if meta.is_file() && path.extension().is_some_and(|e| e == "php") {
                let relative = path.strip_prefix(root).unwrap_or(&path);
                out.push(
                    relative
                        .components()
                        .map(|c| c.as_os_str().to_string_lossy())
                        .collect::<Vec<_>>()
                        .join("/"),
                );
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out)?;
    Ok(out)
}

/// What the parser did with a project: the files it recovered from, and the files it failed on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Observed {
    /// Files with parse errors but a usable tree.
    pub recovered: Vec<String>,
    /// Files with no usable tree.
    pub failed: Vec<String>,
}

/// A recorded budget for one project.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Budget {
    /// Files the parser may recover from.
    pub recovered: u64,
    /// Files the parser may fail on.
    pub failed: u64,
    /// Files real PHP rejects, with the interpreter's verdict as the reason.
    pub excluded: BTreeMap<String, String>,
}

/// The committed budgets, one per project id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Budgets {
    /// Budgets by project id.
    pub projects: BTreeMap<String, Budget>,
}

impl Budgets {
    /// Parses `corpora/parse-budget.toml`.
    pub fn parse(source: &str) -> Result<Self, String> {
        let root: toml::Table = source.parse().map_err(|e| format!("parse budget: {e}"))?;
        let mut projects = BTreeMap::new();
        for (id, value) in root
            .get("project")
            .and_then(toml::Value::as_table)
            .cloned()
            .unwrap_or_default()
        {
            let table = value
                .as_table()
                .ok_or_else(|| format!("budget for `{id}` is not a table"))?;
            let number = |key: &str| {
                table
                    .get(key)
                    .and_then(toml::Value::as_integer)
                    .and_then(|n| u64::try_from(n).ok())
                    .ok_or_else(|| format!("budget for `{id}` needs a non-negative `{key}`"))
            };
            let excluded = table
                .get("excluded")
                .and_then(toml::Value::as_table)
                .map(|t| {
                    t.iter()
                        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                        .collect()
                })
                .unwrap_or_default();
            projects.insert(
                id.clone(),
                Budget {
                    recovered: number("recovered")?,
                    failed: number("failed")?,
                    excluded,
                },
            );
        }
        Ok(Self { projects })
    }

    /// The canonical text of the budgets.
    pub fn render(&self) -> String {
        let mut out = String::from(
            "# Parse-failure budget per real-world project: files the parser recovered from and files it\n\
             # failed on, excluding files real PHP rejects (with the verdict as the reason).\n\
             # A run fails above the budget and below it; lower it deliberately with\n\
             # `cargo xtask parse-budget --update` and review the printed diff.\n",
        );
        for (id, b) in &self.projects {
            out.push_str(&format!(
                "\n[project.{id}]\nrecovered = {}\nfailed = {}\n",
                b.recovered, b.failed
            ));
            if !b.excluded.is_empty() {
                out.push_str(&format!("\n[project.{id}.excluded]\n"));
                for (file, reason) in &b.excluded {
                    let quote =
                        |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
                    out.push_str(&format!("{} = {}\n", quote(file), quote(reason)));
                }
            }
        }
        out
    }

    /// Compares what the parser did with the budget for `id`. Files in the budget's exclusions are
    /// removed from the observation first. A regression and an improvement both fail, each listing the
    /// files in that category.
    pub fn check(&self, id: &str, observed: &Observed) -> Vec<String> {
        let Some(budget) = self.projects.get(id) else {
            return vec![format!(
                "project `{id}` has no parse budget; record one with `--update`"
            )];
        };
        let kept = |files: &[String]| -> Vec<String> {
            files
                .iter()
                .filter(|f| !budget.excluded.contains_key(*f))
                .cloned()
                .collect()
        };
        let mut errors = Vec::new();
        for (kind, now, allowed) in [
            ("recovered", kept(&observed.recovered), budget.recovered),
            ("failed", kept(&observed.failed), budget.failed),
        ] {
            let count = now.len() as u64;
            if count == allowed {
                continue;
            }
            let (verb, hint) = if count > allowed {
                ("regressed", "")
            } else {
                (
                    "improved",
                    "; lower the budget deliberately with `--update`",
                )
            };
            errors.push(format!(
                "project `{id}`: files {kind} {verb}: budget {allowed}, now {count}{hint}\n  files {kind}:{}",
                now.iter().fold(String::new(), |s, f| s + "\n    " + f)
            ));
        }
        errors
    }
}
