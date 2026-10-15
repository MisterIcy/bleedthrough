use std::fmt;
use std::io::{self, Write};

use bleedthrough_findings::{Fingerprint, RelPath, Report};

use crate::Format;

/// Default cap on annotations for the `github-actions` format.
///
/// GitHub's documented limits (read 2026-10-15; Checks REST API docs and the actions/toolkit
/// problem-matcher docs): 10 error, 10 warning and 10 notice annotations per step, and 50 per
/// job across steps. The primary workflow-commands page states no figure. Staying at 10 in
/// total keeps one step within every per-type limit.
pub const DEFAULT_ANNOTATION_CAP: usize = 10;

/// Read access to the original text of analysed files, as the analysis saw them.
pub trait SourceText {
    /// The text of `path`, or `None` when it is no longer available.
    fn source(&self, path: &RelPath) -> Option<String>;
}

/// Options and services an emitter may use.
#[derive(Clone, Copy)]
#[non_exhaustive]
pub struct EmitContext<'a> {
    /// Whether tabular formats write their header row.
    pub header: bool,
    /// Most annotations the `github-actions` format writes.
    pub annotation_cap: usize,
    /// Original source text, needed by the `diff` format.
    pub sources: Option<&'a dyn SourceText>,
}

impl Default for EmitContext<'_> {
    fn default() -> Self {
        Self {
            header: true,
            annotation_cap: DEFAULT_ANNOTATION_CAP,
            sources: None,
        }
    }
}

impl fmt::Debug for EmitContext<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmitContext")
            .field("header", &self.header)
            .field("annotation_cap", &self.annotation_cap)
            .field("sources", &self.sources.map(|_| "<provided>"))
            .finish()
    }
}

impl<'a> EmitContext<'a> {
    /// Defaults: header on, the default annotation cap, no source access.
    pub fn new() -> Self {
        Self::default()
    }

    /// Turns the header row on or off.
    pub fn with_header(mut self, header: bool) -> Self {
        self.header = header;
        self
    }

    /// Sets the annotation cap.
    pub fn with_annotation_cap(mut self, cap: usize) -> Self {
        self.annotation_cap = cap;
        self
    }

    /// Provides access to the original source text.
    pub fn with_sources(mut self, sources: &'a dyn SourceText) -> Self {
        self.sources = Some(sources);
        self
    }
}

/// A fix that an emitter left out, with the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedFix {
    fingerprint: Fingerprint,
    reason: String,
}

impl SkippedFix {
    /// Records that the fix of the finding with this fingerprint was skipped.
    pub fn new(fingerprint: Fingerprint, reason: impl Into<String>) -> Self {
        Self {
            fingerprint,
            reason: reason.into(),
        }
    }

    /// The fingerprint of the finding whose fix was skipped.
    pub fn fingerprint(&self) -> &Fingerprint {
        &self.fingerprint
    }

    /// Why it was skipped.
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// What an emitter reports back besides the bytes it wrote.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct EmitOutcome {
    /// One human-readable line for formats with no place to embed coverage; the caller shows it.
    pub coverage_summary: Option<String>,
    /// Fixes left out of a `diff`, with reasons.
    pub skipped_fixes: Vec<SkippedFix>,
}

impl EmitOutcome {
    /// An outcome with nothing to report.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the coverage summary line.
    pub fn with_coverage_summary(mut self, summary: impl Into<String>) -> Self {
        self.coverage_summary = Some(summary.into());
        self
    }

    /// Sets the skipped fixes.
    pub fn with_skipped_fixes(mut self, skipped: Vec<SkippedFix>) -> Self {
        self.skipped_fixes = skipped;
        self
    }
}

/// Why emission failed. The caller must not treat the output as complete.
#[derive(Debug)]
#[non_exhaustive]
pub enum EmitError {
    /// The destination could not be written.
    Io(io::Error),
    /// The format needs something the context did not provide.
    Unavailable(String),
}

impl fmt::Display for EmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "cannot write report: {e}"),
            Self::Unavailable(what) => write!(f, "cannot emit: {what}"),
        }
    }
}

impl std::error::Error for EmitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Unavailable(_) => None,
        }
    }
}

impl From<io::Error> for EmitError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Driven port: writes a [`Report`] in one output format. Output is UTF-8 and byte-identical
/// for identical reports.
pub trait Emitter {
    /// The format this emitter writes.
    fn format(&self) -> Format;

    /// Writes the report to `out`. Returns an error, never success, when the write fails.
    fn emit(
        &self,
        report: &Report,
        context: &EmitContext<'_>,
        out: &mut dyn Write,
    ) -> Result<EmitOutcome, EmitError>;
}
