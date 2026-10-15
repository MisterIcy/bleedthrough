//! Ubiquitous-language check: public domain types must name a glossary term.

use std::collections::BTreeSet;

use crate::apiscan::Entry;

pub const REQUIRED_TERMS: [&str; 18] = [
    "Source",
    "Sink",
    "Sanitizer",
    "Propagator",
    "Taint",
    "Entry point",
    "Rule",
    "Pack",
    "Finding",
    "Fingerprint",
    "Severity",
    "Confidence",
    "Coverage",
    "Summary",
    "Baseline",
    "Parse quality",
    "Fix",
    "Suppression",
];

/// Terms are the `## ` headings of the glossary; returns the terms and any duplicates found.
pub fn parse_terms(markdown: &str) -> (BTreeSet<String>, Vec<String>) {
    let mut terms = BTreeSet::new();
    let mut errors = Vec::new();
    for heading in markdown.lines().filter_map(|l| l.strip_prefix("## ")) {
        let term = heading.trim().to_string();
        if !terms.insert(term.clone()) {
            errors.push(format!("glossary defines `{term}` more than once"));
        }
    }
    for required in REQUIRED_TERMS.iter().filter(|t| !terms.contains(**t)) {
        errors.push(format!("glossary is missing required term `{required}`"));
    }
    (terms, errors)
}

/// Checks the public types of one domain crate against the glossary.
pub fn check_types(krate: &str, entries: &[Entry], terms: &BTreeSet<String>) -> Vec<String> {
    let mut errors = Vec::new();
    for entry in entries.iter().filter(|e| e.is_type) {
        match term_of(entry) {
            None => errors.push(format!(
                "{krate}: public type `{}` has no `Term:` line (name a glossary term or `Term: exempt`)",
                entry.path
            )),
            Some("exempt") => {}
            Some(term) if !terms.contains(term) => {
                errors.push(format!("{krate}: `{}` names unknown glossary term `{term}`", entry.path));
            }
            Some(_) => {}
        }
    }
    errors
}

fn term_of(entry: &Entry) -> Option<&str> {
    entry
        .docs
        .iter()
        .find_map(|line| line.strip_prefix("Term:"))
        .map(str::trim)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glossary(extra: &str) -> String {
        let mut text = String::new();
        for term in REQUIRED_TERMS {
            text.push_str(&format!("## {term}\n\nDefinition.\n\n"));
        }
        text + extra
    }

    fn typed(doc: &[&str]) -> Entry {
        Entry {
            path: "crate::Thing".into(),
            kind: "struct".into(),
            signature: String::new(),
            is_type: true,
            docs: doc.iter().map(|s| (*s).into()).collect(),
        }
    }

    #[test]
    fn complete_glossary_parses_cleanly() {
        let (terms, errors) = parse_terms(&glossary(""));
        assert!(errors.is_empty());
        assert_eq!(terms.len(), REQUIRED_TERMS.len());
    }

    #[test]
    fn duplicate_and_missing_terms_fail() {
        let (_, errors) = parse_terms(&glossary("## Sink\n"));
        assert!(errors.iter().any(|e| e.contains("more than once")));
        let (_, errors) = parse_terms("## Source\n");
        assert!(
            errors
                .iter()
                .any(|e| e.contains("missing required term `Sink`"))
        );
    }

    #[test]
    fn type_without_term_fails() {
        let (terms, _) = parse_terms(&glossary(""));
        let errors = check_types("domain", &[typed(&["Just docs."])], &terms);
        assert!(errors[0].contains("crate::Thing"));
    }

    #[test]
    fn unknown_term_fails_and_exempt_passes() {
        let (terms, _) = parse_terms(&glossary(""));
        let errors = check_types("domain", &[typed(&["Term: Gizmo"])], &terms);
        assert!(errors[0].contains("Gizmo"));
        assert!(check_types("domain", &[typed(&["Term: exempt"])], &terms).is_empty());
        assert!(check_types("domain", &[typed(&["Term: Finding"])], &terms).is_empty());
    }

    #[test]
    fn removing_a_term_in_use_fails() {
        let without = glossary("").replace("## Finding\n", "## Findings\n");
        let (terms, _) = parse_terms(&without);
        let errors = check_types("domain", &[typed(&["Term: Finding"])], &terms);
        assert!(errors[0].contains("unknown glossary term `Finding`"));
    }
}
