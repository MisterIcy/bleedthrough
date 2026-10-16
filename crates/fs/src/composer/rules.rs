//! The autoload rules of one source (the root project or one locked package).

use serde_json::Value;

use super::files::normalise;

/// Term: Symbol table
///
/// One PSR-4 or PSR-0 prefix with its directories in declared order (project-relative).
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct PrefixRule {
    /// The namespace prefix (PSR-4: ends with `\` or is empty) or class-name prefix (PSR-0).
    pub prefix: String,
    /// Directories, project-relative, normalised.
    pub dirs: Vec<String>,
}

/// Term: Symbol table
///
/// The autoload section of one source, with every path made project-relative.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct AutoloadRules {
    /// `psr-4` entries in declared order.
    pub psr4: Vec<PrefixRule>,
    /// `psr-0` entries in declared order.
    pub psr0: Vec<PrefixRule>,
    /// `classmap` paths (files or directories).
    pub classmap: Vec<String>,
    /// `files` entries, which execute on load.
    pub files: Vec<String>,
    /// `exclude-from-classmap` patterns, project-relative.
    pub exclude: Vec<String>,
}

/// Reads the `autoload` and `autoload-dev` sections of a `composer.json` object or a lock package.
/// `base` is the source's directory; problems are appended to `problems`.
pub fn read_rules(value: &Value, base: &str, problems: &mut Vec<String>) -> AutoloadRules {
    let mut rules = AutoloadRules::default();
    for section in ["autoload", "autoload-dev"] {
        let Some(section) = value.get(section) else {
            continue;
        };
        let Some(section) = section.as_object() else {
            problems.push(format!("`{}` is not an object", "autoload"));
            continue;
        };
        for (key, entry) in section {
            match key.as_str() {
                "psr-4" => prefixes(entry, base, &mut rules.psr4, problems),
                "psr-0" => prefixes(entry, base, &mut rules.psr0, problems),
                "classmap" => paths(entry, base, &mut rules.classmap, problems),
                "files" => paths(entry, base, &mut rules.files, problems),
                "exclude-from-classmap" => paths(entry, base, &mut rules.exclude, problems),
                _ => {}
            }
        }
    }
    rules
}

fn strings(entry: &Value) -> Option<Vec<&str>> {
    match entry {
        Value::String(s) => Some(vec![s.as_str()]),
        Value::Array(items) => items.iter().map(Value::as_str).collect(),
        _ => None,
    }
}

fn paths(entry: &Value, base: &str, out: &mut Vec<String>, problems: &mut Vec<String>) {
    let Some(list) = strings(entry) else {
        problems.push("a path list contains something that is not a string".to_string());
        return;
    };
    for raw in list {
        match normalise(base, raw) {
            Some(path) => out.push(path),
            None => problems.push(format!(
                "the path `{raw}` leaves the project and is ignored"
            )),
        }
    }
}

fn prefixes(entry: &Value, base: &str, out: &mut Vec<PrefixRule>, problems: &mut Vec<String>) {
    let Some(map) = entry.as_object() else {
        problems.push("a prefix map is not an object".to_string());
        return;
    };
    for (prefix, dirs) in map {
        let Some(list) = strings(dirs) else {
            problems.push(format!("the directories of `{prefix}` are not strings"));
            continue;
        };
        let mut normalised = Vec::new();
        for raw in list {
            match normalise(base, raw) {
                Some(dir) => normalised.push(dir),
                None => problems.push(format!(
                    "the directory `{raw}` of `{prefix}` leaves the project and is ignored"
                )),
            }
        }
        if !normalised.is_empty() {
            out.push(PrefixRule {
                prefix: prefix.clone(),
                dirs: normalised,
            });
        }
    }
}
