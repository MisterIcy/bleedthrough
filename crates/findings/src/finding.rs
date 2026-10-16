use crate::{Confidence, Fingerprint, Fix, Location, Message, RuleId, Trace};
use bleedthrough_taxonomy::{Severity, Taxonomy};

/// Term: Finding
///
/// The parts of a finding. Each part validated itself on construction, so assembling a
/// [`Finding`] cannot fail; in particular a [`Taxonomy`] always has a CWE and an OWASP tag.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    clippy::exhaustive_structs,
    reason = "an input record: every part is required by callers"
)]
pub struct FindingParts {
    /// The rule that produced the finding.
    pub rule: RuleId,
    /// The concise message.
    pub message: Message,
    /// The primary location.
    pub location: Location,
    /// The mandatory classification.
    pub taxonomy: Taxonomy,
    /// How serious the finding is.
    pub severity: Severity,
    /// How sure the analysis is.
    pub confidence: Confidence,
    /// The stable identity.
    pub fingerprint: Fingerprint,
    /// Whether the rule runs only in paranoid mode.
    pub paranoid: bool,
    /// The flow from source to sink, when available.
    pub trace: Option<Trace>,
    /// A suggested fix, when a safe one is known.
    pub fix: Option<Fix>,
}

/// Term: Finding
///
/// A reported instance of a rule matching. Immutable; carries no wall-clock time or
/// machine-specific data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    parts: FindingParts,
}

impl Finding {
    /// Assembles a finding from validated parts.
    pub fn new(parts: FindingParts) -> Self {
        Self { parts }
    }

    /// The rule that produced the finding.
    pub fn rule(&self) -> &RuleId {
        &self.parts.rule
    }

    /// The concise message.
    pub fn message(&self) -> &Message {
        &self.parts.message
    }

    /// The primary location.
    pub fn location(&self) -> &Location {
        &self.parts.location
    }

    /// The classification.
    pub fn taxonomy(&self) -> &Taxonomy {
        &self.parts.taxonomy
    }

    /// The severity.
    pub fn severity(&self) -> &Severity {
        &self.parts.severity
    }

    /// The confidence.
    pub fn confidence(&self) -> &Confidence {
        &self.parts.confidence
    }

    /// The fingerprint.
    pub fn fingerprint(&self) -> &Fingerprint {
        &self.parts.fingerprint
    }

    /// Whether the rule runs only in paranoid mode.
    pub fn paranoid(&self) -> bool {
        self.parts.paranoid
    }

    /// The trace, when available.
    pub fn trace(&self) -> Option<&Trace> {
        self.parts.trace.as_ref()
    }

    /// The fix, when one is known.
    pub fn fix(&self) -> Option<&Fix> {
        self.parts.fix.as_ref()
    }

    /// The total order: path, start position, rule, fingerprint.
    pub(crate) fn order_key(&self) -> impl Ord + '_ {
        let loc = self.location();
        (loc.path(), loc.start(), self.rule(), self.fingerprint())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConfidenceTier, FingerprintInput, Position, Reason, ReasonCode, RelPath};
    use bleedthrough_taxonomy::{
        AttackComplexity, AttackVector, Cwe, Impact, Metrics, OwaspRef, PrivilegesRequired, Scope,
        SeverityLevel, UserInteraction,
    };

    fn finding(severity: Severity, confidence: Confidence) -> Finding {
        let rule = RuleId::new("BT0001").unwrap();
        let p = Position::new(1, 1).unwrap();
        Finding::new(FindingParts {
            fingerprint: Fingerprint::compute(&FingerprintInput::new(rule.clone(), "f", vec![], 0)),
            rule,
            message: Message::new("m").unwrap(),
            location: Location::new(RelPath::new("a.php").unwrap(), p, p, 0, 0).unwrap(),
            taxonomy: Taxonomy::new(
                vec![Cwe::new(89).unwrap()],
                vec![OwaspRef::new(2021, "A03").unwrap()],
            )
            .unwrap(),
            severity,
            confidence,
            paranoid: false,
            trace: None,
            fix: None,
        })
    }

    /// spec: finding-model/Severity is a level with an optional derived score
    #[test]
    fn severity_from_rule_metrics_is_independent_of_confidence() {
        let metrics = Metrics {
            attack_vector: AttackVector::Network,
            attack_complexity: AttackComplexity::Low,
            privileges_required: PrivilegesRequired::None,
            user_interaction: UserInteraction::None,
            scope: Scope::Unchanged,
            confidentiality: Impact::High,
            integrity: Impact::High,
            availability: Impact::High,
        };
        let reason = Reason::new(ReasonCode::SourceInferred);
        let low = Confidence::new(ConfidenceTier::Low, vec![reason]).unwrap();
        let f = finding(Severity::from_metrics(&metrics), low);
        assert_eq!(f.severity().level(), SeverityLevel::Critical);
        assert_eq!(f.severity().score_text().unwrap(), "9.8");
        assert_eq!(f.confidence().tier(), ConfidenceTier::Low);
    }

    #[test]
    fn a_level_only_finding_is_valid() {
        let high = Confidence::new(ConfidenceTier::High, vec![]).unwrap();
        let f = finding(Severity::new(SeverityLevel::Low), high);
        assert!(f.severity().score_tenths().is_none() && f.severity().vector().is_none());
    }
}
