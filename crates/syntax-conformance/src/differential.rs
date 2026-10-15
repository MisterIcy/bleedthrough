//! Differential comparison: every registered backend must produce the same dump for every fixture,
//! unless the difference is listed, with a reason, in a reviewed allowance file.

use std::collections::BTreeMap;

use bleedthrough_syntax::{ParseOptions, ParserBackend};
use toml::{Table, Value};

use crate::corpus::Corpus;
use crate::dump::dump_tree;
use crate::suite::{Failure, failure, first_difference};

/// Fixtures whose cross-backend differences are accepted, each with the reason.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Allowances {
    reasons: BTreeMap<String, String>,
}

impl Allowances {
    /// No allowances: any difference fails.
    pub fn none() -> Self {
        Self::default()
    }

    /// Parses the allowance file: `[[allowance]]` entries with `fixture` and a non-empty `reason`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let root: Table = text.parse().map_err(|e| format!("allowances: {e}"))?;
        let mut reasons = BTreeMap::new();
        if let Some(Value::Array(items)) = root.get("allowance") {
            for item in items {
                let table = item
                    .as_table()
                    .ok_or("allowances: each entry must be a table")?;
                let fixture = table
                    .get("fixture")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let reason = table
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if fixture.is_empty() {
                    return Err("allowances: an entry has no `fixture`".to_string());
                }
                if reason.trim().is_empty() {
                    return Err(format!("allowances: {fixture} has no reason"));
                }
                reasons.insert(fixture.to_string(), reason.to_string());
            }
        }
        Ok(Self { reasons })
    }

    /// Every listed fixture with its reason, in path order.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.reasons.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// The reason a fixture is allowed to differ, if it is.
    pub fn reason(&self, fixture: &str) -> Option<&str> {
        self.reasons.get(fixture).map(String::as_str)
    }
}

/// Compares the dumps of all `backends` for every fixture; the first backend is the reference.
pub fn compare(
    backends: &[(&str, &dyn ParserBackend)],
    corpus: &Corpus,
    allowances: &Allowances,
    options: &ParseOptions,
) -> Vec<Failure> {
    let mut out = Vec::new();
    let Some(((reference_name, reference), others)) = backends.split_first() else {
        return out;
    };
    for fixture in &corpus.fixtures {
        let Ok(bytes) = corpus.read(fixture) else {
            continue;
        };
        let expected = dump_tree(reference.parse(&fixture.path, &bytes, options).tree());
        for (name, backend) in others {
            let actual = dump_tree(backend.parse(&fixture.path, &bytes, options).tree());
            if actual != expected && allowances.reason(&fixture.path).is_none() {
                out.push(failure(
                    &fixture.path,
                    "differential",
                    format!(
                        "{name} differs from {reference_name}: {}",
                        first_difference(&expected, &actual)
                    ),
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowances_need_a_fixture_and_a_reason() {
        assert!(Allowances::parse("[[allowance]]\nfixture = \"a.php\"\n").is_err());
        assert!(Allowances::parse("[[allowance]]\nreason = \"x\"\n").is_err());
        let ok =
            Allowances::parse("[[allowance]]\nfixture = \"a.php\"\nreason = \"spans differ\"\n")
                .unwrap();
        assert_eq!(ok.reason("a.php"), Some("spans differ"));
        assert_eq!(ok.reason("b.php"), None);
    }

    #[test]
    fn an_empty_file_allows_nothing() {
        assert_eq!(Allowances::parse("").unwrap(), Allowances::none());
    }
}
