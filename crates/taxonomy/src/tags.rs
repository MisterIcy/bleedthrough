use std::fmt;
use std::str::FromStr;

use crate::{Registry, TaxonomyError};

/// Term: Weakness
///
/// A CWE weakness number that is present in the registry. Rendered as `CWE-<number>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cwe(u32);

impl Cwe {
    /// Accepts only numbers in the built-in registry.
    pub fn new(number: u32) -> Result<Self, TaxonomyError> {
        Self::new_in(Registry::builtin(), number)
    }

    /// Accepts only numbers in the given registry.
    pub fn new_in(registry: &Registry, number: u32) -> Result<Self, TaxonomyError> {
        registry
            .weakness(number)
            .map(|_| Self(number))
            .ok_or(TaxonomyError::UnknownCwe(number))
    }

    /// Parses `CWE-<number>` against the built-in registry.
    pub fn parse(text: &str) -> Result<Self, TaxonomyError> {
        Self::parse_in(Registry::builtin(), text)
    }

    /// Parses `CWE-<number>` against the given registry.
    pub fn parse_in(registry: &Registry, text: &str) -> Result<Self, TaxonomyError> {
        let digits = text
            .strip_prefix("CWE-")
            .filter(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
            .ok_or_else(|| TaxonomyError::InvalidCwe(text.to_string()))?;
        let number = digits
            .parse::<u32>()
            .map_err(|_| TaxonomyError::InvalidCwe(text.to_string()))?;
        Self::new_in(registry, number)
    }

    /// The weakness number.
    pub fn number(&self) -> u32 {
        self.0
    }
}

impl fmt::Display for Cwe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CWE-{}", self.0)
    }
}

impl FromStr for Cwe {
    type Err = TaxonomyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// Term: Category
///
/// An OWASP Top 10 category qualified by its edition, such as 2021 and `A03`, that is present in
/// the registry. Rendered as `OWASP:<edition>:<id>`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OwaspRef {
    edition: u16,
    id: String,
}

impl OwaspRef {
    /// Accepts only categories in the built-in registry.
    pub fn new(edition: u16, id: impl Into<String>) -> Result<Self, TaxonomyError> {
        Self::new_in(Registry::builtin(), edition, id)
    }

    /// Accepts only categories in the given registry.
    pub fn new_in(
        registry: &Registry,
        edition: u16,
        id: impl Into<String>,
    ) -> Result<Self, TaxonomyError> {
        let id = id.into();
        if !registry.has_edition(edition) {
            return Err(TaxonomyError::UnknownOwaspEdition(edition));
        }
        if registry.category(edition, &id).is_none() {
            return Err(TaxonomyError::UnknownOwaspCategory { edition, id });
        }
        Ok(Self { edition, id })
    }

    /// Parses `OWASP:<edition>:<id>` against the built-in registry.
    pub fn parse(text: &str) -> Result<Self, TaxonomyError> {
        Self::parse_in(Registry::builtin(), text)
    }

    /// Parses `OWASP:<edition>:<id>` against the given registry.
    pub fn parse_in(registry: &Registry, text: &str) -> Result<Self, TaxonomyError> {
        let bad = || TaxonomyError::InvalidOwasp(text.to_string());
        let rest = text.strip_prefix("OWASP:").ok_or_else(bad)?;
        let (edition, id) = rest.split_once(':').ok_or_else(bad)?;
        let edition = edition
            .parse::<u16>()
            .ok()
            .filter(|_| edition.bytes().all(|b| b.is_ascii_digit()))
            .ok_or_else(bad)?;
        Self::new_in(registry, edition, id)
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

impl fmt::Display for OwaspRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OWASP:{}:{}", self.edition, self.id)
    }
}

impl FromStr for OwaspRef {
    type Err = TaxonomyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// Term: Taxonomy
///
/// The mandatory classification of a rule or finding: at least one weakness and one
/// edition-qualified category. Both lists are sorted and deduplicated.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Taxonomy {
    cwe: Vec<Cwe>,
    owasp: Vec<OwaspRef>,
}

impl Taxonomy {
    /// Fails with the missing tag kind when either list is empty. The tags are registered by
    /// construction; whether the categories fit the weaknesses is checked by
    /// [`Taxonomy::checked`].
    pub fn new(mut cwe: Vec<Cwe>, mut owasp: Vec<OwaspRef>) -> Result<Self, TaxonomyError> {
        if cwe.is_empty() {
            return Err(TaxonomyError::MissingCwe);
        }
        if owasp.is_empty() {
            return Err(TaxonomyError::MissingOwasp);
        }
        cwe.sort();
        cwe.dedup();
        owasp.sort();
        owasp.dedup();
        Ok(Self { cwe, owasp })
    }

    /// Like [`Taxonomy::new`], and additionally requires every category to be in the mapping of at
    /// least one of the weaknesses in `registry`, unless it is listed in `extra` with a non-empty
    /// reason (a deliberate cross-mapping; such categories are added to the tags).
    pub fn checked(
        registry: &Registry,
        cwe: Vec<Cwe>,
        owasp: Vec<OwaspRef>,
        extra: &[(OwaspRef, &str)],
    ) -> Result<Self, TaxonomyError> {
        let mut all = owasp;
        for (category, reason) in extra {
            if reason.trim().is_empty() {
                return Err(TaxonomyError::EmptyReason(category.to_string()));
            }
            all.push(category.clone());
        }
        let taxonomy = Self::new(cwe, all)?;
        for category in &taxonomy.owasp {
            let explained = extra.iter().any(|(c, _)| c == category);
            let mapped = taxonomy.cwe.iter().any(|w| {
                registry.weakness(w.number()).is_some_and(|e| {
                    e.categories
                        .iter()
                        .any(|(ed, id)| *ed == category.edition && *id == category.id)
                })
            });
            if !mapped && !explained {
                return Err(TaxonomyError::UnmappedCategory {
                    category: category.to_string(),
                    weaknesses: taxonomy.cwe.iter().map(Cwe::to_string).collect(),
                });
            }
        }
        Ok(taxonomy)
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
    use crate::{CategoryEntry, WeaknessEntry};

    fn cwe(n: u32) -> Cwe {
        Cwe::new(n).unwrap()
    }

    fn owasp(id: &str) -> OwaspRef {
        OwaspRef::new(2021, id).unwrap()
    }

    #[test]
    fn missing_kind_is_named() {
        assert_eq!(
            Taxonomy::new(vec![], vec![owasp("A03")]).unwrap_err(),
            TaxonomyError::MissingCwe
        );
        assert_eq!(
            Taxonomy::new(vec![cwe(89)], vec![]).unwrap_err(),
            TaxonomyError::MissingOwasp
        );
    }

    #[test]
    fn unregistered_tags_are_rejected_naming_the_value() {
        assert_eq!(Cwe::new(79).unwrap_err(), TaxonomyError::UnknownCwe(79));
        assert_eq!(Cwe::new(0).unwrap_err(), TaxonomyError::UnknownCwe(0));
        assert_eq!(
            OwaspRef::new(2025, "A05").unwrap_err(),
            TaxonomyError::UnknownOwaspEdition(2025)
        );
        assert_eq!(
            OwaspRef::new(2021, "A11").unwrap_err(),
            TaxonomyError::UnknownOwaspCategory {
                edition: 2021,
                id: "A11".into()
            }
        );
    }

    /// spec: security-taxonomy/Tags have stable text forms
    #[test]
    fn text_forms_round_trip() {
        assert_eq!(cwe(89).to_string(), "CWE-89");
        assert_eq!(Cwe::parse("CWE-89").unwrap(), cwe(89));
        let o = OwaspRef::parse("OWASP:2021:A03").unwrap();
        assert_eq!(o.to_string(), "OWASP:2021:A03");
        assert!(Cwe::parse("CWE-").is_err() && Cwe::parse("cwe-89").is_err());
        assert!(Cwe::parse("CWE-+89").is_err());
        assert!(OwaspRef::parse("OWASP:2021").is_err() && OwaspRef::parse("A03").is_err());
        assert_eq!(
            Cwe::parse("CWE-79").unwrap_err(),
            TaxonomyError::UnknownCwe(79)
        );
    }

    #[test]
    fn two_editions_are_kept_and_distinguishable_in_a_registry_that_has_them() {
        let r = Registry::new(
            vec![WeaknessEntry {
                number: 89,
                name: "SQL",
                catalog: "x",
                categories: &[(2021, "A03"), (2025, "A05")],
            }],
            vec![
                CategoryEntry {
                    edition: 2021,
                    id: "A03",
                    name: "Injection",
                },
                CategoryEntry {
                    edition: 2025,
                    id: "A05",
                    name: "Injection",
                },
            ],
        );
        let t = Taxonomy::checked(
            &r,
            vec![Cwe::new_in(&r, 89).unwrap(), Cwe::new_in(&r, 89).unwrap()],
            vec![
                OwaspRef::new_in(&r, 2025, "A05").unwrap(),
                OwaspRef::new_in(&r, 2021, "A03").unwrap(),
            ],
            &[],
        )
        .unwrap();
        assert_eq!(t.cwe().len(), 1);
        assert_eq!(
            t.owasp().iter().map(OwaspRef::edition).collect::<Vec<_>>(),
            [2021, 2025]
        );
    }

    /// spec: security-taxonomy/Weakness to category mapping is checked, not assumed
    #[test]
    fn mapping_is_checked() {
        let r = Registry::builtin();
        assert!(Taxonomy::checked(r, vec![cwe(89)], vec![owasp("A03")], &[]).is_ok());
        assert!(Taxonomy::checked(r, vec![cwe(601)], vec![owasp("A01")], &[]).is_ok());
        let err = Taxonomy::checked(r, vec![cwe(89)], vec![owasp("A02")], &[]).unwrap_err();
        assert_eq!(
            err,
            TaxonomyError::UnmappedCategory {
                category: "OWASP:2021:A02".into(),
                weaknesses: vec!["CWE-89".into()]
            }
        );
    }

    #[test]
    fn a_stated_reason_allows_a_cross_mapping_and_an_empty_one_does_not() {
        let r = Registry::builtin();
        let t = Taxonomy::checked(
            r,
            vec![cwe(89)],
            vec![owasp("A03")],
            &[(owasp("A02"), "credentials leak through the query")],
        )
        .unwrap();
        assert_eq!(t.owasp().len(), 2);
        assert_eq!(
            Taxonomy::checked(r, vec![cwe(89)], vec![owasp("A03")], &[(owasp("A02"), " ")])
                .unwrap_err(),
            TaxonomyError::EmptyReason("OWASP:2021:A02".into())
        );
    }

    #[test]
    fn owasp_category_id_is_exposed() {
        assert_eq!(owasp("A03").id(), "A03");
        assert_eq!(owasp("A01").id(), "A01");
    }
}
