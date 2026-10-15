use std::fmt;

use bleedthrough_taxonomy::{
    AttackComplexity, AttackVector, Cwe, Impact, Metrics, OwaspRef, PrivilegesRequired, Registry,
    Scope, Severity, Taxonomy, UserInteraction,
};

use crate::body::{RuleBody, validate_body};
use crate::draft::{ExtraCategoryDraft, MetricsDraft, RuleDraft};
use crate::error::RuleError;
use crate::id::RuleId;

/// Term: Rule
///
/// Where a rule was defined, for error messages. Never part of the rule's content or hash.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[allow(
    clippy::exhaustive_structs,
    reason = "a plain pair filled in by rule sources"
)]
pub struct Origin {
    /// The file, as the source names it.
    pub file: String,
    /// The line of the rule, from 1.
    pub line: u32,
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.file, self.line)
    }
}

/// Term: Rule
///
/// A PHP version as `major.minor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhpVersion {
    major: u8,
    minor: u8,
}

impl PhpVersion {
    /// Parses `8.3`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let bad = || format!("invalid PHP version {text:?}: expected `major.minor`, such as `8.3`");
        let (major, minor) = text.split_once('.').ok_or_else(bad)?;
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        if !digits(major) || !digits(minor) {
            return Err(bad());
        }
        Ok(Self {
            major: major.parse().map_err(|_| bad())?,
            minor: minor.parse().map_err(|_| bad())?,
        })
    }

    /// The major version.
    pub fn major(&self) -> u8 {
        self.major
    }

    /// The minor version.
    pub fn minor(&self) -> u8 {
        self.minor
    }
}

impl fmt::Display for PhpVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// Term: Rule
///
/// A validated rule. Fields are private and every constructor validates, so an invalid rule cannot
/// exist. Built from a [`RuleDraft`] by [`RuleDraft::validate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    id: RuleId,
    title: String,
    description: String,
    remediation: String,
    area: Option<String>,
    references: Vec<String>,
    taxonomy: Taxonomy,
    metrics: Metrics,
    paranoid: bool,
    php_min: Option<PhpVersion>,
    php_max: Option<PhpVersion>,
    pack: Option<String>,
    body: RuleBody,
    origin: Option<Origin>,
}

impl Rule {
    /// The identifier.
    pub fn id(&self) -> &RuleId {
        &self.id
    }

    /// The title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The description.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// The remediation text.
    pub fn remediation(&self) -> &str {
        &self.remediation
    }

    /// The area, when given.
    pub fn area(&self) -> Option<&str> {
        self.area.as_deref()
    }

    /// The references.
    pub fn references(&self) -> &[String] {
        &self.references
    }

    /// The CWE and OWASP tags.
    pub fn taxonomy(&self) -> &Taxonomy {
        &self.taxonomy
    }

    /// The declared CVSS 3.1 base metrics.
    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    /// The severity of every finding of this rule, derived from its metrics.
    pub fn severity(&self) -> Severity {
        Severity::from_metrics(&self.metrics)
    }

    /// Whether the rule runs only in paranoid mode.
    pub fn paranoid(&self) -> bool {
        self.paranoid
    }

    /// The lowest PHP version the rule applies to; `None` means no lower bound.
    pub fn php_min(&self) -> Option<PhpVersion> {
        self.php_min
    }

    /// The highest PHP version the rule applies to; `None` means no upper bound.
    pub fn php_max(&self) -> Option<PhpVersion> {
        self.php_max
    }

    /// The owning pack, when any.
    pub fn pack(&self) -> Option<&str> {
        self.pack.as_deref()
    }

    /// The taint or pattern body.
    pub fn body(&self) -> &RuleBody {
        &self.body
    }

    /// Where the rule was defined, when the source recorded it.
    pub fn origin(&self) -> Option<&Origin> {
        self.origin.as_ref()
    }

    /// Records where the rule was defined.
    #[must_use]
    pub fn with_origin(mut self, origin: Origin) -> Self {
        self.origin = Some(origin);
        self
    }

    /// A canonical text form of the rule's content: fixed field order, sets sorted, no origin and
    /// no comments, so it is stable under reordering and under edits that do not change meaning.
    pub fn canonical(&self) -> String {
        let mut lines = vec![
            format!("id={}", self.id),
            format!("title={:?}", self.title),
            format!("description={:?}", self.description),
            format!("remediation={:?}", self.remediation),
            format!("area={:?}", self.area),
            format!("paranoid={}", self.paranoid),
            format!(
                "php={:?}..{:?}",
                self.php_min.map(|v| v.to_string()),
                self.php_max.map(|v| v.to_string())
            ),
            format!("pack={:?}", self.pack),
            format!("metrics={}", self.metrics),
        ];
        let mut refs = self.references.clone();
        refs.sort();
        lines.extend(refs.iter().map(|r| format!("reference={r:?}")));
        lines.extend(self.taxonomy.cwe().iter().map(|c| format!("cwe={c}")));
        lines.extend(self.taxonomy.owasp().iter().map(|o| format!("owasp={o}")));
        self.body.canonical(&mut lines);
        let mut text = lines.join("\n");
        text.push('\n');
        text
    }
}

fn required(value: &Option<String>, field: &str, errors: &mut Vec<RuleError>) -> String {
    match value.as_deref().map(str::trim) {
        Some(v) if !v.is_empty() => v.to_string(),
        Some(_) => {
            errors.push(RuleError::new(field, "must not be empty"));
            String::new()
        }
        None => {
            errors.push(RuleError::new(field, "is required"));
            String::new()
        }
    }
}

fn metric<T>(
    value: &Option<String>,
    field: &str,
    words: &[&str],
    parse: fn(&str) -> Option<T>,
    errors: &mut Vec<RuleError>,
) -> Option<T> {
    let field = format!("metrics.{field}");
    let Some(word) = value.as_deref() else {
        errors.push(RuleError::new(
            field,
            format!("is required; one of: {}", words.join(", ")),
        ));
        return None;
    };
    let parsed = parse(word);
    if parsed.is_none() {
        errors.push(RuleError::new(
            field,
            format!("invalid value {word:?}; one of: {}", words.join(", ")),
        ));
    }
    parsed
}

fn metrics(d: &MetricsDraft, errors: &mut Vec<RuleError>) -> Option<Metrics> {
    let (av, ac) = (&d.attack_vector, &d.attack_complexity);
    let (pr, ui) = (&d.privileges_required, &d.user_interaction);
    let imp = |v: &Option<String>, f: &str, e: &mut Vec<RuleError>| {
        metric(v, f, Impact::words(), Impact::parse, e)
    };
    let attack_vector = metric(
        av,
        "attack_vector",
        AttackVector::words(),
        AttackVector::parse,
        errors,
    );
    let attack_complexity = metric(
        ac,
        "attack_complexity",
        AttackComplexity::words(),
        AttackComplexity::parse,
        errors,
    );
    let privileges_required = metric(
        pr,
        "privileges_required",
        PrivilegesRequired::words(),
        PrivilegesRequired::parse,
        errors,
    );
    let user_interaction = metric(
        ui,
        "user_interaction",
        UserInteraction::words(),
        UserInteraction::parse,
        errors,
    );
    let scope = metric(&d.scope, "scope", Scope::words(), Scope::parse, errors);
    let confidentiality = imp(&d.confidentiality, "confidentiality", errors);
    let integrity = imp(&d.integrity, "integrity", errors);
    let availability = imp(&d.availability, "availability", errors);
    Some(Metrics {
        attack_vector: attack_vector?,
        attack_complexity: attack_complexity?,
        privileges_required: privileges_required?,
        user_interaction: user_interaction?,
        scope: scope?,
        confidentiality: confidentiality?,
        integrity: integrity?,
        availability: availability?,
    })
}

fn tags<T, E: fmt::Display>(
    items: &[String],
    field: &str,
    parse: impl Fn(&str) -> Result<T, E>,
    errors: &mut Vec<RuleError>,
) -> Vec<T> {
    let mut out = Vec::new();
    for (i, text) in items.iter().enumerate() {
        match parse(text) {
            Ok(v) => out.push(v),
            Err(e) => errors.push(RuleError::new(format!("{field}[{i}]"), e.to_string())),
        }
    }
    out
}

fn extras(items: &[ExtraCategoryDraft], errors: &mut Vec<RuleError>) -> Vec<(OwaspRef, String)> {
    let mut out = Vec::new();
    for (i, e) in items.iter().enumerate() {
        match OwaspRef::parse(&e.category) {
            Ok(c) => out.push((c, e.reason.clone())),
            Err(err) => errors.push(RuleError::new(
                format!("owasp_extra[{i}].category"),
                err.to_string(),
            )),
        }
    }
    out
}

fn taxonomy(d: &RuleDraft, errors: &mut Vec<RuleError>) -> Option<Taxonomy> {
    let cwe = tags(&d.cwe, "cwe", Cwe::parse, errors);
    let owasp = tags(&d.owasp, "owasp", OwaspRef::parse, errors);
    let extra = extras(&d.owasp_extra, errors);
    let before = errors.len();
    if d.cwe.is_empty() {
        errors.push(RuleError::new(
            "cwe",
            "a rule needs at least one registered CWE",
        ));
    }
    if d.owasp.is_empty() && d.owasp_extra.is_empty() {
        errors.push(RuleError::new(
            "owasp",
            "a rule needs at least one registered OWASP category",
        ));
    }
    if errors.len() != before || cwe.len() != d.cwe.len() || owasp.len() != d.owasp.len() {
        return None;
    }
    let extra: Vec<(OwaspRef, &str)> = extra.iter().map(|(c, r)| (c.clone(), r.as_str())).collect();
    match Taxonomy::checked(Registry::builtin(), cwe, owasp, &extra) {
        Ok(t) => Some(t),
        Err(e) => {
            errors.push(RuleError::new("owasp", e.to_string()));
            None
        }
    }
}

fn php_versions(
    d: &RuleDraft,
    errors: &mut Vec<RuleError>,
) -> (Option<PhpVersion>, Option<PhpVersion>) {
    let parse = |value: &Option<String>, field: &str, errors: &mut Vec<RuleError>| {
        value.as_deref().and_then(|v| {
            PhpVersion::parse(v)
                .map_err(|e| errors.push(RuleError::new(field, e)))
                .ok()
        })
    };
    let min = parse(&d.php_min, "php_min", errors);
    let max = parse(&d.php_max, "php_max", errors);
    if let (Some(lo), Some(hi)) = (min, max)
        && lo > hi
    {
        errors.push(RuleError::new(
            "php_min",
            format!("{lo} is above php_max {hi}"),
        ));
    }
    (min, max)
}

impl RuleDraft {
    /// Validates the draft. On failure returns every problem found, each naming the rule (when its
    /// identifier is readable) and the field.
    pub fn validate(&self) -> Result<Rule, Vec<RuleError>> {
        let mut errors = Vec::new();
        let id = match self.id.as_deref() {
            Some(text) => RuleId::new(text)
                .map_err(|e| errors.push(RuleError::new("id", e)))
                .ok(),
            None => {
                errors.push(RuleError::new("id", "is required"));
                None
            }
        };
        let title = required(&self.title, "title", &mut errors);
        let description = required(&self.description, "description", &mut errors);
        let remediation = required(&self.remediation, "remediation", &mut errors);
        let taxonomy = taxonomy(self, &mut errors);
        let metrics = metrics(&self.metrics, &mut errors);
        let (php_min, php_max) = php_versions(self, &mut errors);
        let body = match &self.body {
            Some(b) => validate_body(b, &mut errors),
            None => {
                errors.push(RuleError::new(
                    "body",
                    "a rule needs a taint or pattern body",
                ));
                None
            }
        };
        for (i, r) in self.references.iter().enumerate() {
            if r.trim().is_empty() {
                errors.push(RuleError::new(
                    format!("references[{i}]"),
                    "must not be empty",
                ));
            }
        }
        if let (true, Some(id)) = (!errors.is_empty(), self.id.as_deref()) {
            errors = errors.into_iter().map(|e| e.in_rule(id)).collect();
        }
        match (id, taxonomy, metrics, body) {
            (Some(id), Some(taxonomy), Some(metrics), Some(body)) if errors.is_empty() => {
                Ok(Rule {
                    id,
                    title,
                    description,
                    remediation,
                    area: self.area.clone(),
                    references: self.references.clone(),
                    taxonomy,
                    metrics,
                    paranoid: self.paranoid,
                    php_min,
                    php_max,
                    pack: self.pack.clone(),
                    body,
                    origin: None,
                })
            }
            _ => Err(errors),
        }
    }
}
