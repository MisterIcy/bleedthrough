//! Deterministic matching of reported findings to corpus labels, and the counts that follow.
//!
//! A finding matches an expectation when the file is equal, any of the finding's CWEs equals the
//! expectation's CWE, the line ranges overlap and the confidence tier is at least the minimum.
//! Expectations are assigned in sorted order to the first unused finding in sorted order, so the
//! outcome never depends on the order of the report.

use std::collections::BTreeMap;

use crate::corpus::{Case, Corpus, Expect, Label, Range};
use crate::report::{Finding, tier_rank};

/// Counts of true positives, false positives and false negatives.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    /// Expected findings that were reported.
    pub tp: u32,
    /// Findings of the case's CWE inside a safe case's protected range.
    pub fp: u32,
    /// Expected findings that were not reported.
    pub fn_: u32,
    /// Cases marked as known gaps.
    pub known_gaps: u32,
}

impl Counts {
    /// `tp / (tp + fp)`, undefined when nothing was reported.
    pub fn precision(&self) -> Option<f64> {
        ratio(self.tp, self.tp + self.fp)
    }

    /// `tp / (tp + fn)`, undefined when nothing was expected.
    pub fn recall(&self) -> Option<f64> {
        ratio(self.tp, self.tp + self.fn_)
    }

    fn add(&mut self, other: &Self) {
        self.tp += other.tp;
        self.fp += other.fp;
        self.fn_ += other.fn_;
        self.known_gaps += other.known_gaps;
    }
}

fn ratio(numerator: u32, denominator: u32) -> Option<f64> {
    (denominator > 0).then(|| f64::from(numerator) / f64::from(denominator))
}

/// What happened to one case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseOutcome {
    /// The case.
    pub id: String,
    /// Its weakness class.
    pub cwe: String,
    /// Whether every expectation was met (vulnerable) or nothing was reported (safe).
    pub passed: bool,
    /// Whether the manifest marks it as a known gap.
    pub known_gap: bool,
    /// Its counts.
    pub counts: Counts,
}

/// A finding that matched no expectation and lies outside every protected range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unlabelled {
    /// File.
    pub path: String,
    /// First line.
    pub line: u32,
    /// The reporting rule.
    pub rule: String,
    /// The finding's CWEs.
    pub cwes: Vec<String>,
}

/// Counts attributable to a rule: false negatives belong to no rule.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RuleCounts {
    /// Findings of the rule that matched an expectation.
    pub tp: u32,
    /// Findings of the rule inside a safe case's protected range.
    pub fp: u32,
}

/// Everything scoring produces.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Scored {
    /// One outcome per case, in manifest order.
    pub cases: Vec<CaseOutcome>,
    /// Counts per weakness class.
    pub per_cwe: BTreeMap<String, Counts>,
    /// Counts per reporting rule.
    pub per_rule: BTreeMap<String, RuleCounts>,
    /// Unlabelled findings, ordered.
    pub unlabelled: Vec<Unlabelled>,
}

impl Scored {
    /// Totals over every class.
    pub fn totals(&self) -> Counts {
        let mut total = Counts::default();
        self.per_cwe.values().for_each(|c| total.add(c));
        total
    }
}

fn overlaps(range: &Range, finding: &Finding) -> bool {
    range.file == finding.path && finding.start <= range.end && range.start <= finding.end
}

fn compare(a: &Finding, b: &Finding) -> std::cmp::Ordering {
    (&a.path, a.start, a.end, &a.rule, &a.fingerprint)
        .cmp(&(&b.path, b.start, b.end, &b.rule, &b.fingerprint))
        .then_with(|| b.tier.cmp(&a.tier))
        .then_with(|| a.cwes.cmp(&b.cwes))
}

fn new_outcomes(corpus: &Corpus) -> BTreeMap<&str, CaseOutcome> {
    corpus
        .cases
        .iter()
        .map(|case| {
            let known_gaps = u32::from(case.known_gap.is_some());
            let outcome = CaseOutcome {
                id: case.id.clone(),
                cwe: case.cwe.clone(),
                passed: true,
                known_gap: case.known_gap.is_some(),
                counts: Counts {
                    known_gaps,
                    ..Counts::default()
                },
            };
            (case.id.as_str(), outcome)
        })
        .collect()
}

/// Assigns expectations, in sorted order, to the first unused matching finding.
fn match_expectations(
    corpus: &Corpus,
    findings: &[&Finding],
    used: &mut [bool],
    outcomes: &mut BTreeMap<&str, CaseOutcome>,
    scored: &mut Scored,
) {
    let mut expectations: Vec<(&Case, &Expect)> = corpus
        .cases
        .iter()
        .filter(|c| c.label == Label::Vulnerable)
        .flat_map(|c| c.expect.iter().map(move |e| (c, e)))
        .collect();
    expectations.sort_by_key(|(c, e)| (&c.id, &e.range.file, e.range.start, e.range.end, &e.cwe));
    for (case, expect) in expectations {
        let minimum = tier_rank(&expect.min_confidence).unwrap_or(0);
        let found = findings.iter().enumerate().find(|(i, f)| {
            !used[*i]
                && overlaps(&expect.range, f)
                && f.cwes.contains(&expect.cwe)
                && f.tier >= minimum
        });
        let Some(outcome) = outcomes.get_mut(case.id.as_str()) else {
            continue;
        };
        if let Some((i, f)) = found {
            used[i] = true;
            outcome.counts.tp += 1;
            scored.per_rule.entry(f.rule.clone()).or_default().tp += 1;
        } else {
            outcome.counts.fn_ += 1;
            outcome.passed = false;
        }
    }
}

/// Scores `findings` against the corpus.
pub fn score(corpus: &Corpus, findings: &[Finding]) -> Scored {
    let mut findings: Vec<&Finding> = findings.iter().collect();
    findings.sort_by(|a, b| compare(a, b));
    let mut used = vec![false; findings.len()];
    let mut scored = Scored::default();
    let mut outcomes = new_outcomes(corpus);
    match_expectations(corpus, &findings, &mut used, &mut outcomes, &mut scored);

    let leftovers = findings
        .iter()
        .zip(&used)
        .filter(|(_, u)| !**u)
        .map(|(f, _)| *f);
    for finding in leftovers {
        let hit = corpus.cases.iter().find(|case| {
            case.label == Label::Safe
                && finding.cwes.contains(&case.cwe)
                && case.protected().iter().any(|r| overlaps(r, finding))
        });
        match hit.and_then(|case| outcomes.get_mut(case.id.as_str())) {
            Some(outcome) => {
                outcome.counts.fp += 1;
                outcome.passed = false;
                scored.per_rule.entry(finding.rule.clone()).or_default().fp += 1;
            }
            None => scored.unlabelled.push(Unlabelled {
                path: finding.path.clone(),
                line: finding.start,
                rule: finding.rule.clone(),
                cwes: finding.cwes.clone(),
            }),
        }
    }

    for case in &corpus.cases {
        if let Some(outcome) = outcomes.remove(case.id.as_str()) {
            scored
                .per_cwe
                .entry(case.cwe.clone())
                .or_default()
                .add(&outcome.counts);
            scored.cases.push(outcome);
        }
    }
    scored
}
