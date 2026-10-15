//! Parse diagnostics and the quality of a parsed file.

use crate::span::Span;

/// Term: exempt
///
/// The stable code of a [`Diagnostic`]; the spelling from [`DiagnosticCode::as_str`] is part of
/// the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum DiagnosticCode {
    /// A token appeared where the grammar does not allow it.
    UnexpectedToken,
    /// The file ended inside a construct.
    UnexpectedEof,
    /// A quoted string has no closing quote.
    UnclosedString,
    /// The nesting depth exceeded the backend's limit.
    RecursionLimit,
    /// The backend panicked or otherwise failed.
    BackendFailure,
    /// The input exceeds the size limit; nothing was parsed.
    InputTooLarge,
    /// The source holds a construct the model has no node for.
    UnsupportedConstruct,
}

impl DiagnosticCode {
    /// The stable snake_case spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnexpectedToken => "unexpected_token",
            Self::UnexpectedEof => "unexpected_eof",
            Self::UnclosedString => "unclosed_string",
            Self::RecursionLimit => "recursion_limit",
            Self::BackendFailure => "backend_failure",
            Self::InputTooLarge => "input_too_large",
            Self::UnsupportedConstruct => "unsupported_construct",
        }
    }
}

/// Term: exempt
///
/// One problem found while parsing a file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    clippy::exhaustive_structs,
    reason = "a plain record of code, message and span"
)]
pub struct Diagnostic {
    /// The stable code.
    pub code: DiagnosticCode,
    /// A human-readable message; its wording is not part of the contract.
    pub message: String,
    /// Where the problem is.
    pub span: Span,
}

impl Diagnostic {
    /// Builds a diagnostic.
    pub fn new(code: DiagnosticCode, message: impl Into<String>, span: Span) -> Self {
        Self {
            code,
            message: message.into(),
            span,
        }
    }
}

/// Term: Parse quality
///
/// How much of a file the parser understood: clean (no diagnostics and no unsupported node),
/// recovered (diagnostics or unsupported nodes, but a usable tree), or failed (no usable tree).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ParseQuality {
    /// No diagnostics and no unsupported node.
    Clean,
    /// Diagnostics or unsupported nodes, but a usable tree.
    Recovered,
    /// No usable tree.
    Failed,
}

impl ParseQuality {
    /// Applies the quality rule to the facts about a parse.
    pub fn classify(diagnostics: usize, unsupported: usize, usable_tree: bool) -> Self {
        if !usable_tree {
            Self::Failed
        } else if diagnostics == 0 && unsupported == 0 {
            Self::Clean
        } else {
            Self::Recovered
        }
    }

    /// The stable lowercase spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Recovered => "recovered",
            Self::Failed => "failed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_rule() {
        assert_eq!(ParseQuality::classify(0, 0, true), ParseQuality::Clean);
        assert_eq!(ParseQuality::classify(1, 0, true), ParseQuality::Recovered);
        assert_eq!(ParseQuality::classify(0, 2, true), ParseQuality::Recovered);
        assert_eq!(ParseQuality::classify(0, 0, false), ParseQuality::Failed);
        assert_eq!(ParseQuality::classify(3, 3, false), ParseQuality::Failed);
    }

    #[test]
    fn codes_have_stable_spellings() {
        assert_eq!(DiagnosticCode::UnexpectedEof.as_str(), "unexpected_eof");
        assert_eq!(DiagnosticCode::InputTooLarge.as_str(), "input_too_large");
        assert_eq!(ParseQuality::Recovered.as_str(), "recovered");
    }
}
