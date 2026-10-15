//! The `diff` emitter: a `git apply`-compatible unified diff of the suggested fixes.

use std::collections::BTreeMap;
use std::io::Write;

use bleedthrough_app::{
    EmitContext, EmitError, EmitOutcome, Emitter, Format, SkippedFix, SourceText,
};
use bleedthrough_findings::{Edit, Fix, RelPath, Report};
use similar::TextDiff;

use crate::summary;

/// An accepted edit's byte range and replacement.
struct Accepted {
    start: usize,
    end: usize,
    replacement: String,
}

type Files = BTreeMap<RelPath, (String, Vec<Accepted>)>;

fn overlaps(a: &Accepted, e: &Edit) -> bool {
    e.start() < a.end && a.start < e.end()
}

/// Loads the file text on first use; `None` when the source is gone.
fn load<'a>(
    files: &'a mut Files,
    sources: &dyn SourceText,
    path: &RelPath,
) -> Option<&'a mut (String, Vec<Accepted>)> {
    if !files.contains_key(path) {
        files.insert(path.clone(), (sources.source(path)?, Vec::new()));
    }
    files.get_mut(path)
}

/// Why the fix cannot be included, if it cannot.
fn conflict(fix: &Fix, files: &mut Files, sources: &dyn SourceText) -> Option<String> {
    for edit in fix.edits() {
        let Some((text, accepted)) = load(files, sources, edit.path()) else {
            return Some(format!(
                "source of {} is no longer available",
                edit.path().as_str()
            ));
        };
        if text.get(edit.start()..edit.end()) != Some(edit.expected()) {
            return Some(format!(
                "{} changed since it was analysed",
                edit.path().as_str()
            ));
        }
        if accepted.iter().any(|a| overlaps(a, edit)) {
            return Some(format!(
                "overlaps a fix already included in {}",
                edit.path().as_str()
            ));
        }
    }
    None
}

fn apply(text: &str, accepted: &mut [Accepted]) -> String {
    accepted.sort_by_key(|a| a.start);
    let mut out = text.to_string();
    for a in accepted.iter().rev() {
        out.replace_range(a.start..a.end, &a.replacement);
    }
    out
}

pub struct DiffEmitter;

impl Emitter for DiffEmitter {
    fn format(&self) -> Format {
        Format::Diff
    }

    fn emit(
        &self,
        report: &Report,
        ctx: &EmitContext<'_>,
        out: &mut dyn Write,
    ) -> Result<EmitOutcome, EmitError> {
        let mut files = Files::new();
        let mut skipped = Vec::new();
        let with_fix = || {
            report
                .findings()
                .iter()
                .filter_map(|f| f.fix().map(|fix| (f, fix)))
        };
        if with_fix().next().is_some() && ctx.sources.is_none() {
            return Err(EmitError::Unavailable(
                "the diff format needs access to the original source text".into(),
            ));
        }
        for (finding, fix) in with_fix() {
            let sources = ctx.sources.expect("checked above");
            match conflict(fix, &mut files, sources) {
                Some(reason) => {
                    skipped.push(SkippedFix::new(finding.fingerprint().clone(), reason))
                }
                None => {
                    for e in fix.edits() {
                        let (_, accepted) =
                            files.get_mut(e.path()).expect("loaded by conflict check");
                        accepted.push(Accepted {
                            start: e.start(),
                            end: e.end(),
                            replacement: e.replacement().to_string(),
                        });
                    }
                }
            }
        }
        let mut patch = String::new();
        for (path, (text, accepted)) in &mut files {
            if accepted.is_empty() {
                continue;
            }
            let new = apply(text, accepted);
            let (a, b) = (
                format!("a/{}", path.as_str()),
                format!("b/{}", path.as_str()),
            );
            patch += &format!("diff --git {a} {b}\n");
            let diff = TextDiff::from_lines(text.as_str(), new.as_str());
            patch += &diff
                .unified_diff()
                .context_radius(3)
                .header(&a, &b)
                .to_string();
        }
        out.write_all(patch.as_bytes())?;
        out.flush()?;
        Ok(EmitOutcome::new()
            .with_coverage_summary(summary::coverage_line(report))
            .with_skipped_fixes(skipped))
    }
}
