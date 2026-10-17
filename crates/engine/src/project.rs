//! Whole-project taint analysis: summaries composed callees-first over the component order, a
//! fixpoint inside each recursive component, and rounds over the abstract locations.

use std::collections::{BTreeMap, BTreeSet};

use bleedthrough_ir::Graph;
use bleedthrough_symbols::SymbolTable;

use crate::callgraph::{CallGraph, IncludeOutcome, SourceUnit, UnitId};
use crate::dataflow::{Context, Heap, Inter, Limits, SinkHit, UnitFlow, UnitInput, analyse_unit};
use crate::label::Taint;
use crate::pattern::{PatternHit, scan_patterns};
use crate::rule_index::RuleIndex;

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
    /// The abstract locations at the end.
    pub heap: Heap,
    /// The rounds that ran.
    pub rounds: u32,
    /// Whether the locations settled within the round limit.
    pub settled: bool,
}

/// Analyses every unit of the project. Deterministic: units, components and locations are visited
/// in a fixed order.
#[allow(
    clippy::too_many_lines,
    reason = "one pass: rounds over components over units"
)]
pub fn analyse_project(input: &ProjectInput<'_>) -> ProjectFlow {
    let count = input.graph.units().len();
    let mut graphs: Vec<Option<&Graph>> = vec![None; count];
    let mut by_file: BTreeMap<&str, Vec<&Graph>> = BTreeMap::new();
    for unit in input.units {
        if let Some(id) = input.graph.find(unit.file, &unit.graph.key) {
            graphs[id.index()] = Some(unit.graph);
        }
        by_file.entry(unit.file).or_default().push(unit.graph);
    }
    let Some(graphs) = graphs.into_iter().collect::<Option<Vec<&Graph>>>() else {
        return ProjectFlow::default();
    };
    let included: BTreeSet<UnitId> = input
        .graph
        .includes()
        .iter()
        .filter_map(|site| match &site.outcome {
            IncludeOutcome::Resolved(targets) => Some(targets.iter().copied()),
            _ => None,
        })
        .flatten()
        .collect();

    let mut heap = Heap::new();
    let mut flows: Vec<UnitFlow> = Vec::new();
    let mut rounds = 0;
    let mut settled = false;
    while rounds < input.limits.rounds.max(1) {
        rounds += 1;
        let mut summaries: Vec<Option<UnitFlow>> = vec![None; count];
        let analyse = |id: UnitId, summaries: &[Option<UnitFlow>]| {
            let ctx = Context {
                table: input.table,
                index: input.index,
                limits: input.limits,
                heap: &heap,
                inter: Some(Inter {
                    graph: input.graph,
                    graphs: &graphs,
                    summaries,
                }),
            };
            let graph = graphs[id.index()];
            let file = &input.graph.unit(id).file;
            let file_graphs = by_file.get(&**file).map_or(&[][..], Vec::as_slice);
            analyse_unit(
                &ctx,
                &UnitInput {
                    id,
                    graph,
                    file_graphs,
                    shared_scope: included.contains(&id),
                },
            )
        };
        for component in input.graph.components() {
            if !component.recursive {
                for id in &component.units {
                    let flow = analyse(*id, &summaries);
                    summaries[id.index()] = Some(flow);
                }
                continue;
            }
            let mut stable = false;
            for _ in 0..input.limits.iterations.max(1) {
                let mut changed = false;
                for id in &component.units {
                    let flow = analyse(*id, &summaries);
                    changed |= summaries[id.index()].as_ref() != Some(&flow);
                    summaries[id.index()] = Some(flow);
                }
                if !changed {
                    stable = true;
                    break;
                }
            }
            if !stable {
                for id in &component.units {
                    if let Some(flow) = summaries[id.index()].as_mut() {
                        widen(flow);
                    }
                }
            }
        }
        flows = summaries.into_iter().flatten().collect();
        let next = next_heap(&heap, &flows, input.limits.max_origins);
        if next == heap {
            settled = true;
            break;
        }
        heap = next;
    }
    let mut patterns = Vec::new();
    for (at, graph) in graphs.iter().enumerate() {
        let id = UnitId::from_index(at);
        let file = &input.graph.unit(id).file;
        patterns.extend(scan_patterns(input.index, input.table, id, file, graph));
    }
    ProjectFlow {
        patterns,
        hits: concrete_hits(&flows),
        flows,
        heap,
        rounds,
        settled,
    }
}

/// Marks a summary whose component did not settle: everything it says is less certain.
fn widen(flow: &mut UnitFlow) {
    flow.limits.iterations = true;
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

/// Every hit from a concrete source, one per (sink, source, sink location).
fn concrete_hits(flows: &[UnitFlow]) -> Vec<SinkHit> {
    let mut merged: BTreeMap<(u32, crate::label::Origin, bleedthrough_syntax::Span), SinkHit> =
        BTreeMap::new();
    for flow in flows {
        for hit in flow.hits.iter().filter(|h| !h.label.origin.is_symbolic()) {
            let key = (hit.sink, hit.label.origin, hit.span);
            match merged.get_mut(&key) {
                Some(existing) => {
                    let both = Taint::of(existing.label.clone())
                        .join(&Taint::of(hit.label.clone()), usize::MAX);
                    if let Some(label) = both.labels().first() {
                        existing.label = label.clone();
                    }
                }
                None => {
                    merged.insert(key, hit.clone());
                }
            }
        }
    }
    merged.into_values().collect()
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
