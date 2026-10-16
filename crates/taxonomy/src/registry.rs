//! The curated registry of supported weaknesses and OWASP categories.
//!
//! Sources, recorded while authoring (see `docs/taxonomy.md`): weakness names from the MITRE CWE
//! catalog definition pages (<https://cwe.mitre.org/data/definitions/N.html>), checked against
//! catalog version 4.20; category names and the weakness-to-category mapping from the OWASP Top 10
//! 2021 publication (<https://owasp.org/Top10/2021/>, "List of Mapped CWEs" of each category).

use std::sync::OnceLock;

/// The CWE catalog version the built-in names were checked against.
pub const CATALOG: &str = "4.20";

/// Term: Registry
///
/// One supported weakness: its CWE number, official name, the catalog version the name was
/// checked against, and the OWASP categories (edition, id) it is mapped to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs)]
pub struct WeaknessEntry {
    /// The CWE number.
    pub number: u32,
    /// The official name.
    pub name: &'static str,
    /// The CWE catalog version the name was checked against.
    pub catalog: &'static str,
    /// The OWASP categories the weakness is conventionally mapped to, as (edition, id).
    pub categories: &'static [(u16, &'static str)],
}

/// Term: Registry
///
/// One supported OWASP Top 10 category of one edition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs)]
pub struct CategoryEntry {
    /// The edition year.
    pub edition: u16,
    /// The category id, such as `A03`.
    pub id: &'static str,
    /// The official name.
    pub name: &'static str,
}

/// Term: Registry
///
/// A set of supported weaknesses and categories. [`Registry::builtin`] is the curated v1 registry;
/// [`Registry::new`] builds another one, for tests and for editions not yet curated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registry {
    weaknesses: Vec<WeaknessEntry>,
    categories: Vec<CategoryEntry>,
}

impl Registry {
    /// The curated registry shipped with the tool. Adding an entry is an explicit edit here.
    pub fn builtin() -> &'static Registry {
        static BUILTIN: OnceLock<Registry> = OnceLock::new();
        BUILTIN.get_or_init(|| Registry::new(WEAKNESSES.to_vec(), CATEGORIES.to_vec()))
    }

    /// A registry of the given entries; weaknesses are ordered by number, categories by edition
    /// and id.
    pub fn new(mut weaknesses: Vec<WeaknessEntry>, mut categories: Vec<CategoryEntry>) -> Self {
        weaknesses.sort_by_key(|w| w.number);
        categories.sort_by_key(|c| (c.edition, c.id));
        Self {
            weaknesses,
            categories,
        }
    }

    /// The weakness with this CWE number, when registered.
    pub fn weakness(&self, number: u32) -> Option<&WeaknessEntry> {
        self.weaknesses
            .binary_search_by_key(&number, |w| w.number)
            .ok()
            .map(|i| &self.weaknesses[i])
    }

    /// The category of this edition and id, when registered.
    pub fn category(&self, edition: u16, id: &str) -> Option<&CategoryEntry> {
        self.categories
            .iter()
            .find(|c| c.edition == edition && c.id == id)
    }

    /// Whether any category of this edition is registered.
    pub fn has_edition(&self, edition: u16) -> bool {
        self.categories.iter().any(|c| c.edition == edition)
    }

    /// All registered weaknesses, ascending by number.
    pub fn weaknesses(&self) -> &[WeaknessEntry] {
        &self.weaknesses
    }

    /// All registered categories, by edition then id.
    pub fn categories(&self) -> &[CategoryEntry] {
        &self.categories
    }
}

static CATEGORIES: &[CategoryEntry] = &[
    CategoryEntry {
        edition: 2021,
        id: "A01",
        name: "Broken Access Control",
    },
    CategoryEntry {
        edition: 2021,
        id: "A02",
        name: "Cryptographic Failures",
    },
    CategoryEntry {
        edition: 2021,
        id: "A03",
        name: "Injection",
    },
    CategoryEntry {
        edition: 2021,
        id: "A04",
        name: "Insecure Design",
    },
    CategoryEntry {
        edition: 2021,
        id: "A05",
        name: "Security Misconfiguration",
    },
    CategoryEntry {
        edition: 2021,
        id: "A06",
        name: "Vulnerable and Outdated Components",
    },
    CategoryEntry {
        edition: 2021,
        id: "A07",
        name: "Identification and Authentication Failures",
    },
    CategoryEntry {
        edition: 2021,
        id: "A08",
        name: "Software and Data Integrity Failures",
    },
    CategoryEntry {
        edition: 2021,
        id: "A09",
        name: "Security Logging and Monitoring Failures",
    },
    CategoryEntry {
        edition: 2021,
        id: "A10",
        name: "Server Side Request Forgery (SSRF)",
    },
];

// Cross-site scripting (CWE-79) is deliberately absent in v1.
static WEAKNESSES: &[WeaknessEntry] = &[
    WeaknessEntry {
        number: 22,
        name: "Improper Limitation of a Pathname to a Restricted Directory ('Path Traversal')",
        catalog: CATALOG,
        categories: &[(2021, "A01")],
    },
    WeaknessEntry {
        number: 73,
        name: "External Control of File Name or Path",
        catalog: CATALOG,
        categories: &[(2021, "A03"), (2021, "A04")],
    },
    WeaknessEntry {
        number: 77,
        name: "Improper Neutralization of Special Elements used in a Command ('Command Injection')",
        catalog: CATALOG,
        categories: &[(2021, "A03")],
    },
    WeaknessEntry {
        number: 78,
        name: "Improper Neutralization of Special Elements used in an OS Command ('OS Command Injection')",
        catalog: CATALOG,
        categories: &[(2021, "A03")],
    },
    WeaknessEntry {
        number: 88,
        name: "Improper Neutralization of Argument Delimiters in a Command ('Argument Injection')",
        catalog: CATALOG,
        categories: &[(2021, "A03")],
    },
    WeaknessEntry {
        number: 89,
        name: "Improper Neutralization of Special Elements used in an SQL Command ('SQL Injection')",
        catalog: CATALOG,
        categories: &[(2021, "A03")],
    },
    WeaknessEntry {
        number: 90,
        name: "Improper Neutralization of Special Elements used in an LDAP Query ('LDAP Injection')",
        catalog: CATALOG,
        categories: &[(2021, "A03")],
    },
    WeaknessEntry {
        number: 94,
        name: "Improper Control of Generation of Code ('Code Injection')",
        catalog: CATALOG,
        categories: &[(2021, "A03")],
    },
    WeaknessEntry {
        number: 95,
        name: "Improper Neutralization of Directives in Dynamically Evaluated Code ('Eval Injection')",
        catalog: CATALOG,
        categories: &[(2021, "A03")],
    },
    WeaknessEntry {
        number: 98,
        name: "Improper Control of Filename for Include/Require Statement in PHP Program ('PHP Remote File Inclusion')",
        catalog: CATALOG,
        categories: &[(2021, "A03")],
    },
    WeaknessEntry {
        number: 259,
        name: "Use of Hard-coded Password",
        catalog: CATALOG,
        categories: &[(2021, "A02"), (2021, "A07")],
    },
    WeaknessEntry {
        number: 321,
        name: "Use of Hard-coded Cryptographic Key",
        catalog: CATALOG,
        categories: &[(2021, "A02")],
    },
    WeaknessEntry {
        number: 327,
        name: "Use of a Broken or Risky Cryptographic Algorithm",
        catalog: CATALOG,
        categories: &[(2021, "A02")],
    },
    WeaknessEntry {
        number: 328,
        name: "Use of Weak Hash",
        catalog: CATALOG,
        categories: &[(2021, "A02")],
    },
    WeaknessEntry {
        number: 330,
        name: "Use of Insufficiently Random Values",
        catalog: CATALOG,
        categories: &[(2021, "A02")],
    },
    WeaknessEntry {
        number: 338,
        name: "Use of Cryptographically Weak Pseudo-Random Number Generator (PRNG)",
        catalog: CATALOG,
        categories: &[(2021, "A02")],
    },
    WeaknessEntry {
        number: 434,
        name: "Unrestricted Upload of File with Dangerous Type",
        catalog: CATALOG,
        categories: &[(2021, "A04")],
    },
    WeaknessEntry {
        number: 502,
        name: "Deserialization of Untrusted Data",
        catalog: CATALOG,
        categories: &[(2021, "A08")],
    },
    WeaknessEntry {
        number: 601,
        name: "URL Redirection to Untrusted Site ('Open Redirect')",
        catalog: CATALOG,
        categories: &[(2021, "A01")],
    },
    WeaknessEntry {
        number: 611,
        name: "Improper Restriction of XML External Entity Reference",
        catalog: CATALOG,
        categories: &[(2021, "A05")],
    },
    WeaknessEntry {
        number: 614,
        name: "Sensitive Cookie in HTTPS Session Without 'Secure' Attribute",
        catalog: CATALOG,
        categories: &[(2021, "A05")],
    },
    WeaknessEntry {
        number: 643,
        name: "Improper Neutralization of Data within XPath Expressions ('XPath Injection')",
        catalog: CATALOG,
        categories: &[(2021, "A03")],
    },
    WeaknessEntry {
        number: 798,
        name: "Use of Hard-coded Credentials",
        catalog: CATALOG,
        categories: &[(2021, "A07")],
    },
    WeaknessEntry {
        number: 918,
        name: "Server-Side Request Forgery (SSRF)",
        catalog: CATALOG,
        categories: &[(2021, "A10")],
    },
    WeaknessEntry {
        number: 1004,
        name: "Sensitive Cookie Without 'HttpOnly' Flag",
        catalog: CATALOG,
        categories: &[(2021, "A05")],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// spec: security-taxonomy/CWE identifiers are validated against a curated registry
    #[test]
    fn known_and_unknown_weaknesses() {
        let r = Registry::builtin();
        assert!(r.weakness(89).unwrap().name.contains("SQL Injection"));
        assert!(r.weakness(601).is_some());
        assert!(r.weakness(79).is_none());
        assert!(r.weakness(99_999).is_none());
    }

    #[test]
    fn categories_and_editions() {
        let r = Registry::builtin();
        assert_eq!(r.category(2021, "A03").unwrap().name, "Injection");
        assert!(r.category(2021, "A11").is_none());
        assert!(r.has_edition(2021) && !r.has_edition(2025));
        assert_eq!(r.categories().len(), 10);
    }

    #[test]
    fn entries_are_sorted_checked_and_cite_registered_categories() {
        let r = Registry::builtin();
        assert!(r.weaknesses().windows(2).all(|w| w[0].number < w[1].number));
        for w in r.weaknesses() {
            assert_eq!(w.catalog, CATALOG);
            assert!(!w.categories.is_empty());
            for (e, id) in w.categories {
                assert!(
                    r.category(*e, id).is_some(),
                    "CWE-{} cites {e}:{id}",
                    w.number
                );
            }
        }
    }
}
