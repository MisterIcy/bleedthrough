//! The `sarif` emitter: the SARIF 2.1.0 subset Bleedthrough produces.
//!
//! Built from `serde_json::Value`s. Whether a `Value` object keeps insertion order or sorts its keys
//! depends on the `preserve_order` feature, which any crate in the build may switch on (the Mago
//! parser does, through `schemars`), so the output sorts all objects itself and does not depend on it.

use std::collections::BTreeMap;
use std::io::Write;

use bleedthrough_app::{EmitContext, EmitError, EmitOutcome, Emitter, Format};
use bleedthrough_findings::{Finding, Location, Report, SeverityLevel};
use serde_json::{Value, json};

use crate::{summary, wire};

const FINGERPRINT_KEY: &str = "bleedthrough/v1";
const CWE: &str = "CWE";

fn level(l: SeverityLevel) -> &'static str {
    match l {
        SeverityLevel::Critical | SeverityLevel::High => "error",
        SeverityLevel::Medium => "warning",
        _ => "note",
    }
}

/// GitHub's `security-severity`, in tenths: the score when present, else a level default.
fn security_tenths(f: &Finding) -> u8 {
    f.severity()
        .score_tenths()
        .unwrap_or(match f.severity().level() {
            SeverityLevel::Critical => 95,
            SeverityLevel::High => 80,
            SeverityLevel::Medium => 55,
            SeverityLevel::Low => 30,
            _ => 0,
        })
}

fn owasp_name(edition: u16) -> String {
    format!("OWASP Top 10 {edition}")
}

fn physical(l: &Location) -> Value {
    json!({
        "artifactLocation": { "uri": l.path().as_str(), "uriBaseId": "%SRCROOT%" },
        "region": {
            "startLine": l.start().line(), "startColumn": l.start().column(),
            "endLine": l.end().line(), "endColumn": l.end().column(),
            "byteOffset": l.byte_start(), "byteLength": l.byte_end() - l.byte_start(),
        },
    })
}

/// Taxonomy components and, for lookup, each taxon's `(component index, taxon index)`.
struct Taxonomies {
    components: Vec<(String, Vec<String>)>,
}

impl Taxonomies {
    fn of(report: &Report) -> Self {
        let mut by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for f in report.findings() {
            let t = f.taxonomy();
            by_name
                .entry(CWE.into())
                .or_default()
                .extend(t.cwe().iter().map(|c| c.number().to_string()));
            for o in t.owasp() {
                by_name
                    .entry(owasp_name(o.edition()))
                    .or_default()
                    .push(o.id().to_string());
            }
        }
        // `CWE` sorts before `OWASP ...`, so the BTreeMap order is already canonical.
        let components = by_name
            .into_iter()
            .map(|(name, mut taxa)| {
                taxa.sort_by_key(|t| (t.len(), t.clone()));
                taxa.dedup();
                (name, taxa)
            })
            .collect();
        Self { components }
    }

    fn reference(&self, component: &str, taxon: &str) -> Value {
        let ci = self
            .components
            .iter()
            .position(|(n, _)| n == component)
            .expect("collected from findings");
        let ti = self.components[ci]
            .1
            .iter()
            .position(|t| t == taxon)
            .expect("collected from findings");
        json!({ "kinds": ["superset"], "target": { "id": taxon, "index": ti, "toolComponent": { "name": component, "index": ci } } })
    }

    fn to_json(&self) -> Vec<Value> {
        self.components
            .iter()
            .map(|(name, taxa)| json!({ "name": name, "taxa": taxa.iter().map(|id| json!({ "id": id })).collect::<Vec<_>>() }))
            .collect()
    }
}

fn rule_object(id: &str, findings: &[&Finding], taxonomies: &Taxonomies) -> Value {
    let first = findings[0];
    let tenths = findings
        .iter()
        .map(|f| security_tenths(f))
        .max()
        .unwrap_or(0);
    let t = first.taxonomy();
    let mut relationships: Vec<Value> = t
        .cwe()
        .iter()
        .map(|c| taxonomies.reference(CWE, &c.number().to_string()))
        .collect();
    relationships.extend(
        t.owasp()
            .iter()
            .map(|o| taxonomies.reference(&owasp_name(o.edition()), o.id())),
    );
    let mut tags: Vec<String> = t
        .cwe()
        .iter()
        .map(|c| format!("external/cwe/cwe-{}", c.number()))
        .collect();
    tags.push("security".into());
    json!({
        "id": id,
        "shortDescription": { "text": id },
        "relationships": relationships,
        "properties": { "security-severity": format!("{}.{}", tenths / 10, tenths % 10), "tags": tags },
    })
}

fn code_flow(f: &Finding) -> Option<Value> {
    let trace = f.trace()?;
    let steps: Vec<Value> = trace
        .steps()
        .iter()
        .map(|s| json!({ "kinds": [s.kind().as_str()], "location": { "physicalLocation": physical(s.location()), "message": { "text": s.label() } } }))
        .collect();
    Some(
        json!({ "threadFlows": [{ "locations": steps }], "properties": { "truncated": trace.truncated() } }),
    )
}

fn fix_object(f: &Finding) -> Option<Value> {
    let fix = f.fix()?;
    let mut by_file: BTreeMap<&str, Vec<Value>> = BTreeMap::new();
    for e in fix.edits() {
        by_file.entry(e.path().as_str()).or_default().push(json!({
            "deletedRegion": { "byteOffset": e.start(), "byteLength": e.end() - e.start() },
            "insertedContent": { "text": e.replacement() },
        }));
    }
    let changes: Vec<Value> = by_file
        .into_iter()
        .map(|(uri, replacements)| json!({ "artifactLocation": { "uri": uri, "uriBaseId": "%SRCROOT%" }, "replacements": replacements }))
        .collect();
    Some(
        json!({ "description": { "text": fix.description() }, "artifactChanges": changes, "properties": { "applicability": fix.applicability().as_str() } }),
    )
}

fn result(f: &Finding, rule_index: usize) -> Value {
    let reasons: Vec<Value> = f
        .confidence()
        .reasons()
        .iter()
        .map(|r| json!({ "code": r.code().as_str(), "detail": r.detail() }))
        .collect();
    let mut properties = json!({
        "confidence": f.confidence().tier().as_str(),
        "confidenceReasons": reasons,
        "paranoid": f.paranoid(),
        "severity": f.severity().level().as_str(),
        "securitySeverity": f.severity().score_text(),
        "severityVector": f.severity().vector(),
    });
    if let (Some(map), None) = (properties.as_object_mut(), f.severity().score_tenths()) {
        map.remove("securitySeverity");
    }
    let mut result = json!({
        "ruleId": f.rule().as_str(),
        "ruleIndex": rule_index,
        "level": level(f.severity().level()),
        "message": { "text": f.message().as_str() },
        "locations": [{ "physicalLocation": physical(f.location()) }],
        "partialFingerprints": { FINGERPRINT_KEY: f.fingerprint().as_str() },
        "properties": properties,
    });
    let map = result.as_object_mut().expect("object literal");
    if let Some(flow) = code_flow(f) {
        map.insert("codeFlows".into(), json!([flow]));
    }
    if let Some(fix) = fix_object(f) {
        map.insert("fixes".into(), json!([fix]));
    }
    result
}

fn notification(level: &str, text: String, path: Option<&str>) -> Value {
    let mut n = json!({ "level": level, "message": { "text": text } });
    if let Some(uri) = path {
        n["locations"] = json!([{ "physicalLocation": { "artifactLocation": { "uri": uri, "uriBaseId": "%SRCROOT%" } } }]);
    }
    n
}

fn notifications(report: &Report) -> Vec<Value> {
    let c = report.coverage();
    let mut out = Vec::new();
    if c.files_failed > 0 {
        out.push(notification(
            "error",
            format!(
                "{} file(s) failed to parse and were not analysed",
                c.files_failed
            ),
            None,
        ));
    }
    for s in &c.skipped {
        out.push(notification(
            "warning",
            format!("file skipped: {}", s.reason().as_str()),
            Some(s.path().as_str()),
        ));
    }
    for l in &c.limits_hit {
        out.push(notification(
            "warning",
            format!("analysis limit hit: {}", l.kind().as_str()),
            l.path().map(|p| p.as_str()),
        ));
    }
    if c.files_recovered > 0 {
        out.push(notification(
            "note",
            format!("{} file(s) parsed with error recovery", c.files_recovered),
            None,
        ));
    }
    if c.unresolved_includes + c.unresolved_calls > 0 {
        let text = format!(
            "{} unresolved dynamic include(s), {} unresolved dynamic call(s)",
            c.unresolved_includes, c.unresolved_calls
        );
        out.push(notification("note", text, None));
    }
    out
}

pub fn log(report: &Report) -> Value {
    let taxonomies = Taxonomies::of(report);
    let mut by_rule: BTreeMap<&str, Vec<&Finding>> = BTreeMap::new();
    for f in report.findings() {
        by_rule.entry(f.rule().as_str()).or_default().push(f);
    }
    let rules: Vec<Value> = by_rule
        .iter()
        .map(|(id, fs)| rule_object(id, fs, &taxonomies))
        .collect();
    let index_of = |id: &str| {
        by_rule
            .keys()
            .position(|k| *k == id)
            .expect("rule declared")
    };
    let results: Vec<Value> = report
        .findings()
        .iter()
        .map(|f| result(f, index_of(f.rule().as_str())))
        .collect();
    let run = report.run();
    json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": { "driver": { "name": run.tool_name(), "version": run.tool_version(), "rules": rules } },
            "taxonomies": taxonomies.to_json(),
            "results": results,
            "invocations": [{
                "executionSuccessful": summary::analysis_complete(report),
                "toolExecutionNotifications": notifications(report),
                "properties": {
                    "schemaVersion": run.schema_version(),
                    "coverage": serde_json::to_value(wire::coverage(report.coverage())).expect("plain data"),
                    "suppressed": serde_json::to_value(wire::suppressed(report.suppressed())).expect("plain data"),
                },
            }],
        }],
    })
}

pub struct SarifEmitter;

impl Emitter for SarifEmitter {
    fn format(&self) -> Format {
        Format::Sarif
    }

    fn emit(
        &self,
        report: &Report,
        _: &EmitContext<'_>,
        out: &mut dyn Write,
    ) -> Result<EmitOutcome, EmitError> {
        let mut document = log(report);
        document.sort_all_objects();
        let mut bytes =
            serde_json::to_vec_pretty(&document).map_err(|e| EmitError::Io(e.into()))?;
        bytes.push(b'\n');
        out.write_all(&bytes)?;
        out.flush()?;
        Ok(EmitOutcome::new())
    }
}
