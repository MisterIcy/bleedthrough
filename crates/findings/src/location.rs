use crate::ModelError;

/// Term: Finding
///
/// A path relative to the analysed project root, with forward slashes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelPath(String);

impl RelPath {
    /// Converts backslashes to forward slashes, then rejects absolute paths (including drive
    /// letters), empty paths, and empty, `.` or `..` segments.
    pub fn new(path: impl AsRef<str>) -> Result<Self, ModelError> {
        let raw = path.as_ref();
        let path = raw.replace('\\', "/");
        let drive = path.as_bytes().get(1) == Some(&b':')
            && path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic);
        let bad_segment = path.split('/').any(|s| matches!(s, "" | "." | ".."));
        if path.is_empty() || path.contains('\0') || drive || bad_segment {
            return Err(ModelError::InvalidPath(raw.to_string()));
        }
        Ok(Self(path))
    }

    /// The path text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Term: Finding
///
/// A 1-based line and a 1-based column counted in Unicode scalar values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position {
    line: u32,
    column: u32,
}

impl Position {
    /// Rejects a zero line or column.
    pub fn new(line: u32, column: u32) -> Result<Self, ModelError> {
        if line == 0 || column == 0 {
            return Err(ModelError::InvalidRange(format!(
                "position {line}:{column} is not 1-based"
            )));
        }
        Ok(Self { line, column })
    }

    /// 1-based line.
    pub fn line(&self) -> u32 {
        self.line
    }

    /// 1-based column in Unicode scalar values.
    pub fn column(&self) -> u32 {
        self.column
    }

    /// Position of a byte offset in `text`; lines break at `\n`.
    pub fn at_offset(text: &str, offset: usize) -> Result<Self, ModelError> {
        if !text.is_char_boundary(offset) {
            return Err(ModelError::InvalidOffset(offset));
        }
        let before = &text[..offset];
        let line = before.matches('\n').count() + 1;
        let line_start = before.rfind('\n').map_or(0, |i| i + 1);
        let column = before[line_start..].chars().count() + 1;
        Self::new(
            u32::try_from(line).map_err(|_| ModelError::InvalidOffset(offset))?,
            u32::try_from(column).map_err(|_| ModelError::InvalidOffset(offset))?,
        )
    }
}

/// Term: Finding
///
/// Where a finding or trace step lies: a project-relative path, a start and end position, and
/// the byte offsets of the same range.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Location {
    path: RelPath,
    start: Position,
    end: Position,
    byte_start: usize,
    byte_end: usize,
}

impl Location {
    /// Builds a location; the end must not precede the start.
    pub fn new(
        path: RelPath,
        start: Position,
        end: Position,
        byte_start: usize,
        byte_end: usize,
    ) -> Result<Self, ModelError> {
        if end < start || byte_end < byte_start {
            return Err(ModelError::InvalidRange("end precedes start".into()));
        }
        Ok(Self {
            path,
            start,
            end,
            byte_start,
            byte_end,
        })
    }

    /// Derives positions from a byte range of the file's `text`.
    pub fn from_byte_range(
        path: RelPath,
        text: &str,
        byte_start: usize,
        byte_end: usize,
    ) -> Result<Self, ModelError> {
        let start = Position::at_offset(text, byte_start)?;
        let end = Position::at_offset(text, byte_end)?;
        Self::new(path, start, end, byte_start, byte_end)
    }

    /// Project-relative path.
    pub fn path(&self) -> &RelPath {
        &self.path
    }

    /// Start position.
    pub fn start(&self) -> Position {
        self.start
    }

    /// End position.
    pub fn end(&self) -> Position {
        self.end
    }

    /// Byte offset of the start.
    pub fn byte_start(&self) -> usize {
        self.byte_start
    }

    /// Byte offset of the end.
    pub fn byte_end(&self) -> usize {
        self.byte_end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_absolute_and_parent_paths() {
        for bad in [
            "/etc/passwd",
            "C:/x.php",
            "c:\\x.php",
            "\\\\srv\\share",
            "../x.php",
            "a/../b.php",
            "a//b",
            "./a",
            "",
        ] {
            assert!(RelPath::new(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn backslashes_become_forward_slashes() {
        assert_eq!(
            RelPath::new("src\\Http\\Ctl.php").unwrap().as_str(),
            "src/Http/Ctl.php"
        );
    }

    #[test]
    fn column_counts_scalar_values_and_offsets_locate_the_text() {
        let text = "<?php\n$é = '日本'; echo $x;\n";
        let start = text.find("echo").unwrap();
        let loc = Location::from_byte_range(RelPath::new("a.php").unwrap(), text, start, start + 4)
            .unwrap();
        assert_eq!((loc.start().line(), loc.start().column()), (2, 12));
        assert_eq!(&text[loc.byte_start()..loc.byte_end()], "echo");
        assert!(
            start > loc.start().column() as usize + 5,
            "bytes exceed columns"
        );
    }

    #[test]
    fn offset_inside_a_character_fails() {
        assert!(Position::at_offset("é", 1).is_err());
    }

    #[test]
    fn zero_and_reversed_ranges_fail() {
        assert!(Position::new(0, 1).is_err());
        let p = RelPath::new("a.php").unwrap();
        let (a, b) = (Position::new(2, 1).unwrap(), Position::new(1, 1).unwrap());
        assert!(Location::new(p, a, b, 5, 6).is_err());
    }
}
