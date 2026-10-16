//! The evaluation result: verdict, metrics, a JSON document with a versioned schema and a table.
//!
//! Output is deterministic: sorted maps, fixed rounding, no timestamps and no absolute paths.

use serde_json::{Map, Value, json};

use crate::corpus::Corpus;
use crate::matching::{Counts, Scored};

/// The version of the result document's schema (`schemas/eval-result.schema.json`).
pub const SCHEMA_VERSION: &str = "1";

/// The outcome of an evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Every expectation is met.
    Pass,
    /// A regression, an unrecorded improvement, or an invalid report.
    Fail,
    /// There is no scan to evaluate yet.
    NotEvaluated,
}

impl Verdict {
    /// The word used in the document.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::NotEvaluated => "not-evaluated",
        }
    }

    /// The exit status `cargo xtask eval` uses: 0 pass, 1 fail, 3 not evaluated.
    pub fn exit_code(self) -> u8 {
        match self {
            Self::Pass => 0,
            Self::Fail => 1,
            Self::NotEvaluated => 3,
        }
    }
}

/// A finished evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    /// The verdict.
    pub verdict: Verdict,
    /// Why, for `not-evaluated`.
    pub reason: Option<String>,
    /// The metrics; absent unless a valid report was scored.
    pub scored: Option<Scored>,
    /// What is wrong, for `fail`.
    pub problems: Vec<String>,
}

impl Evaluation {
    /// An evaluation with no metrics.
    pub fn without_metrics(
        verdict: Verdict,
        reason: Option<String>,
        problems: Vec<String>,
    ) -> Self {
        Self {
            verdict,
            reason,
            scored: None,
            problems,
        }
    }
}

fn metric(value: Option<f64>) -> Value {
    value.map_or(Value::Null, |v| json!((v * 10_000.0).round() / 10_000.0))
}

fn counts_json(counts: &Counts) -> Value {
    json!({
        "tp": counts.tp,
        "fp": counts.fp,
        "fn": counts.fn_,
        "known_gaps": counts.known_gaps,
        "precision": metric(counts.precision()),
        "recall": metric(counts.recall()),
    })
}

/// The machine-readable document, pretty-printed with a trailing newline.
pub fn to_json(evaluation: &Evaluation, corpus: &Corpus) -> String {
    let mut doc = Map::new();
    doc.insert("schema_version".into(), json!(SCHEMA_VERSION));
    doc.insert("verdict".into(), json!(evaluation.verdict.as_str()));
    doc.insert("reason".into(), json!(evaluation.reason));
    doc.insert("problems".into(), json!(evaluation.problems));
    if let Some(scored) = &evaluation.scored {
        doc.insert("totals".into(), counts_json(&scored.totals()));
        let per_cwe: Map<String, Value> = scored
            .per_cwe
            .iter()
            .map(|(k, v)| (k.clone(), counts_json(v)))
            .collect();
        doc.insert("per_cwe".into(), Value::Object(per_cwe));
        let per_rule: Map<String, Value> = scored
            .per_rule
            .iter()
            .map(|(rule, c)| {
                let counts = Counts {
                    tp: c.tp,
                    fp: c.fp,
                    ..Counts::default()
                };
                (
                    rule.clone(),
                    json!({
                        "tp": c.tp,
                        "fp": c.fp,
                        "fn": null,
                        "precision": metric(counts.precision()),
                        "recall": null,
                    }),
                )
            })
            .collect();
        doc.insert("per_rule".into(), Value::Object(per_rule));
        let cases: Vec<Value> = scored
            .cases
            .iter()
            .map(|c| {
                json!({"id": c.id, "cwe": c.cwe, "passed": c.passed, "known_gap": c.known_gap,
                       "tp": c.counts.tp, "fp": c.counts.fp, "fn": c.counts.fn_})
            })
            .collect();
        doc.insert("cases".into(), Value::Array(cases));
        let gaps: Vec<Value> = corpus
            .cases
            .iter()
            .filter_map(|c| {
                c.known_gap
                    .as_ref()
                    .map(|g| json!({"id": c.id, "cwe": c.cwe, "reason": g.reason, "tracking": g.tracking}))
            })
            .collect();
        doc.insert("known_gaps".into(), Value::Array(gaps));
        let unlabelled: Vec<Value> = scored
            .unlabelled
            .iter()
            .map(|u| json!({"path": u.path, "line": u.line, "rule": u.rule, "cwe": u.cwes}))
            .collect();
        doc.insert("unlabelled".into(), Value::Array(unlabelled));
    }
    let mut text = serde_json::to_string_pretty(&Value::Object(doc)).unwrap_or_default();
    text.push('\n');
    text
}

fn cell(value: Option<f64>) -> String {
    value.map_or_else(|| "undefined".to_string(), |v| format!("{v:.4}"))
}

/// The human-readable report.
pub fn table(evaluation: &Evaluation) -> String {
    let mut out = format!("verdict: {}\n", evaluation.verdict.as_str());
    if let Some(reason) = &evaluation.reason {
        out.push_str(&format!("reason: {reason}\n"));
    }
    if let Some(scored) = &evaluation.scored {
        out.push_str("\n| class | tp | fp | fn | gaps | precision | recall |\n|---|---|---|---|---|---|---|\n");
        for (cwe, c) in &scored.per_cwe {
            out.push_str(&format!(
                "| {cwe} | {} | {} | {} | {} | {} | {} |\n",
                c.tp,
                c.fp,
                c.fn_,
                c.known_gaps,
                cell(c.precision()),
                cell(c.recall())
            ));
        }
        let t = scored.totals();
        out.push_str(&format!(
            "| total | {} | {} | {} | {} | {} | {} |\n",
            t.tp,
            t.fp,
            t.fn_,
            t.known_gaps,
            cell(t.precision()),
            cell(t.recall())
        ));
        if !scored.per_rule.is_empty() {
            out.push_str("\n| rule | tp | fp | precision |\n|---|---|---|---|\n");
            for (rule, c) in &scored.per_rule {
                let counts = Counts {
                    tp: c.tp,
                    fp: c.fp,
                    ..Counts::default()
                };
                out.push_str(&format!(
                    "| {rule} | {} | {} | {} |\n",
                    c.tp,
                    c.fp,
                    cell(counts.precision())
                ));
            }
        }
        if !scored.unlabelled.is_empty() {
            out.push_str(&format!(
                "\n{} unlabelled finding(s):\n",
                scored.unlabelled.len()
            ));
            for u in &scored.unlabelled {
                out.push_str(&format!(
                    "  {}:{} {} {}\n",
                    u.path,
                    u.line,
                    u.rule,
                    u.cwes.join(",")
                ));
            }
        }
    }
    for problem in &evaluation.problems {
        out.push_str(&format!("problem: {problem}\n"));
    }
    out
}
