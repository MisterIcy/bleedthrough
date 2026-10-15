//! Plain definition records: what a [`RuleSource`](crate::RuleSource) hands to the rules crate.
//! They carry text exactly as written; [`RuleDraft::validate`](crate::RuleDraft::validate) turns
//! them into a [`Rule`](crate::Rule) or reports every problem.
#![allow(
    clippy::exhaustive_structs,
    clippy::exhaustive_enums,
    reason = "plain data records filled in by rule sources"
)]

/// Term: Rule
///
/// A rule as written in a rule file, before validation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuleDraft {
    /// The identifier, `BT` and four digits.
    pub id: Option<String>,
    /// A short title.
    pub title: Option<String>,
    /// What the rule detects and why it matters.
    pub description: Option<String>,
    /// How to fix a finding.
    pub remediation: Option<String>,
    /// The area, such as `injection`; informational.
    pub area: Option<String>,
    /// Reference URLs or identifiers.
    pub references: Vec<String>,
    /// CWE tags, `CWE-<number>`.
    pub cwe: Vec<String>,
    /// OWASP tags, `OWASP:<edition>:<id>`.
    pub owasp: Vec<String>,
    /// Deliberate cross-mapped OWASP categories with their reasons.
    pub owasp_extra: Vec<ExtraCategoryDraft>,
    /// The CVSS 3.1 base metrics.
    pub metrics: MetricsDraft,
    /// Whether the rule runs only in paranoid mode.
    pub paranoid: bool,
    /// The lowest PHP version the rule applies to, `major.minor`.
    pub php_min: Option<String>,
    /// The highest PHP version the rule applies to, `major.minor`.
    pub php_max: Option<String>,
    /// The owning pack, when the rule belongs to one.
    pub pack: Option<String>,
    /// The taint or pattern body.
    pub body: Option<BodyDraft>,
}

/// Term: Rule
///
/// A cross-mapped category with the reason it was added.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExtraCategoryDraft {
    /// The category, `OWASP:<edition>:<id>`.
    pub category: String,
    /// Why the weakness maps to it.
    pub reason: String,
}

/// Term: Rule
///
/// The eight CVSS 3.1 base metrics as words; every one is required.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MetricsDraft {
    /// `network`, `adjacent`, `local` or `physical`.
    pub attack_vector: Option<String>,
    /// `low` or `high`.
    pub attack_complexity: Option<String>,
    /// `none`, `low` or `high`.
    pub privileges_required: Option<String>,
    /// `none` or `required`.
    pub user_interaction: Option<String>,
    /// `unchanged` or `changed`.
    pub scope: Option<String>,
    /// `none`, `low` or `high`.
    pub confidentiality: Option<String>,
    /// `none`, `low` or `high`.
    pub integrity: Option<String>,
    /// `none`, `low` or `high`.
    pub availability: Option<String>,
}

/// Term: Rule
///
/// The body of a rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyDraft {
    /// Sources, sinks, sanitizers and propagators.
    Taint(TaintDraft),
    /// A simple AST pattern.
    Pattern(PatternDraft),
}

/// Term: Rule
///
/// A taint rule as written.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TaintDraft {
    /// Source symbols.
    pub sources: Vec<String>,
    /// Sinks.
    pub sinks: Vec<SinkDraft>,
    /// Sanitizers.
    pub sanitizers: Vec<SanitizerDraft>,
    /// Propagators.
    pub propagators: Vec<PropagatorDraft>,
}

/// Term: Rule
///
/// A sink as written.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SinkDraft {
    /// The symbol.
    pub symbol: String,
    /// The vulnerable arguments, by position (from 1) or parameter name.
    pub args: Vec<String>,
    /// The sink class.
    pub class: String,
}

/// Term: Rule
///
/// A sanitizer as written.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SanitizerDraft {
    /// The symbol.
    pub symbol: String,
    /// The sink classes it neutralises taint for.
    pub clears: Vec<String>,
}

/// Term: Rule
///
/// A propagator as written.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PropagatorDraft {
    /// The symbol.
    pub symbol: String,
    /// The inputs that flow to the result: positions, names or `this`.
    pub from: Vec<String>,
}

/// Term: Rule
///
/// A pattern rule as written.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PatternDraft {
    /// `call`, `use` or `include-non-literal`.
    pub shape: String,
    /// The symbol, for `call` and `use`.
    pub symbol: Option<String>,
    /// Argument conditions, for `call`.
    pub args: Vec<ArgConditionDraft>,
}

/// Term: Rule
///
/// An argument condition as written.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ArgConditionDraft {
    /// The argument, by position (from 1) or parameter name.
    pub arg: String,
    /// `literal`, `non-literal`, `absent`, `equals` or `matches`.
    pub test: String,
    /// The value, for `equals` and `matches`.
    pub value: Option<String>,
}
