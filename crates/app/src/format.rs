use std::fmt;
use std::str::FromStr;

/// An output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Format {
    /// One JSON document.
    Json,
    /// A SARIF 2.1.0 log.
    Sarif,
    /// One JSON object per line.
    Ndjson,
    /// Tab-separated rows.
    Tsv,
    /// GitHub Actions workflow-command annotations.
    GithubActions,
    /// A unified diff of suggested fixes.
    Diff,
}

impl Format {
    /// The six format names, in documentation order. The `jsonl` alias is accepted but not listed.
    pub const NAMES: [&'static str; 6] =
        ["json", "sarif", "ndjson", "tsv", "github-actions", "diff"];

    /// Every format, in the order of [`Format::NAMES`].
    pub const ALL: [Self; 6] = [
        Self::Json,
        Self::Sarif,
        Self::Ndjson,
        Self::Tsv,
        Self::GithubActions,
        Self::Diff,
    ];

    /// The canonical name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Sarif => "sarif",
            Self::Ndjson => "ndjson",
            Self::Tsv => "tsv",
            Self::GithubActions => "github-actions",
            Self::Diff => "diff",
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A format name that is not provided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownFormat {
    requested: String,
}

impl UnknownFormat {
    /// The name that was requested.
    pub fn requested(&self) -> &str {
        &self.requested
    }
}

impl fmt::Display for UnknownFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown format `{}`; valid formats: {}",
            self.requested,
            Format::NAMES.join(", ")
        )
    }
}

impl std::error::Error for UnknownFormat {}

impl FromStr for Format {
    type Err = UnknownFormat;

    /// Parses a format name; `jsonl` is an alias of `ndjson`.
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "jsonl" => Ok(Self::Ndjson),
            other => Self::ALL
                .into_iter()
                .find(|f| f.as_str() == other)
                .ok_or_else(|| UnknownFormat {
                    requested: other.to_string(),
                }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_resolves_to_ndjson() {
        assert_eq!("jsonl".parse::<Format>().unwrap(), Format::Ndjson);
        assert_eq!("ndjson".parse::<Format>().unwrap(), Format::Ndjson);
    }

    #[test]
    fn every_name_round_trips() {
        for (name, format) in Format::NAMES.into_iter().zip(Format::ALL) {
            assert_eq!(name.parse::<Format>().unwrap(), format);
            assert_eq!(format.as_str(), name);
        }
    }

    #[test]
    fn unknown_name_lists_the_six_valid_ones() {
        let message = "yaml".parse::<Format>().unwrap_err().to_string();
        assert!(message.contains("`yaml`"));
        for name in Format::NAMES {
            assert!(message.contains(name), "{message}");
        }
    }
}
