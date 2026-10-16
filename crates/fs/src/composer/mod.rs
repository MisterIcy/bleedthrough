//! Composer's autoload configuration: `composer.json` and `composer.lock` read into rules, a
//! class locator, the locked package list and the `files` entries. The optimised autoloader files
//! under `vendor/composer/` are not parsed: the JSON files are the source of truth.

mod files;
mod map;
mod rules;
mod scan;

use serde_json::Value;

pub use files::{DirEntry, Fs, MemFs, OsFs, normalise};
pub use map::{AutoloadMap, ComposerLocator};
pub use rules::{AutoloadRules, PrefixRule, read_rules};

/// Term: Symbol table
///
/// A package of `composer.lock`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct LockedPackage {
    /// The package name, `vendor/name`.
    pub name: String,
    /// The locked version.
    pub version: String,
    /// The source or dist reference (a commit hash), when the lock has one.
    pub reference: Option<String>,
    /// Whether it comes from `packages-dev`.
    pub dev: bool,
}

/// Term: Symbol table
///
/// A problem reading Composer's files; never fatal.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct ComposerDiagnostic {
    /// The file the problem concerns (`composer.json`, `composer.lock` or a scanned path).
    pub file: String,
    /// What is wrong.
    pub message: String,
}

/// Term: Symbol table
///
/// What Composer's files say about a project.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ComposerProject {
    map: AutoloadMap,
    packages: Vec<LockedPackage>,
    autoload_files: Vec<String>,
    diagnostics: Vec<ComposerDiagnostic>,
}

impl ComposerProject {
    /// Reads `composer.json` and `composer.lock` at the root of `fs`. Missing or malformed files
    /// give diagnostics and contribute no rules.
    pub fn read(fs: &dyn Fs) -> Self {
        let mut diagnostics = Vec::new();
        let mut problem = |file: &str, message: String| {
            diagnostics.push(ComposerDiagnostic {
                file: file.to_string(),
                message,
            });
        };
        let manifest = load_json(fs, "composer.json", &mut problem);
        let lock = load_json(fs, "composer.lock", &mut problem);
        let vendor = manifest
            .as_ref()
            .and_then(|m| m.pointer("/config/vendor-dir"))
            .and_then(Value::as_str)
            .and_then(|dir| normalise("", dir))
            .filter(|dir| !dir.is_empty())
            .unwrap_or_else(|| "vendor".to_string());

        let mut sources: Vec<AutoloadRules> = Vec::new();
        let mut collect = |value: &Value, base: &str, file: &str| {
            let mut found = Vec::new();
            let rules = read_rules(value, base, &mut found);
            for message in found {
                problem(file, message);
            }
            sources.push(rules);
        };
        if let Some(manifest) = &manifest {
            collect(manifest, "", "composer.json");
        }
        let packages = lock
            .as_ref()
            .map(|l| locked_packages(l, &vendor, &mut collect))
            .unwrap_or_default();

        let mut scan_problems = Vec::new();
        let map = AutoloadMap::build(&sources, fs, &mut scan_problems);
        let mut autoload_files = Vec::new();
        for path in sources.iter().flat_map(|s| s.files.iter()) {
            if fs.is_file(path) && fs.stays_inside(path) {
                if !autoload_files.contains(path) {
                    autoload_files.push(path.clone());
                }
            } else {
                scan_problems.push(format!("the autoload file `{path}` does not exist"));
            }
        }
        diagnostics.extend(scan_problems.into_iter().map(|message| ComposerDiagnostic {
            file: "autoload".to_string(),
            message,
        }));
        diagnostics.sort();
        Self {
            map,
            packages,
            autoload_files,
            diagnostics,
        }
    }

    /// The autoload map.
    pub fn map(&self) -> &AutoloadMap {
        &self.map
    }

    /// A class locator over this project's map.
    pub fn locator<'a>(&'a self, fs: &'a dyn Fs) -> ComposerLocator<'a> {
        ComposerLocator::new(&self.map, fs)
    }

    /// The locked packages ordered by name.
    pub fn packages(&self) -> &[LockedPackage] {
        &self.packages
    }

    /// The `files` autoload entries that exist, project-relative, root first then by package name.
    pub fn autoload_files(&self) -> &[String] {
        &self.autoload_files
    }

    /// Problems found while reading, sorted.
    pub fn diagnostics(&self) -> &[ComposerDiagnostic] {
        &self.diagnostics
    }
}

/// Reports a problem with a file: the file's name and what is wrong.
type Problem<'a> = &'a mut dyn FnMut(&str, String);

/// Receives one source's autoload JSON, its base directory and the file it came from.
type Collect<'a> = &'a mut dyn FnMut(&Value, &str, &str);

fn load_json(fs: &dyn Fs, path: &str, problem: Problem<'_>) -> Option<Value> {
    let text = match fs.read_to_string(path) {
        Ok(text) => text,
        Err(e) => {
            problem(path, format!("cannot be read: {e}"));
            return None;
        }
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(value) if value.is_object() => Some(value),
        Ok(_) => {
            problem(path, "is not a JSON object".to_string());
            None
        }
        Err(e) => {
            problem(path, format!("is not valid JSON: {e}"));
            None
        }
    }
}

/// Reads `packages` and `packages-dev`, handing each package's rules to `collect`, and returns
/// the package list ordered by name.
fn locked_packages(lock: &Value, vendor: &str, collect: Collect<'_>) -> Vec<LockedPackage> {
    let mut found: Vec<(LockedPackage, &Value)> = Vec::new();
    for (key, dev) in [("packages", false), ("packages-dev", true)] {
        let Some(list) = lock.get(key).and_then(Value::as_array) else {
            continue;
        };
        for package in list {
            let Some(name) = package.get("name").and_then(Value::as_str) else {
                continue;
            };
            let text = |field: &str| package.get(field).and_then(Value::as_str);
            let reference = ["source", "dist"]
                .iter()
                .find_map(|k| package.get(*k)?.get("reference")?.as_str());
            found.push((
                LockedPackage {
                    name: name.to_string(),
                    version: text("version").unwrap_or_default().to_string(),
                    reference: reference.map(str::to_string),
                    dev,
                },
                package,
            ));
        }
    }
    found.sort_by(|a, b| a.0.name.cmp(&b.0.name));
    found.dedup_by(|a, b| a.0.name == b.0.name);
    for (package, value) in &found {
        let base = format!("{vendor}/{}", package.name);
        collect(value, &base, "composer.lock");
    }
    found.into_iter().map(|(p, _)| p).collect()
}
