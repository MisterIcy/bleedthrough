use crate::{ConfidenceTier, Coverage, Finding, Suppressed};

/// The report schema version; a breaking change bumps the major number.
pub const REPORT_SCHEMA_VERSION: &str = "1.0";

/// Term: Report
///
/// The run options that affect results.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RunOptions {
    /// Findings below this tier were hidden.
    pub confidence_threshold: ConfidenceTier,
    /// Whether paranoid rules ran.
    pub paranoid: bool,
    /// The PHP version modelled.
    pub php_version: String,
    /// Identifier of the rule and pack set.
    pub ruleset_id: String,
}

impl RunOptions {
    /// Builds the options.
    pub fn new(
        confidence_threshold: ConfidenceTier,
        paranoid: bool,
        php_version: impl Into<String>,
        ruleset_id: impl Into<String>,
    ) -> Self {
        Self {
            confidence_threshold,
            paranoid,
            php_version: php_version.into(),
            ruleset_id: ruleset_id.into(),
        }
    }
}

/// Term: Report
///
/// Who produced a report and with which options. Holds no times, durations or process data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunInfo {
    tool_name: String,
    tool_version: String,
    options: RunOptions,
}

impl RunInfo {
    /// Builds the run description.
    pub fn new(
        tool_name: impl Into<String>,
        tool_version: impl Into<String>,
        options: RunOptions,
    ) -> Self {
        Self {
            tool_name: tool_name.into(),
            tool_version: tool_version.into(),
            options,
        }
    }

    /// The tool name.
    pub fn tool_name(&self) -> &str {
        &self.tool_name
    }

    /// The tool version.
    pub fn tool_version(&self) -> &str {
        &self.tool_version
    }

    /// The run options.
    pub fn options(&self) -> &RunOptions {
        &self.options
    }

    /// The schema version of the report this belongs to.
    pub fn schema_version(&self) -> &'static str {
        REPORT_SCHEMA_VERSION
    }
}

/// Term: Report
///
/// A complete, immutable result of a run: findings in a total order, coverage and suppressed
/// counts. The same inputs always give an equal report, whatever order they arrived in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    run: RunInfo,
    findings: Vec<Finding>,
    coverage: Coverage,
    suppressed: Suppressed,
}

impl Report {
    /// Sorts the findings by path, start, rule and fingerprint, and puts the coverage lists
    /// into canonical order.
    pub fn new(
        run: RunInfo,
        mut findings: Vec<Finding>,
        mut coverage: Coverage,
        suppressed: Suppressed,
    ) -> Self {
        findings.sort_by(|a, b| a.order_key().cmp(&b.order_key()));
        coverage.canonicalize();
        Self {
            run,
            findings,
            coverage,
            suppressed,
        }
    }

    /// The run description.
    pub fn run(&self) -> &RunInfo {
        &self.run
    }

    /// The findings in order.
    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    /// What was and was not analysed.
    pub fn coverage(&self) -> &Coverage {
        &self.coverage
    }

    /// Findings hidden from this report.
    pub fn suppressed(&self) -> &Suppressed {
        &self.suppressed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Applicability, Confidence, Cwe, Edit, FindingParts, Fingerprint, FingerprintInput, Fix,
        Location, Message, OwaspRef, Position, RelPath, RuleId, Severity, SeverityLevel, Taxonomy,
    };

    fn finding(path: &str, line: u32, col: u32, rule: &str, symbol: &str) -> Finding {
        let rule = RuleId::new(rule).unwrap();
        let p = Position::new(line, col).unwrap();
        let rel = RelPath::new(path).unwrap();
        let fix = Edit::new(rel.clone(), 0, 1, "a", "b")
            .and_then(|e| Fix::new("d", Applicability::Mechanical, vec![e]))
            .ok();
        Finding::new(FindingParts {
            fingerprint: Fingerprint::compute(&FingerprintInput::new(
                rule.clone(),
                symbol,
                vec![],
                0,
            )),
            rule,
            message: Message::new("m").unwrap(),
            location: Location::new(rel, p, p, 0, 0).unwrap(),
            taxonomy: Taxonomy::new(
                vec![Cwe::new(89).unwrap()],
                vec![OwaspRef::new(2021, "A03").unwrap()],
            )
            .unwrap(),
            severity: Severity::new(SeverityLevel::High),
            confidence: Confidence::new(ConfidenceTier::High, vec![]).unwrap(),
            paranoid: false,
            trace: None,
            fix,
        })
    }

    fn report(findings: Vec<Finding>) -> Report {
        let opts = RunOptions::new(ConfidenceTier::Medium, false, "8.3", "rs1");
        Report::new(
            RunInfo::new("bleedthrough", "0.0.0", opts),
            findings,
            Coverage::default(),
            Suppressed::default(),
        )
    }

    fn set() -> Vec<Finding> {
        vec![
            finding("b.php", 1, 1, "r1", "s"),
            finding("a.php", 9, 1, "r1", "s"),
            finding("a.php", 2, 5, "r2", "s"),
            finding("a.php", 2, 5, "r1", "t"),
            finding("a.php", 2, 5, "r1", "s"),
            finding("a.php", 2, 1, "z", "s"),
        ]
    }

    #[test]
    fn ordered_by_path_start_rule_then_fingerprint() {
        let r = report(set());
        let keys: Vec<_> = r
            .findings()
            .iter()
            .map(|f| {
                (
                    f.location().path().as_str(),
                    f.location().start().line(),
                    f.location().start().column(),
                    f.rule().as_str(),
                )
            })
            .collect();
        let mut expected = keys.clone();
        expected.sort();
        assert_eq!(keys, expected);
        let ties: Vec<_> = r
            .findings()
            .iter()
            .filter(|f| {
                f.rule().as_str() == "r1"
                    && f.location().start().line() == 2
                    && f.location().path().as_str() == "a.php"
            })
            .collect();
        assert_eq!(ties.len(), 2);
        assert!(ties[0].fingerprint() < ties[1].fingerprint());
    }

    #[test]
    fn shuffled_input_gives_an_identical_report() {
        let forward = report(set());
        let mut reversed = set();
        reversed.reverse();
        assert_eq!(forward, report(reversed));
        let mut rotated = set();
        rotated.rotate_left(2);
        assert_eq!(forward, report(rotated));
    }

    #[test]
    fn empty_findings_still_hold_coverage() {
        let mut coverage = Coverage::new("8.3");
        coverage.files_failed = 3;
        let opts = RunOptions::new(ConfidenceTier::High, true, "8.3", "rs1");
        let r = Report::new(
            RunInfo::new("bt", "0", opts),
            vec![],
            coverage,
            Suppressed::default(),
        );
        assert!(r.findings().is_empty());
        assert_eq!(r.coverage().files_failed, 3);
        assert_eq!(r.run().schema_version(), "1.0");
    }
}
