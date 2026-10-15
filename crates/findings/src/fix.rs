use crate::{ModelError, RelPath};

/// Term: Fix
///
/// Whether a fix can be applied without a human decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Applicability {
    /// Safe to apply mechanically.
    Mechanical,
    /// Needs a human to review it.
    NeedsReview,
}

impl Applicability {
    /// The stable wire name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mechanical => "mechanical",
            Self::NeedsReview => "needs_review",
        }
    }
}

/// Term: Fix
///
/// One replacement of a non-empty byte range, with the original text expected there.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Edit {
    path: RelPath,
    start: usize,
    end: usize,
    expected: String,
    replacement: String,
}

impl Edit {
    /// Rejects an empty or reversed range and expected text whose length differs from it.
    pub fn new(
        path: RelPath,
        start: usize,
        end: usize,
        expected: impl Into<String>,
        replacement: impl Into<String>,
    ) -> Result<Self, ModelError> {
        let expected = expected.into();
        if end <= start {
            return Err(ModelError::InvalidEdit(format!(
                "range {start}..{end} is empty"
            )));
        }
        if expected.len() != end - start {
            return Err(ModelError::InvalidEdit(format!(
                "expected text has {} bytes for range {start}..{end}",
                expected.len()
            )));
        }
        Ok(Self {
            path,
            start,
            end,
            expected,
            replacement: replacement.into(),
        })
    }

    /// The file edited.
    pub fn path(&self) -> &RelPath {
        &self.path
    }

    /// Byte offset where the replaced range starts.
    pub fn start(&self) -> usize {
        self.start
    }

    /// Byte offset where the replaced range ends.
    pub fn end(&self) -> usize {
        self.end
    }

    /// The original text expected in the range.
    pub fn expected(&self) -> &str {
        &self.expected
    }

    /// The replacement text.
    pub fn replacement(&self) -> &str {
        &self.replacement
    }
}

/// Term: Fix
///
/// A described, non-overlapping set of edits that resolves a finding. Edits are ordered by
/// path then position.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fix {
    description: String,
    applicability: Applicability,
    edits: Vec<Edit>,
}

impl Fix {
    /// Needs at least one edit; edits in one file must not overlap (touching is allowed).
    pub fn new(
        description: impl Into<String>,
        applicability: Applicability,
        mut edits: Vec<Edit>,
    ) -> Result<Self, ModelError> {
        if edits.is_empty() {
            return Err(ModelError::EmptyFix);
        }
        edits.sort_by(|a, b| (&a.path, a.start, a.end).cmp(&(&b.path, b.start, b.end)));
        if let Some(pair) = edits
            .windows(2)
            .find(|w| w[0].path == w[1].path && w[1].start < w[0].end)
        {
            return Err(ModelError::OverlappingEdits(
                pair[0].path.as_str().to_string(),
            ));
        }
        Ok(Self {
            description: description.into(),
            applicability,
            edits,
        })
    }

    /// What the fix does.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Whether review is needed.
    pub fn applicability(&self) -> Applicability {
        self.applicability
    }

    /// The edits in canonical order.
    pub fn edits(&self) -> &[Edit] {
        &self.edits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(path: &str, start: usize, end: usize) -> Result<Edit, ModelError> {
        Edit::new(
            RelPath::new(path).unwrap(),
            start,
            end,
            "x".repeat(end - start),
            "y",
        )
    }

    #[test]
    fn overlapping_edits_fail() {
        let edits = vec![edit("a.php", 0, 5).unwrap(), edit("a.php", 4, 8).unwrap()];
        assert!(matches!(
            Fix::new("d", Applicability::Mechanical, edits),
            Err(ModelError::OverlappingEdits(_))
        ));
    }

    #[test]
    fn touching_edits_and_other_files_are_fine() {
        let edits = vec![
            edit("b.php", 0, 5).unwrap(),
            edit("a.php", 5, 8).unwrap(),
            edit("a.php", 0, 5).unwrap(),
        ];
        let fix = Fix::new("d", Applicability::NeedsReview, edits).unwrap();
        assert_eq!(fix.edits()[0].path().as_str(), "a.php");
        assert_eq!(fix.edits()[0].start(), 0);
    }

    #[test]
    fn empty_range_wrong_length_and_no_edits_fail() {
        let p = || RelPath::new("a.php").unwrap();
        assert!(Edit::new(p(), 3, 3, "", "z").is_err());
        assert!(Edit::new(p(), 3, 2, "", "z").is_err());
        assert!(Edit::new(p(), 0, 3, "ab", "z").is_err());
        assert_eq!(
            Fix::new("d", Applicability::Mechanical, vec![]).unwrap_err(),
            ModelError::EmptyFix
        );
    }
}
