//! The generic node wrapper, names and literals shared by every node family.

use crate::span::Span;

/// Term: exempt
///
/// A syntax node: a span and a kind. Statements, expressions, types and members are all `Node`s.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    clippy::exhaustive_structs,
    reason = "a plain pair of span and kind; tree nodes are data built by adapters"
)]
pub struct Node<K> {
    /// Where the node lies in the file.
    pub span: Span,
    /// What the node is.
    pub kind: K,
}

impl<K> Node<K> {
    /// Builds a node.
    pub const fn new(span: Span, kind: K) -> Self {
        Self { span, kind }
    }
}

/// Term: exempt
///
/// How a name was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum NameKind {
    /// `Foo`
    Unqualified,
    /// `Foo\Bar`
    Qualified,
    /// `\Foo\Bar`
    FullyQualified,
    /// `namespace\Foo`
    Relative,
}

/// Term: exempt
///
/// A class, function or constant name as written, never resolved. The text has no leading `\`
/// and no `namespace\` prefix; [`NameKind`] records which was there.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    clippy::exhaustive_structs,
    reason = "a plain record of span, qualification and text"
)]
pub struct Name {
    /// Where the name lies.
    pub span: Span,
    /// How it was written.
    pub kind: NameKind,
    /// The text without the `\` or `namespace\` prefix.
    pub text: Box<str>,
}

impl Name {
    /// Classifies a name from its source spelling.
    pub fn from_written(span: Span, written: &str) -> Self {
        let relative = written
            .get(..10)
            .filter(|p| p.eq_ignore_ascii_case("namespace\\"));
        let (kind, text) = if let Some(rest) = written.strip_prefix('\\') {
            (NameKind::FullyQualified, rest)
        } else if relative.is_some() {
            (NameKind::Relative, &written[10..])
        } else if written.contains('\\') {
            (NameKind::Qualified, written)
        } else {
            (NameKind::Unqualified, written)
        };
        Self {
            span,
            kind,
            text: text.into(),
        }
    }
}

/// Term: exempt
///
/// The quote style of a string literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum QuoteKind {
    /// `'text'`
    Single,
    /// `"text"` without interpolation
    Double,
}

/// Term: exempt
///
/// A literal value, keeping its raw source text.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
#[allow(
    missing_docs,
    reason = "variants and fields are named after the PHP grammar"
)]
pub enum Literal {
    Int { raw: Box<str> },
    Float { raw: Box<str> },
    String { raw: Box<str>, quote: QuoteKind },
    Bool(bool),
    Null,
}

/// Term: exempt
///
/// The form of a string with parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StringKind {
    /// `"... $x ..."`
    DoubleQuoted,
    /// `` `cmd $x` ``
    ShellExec,
    /// `<<<LABEL`
    Heredoc,
    /// `<<<'LABEL'`
    Nowdoc,
}

/// Term: exempt
///
/// One part of an interpolated string, heredoc or nowdoc.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
#[allow(
    missing_docs,
    reason = "variants and fields are named after the PHP grammar"
)]
pub enum StringPartKind {
    Text { raw: Box<str> },
    Interpolation(Box<crate::expr::Expr>),
}

/// A string part with its span.
pub type StringPart = Node<StringPartKind>;

#[cfg(test)]
mod tests {
    use super::*;

    fn name(written: &str) -> (NameKind, String) {
        let n = Name::from_written(Span::new(0, 0), written);
        (n.kind, n.text.to_string())
    }

    #[test]
    fn names_keep_qualification() {
        assert_eq!(
            name("\\Foo\\Bar"),
            (NameKind::FullyQualified, "Foo\\Bar".into())
        );
        assert_eq!(name("Foo\\Bar"), (NameKind::Qualified, "Foo\\Bar".into()));
        assert_eq!(name("namespace\\Foo"), (NameKind::Relative, "Foo".into()));
        assert_eq!(
            name("NAMESPACE\\Foo\\Bar"),
            (NameKind::Relative, "Foo\\Bar".into())
        );
        assert_eq!(name("Foo"), (NameKind::Unqualified, "Foo".into()));
        assert_eq!(
            name("namespaces"),
            (NameKind::Unqualified, "namespaces".into())
        );
    }
}
