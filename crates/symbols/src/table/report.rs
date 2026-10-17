//! The unknown-built-in report: names project code refers to that nothing declares.

use super::SymbolTable;
use crate::builtin::{Availability, Builtins};
use crate::name::Fqn;
use crate::summary::FileSummary;

/// Term: Built-in data
///
/// A name that built-in data knows, but not for the modelled PHP version.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct Unavailable {
    /// The function or class.
    pub name: Fqn,
    /// Why it is not available: not in this version, or its extension is not bundled.
    pub availability: Availability,
}

/// Term: Built-in data
///
/// What the referenced functions and classes of some files come to when checked against the table
/// and the built-in data, each list sorted and distinct.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct UnknownReport {
    /// Classes neither the table nor the built-in data declares.
    pub classes: Vec<Fqn>,
    /// Functions neither the table nor the built-in data declares (the global name when the call
    /// could have been namespaced).
    pub functions: Vec<Fqn>,
    /// Names the built-in data has only for other PHP versions, or in an unbundled extension.
    pub unavailable: Vec<Unavailable>,
}

impl SymbolTable {
    /// Checks what `files` refer to against the table and `builtins`.
    pub fn unknown_references<'a>(
        &self,
        builtins: &Builtins,
        files: impl IntoIterator<Item = &'a FileSummary>,
    ) -> UnknownReport {
        let mut report = UnknownReport::default();
        for file in files {
            for class in &file.referenced_classes {
                if self.class(class).is_some() {
                    continue;
                }
                match builtins.class(class) {
                    Availability::Unknown => report.classes.push(class.clone()),
                    Availability::Available { .. } => {}
                    other => report.unavailable.push(Unavailable {
                        name: class.clone(),
                        availability: other,
                    }),
                }
            }
            for call in &file.referenced_functions {
                let known = self.function(&call.primary).is_some()
                    || call
                        .fallback
                        .as_ref()
                        .is_some_and(|f| self.function(f).is_some());
                if known {
                    continue;
                }
                let name = call.fallback.as_ref().unwrap_or(&call.primary);
                match builtins.function(name) {
                    Availability::Unknown => report.functions.push(name.clone()),
                    Availability::Available { .. } => {}
                    other => report.unavailable.push(Unavailable {
                        name: name.clone(),
                        availability: other,
                    }),
                }
            }
        }
        report.classes.sort();
        report.classes.dedup();
        report.functions.sort();
        report.functions.dedup();
        report.unavailable.sort_by(|a, b| a.name.cmp(&b.name));
        report.unavailable.dedup();
        report
    }
}
