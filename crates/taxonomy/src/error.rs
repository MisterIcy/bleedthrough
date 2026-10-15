use std::fmt;

/// Term: exempt
///
/// Why a taxonomy value could not be constructed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TaxonomyError {
    /// A tag list had no CWE identifier.
    MissingCwe,
    /// A tag list had no OWASP Top 10 category.
    MissingOwasp,
    /// A CWE text was not of the form `CWE-<number>`.
    InvalidCwe(String),
    /// An OWASP category text was not of the form `OWASP:<edition>:A<nn>`.
    InvalidOwasp(String),
    /// A CWE number is not in the registry.
    UnknownCwe(u32),
    /// An OWASP edition is not in the registry.
    UnknownOwaspEdition(u16),
    /// An OWASP category id is not registered for its edition.
    UnknownOwaspCategory {
        /// The edition year.
        edition: u16,
        /// The category id.
        id: String,
    },
    /// A category is not mapped from any of the weaknesses and no reason was given.
    UnmappedCategory {
        /// The category, in its text form.
        category: String,
        /// The weaknesses it was paired with, in their text form.
        weaknesses: Vec<String>,
    },
    /// A deliberate cross-mapping carried an empty reason.
    EmptyReason(String),
    /// A severity score was above 10.0.
    InvalidScore(u16),
}

impl fmt::Display for TaxonomyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCwe => f.write_str("no CWE identifier (taxonomy kind: cwe)"),
            Self::MissingOwasp => f.write_str("no OWASP category (taxonomy kind: owasp)"),
            Self::InvalidCwe(s) => write!(f, "invalid CWE {s:?}: expected `CWE-<number>`"),
            Self::InvalidOwasp(s) => write!(f, "invalid OWASP category {s:?}"),
            Self::UnknownCwe(n) => write!(f, "CWE-{n} is not in the weakness registry"),
            Self::UnknownOwaspEdition(e) => write!(f, "OWASP edition {e} is not in the registry"),
            Self::UnknownOwaspCategory { edition, id } => {
                write!(
                    f,
                    "OWASP category {id} is not registered for edition {edition}"
                )
            }
            Self::UnmappedCategory {
                category,
                weaknesses,
            } => write!(
                f,
                "{category} is not mapped from {} and no reason was given",
                weaknesses.join(", ")
            ),
            Self::EmptyReason(c) => write!(f, "the reason for adding {c} is empty"),
            Self::InvalidScore(t) => write!(f, "severity score {t} tenths is above 10.0"),
        }
    }
}

impl std::error::Error for TaxonomyError {}
