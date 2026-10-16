//! Stages: the named things a benchmark measures.
//!
//! A stage is registered, not wired in: adding one later (symbol table, analysis) needs no change to
//! the harness or the result format. A stage that cannot run yet reports why instead of a number.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::memory;
use crate::timing::{Config, Summary, sample};

/// A stage's name and what measuring it produced.
pub type Named = (String, Outcome);

/// Counters a pass reports (files, bytes, nodes, errors ...): exact for a given input.
pub type Counters = BTreeMap<String, u64>;

/// A named, reproducible input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDesc {
    /// `synthetic:<parameters>` or `real:<project id>`.
    pub name: String,
    /// Files in the input.
    pub files: u64,
    /// Bytes in the input.
    pub bytes: u64,
}

/// What one pass allocated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allocation {
    /// Number of allocations.
    pub count: u64,
    /// Total bytes allocated.
    pub total_bytes: u64,
    /// Peak live bytes.
    pub peak_bytes: u64,
}

/// A measured stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measurement {
    /// Warm-up passes run.
    pub warmups: u32,
    /// Timing summary.
    pub timing: Summary,
    /// Peak resident memory of the process after the run, if the platform exposes it.
    pub peak_rss_kb: Option<u64>,
    /// Counters of the last pass.
    pub counters: Counters,
    /// Allocation counters, when the allocation mode ran.
    pub allocation: Option<Allocation>,
}

/// The outcome of one stage on one input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The stage ran.
    Measured(Box<Measurement>),
    /// The stage cannot run; never omitted, never zero.
    Unavailable(String),
}

/// One thing to measure.
pub trait Stage {
    /// The stage's name in results and comparisons, for example `parse`.
    fn name(&self) -> String;

    /// Why the stage cannot run now, or `None` when it can.
    fn unavailable(&self) -> Option<String> {
        None
    }

    /// One pass over the already-loaded input; the harness times this call and nothing else.
    fn pass(&mut self);

    /// The structural counters of the input (files, bytes, nodes ...), computed outside the timed
    /// region. They must be the same every time it is called.
    fn counters(&mut self) -> Counters;
}

/// Runs a stage on its input: timing in `config`, peak memory afterwards.
pub fn measure(stage: &mut dyn Stage, config: &Config) -> Result<Outcome, String> {
    if let Some(reason) = stage.unavailable() {
        return Ok(Outcome::Unavailable(reason));
    }
    let before = stage.counters();
    let samples = sample(config, || stage.pass())?;
    let counters = stage.counters();
    if before != counters {
        return Err(format!(
            "stage `{}` reported different counters between passes: it is not deterministic",
            stage.name()
        ));
    }
    let timing = Summary::of(&samples).ok_or("no samples")?;
    Ok(Outcome::Measured(Box::new(Measurement {
        warmups: config.warmups,
        timing,
        peak_rss_kb: memory::peak_rss_kb(),
        counters,
        allocation: None,
    })))
}

/// The registered stages for one input.
#[derive(Default)]
pub struct Registry<'a> {
    stages: Vec<Box<dyn Stage + 'a>>,
}

impl<'a> Registry<'a> {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a stage; results list stages in registration order.
    pub fn register(&mut self, stage: Box<dyn Stage + 'a>) {
        self.stages.push(stage);
    }

    /// Measures every stage, in order.
    pub fn run(&mut self, config: &Config) -> Result<Vec<Named>, String> {
        let mut results = Vec::new();
        for stage in &mut self.stages {
            results.push((stage.name(), measure(stage.as_mut(), config)?));
        }
        Ok(results)
    }
}

/// The end-to-end stage: the `bleedthrough` binary run as a user would run it, as a subprocess
/// (`<binary> scan <root> --format json`).
///
/// The stage is unavailable while the binary offers no scan command: it prints no report, exits with
/// the usage status 2, or says it is not implemented (the same rule as the accuracy evaluation). The
/// timed region is the whole process: start-up, file discovery and reading, parsing, analysis and
/// writing the report to a pipe.
#[derive(Debug)]
pub struct EndToEnd {
    binary: PathBuf,
    root: PathBuf,
    unavailable: Option<String>,
}

fn run_scan(binary: &Path, root: &Path) -> std::io::Result<std::process::Output> {
    Command::new(binary)
        .arg("scan")
        .arg(root)
        .args(["--format", "json"])
        .output()
}

/// Why the output of a scan means "no scan command yet", if it does.
pub fn no_scan_reason(code: Option<i32>, stdout: &[u8], stderr: &[u8]) -> Option<String> {
    let said = |bytes: &[u8]| String::from_utf8_lossy(bytes).contains("not implemented");
    let silent = code == Some(0) && stdout.iter().all(u8::is_ascii_whitespace);
    (said(stdout) || said(stderr) || code == Some(2) || silent).then(|| {
        "the `bleedthrough` binary offers no scan command yet (it printed no report)".to_string()
    })
}

impl EndToEnd {
    /// Probes the binary once on `root` to decide whether the stage can run.
    pub fn new(binary: PathBuf, root: PathBuf) -> Self {
        let unavailable = match run_scan(&binary, &root) {
            Err(e) => Some(format!("cannot run {}: {e}", binary.display())),
            Ok(out) => no_scan_reason(out.status.code(), &out.stdout, &out.stderr).or_else(|| {
                (out.status.code() != Some(0)).then(|| {
                    format!(
                        "the scan exited with {:?}: {}",
                        out.status.code(),
                        String::from_utf8_lossy(&out.stderr).trim()
                    )
                })
            }),
        };
        Self {
            binary,
            root,
            unavailable,
        }
    }
}

impl Stage for EndToEnd {
    fn name(&self) -> String {
        "end-to-end".to_string()
    }

    fn unavailable(&self) -> Option<String> {
        self.unavailable.clone()
    }

    fn pass(&mut self) {
        let _ = run_scan(&self.binary, &self.root);
    }

    fn counters(&mut self) -> Counters {
        let report_bytes = run_scan(&self.binary, &self.root).map_or(0, |o| o.stdout.len() as u64);
        Counters::from([("report_bytes".to_string(), report_bytes)])
    }
}
