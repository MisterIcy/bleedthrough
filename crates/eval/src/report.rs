//! Reading the `json` report of a scan: schema validation and the few fields scoring needs.
//!
//! Nothing here knows the engine; the report is text, checked against the published schema.

use serde_json::Value;

/// The published report schema, embedded so validation needs no file and no network.
pub const REPORT_SCHEMA: &str = include_str!("../../../schemas/report.schema.json");

/// One finding, reduced to what matching uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The reporting rule (attribution only).
    pub rule: String,
    /// File relative to the scanned root.
    pub path: String,
    /// First line of the location.
    pub start: u32,
    /// Last line of the location.
    pub end: u32,
    /// Every CWE of the finding's taxonomy.
    pub cwes: Vec<String>,
    /// Confidence rank: 0 low, 1 medium, 2 high.
    pub tier: u8,
    /// The finding's fingerprint, used only to order findings deterministically.
    pub fingerprint: String,
}

/// The confidence rank of a tier name.
pub fn tier_rank(tier: &str) -> Option<u8> {
    crate::corpus::TIERS
        .iter()
        .position(|t| *t == tier)
        .and_then(|p| u8::try_from(p).ok())
}

/// The findings of an in-process analysis, reduced to what matching uses: the same reduction
/// [`read`] makes of a `json` report, without serialising.
pub fn from_domain(findings: &[bleedthrough_findings::Finding]) -> Vec<Finding> {
    findings
        .iter()
        .map(|f| Finding {
            rule: f.rule().as_str().to_string(),
            path: f.location().path().as_str().to_string(),
            start: f.location().start().line(),
            end: f.location().end().line(),
            cwes: f.taxonomy().cwe().iter().map(ToString::to_string).collect(),
            tier: tier_rank(f.confidence().tier().as_str()).unwrap_or_default(),
            fingerprint: f.fingerprint().as_str().to_string(),
        })
        .collect()
}

/// Validates `text` against the report schema and reads its findings.
pub fn read(text: &str) -> Result<Vec<Finding>, String> {
    let instance: Value =
        serde_json::from_str(text).map_err(|e| format!("report is not JSON: {e}"))?;
    let schema: Value = serde_json::from_str(REPORT_SCHEMA).map_err(|e| format!("schema: {e}"))?;
    let validator = jsonschema::validator_for(&schema).map_err(|e| format!("schema: {e}"))?;
    let errors: Vec<String> = validator
        .iter_errors(&instance)
        .map(|e| format!("{} at {}", e, e.instance_path))
        .take(5)
        .collect();
    if !errors.is_empty() {
        return Err(format!(
            "report does not validate against report.schema.json: {}",
            errors.join("; ")
        ));
    }
    let findings = instance
        .get("findings")
        .and_then(Value::as_array)
        .ok_or("the report is not a `json` document (it has no `findings` array)")?;
    findings.iter().map(finding).collect()
}

fn finding(value: &Value) -> Result<Finding, String> {
    let text = |v: Option<&Value>| v.and_then(Value::as_str).map(str::to_string);
    let line = |v: Option<&Value>| {
        v.and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
    };
    let location = value.get("location");
    let tier = text(value.pointer("/confidence/tier")).unwrap_or_default();
    Ok(Finding {
        rule: text(value.get("rule")).unwrap_or_default(),
        path: text(location.and_then(|l| l.get("path"))).unwrap_or_default(),
        start: line(location.and_then(|l| l.pointer("/start/line")))
            .ok_or("finding without a start line")?,
        end: line(location.and_then(|l| l.pointer("/end/line")))
            .ok_or("finding without an end line")?,
        cwes: value
            .pointer("/taxonomy/cwe")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        tier: tier_rank(&tier).ok_or_else(|| format!("unknown confidence tier `{tier}`"))?,
        fingerprint: text(value.get("fingerprint")).unwrap_or_default(),
    })
}
