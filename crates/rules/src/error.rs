use std::fmt;

use crate::Origin;

/// Term: exempt
///
/// One problem found while validating a rule: which rule (when known), which field and what is
/// wrong. Validation collects every problem rather than stopping at the first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleError {
    rule: Option<String>,
    origin: Option<Origin>,
    field: String,
    message: String,
}

impl RuleError {
    /// An error about `field` (a dotted path such as `sinks[0].class`).
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            rule: None,
            origin: None,
            field: field.into(),
            message: message.into(),
        }
    }

    /// The same error attributed to a rule identifier.
    #[must_use]
    pub fn in_rule(mut self, rule: impl Into<String>) -> Self {
        self.rule = Some(rule.into());
        self
    }

    /// The same error attributed to the place a rule was defined.
    #[must_use]
    pub fn at(mut self, origin: Origin) -> Self {
        self.origin = Some(origin);
        self
    }

    /// Where the offending rule was defined, when known.
    pub fn origin(&self) -> Option<&Origin> {
        self.origin.as_ref()
    }

    /// The rule identifier, when known.
    pub fn rule(&self) -> Option<&str> {
        self.rule.as_deref()
    }

    /// The field path.
    pub fn field(&self) -> &str {
        &self.field
    }

    /// What is wrong.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for RuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.rule {
            Some(rule) => write!(f, "rule {rule}: {}: {}", self.field, self.message),
            None => write!(f, "{}: {}", self.field, self.message),
        }
    }
}

impl std::error::Error for RuleError {}

/// Prefixes the field of every error, for errors found inside a nested value.
pub(crate) fn prefixed(prefix: &str, errors: Vec<RuleError>) -> Vec<RuleError> {
    errors
        .into_iter()
        .map(|mut e| {
            e.field = format!("{prefix}.{}", e.field);
            e
        })
        .collect()
}
