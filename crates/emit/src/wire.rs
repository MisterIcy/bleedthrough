//! Wire types of the `json` document and the `ndjson` records, decoupled from the domain model.
//!
//! Every struct serializes its fields in declaration order and holds no hash maps, so output
//! never depends on iteration order. Optional values are written as `null`, never omitted.

use bleedthrough_findings::{
    CacheStats, Coverage, Finding, Report, RunInfo, Severity, Suppressed, Trace,
};
use serde::{Serialize, Serializer};

/// Tenths of a score, written as a JSON number with one decimal place (`7.5`, `10.0`).
pub struct Score(u8);

impl Serialize for Score {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(f64::from(self.0) / 10.0)
    }
}

#[derive(Serialize)]
pub struct Tool<'a> {
    name: &'a str,
    version: &'a str,
}

#[derive(Serialize)]
pub struct Options<'a> {
    confidence_threshold: &'static str,
    paranoid: bool,
    php_version: &'a str,
    ruleset_id: &'a str,
}

#[derive(Serialize)]
struct PositionW {
    line: u32,
    column: u32,
}

#[derive(Serialize)]
struct LocationW<'a> {
    path: &'a str,
    start: PositionW,
    end: PositionW,
    byte_start: usize,
    byte_end: usize,
}

#[derive(Serialize)]
struct OwaspW<'a> {
    edition: u16,
    id: &'a str,
}

#[derive(Serialize)]
struct TaxonomyW<'a> {
    cwe: Vec<String>,
    owasp: Vec<OwaspW<'a>>,
}

#[derive(Serialize)]
struct SeverityW<'a> {
    level: &'static str,
    score: Option<Score>,
    vector: Option<&'a str>,
}

#[derive(Serialize)]
struct ReasonW<'a> {
    code: &'static str,
    detail: Option<&'a str>,
}

#[derive(Serialize)]
struct ConfidenceW<'a> {
    tier: &'static str,
    reasons: Vec<ReasonW<'a>>,
}

#[derive(Serialize)]
struct StepW<'a> {
    kind: &'static str,
    label: &'a str,
    location: LocationW<'a>,
}

#[derive(Serialize)]
struct TraceW<'a> {
    truncated: bool,
    steps: Vec<StepW<'a>>,
}

#[derive(Serialize)]
struct EditW<'a> {
    path: &'a str,
    byte_start: usize,
    byte_end: usize,
    expected: &'a str,
    replacement: &'a str,
}

#[derive(Serialize)]
struct FixW<'a> {
    description: &'a str,
    applicability: &'static str,
    edits: Vec<EditW<'a>>,
}

#[derive(Serialize)]
pub struct FindingW<'a> {
    rule: &'a str,
    message: &'a str,
    location: LocationW<'a>,
    taxonomy: TaxonomyW<'a>,
    severity: SeverityW<'a>,
    confidence: ConfidenceW<'a>,
    fingerprint: &'a str,
    paranoid: bool,
    trace: Option<TraceW<'a>>,
    fix: Option<FixW<'a>>,
}

#[derive(Serialize)]
struct SkippedW<'a> {
    path: &'a str,
    reason: &'static str,
}

#[derive(Serialize)]
struct LimitW<'a> {
    kind: &'static str,
    path: Option<&'a str>,
}

#[derive(Serialize)]
struct PackW<'a> {
    name: &'a str,
    version: &'a str,
}

#[derive(Serialize)]
struct CacheW {
    hits: u64,
    misses: u64,
    invalidations: u64,
}

#[derive(Serialize)]
pub struct CoverageW<'a> {
    files_clean: u64,
    files_recovered: u64,
    files_failed: u64,
    skipped: Vec<SkippedW<'a>>,
    limits_hit: Vec<LimitW<'a>>,
    unresolved_includes: u64,
    unresolved_calls: u64,
    packs: Vec<PackW<'a>>,
    php_version: &'a str,
    cache: Option<CacheW>,
}

#[derive(Serialize)]
pub struct SuppressedW {
    below_threshold_low: u64,
    below_threshold_medium: u64,
    baselined: u64,
}

/// The `json` document.
#[derive(Serialize)]
pub struct Document<'a> {
    schema_version: &'static str,
    tool: Tool<'a>,
    options: Options<'a>,
    findings: Vec<FindingW<'a>>,
    coverage: CoverageW<'a>,
    suppressed: SuppressedW,
}

/// The first `ndjson` record.
#[derive(Serialize)]
pub struct RunRecord<'a> {
    r#type: &'static str,
    schema_version: &'static str,
    tool: Tool<'a>,
    options: Options<'a>,
}

/// One `ndjson` finding record.
#[derive(Serialize)]
pub struct FindingRecord<'a> {
    r#type: &'static str,
    #[serde(flatten)]
    finding: FindingW<'a>,
}

/// The last `ndjson` record; its absence marks an interrupted stream.
#[derive(Serialize)]
pub struct SummaryRecord<'a> {
    r#type: &'static str,
    coverage: CoverageW<'a>,
    suppressed: SuppressedW,
}

fn tool(run: &RunInfo) -> Tool<'_> {
    Tool {
        name: run.tool_name(),
        version: run.tool_version(),
    }
}

fn options(run: &RunInfo) -> Options<'_> {
    let o = run.options();
    Options {
        confidence_threshold: o.confidence_threshold.as_str(),
        paranoid: o.paranoid,
        php_version: &o.php_version,
        ruleset_id: &o.ruleset_id,
    }
}

fn location(l: &bleedthrough_findings::Location) -> LocationW<'_> {
    let pos = |p: bleedthrough_findings::Position| PositionW {
        line: p.line(),
        column: p.column(),
    };
    LocationW {
        path: l.path().as_str(),
        start: pos(l.start()),
        end: pos(l.end()),
        byte_start: l.byte_start(),
        byte_end: l.byte_end(),
    }
}

fn severity(s: &Severity) -> SeverityW<'_> {
    SeverityW {
        level: s.level().as_str(),
        score: s.score_tenths().map(Score),
        vector: s.vector(),
    }
}

fn step(s: &bleedthrough_findings::Step) -> StepW<'_> {
    StepW {
        kind: s.kind().as_str(),
        label: s.label(),
        location: location(s.location()),
    }
}

fn trace(t: &Trace) -> TraceW<'_> {
    TraceW {
        truncated: t.truncated(),
        steps: t.steps().iter().map(step).collect(),
    }
}

fn edit(e: &bleedthrough_findings::Edit) -> EditW<'_> {
    EditW {
        path: e.path().as_str(),
        byte_start: e.start(),
        byte_end: e.end(),
        expected: e.expected(),
        replacement: e.replacement(),
    }
}

fn fix(f: &bleedthrough_findings::Fix) -> FixW<'_> {
    FixW {
        description: f.description(),
        applicability: f.applicability().as_str(),
        edits: f.edits().iter().map(edit).collect(),
    }
}

/// Maps one finding to its wire form.
pub fn finding(f: &Finding) -> FindingW<'_> {
    FindingW {
        rule: f.rule().as_str(),
        message: f.message().as_str(),
        location: location(f.location()),
        taxonomy: TaxonomyW {
            cwe: f
                .taxonomy()
                .cwe()
                .iter()
                .map(|c| format!("CWE-{}", c.number()))
                .collect(),
            owasp: f
                .taxonomy()
                .owasp()
                .iter()
                .map(|o| OwaspW {
                    edition: o.edition(),
                    id: o.id(),
                })
                .collect(),
        },
        severity: severity(f.severity()),
        confidence: ConfidenceW {
            tier: f.confidence().tier().as_str(),
            reasons: f
                .confidence()
                .reasons()
                .iter()
                .map(|r| ReasonW {
                    code: r.code().as_str(),
                    detail: r.detail(),
                })
                .collect(),
        },
        fingerprint: f.fingerprint().as_str(),
        paranoid: f.paranoid(),
        trace: f.trace().map(trace),
        fix: f.fix().map(fix),
    }
}

pub fn coverage(c: &Coverage) -> CoverageW<'_> {
    let cache = |s: &CacheStats| CacheW {
        hits: s.hits,
        misses: s.misses,
        invalidations: s.invalidations,
    };
    CoverageW {
        files_clean: c.files_clean,
        files_recovered: c.files_recovered,
        files_failed: c.files_failed,
        skipped: c
            .skipped
            .iter()
            .map(|s| SkippedW {
                path: s.path().as_str(),
                reason: s.reason().as_str(),
            })
            .collect(),
        limits_hit: c
            .limits_hit
            .iter()
            .map(|l| LimitW {
                kind: l.kind().as_str(),
                path: l.path().map(|p| p.as_str()),
            })
            .collect(),
        unresolved_includes: c.unresolved_includes,
        unresolved_calls: c.unresolved_calls,
        packs: c
            .packs
            .iter()
            .map(|p| PackW {
                name: p.name(),
                version: p.version(),
            })
            .collect(),
        php_version: &c.php_version,
        cache: c.cache.as_ref().map(cache),
    }
}

pub fn suppressed(s: &Suppressed) -> SuppressedW {
    SuppressedW {
        below_threshold_low: s.below_threshold_low,
        below_threshold_medium: s.below_threshold_medium,
        baselined: s.baselined,
    }
}

/// Maps a whole report to the `json` document.
pub fn document(r: &Report) -> Document<'_> {
    Document {
        schema_version: r.run().schema_version(),
        tool: tool(r.run()),
        options: options(r.run()),
        findings: r.findings().iter().map(finding).collect(),
        coverage: coverage(r.coverage()),
        suppressed: suppressed(r.suppressed()),
    }
}

/// The `run` record.
pub fn run_record(r: &Report) -> RunRecord<'_> {
    RunRecord {
        r#type: "run",
        schema_version: r.run().schema_version(),
        tool: tool(r.run()),
        options: options(r.run()),
    }
}

/// A `finding` record.
pub fn finding_record(f: &Finding) -> FindingRecord<'_> {
    FindingRecord {
        r#type: "finding",
        finding: finding(f),
    }
}

/// The `summary` record.
pub fn summary_record(r: &Report) -> SummaryRecord<'_> {
    SummaryRecord {
        r#type: "summary",
        coverage: coverage(r.coverage()),
        suppressed: suppressed(r.suppressed()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_score_renders_with_one_decimal_place() {
        for tenths in 0..=100u8 {
            let text = serde_json::to_string(&Score(tenths)).unwrap();
            assert_eq!(text, format!("{}.{}", tenths / 10, tenths % 10), "{tenths}");
        }
    }
}
