//! The parser-backend port: input, options, result and the trait every backend implements.

use crate::diagnostic::{Diagnostic, DiagnosticCode, ParseQuality};
use crate::span::Span;
use crate::stmt::StmtKind;
use crate::tree::SyntaxTree;

/// Term: exempt
///
/// How a backend treats incomplete or broken code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum RecoveryMode {
    /// Return only what complete statements yield; broken regions become error nodes.
    #[default]
    Strict,
    /// Also keep unfinished constructs at the end of the file, still reporting what is missing.
    Editor,
}

/// Term: exempt
///
/// Backend-neutral parse options. Build with `ParseOptions::default()` and assign the fields.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParseOptions {
    /// Whether `<?` opens PHP code. Off by default.
    pub short_tags: bool,
    /// Strict or editor recovery.
    pub recovery: RecoveryMode,
    /// Inputs longer than this many bytes are not parsed. Provisionally 8 MiB.
    pub max_bytes: u32,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            short_tags: false,
            recovery: RecoveryMode::Strict,
            max_bytes: 8 * 1024 * 1024,
        }
    }
}

/// Term: exempt
///
/// The identity of a backend: a name and the exact version of the parser it wraps. Usable in cache
/// keys and coverage information.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(
    clippy::exhaustive_structs,
    reason = "a plain pair of name and version"
)]
pub struct BackendInfo {
    /// The backend name, for example `mago-syntax`.
    pub name: String,
    /// The exact version of the wrapped parser.
    pub version: String,
}

/// Term: exempt
///
/// The result of parsing one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFile {
    tree: SyntaxTree,
    diagnostics: Vec<Diagnostic>,
    quality: ParseQuality,
    unsupported: usize,
    errors: usize,
    backend: BackendInfo,
}

impl ParsedFile {
    /// Builds a result, counting holes and classifying quality. A tree whose statements are all
    /// error nodes is not usable and the result is `Failed`.
    pub fn new(tree: SyntaxTree, diagnostics: Vec<Diagnostic>, backend: BackendInfo) -> Self {
        let (unsupported, errors) = tree.hole_counts();
        let only_errors = !tree.statements().is_empty()
            && tree
                .statements()
                .iter()
                .all(|s| matches!(s.kind, StmtKind::Error { .. }));
        let quality = ParseQuality::classify(diagnostics.len(), unsupported, !only_errors);
        Self {
            tree,
            diagnostics,
            quality,
            unsupported,
            errors,
            backend,
        }
    }

    /// A failed result with an empty tree and one explaining diagnostic.
    pub fn failed(
        source_len: u32,
        code: DiagnosticCode,
        message: impl Into<String>,
        backend: BackendInfo,
    ) -> Self {
        let diagnostic = Diagnostic::new(code, message, Span::new(0, 0));
        Self {
            tree: SyntaxTree::empty(source_len),
            diagnostics: vec![diagnostic],
            quality: ParseQuality::Failed,
            unsupported: 0,
            errors: 0,
            backend,
        }
    }

    /// The syntax tree (empty when the parse failed).
    pub fn tree(&self) -> &SyntaxTree {
        &self.tree
    }

    /// The diagnostics in source order.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Clean, recovered or failed.
    pub fn quality(&self) -> ParseQuality {
        self.quality
    }

    /// Number of `Unsupported` nodes.
    pub fn unsupported_count(&self) -> usize {
        self.unsupported
    }

    /// Number of `Error` nodes.
    pub fn error_count(&self) -> usize {
        self.errors
    }

    /// The backend that produced this result.
    pub fn backend(&self) -> &BackendInfo {
        &self.backend
    }
}

/// Term: exempt
///
/// The port every parser backend implements. A backend never panics or aborts for any input, does
/// no I/O, is safe to call from many threads, and returns equal results for equal input.
///
/// An input longer than `options.max_bytes` is not parsed: the result is
/// [`ParsedFile::failed`] with [`DiagnosticCode::InputTooLarge`].
///
/// ```
/// use bleedthrough_syntax::{BackendInfo, ParseOptions, ParsedFile, ParserBackend, SyntaxTree};
///
/// struct Empty;
///
/// impl ParserBackend for Empty {
///     fn info(&self) -> BackendInfo {
///         BackendInfo { name: "empty".into(), version: "0".into() }
///     }
///     fn parse(&self, _path: &str, source: &[u8], _options: &ParseOptions) -> ParsedFile {
///         let len = u32::try_from(source.len()).unwrap_or(u32::MAX);
///         ParsedFile::new(SyntaxTree::empty(len), Vec::new(), self.info())
///     }
/// }
///
/// let result = Empty.parse("a.php", b"", &ParseOptions::default());
/// assert_eq!(result.quality().as_str(), "clean");
/// ```
pub trait ParserBackend: Send + Sync {
    /// The backend's name and the version of the parser it wraps.
    fn info(&self) -> BackendInfo;

    /// Parses `source`, the bytes of the file at `path` (relative to the project root, used only
    /// for labelling), which need not be valid UTF-8.
    fn parse(&self, path: &str, source: &[u8], options: &ParseOptions) -> ParsedFile;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let options = ParseOptions::default();
        assert!(!options.short_tags);
        assert_eq!(options.recovery, RecoveryMode::Strict);
        assert_eq!(options.max_bytes, 8 * 1024 * 1024);
    }

    #[test]
    fn failed_result_has_one_diagnostic() {
        let info = BackendInfo {
            name: "x".into(),
            version: "1".into(),
        };
        let result = ParsedFile::failed(10, DiagnosticCode::InputTooLarge, "too big", info);
        assert_eq!(result.quality(), ParseQuality::Failed);
        assert_eq!(result.diagnostics().len(), 1);
        assert!(result.tree().statements().is_empty());
    }
}
