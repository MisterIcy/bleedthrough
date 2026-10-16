use crate::RelPath;

/// Term: Coverage
///
/// Why a file was not analysed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum SkipReason {
    /// Excluded by configuration.
    Excluded,
    /// Larger than the size limit.
    TooLarge,
    /// Could not be read.
    Unreadable,
    /// Not valid UTF-8.
    NotUtf8,
    /// The per-file time limit stopped the analysis.
    TimeLimit,
}

impl SkipReason {
    /// The stable wire name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Excluded => "excluded",
            Self::TooLarge => "too_large",
            Self::Unreadable => "unreadable",
            Self::NotUtf8 => "not_utf8",
            Self::TimeLimit => "time_limit",
        }
    }
}

/// Term: Coverage
///
/// A file that was not analysed, with the reason.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SkippedFile {
    path: RelPath,
    reason: SkipReason,
}

impl SkippedFile {
    /// Builds a skipped-file entry.
    pub fn new(path: RelPath, reason: SkipReason) -> Self {
        Self { path, reason }
    }

    /// The file.
    pub fn path(&self) -> &RelPath {
        &self.path
    }

    /// Why it was skipped.
    pub fn reason(&self) -> SkipReason {
        self.reason
    }
}

/// Term: Coverage
///
/// Which analysis limit was hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum LimitKind {
    /// The per-file time limit.
    PerFileTime,
    /// The file size limit.
    FileSize,
    /// The call-depth limit.
    CallDepth,
    /// The whole-run time limit.
    TotalTime,
}

impl LimitKind {
    /// The stable wire name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PerFileTime => "per_file_time",
            Self::FileSize => "file_size",
            Self::CallDepth => "call_depth",
            Self::TotalTime => "total_time",
        }
    }
}

/// Term: Coverage
///
/// A limit that was hit, and the file it was hit on when it is per-file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LimitHit {
    kind: LimitKind,
    path: Option<RelPath>,
}

impl LimitHit {
    /// Builds a limit-hit entry.
    pub fn new(kind: LimitKind, path: Option<RelPath>) -> Self {
        Self { kind, path }
    }

    /// Which limit.
    pub fn kind(&self) -> LimitKind {
        self.kind
    }

    /// The file, for per-file limits.
    pub fn path(&self) -> Option<&RelPath> {
        self.path.as_ref()
    }
}

/// Term: Pack
///
/// An active pack and its version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackRef {
    name: String,
    version: String,
}

impl PackRef {
    /// Builds a pack reference.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
        }
    }

    /// The pack name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The pack version.
    pub fn version(&self) -> &str {
        &self.version
    }
}

/// Term: Coverage
///
/// Cache use during a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct CacheStats {
    /// Entries reused.
    pub hits: u64,
    /// Entries computed.
    pub misses: u64,
    /// Entries discarded as stale.
    pub invalidations: u64,
}

impl CacheStats {
    /// Builds the statistics.
    pub fn new(hits: u64, misses: u64, invalidations: u64) -> Self {
        Self {
            hits,
            misses,
            invalidations,
        }
    }
}

/// Term: Coverage
///
/// What a run did and did not analyse. Build it with [`Coverage::new`] and set the public
/// fields; [`crate::Report::new`] puts every list into canonical order, so the result does not
/// depend on the order a parallel run produced them in.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Coverage {
    /// Files parsed without errors.
    pub files_clean: u64,
    /// Files parsed with error recovery.
    pub files_recovered: u64,
    /// Files that failed to parse.
    pub files_failed: u64,
    /// Files not analysed, with reasons.
    pub skipped: Vec<SkippedFile>,
    /// Analysis limits that were hit.
    pub limits_hit: Vec<LimitHit>,
    /// Includes whose target could not be resolved.
    pub unresolved_includes: u64,
    /// Calls whose target could not be resolved.
    pub unresolved_calls: u64,
    /// Active packs.
    pub packs: Vec<PackRef>,
    /// The PHP version modelled, such as `8.3`.
    pub php_version: String,
    /// Cache use, when a cache was used.
    pub cache: Option<CacheStats>,
}

impl Coverage {
    /// An empty coverage for the given modelled PHP version; set the public fields from there.
    pub fn new(php_version: impl Into<String>) -> Self {
        Self {
            php_version: php_version.into(),
            ..Self::default()
        }
    }

    /// Whether anything was left unexamined: failed or recovered files, skips or limits.
    pub fn is_incomplete(&self) -> bool {
        self.files_failed > 0
            || self.files_recovered > 0
            || !self.skipped.is_empty()
            || !self.limits_hit.is_empty()
    }

    pub(crate) fn canonicalize(&mut self) {
        self.skipped.sort();
        self.skipped.dedup();
        self.limits_hit.sort();
        self.limits_hit.dedup();
        self.packs.sort();
        self.packs.dedup();
    }
}

/// Term: Coverage
///
/// Findings that exist but are not in the report, so a hidden finding is never invisible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Suppressed {
    /// Hidden for low confidence.
    pub below_threshold_low: u64,
    /// Hidden for medium confidence.
    pub below_threshold_medium: u64,
    /// Silenced by the baseline.
    pub baselined: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// spec: finding-model/Coverage states what was and was not analysed
    #[test]
    fn default_is_complete_and_failed_files_make_it_incomplete() {
        let mut c = Coverage::new("8.3");
        assert!(!c.is_incomplete());
        c.files_failed = 2;
        assert!(c.is_incomplete());
    }

    #[test]
    fn canonicalize_sorts_and_dedups() {
        let a = RelPath::new("a.php").unwrap();
        let b = RelPath::new("b.php").unwrap();
        let mut c = Coverage::new("8.3");
        c.skipped = vec![
            SkippedFile::new(b.clone(), SkipReason::TooLarge),
            SkippedFile::new(a.clone(), SkipReason::Excluded),
            SkippedFile::new(b, SkipReason::TooLarge),
        ];
        c.packs = vec![PackRef::new("symfony", "1"), PackRef::new("laravel", "2")];
        c.canonicalize();
        assert_eq!(c.skipped.len(), 2);
        assert_eq!(c.skipped[0].path(), &a);
        assert_eq!(c.packs[0].name(), "laravel");
    }
}
