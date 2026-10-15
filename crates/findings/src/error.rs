use std::fmt;

use bleedthrough_taxonomy::TaxonomyError;

/// Term: exempt
///
/// Why a value of the finding model could not be constructed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ModelError {
    /// A rule identifier was empty or contained whitespace or control characters.
    InvalidRuleId(String),
    /// A message was empty.
    EmptyMessage,
    /// A path was empty, absolute, or contained empty, `.` or `..` segments.
    InvalidPath(String),
    /// A line or column was zero, or a range ended before it started.
    InvalidRange(String),
    /// A byte offset did not fall on a character boundary or lay outside the text.
    InvalidOffset(usize),
    /// A taxonomy tag or severity was invalid.
    Taxonomy(TaxonomyError),
    /// A confidence below high carried no reason.
    MissingReason,
    /// A reason code is not part of the vocabulary.
    UnknownReasonCode(String),
    /// A trace had fewer than two steps.
    TraceTooShort,
    /// A trace did not start at a source or did not end at a sink.
    TraceNotSourceToSink,
    /// A trace limit was below two steps.
    TraceLimitTooSmall(usize),
    /// A fix had no edits.
    EmptyFix,
    /// An edit covered an empty or reversed range, or its expected text had the wrong length.
    InvalidEdit(String),
    /// Two edits of one fix overlapped.
    OverlappingEdits(String),
    /// A fingerprint was not `bt1:` followed by 32 lowercase hex digits.
    InvalidFingerprint(String),
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRuleId(s) => write!(f, "invalid rule id {s:?}"),
            Self::EmptyMessage => f.write_str("message is empty"),
            Self::InvalidPath(s) => write!(
                f,
                "invalid path {s:?}: must be relative, non-empty and free of `.`/`..` segments"
            ),
            Self::InvalidRange(s) => write!(f, "invalid range: {s}"),
            Self::InvalidOffset(o) => write!(
                f,
                "byte offset {o} is not on a character boundary of the text"
            ),
            Self::Taxonomy(e) => write!(f, "finding taxonomy: {e}"),
            Self::MissingReason => f.write_str("confidence below high needs at least one reason"),
            Self::UnknownReasonCode(s) => write!(f, "unknown reason code {s:?}"),
            Self::TraceTooShort => f.write_str("trace needs at least a source and a sink"),
            Self::TraceNotSourceToSink => {
                f.write_str("trace must start with a source and end with a sink")
            }
            Self::TraceLimitTooSmall(n) => write!(f, "trace limit {n} is below 2"),
            Self::EmptyFix => f.write_str("fix has no edits"),
            Self::InvalidEdit(s) => write!(f, "invalid edit: {s}"),
            Self::OverlappingEdits(p) => write!(f, "fix has overlapping edits in {p}"),
            Self::InvalidFingerprint(s) => write!(f, "invalid fingerprint {s:?}"),
        }
    }
}

impl std::error::Error for ModelError {}

impl From<TaxonomyError> for ModelError {
    fn from(e: TaxonomyError) -> Self {
        Self::Taxonomy(e)
    }
}
