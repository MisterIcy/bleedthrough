//! From the project analysis to findings: one finding per (rule, source, sink) with a trace read
//! off the label's steps, a confidence derived from the marks it collected, and a fingerprint from
//! the findings model.

use std::collections::BTreeMap;

use bleedthrough_findings::{
    Confidence, ConfidenceTier, Coverage, Finding, FindingParts, Fingerprint, FingerprintInput,
    LimitHit, LimitKind, Location, Message, Reason, ReasonCode, RelPath, RuleId, SkipReason,
    SkippedFile, Step, StepKind, Trace,
};
use bleedthrough_ir::Graph;
use bleedthrough_rules::{Rule, RuleSet};
use bleedthrough_syntax::Span;

use crate::callgraph::{CallGraph, SourceUnit, UnitId};
use crate::coverage::CallCoverage;
use crate::dataflow::{Limits, SinkHit};
use crate::label::{Origin, StepMark, Taint};
use crate::pattern::PatternHit;
use crate::project::{ProjectFlow, unit_graphs};
use crate::rule_index::RuleIndex;

/// Term: Finding
///
/// What turning a project analysis into findings reads.
#[derive(Debug, Clone, Copy)]
#[allow(clippy::exhaustive_structs, reason = "a plain record of references")]
pub struct FindingsInput<'a> {
    /// The rules the index was built from.
    pub rules: &'a RuleSet,
    /// The active rules.
    pub index: &'a RuleIndex,
    /// Where every call and include goes.
    pub graph: &'a CallGraph,
    /// The units, as given to the call graph.
    pub units: &'a [SourceUnit<'a>],
    /// The text of every file by project-relative path.
    pub texts: &'a BTreeMap<&'a str, &'a str>,
    /// The bounds the analysis ran under.
    pub limits: Limits,
}

/// Term: Finding
///
/// The findings of a project analysis, in canonical order: by path, start, rule and fingerprint.
pub fn build_findings(input: &FindingsInput<'_>, flow: &ProjectFlow) -> Vec<Finding> {
    let Some(graphs) = unit_graphs(input.graph, input.units) else {
        return Vec::new();
    };
    let builder = Builder::new(input, &graphs, flow);
    let mut candidates = builder.taint_candidates();
    candidates.extend(builder.pattern_candidates());
    builder.finish(candidates)
}

/// Term: Coverage
///
/// What the analysis could and could not see, in the form the report consumes, with the unresolved
/// calls and includes broken down by reason and every limit reached, by kind and file. Failed and
/// skipped files are the application's to add.
pub fn build_coverage(
    input: &FindingsInput<'_>,
    flow: &ProjectFlow,
    php_version: &str,
) -> (Coverage, CallCoverage) {
    let mut files: BTreeMap<&str, bool> = BTreeMap::new();
    if let Some(graphs) = unit_graphs(input.graph, input.units) {
        for (at, graph) in graphs.iter().enumerate() {
            let file = &*input.graph.unit(UnitId::from_index(at)).file;
            *files.entry(file).or_default() |= graph.recovered;
        }
    }
    let calls = input.graph.coverage().clone();
    let mut limits_hit = Vec::new();
    if let Some(graphs) = unit_graphs(input.graph, input.units) {
        for (at, graph) in graphs.iter().enumerate() {
            let path = RelPath::new(&*input.graph.unit(UnitId::from_index(at)).file).ok();
            let mut hit = |kind| limits_hit.push(LimitHit::new(kind, path.clone()));
            let lowered = graph.limits;
            if lowered.depth {
                hit(LimitKind::ExpressionDepth);
            }
            if lowered.blocks {
                hit(LimitKind::BlockLimit);
            }
            if lowered.instructions {
                hit(LimitKind::InstructionLimit);
            }
            let Some(unit) = flow.flows.get(at) else {
                continue;
            };
            for (reached, kind) in [
                (unit.limits.steps, LimitKind::StepBudget),
                (unit.limits.visits, LimitKind::BlockVisits),
                (unit.limits.origins, LimitKind::OriginCap),
                (unit.limits.iterations, LimitKind::Iterations),
                (unit.limits.depth, LimitKind::SummaryDepth),
                (unit.limits.component, LimitKind::ComponentSize),
            ] {
                if reached {
                    hit(kind);
                }
            }
        }
    }
    let mut skipped: Vec<SkippedFile> = flow
        .skipped
        .iter()
        .filter_map(|(unit, _)| {
            let path = RelPath::new(&*input.graph.unit(*unit).file).ok()?;
            Some(SkippedFile::new(path, SkipReason::InternalFault))
        })
        .collect();
    skipped.sort();
    skipped.dedup();
    if calls.truncated_calls > 0 {
        limits_hit.push(LimitHit::new(LimitKind::Candidates, None));
    }
    if !flow.settled && flow.rounds > 0 {
        limits_hit.push(LimitHit::new(LimitKind::Iterations, None));
    }
    let mut coverage = Coverage::new(php_version);
    coverage.limits_hit = limits_hit;
    coverage.skipped = skipped;
    coverage.files_recovered = files.values().filter(|r| **r).count() as u64;
    coverage.files_clean = files.len() as u64 - coverage.files_recovered;
    coverage.unresolved_calls = calls.total_unresolved_calls();
    coverage.unresolved_includes = calls.total_unresolved_includes();
    (coverage, calls)
}

type GroupKey = (u32, Origin, UnitId, Span);
type OccurrenceKey = (u32, String, Vec<String>);

/// A finding before its occurrence index is known.
struct Candidate {
    rule: u32,
    path: String,
    start: u32,
    tie: String,
    symbol: String,
    snippets: Vec<String>,
    parts: Parts,
}

struct Parts {
    message: String,
    location: Location,
    confidence: Confidence,
    trace: Option<Trace>,
}

struct Builder<'a> {
    input: &'a FindingsInput<'a>,
    flow: &'a ProjectFlow,
    recovered_files: Vec<&'a str>,
    /// Units whose lowering reached a bound, by unit identifier.
    lowering_limited: Vec<bool>,
}

impl<'a> Builder<'a> {
    fn new(input: &'a FindingsInput<'a>, graphs: &[&Graph], flow: &'a ProjectFlow) -> Self {
        let mut recovered_files: Vec<&str> = graphs
            .iter()
            .enumerate()
            .filter(|(_, g)| g.recovered)
            .map(|(at, _)| &*input.graph.unit(UnitId::from_index(at)).file)
            .collect();
        recovered_files.sort_unstable();
        recovered_files.dedup();
        Self {
            input,
            flow,
            recovered_files,
            lowering_limited: graphs.iter().map(|g| g.limits.any()).collect(),
        }
    }

    fn file(&self, unit: UnitId) -> &'a str {
        &self.input.graph.unit(unit).file
    }

    fn recovered(&self, unit: UnitId) -> bool {
        self.recovered_files.binary_search(&self.file(unit)).is_ok()
    }

    fn text(&self, unit: UnitId, span: Span) -> Option<&'a str> {
        let text = self.input.texts.get(self.file(unit))?;
        text.get(span.start as usize..span.end as usize)
    }

    fn location(&self, unit: UnitId, span: Span) -> Option<Location> {
        let file = self.file(unit);
        let text = self.input.texts.get(file)?;
        let path = RelPath::new(file).ok()?;
        Location::from_byte_range(path, text, span.start as usize, span.end as usize).ok()
    }

    fn rule(&self, at: u32) -> Option<&'a Rule> {
        self.input
            .rules
            .get(self.input.index.rules().get(at as usize)?)
    }

    // ---- taint findings ----------------------------------------------------------------

    fn taint_candidates(&self) -> Vec<Candidate> {
        let index = self.input.index;
        let mut grouped: BTreeMap<GroupKey, SinkHit> = BTreeMap::new();
        for hit in &self.flow.hits {
            let key = (
                index.sinks()[hit.sink as usize].rule,
                hit.label.origin,
                hit.unit,
                hit.span,
            );
            match grouped.get_mut(&key) {
                Some(existing) => {
                    let both = Taint::of(existing.label.clone())
                        .join(&Taint::of(hit.label.clone()), usize::MAX);
                    if let Some(label) = both.labels().first() {
                        existing.label = label.clone();
                    }
                    existing.sink = existing.sink.min(hit.sink);
                }
                None => {
                    grouped.insert(key, hit.clone());
                }
            }
        }
        grouped
            .into_iter()
            .filter_map(|((rule, _, _, _), hit)| self.taint_candidate(rule, &hit))
            .collect()
    }

    fn taint_candidate(&self, rule_at: u32, hit: &SinkHit) -> Option<Candidate> {
        let rule = self.rule(rule_at)?;
        let Origin::Source {
            source,
            unit: source_unit,
            span: source_span,
        } = hit.label.origin
        else {
            return None;
        };
        let index = self.input.index;
        let sink = &index.sinks()[hit.sink as usize];
        let source_name = index.sources()[source as usize].symbol.to_string();
        let sink_name = sink.symbol.to_string();
        let sink_location = self.location(hit.unit, hit.span)?;
        let sink_text = self.text(hit.unit, hit.span)?;
        let source_text = self.text(source_unit, source_span)?;

        let mut steps = vec![Step::new(
            self.location(source_unit, source_span)?,
            StepKind::Source,
            format!("source: {source_name}"),
        )];
        let mut wrong_sanitizer = false;
        for step in &hit.label.steps {
            let (kind, label) = match step.mark {
                StepMark::Propagation => (StepKind::Propagation, "value carried on".to_string()),
                StepMark::Call => (StepKind::Call, "passed through a call".to_string()),
                StepMark::Sanitizer(entry) => {
                    let sanitizer = &index.sanitizers()[entry as usize];
                    if sanitizer.clears.contains(sink.class) {
                        continue;
                    }
                    wrong_sanitizer = true;
                    (
                        StepKind::SanitizerConsidered,
                        format!(
                            "{} does not clear {}",
                            sanitizer.symbol,
                            sink.class.as_str()
                        ),
                    )
                }
            };
            if let Some(location) = self.location(step.unit, step.span) {
                steps.push(Step::new(location, kind, label));
            }
        }
        steps.push(Step::new(
            sink_location.clone(),
            StepKind::Sink,
            format!("sink: {sink_name}"),
        ));
        let cut = self.input.limits.trace_steps.clamp(2, 62);
        let trace = Trace::new(steps, cut).ok()?;

        let reasons = self.reasons(hit, source_unit, wrong_sanitizer);
        let file = self.file(hit.unit);
        Some(Candidate {
            rule: rule_at,
            path: file.to_string(),
            start: hit.span.start,
            tie: format!("{source_unit:?}:{}", source_span.start),
            symbol: format!("{file}::{}", self.input.graph.unit(hit.unit).key.name),
            snippets: vec![normalise(sink_text), normalise(source_text)],
            parts: Parts {
                message: format!("{}: {source_name} reaches {sink_name}", rule.title()),
                location: sink_location,
                confidence: confidence(reasons)?,
                trace: Some(trace),
            },
        })
    }

    /// Every condition the flow met that lowers confidence, each with its reason.
    fn reasons(&self, hit: &SinkHit, source_unit: UnitId, wrong_sanitizer: bool) -> Vec<Reason> {
        let unit_limits = |unit: UnitId| {
            self.lowering_limited
                .get(unit.index())
                .copied()
                .unwrap_or(false)
                || self
                    .flow
                    .flows
                    .get(unit.index())
                    .is_some_and(|f| f.limits.any())
        };
        let marks = hit.label.marks;
        let mut reasons = Vec::new();
        if marks.inferred_source {
            reasons.push(Reason::new(ReasonCode::SourceInferred));
        }
        if marks.several > 1 {
            reasons.push(Reason::with_detail(
                ReasonCode::DynamicDispatchSeveralCandidates,
                format!("{} candidates", marks.several),
            ));
        }
        if marks.assumed_unresolved {
            reasons.push(Reason::new(ReasonCode::UnresolvedCalleeAssumedPropagate));
        }
        if marks.widened || unit_limits(hit.unit) || unit_limits(source_unit) {
            reasons.push(Reason::new(ReasonCode::AnalysisWidened));
        }
        let on_path = hit
            .label
            .steps
            .iter()
            .map(|s| s.unit)
            .chain([hit.unit, source_unit]);
        if on_path.into_iter().any(|u| self.recovered(u)) {
            reasons.push(Reason::new(ReasonCode::ParseRecovered));
        }
        if marks.unknown_type {
            reasons.push(Reason::new(ReasonCode::TypeUnknown));
        }
        if wrong_sanitizer {
            reasons.push(Reason::new(ReasonCode::SanitizerWrongForSinkContext));
        }
        reasons
    }

    // ---- pattern findings --------------------------------------------------------------

    fn pattern_candidates(&self) -> Vec<Candidate> {
        let mut seen: BTreeMap<(u32, UnitId, Span), &PatternHit> = BTreeMap::new();
        for hit in &self.flow.patterns {
            let rule = self.input.index.patterns()[hit.pattern as usize].rule;
            seen.entry((rule, hit.unit, hit.span))
                .and_modify(|kept| {
                    if !hit.by_name && kept.by_name {
                        *kept = hit;
                    }
                })
                .or_insert(hit);
        }
        seen.into_iter()
            .filter_map(|((rule_at, unit, span), hit)| {
                let rule = self.rule(rule_at)?;
                let location = self.location(unit, span)?;
                let text = self.text(unit, span)?;
                let mut reasons = Vec::new();
                if hit.by_name {
                    reasons.push(Reason::new(ReasonCode::UnresolvedCalleeAssumedPropagate));
                }
                if hit.recovered || self.recovered(unit) {
                    reasons.push(Reason::new(ReasonCode::ParseRecovered));
                }
                let file = self.file(unit);
                Some(Candidate {
                    rule: rule_at,
                    path: file.to_string(),
                    start: span.start,
                    tie: String::new(),
                    symbol: format!("{file}::{}", self.input.graph.unit(unit).key.name),
                    snippets: vec![normalise(text)],
                    parts: Parts {
                        message: rule.title().to_string(),
                        location,
                        confidence: confidence(reasons)?,
                        trace: None,
                    },
                })
            })
            .collect()
    }

    // ---- assembly ----------------------------------------------------------------------

    fn finish(&self, mut candidates: Vec<Candidate>) -> Vec<Finding> {
        candidates.sort_by(|a, b| {
            (&a.path, a.start, a.rule, &a.tie).cmp(&(&b.path, b.start, b.rule, &b.tie))
        });
        let mut occurrences: BTreeMap<OccurrenceKey, u32> = BTreeMap::new();
        let mut out = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let Some(rule) = self.rule(candidate.rule) else {
                continue;
            };
            let Ok(id) = RuleId::new(rule.id().as_str()) else {
                continue;
            };
            let seen = occurrences
                .entry((
                    candidate.rule,
                    candidate.symbol.clone(),
                    candidate.snippets.clone(),
                ))
                .or_default();
            let occurrence = *seen;
            *seen += 1;
            let Ok(message) = Message::new(candidate.parts.message) else {
                continue;
            };
            let fingerprint = Fingerprint::compute(&FingerprintInput::new(
                id.clone(),
                candidate.symbol,
                candidate.snippets,
                occurrence,
            ));
            out.push(Finding::new(FindingParts {
                rule: id,
                message,
                location: candidate.parts.location,
                taxonomy: rule.taxonomy().clone(),
                severity: rule.severity(),
                confidence: candidate.parts.confidence,
                fingerprint,
                paranoid: rule.paranoid(),
                trace: candidate.parts.trace,
                fix: None,
            }));
        }
        out.sort_by(|a, b| {
            (
                a.location().path(),
                a.location().byte_start(),
                a.rule(),
                a.fingerprint(),
            )
                .cmp(&(
                    b.location().path(),
                    b.location().byte_start(),
                    b.rule(),
                    b.fingerprint(),
                ))
        });
        out
    }
}

/// The tier is the lowest any reason implies; no reasons means high.
fn confidence(reasons: Vec<Reason>) -> Option<Confidence> {
    let tier = reasons
        .iter()
        .map(|r| match r.code() {
            ReasonCode::UnresolvedCalleeAssumedPropagate
            | ReasonCode::ParseRecovered
            | ReasonCode::AnalysisWidened => ConfidenceTier::Low,
            _ => ConfidenceTier::Medium,
        })
        .min()
        .unwrap_or(ConfidenceTier::High);
    Confidence::new(tier, reasons).ok()
}

/// The text without comments and with whitespace reduced to what keeps two words apart, strings
/// left as they are: what a fingerprint is computed from, so it does not move with formatting.
fn normalise(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let mut out = String::new();
    let mut gap = false;
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        let next = chars.get(at + 1).copied();
        match c {
            '\'' | '"' | '`' => {
                out.push(c);
                at += 1;
                while at < chars.len() {
                    out.push(chars[at]);
                    if chars[at] == '\\' {
                        if let Some(escaped) = chars.get(at + 1) {
                            out.push(*escaped);
                        }
                        at += 2;
                        continue;
                    }
                    at += 1;
                    if chars[at - 1] == c {
                        break;
                    }
                }
                gap = false;
                continue;
            }
            '/' if next == Some('/') => {
                while at < chars.len() && chars[at] != '\n' {
                    at += 1;
                }
                gap = true;
                continue;
            }
            '#' if next != Some('[') => {
                while at < chars.len() && chars[at] != '\n' {
                    at += 1;
                }
                gap = true;
                continue;
            }
            '/' if next == Some('*') => {
                at += 2;
                while at < chars.len() && !(chars[at] == '*' && chars.get(at + 1) == Some(&'/')) {
                    at += 1;
                }
                at += 2;
                gap = true;
                continue;
            }
            c if c.is_whitespace() => gap = true,
            c => {
                if gap && out.chars().last().is_some_and(word) && word(c) {
                    out.push(' ');
                }
                gap = false;
                out.push(c);
            }
        }
        at += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::normalise;

    #[test]
    fn formatting_and_comments_do_not_matter_but_strings_do() {
        assert_eq!(normalise("f( $a ,\n  $b ) // c"), "f($a,$b)");
        assert_eq!(normalise("new  Foo /* x */ ($a)"), "new Foo($a)");
        assert_eq!(normalise("f('a  b', \"c#d\")"), "f('a  b',\"c#d\")");
        assert_ne!(normalise("f('a')"), normalise("f('b')"));
    }
}
