//! What the call graph could not follow, counted by reason.

use std::collections::BTreeMap;

/// Term: Coverage
///
/// The calls and includes the call graph could not resolve, and the candidate sets it cut, counted
/// by reason. The findings coverage keeps the totals; the reasons stay here for the report detail
/// and for tests.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record of counters")]
pub struct CallCoverage {
    /// Unresolved calls by reason code.
    pub unresolved_calls: BTreeMap<Box<str>, u64>,
    /// Unresolved includes by reason code.
    pub unresolved_includes: BTreeMap<Box<str>, u64>,
    /// Calls that reach a declaration with no analysed body (a class that is not loaded).
    pub external_calls: u64,
    /// Calls whose candidate set was cut by a cap.
    pub truncated_calls: u64,
}

impl CallCoverage {
    /// All unresolved calls.
    pub fn total_unresolved_calls(&self) -> u64 {
        self.unresolved_calls.values().sum()
    }

    /// All unresolved includes.
    pub fn total_unresolved_includes(&self) -> u64 {
        self.unresolved_includes.values().sum()
    }

    pub(crate) fn call(&mut self, reason: &str) {
        *self.unresolved_calls.entry(reason.into()).or_default() += 1;
    }

    pub(crate) fn include(&mut self, reason: &str) {
        *self.unresolved_includes.entry(reason.into()).or_default() += 1;
    }
}
