//! Closing a table over the classes its declarations need: a worklist, not interior mutability.
//! Locating and loading go through ports, so the domain never touches files or parsers.

use std::collections::BTreeSet;

use super::SymbolTable;
use crate::name::Fqn;
use crate::summary::FileSummary;

/// Term: Symbol table
///
/// Port: finds the file that declares a class, if any source knows it (Composer's autoload map,
/// in production).
pub trait ClassLocator {
    /// The path of the file expected to declare `class`.
    fn locate(&self, class: &Fqn) -> Option<Box<str>>;
}

/// Term: Symbol table
///
/// Port: produces the summary of a file by path (parse and summarise, in production).
pub trait ClassSource {
    /// The summary of the file at `path`, or `None` if it cannot be read or parsed at all.
    fn load(&self, path: &str) -> Option<FileSummary>;
}

/// Term: Symbol table
///
/// How far closing a table may go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record of limits")]
pub struct LoadLimits {
    /// The most files loaded through the ports.
    pub max_files: usize,
    /// The most dependency levels loaded: 1 loads what the given summaries need directly.
    pub max_depth: usize,
}

impl Default for LoadLimits {
    fn default() -> Self {
        Self {
            max_files: 5_000,
            max_depth: 16,
        }
    }
}

/// Term: Symbol table
///
/// What closing a table did: for the coverage report.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct LoadReport {
    /// Paths loaded through the ports, in load order.
    pub loaded: Vec<Box<str>>,
    /// Classes that were needed but no source could provide, in name order.
    pub not_found: Vec<Fqn>,
    /// Whether a limit stopped loading before the set was closed.
    pub limit_hit: bool,
}

/// Term: Symbol table
///
/// Builds a table from the given summaries and the classes they need.
#[derive(Clone, Copy)]
pub struct TableBuilder<'a> {
    locator: &'a dyn ClassLocator,
    source: &'a dyn ClassSource,
    limits: LoadLimits,
}

impl<'a> TableBuilder<'a> {
    /// A builder with the default limits.
    pub fn new(locator: &'a dyn ClassLocator, source: &'a dyn ClassSource) -> Self {
        Self {
            locator,
            source,
            limits: LoadLimits::default(),
        }
    }

    /// Replaces the limits.
    pub fn with_limits(mut self, limits: LoadLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Closes `given` over the classes it references. The first level wants everything the given
    /// files refer to, bodies included; loaded files only pull in what their declarations name.
    pub fn build(&self, given: Vec<FileSummary>) -> (SymbolTable, LoadReport) {
        let mut report = LoadReport::default();
        let mut known: BTreeSet<Box<str>> = declared_keys(&given);
        let mut paths: BTreeSet<Box<str>> = given.iter().map(|f| f.path.clone()).collect();
        let mut wanted = names_of(
            given.iter().flat_map(|f| f.referenced_classes.iter()),
            &known,
        );
        let mut asked: BTreeSet<Box<str>> = BTreeSet::new();
        let mut all = given;
        let mut missing: Vec<Fqn> = Vec::new();
        let mut level = 0usize;
        while !wanted.is_empty() {
            level += 1;
            if level > self.limits.max_depth {
                report.limit_hit = true;
                break;
            }
            let mut fresh: Vec<FileSummary> = Vec::new();
            for class in &wanted {
                if known.contains(class.key()) || !asked.insert(class.key().into()) {
                    continue;
                }
                let Some(path) = self.locator.locate(class) else {
                    missing.push(class.clone());
                    continue;
                };
                if paths.contains(&path) {
                    // Located in a file already loaded that does not declare it after all.
                    missing.push(class.clone());
                    continue;
                }
                if report.loaded.len() >= self.limits.max_files {
                    report.limit_hit = true;
                    break;
                }
                match self.source.load(&path) {
                    Some(summary) => {
                        paths.insert(path.clone());
                        report.loaded.push(path);
                        known.extend(declared_keys(std::slice::from_ref(&summary)));
                        fresh.push(summary);
                        if !known.contains(class.key()) {
                            // The file loaded but does not declare the class it was located for.
                            missing.push(class.clone());
                        }
                    }
                    None => missing.push(class.clone()),
                }
            }
            if report.limit_hit {
                all.append(&mut fresh);
                break;
            }
            wanted = names_of(
                fresh.iter().flat_map(|f| f.declaration_classes.iter()),
                &known,
            );
            all.append(&mut fresh);
        }
        missing.sort();
        missing.dedup();
        report.not_found = missing;
        (SymbolTable::build(all), report)
    }
}

fn declared_keys(files: &[FileSummary]) -> BTreeSet<Box<str>> {
    files
        .iter()
        .flat_map(|f| f.classes.iter().map(|c| Box::<str>::from(c.name.key())))
        .collect()
}

/// The distinct names in `names` not yet known, in name order.
fn names_of<'n>(names: impl Iterator<Item = &'n Fqn>, known: &BTreeSet<Box<str>>) -> Vec<Fqn> {
    let set: BTreeSet<&Fqn> = names.filter(|n| !known.contains(n.key())).collect();
    set.into_iter().cloned().collect()
}

impl std::fmt::Debug for TableBuilder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TableBuilder")
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}
