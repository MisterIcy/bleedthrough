//! Flow-sensitive taint analysis of one unit: a forward worklist over its blocks with strong
//! updates on locals, union at joins, and flow-insensitive abstract locations for properties,
//! static properties and globals.

use std::collections::{BTreeMap, BTreeSet};

use bleedthrough_ir::{
    CallArg, Callee, ClassOperand, Graph, Instr, InstrKind, InterpPart, MemberRef, Place,
    Resolution, Terminator, TypeClass, ValueId,
};
use bleedthrough_rules::{ArgSel, SinkClass};
use bleedthrough_symbols::{Fqn, ParamDecl, SymbolTable};
use bleedthrough_syntax::{BinaryOp, CastKind, Span};

use crate::callgraph::{CallGraph, CallOutcome, IncludeOutcome, UnitId};
use crate::calls::{
    CallInfo, candidate_count, merge_matches, multi_target, param_decls, selected_args, subjects,
};
use crate::label::{Applied, FlowStep, HeapKey, Label, Origin, StepMark, Taint};
use crate::rule_index::{Matches, RuleIndex, Subject};

/// Term: Analysis limit
///
/// The bounds of the per-unit analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record of limits")]
pub struct Limits {
    /// Most origins one value keeps.
    pub max_origins: usize,
    /// Most times one block is processed.
    pub block_visits: u32,
    /// Most instruction transfers per unit.
    pub step_budget: u64,
    /// Deepest closure-in-closure analysis.
    pub closure_depth: u32,
    /// Most passes over a recursive component before its summaries are widened.
    pub iterations: u32,
    /// Most rounds of the property, static property and global locations.
    pub rounds: u32,
    /// Most steps a finding's trace keeps (at least 2, at most 62); longer ones are cut in the
    /// middle and marked.
    pub trace_steps: usize,
    /// Most units of a recursive component iterated to the full iteration limit; a larger one
    /// iterates twice.
    pub component_size: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_origins: 16,
            block_visits: 32,
            step_budget: 2_000_000,
            closure_depth: 4,
            iterations: 8,
            rounds: 4,
            trace_steps: 24,
            component_size: 512,
        }
    }
}

/// Term: Abstract location
///
/// The taint of every tracked property, static property and global, as of the previous round.
pub type Heap = BTreeMap<HeapKey, Taint>;

/// Term: Finding
///
/// Everything the analysis of units reads besides the unit itself.
#[derive(Debug, Clone, Copy)]
#[allow(clippy::exhaustive_structs, reason = "a plain record of references")]
pub struct Context<'a> {
    /// The symbols.
    pub table: &'a SymbolTable,
    /// The active taint rules.
    pub index: &'a RuleIndex,
    /// The bounds.
    pub limits: Limits,
    /// The abstract locations as of the previous round.
    pub heap: &'a Heap,
    /// The call graph and the summaries computed so far, when calls are to be followed.
    pub inter: Option<Inter<'a>>,
}

/// Term: Summary
///
/// What a call or include needs to apply the summary of its target.
#[derive(Debug, Clone, Copy)]
#[allow(clippy::exhaustive_structs, reason = "a plain record of references")]
pub struct Inter<'a> {
    /// Where every call and include goes.
    pub graph: &'a CallGraph,
    /// The graph of every unit, by unit identifier.
    pub graphs: &'a [&'a Graph],
    /// The summaries computed so far.
    pub summaries: Summaries<'a>,
}

/// Term: Summary
///
/// The summaries a unit can use: those of components already done, and those of the component being
/// computed, which take precedence.
#[derive(Debug, Clone, Copy)]
#[allow(clippy::exhaustive_structs, reason = "a plain record of references")]
pub struct Summaries<'a> {
    /// The summary of every finished unit, by unit identifier.
    pub done: &'a [Option<UnitFlow>],
    /// The summaries of the component being computed, as of the previous pass.
    pub local: &'a BTreeMap<UnitId, UnitFlow>,
}

impl<'a> Summaries<'a> {
    /// The summary of `unit`, if it has one yet.
    pub fn get(&self, unit: UnitId) -> Option<&'a UnitFlow> {
        self.local
            .get(&unit)
            .or_else(|| self.done.get(unit.index()).and_then(Option::as_ref))
    }
}

/// Term: Finding
///
/// The unit to analyse, with the other graphs of its file (closures are analysed where they are
/// created).
#[derive(Debug, Clone, Copy)]
#[allow(clippy::exhaustive_structs, reason = "a plain record of references")]
pub struct UnitInput<'a> {
    /// The unit's identity in the call graph.
    pub id: UnitId,
    /// The graph.
    pub graph: &'a Graph,
    /// Every graph of the file, this one included.
    pub file_graphs: &'a [&'a Graph],
    /// Whether the unit is included by others: its locals are then symbolic inputs, and the state
    /// they end in is part of its summary.
    pub shared_scope: bool,
}

/// Term: Finding
///
/// A tainted value reaching a vulnerable argument of a sink: the raw material of a finding, or,
/// when the origin is symbolic, of a summary.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct SinkHit {
    /// The sink entry in the rule index.
    pub sink: u32,
    /// The taint that arrived, as it was when it did.
    pub label: Label,
    /// The unit that contains the sink call.
    pub unit: UnitId,
    /// The sink call.
    pub span: Span,
    /// The vulnerable argument the taint arrived in.
    pub arg: Span,
}

/// Term: Analysis limit
///
/// Which bounds a unit's analysis reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record of flags")]
pub struct FlowLimits {
    /// The step budget ran out.
    pub steps: bool,
    /// A block hit its visit cap.
    pub visits: bool,
    /// A value had more origins than the cap.
    pub origins: bool,
    /// A recursive component did not settle within the iteration limit.
    pub iterations: bool,
    /// Closures nested deeper than the closure depth were not followed.
    pub depth: bool,
    /// The unit is in a component larger than the component limit, which iterated fewer times.
    pub component: bool,
}

impl FlowLimits {
    /// Whether any bound was reached.
    pub fn any(&self) -> bool {
        self.steps || self.visits || self.origins || self.iterations || self.depth || self.component
    }
}

/// Term: Finding
///
/// What analysing one unit found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct UnitFlow {
    /// Sinks reached, in (sink, origin, span) order.
    pub hits: Vec<SinkHit>,
    /// The taint of what the unit returns (and yields).
    pub returns: Taint,
    /// The unit's writes to properties, static properties and globals.
    pub writes: Heap,
    /// The taint each by-reference parameter ends with, by position.
    pub params: Vec<(u32, Taint)>,
    /// For an included unit, the taint each variable ends with, by name.
    pub scope: BTreeMap<Box<str>, Taint>,
    /// Bounds reached.
    pub limits: FlowLimits,
}

type Locals = BTreeMap<u32, Taint>;
type HitKey = (u32, Origin, Span);

/// Analyses `unit` with its parameters, receiver and captures as symbolic inputs.
pub fn analyse_unit(ctx: &Context<'_>, unit: &UnitInput<'_>) -> UnitFlow {
    let graph = unit.graph;
    let mut seed = Locals::new();
    for (position, param) in graph.params.iter().enumerate() {
        let label = Label::new(Origin::Param(position as u32));
        seed.insert(param.local.raw(), Taint::of(label));
    }
    for (position, capture) in graph.captures.iter().enumerate() {
        let label = Label::new(Origin::Capture(position as u32));
        seed.insert(capture.local.raw(), Taint::of(label));
    }
    if !graph.is_static
        && let Some(at) = graph.locals.iter().position(|n| &**n == "this")
    {
        seed.insert(at as u32, Taint::of(Label::new(Origin::Receiver)));
    }
    if unit.shared_scope {
        for (at, name) in graph.locals.iter().enumerate() {
            if !name.starts_with('_') && &**name != "this" {
                seed.insert(at as u32, Taint::of(Label::new(Origin::Param(at as u32))));
            }
        }
    }
    let mut run = Run::new(ctx, unit.id, unit.id, graph, unit.file_graphs, 0);
    run.shared_scope = unit.shared_scope;
    run.run(seed);
    run.finish()
}

struct Run<'a, 'c> {
    ctx: &'c Context<'a>,
    unit: UnitId,
    graph: &'a Graph,
    file_graphs: &'a [&'a Graph],
    depth: u32,
    /// The unit whose call and include sites this graph's positions refer to.
    site_unit: UnitId,
    shared_scope: bool,
    /// The block and position of the instruction being transferred.
    at: (u32, u32),
    /// Variables an include introduced that the graph itself never names.
    extras: Vec<Box<str>>,
    exit: Locals,
    defs: Vec<Option<&'a Instr>>,
    values: Vec<Taint>,
    hits: BTreeMap<HitKey, SinkHit>,
    returns: Taint,
    writes: Heap,
    limits: FlowLimits,
    steps: u64,
}

impl<'a, 'c> Run<'a, 'c> {
    #[allow(
        clippy::too_many_arguments,
        reason = "the unit, where its sites are, and its graphs"
    )]
    fn new(
        ctx: &'c Context<'a>,
        unit: UnitId,
        site_unit: UnitId,
        graph: &'a Graph,
        file_graphs: &'a [&'a Graph],
        depth: u32,
    ) -> Self {
        let mut defs = vec![None; graph.value_count as usize];
        for block in &graph.blocks {
            for instr in &block.instrs {
                if let Some(dest) = instr.dest
                    && let Some(slot) = defs.get_mut(dest.index())
                {
                    *slot = Some(instr);
                }
            }
        }
        Self {
            ctx,
            unit,
            graph,
            file_graphs,
            depth,
            site_unit,
            shared_scope: false,
            at: (0, 0),
            extras: Vec::new(),
            exit: Locals::new(),
            defs,
            values: vec![Taint::clean(); graph.value_count as usize],
            hits: BTreeMap::new(),
            returns: Taint::clean(),
            writes: Heap::new(),
            limits: FlowLimits::default(),
            steps: 0,
        }
    }

    fn cap(&self) -> usize {
        self.ctx.limits.max_origins
    }

    fn finish(mut self) -> UnitFlow {
        let widened = self.limits.any();
        let mut hits: Vec<SinkHit> = std::mem::take(&mut self.hits).into_values().collect();
        if widened {
            for hit in &mut hits {
                hit.label.marks.widened = true;
            }
        }
        let params = self
            .graph
            .params
            .iter()
            .enumerate()
            .filter(|(_, p)| p.by_ref)
            .map(|(at, p)| {
                let taint = self.exit.get(&p.local.raw()).cloned().unwrap_or_default();
                (at as u32, taint)
            })
            .collect();
        let mut scope = BTreeMap::new();
        if self.shared_scope {
            for (id, taint) in &self.exit {
                if let Some(name) = self.name_of(*id) {
                    scope.insert(name.into(), taint.clone());
                }
            }
        }
        UnitFlow {
            hits,
            returns: self.returns,
            writes: self.writes,
            params,
            scope,
            limits: self.limits,
        }
    }

    fn name_of(&self, local: u32) -> Option<&str> {
        let at = local as usize;
        match self.graph.locals.get(at) {
            Some(name) => Some(name),
            None => self.extras.get(at - self.graph.locals.len()).map(|n| &**n),
        }
    }

    /// The variable called `name`, introduced if the graph never names it.
    fn local_named(&mut self, name: &str) -> u32 {
        if let Some(at) = self.graph.locals.iter().position(|n| &**n == name) {
            return at as u32;
        }
        let at = match self.extras.iter().position(|n| &**n == name) {
            Some(at) => at,
            None => {
                self.extras.push(name.into());
                self.extras.len() - 1
            }
        };
        (self.graph.locals.len() + at) as u32
    }

    // ---- the worklist ------------------------------------------------------------------

    fn run(&mut self, seed: Locals) {
        let order = self.graph.reverse_post_order();
        let count = self.graph.blocks.len();
        let mut position = vec![usize::MAX; count];
        for (at, block) in order.iter().enumerate() {
            position[block.index()] = at;
        }
        let mut states: Vec<Option<Locals>> = vec![None; count];
        let mut visits = vec![0u32; count];
        states[self.graph.entry.index()] = Some(seed);
        let mut work: BTreeSet<usize> = BTreeSet::new();
        work.insert(0);
        while let Some(at) = work.pop_first() {
            let block_id = order[at];
            let b = block_id.index();
            if visits[b] >= self.ctx.limits.block_visits {
                self.limits.visits = true;
                continue;
            }
            visits[b] += 1;
            let Some(entry_state) = states[b].clone() else {
                continue;
            };
            let mut state = entry_state.clone();
            let block = self.graph.block(block_id);
            for (position, instr) in block.instrs.iter().enumerate() {
                self.at = (block_id.index() as u32, position as u32);
                self.steps += 1;
                if self.steps > self.ctx.limits.step_budget {
                    self.limits.steps = true;
                    return;
                }
                self.instr(instr, &mut state);
            }
            if let Terminator::Return(Some(v)) = &block.term {
                let t = self.values[v.index()].clone();
                self.join_returns(&t);
            }
            for successor in self.graph.successors(block_id) {
                let s = successor.index();
                if self.merge_into(&mut states[s], &state) {
                    work.insert(position[s]);
                }
            }
            for handler in &block.handlers {
                let h = handler.index();
                let mut changed = self.merge_into(&mut states[h], &entry_state);
                changed |= self.merge_into(&mut states[h], &state);
                if changed {
                    work.insert(position[h]);
                }
            }
        }
        if let Some(state) = states[self.graph.exit.index()].take() {
            self.exit = state;
        }
    }

    fn merge_into(&mut self, slot: &mut Option<Locals>, from: &Locals) -> bool {
        let cap = self.cap();
        let Some(into) = slot else {
            *slot = Some(from.clone());
            return true;
        };
        let mut changed = false;
        for (local, taint) in from {
            match into.get_mut(local) {
                Some(existing) => changed |= existing.join_in(taint, cap),
                None => {
                    into.insert(*local, taint.clone());
                    changed = true;
                }
            }
        }
        changed
    }

    fn join_returns(&mut self, taint: &Taint) {
        let cap = self.cap();
        self.returns.join_in(taint, cap);
    }

    // ---- instructions ------------------------------------------------------------------

    fn tv(&self, v: ValueId) -> Taint {
        self.values.get(v.index()).cloned().unwrap_or_default()
    }

    fn join_values(&self, values: impl IntoIterator<Item = ValueId>) -> Taint {
        let cap = self.cap();
        let mut out = Taint::clean();
        for v in values {
            out.join_in(&self.values[v.index()], cap);
        }
        out
    }

    fn instr(&mut self, ins: &'a Instr, st: &mut Locals) {
        let result = self.transfer(ins, st);
        if let Some(dest) = ins.dest {
            let pruned = match ins.ty {
                TypeClass::Inert => Taint::clean(),
                // Only a computed result is a decision a known type could have changed: reading a
                // source or a variable forwards what is already there.
                TypeClass::Unknown
                    if matches!(
                        ins.kind,
                        InstrKind::Call { .. } | InstrKind::New { .. } | InstrKind::Unknown { .. }
                    ) =>
                {
                    result.map_marks(|m| m.unknown_type = true)
                }
                _ => result,
            };
            if pruned.widened() {
                self.limits.origins = true;
            }
            self.values[dest.index()] = pruned;
        }
    }

    #[allow(clippy::too_many_lines, reason = "one arm per instruction kind")]
    fn transfer(&mut self, ins: &'a Instr, st: &mut Locals) -> Taint {
        match &ins.kind {
            InstrKind::Const(_)
            | InstrKind::Magic(_)
            | InstrKind::ConstFetch(_)
            | InstrKind::ClassConst { .. }
            | InstrKind::CallableRef { .. }
            | InstrKind::Eval(_)
            | InstrKind::Print(_)
            | InstrKind::Test { .. }
            | InstrKind::Unary { .. } => Taint::clean(),
            InstrKind::Echo(_) | InstrKind::Exit(_) => Taint::clean(),
            InstrKind::Include { .. } => self.include(ins, st),
            InstrKind::Superglobal(name) => self.superglobal(name, ins.span),
            InstrKind::Load(place) => self.load(place, st),
            InstrKind::LoadElem { base, .. } => self.tv(*base),
            InstrKind::Store { place, value, weak } => {
                let t = self.tv(*value).stepped(self.prop(ins.span));
                self.store(place, t, *weak, st);
                Taint::clean()
            }
            InstrKind::Concat(parts) => self
                .join_values(parts.iter().map(|p| p.value))
                .stepped(self.prop(ins.span)),
            InstrKind::Interp { parts, .. } => self
                .join_values(parts.iter().filter_map(|p| match p {
                    InterpPart::Value(o) => Some(o.value),
                    _ => None,
                }))
                .stepped(self.prop(ins.span)),
            InstrKind::Binary {
                op: BinaryOp::Add,
                lhs,
                rhs,
            } => self.join_values([*lhs, *rhs]).stepped(self.prop(ins.span)),
            InstrKind::Binary { .. } => Taint::clean(),
            InstrKind::Cast { kind, operand } => match kind {
                CastKind::Int
                | CastKind::Float
                | CastKind::Bool
                | CastKind::Unset
                | CastKind::Void => Taint::clean(),
                _ => self.tv(*operand).stepped(self.prop(ins.span)),
            },
            InstrKind::Copy(v) | InstrKind::YieldFrom(v) => {
                let t = self.tv(*v);
                if matches!(ins.kind, InstrKind::YieldFrom(_)) {
                    self.join_returns(&t);
                }
                t
            }
            InstrKind::Array(elems) => self.join_values(
                elems
                    .iter()
                    .flat_map(|e| e.key.into_iter().chain([e.value])),
            ),
            InstrKind::Yield { key, value } => {
                let t = self.join_values(key.iter().chain(value.iter()).copied());
                self.join_returns(&t);
                Taint::clean()
            }
            InstrKind::Iter { subject, .. } => self.tv(*subject),
            InstrKind::Unknown { operands, .. } => self.join_values(operands.iter().copied()),
            InstrKind::Closure { unit, captures } => self.closure(unit, captures, st),
            InstrKind::Call {
                callee,
                args,
                resolution,
            } => self.call(ins, Some(callee), None, args, resolution, st),
            InstrKind::New {
                class,
                args,
                resolution,
            } => self.call(ins, None, Some(class), args, resolution, st),
            _ => Taint::clean(),
        }
    }

    // ---- reads and writes --------------------------------------------------------------

    fn superglobal(&mut self, name: &str, span: Span) -> Taint {
        let written = format!("${name}");
        let matches = self
            .ctx
            .index
            .lookup(self.ctx.table, Subject::Superglobal(&written));
        self.source_taint(&matches, span)
    }

    fn source_taint(&self, matches: &Matches, span: Span) -> Taint {
        let cap = self.cap();
        let mut out = Taint::clean();
        for source in &matches.sources {
            let origin = Origin::Source {
                source: *source,
                unit: self.unit,
                span,
            };
            out.join_in(&Taint::of(Label::new(origin)), cap);
        }
        out
    }

    fn load(&self, place: &Place, st: &Locals) -> Taint {
        let cap = self.cap();
        match place {
            Place::Local(l) => st.get(&l.raw()).cloned().unwrap_or_default(),
            _ => {
                let mut out = Taint::clean();
                for key in self.heap_keys(place) {
                    for t in read_location(self.ctx.heap, &key) {
                        let marked = t.map_marks(|m| m.inferred_source = true);
                        out.join_in(&marked, cap);
                    }
                    for t in read_location(&self.writes, &key) {
                        out.join_in(t, cap);
                    }
                }
                if let Place::Prop { object, owners, .. } = place
                    && owners.is_empty()
                {
                    out.join_in(&self.values[object.index()], cap);
                }
                out
            }
        }
    }

    fn store(&mut self, place: &Place, taint: Taint, weak: bool, st: &mut Locals) {
        let cap = self.cap();
        match place {
            Place::Local(l) => {
                let key = l.raw();
                if weak {
                    if !taint.is_clean() {
                        st.entry(key).or_default().join_in(&taint, cap);
                    }
                } else if taint.is_clean() {
                    st.remove(&key);
                } else {
                    st.insert(key, taint);
                }
            }
            _ => {
                if taint.is_clean() {
                    return;
                }
                for key in self.heap_keys(place) {
                    self.writes.entry(key).or_default().join_in(&taint, cap);
                }
            }
        }
    }

    /// The abstract locations a place denotes: one per possible declaring class, or the unknown
    /// class when the receiver's type is not known. A read with an unknown class also reads every
    /// class's property of that name.
    fn heap_keys(&self, place: &Place) -> Vec<HeapKey> {
        match place {
            Place::Local(_) => Vec::new(),
            Place::Global(name) => vec![HeapKey::Global(name.clone())],
            Place::Prop { name, owners, .. } => {
                let MemberRef::Named(name) = name else {
                    return Vec::new();
                };
                self.declaring_keys(name, owners, false)
            }
            Place::StaticProp { class, name } => {
                let MemberRef::Named(name) = name else {
                    return Vec::new();
                };
                self.declaring_keys(name, &class.classes, true)
            }
            _ => Vec::new(),
        }
    }

    fn declaring_keys(&self, name: &str, classes: &[Fqn], is_static: bool) -> Vec<HeapKey> {
        let make = |class: Option<&Fqn>| {
            if is_static {
                HeapKey::static_prop(name, class)
            } else {
                HeapKey::prop(name, class)
            }
        };
        if classes.is_empty() {
            return vec![make(None)];
        }
        let mut keys: Vec<HeapKey> = classes
            .iter()
            .map(|class| {
                let found = self.ctx.table.property(class, name, None);
                let declaring = found.candidates.first().map(|c| &c.owner.name);
                make(Some(declaring.unwrap_or(class)))
            })
            .collect();
        keys.sort();
        keys.dedup();
        keys
    }

    // ---- closures ----------------------------------------------------------------------

    fn closure(
        &mut self,
        key: &bleedthrough_ir::UnitKey,
        captures: &[bleedthrough_ir::CaptureBinding],
        st: &Locals,
    ) -> Taint {
        let cap = self.cap();
        let mut value = Taint::clean();
        let mut seed = Locals::new();
        for binding in captures {
            let t = st.get(&binding.outer.raw()).cloned().unwrap_or_default();
            if !binding.by_ref {
                value.join_in(&t, cap);
            }
            if !t.is_clean() {
                seed.insert(binding.inner.raw(), t);
            }
        }
        if self.depth >= self.ctx.limits.closure_depth {
            self.limits.depth = true;
            return value;
        }
        let Some(body) = self.file_graphs.iter().copied().find(|g| g.key == *key) else {
            return value;
        };
        let site_unit = self.ctx.inter.and_then(|inter| {
            let file = &inter.graph.unit(self.site_unit).file;
            inter.graph.find(file, key)
        });
        let mut inner = Run::new(
            self.ctx,
            self.unit,
            site_unit.unwrap_or(self.site_unit),
            body,
            self.file_graphs,
            self.depth + 1,
        );
        inner.run(seed);
        let flow = inner.finish();
        for hit in flow.hits {
            self.record(hit);
        }
        for (location, taint) in flow.writes {
            self.writes
                .entry(location)
                .or_default()
                .join_in(&taint, cap);
        }
        value.join_in(&flow.returns, cap);
        self.limits.steps |= flow.limits.steps;
        self.limits.visits |= flow.limits.visits;
        self.limits.origins |= flow.limits.origins;
        self.limits.depth |= flow.limits.depth;
        value
    }

    /// A propagation step at `span` in the unit being analysed.
    fn prop(&self, span: Span) -> FlowStep {
        FlowStep {
            unit: self.unit,
            span,
            mark: StepMark::Propagation,
        }
    }

    /// The step of crossing the call at `span` from the unit being analysed.
    fn call_step(&self, span: Span) -> FlowStep {
        FlowStep {
            unit: self.unit,
            span,
            mark: StepMark::Call,
        }
    }

    fn record(&mut self, hit: SinkHit) {
        let key = (hit.sink, hit.label.origin, hit.span);
        match self.hits.get_mut(&key) {
            Some(existing) => {
                let merged = Taint::of(existing.label.clone())
                    .join(&Taint::of(hit.label.clone()), usize::MAX);
                if let Some(label) = merged.labels().first() {
                    existing.label = label.clone();
                }
            }
            None => {
                self.hits.insert(key, hit);
            }
        }
    }

    // ---- calls -------------------------------------------------------------------------

    #[allow(
        clippy::too_many_arguments,
        reason = "the call's pieces, read together"
    )]
    #[allow(
        clippy::cognitive_complexity,
        reason = "the call policy, in the order it applies"
    )]
    fn call(
        &mut self,
        ins: &'a Instr,
        callee: Option<&'a Callee>,
        new_class: Option<&'a ClassOperand>,
        args: &'a [CallArg],
        resolution: &'a Resolution,
        st: &mut Locals,
    ) -> Taint {
        let cap = self.cap();
        let info = CallInfo::of(resolution);
        let receiver = match callee {
            Some(Callee::Method { receiver, .. }) => Some(*receiver),
            _ => None,
        };
        let subjects = subjects(callee, new_class, &info);
        let mut matches = Matches::default();
        for subject in &subjects {
            let found = self.ctx.index.lookup(self.ctx.table, subject.as_subject());
            merge_matches(&mut matches, found);
        }
        let params = self.param_decls(&info);
        for sink in matches.sinks.clone() {
            self.check_sink(sink, ins.span, args, receiver, params);
        }
        let arg_taints: Vec<Taint> = args.iter().map(|a| self.tv(a.value)).collect();
        let receiver_taint = receiver.map(|r| self.tv(r));
        let mut result = self.source_taint(&matches, ins.span);

        if !matches.sanitizers.is_empty() {
            let mut flow = self.all_inputs(&arg_taints, receiver_taint.as_ref());
            for entry in &matches.sanitizers {
                let clears = self.ctx.index.sanitizers()[*entry as usize].clears;
                let applied = Applied {
                    entry: *entry,
                    span: ins.span,
                };
                flow = flow.cleared_for(self.unit, clears, applied);
            }
            result.join_in(&flow, cap);
        } else if !matches.propagators.is_empty() {
            for entry in &matches.propagators {
                let from = &self.ctx.index.propagators()[*entry as usize].from;
                for sel in from {
                    let picked =
                        self.select(sel, args, &arg_taints, receiver_taint.as_ref(), params);
                    result.join_in(&picked, cap);
                }
            }
            self.write_back_by_ref(args, params, &result, &arg_taints, st);
        } else if matches.sources.is_empty() {
            let applied =
                self.apply_summaries(args, &arg_taints, receiver_taint.as_ref(), ins.span, st);
            let default_needed = applied.as_ref().is_none_or(|a| a.partial);
            if let Some(applied) = &applied {
                result.join_in(&applied.result, cap);
            }
            if default_needed {
                let flow = self.all_inputs(&arg_taints, receiver_taint.as_ref());
                let flow = match &info {
                    CallInfo::Unresolved
                    | CallInfo::UnresolvedFunctionNotFound
                    | CallInfo::Missing
                    | CallInfo::Resolved { partial: true, .. } => {
                        flow.map_marks(|m| m.assumed_unresolved = true)
                    }
                    _ if applied.is_some() => flow.map_marks(|m| m.assumed_unresolved = true),
                    _ => flow,
                };
                let flow = match &info {
                    CallInfo::Resolved { targets, .. } if multi_target(targets) => {
                        let count = candidate_count(targets);
                        flow.map_marks(|m| m.several = m.several.max(count))
                    }
                    _ => flow,
                };
                result.join_in(&flow, cap);
                if let Some(Callee::Value(v)) = callee {
                    result.join_in(&self.tv(*v), cap);
                }
            }
            if applied.is_none() {
                self.write_back_by_ref(args, params, &result, &arg_taints, st);
            }
        }
        result
    }

    /// The units this call goes to, whether dispatch may reach several, and whether part of the
    /// call is not covered by them.
    fn site_targets(&self) -> Option<SiteTargets> {
        let inter = self.ctx.inter?;
        let site = inter
            .graph
            .sites_of(self.site_unit)
            .iter()
            .find(|s| s.block.index() as u32 == self.at.0 && s.index == self.at.1)?;
        match &site.outcome {
            CallOutcome::Units {
                targets,
                multi_target,
                truncated,
                residual,
            } if !targets.is_empty() => Some(SiteTargets {
                targets: targets.clone(),
                multi: *multi_target || targets.len() > 1,
                partial: *truncated || !residual.is_empty(),
            }),
            _ => None,
        }
    }

    /// Applies the summary of every target of the call at the current position; `None` when the
    /// call has no analysed target.
    #[allow(
        clippy::too_many_arguments,
        reason = "the call's pieces, read together"
    )]
    fn apply_summaries(
        &mut self,
        args: &[CallArg],
        arg_taints: &[Taint],
        receiver: Option<&Taint>,
        call: Span,
        st: &mut Locals,
    ) -> Option<Summarised> {
        let inter = self.ctx.inter?;
        let SiteTargets {
            targets,
            multi,
            partial,
        } = self.site_targets()?;
        let cap = self.cap();
        let several = if multi {
            u16::try_from(targets.len()).unwrap_or(u16::MAX).max(2)
        } else {
            0
        };
        let mut result = Taint::clean();
        for target in targets {
            let Some(flow) = inter.summaries.get(target) else {
                // Not summarised yet (the same recursive component): the empty summary.
                continue;
            };
            let callee = inter.graphs[target.index()];
            let (params, bound) = bind_args(callee, args, arg_taints);
            let inputs = Inputs {
                params,
                receiver: receiver.cloned(),
            };
            let returned = self.apply(flow, &inputs, call, several);
            result.join_in(&returned, cap);
            for (position, effect) in &flow.params {
                let landed = effect.substitute(cap, self.call_step(call), |o| inputs.get(o));
                if landed.is_clean() {
                    continue;
                }
                for at in bound.get(*position as usize).into_iter().flatten() {
                    let Some(def) = self.defs.get(args[*at].value.index()).copied().flatten()
                    else {
                        continue;
                    };
                    if let InstrKind::Load(place) = &def.kind {
                        let place = place.clone();
                        self.store(&place, landed.clone(), true, st);
                    }
                }
            }
        }
        Some(Summarised { result, partial })
    }

    /// Substitutes `inputs` into a callee's summary: sink hits and heap writes are recorded here,
    /// and the returned taint is the callee's result.
    fn apply(&mut self, flow: &UnitFlow, inputs: &Inputs, call: Span, several: u16) -> Taint {
        let cap = self.cap();
        let via = self.call_step(call);
        for hit in &flow.hits {
            if !hit.label.origin.is_symbolic() {
                continue;
            }
            let Some(actual) = inputs.get(&hit.label.origin) else {
                continue;
            };
            let entry = &self.ctx.index.sinks()[hit.sink as usize];
            let (class, rule) = (entry.class, entry.rule);
            for outer in actual.labels() {
                let mut label = hit.label.compose(outer, via);
                label.marks.several = label.marks.several.max(several);
                if !self.fires(&label, class, rule) {
                    continue;
                }
                self.record(SinkHit {
                    sink: hit.sink,
                    label,
                    unit: hit.unit,
                    span: hit.span,
                    arg: hit.arg,
                });
            }
        }
        for (location, taint) in &flow.writes {
            let landed = taint.substitute(cap, via, |o| inputs.get(o));
            if !landed.is_clean() {
                self.writes
                    .entry(location.clone())
                    .or_default()
                    .join_in(&landed, cap);
            }
        }
        let returned = flow.returns.substitute(cap, via, |o| inputs.get(o));
        if several > 1 {
            returned.map_marks(|m| m.several = m.several.max(several))
        } else {
            returned
        }
    }

    /// An `include` or `require` that resolved: the included files' top-level code runs in this
    /// scope, so their variables are this unit's.
    fn include(&mut self, ins: &Instr, st: &mut Locals) -> Taint {
        let Some(inter) = self.ctx.inter else {
            return Taint::clean();
        };
        let Some(site) = inter.graph.includes().iter().find(|s| {
            s.caller == self.site_unit
                && s.block.index() as u32 == self.at.0
                && s.index == self.at.1
        }) else {
            return Taint::clean();
        };
        let IncludeOutcome::Resolved(targets) = &site.outcome else {
            return Taint::clean();
        };
        let cap = self.cap();
        let several = u16::try_from(targets.len()).unwrap_or(u16::MAX);
        let mut result = Taint::clean();
        for target in targets {
            let Some(flow) = inter.summaries.get(*target) else {
                continue;
            };
            let callee = inter.graphs[target.index()];
            let params: Vec<Taint> = callee
                .locals
                .iter()
                .map(|name| {
                    self.find_local(name)
                        .and_then(|id| st.get(&id))
                        .cloned()
                        .unwrap_or_default()
                })
                .collect();
            let inputs = Inputs {
                params,
                receiver: None,
            };
            result.join_in(&self.apply(flow, &inputs, ins.span, several), cap);
            for (name, taint) in &flow.scope {
                let landed = taint.substitute(cap, self.call_step(ins.span), |o| inputs.get(o));
                let id = self.local_named(name);
                if several > 1 {
                    if !landed.is_clean() {
                        st.entry(id).or_default().join_in(&landed, cap);
                    }
                } else if landed.is_clean() {
                    st.remove(&id);
                } else {
                    st.insert(id, landed);
                }
            }
        }
        result
    }

    fn find_local(&self, name: &str) -> Option<u32> {
        if let Some(at) = self.graph.locals.iter().position(|n| &**n == name) {
            return Some(at as u32);
        }
        let at = self.extras.iter().position(|n| &**n == name)?;
        Some((self.graph.locals.len() + at) as u32)
    }

    fn all_inputs(&self, args: &[Taint], receiver: Option<&Taint>) -> Taint {
        let cap = self.cap();
        let mut out = Taint::clean();
        for t in args.iter().chain(receiver) {
            out.join_in(t, cap);
        }
        out
    }

    /// The symbols the call may reach, for the rule lookup.
    fn param_decls(&self, info: &CallInfo<'_>) -> Option<&'c [ParamDecl]>
    where
        'a: 'c,
    {
        let table: &'a SymbolTable = self.ctx.table;
        param_decls(table, info)
    }

    /// The taint of the inputs a selector names.
    #[allow(
        clippy::too_many_arguments,
        reason = "the call's pieces, read together"
    )]
    fn select(
        &self,
        sel: &ArgSel,
        args: &[CallArg],
        taints: &[Taint],
        receiver: Option<&Taint>,
        params: Option<&[ParamDecl]>,
    ) -> Taint {
        let cap = self.cap();
        let mut out = Taint::clean();
        if matches!(sel, ArgSel::Receiver) {
            if let Some(r) = receiver {
                out.join_in(r, cap);
            }
            return out;
        }
        for at in selected_args(sel, args, params) {
            out.join_in(&taints[at], cap);
        }
        out
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "the call's pieces, read together"
    )]
    fn check_sink(
        &mut self,
        sink: u32,
        call: Span,
        args: &[CallArg],
        receiver: Option<ValueId>,
        params: Option<&[ParamDecl]>,
    ) {
        let entry = &self.ctx.index.sinks()[sink as usize];
        let (class, rule, vulnerable) = (entry.class, entry.rule, entry.args.clone());
        let mut found: Vec<(Taint, Span)> = Vec::new();
        for sel in &vulnerable {
            if matches!(sel, ArgSel::Receiver) {
                if let Some(r) = receiver {
                    found.push((self.tv(r), call));
                }
                continue;
            }
            for at in selected_args(sel, args, params) {
                found.push((self.tv(args[at].value), args[at].span));
            }
        }
        for (taint, arg) in found {
            for label in taint.labels() {
                if self.fires(label, class, rule) {
                    self.record(SinkHit {
                        sink,
                        label: label.clone(),
                        unit: self.unit,
                        span: call,
                        arg,
                    });
                }
            }
        }
    }

    /// Whether `label` counts for a sink of `class` in `rule`: not cleared for the class, and, when
    /// its origin is a concrete source, a source of that rule.
    fn fires(&self, label: &Label, class: SinkClass, rule: u32) -> bool {
        if label.cleared.contains(class) {
            return false;
        }
        match label.origin {
            Origin::Source { source, .. } => self.ctx.index.sources()[source as usize]
                .rules
                .contains(&rule),
            _ => true,
        }
    }

    /// A by-reference parameter of the callee receives what flowed into the call.
    #[allow(
        clippy::too_many_arguments,
        reason = "the call's pieces, read together"
    )]
    fn write_back_by_ref(
        &mut self,
        args: &[CallArg],
        params: Option<&[ParamDecl]>,
        result: &Taint,
        arg_taints: &[Taint],
        st: &mut Locals,
    ) {
        let Some(params) = params else {
            return;
        };
        let cap = self.cap();
        let mut flow = result.clone();
        for t in arg_taints {
            flow.join_in(t, cap);
        }
        if flow.is_clean() {
            return;
        }
        for (position, arg) in args.iter().enumerate() {
            let param = params
                .get(position)
                .or_else(|| params.last().filter(|p| p.variadic));
            if arg.spread || !param.is_some_and(|p| p.by_ref) {
                continue;
            }
            let Some(def) = self.defs.get(arg.value.index()).copied().flatten() else {
                continue;
            };
            if let InstrKind::Load(place) = &def.kind {
                let place = place.clone();
                self.store(&place, flow.clone(), true, st);
            }
        }
    }
}

/// The units a call goes to.
struct SiteTargets {
    targets: Vec<UnitId>,
    /// Dispatch may reach several of them.
    multi: bool,
    /// Part of the call is not covered by them.
    partial: bool,
}

/// What applying the summaries of a call's targets produced.
struct Summarised {
    result: Taint,
    /// Part of the call is not covered by the targets, so the default policy applies as well.
    partial: bool,
}

/// What stands for a callee's symbolic origins at a call.
struct Inputs {
    params: Vec<Taint>,
    receiver: Option<Taint>,
}

impl Inputs {
    fn get(&self, origin: &Origin) -> Option<&Taint> {
        match origin {
            Origin::Param(at) => self.params.get(*at as usize),
            Origin::Receiver => self.receiver.as_ref(),
            _ => None,
        }
    }
}

/// The taint of each parameter of `callee` given the arguments, and the arguments that bind to
/// each. A spread argument binds to every parameter from its position on; surplus positional
/// arguments go to a trailing variadic parameter.
fn bind_args(callee: &Graph, args: &[CallArg], taints: &[Taint]) -> (Vec<Taint>, Vec<Vec<usize>>) {
    let count = callee.params.len();
    let mut bound: Vec<Vec<usize>> = vec![Vec::new(); count];
    let mut positional = 0usize;
    for (at, arg) in args.iter().enumerate() {
        if let Some(name) = &arg.name {
            if let Some(p) = callee.params.iter().position(|p| *p.name == **name) {
                bound[p].push(at);
            }
        } else if arg.spread {
            for slot in bound.iter_mut().skip(positional) {
                slot.push(at);
            }
            positional = count;
        } else {
            let slot = if positional < count {
                Some(positional)
            } else {
                callee
                    .params
                    .last()
                    .filter(|p| p.variadic)
                    .map(|_| count - 1)
            };
            if let Some(slot) = slot {
                bound[slot].push(at);
            }
            positional += 1;
        }
    }
    let mut params = Vec::with_capacity(count);
    for list in &bound {
        let mut taint = Taint::clean();
        for at in list {
            taint.join_in(&taints[*at], usize::MAX);
        }
        params.push(taint);
    }
    (params, bound)
}

/// The taints stored for a location. A property of an unknown class reads that name in every
/// class, and a property of a known class also reads what was stored under the unknown class.
fn read_location<'h>(heap: &'h Heap, key: &HeapKey) -> Vec<&'h Taint> {
    let mut out: Vec<&Taint> = Vec::new();
    let (name, class, is_static) = match key {
        HeapKey::Prop { name, class } => (name, class, false),
        HeapKey::Static { name, class } => (name, class, true),
        HeapKey::Global(_) => return heap.get(key).into_iter().collect(),
    };
    let make = |class: &str| {
        if is_static {
            HeapKey::Static {
                name: name.clone(),
                class: class.into(),
            }
        } else {
            HeapKey::Prop {
                name: name.clone(),
                class: class.into(),
            }
        }
    };
    if &**class == "?" {
        for (k, t) in heap.range(make("")..) {
            let same = match k {
                HeapKey::Prop { name: n, .. } => !is_static && n == name,
                HeapKey::Static { name: n, .. } => is_static && n == name,
                HeapKey::Global(_) => false,
            };
            if !same {
                break;
            }
            out.push(t);
        }
    } else {
        out.extend(heap.get(key));
        out.extend(heap.get(&make("?")));
    }
    out
}
