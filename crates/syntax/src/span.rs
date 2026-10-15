//! Byte-offset spans and the conversion of offsets to line and column.

/// Term: exempt
///
/// A half-open range of byte offsets (`start..end`) into a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[allow(
    clippy::exhaustive_structs,
    reason = "a plain pair of offsets; invalid ranges are representable on purpose so the invariant checker can report them"
)]
pub struct Span {
    /// First byte of the range.
    pub start: u32,
    /// One past the last byte of the range.
    pub end: u32,
}

impl Span {
    /// Builds a span; no validation, see [`crate::check_invariants`].
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    /// Number of bytes covered (zero when `end` precedes `start`).
    pub const fn len(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    /// Whether the span covers no bytes.
    pub const fn is_empty(&self) -> bool {
        self.end <= self.start
    }

    /// Whether `other` lies entirely within this span.
    pub const fn contains(&self, other: Span) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    /// The smallest span covering both.
    pub fn join(self, other: Span) -> Span {
        Span::new(self.start.min(other.start), self.end.max(other.end))
    }
}

/// Term: exempt
///
/// A 1-based line and a 1-based column counted in Unicode scalar values. Each byte of invalid
/// UTF-8 counts as one column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(
    clippy::exhaustive_structs,
    reason = "a line and column pair; the meaning of each part is fixed by definition"
)]
pub struct LineColumn {
    /// 1-based line; lines break at `\n`.
    pub line: u32,
    /// 1-based column in scalar values.
    pub column: u32,
}

/// Term: exempt
///
/// Maps byte offsets of one source file to [`LineColumn`].
#[derive(Debug, Clone)]
pub struct LineIndex<'a> {
    source: &'a [u8],
    line_starts: Vec<u32>,
}

impl<'a> LineIndex<'a> {
    /// Indexes the line starts of `source`. Offsets beyond `u32::MAX` are not indexed.
    pub fn new(source: &'a [u8]) -> Self {
        let mut line_starts = vec![0];
        let breaks = source.iter().enumerate().filter(|(_, b)| **b == b'\n');
        for (index, _) in breaks {
            if let Ok(next) = u32::try_from(index + 1) {
                line_starts.push(next);
            }
        }
        Self {
            source,
            line_starts,
        }
    }

    /// Number of lines (a final line without a newline counts; an empty file has one line).
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// The line and column of a byte offset; offsets past the end clamp to the end.
    pub fn line_column(&self, offset: u32) -> LineColumn {
        let offset = offset.min(u32::try_from(self.source.len()).unwrap_or(u32::MAX));
        let line = self.line_starts.partition_point(|start| *start <= offset);
        let start = self.line_starts[line.saturating_sub(1)];
        let text = &self.source[start as usize..offset as usize];
        LineColumn {
            line: u32::try_from(line).unwrap_or(u32::MAX),
            column: scalar_count(text).saturating_add(1),
        }
    }
}

/// Counts scalar values, with each invalid byte counting as one.
fn scalar_count(bytes: &[u8]) -> u32 {
    let mut count: usize = 0;
    let mut rest = bytes;
    while !rest.is_empty() {
        match std::str::from_utf8(rest) {
            Ok(text) => {
                count += text.chars().count();
                break;
            }
            Err(error) => {
                let (valid, invalid) = rest.split_at(error.valid_up_to());
                count += std::str::from_utf8(valid).map_or(0, |t| t.chars().count());
                let bad = error.error_len().unwrap_or(invalid.len());
                count += bad;
                rest = &invalid[bad..];
            }
        }
    }
    u32::try_from(count).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(source: &[u8], offset: u32) -> (u32, u32) {
        let p = LineIndex::new(source).line_column(offset);
        (p.line, p.column)
    }

    #[test]
    fn ascii_positions() {
        let source = b"ab\ncd\n";
        assert_eq!(at(source, 0), (1, 1));
        assert_eq!(at(source, 1), (1, 2));
        assert_eq!(at(source, 3), (2, 1));
        assert_eq!(at(source, 4), (2, 2));
    }

    #[test]
    fn multibyte_columns_count_scalars() {
        let source = "é€x".as_bytes();
        assert_eq!(at(source, 0), (1, 1));
        assert_eq!(at(source, 2), (1, 2));
        assert_eq!(at(source, 5), (1, 3));
    }

    #[test]
    fn crlf_breaks_at_the_line_feed() {
        let source = b"a\r\nb";
        assert_eq!(at(source, 1), (1, 2));
        assert_eq!(at(source, 2), (1, 3));
        assert_eq!(at(source, 3), (2, 1));
    }

    #[test]
    fn final_line_without_newline() {
        let source = b"ab\ncd";
        let index = LineIndex::new(source);
        assert_eq!(index.line_count(), 2);
        assert_eq!(at(source, 5), (2, 3));
        assert_eq!(at(b"", 0), (1, 1));
    }

    #[test]
    fn each_invalid_byte_is_one_column() {
        let source = b"a\xFF\xFEb\xE2\x82";
        assert_eq!(at(source, 3), (1, 4));
        assert_eq!(at(source, 4), (1, 5));
        assert_eq!(at(source, 6), (1, 7));
    }

    #[test]
    fn offsets_past_the_end_clamp() {
        assert_eq!(at(b"ab", 99), (1, 3));
    }

    #[test]
    fn span_helpers() {
        let span = Span::new(2, 5);
        assert_eq!(span.len(), 3);
        assert!(span.contains(Span::new(3, 4)));
        assert!(!span.contains(Span::new(1, 4)));
        assert_eq!(span.join(Span::new(0, 3)), Span::new(0, 5));
        assert!(Span::new(5, 2).is_empty());
    }
}
