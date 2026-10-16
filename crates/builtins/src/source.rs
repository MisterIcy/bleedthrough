//! The port through which consumers obtain the built-in data.

use std::fmt;

use crate::data::BuiltinData;

/// Term: exempt
///
/// Why a source could not supply the data.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain message")]
pub struct SourceError {
    /// What went wrong, for a human.
    pub message: String,
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SourceError {}

/// Term: Built-in data
///
/// The port yielding the parsed built-in data. An embedded file, a fixture and an in-memory value
/// all implement it, so consumers depend only on this trait and the model.
pub trait BuiltinDataSource: Send + Sync {
    /// The data. Two calls on one source return equal data.
    fn load(&self) -> Result<BuiltinData, SourceError>;
}

/// Term: Built-in data
///
/// A source holding data that was built or read elsewhere; for tests and for callers that read
/// the file themselves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InMemorySource {
    data: BuiltinData,
}

impl InMemorySource {
    /// A source that returns `data`.
    pub fn new(data: BuiltinData) -> Self {
        Self { data }
    }

    /// A source over data text in the canonical format, read now.
    pub fn from_text(text: &str) -> Result<Self, SourceError> {
        crate::format::read(text)
            .map(Self::new)
            .map_err(|e| SourceError {
                message: e.to_string(),
            })
    }
}

impl BuiltinDataSource for InMemorySource {
    fn load(&self) -> Result<BuiltinData, SourceError> {
        Ok(self.data.clone())
    }
}
