//! Diagnostics produced while summarising, resolving and typing; none of them stops an analysis.

use bleedthrough_syntax::Span;

/// Term: exempt
///
/// The stable code of a [`SymbolDiagnostic`]; the spelling from [`SymbolDiagnosticCode::as_str`] is
/// part of the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum SymbolDiagnosticCode {
    /// Two imports in one namespace block use the same alias.
    DuplicateImport,
    /// A docblock type or tag could not be parsed.
    DocblockSyntax,
    /// A docblock construct outside the supported subset was ignored.
    DocblockUnsupported,
    /// A docblock type is provably disjoint from the native type; the native type is used.
    DocblockContradiction,
    /// A docblock type could not be shown to refine the native type; the native type is used.
    DocblockUnverified,
    /// A type text (built-in data or declaration) could not be converted.
    TypeConversion,
    /// A built-in symbol could not be added to the table.
    BuiltinData,
}

impl SymbolDiagnosticCode {
    /// The stable snake_case spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DuplicateImport => "duplicate_import",
            Self::DocblockSyntax => "docblock_syntax",
            Self::DocblockUnsupported => "docblock_unsupported",
            Self::DocblockContradiction => "docblock_contradiction",
            Self::DocblockUnverified => "docblock_unverified",
            Self::TypeConversion => "type_conversion",
            Self::BuiltinData => "builtin_data",
        }
    }
}

/// Term: exempt
///
/// One problem found in a single file; the span lies in that file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(
    clippy::exhaustive_structs,
    reason = "a plain record of code, message and span"
)]
pub struct SymbolDiagnostic {
    /// The stable code.
    pub code: SymbolDiagnosticCode,
    /// A human-readable message; its wording is not part of the contract.
    pub message: String,
    /// Where the problem is.
    pub span: Span,
}

impl SymbolDiagnostic {
    /// Builds a diagnostic.
    pub fn new(code: SymbolDiagnosticCode, message: impl Into<String>, span: Span) -> Self {
        Self {
            code,
            message: message.into(),
            span,
        }
    }
}
