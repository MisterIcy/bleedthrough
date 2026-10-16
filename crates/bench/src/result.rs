//! The benchmark result document: versioned, machine-readable, no secrets and no absolute paths.
//!
//! Equal measurements serialise identically: keys are sorted and numbers are integers.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::stage::{Allocation, InputDesc, Measurement, Outcome};
use crate::timing::Summary;

/// The schema version this build writes (`schemas/bench-result.schema.json`).
pub const SCHEMA_VERSION: &str = "1";

/// The schema versions this build reads.
pub const SUPPORTED_VERSIONS: [&str; 1] = ["1"];

/// The machine a result came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Environment {
    /// Operating system.
    pub os: String,
    /// CPU model, `unknown` where it cannot be read.
    pub cpu_model: String,
    /// Logical cores.
    pub logical_cores: u32,
    /// `release` or `debug`.
    pub profile: String,
}

impl Environment {
    /// Describes the current machine and build.
    pub fn current() -> Self {
        let cpu_model = std::fs::read_to_string("/proc/cpuinfo")
            .ok()
            .and_then(|text| {
                text.lines()
                    .find_map(|l| l.strip_prefix("model name").and_then(|r| r.split_once(':')))
                    .map(|(_, model)| model.trim().to_string())
            })
            .unwrap_or_else(|| "unknown".to_string());
        Self {
            os: std::env::consts::OS.to_string(),
            cpu_model,
            logical_cores: std::thread::available_parallelism()
                .map_or(1, |n| u32::try_from(n.get()).unwrap_or(u32::MAX)),
            profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
            .to_string(),
        }
    }

    /// Whether two results may be compared without `--force`: same OS, CPU model and core count.
    pub fn comparable_with(&self, other: &Self) -> bool {
        self.os == other.os
            && self.cpu_model == other.cpu_model
            && self.logical_cores == other.logical_cores
    }
}

/// One stage's outcome on one input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageResult {
    /// The stage's name.
    pub stage: String,
    /// The name of the input it ran on.
    pub input: String,
    /// What happened.
    pub outcome: Outcome,
}

/// A result document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// The version of the tool that measured.
    pub tool_version: String,
    /// The commit measured, when known.
    pub commit: Option<String>,
    /// The machine and build.
    pub environment: Environment,
    /// The inputs.
    pub inputs: Vec<InputDesc>,
    /// The stage outcomes.
    pub results: Vec<StageResult>,
}

fn per_second(count: u64, nanos: u64) -> u64 {
    if nanos == 0 {
        0
    } else {
        u64::try_from(u128::from(count) * 1_000_000_000 / u128::from(nanos)).unwrap_or(u64::MAX)
    }
}

fn measurement_json(m: &Measurement, input: &InputDesc) -> Value {
    let allocation = m.allocation.map_or(
        Value::Null,
        |a| json!({"count": a.count, "total_bytes": a.total_bytes, "peak_bytes": a.peak_bytes}),
    );
    json!({
        "status": "measured",
        "iterations": m.timing.samples_ns.len(),
        "warmups": m.warmups,
        "samples_ns": m.timing.samples_ns,
        "median_ns": m.timing.median_ns,
        "spread_ns": m.timing.spread_ns,
        "min_ns": m.timing.min_ns,
        "files_per_sec": per_second(input.files, m.timing.median_ns),
        "bytes_per_sec": per_second(input.bytes, m.timing.median_ns),
        "peak_rss_kb": m.peak_rss_kb,
        "counters": m.counters,
        "allocation": allocation,
    })
}

impl Document {
    /// The document as pretty-printed JSON with a trailing newline.
    pub fn to_json(&self) -> String {
        let stages: Vec<Value> = self
            .results
            .iter()
            .map(|r| {
                let input = self.inputs.iter().find(|i| i.name == r.input);
                let mut entry = match (&r.outcome, input) {
                    (Outcome::Measured(m), Some(input)) => measurement_json(m, input),
                    (Outcome::Unavailable(reason), _) => {
                        json!({"status": "unavailable", "reason": reason})
                    }
                    (Outcome::Measured(_), None) => json!({"status": "measured"}),
                };
                if let Value::Object(map) = &mut entry {
                    map.insert("stage".into(), json!(r.stage));
                    map.insert("input".into(), json!(r.input));
                }
                entry
            })
            .collect();
        let doc = json!({
            "schema_version": SCHEMA_VERSION,
            "tool": {"name": "bleedthrough-bench", "version": self.tool_version, "commit": self.commit},
            "environment": {
                "os": self.environment.os,
                "cpu_model": self.environment.cpu_model,
                "logical_cores": self.environment.logical_cores,
                "profile": self.environment.profile,
            },
            "inputs": self.inputs.iter().map(|i| json!({"name": i.name, "files": i.files, "bytes": i.bytes})).collect::<Vec<_>>(),
            "stages": stages,
        });
        let mut text = serde_json::to_string_pretty(&doc).unwrap_or_default();
        text.push('\n');
        text
    }

    /// Reads a result document; a version this build does not read is rejected naming the version.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let doc: Value =
            serde_json::from_str(text).map_err(|e| format!("result is not JSON: {e}"))?;
        let version = doc
            .get("schema_version")
            .and_then(Value::as_str)
            .ok_or("result has no schema_version")?;
        if !SUPPORTED_VERSIONS.contains(&version) {
            return Err(format!(
                "result schema version `{version}` is not supported (supported: {})",
                SUPPORTED_VERSIONS.join(", ")
            ));
        }
        let text_of = |v: Option<&Value>, what: &str| {
            v.and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| format!("result: missing {what}"))
        };
        let number = |v: Option<&Value>, what: &str| {
            v.and_then(Value::as_u64)
                .ok_or_else(|| format!("result: missing {what}"))
        };
        let env = doc.get("environment");
        let environment = Environment {
            os: text_of(env.and_then(|e| e.get("os")), "environment.os")?,
            cpu_model: text_of(
                env.and_then(|e| e.get("cpu_model")),
                "environment.cpu_model",
            )?,
            logical_cores: u32::try_from(number(
                env.and_then(|e| e.get("logical_cores")),
                "environment.logical_cores",
            )?)
            .map_err(|_| "result: logical_cores out of range")?,
            profile: text_of(env.and_then(|e| e.get("profile")), "environment.profile")?,
        };
        let mut inputs = Vec::new();
        for input in doc
            .get("inputs")
            .and_then(Value::as_array)
            .ok_or("result: missing inputs")?
        {
            inputs.push(InputDesc {
                name: text_of(input.get("name"), "input name")?,
                files: number(input.get("files"), "input files")?,
                bytes: number(input.get("bytes"), "input bytes")?,
            });
        }
        let mut results = Vec::new();
        for stage in doc
            .get("stages")
            .and_then(Value::as_array)
            .ok_or("result: missing stages")?
        {
            results.push(StageResult {
                stage: text_of(stage.get("stage"), "stage name")?,
                input: text_of(stage.get("input"), "stage input")?,
                outcome: outcome_from(stage)?,
            });
        }
        Ok(Self {
            tool_version: text_of(doc.pointer("/tool/version"), "tool.version")?,
            commit: doc
                .pointer("/tool/commit")
                .and_then(Value::as_str)
                .map(str::to_string),
            environment,
            inputs,
            results,
        })
    }
}

fn outcome_from(stage: &Value) -> Result<Outcome, String> {
    if stage.get("status").and_then(Value::as_str) == Some("unavailable") {
        let reason = stage
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("no reason given");
        return Ok(Outcome::Unavailable(reason.to_string()));
    }
    let samples: Vec<u64> = stage
        .get("samples_ns")
        .and_then(Value::as_array)
        .ok_or("result: measured stage without samples_ns")?
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    let timing = Summary::of(&samples).ok_or("result: measured stage with no samples")?;
    let counters: BTreeMap<String, u64> = stage
        .get("counters")
        .and_then(Value::as_object)
        .map(|m: &Map<String, Value>| {
            m.iter()
                .filter_map(|(k, v)| v.as_u64().map(|n| (k.clone(), n)))
                .collect()
        })
        .unwrap_or_default();
    let allocation = stage
        .get("allocation")
        .filter(|a| a.is_object())
        .map(|a| Allocation {
            count: a.get("count").and_then(Value::as_u64).unwrap_or(0),
            total_bytes: a.get("total_bytes").and_then(Value::as_u64).unwrap_or(0),
            peak_bytes: a.get("peak_bytes").and_then(Value::as_u64).unwrap_or(0),
        });
    Ok(Outcome::Measured(Box::new(Measurement {
        warmups: stage
            .get("warmups")
            .and_then(Value::as_u64)
            .and_then(|w| u32::try_from(w).ok())
            .unwrap_or(0),
        timing,
        peak_rss_kb: stage.get("peak_rss_kb").and_then(Value::as_u64),
        counters,
        allocation,
    })))
}

impl Document {
    /// Merges documents measured on one machine into one: samples of the same stage and input are
    /// concatenated (so alternating base/head rounds give each build all its samples), peak memory is
    /// the largest seen. Environments, inputs and stage lists must agree.
    pub fn merge(documents: &[Self]) -> Result<Self, String> {
        let (first, rest) = documents.split_first().ok_or("nothing to merge")?;
        let mut merged = first.clone();
        for doc in rest {
            if !doc.environment.comparable_with(&first.environment) || doc.inputs != first.inputs {
                return Err("documents to merge differ in environment or inputs".to_string());
            }
            if doc.results.len() != merged.results.len() {
                return Err("documents to merge list different stages".to_string());
            }
            for (into, from) in merged.results.iter_mut().zip(&doc.results) {
                if (&into.stage, &into.input) != (&from.stage, &from.input) {
                    return Err("documents to merge list different stages".to_string());
                }
                if let (Outcome::Measured(a), Outcome::Measured(b)) =
                    (&mut into.outcome, &from.outcome)
                {
                    let mut samples = a.timing.samples_ns.clone();
                    samples.extend(&b.timing.samples_ns);
                    a.timing = Summary::of(&samples).ok_or("no samples")?;
                    a.peak_rss_kb = a.peak_rss_kb.max(b.peak_rss_kb);
                }
            }
        }
        Ok(merged)
    }
}
