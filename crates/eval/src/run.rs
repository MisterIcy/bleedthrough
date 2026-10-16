//! Running an evaluation: obtain the report (by running the binary, or from a file), score it,
//! and compare it with the expectations.

use std::path::Path;
use std::process::Command;

use crate::corpus::Corpus;
use crate::expectations::Expectations;
use crate::matching::score;
use crate::report;
use crate::result::{Evaluation, Verdict};

/// What obtaining a report produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Obtained {
    /// The report text.
    Report(String),
    /// There is nothing to evaluate yet.
    NoScan(String),
    /// The scan failed.
    Failed(String),
}

/// Runs `<binary> scan <root> --format json` and classifies the outcome.
///
/// No scan exists when the binary prints nothing and exits successfully, exits with the usage
/// status 2, or says the command is not implemented. Anything else that fails is a failure.
pub fn run_scan(binary: &Path, root: &Path) -> Obtained {
    let output = match Command::new(binary)
        .arg("scan")
        .arg(root)
        .args(["--format", "json"])
        .output()
    {
        Ok(output) => output,
        Err(e) => return Obtained::Failed(format!("cannot run {}: {e}", binary.display())),
    };
    classify(
        output.status.code(),
        &String::from_utf8_lossy(&output.stdout),
        &String::from_utf8_lossy(&output.stderr),
    )
}

/// Classifies a finished scan process; see [`run_scan`].
pub fn classify(code: Option<i32>, stdout: &str, stderr: &str) -> Obtained {
    let said_unimplemented =
        stderr.contains("not implemented") || stdout.contains("not implemented");
    if said_unimplemented || code == Some(2) || (code == Some(0) && stdout.trim().is_empty()) {
        return Obtained::NoScan(
            "the `bleedthrough` binary offers no scan command yet (it printed no report)"
                .to_string(),
        );
    }
    if code != Some(0) {
        return Obtained::Failed(format!(
            "the scan exited with status {}: {}",
            code.map_or_else(|| "none (signal)".to_string(), |c| c.to_string()),
            stderr.trim()
        ));
    }
    Obtained::Report(stdout.to_string())
}

/// Scores a report text against the corpus and the expectations.
pub fn evaluate_report(corpus: &Corpus, expectations: &Expectations, text: &str) -> Evaluation {
    match report::read(text) {
        Err(e) => Evaluation::without_metrics(Verdict::Fail, None, vec![e]),
        Ok(findings) => {
            let scored = score(corpus, &findings);
            let problems = expectations.compare(corpus, &scored);
            Evaluation {
                verdict: if problems.is_empty() {
                    Verdict::Pass
                } else {
                    Verdict::Fail
                },
                reason: None,
                scored: Some(scored),
                problems,
            }
        }
    }
}

/// Evaluates what [`Obtained`] produced.
pub fn evaluate(corpus: &Corpus, expectations: &Expectations, obtained: &Obtained) -> Evaluation {
    match obtained {
        Obtained::Report(text) => evaluate_report(corpus, expectations, text),
        Obtained::NoScan(reason) => {
            Evaluation::without_metrics(Verdict::NotEvaluated, Some(reason.clone()), Vec::new())
        }
        Obtained::Failed(why) => {
            Evaluation::without_metrics(Verdict::Fail, None, vec![why.clone()])
        }
    }
}

/// The exit status in CI mode: `not evaluated` passes only while the scan switch is false.
pub fn ci_exit_code(verdict: Verdict, expectations: &Expectations) -> u8 {
    match verdict {
        Verdict::NotEvaluated if !expectations.scan_switch => 0,
        Verdict::NotEvaluated => 1,
        other => other.exit_code(),
    }
}

/// Scores a report without comparing it with expectations: for generated label files, where there is
/// nothing committed to ratchet against. The verdict is `pass` when the report is valid.
pub fn evaluate_metrics_only(corpus: &Corpus, text: &str) -> Evaluation {
    match report::read(text) {
        Err(e) => Evaluation::without_metrics(Verdict::Fail, None, vec![e]),
        Ok(findings) => Evaluation {
            verdict: Verdict::Pass,
            reason: None,
            scored: Some(score(corpus, &findings)),
            problems: Vec::new(),
        },
    }
}
