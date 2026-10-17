//! Whole-project taint analysis: summaries composed callees-first over the component order, a
//! fixpoint inside each recursive component, and rounds over the abstract locations.

use std::collections::{BTreeMap, BTreeSet};
use std::panic::{AssertUnwindSafe, catch_unwind};

use bleedthrough_ir::Graph;
use bleedthrough_symbols::SymbolTable;
use bleedthrough_syntax::Span;

use crate::callgraph::{CallGraph, IncludeOutcome, SourceUnit, UnitId};
use crate::dataflow::{
    Context, Heap, Inter, Limits, SinkHit, Summaries, UnitFlow, UnitInput, analyse_unit,
};
use crate::label::Taint;
use crate::pattern::{PatternHit, scan_patterns};
use crate::rule_index::RuleIndex;

/// Term: Summary
///
/// Decides which units fail in a fault-injection test.
pub type FaultHook = fn(UnitId) -> bool;

/// Term: Summary
///
/// What the project analysis reads.
#[derive(Debug, Clone, Copy)]
#[allow(clippy::exhaustive_structs, reason = "a plain record of references")]
pub struct ProjectInput<'a> {
    /// The symbols.
    pub table: &'a SymbolTable,
    /// The active taint rules.
    pub index: &'a RuleIndex,
    /// The bounds.
    pub limits: Limits,
    /// Where every call and include goes.
    pub graph: &'a CallGraph,
    /// The units, as given to the call graph.
    pub units: &'a [SourceUnit<'a>],
    /// A fault injection hook for tests: a unit it returns true for fails as if it had violated
    /// an internal invariant.
    pub fail_units: Option<FaultHook>,
}

/// Term: Summary
///
/// What the project analysis produced.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct ProjectFlow {
    /// The summary of every unit, by unit identifier.
    pub flows: Vec<UnitFlow>,
    /// The sinks concrete sources reach, de-duplicated by (sink, source, location).
    pub hits: Vec<SinkHit>,
    /// Where pattern rules matched, by unit and position.
    pub patterns: Vec<PatternHit>,
    /// The units whose analysis failed an internal invariant and was skipped, with the reason.
    pub skipped: Vec<(UnitId, Box<str>)>,
    /// The abstract locations at the end.
    pub heap: Heap,
    /// The rounds that ran.
    pub rounds: u32,
    /// Whether the locations settled within the round limit.
    pub settled: bool,
}

/// Term: Summary
///
/// The derived data every work unit reads: each unit's graph, the graphs of each file and the units
/// that are included by others. Built once from the input.
#[derive(Debug)]
pub struct ProjectView<'a> {
    input: ProjectInput<'a>,
    graphs: Vec<&'a Graph>,
    by_file: BTreeMap<&'a str, Vec<&'a Graph>>,
    included: BTreeSet<UnitId>,
}

/// Term: Summary
///
/// The summaries of one call-graph component, and the units of it that failed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct ComponentSummaries {
    /// The summary of each unit of the component, in unit order.
    pub flows: Vec<(UnitId, UnitFlow)>,
    /// The units that failed an internal invariant, with the reason.
    pub skipped: Vec<(UnitId, Box<str>)>,
}

/// Term: Finding
///
/// What scanning one unit found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct UnitResult {
    /// The unit.
    pub unit: Option<UnitId>,
    /// The sinks concrete sources reach in it.
    pub hits: Vec<SinkHit>,
    /// The pattern rules that matched in it.
    pub patterns: Vec<PatternHit>,
    /// Why the unit was skipped, if it was.
    pub skipped: Option<Box<str>>,
}

impl<'a> ProjectView<'a> {
    /// The view of `input`; `None` when a unit of the call graph was not given.
    pub fn new(input: &ProjectInput<'a>) -> Option<Self> {
        let graphs = unit_graphs(input.graph, input.units)?;
        let mut by_file: BTreeMap<&str, Vec<&Graph>> = BTreeMap::new();
        for unit in input.units {
            by_file.entry(unit.file).or_default().push(unit.graph);
        }
        for graphs in by_file.values_mut() {
            graphs.sort_by(|a, b| a.key.cmp(&b.key));
        }
        let included = input
            .graph
            .includes()
            .iter()
            .filter_map(|site| match &site.outcome {
                IncludeOutcome::Resolved(targets) => Some(targets.iter().copied()),
                _ => None,
            })
            .flatten()
            .collect();
        Some(Self {
            input: *input,
            graphs,
            by_file,
            included,
        })
    }

    fn analyse(
        &self,
        id: UnitId,
        summaries: Summaries<'_>,
        heap: &Heap,
        skipped: &mut Vec<(UnitId, Box<str>)>,
    ) -> UnitFlow {
        let input = &self.input;
        let ctx = Context {
            table: input.table,
            index: input.index,
            limits: input.limits,
            heap,
            inter: Some(Inter {
                graph: input.graph,
                graphs: &self.graphs,
                summaries,
            }),
        };
        let file = &input.graph.unit(id).file;
        let unit = UnitInput {
            id,
            graph: self.graphs[id.index()],
            file_graphs: self.by_file.get(&**file).map_or(&[][..], Vec::as_slice),
            shared_scope: self.included.contains(&id),
        };
        guarded(skipped, id, || {
            if input.fail_units.is_some_and(|fails| fails(id)) {
                panic!("injected fault");
            }
            analyse_unit(&ctx, &unit)
        })
        .unwrap_or_default()
    }

    /// The summaries of the component at `component` (an index into the call graph's components),
    /// given those of the components it calls and the locations of the previous round. A pure
    /// function of its arguments: independent components can be computed in any order or at once.
    pub fn summarise_component(
        &self,
        component: usize,
        done: &[Option<UnitFlow>],
        heap: &Heap,
    ) -> ComponentSummaries {
        let input = &self.input;
        let component = &input.graph.components()[component];
        let mut skipped: Vec<(UnitId, Box<str>)> = Vec::new();
        let mut local: BTreeMap<UnitId, UnitFlow> = BTreeMap::new();
        if !component.recursive {
            for id in &component.units {
                let flow = self.analyse(
                    *id,
                    Summaries {
                        done,
                        local: &BTreeMap::new(),
                    },
                    heap,
                    &mut skipped,
                );
                local.insert(*id, flow);
            }
            return ComponentSummaries {
                flows: local.into_iter().collect(),
                skipped,
            };
        }
        let mut stable = false;
        let oversized = component.units.len() > input.limits.component_size;
        let passes = if oversized {
            input.limits.iterations.clamp(1, 2)
        } else {
            input.limits.iterations.max(1)
        };
        for _ in 0..passes {
            let mut changed = false;
            let mut next = local.clone();
            for id in &component.units {
                let flow = self.analyse(*id, Summaries { done, local: &next }, heap, &mut skipped);
                changed |= local.get(id) != Some(&flow);
                next.insert(*id, flow);
            }
            local = next;
            if !changed {
                stable = true;
                break;
            }
        }
        for flow in local.values_mut() {
            if oversized {
                flow.limits.component = true;
                mark_widened(flow);
            }
            if !stable {
                widen(flow);
            }
        }
        skipped.sort();
        skipped.dedup();
        ComponentSummaries {
            flows: local.into_iter().collect(),
            skipped,
        }
    }

    /// What scanning the unit `id` finds, given its own summary: the concrete sink hits and the
    /// pattern matches.
    pub fn scan_unit(&self, id: UnitId, flow: &UnitFlow) -> UnitResult {
        let input = &self.input;
        let file = &input.graph.unit(id).file;
        let mut skipped = Vec::new();
        let patterns = guarded(&mut skipped, id, || {
            scan_patterns(input.index, input.table, id, file, self.graphs[id.index()])
        })
        .unwrap_or_default();
        let mut hits: Vec<SinkHit> = flow
            .hits
            .iter()
            .filter(|h| !h.label.origin.is_symbolic())
            .cloned()
            .collect();
        hits.sort_by_key(|h| (h.sink, h.label.origin, h.span));
        UnitResult {
            unit: Some(id),
            hits,
            patterns,
            skipped: skipped.into_iter().next().map(|(_, reason)| reason),
        }
    }
}

/// Merges the results of scanning units into one, whatever order they arrive in: hits are
/// de-duplicated by (sink, source, location) and everything is put in canonical order.
pub fn merge(results: impl IntoIterator<Item = UnitResult>) -> UnitResult {
    let mut hits: BTreeMap<(u32, crate::label::Origin, Span), SinkHit> = BTreeMap::new();
    let mut out = UnitResult::default();
    let mut skipped: BTreeMap<UnitId, Box<str>> = BTreeMap::new();
    for result in results {
        for hit in result.hits {
            let key = (hit.sink, hit.label.origin, hit.span);
            match hits.get_mut(&key) {
                Some(existing) => {
                    let both =
                        Taint::of(existing.label.clone()).join(&Taint::of(hit.label), usize::MAX);
                    if let Some(label) = both.labels().first() {
                        existing.label = label.clone();
                    }
                }
                None => {
                    hits.insert(key, hit);
                }
            }
        }
        out.patterns.extend(result.patterns);
        if let (Some(unit), Some(reason)) = (result.unit, result.skipped) {
            skipped.entry(unit).or_insert(reason);
        }
    }
    out.patterns
        .sort_by_key(|p| (p.unit, p.span, p.pattern, p.by_name, p.recovered));
    out.patterns.dedup();
    out.hits = hits.into_values().collect();
    out.skipped = skipped.into_iter().next().map(|(_, r)| r);
    out
}

/// Analyses every unit of the project: the components callees-first, rounds over the abstract
/// locations, then a scan of each unit. Deterministic: units, components and locations are visited
/// in a fixed order.
pub fn analyse_project(input: &ProjectInput<'_>) -> ProjectFlow {
    let Some(view) = ProjectView::new(input) else {
        return ProjectFlow::default();
    };
    let count = input.graph.units().len();
    let mut skipped: BTreeMap<UnitId, Box<str>> = BTreeMap::new();
    let mut heap = Heap::new();
    let mut flows: Vec<UnitFlow> = Vec::new();
    let mut rounds = 0;
    let mut settled = false;
    while rounds < input.limits.rounds.max(1) {
        rounds += 1;
        let mut summaries: Vec<Option<UnitFlow>> = vec![None; count];
        for at in 0..input.graph.components().len() {
            let done = view.summarise_component(at, &summaries, &heap);
            for (id, flow) in done.flows {
                summaries[id.index()] = Some(flow);
            }
            skipped.extend(done.skipped);
        }
        flows = summaries.into_iter().flatten().collect();
        let next = next_heap(&heap, &flows, input.limits.max_origins);
        if next == heap {
            settled = true;
            break;
        }
        heap = next;
    }
    let mut scanned = Vec::new();
    for (at, flow) in flows.iter().enumerate() {
        let result = view.scan_unit(UnitId::from_index(at), flow);
        if let (Some(unit), Some(reason)) = (result.unit, result.skipped.clone()) {
            skipped.entry(unit).or_insert(reason);
        }
        scanned.push(result);
    }
    let merged = merge(scanned);
    ProjectFlow {
        skipped: skipped.into_iter().collect(),
        patterns: merged.patterns,
        hits: merged.hits,
        flows,
        heap,
        rounds,
        settled,
    }
}

/// Marks a summary whose component did not settle: everything it says is less certain.
fn widen(flow: &mut UnitFlow) {
    flow.limits.iterations = true;
    mark_widened(flow);
}

/// Marks what a summary says as coming from a widened analysis.
fn mark_widened(flow: &mut UnitFlow) {
    for hit in &mut flow.hits {
        hit.label.marks.widened = true;
    }
    flow.returns = flow.returns.map_marks(|m| m.widened = true);
}

/// The locations after joining what every unit wrote into the previous ones. Only concrete origins
/// are kept: a symbolic one means nothing outside the unit it was written in.
fn next_heap(previous: &Heap, flows: &[UnitFlow], cap: usize) -> Heap {
    let mut next = previous.clone();
    for flow in flows {
        for (location, taint) in &flow.writes {
            let concrete = taint.concrete_only();
            if !concrete.is_clean() {
                next.entry(location.clone())
                    .or_insert_with(Taint::clean)
                    .join_in(&concrete, cap);
            }
        }
    }
    next
}

/// The graph of every unit, by unit identifier; `None` when a unit of the call graph was not given.
pub(crate) fn unit_graphs<'a>(
    graph: &CallGraph,
    units: &[SourceUnit<'a>],
) -> Option<Vec<&'a Graph>> {
    let mut graphs: Vec<Option<&'a Graph>> = vec![None; graph.units().len()];
    for unit in units {
        if let Some(id) = graph.find(unit.file, &unit.graph.key) {
            graphs[id.index()] = Some(unit.graph);
        }
    }
    graphs.into_iter().collect()
}

/// Runs `work` for a unit; an internal fault becomes a skip record and `None`, so the other units
/// are unaffected.
fn guarded<T>(
    skipped: &mut Vec<(UnitId, Box<str>)>,
    unit: UnitId,
    work: impl FnOnce() -> T,
) -> Option<T> {
    match catch_unwind(AssertUnwindSafe(work)) {
        Ok(done) => Some(done),
        Err(payload) => {
            let reason = payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "internal fault".to_string());
            skipped.push((unit, reason.into()));
            None
        }
    }
}
