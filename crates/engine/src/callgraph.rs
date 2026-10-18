//! The call graph over lowered units: one node per unit, an edge per call-resolution candidate and
//! per resolved include, and the strongly connected components in callee-first order.

use std::collections::BTreeMap;

use bleedthrough_ir::{BlockId, Graph, InstrKind, Resolution, UnitKey, UnitKind, ValueId};
use bleedthrough_symbols::{
    CallResolution, DynamicKind, Fqn, SymbolTable, Target, TargetKind, UnresolvedReason,
};
use bleedthrough_syntax::Span;

use crate::coverage::CallCoverage;
use crate::fold::{Folded, Folder};
use crate::include::{FoldedPath, IncludeResolver};
use crate::scc::strongly_connected;

/// Term: Call graph
///
/// A lowered unit with the file it came from: what the call graph is built from.
#[derive(Debug, Clone, Copy)]
#[allow(clippy::exhaustive_structs, reason = "a plain pair")]
pub struct SourceUnit<'a> {
    /// The project-relative path of the file.
    pub file: &'a str,
    /// The unit's graph.
    pub graph: &'a Graph,
}

/// Term: Call graph
///
/// A unit's place in the call graph. Identifiers follow the order of (file, unit key), so they do
/// not depend on the order the files were supplied in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitId(u32);

impl UnitId {
    /// The position of the unit in [`CallGraph::units`].
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// The identifier at position `index` of [`CallGraph::units`].
    ///
    /// # Panics
    /// If `index` does not fit the identifier space (more than `u32::MAX` units).
    pub fn from_index(index: usize) -> Self {
        Self(u32::try_from(index).expect("fewer than 2^32 units"))
    }
}

/// Term: Call graph
///
/// What the call graph knows of a unit.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct UnitInfo {
    /// The file it is in.
    pub file: Box<str>,
    /// Its key inside the file.
    pub key: UnitKey,
    /// What kind of unit it is.
    pub kind: UnitKind,
}

/// Term: Call graph
///
/// Why a call has no target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Unresolved {
    /// Type inference could not resolve it, for this reason.
    Inference(UnresolvedReason),
    /// The callee is computed at run time.
    Dynamic(DynamicKind),
    /// Type inference recorded nothing for the call.
    NoFacts,
}

impl Unresolved {
    /// The stable reason code.
    pub fn code(self) -> &'static str {
        match self {
            Self::Inference(reason) => reason.as_str(),
            Self::Dynamic(DynamicKind::VariableFunction) => "dynamic_variable_function",
            Self::Dynamic(DynamicKind::VariableMethod) => "dynamic_variable_method",
            Self::Dynamic(DynamicKind::VariableClass) => "dynamic_variable_class",
            Self::Dynamic(DynamicKind::CallableString) => "dynamic_callable_string",
            Self::Dynamic(_) => "dynamic",
            Self::NoFacts => "no_facts",
        }
    }
}

/// Term: Call graph
///
/// Where a call goes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CallOutcome {
    /// To these analysed units (possibly none: `new` on a class without a constructor).
    Units {
        /// The targets, in unit order.
        targets: Vec<UnitId>,
        /// Whether dispatch may reach several candidates.
        multi_target: bool,
        /// Whether a cap cut the candidate set.
        truncated: bool,
        /// Reasons part of the receiver did not resolve.
        residual: Vec<UnresolvedReason>,
    },
    /// To a built-in function or method.
    Builtin(Fqn),
    /// To a declaration whose body is not among the analysed units.
    External(Box<str>),
    /// To nowhere known.
    Unresolved(Unresolved),
}

/// Term: Call graph
///
/// One call, `new` or first-class callable in a unit, and where it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct CallSite {
    /// The unit that contains it.
    pub caller: UnitId,
    /// The block.
    pub block: BlockId,
    /// The instruction's position inside the block.
    pub index: u32,
    /// The call's source.
    pub span: Span,
    /// Where it goes.
    pub outcome: CallOutcome,
}

/// Term: Call graph
///
/// Where an include goes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum IncludeOutcome {
    /// To the top-level code of these files.
    Resolved(Vec<UnitId>),
    /// The path is not a constant the engine can evaluate.
    Dynamic,
    /// The path folded to this text but no analysed file matches.
    NotFound(Box<str>),
}

/// Term: Call graph
///
/// One include in a unit and where it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct IncludeSite {
    /// The unit that contains it.
    pub caller: UnitId,
    /// The block.
    pub block: BlockId,
    /// The instruction's position inside the block.
    pub index: u32,
    /// The include's source.
    pub span: Span,
    /// Where it goes.
    pub outcome: IncludeOutcome,
}

/// Term: Call graph
///
/// A strongly connected component: units that may call one another, or one unit.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct Component {
    /// The units, in unit order.
    pub units: Vec<UnitId>,
    /// Whether the component contains a cycle (mutual recursion or a unit calling itself).
    pub recursive: bool,
}

/// Term: Call graph
///
/// How the call graph is bounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record of limits")]
pub struct CallGraphConfig {
    /// Most candidate targets kept for one call.
    pub max_candidates: usize,
}

impl Default for CallGraphConfig {
    fn default() -> Self {
        Self { max_candidates: 32 }
    }
}

/// Term: Call graph
///
/// The call graph of a set of units.
#[derive(Debug, Clone)]
pub struct CallGraph {
    units: Vec<UnitInfo>,
    sites: Vec<CallSite>,
    includes: Vec<IncludeSite>,
    edges: Vec<Vec<UnitId>>,
    components: Vec<Component>,
    component_of: Vec<u32>,
    coverage: CallCoverage,
}

impl CallGraph {
    /// Builds the graph. The result does not depend on the order of `units`.
    pub fn build(
        units: &[SourceUnit<'_>],
        table: &SymbolTable,
        resolver: &dyn IncludeResolver,
        config: &CallGraphConfig,
    ) -> Self {
        let mut ordered: Vec<&SourceUnit<'_>> = units.iter().collect();
        ordered.sort_by(|a, b| (a.file, &a.graph.key).cmp(&(b.file, &b.graph.key)));
        let index = Index::new(&ordered);
        let mut builder = Builder {
            table,
            resolver,
            config,
            index: &index,
            sites: Vec::new(),
            includes: Vec::new(),
            coverage: CallCoverage::default(),
        };
        for (position, unit) in ordered.iter().enumerate() {
            builder.scan(UnitId(position as u32), unit);
        }
        let Builder {
            sites,
            includes,
            coverage,
            ..
        } = builder;
        let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); ordered.len()];
        for site in &sites {
            if let CallOutcome::Units { targets, .. } = &site.outcome {
                adjacency[site.caller.index()].extend(targets.iter().map(|t| t.index()));
            }
        }
        for site in &includes {
            if let IncludeOutcome::Resolved(targets) = &site.outcome {
                adjacency[site.caller.index()].extend(targets.iter().map(|t| t.index()));
            }
        }
        for successors in &mut adjacency {
            successors.sort_unstable();
            successors.dedup();
        }
        let components: Vec<Component> = strongly_connected(&adjacency)
            .into_iter()
            .map(|members| {
                let recursive =
                    members.len() > 1 || members.first().is_some_and(|m| adjacency[*m].contains(m));
                Component {
                    units: members.into_iter().map(|m| UnitId(m as u32)).collect(),
                    recursive,
                }
            })
            .collect();
        let mut component_of = vec![0u32; ordered.len()];
        for (number, component) in components.iter().enumerate() {
            for unit in &component.units {
                component_of[unit.index()] = number as u32;
            }
        }
        Self {
            units: ordered
                .iter()
                .map(|u| UnitInfo {
                    file: u.file.into(),
                    key: u.graph.key.clone(),
                    kind: u.graph.kind,
                })
                .collect(),
            sites,
            includes,
            edges: adjacency
                .into_iter()
                .map(|s| s.into_iter().map(|n| UnitId(n as u32)).collect())
                .collect(),
            components,
            component_of,
            coverage,
        }
    }

    /// Every unit, in identifier order.
    pub fn units(&self) -> &[UnitInfo] {
        &self.units
    }

    /// What is known of `unit`.
    pub fn unit(&self, unit: UnitId) -> &UnitInfo {
        &self.units[unit.index()]
    }

    /// The identifier of the unit with this key in this file.
    pub fn find(&self, file: &str, key: &UnitKey) -> Option<UnitId> {
        let probe = |u: &UnitInfo| (&*u.file, &u.key).cmp(&(file, key));
        self.units
            .binary_search_by(probe)
            .ok()
            .map(|i| UnitId(i as u32))
    }

    /// Every call site, in (caller, block, instruction) order.
    pub fn sites(&self) -> &[CallSite] {
        &self.sites
    }

    /// The call sites of `unit`.
    pub fn sites_of(&self, unit: UnitId) -> &[CallSite] {
        let start = self.sites.partition_point(|s| s.caller < unit);
        let end = self.sites.partition_point(|s| s.caller <= unit);
        &self.sites[start..end]
    }

    /// Every include, in (caller, block, instruction) order.
    pub fn includes(&self) -> &[IncludeSite] {
        &self.includes
    }

    /// The units `unit` may call or include, sorted and distinct.
    pub fn callees(&self, unit: UnitId) -> &[UnitId] {
        &self.edges[unit.index()]
    }

    /// The strongly connected components, callees first.
    pub fn components(&self) -> &[Component] {
        &self.components
    }

    /// The position of `unit`'s component in [`Self::components`].
    pub fn component_of(&self, unit: UnitId) -> usize {
        self.component_of[unit.index()] as usize
    }

    /// What could not be followed, by reason.
    pub fn coverage(&self) -> &CallCoverage {
        &self.coverage
    }
}

/// Lookups from declarations to units.
struct Index {
    functions: BTreeMap<String, Vec<UnitId>>,
    methods: BTreeMap<String, Vec<UnitId>>,
    mains: BTreeMap<Box<str>, UnitId>,
}

impl Index {
    fn new(ordered: &[&SourceUnit<'_>]) -> Self {
        let mut functions: BTreeMap<String, Vec<UnitId>> = BTreeMap::new();
        let mut methods: BTreeMap<String, Vec<UnitId>> = BTreeMap::new();
        let mut mains: BTreeMap<Box<str>, UnitId> = BTreeMap::new();
        for (position, unit) in ordered.iter().enumerate() {
            let id = UnitId(position as u32);
            let key = unit.graph.key.name.to_ascii_lowercase();
            match unit.graph.kind {
                UnitKind::Function => functions.entry(key).or_default().push(id),
                UnitKind::Method => methods.entry(key).or_default().push(id),
                UnitKind::Main => {
                    mains.entry(unit.file.into()).or_insert(id);
                }
                _ => {}
            }
        }
        Self {
            functions,
            methods,
            mains,
        }
    }

    fn method(&self, owner: &Fqn, name: &str) -> &[UnitId] {
        let key = format!("{}::{}", owner.key(), name.to_ascii_lowercase());
        self.methods.get(&key).map_or(&[], Vec::as_slice)
    }
}

struct Builder<'a> {
    table: &'a SymbolTable,
    resolver: &'a dyn IncludeResolver,
    config: &'a CallGraphConfig,
    index: &'a Index,
    sites: Vec<CallSite>,
    includes: Vec<IncludeSite>,
    coverage: CallCoverage,
}

impl Builder<'_> {
    fn scan(&mut self, caller: UnitId, unit: &SourceUnit<'_>) {
        let folder = Folder::new(unit.graph, self.table, unit.file);
        for (b, block) in unit.graph.blocks.iter().enumerate() {
            for (i, instr) in block.instrs.iter().enumerate() {
                let (block_id, index) = (BlockId::new(b as u32), i as u32);
                match &instr.kind {
                    InstrKind::Call { resolution, .. } => {
                        let outcome = self.call(resolution);
                        self.push_site((caller, block_id, index), instr.span, outcome);
                    }
                    InstrKind::New { resolution, .. } => {
                        let outcome = self.call(resolution);
                        self.push_site((caller, block_id, index), instr.span, outcome);
                    }
                    InstrKind::CallableRef { resolution, .. } => {
                        let outcome = self.call(resolution);
                        self.push_site((caller, block_id, index), instr.span, outcome);
                    }
                    InstrKind::Include { path, .. } => {
                        let outcome = self.include(unit.file, &folder, *path);
                        self.includes.push(IncludeSite {
                            caller,
                            block: block_id,
                            index,
                            span: instr.span,
                            outcome,
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    fn push_site(&mut self, at: (UnitId, BlockId, u32), span: Span, outcome: CallOutcome) {
        match &outcome {
            CallOutcome::Unresolved(reason) => self.coverage.call(reason.code()),
            CallOutcome::External(_) => self.coverage.external_calls += 1,
            CallOutcome::Units {
                truncated: true, ..
            } => self.coverage.truncated_calls += 1,
            _ => {}
        }
        let (caller, block, index) = at;
        self.sites.push(CallSite {
            caller,
            block,
            index,
            span,
            outcome,
        });
    }

    fn include(&mut self, from: &str, folder: &Folder<'_>, path: ValueId) -> IncludeOutcome {
        let Folded::Value(text) = folder.fold(path) else {
            self.coverage.include("dynamic");
            return IncludeOutcome::Dynamic;
        };
        let folded = FoldedPath::new(&text);
        let mut targets: Vec<UnitId> = self
            .resolver
            .resolve(from, &folded)
            .iter()
            .filter_map(|file| self.index.mains.get(file).copied())
            .collect();
        targets.sort_unstable();
        targets.dedup();
        if targets.is_empty() {
            self.coverage.include("not_found");
            return IncludeOutcome::NotFound(folded.as_str().into());
        }
        IncludeOutcome::Resolved(targets)
    }

    fn call(&mut self, resolution: &Resolution) -> CallOutcome {
        let Resolution::Facts(facts) = resolution else {
            return CallOutcome::Unresolved(Unresolved::NoFacts);
        };
        match facts {
            CallResolution::Dynamic(kind) => CallOutcome::Unresolved(Unresolved::Dynamic(*kind)),
            CallResolution::Unresolved(reason) => {
                CallOutcome::Unresolved(Unresolved::Inference(*reason))
            }
            CallResolution::Resolved { targets, residual } => self.resolved(targets, residual),
            _ => CallOutcome::Unresolved(Unresolved::NoFacts),
        }
    }

    fn resolved(&self, targets: &[Target], residual: &[UnresolvedReason]) -> CallOutcome {
        let mut units: Vec<UnitId> = Vec::new();
        let mut builtin: Option<Fqn> = None;
        let mut external: Option<Box<str>> = None;
        for target in targets {
            let found = self.units_of(target);
            if found.is_empty() {
                match self.declaration_kind(target) {
                    Declaration::Builtin(name) => {
                        builtin.get_or_insert(name);
                    }
                    Declaration::Other => {
                        external.get_or_insert_with(|| target.to_string().into());
                    }
                }
            }
            units.extend(found);
        }
        units.sort_unstable();
        units.dedup();
        if units.is_empty() {
            if let Some(name) = builtin {
                return CallOutcome::Builtin(name);
            }
            if let Some(name) = external {
                return CallOutcome::External(name);
            }
            if !targets.is_empty() {
                return CallOutcome::External("unknown".into());
            }
        }
        let multi_target = units.len() > 1 || targets.len() > 1;
        let cut = units.len() > self.config.max_candidates;
        units.truncate(self.config.max_candidates);
        CallOutcome::Units {
            targets: units,
            multi_target,
            truncated: cut || residual.contains(&UnresolvedReason::LimitHit),
            residual: residual.to_vec(),
        }
    }

    /// The analysed units a target may run: its own body and the bodies of its implementors.
    fn units_of(&self, target: &Target) -> Vec<UnitId> {
        match target.kind {
            TargetKind::Function => self
                .index
                .functions
                .get(&target.name.to_ascii_lowercase())
                .cloned()
                .unwrap_or_default(),
            TargetKind::Method | TargetKind::Constructor => {
                let name = if target.kind == TargetKind::Constructor {
                    "__construct"
                } else {
                    &target.name
                };
                let mut out: Vec<UnitId> = Vec::new();
                for class in target.owner.iter().chain(target.implementors.iter()) {
                    out.extend(self.method_units(class, name));
                }
                out
            }
            _ => Vec::new(),
        }
    }

    /// The bodies `class::name` may run: the first declaration up the hierarchy that has one.
    fn method_units(&self, class: &Fqn, name: &str) -> Vec<UnitId> {
        let found = self.table.method(class, name, None);
        for candidate in &found.candidates {
            let units = self.index.method(&candidate.owner.name, name);
            if !units.is_empty() {
                return units.to_vec();
            }
        }
        self.index.method(class, name).to_vec()
    }

    fn declaration_kind(&self, target: &Target) -> Declaration {
        match (&target.kind, &target.owner) {
            (TargetKind::Function, _) => {
                let name = Fqn::new(&target.name);
                match self.table.function(&name) {
                    Some(decl) if decl.builtin.is_some() => Declaration::Builtin(name),
                    _ => Declaration::Other,
                }
            }
            (_, Some(owner)) => match self.table.class(owner) {
                Some(decl) if decl.builtin.is_some() => {
                    Declaration::Builtin(Fqn::new(&format!("{owner}::{}", target.name)))
                }
                _ => Declaration::Other,
            },
            _ => Declaration::Other,
        }
    }
}

enum Declaration {
    Builtin(Fqn),
    Other,
}
