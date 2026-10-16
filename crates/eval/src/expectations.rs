//! The committed expectations: the ratchet the evaluation is compared with.
//!
//! Per weakness class: the fewest true positives and the most false positives the project accepts,
//! and the known gaps. The file changes only through `--update`, whose diff is printed.

use std::collections::{BTreeMap, BTreeSet};

use toml::{Table, Value};

use crate::corpus::Corpus;
use crate::matching::Scored;

/// What is accepted for one weakness class.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Expected {
    /// The fewest true positives.
    pub min_tp: u32,
    /// The most false positives.
    pub max_fp: u32,
    /// Case ids accepted as known gaps, sorted.
    pub known_gaps: Vec<String>,
}

/// The expectations file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Expectations {
    /// Whether a scan command exists; after that `not evaluated` fails CI.
    pub scan_switch: bool,
    /// Per weakness class.
    pub per_cwe: BTreeMap<String, Expected>,
}

fn count(table: &Table, key: &str) -> Result<u32, String> {
    table
        .get(key)
        .and_then(Value::as_integer)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| format!("`{key}` must be a non-negative integer"))
}

impl Expectations {
    /// Parses the expectations file.
    pub fn parse(text: &str) -> Result<Self, String> {
        let table: Table = text
            .parse()
            .map_err(|e| format!("expectations.toml: {e}"))?;
        let scan_switch = table
            .get("eval")
            .and_then(Value::as_table)
            .and_then(|t| t.get("scan-switch"))
            .and_then(Value::as_bool)
            .ok_or("expectations.toml: `[eval] scan-switch` must be a boolean")?;
        let mut per_cwe = BTreeMap::new();
        if let Some(Value::Table(classes)) = table.get("cwe") {
            for (cwe, value) in classes {
                let entry = value
                    .as_table()
                    .ok_or_else(|| format!("expectations.toml: `{cwe}` must be a table"))?;
                let mut known_gaps: Vec<String> = match entry.get("known_gaps") {
                    Some(Value::Array(items)) => items
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect(),
                    _ => Vec::new(),
                };
                known_gaps.sort();
                per_cwe.insert(
                    cwe.clone(),
                    Expected {
                        min_tp: count(entry, "min_tp").map_err(|e| format!("{cwe}: {e}"))?,
                        max_fp: count(entry, "max_fp").map_err(|e| format!("{cwe}: {e}"))?,
                        known_gaps,
                    },
                );
            }
        }
        Ok(Self {
            scan_switch,
            per_cwe,
        })
    }

    /// The file's text: fixed order, fixed formatting.
    pub fn render(&self) -> String {
        let mut out = String::from(
            "# Accuracy expectations: the ratchet `cargo xtask eval` compares against.\n\
             # Change only with `cargo xtask eval --update` and review the printed diff.\n\n\
             [eval]\n\
             # Records that the `bleedthrough` binary has a scan command. While false, an evaluation\n\
             # that reports `not evaluated` passes CI; once true it fails. The change that adds the\n\
             # scan command must flip it (see docs/evaluation.md).\n",
        );
        out.push_str(&format!("scan-switch = {}\n", self.scan_switch));
        for (cwe, expected) in &self.per_cwe {
            let gaps: Vec<String> = expected
                .known_gaps
                .iter()
                .map(|g| format!("{g:?}"))
                .collect();
            out.push_str(&format!(
                "\n[cwe.\"{cwe}\"]\nmin_tp = {}\nmax_fp = {}\nknown_gaps = [{}]\n",
                expected.min_tp,
                expected.max_fp,
                gaps.join(", ")
            ));
        }
        out
    }

    /// The expectations an `--update` records for this result: what was achieved, and the gaps the
    /// manifest declares.
    pub fn recorded(corpus: &Corpus, scored: &Scored, scan_switch: bool) -> Self {
        let mut per_cwe = BTreeMap::new();
        for (cwe, counts) in &scored.per_cwe {
            let mut known_gaps: Vec<String> = corpus
                .cases
                .iter()
                .filter(|c| &c.cwe == cwe && c.known_gap.is_some())
                .map(|c| c.id.clone())
                .collect();
            known_gaps.sort();
            per_cwe.insert(
                cwe.clone(),
                Expected {
                    min_tp: counts.tp,
                    max_fp: counts.fp,
                    known_gaps,
                },
            );
        }
        Self {
            scan_switch,
            per_cwe,
        }
    }

    /// Every way `scored` is worse than, or inconsistent with, these expectations.
    pub fn compare(&self, corpus: &Corpus, scored: &Scored) -> Vec<String> {
        let mut problems = Vec::new();
        for (cwe, counts) in &scored.per_cwe {
            let Some(expected) = self.per_cwe.get(cwe) else {
                problems.push(format!(
                    "{cwe}: the report has results for this class but the expectations do not; run `cargo xtask eval --update`"
                ));
                continue;
            };
            if counts.tp < expected.min_tp {
                problems.push(format!(
                    "{cwe}: {} true positives, expected at least {}",
                    counts.tp, expected.min_tp
                ));
            }
            if counts.fp > expected.max_fp {
                problems.push(format!(
                    "{cwe}: {} false positives, expected at most {}",
                    counts.fp, expected.max_fp
                ));
            }
        }
        let accepted: BTreeSet<&str> = self
            .per_cwe
            .values()
            .flat_map(|e| e.known_gaps.iter().map(String::as_str))
            .collect();
        for outcome in &scored.cases {
            let gap = accepted.contains(outcome.id.as_str());
            if !outcome.passed && !gap {
                problems.push(format!("{}: case {} fails", outcome.cwe, outcome.id));
            }
            if outcome.passed && gap {
                problems.push(format!(
                    "{}: known gap {} now passes; remove it from the manifest and run `cargo xtask eval --update`",
                    outcome.cwe, outcome.id
                ));
            }
        }
        for id in &accepted {
            if corpus.case(id).is_none() {
                problems.push(format!("expectations name an unknown case `{id}`"));
            }
        }
        let declared: BTreeSet<&str> = corpus
            .cases
            .iter()
            .filter(|c| c.known_gap.is_some())
            .map(|c| c.id.as_str())
            .collect();
        for id in declared.symmetric_difference(&accepted) {
            problems.push(format!(
                "known gap `{id}` is not in both the manifest and the expectations; run `cargo xtask eval --update`"
            ));
        }
        problems
    }

    /// A human-readable diff from `self` to `new`: each changed count, each added or removed gap.
    pub fn diff(&self, new: &Self) -> Vec<String> {
        let mut lines = Vec::new();
        if self.scan_switch != new.scan_switch {
            lines.push(format!(
                "scan-switch: {} -> {}",
                self.scan_switch, new.scan_switch
            ));
        }
        let classes: BTreeSet<&String> = self.per_cwe.keys().chain(new.per_cwe.keys()).collect();
        for cwe in classes {
            let old = self.per_cwe.get(cwe).cloned().unwrap_or_default();
            let Some(new) = new.per_cwe.get(cwe) else {
                lines.push(format!("{cwe}: removed"));
                continue;
            };
            if !self.per_cwe.contains_key(cwe) {
                lines.push(format!("{cwe}: added"));
            }
            if old.min_tp != new.min_tp {
                lines.push(format!("{cwe}: min_tp {} -> {}", old.min_tp, new.min_tp));
            }
            if old.max_fp != new.max_fp {
                lines.push(format!("{cwe}: max_fp {} -> {}", old.max_fp, new.max_fp));
            }
            for gap in new
                .known_gaps
                .iter()
                .filter(|g| !old.known_gaps.contains(g))
            {
                lines.push(format!("{cwe}: known gap added: {gap}"));
            }
            for gap in old
                .known_gaps
                .iter()
                .filter(|g| !new.known_gaps.contains(g))
            {
                lines.push(format!("{cwe}: known gap removed: {gap}"));
            }
        }
        lines
    }
}
