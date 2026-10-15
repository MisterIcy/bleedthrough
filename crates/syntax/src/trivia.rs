//! Comments and docblock lookup.

use crate::span::Span;

/// Term: exempt
///
/// The kind of a comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CommentKind {
    /// `// text`
    Line,
    /// `# text`
    Hash,
    /// `/* text */`
    Block,
    /// `/** text */`
    Doc,
}

/// Term: exempt
///
/// A comment, kept raw and uninterpreted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    clippy::exhaustive_structs,
    reason = "a plain record of kind, span and text"
)]
pub struct Comment {
    /// Line, hash, block or docblock.
    pub kind: CommentKind,
    /// Where the comment lies, delimiters included.
    pub span: Span,
    /// The raw text, delimiters included.
    pub text: Box<str>,
}

/// The docblock that immediately precedes a declaration starting at `declaration_start`:
/// the last docblock ending at or before it with only whitespace in between. `comments` must be in
/// source order.
pub fn docblock_before<'a>(
    comments: &'a [Comment],
    source: &[u8],
    declaration_start: u32,
) -> Option<&'a Comment> {
    let candidate = comments
        .iter()
        .rev()
        .find(|c| c.span.end <= declaration_start && c.kind != CommentKind::Line)?;
    let between = source.get(candidate.span.end as usize..declaration_start as usize)?;
    let adjacent = between.iter().all(u8::is_ascii_whitespace);
    (adjacent && candidate.kind == CommentKind::Doc).then_some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(start: u32, end: u32) -> Comment {
        Comment {
            kind: CommentKind::Doc,
            span: Span::new(start, end),
            text: "/** d */".into(),
        }
    }

    #[test]
    fn adjacent_docblock_is_found() {
        let source = b"/** d */\n  function f() {}";
        let comments = [doc(0, 8)];
        let found = docblock_before(&comments, source, 11);
        assert_eq!(found.map(|c| c.span), Some(Span::new(0, 8)));
    }

    #[test]
    fn detached_docblock_is_not_found() {
        let source = b"/** d */\n$x = 1;\nfunction f() {}";
        assert!(docblock_before(&[doc(0, 8)], source, 17).is_none());
    }

    #[test]
    fn plain_block_comment_is_not_a_docblock() {
        let mut comment = doc(0, 8);
        comment.kind = CommentKind::Block;
        assert!(docblock_before(&[comment], b"/* d */\nf()", 9).is_none());
    }
}
