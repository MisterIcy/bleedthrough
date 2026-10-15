use crate::ModelError;

/// Term: Rule
///
/// The identifier of the rule that produced a finding.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuleId(String);

impl RuleId {
    /// Accepts a non-empty identifier without whitespace or control characters.
    pub fn new(id: impl Into<String>) -> Result<Self, ModelError> {
        let id = id.into();
        if id.is_empty() || id.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(ModelError::InvalidRuleId(id));
        }
        Ok(Self(id))
    }

    /// The identifier text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Term: Finding
///
/// The concise human-readable message of a finding.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Message(String);

impl Message {
    /// Accepts any text with at least one non-whitespace character.
    pub fn new(text: impl Into<String>) -> Result<Self, ModelError> {
        let text = text.into();
        if text.trim().is_empty() {
            return Err(ModelError::EmptyMessage);
        }
        Ok(Self(text))
    }

    /// The message text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_id_rejects_empty_and_whitespace() {
        assert!(RuleId::new("").is_err());
        assert!(RuleId::new("a b").is_err());
        assert_eq!(RuleId::new("php.sqli").unwrap().as_str(), "php.sqli");
    }

    #[test]
    fn message_rejects_blank() {
        assert_eq!(Message::new("  \n").unwrap_err(), ModelError::EmptyMessage);
        assert!(Message::new("tainted\tvalue").is_ok());
    }
}
