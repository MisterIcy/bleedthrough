//! The autoload map: classmap, PSR-4 and PSR-0 rules in Composer's order, and the locator.

use std::collections::BTreeMap;

use bleedthrough_symbols::{ClassLocator, Fqn};

use super::files::{Fs, normalise};
use super::rules::{AutoloadRules, PrefixRule};
use super::scan::declared_classes;

/// Deepest directory nesting a classmap scan follows.
const MAX_SCAN_DEPTH: usize = 32;

/// Term: Symbol table
///
/// Where Composer would look for classes: every source's rules merged in precedence order. The
/// order inside it never depends on directory enumeration: scans sort their entries.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AutoloadMap {
    /// Class key to files declaring it, in scan order.
    classmap: BTreeMap<String, Vec<String>>,
    /// PSR-4 rules, longest prefix first, then in source order.
    psr4: Vec<PrefixRule>,
    /// PSR-0 rules, longest prefix first, then in source order.
    psr0: Vec<PrefixRule>,
    /// Excluded paths and path patterns.
    exclude: Vec<String>,
}

impl AutoloadMap {
    /// Merges the rules of several sources, root first, scanning classmap entries through `fs`.
    /// Problems are appended to `problems`.
    pub fn build(sources: &[AutoloadRules], fs: &dyn Fs, problems: &mut Vec<String>) -> Self {
        let mut map = Self::default();
        for rules in sources {
            map.exclude.extend(rules.exclude.iter().cloned());
        }
        for rules in sources {
            map.psr4
                .extend(rules.psr4.iter().map(with_trailing_separator));
            map.psr0.extend(rules.psr0.iter().cloned());
        }
        // Stable sort: equal prefixes keep source order.
        map.psr4.sort_by_key(|r| std::cmp::Reverse(r.prefix.len()));
        map.psr0.sort_by_key(|r| std::cmp::Reverse(r.prefix.len()));
        for rules in sources {
            for entry in &rules.classmap {
                map.scan_entry(entry, fs, 0, problems);
            }
        }
        map
    }

    fn scan_entry(&mut self, path: &str, fs: &dyn Fs, depth: usize, problems: &mut Vec<String>) {
        if self.is_excluded(path) || !fs.stays_inside(path) {
            if !fs.stays_inside(path) {
                problems.push(format!(
                    "`{path}` resolves outside the project and is ignored"
                ));
            }
            return;
        }
        if let Some(mut entries) = fs.list_dir(path) {
            if depth >= MAX_SCAN_DEPTH {
                problems.push(format!("`{path}` is nested too deeply to scan"));
                return;
            }
            entries.sort();
            for entry in entries {
                let child = if path.is_empty() {
                    entry.name.clone()
                } else {
                    format!("{path}/{}", entry.name)
                };
                if entry.is_dir || is_php(&entry.name) {
                    self.scan_entry(&child, fs, depth + 1, problems);
                }
            }
        } else if fs.is_file(path) {
            match fs.read_to_string(path) {
                Ok(text) => {
                    for class in declared_classes(&text) {
                        let files = self.classmap.entry(class.to_ascii_lowercase()).or_default();
                        if !files.iter().any(|f| f == path) {
                            files.push(path.to_string());
                        }
                    }
                }
                Err(e) => problems.push(format!("cannot read `{path}` for the classmap: {e}")),
            }
        } else {
            problems.push(format!("the classmap entry `{path}` does not exist"));
        }
    }

    /// Whether `path` equals an exclusion, lies under one, or matches one as a wildcard pattern.
    pub fn is_excluded(&self, path: &str) -> bool {
        self.exclude.iter().any(|pattern| {
            let pattern = pattern.trim_end_matches('/');
            if pattern.contains('*') {
                wildcard(pattern, path)
            } else {
                path == pattern || path.starts_with(&format!("{pattern}/"))
            }
        })
    }

    /// The file expected to declare `class`, trying classmap, then PSR-4, then PSR-0. `None` when
    /// no rule maps the name to an existing file inside the project.
    pub fn locate(&self, class: &Fqn, fs: &dyn Fs) -> Option<String> {
        let usable =
            |path: &str| fs.is_file(path) && fs.stays_inside(path) && !self.is_excluded(path);
        if let Some(files) = self.classmap.get(class.key())
            && let Some(found) = files.iter().find(|f| usable(f))
        {
            return Some(found.clone());
        }
        let name = class.as_str();
        for rule in &self.psr4 {
            let Some(rest) = name.strip_prefix(rule.prefix.as_str()) else {
                continue;
            };
            let relative = format!("{}.php", rest.replace('\\', "/"));
            if let Some(found) = first_existing(&rule.dirs, &relative, &usable) {
                return Some(found);
            }
        }
        for rule in &self.psr0 {
            if !name.starts_with(rule.prefix.as_str()) {
                continue;
            }
            let (namespace, short) = match name.rsplit_once('\\') {
                Some((ns, short)) => (format!("{}/", ns.replace('\\', "/")), short),
                None => (String::new(), name),
            };
            let relative = format!("{namespace}{}.php", short.replace('_', "/"));
            if let Some(found) = first_existing(&rule.dirs, &relative, &usable) {
                return Some(found);
            }
        }
        None
    }
}

fn first_existing(
    dirs: &[String],
    relative: &str,
    usable: &dyn Fn(&str) -> bool,
) -> Option<String> {
    dirs.iter()
        .filter_map(|dir| normalise(dir, relative))
        .find(|path| usable(path))
}

fn with_trailing_separator(rule: &PrefixRule) -> PrefixRule {
    let mut rule = rule.clone();
    if !rule.prefix.is_empty() && !rule.prefix.ends_with('\\') {
        rule.prefix.push('\\');
    }
    rule
}

fn is_php(name: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("php") || e.eq_ignore_ascii_case("inc"))
}

/// `*` matches within a segment, `**` across segments.
fn wildcard(pattern: &str, path: &str) -> bool {
    fn go(p: &[u8], s: &[u8]) -> bool {
        match p.split_first() {
            None => s.is_empty() || s.first() == Some(&b'/'),
            Some((b'*', rest)) => {
                let (deep, rest) = match rest.split_first() {
                    Some((b'*', r)) => (true, r),
                    _ => (false, rest),
                };
                (0..=s.len()).any(|n| (deep || !s[..n].contains(&b'/')) && go(rest, &s[n..]))
            }
            Some((c, rest)) => s.first() == Some(c) && go(rest, &s[1..]),
        }
    }
    go(pattern.as_bytes(), path.as_bytes())
}

/// Term: Symbol table
///
/// The class locator over a Composer autoload map and a file system.
#[derive(Clone, Copy)]
pub struct ComposerLocator<'a> {
    map: &'a AutoloadMap,
    fs: &'a dyn Fs,
}

impl<'a> ComposerLocator<'a> {
    /// A locator over `map`, checking existence through `fs`.
    pub fn new(map: &'a AutoloadMap, fs: &'a dyn Fs) -> Self {
        Self { map, fs }
    }
}

impl std::fmt::Debug for ComposerLocator<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComposerLocator").finish_non_exhaustive()
    }
}

impl ClassLocator for ComposerLocator<'_> {
    fn locate(&self, class: &Fqn) -> Option<Box<str>> {
        self.map.locate(class, self.fs).map(String::into_boxed_str)
    }
}
