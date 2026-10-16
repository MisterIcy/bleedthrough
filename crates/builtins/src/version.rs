//! PHP versions 8.0 to 8.5 and sets of them.

use std::fmt;

/// The highest supported PHP 8 minor version. Raising it adds a version without changing the
/// format: masks are `u16` bitsets and the text form is a range list.
pub const LAST_MINOR: u8 = 5;

/// Term: Version mask
///
/// One supported PHP version, always 8.x.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhpVersion(u8);

impl PhpVersion {
    /// The version 8.`minor`, or `None` when it is not a supported version.
    pub const fn new(minor: u8) -> Option<Self> {
        if minor <= LAST_MINOR {
            Some(Self(minor))
        } else {
            None
        }
    }

    /// Parses the spelling `8.x`.
    pub fn parse(text: &str) -> Option<Self> {
        let minor = text.strip_prefix("8.")?;
        if minor.is_empty() || minor.len() > 2 || !minor.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Self::new(minor.parse().ok()?)
    }

    /// The minor number: 4 for PHP 8.4.
    pub const fn minor(self) -> u8 {
        self.0
    }

    /// Every supported version, oldest first.
    pub fn all() -> impl Iterator<Item = PhpVersion> {
        (0..=LAST_MINOR).map(PhpVersion)
    }
}

impl fmt::Display for PhpVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "8.{}", self.0)
    }
}

/// Term: exempt
///
/// Why a version mask could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MaskError {
    /// A version outside the supported range.
    UnknownVersion(String),
    /// Not a version, a range or a list of them.
    Malformed(String),
    /// Valid, but not the single canonical spelling of the set (unsorted, overlapping or unmerged).
    NotCanonical(String),
}

impl fmt::Display for MaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownVersion(v) => write!(f, "unknown PHP version `{v}`"),
            Self::Malformed(v) => write!(f, "malformed version mask `{v}`"),
            Self::NotCanonical(v) => write!(f, "version mask `{v}` is not in canonical form"),
        }
    }
}

impl std::error::Error for MaskError {}

/// Term: Version mask
///
/// The set of PHP versions a record applies to, as a bitset (bit n is PHP 8.n). The text form is
/// a range list such as `8.0,8.2-8.5`; the set need not be contiguous.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct VersionMask(u16);

impl VersionMask {
    /// No version.
    pub const EMPTY: Self = Self(0);

    /// Every supported version.
    pub const ALL: Self = Self((1u16 << (LAST_MINOR + 1)) - 1);

    /// The set holding only `version`.
    pub const fn of(version: PhpVersion) -> Self {
        Self(1 << version.0)
    }

    /// The versions from `first` to `last` inclusive (empty when `last` precedes `first`).
    pub fn range(first: PhpVersion, last: PhpVersion) -> Self {
        (first.0..=last.0).fold(Self::EMPTY, |m, v| m.union(Self(1 << v)))
    }

    /// The raw bits, bit n for PHP 8.n.
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// Whether the set holds `version`.
    pub const fn contains(self, version: PhpVersion) -> bool {
        self.0 & (1 << version.0) != 0
    }

    /// Whether the set is empty.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The versions in either set.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// The versions in both sets.
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// The versions in this set and not in `other`.
    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// Whether the sets share a version.
    pub const fn overlaps(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// Whether every version of this set is in `other`.
    pub const fn is_subset_of(self, other: Self) -> bool {
        self.0 & !other.0 == 0
    }

    /// The versions in the set, oldest first.
    pub fn versions(self) -> impl Iterator<Item = PhpVersion> {
        PhpVersion::all().filter(move |v| self.contains(*v))
    }

    /// Reads the canonical range-list spelling. The empty set has the empty spelling.
    pub fn parse(text: &str) -> Result<Self, MaskError> {
        let mut mask = Self::EMPTY;
        if !text.is_empty() {
            for item in text.split(',') {
                let (first, last) = match item.split_once('-') {
                    Some((a, b)) => (a, b),
                    None => (item, item),
                };
                let version = |s: &str| {
                    PhpVersion::parse(s).ok_or_else(|| {
                        if s.starts_with("8.") {
                            MaskError::UnknownVersion(s.to_string())
                        } else {
                            MaskError::Malformed(text.to_string())
                        }
                    })
                };
                mask = mask.union(Self::range(version(first)?, version(last)?));
            }
        }
        if mask.to_string() == text {
            Ok(mask)
        } else {
            Err(MaskError::NotCanonical(text.to_string()))
        }
    }
}

impl fmt::Display for VersionMask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        let mut run: Option<(PhpVersion, PhpVersion)> = None;
        let mut flush = |f: &mut fmt::Formatter<'_>, run: (PhpVersion, PhpVersion)| {
            if !std::mem::take(&mut first) {
                f.write_str(",")?;
            }
            if run.0 == run.1 {
                write!(f, "{}", run.0)
            } else {
                write!(f, "{}-{}", run.0, run.1)
            }
        };
        for v in self.versions() {
            run = match run {
                Some((a, b)) if b.0 + 1 == v.0 => Some((a, v)),
                Some(done) => {
                    flush(f, done)?;
                    Some((v, v))
                }
                None => Some((v, v)),
            };
        }
        match run {
            Some(done) => flush(f, done),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(minor: u8) -> PhpVersion {
        PhpVersion::new(minor).unwrap()
    }

    #[test]
    fn versions_are_8_0_to_8_5() {
        assert_eq!(PhpVersion::all().count(), 6);
        assert!(PhpVersion::new(6).is_none());
        assert_eq!(PhpVersion::parse("8.3"), PhpVersion::new(3));
        assert_eq!(PhpVersion::parse("7.4"), None);
        assert_eq!(PhpVersion::parse("8."), None);
        assert_eq!(v(4).to_string(), "8.4");
    }

    #[test]
    fn arithmetic() {
        let a = VersionMask::range(v(0), v(2));
        let b = VersionMask::range(v(2), v(4));
        assert_eq!(a.union(b), VersionMask::range(v(0), v(4)));
        assert_eq!(a.intersection(b), VersionMask::of(v(2)));
        assert_eq!(a.difference(b), VersionMask::range(v(0), v(1)));
        assert!(a.overlaps(b));
        assert!(!VersionMask::of(v(0)).overlaps(VersionMask::of(v(5))));
        assert!(VersionMask::of(v(1)).is_subset_of(a));
        assert!(!b.is_subset_of(a));
        assert!(VersionMask::EMPTY.is_empty());
        assert_eq!(VersionMask::ALL.versions().count(), 6);
        assert_eq!(VersionMask::range(v(3), v(1)), VersionMask::EMPTY);
    }

    #[test]
    fn text_form_uses_ranges_and_allows_gaps() {
        let gap = VersionMask::of(v(0)).union(VersionMask::range(v(2), v(5)));
        assert_eq!(gap.to_string(), "8.0,8.2-8.5");
        assert!(!gap.contains(v(1)));
        assert_eq!(VersionMask::ALL.to_string(), "8.0-8.5");
        assert_eq!(VersionMask::of(v(3)).to_string(), "8.3");
        assert_eq!(VersionMask::EMPTY.to_string(), "");
        let two = VersionMask::of(v(0)).union(VersionMask::of(v(1)));
        assert_eq!(two.to_string(), "8.0-8.1");
        let split = VersionMask::of(v(0)).union(VersionMask::of(v(2)));
        assert_eq!(split.to_string(), "8.0,8.2");
    }

    #[test]
    fn every_mask_round_trips() {
        for bits in 0..=VersionMask::ALL.bits() {
            let mask = VersionMask(bits);
            assert_eq!(VersionMask::parse(&mask.to_string()), Ok(mask));
        }
    }

    #[test]
    fn rejections() {
        assert_eq!(
            VersionMask::parse("8.7"),
            Err(MaskError::UnknownVersion("8.7".into()))
        );
        assert!(matches!(
            VersionMask::parse("7.4"),
            Err(MaskError::Malformed(_))
        ));
        assert!(matches!(
            VersionMask::parse("x"),
            Err(MaskError::Malformed(_))
        ));
        for noncanonical in [
            "8.2,8.0",
            "8.0,8.1",
            "8.0-8.2,8.1",
            "8.1-8.1",
            "8.3-8.1",
            "8.0,,8.2",
        ] {
            assert!(
                VersionMask::parse(noncanonical).is_err(),
                "{noncanonical} must be rejected"
            );
        }
    }
}
