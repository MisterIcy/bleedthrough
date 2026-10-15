//! Maps Mago parse errors to project diagnostics. No Mago error type or message escapes.

use bleedthrough_syntax::{Diagnostic, DiagnosticCode, Span};
use mago_span::HasSpan;
use mago_syntax::error::{ParseError, SyntaxError};

/// Converts a Mago span to ours; offsets beyond `u32` cannot occur (inputs are size-limited).
pub(crate) fn span(span: mago_span::Span) -> Span {
    Span::new(span.start.offset, span.end.offset)
}

fn syntax_code(error: &SyntaxError) -> DiagnosticCode {
    match error {
        SyntaxError::UnexpectedToken(..) | SyntaxError::UnrecognizedToken(..) => {
            DiagnosticCode::UnexpectedToken
        }
        SyntaxError::UnexpectedEndOfFile(..) => DiagnosticCode::UnexpectedEof,
        SyntaxError::RecursionLimitExceeded(..) => DiagnosticCode::RecursionLimit,
    }
}

/// The project diagnostic code for a Mago error.
pub(crate) fn code(error: &ParseError) -> DiagnosticCode {
    match error {
        ParseError::SyntaxError(inner) => syntax_code(inner),
        ParseError::UnexpectedEndOfFile(..) => DiagnosticCode::UnexpectedEof,
        ParseError::UnexpectedToken(..) => DiagnosticCode::UnexpectedToken,
        ParseError::UnclosedLiteralString(..) => DiagnosticCode::UnclosedString,
        ParseError::RecursionLimitExceeded(..) => DiagnosticCode::RecursionLimit,
    }
}

fn message(code: DiagnosticCode, source: &[u8], at: Span) -> String {
    let text = source
        .get(at.start as usize..at.end as usize)
        .unwrap_or(&[]);
    let text = String::from_utf8_lossy(text);
    match code {
        DiagnosticCode::UnexpectedToken => format!("unexpected token `{}`", text.trim()),
        DiagnosticCode::UnexpectedEof => "unexpected end of file".to_string(),
        DiagnosticCode::UnclosedString => "unclosed string".to_string(),
        DiagnosticCode::RecursionLimit => "nesting is too deep".to_string(),
        DiagnosticCode::BackendFailure => "the parser failed".to_string(),
        DiagnosticCode::InputTooLarge => "the input is too large".to_string(),
        DiagnosticCode::UnsupportedConstruct => "unsupported construct".to_string(),
        _ => "syntax error".to_string(),
    }
}

/// Maps one Mago error to a diagnostic.
pub(crate) fn diagnostic(error: &ParseError, source: &[u8]) -> Diagnostic {
    let code = code(error);
    let at = span(error.span());
    Diagnostic::new(code, message(code, source, at), at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mago_database::file::FileId;
    use mago_span::Position;
    use mago_syntax::cst::LiteralStringKind;
    use mago_syntax::error::Expected;
    use mago_syntax::token::TokenKind;

    fn at(start: u32, end: u32) -> mago_span::Span {
        mago_span::Span::new(FileId::zero(), Position::new(start), Position::new(end))
    }

    fn check(error: ParseError, expected: DiagnosticCode, range: (u32, u32)) {
        let found = diagnostic(&error, b"abcdefghij");
        assert_eq!(found.code, expected);
        assert_eq!((found.span.start, found.span.end), range);
    }

    #[test]
    fn syntax_error_variants() {
        let f = FileId::zero();
        let p = Position::new(3);
        check(
            ParseError::SyntaxError(SyntaxError::UnexpectedToken(f, b'x', p)),
            DiagnosticCode::UnexpectedToken,
            (3, 4),
        );
        check(
            ParseError::SyntaxError(SyntaxError::UnrecognizedToken(f, b'x', p)),
            DiagnosticCode::UnexpectedToken,
            (3, 4),
        );
        check(
            ParseError::SyntaxError(SyntaxError::UnexpectedEndOfFile(f, p)),
            DiagnosticCode::UnexpectedEof,
            (3, 4),
        );
        check(
            ParseError::SyntaxError(SyntaxError::RecursionLimitExceeded(f, p)),
            DiagnosticCode::RecursionLimit,
            (3, 4),
        );
    }

    #[test]
    fn parse_error_variants() {
        let expected = Expected::Exactly(TokenKind::Semicolon);
        check(
            ParseError::UnexpectedEndOfFile(expected, FileId::zero(), Position::new(10)),
            DiagnosticCode::UnexpectedEof,
            (10, 10),
        );
        check(
            ParseError::UnexpectedToken(expected, TokenKind::Comma, at(2, 3)),
            DiagnosticCode::UnexpectedToken,
            (2, 3),
        );
        check(
            ParseError::UnclosedLiteralString(LiteralStringKind::SingleQuoted, at(1, 9)),
            DiagnosticCode::UnclosedString,
            (1, 9),
        );
        check(
            ParseError::RecursionLimitExceeded(at(0, 5)),
            DiagnosticCode::RecursionLimit,
            (0, 5),
        );
    }

    #[test]
    fn unexpected_token_message_quotes_the_source_text() {
        let found = diagnostic(&ParseError::RecursionLimitExceeded(at(0, 1)), b"x");
        assert!(!found.message.is_empty());
        let token = ParseError::UnexpectedToken(Expected::OneOf(&[]), TokenKind::Comma, at(2, 4));
        assert_eq!(
            diagnostic(&token, b"abcdef").message,
            "unexpected token `cd`"
        );
    }
}
