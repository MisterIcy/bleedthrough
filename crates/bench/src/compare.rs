//! Comparing two results from one machine: noise-aware, never across environments by accident.
//!
//! A change is a regression only when it exceeds both the threshold and `k` times the noise of the
//! base runs (median absolute deviation over median).

use std::collections::BTreeMap;

use crate::result::{Document, StageResult};
use crate::stage::Outcome;

/// How strict the comparison is.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// Relative slowdown that always counts, for example 0.10.
    pub threshold: f64,
    /// Multiples of the base noise a slowdown must exceed.
    pub k: f64,
    /// Compare results from different environments anyway; the report is labelled.
    pub force: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            threshold: 0.10,
            k: 3.0,
            force: false,
        }
    }
}

/// The verdict on one stage and input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Slower than both the threshold and the noise allow.
    Regression,
    /// Faster than both the threshold and the noise allow.
    Improvement,
    /// Within threshold or noise.
    NoChange,
    /// One side has no measurement.
    Unavailable,
}

/// One row of the comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The stage.
    pub stage: String,
    /// The input.
    pub input: String,
    /// Base median in nanoseconds.
    pub base_median_ns: Option<u64>,
    /// Head median in nanoseconds.
    pub head_median_ns: Option<u64>,
    /// Relative difference of the medians, positive when head is slower.
    pub difference: Option<f64>,
    /// Noise of the base runs, relative.
    pub noise: Option<f64>,
    /// The limit the difference was held against: `max(threshold, k * noise)`.
    pub limit: Option<f64>,
    /// The verdict.
    pub verdict: Verdict,
}

/// A finished comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    /// Compared with `--force` across different environments.
    pub forced: bool,
    /// One row per stage and input present in base.
    pub rows: Vec<Row>,
}

impl Comparison {
    /// The rows that are regressions.
    pub fn regressions(&self) -> Vec<&Row> {
        self.rows
            .iter()
            .filter(|r| r.verdict == Verdict::Regression)
            .collect()
    }

    /// A human-readable report (markdown); regressions name the stage, the input and the difference.
    pub fn report(&self) -> String {
        let mut out = String::new();
        if self.forced {
            out.push_str("**forced comparison across different environments: not reliable**\n\n");
        }
        out.push_str("| stage | input | base | head | difference | limit | verdict |\n|---|---|---|---|---|---|---|\n");
        let ms = |n: Option<u64>| {
            n.map_or_else(
                || "n/a".to_string(),
                |n| format!("{:.2} ms", n as f64 / 1e6),
            )
        };
        let pct =
            |v: Option<f64>| v.map_or_else(|| "n/a".to_string(), |v| format!("{:+.1}%", v * 100.0));
        for row in &self.rows {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {:?} |\n",
                row.stage,
                row.input,
                ms(row.base_median_ns),
                ms(row.head_median_ns),
                pct(row.difference),
                row.limit
                    .map_or_else(|| "n/a".to_string(), |l| format!("{:.1}%", l * 100.0)),
                row.verdict
            ));
        }
        for row in self.regressions() {
            out.push_str(&format!(
                "\nREGRESSION: stage `{}` on `{}` is {} slower (limit {})\n",
                row.stage,
                row.input,
                pct(row.difference),
                row.limit
                    .map_or_else(String::new, |l| format!("{:.1}%", l * 100.0)),
            ));
        }
        out
    }
}

fn index(doc: &Document) -> BTreeMap<(&str, &str), &StageResult> {
    doc.results
        .iter()
        .map(|r| ((r.stage.as_str(), r.input.as_str()), r))
        .collect()
}

/// Compares `head` with `base`. Refuses results from different environments unless forced, and
/// results whose named inputs disagree on file or byte counts.
pub fn compare(base: &Document, head: &Document, options: &Options) -> Result<Comparison, String> {
    let forced = !base.environment.comparable_with(&head.environment);
    if forced && !options.force {
        return Err(format!(
            "the results come from different environments (base: {} / {} cores / {}; head: {} / {} cores / {}); compare only on one machine or pass --force",
            base.environment.cpu_model,
            base.environment.logical_cores,
            base.environment.os,
            head.environment.cpu_model,
            head.environment.logical_cores,
            head.environment.os,
        ));
    }
    for input in &base.inputs {
        if let Some(other) = head.inputs.iter().find(|i| i.name == input.name)
            && (other.files != input.files || other.bytes != input.bytes)
        {
            return Err(format!(
                "input `{}` drifted: base has {} files / {} bytes, head has {} files / {} bytes",
                input.name, input.files, input.bytes, other.files, other.bytes
            ));
        }
    }
    let head_index = index(head);
    let rows = base
        .results
        .iter()
        .map(|b| {
            row(
                b,
                head_index
                    .get(&(b.stage.as_str(), b.input.as_str()))
                    .copied(),
                options,
            )
        })
        .collect();
    Ok(Comparison { forced, rows })
}

fn row(base: &StageResult, head: Option<&StageResult>, options: &Options) -> Row {
    let mut row = Row {
        stage: base.stage.clone(),
        input: base.input.clone(),
        base_median_ns: None,
        head_median_ns: None,
        difference: None,
        noise: None,
        limit: None,
        verdict: Verdict::Unavailable,
    };
    let (Outcome::Measured(b), Some(Outcome::Measured(h))) =
        (&base.outcome, head.map(|h| &h.outcome))
    else {
        return row;
    };
    if b.timing.median_ns == 0 {
        return row;
    }
    let difference =
        (h.timing.median_ns as f64 - b.timing.median_ns as f64) / b.timing.median_ns as f64;
    let noise = b.timing.noise();
    let limit = options.threshold.max(options.k * noise);
    row.base_median_ns = Some(b.timing.median_ns);
    row.head_median_ns = Some(h.timing.median_ns);
    row.difference = Some(difference);
    row.noise = Some(noise);
    row.limit = Some(limit);
    row.verdict = if difference > limit {
        Verdict::Regression
    } else if difference < -limit {
        Verdict::Improvement
    } else {
        Verdict::NoChange
    };
    row
}
