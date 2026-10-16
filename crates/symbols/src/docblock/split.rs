//! Splitting a raw docblock into tags, keeping the file offset of every byte of every tag body.

use bleedthrough_syntax::Span;

/// One `@tag` with its body. Continuation lines are joined with a newline, so a multi-line type or
/// description stays one body.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct DocTag {
    /// The tag name without `@` and without a `psalm-` or `phpstan-` prefix.
    pub name: String,
    /// Whether the tag carried a `psalm-` or `phpstan-` prefix (it then wins over the plain tag).
    pub prefixed: bool,
    pub body: String,
    /// `offsets[i]` is the file offset of byte `i` of `body`.
    pub offsets: Vec<u32>,
    pub span: Span,
}

impl DocTag {
    fn new(raw_name: &str, start: u32) -> Self {
        let (name, prefixed) = ["psalm-", "phpstan-"]
            .iter()
            .find_map(|p| raw_name.strip_prefix(p))
            .map_or((raw_name, false), |rest| (rest, true));
        Self {
            name: name.to_ascii_lowercase(),
            prefixed,
            body: String::new(),
            offsets: Vec::new(),
            span: Span::new(start, start),
        }
    }

    fn push(&mut self, text: &str, offset: u32, separator: bool) {
        if separator && !self.body.is_empty() {
            self.body.push('\n');
            self.offsets.push(offset);
        }
        self.body.push_str(text);
        for i in 0..text.len() {
            self.offsets
                .push(offset.saturating_add(u32::try_from(i).unwrap_or(0)));
        }
    }
}

/// A line of the comment stripped of `/**`, `*/` and the leading `*`, with the file offset of its
/// first remaining byte.
struct Line<'a> {
    text: &'a str,
    offset: u32,
}

fn clean_lines(comment: &str, start: u32) -> Vec<Line<'_>> {
    let mut lines = Vec::new();
    let mut line_start = 0usize;
    for raw in comment.split('\n') {
        let mut text = raw.strip_suffix('\r').unwrap_or(raw);
        let mut begin = 0usize;
        let trimmed = text.trim_start();
        begin += text.len() - trimmed.len();
        text = trimmed;
        if let Some(rest) = text.strip_prefix("/**") {
            begin += 3;
            text = rest;
        } else if text.starts_with("*/") {
            text = "";
        } else if let Some(rest) = text.strip_prefix('*') {
            begin += 1;
            text = rest;
        }
        if let Some(rest) = text.strip_suffix("*/") {
            text = rest;
        }
        let lead = text.len() - text.trim_start().len();
        begin += lead;
        let text = text.trim();
        let offset = start.saturating_add(u32::try_from(line_start + begin).unwrap_or(0));
        lines.push(Line { text, offset });
        line_start += raw.len() + 1;
    }
    lines
}

/// The tags of a docblock whose text starts at file offset `start`.
pub(super) fn split_tags(comment: &str, start: u32) -> Vec<DocTag> {
    let mut tags: Vec<DocTag> = Vec::new();
    let mut open = false;
    for line in clean_lines(comment, start) {
        if line.text.is_empty() {
            open = false;
        } else if let Some(rest) = line.text.strip_prefix('@') {
            let name_len = rest
                .find(|c: char| c.is_whitespace() || c == '(')
                .unwrap_or(rest.len());
            let mut tag = DocTag::new(&rest[..name_len], line.offset);
            let body = rest[name_len..].trim_start();
            let shift = 1 + rest.len() - body.len();
            tag.push(body, line.offset + u32::try_from(shift).unwrap_or(0), false);
            tag.span = Span::new(
                line.offset,
                line.offset + u32::try_from(line.text.len()).unwrap_or(0),
            );
            tags.push(tag);
            open = true;
        } else if let (true, Some(tag)) = (open, tags.last_mut()) {
            tag.push(line.text, line.offset, true);
            tag.span = Span::new(
                tag.span.start,
                line.offset + u32::try_from(line.text.len()).unwrap_or(0),
            );
        }
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_split_and_continued() {
        let text = "/**\n * Summary line.\n *\n * @param int $a the a\n *   continued\n * @return string\n */";
        let tags = split_tags(text, 100);
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].name, "param");
        assert_eq!(tags[0].body, "int $a the a\ncontinued");
        assert_eq!(tags[1].name, "return");
        assert_eq!(tags[1].body, "string");
    }

    #[test]
    fn offsets_point_back_into_the_comment() {
        let text = "/** @var Foo $x */";
        let tags = split_tags(text, 10);
        assert_eq!(tags[0].body, "Foo $x");
        let at = tags[0].offsets[0] as usize - 10;
        assert_eq!(&text[at..at + 3], "Foo");
        let at = tags[0].offsets[4] as usize - 10;
        assert_eq!(&text[at..at + 2], "$x");
    }

    #[test]
    fn prefixes_are_recognised_and_stripped() {
        let tags = split_tags("/** @psalm-return list<int> */", 0);
        assert_eq!(tags[0].name, "return");
        assert!(tags[0].prefixed);
        assert!(!split_tags("/** @return int */", 0)[0].prefixed);
    }

    #[test]
    fn a_blank_line_ends_a_tag() {
        let tags = split_tags("/**\n * @return int\n *\n * not part of the tag\n */", 0);
        assert_eq!(tags[0].body, "int");
    }

    #[test]
    fn single_line_docblocks_and_crlf_work() {
        let tags = split_tags("/**\r\n * @param string $s\r\n */", 0);
        assert_eq!(tags[0].body, "string $s");
    }
}
