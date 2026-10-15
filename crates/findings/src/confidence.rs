use crate::ModelError;

/// Term: Confidence
///
/// How sure the analysis is, from lowest to highest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ConfidenceTier {
    /// Low.
    Low,
    /// Medium.
    Medium,
    /// High.
    High,
}

impl ConfidenceTier {
    /// The stable wire name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// Term: Confidence
///
/// The closed vocabulary of reasons a finding is not high confidence. Adding a code is an
/// additive schema change; unknown wire codes are rejected by [`ReasonCode::from_code`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ReasonCode {
    /// The source was inferred rather than declared.
    SourceInferred,
    /// Dynamic dispatch with several candidate targets.
    DynamicDispatchSeveralCandidates,
    /// An unresolved callee was assumed to propagate taint.
    UnresolvedCalleeAssumedPropagate,
    /// The file was parsed with error recovery.
    ParseRecovered,
    /// A type needed by the analysis was unknown.
    TypeUnknown,
    /// The analysis hit a limit and widened its result.
    AnalysisWidened,
    /// A sanitizer was applied that is wrong for the sink context.
    SanitizerWrongForSinkContext,
}

impl ReasonCode {
    /// Every code, in declaration order.
    pub const ALL: [Self; 7] = [
        Self::SourceInferred,
        Self::DynamicDispatchSeveralCandidates,
        Self::UnresolvedCalleeAssumedPropagate,
        Self::ParseRecovered,
        Self::TypeUnknown,
        Self::AnalysisWidened,
        Self::SanitizerWrongForSinkContext,
    ];

    /// The stable snake_case wire code.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SourceInferred => "source_inferred",
            Self::DynamicDispatchSeveralCandidates => "dynamic_dispatch_several_candidates",
            Self::UnresolvedCalleeAssumedPropagate => "unresolved_callee_assumed_propagate",
            Self::ParseRecovered => "parse_recovered",
            Self::TypeUnknown => "type_unknown",
            Self::AnalysisWidened => "analysis_widened",
            Self::SanitizerWrongForSinkContext => "sanitizer_wrong_for_sink_context",
        }
    }

    /// Parses a wire code; unknown codes are rejected.
    pub fn from_code(code: &str) -> Result<Self, ModelError> {
        Self::ALL
            .into_iter()
            .find(|c| c.as_str() == code)
            .ok_or_else(|| ModelError::UnknownReasonCode(code.to_string()))
    }
}

/// Term: Confidence
///
/// A reason code with an optional short detail string.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Reason {
    code: ReasonCode,
    detail: Option<String>,
}

impl Reason {
    /// A reason without detail.
    pub fn new(code: ReasonCode) -> Self {
        Self { code, detail: None }
    }

    /// A reason with a detail string, such as the candidate count.
    pub fn with_detail(code: ReasonCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: Some(detail.into()),
        }
    }

    /// The code.
    pub fn code(&self) -> ReasonCode {
        self.code
    }

    /// The detail, when present.
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
}

/// Term: Confidence
///
/// A tier with its reasons. A tier below high requires at least one reason. Reasons are sorted
/// by code then detail and deduplicated.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Confidence {
    tier: ConfidenceTier,
    reasons: Vec<Reason>,
}

impl Confidence {
    /// Fails when the tier is below high and no reason is given.
    pub fn new(tier: ConfidenceTier, mut reasons: Vec<Reason>) -> Result<Self, ModelError> {
        if tier < ConfidenceTier::High && reasons.is_empty() {
            return Err(ModelError::MissingReason);
        }
        reasons.sort();
        reasons.dedup();
        Ok(Self { tier, reasons })
    }

    /// The tier.
    pub fn tier(&self) -> ConfidenceTier {
        self.tier
    }

    /// The reasons, in canonical order.
    pub fn reasons(&self) -> &[Reason] {
        &self.reasons
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sub_high_needs_a_reason() {
        assert_eq!(
            Confidence::new(ConfidenceTier::Medium, vec![]).unwrap_err(),
            ModelError::MissingReason
        );
        assert!(Confidence::new(ConfidenceTier::Low, vec![]).is_err());
        assert!(Confidence::new(ConfidenceTier::High, vec![]).is_ok());
    }

    #[test]
    fn vocabulary_has_every_required_code_and_round_trips() {
        let required = [
            "source_inferred",
            "dynamic_dispatch_several_candidates",
            "unresolved_callee_assumed_propagate",
            "parse_recovered",
            "type_unknown",
            "analysis_widened",
            "sanitizer_wrong_for_sink_context",
        ];
        for code in required {
            assert_eq!(ReasonCode::from_code(code).unwrap().as_str(), code);
        }
        assert_eq!(ReasonCode::ALL.len(), required.len());
        assert!(ReasonCode::from_code("made_up").is_err());
    }

    #[test]
    fn reasons_are_canonical_and_keep_detail() {
        let c = Confidence::new(
            ConfidenceTier::Medium,
            vec![
                Reason::new(ReasonCode::TypeUnknown),
                Reason::with_detail(ReasonCode::DynamicDispatchSeveralCandidates, "3"),
                Reason::new(ReasonCode::TypeUnknown),
            ],
        )
        .unwrap();
        assert_eq!(c.reasons().len(), 2);
        assert_eq!(
            c.reasons()[0].code(),
            ReasonCode::DynamicDispatchSeveralCandidates
        );
        assert_eq!(c.reasons()[0].detail(), Some("3"));
    }
}
