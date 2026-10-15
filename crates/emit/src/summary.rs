//! The one-line coverage summary for formats with no place to embed coverage.

use bleedthrough_findings::Report;

/// Describes what was and was not analysed, and what was hidden, in one line.
pub fn coverage_line(report: &Report) -> String {
    let c = report.coverage();
    let not_analysed = c.files_failed + c.skipped.len() as u64;
    let mut parts = Vec::new();
    if not_analysed > 0 {
        parts.push(format!(
            "{not_analysed} file(s) not analysed ({} failed to parse, {} skipped)",
            c.files_failed,
            c.skipped.len()
        ));
    }
    if c.files_recovered > 0 {
        parts.push(format!(
            "{} file(s) parsed with error recovery",
            c.files_recovered
        ));
    }
    if !c.limits_hit.is_empty() {
        parts.push(format!("{} analysis limit(s) hit", c.limits_hit.len()));
    }
    if c.unresolved_includes + c.unresolved_calls > 0 {
        parts.push(format!(
            "{} unresolved dynamic include(s), {} unresolved dynamic call(s)",
            c.unresolved_includes, c.unresolved_calls
        ));
    }
    if parts.is_empty() {
        parts.push(format!("all {} file(s) analysed", c.files_clean));
    }
    let s = report.suppressed();
    if s.below_threshold_low + s.below_threshold_medium + s.baselined > 0 {
        parts.push(format!(
            "suppressed: {} low and {} medium confidence, {} by baseline",
            s.below_threshold_low, s.below_threshold_medium, s.baselined
        ));
    }
    format!("coverage: {}", parts.join("; "))
}

/// Whether every file was examined within the limits: no failed or skipped files, no limits hit.
pub fn analysis_complete(report: &Report) -> bool {
    let c = report.coverage();
    c.files_failed == 0 && c.skipped.is_empty() && c.limits_hit.is_empty()
}
