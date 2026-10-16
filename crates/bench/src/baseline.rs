//! The committed baseline of deterministic counters (`benchmarks/baselines.toml`).
//!
//! Input counters (files, bytes, nodes, errors) are exact. Allocation counters (`alloc_*`) may move
//! by the stated tolerance; beyond it, a rise is a regression and a fall must be recorded with the
//! update command, so the baseline can only improve on purpose.

use std::collections::{BTreeMap, BTreeSet};

use toml::{Table, Value};

use crate::stage::{Measurement, Outcome};

/// Counters of one stage on one input.
pub type CounterSet = BTreeMap<String, u64>;

/// `(stage, input)` to its counters.
pub type Entries = BTreeMap<(String, String), CounterSet>;

/// The default tolerance for allocation counters, in percent.
pub const DEFAULT_TOLERANCE_PCT: u64 = 2;

/// The baseline file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Baselines {
    /// Allowed relative movement of `alloc_*` counters, in percent.
    pub tolerance_pct: u64,
    /// The recorded counters.
    pub entries: Entries,
}

/// The deterministic counters of a measurement: its structural counters and its allocation counters.
pub fn observed(measurement: &Measurement) -> CounterSet {
    let mut set = measurement.counters.clone();
    if let Some(a) = measurement.allocation {
        set.insert("alloc_count".into(), a.count);
        set.insert("alloc_total_bytes".into(), a.total_bytes);
        set.insert("alloc_peak_bytes".into(), a.peak_bytes);
    }
    set
}

/// The observed counters of every measured stage of a run.
pub fn observed_all(results: &[(String, String, Outcome)]) -> Entries {
    results
        .iter()
        .filter_map(|(stage, input, outcome)| match outcome {
            Outcome::Measured(m) => Some(((stage.clone(), input.clone()), observed(m))),
            Outcome::Unavailable(_) => None,
        })
        .collect()
}

impl Baselines {
    /// Parses the baseline file.
    pub fn parse(text: &str) -> Result<Self, String> {
        let table: Table = text.parse().map_err(|e| format!("baselines.toml: {e}"))?;
        let tolerance_pct = table
            .get("tolerance_pct")
            .and_then(Value::as_integer)
            .and_then(|n| u64::try_from(n).ok())
            .unwrap_or(DEFAULT_TOLERANCE_PCT);
        let mut entries = Entries::new();
        for (stage, inputs) in &table {
            let Value::Table(inputs) = inputs else {
                continue;
            };
            for (input, counters) in inputs {
                let counters = counters.as_table().ok_or_else(|| {
                    format!("baselines.toml: [{stage}.\"{input}\"] must be a table")
                })?;
                let set = counters
                    .iter()
                    .map(|(k, v)| {
                        v.as_integer()
                            .and_then(|n| u64::try_from(n).ok())
                            .map(|n| (k.clone(), n))
                            .ok_or_else(|| {
                                format!("baselines.toml: `{k}` must be a non-negative integer")
                            })
                    })
                    .collect::<Result<CounterSet, _>>()?;
                entries.insert((stage.clone(), input.clone()), set);
            }
        }
        Ok(Self {
            tolerance_pct,
            entries,
        })
    }

    /// The file's text: fixed order and formatting.
    pub fn render(&self) -> String {
        let mut out = String::from(
            "# Deterministic benchmark counters: exact input counters and allocation counters within\n\
             # the tolerance. Machine-independent; wall time is never recorded here.\n\
             # Change only with `cargo xtask bench-counters --update` and review the printed diff.\n",
        );
        out.push_str(&format!("tolerance_pct = {}\n", self.tolerance_pct));
        for ((stage, input), counters) in &self.entries {
            out.push_str(&format!("\n[{stage}.\"{input}\"]\n"));
            for (name, value) in counters {
                out.push_str(&format!("{name} = {value}\n"));
            }
        }
        out
    }

    /// Every way `now` differs from the baseline beyond what is allowed; each names the stage, the
    /// counter and both values.
    pub fn check(&self, now: &Entries) -> Vec<String> {
        let mut problems = Vec::new();
        for (key, counters) in now {
            let (stage, input) = key;
            let Some(recorded) = self.entries.get(key) else {
                problems.push(format!(
                    "stage `{stage}` on `{input}` has no baseline; run `cargo xtask bench-counters --update`"
                ));
                continue;
            };
            let names: BTreeSet<&String> = counters.keys().chain(recorded.keys()).collect();
            for name in names {
                match (recorded.get(name), counters.get(name)) {
                    (Some(&was), Some(&is)) => {
                        if let Some(problem) = self.judge(name, was, is) {
                            problems.push(format!("stage `{stage}` on `{input}`: counter `{name}` {problem}: baseline {was}, now {is}"));
                        }
                    }
                    (Some(&was), None) => problems.push(format!(
                        "stage `{stage}` on `{input}`: counter `{name}` is no longer reported (baseline {was})"
                    )),
                    (None, Some(&is)) => problems.push(format!(
                        "stage `{stage}` on `{input}`: counter `{name}` is new (now {is}); run `cargo xtask bench-counters --update`"
                    )),
                    (None, None) => {}
                }
            }
        }
        for (stage, input) in self.entries.keys().filter(|k| !now.contains_key(*k)) {
            problems.push(format!(
                "baseline for stage `{stage}` on `{input}` was not measured"
            ));
        }
        problems
    }

    fn judge(&self, name: &str, was: u64, is: u64) -> Option<String> {
        if was == is {
            return None;
        }
        if !name.starts_with("alloc_") {
            return Some("changed (input counters are exact)".to_string());
        }
        // Integer arithmetic: |is - was| * 100 > was * tolerance.
        let moved = u128::from(was.abs_diff(is)) * 100;
        if moved <= u128::from(was) * u128::from(self.tolerance_pct) {
            return None;
        }
        Some(if is > was {
            format!("regressed (more than {}% above)", self.tolerance_pct)
        } else {
            format!(
                "improved (more than {}% below); record it with --update",
                self.tolerance_pct
            )
        })
    }

    /// Replaces the entries with `now`, keeping the tolerance.
    pub fn updated(&self, now: Entries) -> Self {
        Self {
            tolerance_pct: self.tolerance_pct,
            entries: now,
        }
    }

    /// A reviewable diff from `self` to `new`: each counter that changed, appeared or disappeared.
    pub fn diff(&self, new: &Self) -> Vec<String> {
        let mut lines = Vec::new();
        let keys: BTreeSet<&(String, String)> =
            self.entries.keys().chain(new.entries.keys()).collect();
        for key in keys {
            let (stage, input) = key;
            let empty = CounterSet::new();
            let (old, now) = (
                self.entries.get(key).unwrap_or(&empty),
                new.entries.get(key).unwrap_or(&empty),
            );
            let names: BTreeSet<&String> = old.keys().chain(now.keys()).collect();
            for name in names {
                match (old.get(name), now.get(name)) {
                    (Some(a), Some(b)) if a != b => {
                        lines.push(format!("{stage} `{input}` {name}: {a} -> {b}"))
                    }
                    (None, Some(b)) => lines.push(format!("{stage} `{input}` {name}: added ({b})")),
                    (Some(a), None) => {
                        lines.push(format!("{stage} `{input}` {name}: removed (was {a})"))
                    }
                    _ => {}
                }
            }
        }
        lines
    }
}

/// The counter set of a stage from its structural counters and one allocation measurement.
pub fn with_allocation(counters: &CounterSet, allocation: crate::stage::Allocation) -> CounterSet {
    let mut set = counters.clone();
    set.insert("alloc_count".into(), allocation.count);
    set.insert("alloc_total_bytes".into(), allocation.total_bytes);
    set.insert("alloc_peak_bytes".into(), allocation.peak_bytes);
    set
}
