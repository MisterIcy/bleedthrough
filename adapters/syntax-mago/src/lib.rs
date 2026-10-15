//! **Layer:** Infrastructure (parser adapter).
//!
//! **Responsibility:** implements the `ParserBackend` port with the Mago PHP parser.
//!
//! **Permitted workspace dependencies:** `bleedthrough-syntax`.

mod convert;
mod errors;

use std::panic::{AssertUnwindSafe, catch_unwind};

use bleedthrough_syntax::{
    BackendInfo, Comment, CommentKind, DiagnosticCode, ParseOptions, ParsedFile, ParserBackend,
    RecoveryMode, Span, SyntaxTree,
};
use mago_allocator::LocalArena;
use mago_database::file::FileId;
use mago_syntax::cst::{Trivia, TriviaKind};
use mago_syntax::parser::parse_file_content_with_settings;
use mago_syntax::settings::{LexerSettings, ParserSettings};

/// The exact Mago version this adapter is built and tested against. A test checks it against
/// `Cargo.lock`, so the reported identity cannot drift from the code that runs.
const MAGO_VERSION: &str = "1.57.0";

/// Term: exempt
///
/// The Mago-backed implementation of the `ParserBackend` port.
#[derive(Debug, Clone, Copy, Default)]
#[allow(
    clippy::exhaustive_structs,
    reason = "a unit-like backend with no state"
)]
pub struct MagoBackend;

impl MagoBackend {
    /// Creates the backend.
    pub fn new() -> Self {
        Self
    }
}

fn comment_kind(kind: TriviaKind) -> Option<CommentKind> {
    match kind {
        TriviaKind::WhiteSpace => None,
        TriviaKind::SingleLineComment => Some(CommentKind::Line),
        TriviaKind::MultiLineComment => Some(CommentKind::Block),
        TriviaKind::HashComment => Some(CommentKind::Hash),
        TriviaKind::DocBlockComment => Some(CommentKind::Doc),
    }
}

fn comments(trivia: &[&Trivia<'_>]) -> Vec<Comment> {
    trivia
        .iter()
        .filter_map(|t| {
            let kind = comment_kind(t.kind)?;
            Some(Comment {
                kind,
                span: errors::span(t.span),
                text: String::from_utf8_lossy(t.value).into(),
            })
        })
        .collect()
}

fn parse_inner(source: &[u8], options: &ParseOptions, info: &BackendInfo) -> ParsedFile {
    let arena = LocalArena::new();
    let settings = ParserSettings {
        lexer: LexerSettings {
            enable_short_tags: options.short_tags,
        },
        recover_incomplete: options.recovery == RecoveryMode::Editor,
    };
    let program = parse_file_content_with_settings(&arena, FileId::zero(), source, settings);
    let trivia: Vec<&Trivia<'_>> = program.trivia.iter().collect();
    let comments = comments(&trivia);
    let spans: Vec<Span> = comments.iter().map(|c| c.span).collect();
    let mut cx = convert::Cx::new(source, program.errors, spans);
    let statements = convert::convert_program(&mut cx, program);
    let len = cx.source_len();
    let diagnostics = cx.into_diagnostics();
    ParsedFile::new(
        SyntaxTree::new(statements, comments, len),
        diagnostics,
        info.clone(),
    )
}

impl ParserBackend for MagoBackend {
    fn info(&self) -> BackendInfo {
        BackendInfo {
            name: "mago-syntax".to_string(),
            version: MAGO_VERSION.to_string(),
        }
    }

    fn parse(&self, _path: &str, source: &[u8], options: &ParseOptions) -> ParsedFile {
        let info = self.info();
        let len = u32::try_from(source.len()).unwrap_or(u32::MAX);
        if source.len() > options.max_bytes as usize {
            return ParsedFile::failed(
                len,
                DiagnosticCode::InputTooLarge,
                format!(
                    "the input is {} bytes; the limit is {}",
                    source.len(),
                    options.max_bytes
                ),
                info,
            );
        }
        on_parse_stack(len, &info, || parse_inner(source, options, &info))
    }
}

/// Stack for one parse. Mago recurses once per nesting level and a stack overflow aborts the
/// process, which no `catch_unwind` can stop; so every parse runs on its own thread with a stack
/// that holds Mago's own nesting limit with a wide margin, whatever thread the caller is on. The
/// memory is reserved, not committed, so ordinary files never touch most of it.
const PARSE_STACK_BYTES: usize = 64 * 1024 * 1024;

fn on_parse_stack(
    len: u32,
    info: &BackendInfo,
    run: impl FnOnce() -> ParsedFile + Send,
) -> ParsedFile {
    std::thread::scope(|scope| {
        let spawned = std::thread::Builder::new()
            .name("bleedthrough-parse".to_string())
            .stack_size(PARSE_STACK_BYTES)
            .spawn_scoped(scope, || guarded(len, info.clone(), run));
        match spawned.map(std::thread::ScopedJoinHandle::join) {
            Ok(Ok(parsed)) => parsed,
            Ok(Err(_)) | Err(_) => ParsedFile::failed(
                len,
                DiagnosticCode::BackendFailure,
                "the parser could not run on this input",
                info.clone(),
            ),
        }
    })
}

/// Runs a parse and turns a panic into a failed result: the port promises a backend never panics.
fn guarded(len: u32, info: BackendInfo, run: impl FnOnce() -> ParsedFile) -> ParsedFile {
    match catch_unwind(AssertUnwindSafe(run)) {
        Ok(parsed) => parsed,
        Err(_) => ParsedFile::failed(
            len,
            DiagnosticCode::BackendFailure,
            "the parser panicked on this input",
            info,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The safety net relies on unwinding; `panic = "abort"` would make `catch_unwind` useless.
    #[test]
    fn a_panic_becomes_a_failed_result() {
        let info = BackendInfo {
            name: "x".into(),
            version: "0".into(),
        };
        let parsed = super::guarded(7, info, || panic!("forced"));
        assert_eq!(parsed.quality(), bleedthrough_syntax::ParseQuality::Failed);
        assert_eq!(parsed.diagnostics()[0].code, DiagnosticCode::BackendFailure);
        assert_eq!(parsed.tree().source_len(), 7);
    }

    #[test]
    fn a_caught_panic_does_not_abort() {
        let caught = std::panic::catch_unwind(|| {
            if std::hint::black_box(true) {
                panic!("forced");
            }
        });
        assert!(caught.is_err());
    }
}
