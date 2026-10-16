//! The file-system view the Composer reader needs, so rules and locating run without a disk in
//! tests. Paths are project-relative with `/` separators, as everywhere in this module.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Term: exempt
///
/// One entry of a directory listing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct DirEntry {
    /// The entry's name, without its directory.
    pub name: String,
    /// Whether it is a directory.
    pub is_dir: bool,
}

/// Term: exempt
///
/// The file operations the Composer reader performs.
pub trait Fs {
    /// The text of the file, or why it cannot be read.
    fn read_to_string(&self, path: &str) -> Result<String, String>;
    /// Whether `path` is an existing regular file.
    fn is_file(&self, path: &str) -> bool;
    /// The entries of the directory (any order), or `None` if `path` is not a directory.
    fn list_dir(&self, path: &str) -> Option<Vec<DirEntry>>;
    /// Whether the real location of `path` (after symlinks) lies inside the project root.
    fn stays_inside(&self, path: &str) -> bool;
}

/// Term: exempt
///
/// The real file system under a project root.
#[derive(Debug, Clone)]
pub struct OsFs {
    root: PathBuf,
    canonical_root: PathBuf,
}

impl OsFs {
    /// A view of the directory `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let canonical_root = root.canonicalize().unwrap_or_else(|_| root.clone());
        Self {
            root,
            canonical_root,
        }
    }

    fn resolve(&self, path: &str) -> PathBuf {
        if path.is_empty() {
            self.root.clone()
        } else {
            self.root.join(path)
        }
    }
}

impl Fs for OsFs {
    fn read_to_string(&self, path: &str) -> Result<String, String> {
        std::fs::read_to_string(self.resolve(path)).map_err(|e| e.to_string())
    }

    fn is_file(&self, path: &str) -> bool {
        self.resolve(path).is_file()
    }

    fn list_dir(&self, path: &str) -> Option<Vec<DirEntry>> {
        let entries = std::fs::read_dir(self.resolve(path)).ok()?;
        Some(
            entries
                .filter_map(Result::ok)
                .map(|e| DirEntry {
                    name: e.file_name().to_string_lossy().into_owned(),
                    is_dir: e.path().is_dir(),
                })
                .collect(),
        )
    }

    fn stays_inside(&self, path: &str) -> bool {
        match self.resolve(path).canonicalize() {
            Ok(real) => real.starts_with(&self.canonical_root),
            // Nothing there to escape through.
            Err(_) => true,
        }
    }
}

/// Term: exempt
///
/// An in-memory file system for tests and previews.
#[derive(Debug, Clone, Default)]
pub struct MemFs {
    files: BTreeMap<String, String>,
    outside: BTreeSet<String>,
}

impl MemFs {
    /// An empty file system.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a file; its directories exist implicitly.
    #[must_use]
    pub fn with_file(mut self, path: &str, contents: &str) -> Self {
        self.files.insert(path.to_string(), contents.to_string());
        self
    }

    /// Marks `path` (a file or a directory prefix) as a symlink to somewhere outside the project.
    #[must_use]
    pub fn with_escaping_link(mut self, path: &str) -> Self {
        self.outside.insert(path.to_string());
        self
    }
}

impl Fs for MemFs {
    fn read_to_string(&self, path: &str) -> Result<String, String> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| format!("{path}: no such file"))
    }

    fn is_file(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    fn list_dir(&self, path: &str) -> Option<Vec<DirEntry>> {
        let prefix = if path.is_empty() {
            String::new()
        } else {
            format!("{path}/")
        };
        let mut found: BTreeMap<String, bool> = BTreeMap::new();
        for file in self.files.keys() {
            if let Some(rest) = file.strip_prefix(&prefix) {
                match rest.split_once('/') {
                    Some((dir, _)) => {
                        found.insert(dir.to_string(), true);
                    }
                    None => {
                        found.entry(rest.to_string()).or_insert(false);
                    }
                }
            }
        }
        (!found.is_empty()).then(|| {
            // Deliberately reversed: callers must not depend on enumeration order.
            found
                .into_iter()
                .rev()
                .map(|(name, is_dir)| DirEntry { name, is_dir })
                .collect()
        })
    }

    fn stays_inside(&self, path: &str) -> bool {
        !self
            .outside
            .iter()
            .any(|o| path == o || path.starts_with(&format!("{o}/")))
    }
}

/// Joins `base` and `relative` and removes `.`, `..` and empty segments. `None` when the result
/// would leave the project root or `relative` is absolute.
pub fn normalise(base: &str, relative: &str) -> Option<String> {
    if relative.starts_with('/') || Path::new(relative).has_root() {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for segment in base.split('/').chain(relative.split('/')) {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}
