//! The `tsv` emitter.

use std::io::Write;

use bleedthrough_app::{EmitContext, EmitError, EmitOutcome, Emitter, Format};
use bleedthrough_findings::{Finding, Report};

use crate::summary;

const HEADER: [&str; 12] = [
    "path",
    "line",
    "column",
    "end_line",
    "end_column",
    "rule",
    "severity",
    "confidence",
    "cwe",
    "owasp",
    "fingerprint",
    "message",
];

/// Escapes tab, newline, carriage return and backslash as `\t`, `\n`, `\r`, `\\`.
pub fn escape(cell: &str) -> String {
    let mut out = String::with_capacity(cell.len());
    for c in cell.chars() {
        match c {
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\\' => out.push_str("\\\\"),
            other => out.push(other),
        }
    }
    out
}

fn row(f: &Finding) -> [String; 12] {
    let (loc, tax) = (f.location(), f.taxonomy());
    [
        loc.path().as_str().to_string(),
        loc.start().line().to_string(),
        loc.start().column().to_string(),
        loc.end().line().to_string(),
        loc.end().column().to_string(),
        f.rule().as_str().to_string(),
        f.severity().level().as_str().to_string(),
        f.confidence().tier().as_str().to_string(),
        tax.cwe()
            .iter()
            .map(|c| format!("CWE-{}", c.number()))
            .collect::<Vec<_>>()
            .join(","),
        tax.owasp()
            .iter()
            .map(|o| format!("{}:{}", o.id(), o.edition()))
            .collect::<Vec<_>>()
            .join(","),
        f.fingerprint().as_str().to_string(),
        f.message().as_str().to_string(),
    ]
}

fn line(cells: &[String]) -> String {
    let escaped: Vec<String> = cells.iter().map(|c| escape(c)).collect();
    escaped.join("\t") + "\n"
}

pub struct TsvEmitter;

impl Emitter for TsvEmitter {
    fn format(&self) -> Format {
        Format::Tsv
    }

    fn emit(
        &self,
        report: &Report,
        ctx: &EmitContext<'_>,
        out: &mut dyn Write,
    ) -> Result<EmitOutcome, EmitError> {
        let mut text = String::new();
        if ctx.header {
            text += &line(&HEADER.map(str::to_string));
        }
        for finding in report.findings() {
            text += &line(&row(finding));
        }
        out.write_all(text.as_bytes())?;
        out.flush()?;
        Ok(EmitOutcome::new().with_coverage_summary(summary::coverage_line(report)))
    }
}

#[cfg(test)]
pub fn unescape(cell: &str) -> String {
    let mut out = String::new();
    let mut chars = cell.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testgen::hostile_strings;

    #[test]
    fn escape_round_trips_and_stays_on_one_line() {
        for s in hostile_strings() {
            let e = escape(&s);
            assert!(!e.contains(['\t', '\n', '\r']), "{e:?}");
            assert_eq!(unescape(&e), s);
        }
    }

    /// spec: report-emitters/TSV format
    #[test]
    fn documented_escapes() {
        assert_eq!(escape("a\tb\nc\r\\"), "a\\tb\\nc\\r\\\\");
    }
}
