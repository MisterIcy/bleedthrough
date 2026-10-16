//! The shape-coverage report: per weakness class, which flow shapes have no case.

use std::collections::{BTreeMap, BTreeSet};

use crate::corpus::Corpus;

/// Coverage of one weakness class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassCoverage {
    /// The weakness class.
    pub cwe: String,
    /// Number of cases (both labels).
    pub cases: usize,
    /// Shapes with at least one case.
    pub covered: Vec<String>,
    /// Shapes the class does not admit, with the stated reason.
    pub not_applicable: Vec<String>,
    /// Shapes with no case.
    pub missing: Vec<String>,
}

/// The coverage of every class that has at least one case, ordered by CWE number then text.
pub fn coverage(corpus: &Corpus) -> Vec<ClassCoverage> {
    let mut by_class: BTreeMap<String, Vec<&crate::corpus::Case>> = BTreeMap::new();
    for case in &corpus.cases {
        by_class.entry(case.cwe.clone()).or_default().push(case);
    }
    let mut classes: Vec<_> = by_class.into_iter().collect();
    classes.sort_by_key(|(cwe, _)| (cwe_number(cwe), cwe.clone()));
    classes
        .into_iter()
        .map(|(cwe, cases)| {
            let have: BTreeSet<&str> = cases
                .iter()
                .flat_map(|c| c.shapes.iter().map(String::as_str))
                .collect();
            let na: BTreeSet<&str> = corpus
                .classes
                .iter()
                .filter(|n| n.cwe == cwe)
                .flat_map(|n| n.not_applicable.iter().map(String::as_str))
                .collect();
            let pick = |keep: &dyn Fn(&str) -> bool| -> Vec<String> {
                corpus.shapes.iter().filter(|s| keep(s)).cloned().collect()
            };
            ClassCoverage {
                cases: cases.len(),
                covered: pick(&|s| have.contains(s)),
                not_applicable: pick(&|s| na.contains(s) && !have.contains(s)),
                missing: pick(&|s| !have.contains(s) && !na.contains(s)),
                cwe,
            }
        })
        .collect()
}

fn cwe_number(cwe: &str) -> u64 {
    cwe.strip_prefix("CWE-")
        .and_then(|n| n.parse().ok())
        .unwrap_or(u64::MAX)
}

/// The report as text (markdown), deterministic.
pub fn render(coverage: &[ClassCoverage], shapes: &[String]) -> String {
    let mut out = String::from("| CWE | Cases |");
    for shape in shapes {
        out.push_str(&format!(" {shape} |"));
    }
    out.push_str("\n|---|---|");
    out.push_str(&"---|".repeat(shapes.len()));
    out.push('\n');
    for class in coverage {
        out.push_str(&format!("| {} | {} |", class.cwe, class.cases));
        for shape in shapes {
            let mark = if class.covered.contains(shape) {
                "yes"
            } else if class.not_applicable.contains(shape) {
                "n/a"
            } else {
                "**missing**"
            };
            out.push_str(&format!(" {mark} |"));
        }
        out.push('\n');
    }
    for class in coverage.iter().filter(|c| !c.missing.is_empty()) {
        out.push_str(&format!(
            "\n{} has no case for: {}\n",
            class.cwe,
            class.missing.join(", ")
        ));
    }
    out
}
