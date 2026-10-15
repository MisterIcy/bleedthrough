//! A directory of TOML rule files as a [`RuleSource`], the content hash of a loaded rule set and
//! the `rules check` entry point.

use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};

use bleedthrough_rules::{LocatedError, RuleSet, RuleSource, SourceOutput, load_rule_set};
use sha2::{Digest, Sha256};

use crate::toml_rules::read_file;

/// The file listing identifiers that can never be reissued, inside the rules directory.
pub const RETIRED_FILE: &str = "RETIRED.md";

/// Term: exempt
///
/// A rule set loaded from files, with its content hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedRules {
    /// The validated rules, ordered by identifier.
    pub set: RuleSet,
    /// SHA-256 of the canonical content, as lowercase hex; see [`content_hash`].
    pub hash: String,
}

/// Term: Rule
///
/// A directory tree of `*.toml` rule files, read in path order, plus its `RETIRED.md`.
#[derive(Debug, Clone)]
pub struct TomlRuleDir {
    root: PathBuf,
}

impl TomlRuleDir {
    /// A source reading `root` recursively.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn files(&self, dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                self.files(&path, out)?;
            } else if path.extension().is_some_and(|e| e == "toml") {
                out.push(path);
            }
        }
        Ok(())
    }

    /// The name used in messages: the path below the root with `/` separators.
    fn name(&self, path: &Path) -> String {
        let rel = path.strip_prefix(&self.root).unwrap_or(path);
        rel.components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
    }

    fn retired(&self) -> Vec<String> {
        let Ok(text) = std::fs::read_to_string(self.root.join(RETIRED_FILE)) else {
            return vec![];
        };
        text.lines()
            .filter_map(|l| l.trim().strip_prefix("- "))
            .filter_map(|l| l.split_whitespace().next())
            .map(|id| id.trim_matches('`').to_string())
            .collect()
    }
}

impl RuleSource for TomlRuleDir {
    fn load(&self) -> SourceOutput {
        let mut out = SourceOutput {
            retired: self.retired(),
            ..SourceOutput::default()
        };
        let mut files = Vec::new();
        if let Err(e) = self.files(&self.root, &mut files) {
            let name = self.root.to_string_lossy().into_owned();
            out.errors.push(LocatedError::new(
                name,
                None,
                None,
                format!("cannot read the rules directory: {e}"),
            ));
            return out;
        }
        for path in files {
            let name = self.name(&path);
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    let read = read_file(&name, &text);
                    out.rules.extend(read.rules);
                    out.errors.extend(read.errors);
                }
                Err(e) => out.errors.push(LocatedError::new(
                    name,
                    None,
                    None,
                    format!("cannot read the file: {e}"),
                )),
            }
        }
        out
    }
}

/// SHA-256 over the canonical form of the validated rule set, as lowercase hex. It depends on
/// rule content only: not on paths, comments, key order or file enumeration order.
pub fn content_hash(set: &RuleSet) -> String {
    let digest = Sha256::digest(set.canonical().as_bytes());
    digest.iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Loads, validates and hashes the rules below `root`.
pub fn load_rules(root: &Path) -> Result<LoadedRules, Vec<LocatedError>> {
    let set = load_rule_set(&TomlRuleDir::new(root))?;
    let hash = content_hash(&set);
    Ok(LoadedRules { set, hash })
}

/// Term: exempt
///
/// The outcome of `rules check`. The command-line front-end maps it to the process exit code by
/// the project's exit-code scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CheckStatus {
    /// Every rule file is valid.
    Valid,
    /// The rules cannot be used: the tool-error status.
    ToolError,
}

/// The `rules check` library entry point: loads and validates `root` without analysing any code
/// and writes the report to `out` (errors one per line as `file:line: key: message`, or a one-line
/// summary with the hash).
pub fn check_rules(root: &Path, out: &mut dyn io::Write) -> io::Result<CheckStatus> {
    match load_rules(root) {
        Ok(loaded) => {
            writeln!(
                out,
                "{} rule(s) valid, hash {}",
                loaded.set.len(),
                loaded.hash
            )?;
            Ok(CheckStatus::Valid)
        }
        Err(errors) => {
            for e in &errors {
                writeln!(out, "{e}")?;
            }
            writeln!(
                out,
                "{} error(s) in the rules at {}",
                errors.len(),
                root.display()
            )?;
            Ok(CheckStatus::ToolError)
        }
    }
}
