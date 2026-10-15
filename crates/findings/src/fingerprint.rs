use sha2::{Digest, Sha256};

use crate::{ModelError, RuleId};

/// The visible scheme prefix; a change of scheme changes it.
pub const FINGERPRINT_PREFIX: &str = "bt1:";

const HEX_DIGITS: usize = 32;

/// Term: Fingerprint
///
/// Everything a fingerprint is derived from. Snippets must already be normalized by the
/// producer (whitespace and comments removed); line numbers and paths are deliberately absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FingerprintInput {
    rule: RuleId,
    symbol: String,
    snippets: Vec<String>,
    occurrence: u32,
}

impl FingerprintInput {
    /// Builds the input from the rule, enclosing symbol, key snippets and occurrence index.
    pub fn new(
        rule: RuleId,
        symbol: impl Into<String>,
        snippets: Vec<String>,
        occurrence: u32,
    ) -> Self {
        Self {
            rule,
            symbol: symbol.into(),
            snippets,
            occurrence,
        }
    }
}

/// Term: Fingerprint
///
/// A stable identity: `bt1:` followed by the first 128 bits of a SHA-256 over a canonical,
/// length-prefixed encoding of the input, in lowercase hex.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fingerprint(String);

impl Fingerprint {
    /// Computes the fingerprint of an input.
    pub fn compute(input: &FingerprintInput) -> Self {
        let mut hasher = Sha256::new();
        let mut field = |bytes: &[u8]| {
            hasher.update((bytes.len() as u64).to_be_bytes());
            hasher.update(bytes);
        };
        field(FINGERPRINT_PREFIX.as_bytes());
        field(input.rule.as_str().as_bytes());
        field(input.symbol.as_bytes());
        field(&(input.snippets.len() as u64).to_be_bytes());
        for snippet in &input.snippets {
            field(snippet.as_bytes());
        }
        field(&input.occurrence.to_be_bytes());
        let digest = hasher.finalize();
        let hex: String = digest
            .iter()
            .take(HEX_DIGITS / 2)
            .map(|b| format!("{b:02x}"))
            .collect();
        Self(format!("{FINGERPRINT_PREFIX}{hex}"))
    }

    /// Accepts a previously computed fingerprint, as read from a baseline.
    pub fn parse(text: &str) -> Result<Self, ModelError> {
        let ok = text.strip_prefix(FINGERPRINT_PREFIX).is_some_and(|h| {
            h.len() == HEX_DIGITS
                && h.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        });
        if ok {
            Ok(Self(text.to_string()))
        } else {
            Err(ModelError::InvalidFingerprint(text.to_string()))
        }
    }

    /// The fingerprint text, including the scheme prefix.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(symbol: &str, snippets: &[&str], occurrence: u32) -> FingerprintInput {
        let snippets = snippets.iter().map(|s| (*s).to_string()).collect();
        FingerprintInput::new(
            RuleId::new("php.sqli").unwrap(),
            symbol,
            snippets,
            occurrence,
        )
    }

    fn fp(i: &FingerprintInput) -> Fingerprint {
        Fingerprint::compute(i)
    }

    #[test]
    fn shape_and_parse_round_trip() {
        let f = fp(&input("A::b", &["$_GET['id']", "query($q)"], 0));
        assert!(f.as_str().starts_with("bt1:") && f.as_str().len() == 4 + 32);
        assert_eq!(Fingerprint::parse(f.as_str()).unwrap(), f);
        assert!(Fingerprint::parse("bt2:00000000000000000000000000000000").is_err());
        assert!(Fingerprint::parse("bt1:ABCDEF00000000000000000000000000").is_err());
    }

    #[test]
    fn independent_of_position_because_input_has_none() {
        // The input carries no line numbers or paths, so moving code cannot change it.
        let a = fp(&input("A::b", &["x"], 0));
        let b = fp(&input("A::b", &["x"], 0));
        assert_eq!(a, b);
    }

    #[test]
    fn changed_flow_or_occurrence_changes_it() {
        let base = fp(&input("A::b", &["$_GET['id']", "query($q)"], 0));
        assert_ne!(
            base,
            fp(&input("A::b", &["$_GET['id']", "exec($q)"], 0)),
            "sink changed"
        );
        assert_ne!(
            base,
            fp(&input("A::b", &["$_POST['id']", "query($q)"], 0)),
            "source changed"
        );
        assert_ne!(
            base,
            fp(&input("A::c", &["$_GET['id']", "query($q)"], 0)),
            "symbol changed"
        );
        assert_ne!(
            base,
            fp(&input("A::b", &["$_GET['id']", "query($q)"], 1)),
            "second occurrence"
        );
    }

    #[test]
    fn scheme_and_rule_are_part_of_the_hash() {
        let other_rule =
            FingerprintInput::new(RuleId::new("php.xss").unwrap(), "A::b", vec!["x".into()], 0);
        assert_ne!(fp(&input("A::b", &["x"], 0)), fp(&other_rule));
    }

    #[test]
    fn hostile_field_boundaries_cannot_collide() {
        assert_ne!(fp(&input("ab", &["c"], 0)), fp(&input("a", &["bc"], 0)));
        assert_ne!(
            fp(&input("s", &["ab", "c"], 0)),
            fp(&input("s", &["a", "bc"], 0))
        );
        assert_ne!(fp(&input("s", &["a", ""], 0)), fp(&input("s", &["a"], 0)));
        assert_ne!(fp(&input("s", &[], 0)), fp(&input("s", &[""], 0)));
        let tricky = "\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\u{1}x";
        assert_ne!(fp(&input("s", &[tricky], 0)), fp(&input("s", &["x"], 0)));
    }
}
