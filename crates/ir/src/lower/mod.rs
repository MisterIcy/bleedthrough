//! Lowering: the owned syntax tree and the inference facts become one graph per unit.
//!
//! Lowering is total: every tree the syntax model can represent lowers, and a construct it does
//! not model becomes a counted unknown instruction. It depends only on the file's tree and its
//! facts, numbers blocks and values in the order it meets them, and never recurses past the depth
//! bound.

mod builder;
mod captures;
mod expr;
mod stmt;
mod types;

use std::collections::BTreeMap;

use bleedthrough_symbols::{FileScopes, FunctionFacts};
use bleedthrough_syntax::{Span, SyntaxTree};

use crate::graph::{Capture, Graph, ParamInfo, UnitKey, UnitKind};
use crate::instr::{InstrKind, Place, TypeClass};
use bleedthrough_symbols::Fqn;
use builder::{Fb, Job, JobBody, Shared};

/// Term: exempt
///
/// The bounds lowering keeps. Past a bound the unit is lowered conservatively and the graph
/// records which bound was reached.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record of limits")]
pub struct LowerConfig {
    /// Deepest expression or statement nesting lowered by recursion; deeper nodes become one
    /// unknown instruction over the variables inside them.
    pub max_depth: usize,
    /// Most blocks in one graph.
    pub max_blocks: usize,
    /// Most instructions in one graph.
    pub max_instructions: usize,
}

impl Default for LowerConfig {
    fn default() -> Self {
        Self {
            max_depth: 256,
            max_blocks: 20_000,
            max_instructions: 200_000,
        }
    }
}

/// Lowers every unit of a file with the default bounds.
pub fn lower_file(tree: &SyntaxTree, facts: &[FunctionFacts]) -> Vec<Graph> {
    lower_file_with(tree, facts, &LowerConfig::default())
}

/// Lowers every unit of a file: each function, method, closure, arrow function, property hook
/// and the top-level code. Graphs are returned in the order of their keys.
pub fn lower_file_with(
    tree: &SyntaxTree,
    facts: &[FunctionFacts],
    config: &LowerConfig,
) -> Vec<Graph> {
    let scopes = FileScopes::build(tree.statements());
    let index = FactsIndex::new(facts);
    let mut jobs: Vec<Job<'_>> = Vec::new();
    let end = tree.statements().last().map_or(0, |s| s.span.end);
    jobs.push(Job {
        kind: UnitKind::Main,
        span: Span::new(0, end),
        fallback_name: "{main}".to_string(),
        owner: None,
        is_static: true,
        params: &[],
        implicit_params: Vec::new(),
        captures: Vec::new(),
        body: JobBody::Stmts(tree.statements()),
        promote: false,
    });
    let mut shared = Shared {
        config,
        scopes: &scopes,
        jobs: &mut jobs,
    };
    let mut graphs = Vec::new();
    while let Some(job) = shared.jobs.pop() {
        let found = index.find(job.kind, job.span);
        graphs.push(lower_unit(&mut shared, &job, found));
    }
    graphs.sort_by(|a, b| a.key.cmp(&b.key));
    graphs
}

/// Facts found by where their unit lies.
struct FactsIndex<'f> {
    by_span: BTreeMap<(u32, u32), Vec<&'f FunctionFacts>>,
}

impl<'f> FactsIndex<'f> {
    fn new(facts: &'f [FunctionFacts]) -> Self {
        let mut by_span: BTreeMap<(u32, u32), Vec<&'f FunctionFacts>> = BTreeMap::new();
        for f in facts {
            by_span
                .entry((f.span.start, f.span.end))
                .or_default()
                .push(f);
        }
        Self { by_span }
    }

    fn find(&self, kind: UnitKind, span: Span) -> Option<&'f FunctionFacts> {
        let candidates = self.by_span.get(&(span.start, span.end))?;
        candidates.iter().copied().find(|f| match kind {
            UnitKind::Main => &*f.name == "{main}",
            UnitKind::Closure | UnitKind::ArrowFn => f.name.starts_with("{closure}"),
            UnitKind::Function | UnitKind::Method => !f.name.starts_with('{'),
            _ => false,
        })
    }
}

fn lower_unit<'a, 'f>(
    shared: &mut Shared<'a, 'f>,
    job: &Job<'a>,
    facts: Option<&'f FunctionFacts>,
) -> Graph {
    let name: Box<str> = facts.map_or_else(|| job.fallback_name.clone().into(), |f| f.name.clone());
    let key = UnitKey {
        span: job.span,
        name,
    };
    let mut fb = Fb::new(shared, facts);
    let params = bind_params(&mut fb, job);
    let captures = bind_captures(&mut fb, job);
    if job.promote {
        promote_parameters(&mut fb, job);
    }
    match job.body {
        JobBody::Stmts(body) => fb.stmts(body),
        JobBody::Expr(body) => {
            let value = fb.expr(body);
            fb.do_return(body.span, Some(value));
        }
    }
    fb.finish(job, key, params, captures)
}

fn bind_params(fb: &mut Fb<'_, '_, '_>, job: &Job<'_>) -> Vec<ParamInfo> {
    let mut out: Vec<ParamInfo> = job
        .params
        .iter()
        .map(|p| {
            let name = p.name.strip_prefix('$').unwrap_or(&p.name);
            ParamInfo {
                local: fb.local(name),
                name: name.into(),
                by_ref: p.by_ref,
                variadic: p.variadic,
                span: p.span,
            }
        })
        .collect();
    for name in &job.implicit_params {
        out.push(ParamInfo {
            local: fb.local(name),
            name: (*name).into(),
            by_ref: false,
            variadic: false,
            span: job.span,
        });
    }
    out
}

fn bind_captures(fb: &mut Fb<'_, '_, '_>, job: &Job<'_>) -> Vec<Capture> {
    job.captures
        .iter()
        .map(|(name, by_ref)| {
            let plain = name.strip_prefix('$').unwrap_or(name);
            Capture {
                local: fb.local(plain),
                name: plain.into(),
                by_ref: *by_ref,
            }
        })
        .collect()
}

/// A constructor parameter with a visibility modifier is also a property: `$this->p = $p`.
fn promote_parameters(fb: &mut Fb<'_, '_, '_>, job: &Job<'_>) {
    let owners: Vec<Fqn> = job.owner.iter().map(|o| Fqn::new(o)).collect();
    for param in job.params.iter().filter(|p| !p.modifiers.is_empty()) {
        let name = param.name.strip_prefix('$').unwrap_or(&param.name);
        let this_local = fb.local("this");
        let object = fb.load_local(param.span, this_local);
        let local = fb.local(name);
        let value = fb.emit(
            param.span,
            TypeClass::Unknown,
            InstrKind::Load(Place::Local(local)),
        );
        fb.emit_effect(
            param.span,
            InstrKind::Store {
                place: Place::Prop {
                    object,
                    name: crate::instr::MemberRef::Named(name.into()),
                    owners: owners.clone(),
                },
                value,
                weak: false,
            },
        );
    }
}
