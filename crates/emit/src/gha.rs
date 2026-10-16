//! The `github-actions` emitter.

use std::io::Write;

use bleedthrough_app::{EmitContext, EmitError, EmitOutcome, Emitter, Format};
use bleedthrough_findings::{Finding, Report};
use bleedthrough_taxonomy::SeverityLevel;

use crate::summary;

/// Escapes a message: `%`, `\r`, `\n` as `%25`, `%0D`, `%0A`.
pub fn escape_message(text: &str) -> String {
    text.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// Escapes a property value: as a message, plus `:` and `,` as `%3A` and `%2C`.
pub fn escape_property(text: &str) -> String {
    escape_message(text).replace(':', "%3A").replace(',', "%2C")
}

fn command(level: SeverityLevel) -> &'static str {
    match level {
        SeverityLevel::Critical | SeverityLevel::High => "error",
        SeverityLevel::Medium => "warning",
        _ => "notice",
    }
}

fn annotation(f: &Finding) -> String {
    let loc = f.location();
    format!(
        "::{} file={},line={},col={},endLine={},endColumn={},title={}::{}\n",
        command(f.severity().level()),
        escape_property(loc.path().as_str()),
        loc.start().line(),
        loc.start().column(),
        loc.end().line(),
        loc.end().column(),
        escape_property(f.rule().as_str()),
        escape_message(f.message().as_str()),
    )
}

fn notice(title: &str, message: &str) -> String {
    format!(
        "::notice title={}::{}\n",
        escape_property(title),
        escape_message(message)
    )
}

pub struct GithubActionsEmitter;

impl Emitter for GithubActionsEmitter {
    fn format(&self) -> Format {
        Format::GithubActions
    }

    fn emit(
        &self,
        report: &Report,
        ctx: &EmitContext<'_>,
        out: &mut dyn Write,
    ) -> Result<EmitOutcome, EmitError> {
        let mut chosen: Vec<&Finding> = report.findings().iter().collect();
        let omitted = chosen.len().saturating_sub(ctx.annotation_cap);
        if omitted > 0 {
            // Stable sort: ties keep the standard finding order.
            chosen.sort_by(|a, b| {
                (b.severity().level(), b.confidence().tier())
                    .cmp(&(a.severity().level(), a.confidence().tier()))
            });
            chosen.truncate(ctx.annotation_cap);
        }
        let mut text: String = chosen.into_iter().map(annotation).collect();
        if omitted > 0 {
            let message = format!(
                "{omitted} more finding(s) omitted: annotation cap of {} reached; use another format for the full report",
                ctx.annotation_cap
            );
            text += &notice("bleedthrough: annotations omitted", &message);
        }
        text += &notice("bleedthrough: coverage", &summary::coverage_line(report));
        out.write_all(text.as_bytes())?;
        out.flush()?;
        Ok(EmitOutcome::new())
    }
}

#[cfg(test)]
pub fn unescape(text: &str) -> String {
    text.replace("%2C", ",")
        .replace("%3A", ":")
        .replace("%0A", "\n")
        .replace("%0D", "\r")
        .replace("%25", "%")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testgen::hostile_strings;

    #[test]
    fn escapes_round_trip_and_leave_one_line() {
        for s in hostile_strings() {
            let (m, p) = (escape_message(&s), escape_property(&s));
            assert!(!m.contains(['\n', '\r']) && !p.contains(['\n', '\r', ':', ',']));
            assert_eq!(unescape(&m), s);
            assert_eq!(unescape(&p), s);
        }
    }

    /// spec: report-emitters/GitHub Actions format
    #[test]
    fn documented_escapes() {
        assert_eq!(escape_message("100%\nx:y"), "100%25%0Ax:y");
        assert_eq!(escape_property("a:b,c"), "a%3Ab%2Cc");
    }
}
