use std::fmt;

/// Term: Rule
///
/// A rule identifier: `BT` followed by four digits. Immutable once released; the area and name of
/// a rule live in its metadata, so renaming never changes the identifier. This is distinct from
/// the free-form rule name carried by a finding, which the findings crate validates on its own.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuleId(String);

impl RuleId {
    /// Accepts `BT` and four ASCII digits, nothing else.
    pub fn new(text: &str) -> Result<Self, String> {
        let digits = text.strip_prefix("BT");
        match digits {
            Some(d) if d.len() == 4 && d.bytes().all(|b| b.is_ascii_digit()) => {
                Ok(Self(text.to_string()))
            }
            _ => Err(format!(
                "invalid rule identifier {text:?}: expected `BT` followed by four digits"
            )),
        }
    }

    /// The identifier text, such as `BT0042`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_shape() {
        assert!(RuleId::new("BT0001").is_ok());
        for bad in [
            "", "BT001", "BT00001", "bt0001", "BT00a1", "XX0001", "BT 001", "BT+001",
        ] {
            assert!(RuleId::new(bad).is_err(), "{bad}");
        }
    }
}
