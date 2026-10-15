use crate::ModelError;

/// Term: Finding
///
/// A CWE weakness number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cwe(u32);

impl Cwe {
    /// Rejects zero.
    pub fn new(number: u32) -> Result<Self, ModelError> {
        if number == 0 {
            return Err(ModelError::InvalidCwe);
        }
        Ok(Self(number))
    }

    /// The weakness number.
    pub fn number(&self) -> u32 {
        self.0
    }
}

/// Term: Finding
///
/// An OWASP Top 10 category qualified by its edition, such as 2021 and `A03`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OwaspRef {
    edition: u16,
    id: String,
}

impl OwaspRef {
    /// Accepts ids of the form `A` followed by two digits.
    pub fn new(edition: u16, id: impl Into<String>) -> Result<Self, ModelError> {
        let id = id.into();
        let b = id.as_bytes();
        if edition == 0
            || b.len() != 3
            || b[0] != b'A'
            || !b[1].is_ascii_digit()
            || !b[2].is_ascii_digit()
        {
            return Err(ModelError::InvalidOwasp(format!("{edition}:{id}")));
        }
        Ok(Self { edition, id })
    }

    /// The edition year.
    pub fn edition(&self) -> u16 {
        self.edition
    }

    /// The category id, such as `A03`.
    pub fn id(&self) -> &str {
        &self.id
    }
}

/// Term: Finding
///
/// The mandatory classification of a finding: at least one CWE and one edition-qualified OWASP
/// category. Both lists are sorted and deduplicated.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Taxonomy {
    cwe: Vec<Cwe>,
    owasp: Vec<OwaspRef>,
}

impl Taxonomy {
    /// Fails with the missing tag kind when either list is empty.
    pub fn new(mut cwe: Vec<Cwe>, mut owasp: Vec<OwaspRef>) -> Result<Self, ModelError> {
        if cwe.is_empty() {
            return Err(ModelError::MissingCwe);
        }
        if owasp.is_empty() {
            return Err(ModelError::MissingOwasp);
        }
        cwe.sort();
        cwe.dedup();
        owasp.sort();
        owasp.dedup();
        Ok(Self { cwe, owasp })
    }

    /// CWE identifiers, ascending.
    pub fn cwe(&self) -> &[Cwe] {
        &self.cwe
    }

    /// OWASP categories, ordered by edition then id.
    pub fn owasp(&self) -> &[OwaspRef] {
        &self.owasp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cwe(n: u32) -> Cwe {
        Cwe::new(n).unwrap()
    }

    #[test]
    fn missing_kind_is_named() {
        let o = OwaspRef::new(2021, "A03").unwrap();
        assert_eq!(
            Taxonomy::new(vec![], vec![o]).unwrap_err(),
            ModelError::MissingCwe
        );
        assert_eq!(
            Taxonomy::new(vec![cwe(89)], vec![]).unwrap_err(),
            ModelError::MissingOwasp
        );
    }

    #[test]
    fn two_editions_are_kept_and_distinguishable() {
        let t = Taxonomy::new(
            vec![cwe(89), cwe(20), cwe(89)],
            vec![
                OwaspRef::new(2025, "A05").unwrap(),
                OwaspRef::new(2021, "A03").unwrap(),
            ],
        )
        .unwrap();
        assert_eq!(t.cwe(), [cwe(20), cwe(89)]);
        assert_eq!(
            t.owasp().iter().map(OwaspRef::edition).collect::<Vec<_>>(),
            [2021, 2025]
        );
    }

    #[test]
    fn malformed_values_fail() {
        assert!(Cwe::new(0).is_err());
        assert!(OwaspRef::new(2021, "3").is_err());
        assert!(OwaspRef::new(2021, "A3x").is_err());
    }
}
