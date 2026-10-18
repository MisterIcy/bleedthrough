//! The per-unit builder: blocks, locals, emission and the jump machinery.

use std::collections::{BTreeMap, BTreeSet};

use bleedthrough_symbols::{FileScopes, FunctionFacts};
use bleedthrough_syntax::{Expr, Param, Span, Stmt};

use super::LowerConfig;
use super::types::{class_names, classify};
use crate::graph::{Block, Capture, Graph, LimitsHit, ParamInfo, Terminator, UnitKey, UnitKind};
use crate::ids::{BlockId, LocalId, ValueId};
use crate::instr::{Instr, InstrKind, Place, TypeClass};
use bleedthrough_symbols::{ExprKey, Fqn};

/// The body a unit runs.
#[derive(Clone, Copy)]
pub(super) enum JobBody<'a> {
    Stmts(&'a [Stmt]),
    /// An arrow function or expression-bodied hook: the expression is returned.
    Expr(&'a Expr),
}

/// A unit found while lowering, waiting to be lowered itself.
pub(super) struct Job<'a> {
    pub kind: UnitKind,
    pub span: Span,
    pub fallback_name: String,
    pub owner: Option<String>,
    pub is_static: bool,
    pub params: &'a [Param],
    /// Parameters the syntax does not list, such as the implicit `$value` of a `set` hook.
    pub implicit_params: Vec<&'static str>,
    pub captures: Vec<(Box<str>, bool)>,
    pub body: JobBody<'a>,
    pub promote: bool,
}

/// What the builder reads and the queue it fills.
pub(super) struct Shared<'a, 'f> {
    pub config: &'f LowerConfig,
    pub scopes: &'f FileScopes,
    pub jobs: &'f mut Vec<Job<'a>>,
}

struct BlockBuf {
    instrs: Vec<Instr>,
    term: Option<Terminator>,
    handlers: Vec<BlockId>,
}

/// A construct a jump may cross or target.
pub(super) enum Scope {
    Loop {
        brk: BlockId,
        cont: BlockId,
    },
    Finally {
        entry: BlockId,
        pending: Vec<BlockId>,
    },
}

/// Which loop target a jump wants.
#[derive(Clone, Copy)]
pub(super) enum JumpKind {
    Break,
    Continue,
}

pub(super) struct Fb<'a, 'f, 's> {
    pub sh: &'s mut Shared<'a, 'f>,
    pub facts: Option<&'f FunctionFacts>,
    blocks: Vec<BlockBuf>,
    pub cur: BlockId,
    locals: Vec<Box<str>>,
    local_index: BTreeMap<Box<str>, LocalId>,
    values: u32,
    instr_count: usize,
    pub scopes: Vec<Scope>,
    pub handlers: Vec<BlockId>,
    pub labels: BTreeMap<Box<str>, BlockId>,
    pub global_names: BTreeSet<Box<str>>,
    ret_local: Option<LocalId>,
    ret_join: Option<BlockId>,
    synthetic: u32,
    pub depth: usize,
    pub recovered: bool,
    pub unknowns: BTreeMap<Box<str>, u32>,
    pub hit: LimitsHit,
}

impl<'a, 'f, 's> Fb<'a, 'f, 's> {
    pub fn new(sh: &'s mut Shared<'a, 'f>, facts: Option<&'f FunctionFacts>) -> Self {
        let mut fb = Self {
            sh,
            facts,
            blocks: Vec::new(),
            cur: BlockId::new(0),
            locals: Vec::new(),
            local_index: BTreeMap::new(),
            values: 0,
            instr_count: 0,
            scopes: Vec::new(),
            handlers: Vec::new(),
            labels: BTreeMap::new(),
            global_names: BTreeSet::new(),
            ret_local: None,
            ret_join: None,
            synthetic: 0,
            depth: 0,
            recovered: false,
            unknowns: BTreeMap::new(),
            hit: LimitsHit::default(),
        };
        fb.cur = fb.new_block();
        fb
    }

    // ---- blocks ------------------------------------------------------------------------

    pub fn new_block(&mut self) -> BlockId {
        if self.blocks.len() >= self.sh.config.max_blocks && !self.blocks.is_empty() {
            self.hit.blocks = true;
            return BlockId::new(u32::try_from(self.blocks.len() - 1).unwrap_or(u32::MAX));
        }
        let id = BlockId::new(u32::try_from(self.blocks.len()).unwrap_or(u32::MAX));
        self.blocks.push(BlockBuf {
            instrs: Vec::new(),
            term: None,
            handlers: self.handlers.clone(),
        });
        id
    }

    pub fn new_block_without_handlers(&mut self) -> BlockId {
        let id = self.new_block();
        self.blocks[id.index()].handlers.clear();
        id
    }

    pub fn set_handlers(&mut self, block: BlockId, handlers: Vec<BlockId>) {
        self.blocks[block.index()].handlers = handlers;
    }

    pub fn terminate(&mut self, term: Terminator) {
        let block = &mut self.blocks[self.cur.index()];
        if block.term.is_none() {
            block.term = Some(term);
        }
    }

    pub fn goto(&mut self, target: BlockId) {
        self.terminate(Terminator::Goto(target));
    }

    /// Ends the current block with `term` and continues in a fresh block nothing reaches.
    pub fn terminate_into_dead(&mut self, term: Terminator) {
        self.terminate(term);
        self.cur = self.new_block();
    }

    // ---- values and instructions -------------------------------------------------------

    fn fresh_value(&mut self) -> ValueId {
        let id = ValueId::new(self.values);
        self.values += 1;
        id
    }

    fn push(&mut self, instr: Instr) {
        if self.instr_count >= self.sh.config.max_instructions {
            self.hit.instructions = true;
            return;
        }
        self.instr_count += 1;
        let block = &mut self.blocks[self.cur.index()];
        if block.term.is_none() {
            block.instrs.push(instr);
        }
    }

    /// Emits an instruction that defines a value.
    pub fn emit(&mut self, span: Span, ty: TypeClass, kind: InstrKind) -> ValueId {
        let dest = self.fresh_value();
        self.push(Instr {
            span,
            dest: Some(dest),
            ty,
            kind,
        });
        dest
    }

    /// Emits an instruction that defines no value.
    pub fn emit_effect(&mut self, span: Span, kind: InstrKind) {
        self.push(Instr {
            span,
            dest: None,
            ty: TypeClass::Unknown,
            kind,
        });
    }

    /// Emits an unknown instruction and counts it by `kind`.
    pub fn unknown(&mut self, span: Span, kind: &str, operands: Vec<ValueId>) -> ValueId {
        *self.unknowns.entry(kind.into()).or_insert(0) += 1;
        self.emit(
            span,
            TypeClass::Unknown,
            InstrKind::Unknown {
                kind: kind.into(),
                operands,
            },
        )
    }

    // ---- facts -------------------------------------------------------------------------

    pub fn ty_of(&self, expr: &Expr) -> TypeClass {
        self.facts
            .and_then(|f| f.type_of(&ExprKey::of(expr)))
            .map_or(TypeClass::Unknown, classify)
    }

    pub fn classes_of(&self, expr: &Expr) -> Vec<Fqn> {
        self.facts
            .and_then(|f| f.type_of(&ExprKey::of(expr)))
            .map_or_else(Vec::new, class_names)
    }

    // ---- locals ------------------------------------------------------------------------

    pub fn local(&mut self, name: &str) -> LocalId {
        let name = name.strip_prefix('$').unwrap_or(name);
        if let Some(id) = self.local_index.get(name) {
            return *id;
        }
        let id = LocalId::new(u32::try_from(self.locals.len()).unwrap_or(u32::MAX));
        let boxed: Box<str> = name.into();
        self.locals.push(boxed.clone());
        self.local_index.insert(boxed, id);
        id
    }

    pub fn fresh_local(&mut self) -> LocalId {
        let name = format!("%t{}", self.synthetic);
        self.synthetic += 1;
        self.local(&name)
    }

    pub fn ret_local(&mut self) -> LocalId {
        if let Some(id) = self.ret_local {
            return id;
        }
        let id = self.local("%ret");
        self.ret_local = Some(id);
        id
    }

    pub fn load_local(&mut self, span: Span, local: LocalId) -> ValueId {
        self.emit(
            span,
            TypeClass::Unknown,
            InstrKind::Load(Place::Local(local)),
        )
    }

    pub fn store_local(&mut self, span: Span, local: LocalId, value: ValueId) {
        self.emit_effect(
            span,
            InstrKind::Store {
                place: Place::Local(local),
                value,
                weak: false,
            },
        );
    }

    // ---- jumps -------------------------------------------------------------------------

    /// Jumps to the loop `levels` out (1 is the innermost), routing through every `finally`
    /// the jump leaves. A level beyond the loops ends the path.
    pub fn jump(&mut self, kind: JumpKind, levels: usize) {
        let mut seen = 0;
        let mut crossed: Vec<usize> = Vec::new();
        let mut target = None;
        for (index, scope) in self.scopes.iter().enumerate().rev() {
            match scope {
                Scope::Finally { .. } => crossed.push(index),
                Scope::Loop { brk, cont } => {
                    seen += 1;
                    if seen == levels {
                        target = Some(match kind {
                            JumpKind::Break => *brk,
                            JumpKind::Continue => *cont,
                        });
                        break;
                    }
                }
            }
        }
        match target {
            Some(t) => {
                let first = self.route(&crossed, t);
                self.terminate_into_dead(Terminator::Goto(first));
            }
            None => self.terminate_into_dead(Terminator::Unreachable),
        }
    }

    /// Chains `target` behind the finally scopes in `crossed` (innermost first) and returns the
    /// first block to go to.
    fn route(&mut self, crossed: &[usize], target: BlockId) -> BlockId {
        let entries: Vec<BlockId> = crossed
            .iter()
            .filter_map(|i| match &self.scopes[*i] {
                Scope::Finally { entry, .. } => Some(*entry),
                Scope::Loop { .. } => None,
            })
            .collect();
        for (position, scope_index) in crossed.iter().enumerate() {
            let next = entries.get(position + 1).copied().unwrap_or(target);
            if let Scope::Finally { pending, .. } = &mut self.scopes[*scope_index]
                && !pending.contains(&next)
            {
                pending.push(next);
            }
        }
        entries.first().copied().unwrap_or(target)
    }

    /// Returns `value` from the unit, through every `finally` in force.
    pub fn do_return(&mut self, span: Span, value: Option<ValueId>) {
        let crossed: Vec<usize> = self
            .scopes
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, s)| matches!(s, Scope::Finally { .. }))
            .map(|(i, _)| i)
            .collect();
        if crossed.is_empty() {
            self.terminate_into_dead(Terminator::Return(value));
            return;
        }
        if let Some(v) = value {
            let local = self.ret_local();
            self.emit_effect(
                span,
                InstrKind::Store {
                    place: Place::Local(local),
                    value: v,
                    weak: false,
                },
            );
        }
        let join = self.return_join(value.is_some(), span);
        let first = self.route(&crossed, join);
        self.terminate_into_dead(Terminator::Goto(first));
    }

    fn return_join(&mut self, with_value: bool, span: Span) -> BlockId {
        if let Some(block) = self.ret_join {
            return block;
        }
        let saved = self.cur;
        let join = self.new_block_without_handlers();
        self.cur = join;
        let value = with_value.then(|| {
            let local = self.ret_local();
            self.load_local(span, local)
        });
        self.terminate(Terminator::Return(value));
        self.cur = saved;
        self.ret_join = Some(join);
        join
    }

    // ---- finishing ---------------------------------------------------------------------

    pub fn finish(
        mut self,
        job: &Job<'_>,
        key: UnitKey,
        params: Vec<ParamInfo>,
        captures: Vec<Capture>,
    ) -> Graph {
        self.terminate(Terminator::Return(None));
        let exit = self.new_block_without_handlers();
        self.cur = exit;
        self.terminate(Terminator::Exit);
        let blocks: Vec<Block> = self
            .blocks
            .into_iter()
            .map(|b| Block {
                instrs: b.instrs,
                term: b.term.unwrap_or(Terminator::Unreachable),
                handlers: b.handlers,
            })
            .collect();
        Graph {
            key,
            kind: job.kind,
            owner: job.owner.as_deref().map(Into::into),
            is_static: job.is_static,
            params,
            captures,
            locals: self.locals,
            blocks,
            entry: BlockId::new(0),
            exit,
            value_count: self.values,
            recovered: self.recovered,
            unknowns: self.unknowns,
            limits: self.hit,
        }
    }
}
