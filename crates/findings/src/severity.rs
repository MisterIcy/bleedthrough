use crate::ModelError;

/// Term: Severity
///
/// How serious a finding is, from lowest to highest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum SeverityLevel {
    /// Informational.
    Info,
    /// Low.
    Low,
    /// Medium.
    Medium,
    /// High.
    High,
    /// Critical.
    Critical,
}

impl SeverityLevel {
    /// The stable wire name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

/// Term: Severity
///
/// A severity level with an optional score (integer tenths, 0 to 100) and an opaque vector
/// string, both supplied by the producer. Independent of confidence.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Severity {
    level: SeverityLevel,
    score_tenths: Option<u8>,
    vector: Option<String>,
}

impl Severity {
    /// A level only; score and vector are absent.
    pub fn new(level: SeverityLevel) -> Self {
        Self {
            level,
            score_tenths: None,
            vector: None,
        }
    }

    /// Adds a score in tenths (75 means 7.5); rejects values above 100.
    pub fn with_score_tenths(mut self, tenths: u16) -> Result<Self, ModelError> {
        let value = u8::try_from(tenths)
            .ok()
            .filter(|t| *t <= 100)
            .ok_or(ModelError::InvalidScore(tenths))?;
        self.score_tenths = Some(value);
        Ok(self)
    }

    /// Adds an opaque vector string.
    pub fn with_vector(mut self, vector: impl Into<String>) -> Self {
        self.vector = Some(vector.into());
        self
    }

    /// The level.
    pub fn level(&self) -> SeverityLevel {
        self.level
    }

    /// The score in tenths, when derived.
    pub fn score_tenths(&self) -> Option<u8> {
        self.score_tenths
    }

    /// The score rendered with one decimal place, such as `7.5`.
    pub fn score_text(&self) -> Option<String> {
        self.score_tenths.map(|t| format!("{}.{}", t / 10, t % 10))
    }

    /// The vector string, when derived.
    pub fn vector(&self) -> Option<&str> {
        self.vector.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_only_is_valid() {
        let s = Severity::new(SeverityLevel::High);
        assert!(s.score_tenths().is_none() && s.vector().is_none());
    }

    #[test]
    fn score_renders_one_decimal_and_is_bounded() {
        let s = Severity::new(SeverityLevel::Low)
            .with_score_tenths(100)
            .unwrap();
        assert_eq!(s.score_text().unwrap(), "10.0");
        assert_eq!(
            Severity::new(SeverityLevel::Low)
                .with_score_tenths(75)
                .unwrap()
                .score_text()
                .unwrap(),
            "7.5"
        );
        assert!(
            Severity::new(SeverityLevel::Low)
                .with_score_tenths(101)
                .is_err()
        );
        assert!(
            Severity::new(SeverityLevel::Low)
                .with_score_tenths(300)
                .is_err()
        );
    }

    #[test]
    fn levels_order_by_seriousness() {
        assert!(
            SeverityLevel::Critical > SeverityLevel::High
                && SeverityLevel::Low > SeverityLevel::Info
        );
    }
}
