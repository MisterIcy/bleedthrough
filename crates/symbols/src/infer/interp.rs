//! The interpreter: abstract interpretation of one body over the owned syntax tree. Statements
//! run in order, branches fork and join, loops iterate to a bounded fixed point.

use std::collections::BTreeMap;

use bleedthrough_syntax::{
    Catch, ElseIf, Expr, ExprKind, Literal, NodeRef, Param, Span, Stmt, StmtKind, SwitchCase,
    Visitor, walk,
};

use super::InferLimits;
use super::ctx::Ctx;
use super::env::Env;
use super::facts::{CallResolution, ExprKey, FunctionFacts, InferStats};
use super::narrow::narrow;
use crate::native::native_type;
use crate::types::Type;

/// What a unit runs.
#[derive(Clone, Copy)]
pub(super) enum Body<'a> {
    Stmts(&'a [Stmt]),
    Expr(&'a Expr),
}

/// A body waiting to be analysed: its parameters, the state it starts in and where it is.
pub(super) struct Unit<'a> {
    pub name: String,
    pub span: Span,
    pub params: &'a [Param],
    pub param_types: Vec<Type>,
    pub body: Body<'a>,
    pub env: Env,
    /// How many closures deep this unit is.
    pub depth: usize,
}

/// One trip through a loop: the back-edge state and the state when the loop is left.
type Trip<'t, 'a> = dyn FnMut(&mut Interp<'a>) -> (Env, Env) + 't;

#[derive(Default)]
struct Frame {
    breaks: Vec<Env>,
    continues: Vec<Env>,
    switch: bool,
}

pub(super) struct Interp<'a> {
    pub ctx: Ctx<'a>,
    pub limits: &'a InferLimits,
    pub env: Env,
    types: BTreeMap<ExprKey, Type>,
    calls: BTreeMap<ExprKey, CallResolution>,
    frames: Vec<Frame>,
    quiet: u32,
    steps: usize,
    pub depth: usize,
    stats: InferStats,
    pub pending: Vec<Unit<'a>>,
    pub unit_depth: usize,
}

struct Collect<'k>(&'k mut Vec<ExprKey>);

impl Visitor for Collect<'_> {
    fn enter(&mut self, node: NodeRef<'_>) -> bool {
        if let NodeRef::Expr(e) = node {
            self.0.push(ExprKey::of(e));
        }
        true
    }
}

/// Analyses one unit and returns its facts plus the closures it created.
pub(super) fn run_unit<'a>(
    ctx: Ctx<'a>,
    limits: &'a InferLimits,
    unit: Unit<'a>,
) -> (FunctionFacts, Vec<Unit<'a>>) {
    let mut interp = Interp {
        ctx,
        limits,
        env: unit.env,
        types: BTreeMap::new(),
        calls: BTreeMap::new(),
        frames: Vec::new(),
        quiet: 0,
        steps: 0,
        depth: 0,
        stats: InferStats::default(),
        pending: Vec::new(),
        unit_depth: unit.depth,
    };
    for (param, ty) in unit.params.iter().zip(unit.param_types) {
        interp.env.set(param.name.trim_start_matches('$'), ty);
        if let Some(default) = &param.default {
            interp.expr(default);
        }
    }
    match unit.body {
        Body::Stmts(stmts) => interp.block(stmts),
        Body::Expr(expr) => {
            interp.expr(expr);
        }
    }
    let facts = FunctionFacts::assemble(
        unit.name,
        unit.span,
        interp.types,
        interp.calls,
        interp.stats,
    );
    (facts, interp.pending)
}

impl<'a> Interp<'a> {
    // ---- recording and budgets -------------------------------------------------------------

    pub fn record_key(&mut self, key: ExprKey, ty: &Type) {
        if self.quiet > 0 {
            return;
        }
        self.types
            .entry(key)
            .and_modify(|t| *t = t.union(ty))
            .or_insert_with(|| ty.clone());
    }

    pub fn record(&mut self, expr: &Expr, ty: &Type) {
        self.record_key(ExprKey::of(expr), ty);
    }

    pub fn record_call(&mut self, expr: &Expr, resolution: CallResolution) {
        if self.quiet > 0 {
            return;
        }
        let key = ExprKey::of(expr);
        let merged = match self.calls.get(&key) {
            Some(previous) => previous.merge(&resolution),
            None => resolution,
        };
        self.calls.insert(key, merged);
    }

    /// Runs `f` without recording anything or creating closure units.
    pub fn quietly<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        self.quiet += 1;
        let result = f(self);
        self.quiet -= 1;
        result
    }

    pub fn is_quiet(&self) -> bool {
        self.quiet > 0
    }

    /// Counts one unit of work; false when the budget is spent.
    fn tick(&mut self) -> bool {
        self.steps += 1;
        if self.steps > self.limits.step_budget {
            self.stats.budget_exhausted = true;
            false
        } else {
            true
        }
    }

    /// Whether another nesting level may be entered.
    pub fn enter(&mut self) -> bool {
        if self.depth >= self.limits.max_depth {
            self.stats.depth_limit_hit = true;
            return false;
        }
        self.tick()
    }

    /// Records `unknown` for every expression in the subtree of `node`.
    pub fn fill_unknown(&mut self, node: NodeRef<'_>) {
        let mut keys = Vec::new();
        walk(node, &mut Collect(&mut keys));
        let unknown = Type::unknown();
        for key in keys {
            self.record_key(key, &unknown);
        }
    }

    /// Records `unknown` for the expressions below `expr`, not for `expr` itself.
    pub fn fill_children(&mut self, expr: &Expr) {
        for child in NodeRef::Expr(expr).children() {
            self.fill_unknown(child);
        }
    }

    // ---- statements ------------------------------------------------------------------------

    pub fn block(&mut self, stmts: &'a [Stmt]) {
        for stmt in stmts {
            self.stmt(stmt);
        }
    }

    fn stmt(&mut self, stmt: &'a Stmt) {
        if !self.enter() {
            self.fill_unknown(NodeRef::Stmt(stmt));
            return;
        }
        self.depth += 1;
        self.stmt_inner(stmt);
        self.depth -= 1;
    }

    #[allow(
        clippy::too_many_lines,
        reason = "a dispatch over every statement kind"
    )]
    fn stmt_inner(&mut self, stmt: &'a Stmt) {
        match &stmt.kind {
            StmtKind::Expr(e) => {
                self.expr(e);
            }
            StmtKind::Echo { values, .. } => {
                for v in values {
                    self.expr(v);
                }
            }
            StmtKind::Return(value) => {
                if let Some(v) = value {
                    self.expr(v);
                }
                self.env.dead = true;
            }
            StmtKind::Block(body) | StmtKind::Namespace { body, .. } => self.block(body),
            StmtKind::Declare { directives, body } => {
                for d in directives {
                    self.expr(&d.value);
                }
                if let Some(body) = body {
                    self.block(body);
                }
            }
            StmtKind::Const { items, .. } => {
                for item in items {
                    self.expr(&item.value);
                }
            }
            StmtKind::If {
                condition,
                then,
                else_ifs,
                otherwise,
            } => self.if_stmt(condition, then, else_ifs, otherwise.as_deref()),
            StmtKind::While { condition, body } => self.while_stmt(condition, body),
            StmtKind::DoWhile { body, condition } => self.do_while_stmt(body, condition),
            StmtKind::For {
                init,
                condition,
                step,
                body,
            } => self.for_stmt(init, condition, step, body),
            StmtKind::Foreach {
                subject,
                key,
                value,
                body,
            } => self.foreach_stmt(subject, key.as_ref(), value, body),
            StmtKind::Switch { subject, cases } => self.switch_stmt(subject, cases),
            StmtKind::Break(levels) => self.jump(levels.as_ref(), true),
            StmtKind::Continue(levels) => self.jump(levels.as_ref(), false),
            StmtKind::Try {
                body,
                catches,
                finally,
            } => self.try_stmt(body, catches, finally.as_deref()),
            StmtKind::Global(vars) => {
                for v in vars {
                    self.fill_unknown(NodeRef::Expr(v));
                    if let ExprKind::Variable(name) = &v.kind {
                        self.env.set(name.trim_start_matches('$'), Type::unknown());
                    }
                }
            }
            StmtKind::StaticVars(vars) => {
                for var in vars {
                    let ty = match &var.init {
                        Some(init) => self.expr(init),
                        None => Type::null(),
                    };
                    self.env.set(var.name.trim_start_matches('$'), ty);
                }
            }
            StmtKind::Unset(targets) => {
                for t in targets {
                    self.expr(t);
                    if let ExprKind::Variable(name) = &t.kind {
                        self.env.remove(name.trim_start_matches('$'));
                    }
                }
            }
            // Declarations are analysed as units of their own; the rest has no expressions.
            _ => {}
        }
    }

    fn if_stmt(
        &mut self,
        condition: &'a Expr,
        then: &'a [Stmt],
        else_ifs: &'a [ElseIf],
        otherwise: Option<&'a [Stmt]>,
    ) {
        self.expr(condition);
        let (yes, no) = narrow(&self.ctx, condition, &self.env);
        let mut ends = Vec::new();
        self.env = yes;
        self.block(then);
        ends.push(self.env.clone());
        let mut rest = no;
        for branch in else_ifs {
            self.env = rest;
            self.expr(&branch.condition);
            let (yes, no) = narrow(&self.ctx, &branch.condition, &self.env);
            self.env = yes;
            self.block(&branch.body);
            ends.push(self.env.clone());
            rest = no;
        }
        self.env = rest;
        if let Some(body) = otherwise {
            self.block(body);
        }
        ends.push(self.env.clone());
        self.env = Env::join_all(ends.iter());
    }

    // ---- loops -----------------------------------------------------------------------------

    /// The current state joined with the `continue` states of the innermost loop.
    pub fn join_continues(&mut self) -> Env {
        let others = self.take_continues();
        others.iter().fold(self.env.clone(), |acc, o| acc.join(o))
    }

    /// Takes the `continue` states of the innermost loop.
    fn take_continues(&mut self) -> Vec<Env> {
        self.frames
            .last_mut()
            .map(|f| std::mem::take(&mut f.continues))
            .unwrap_or_default()
    }

    /// Runs a loop to a fixed point. `trip` starts from the loop-head state in `self.env`, runs
    /// the condition, body and step once, and returns the back-edge state and the state when the
    /// loop is left through its condition. Returns the state after the loop.
    fn fixpoint(&mut self, entry: &Env, trip: &mut Trip<'_, 'a>) -> Env {
        self.frames.push(Frame::default());
        let mut head = entry.clone();
        let mut previous = entry.clone();
        let mut exits: Vec<Env> = Vec::new();
        let mut converged = false;
        for pass in 0..self.limits.loop_passes.max(1) {
            self.env = head.clone();
            let (back, exit) = trip(self);
            exits.push(exit);
            if let Some(frame) = self.frames.last_mut() {
                exits.append(&mut frame.breaks);
            }
            let mut next = entry.join(&back);
            if next.is_within(&head) {
                converged = true;
                break;
            }
            if pass >= 1 {
                next.generalise_changes(&head);
            }
            previous = std::mem::replace(&mut head, next);
        }
        if !converged {
            self.stats.loop_limit_hits += 1;
            let mut wide = head.clone();
            wide.widen_changes(&previous);
            self.env = wide;
            let (_, exit) = trip(self);
            exits.push(exit);
            if let Some(frame) = self.frames.last_mut() {
                exits.append(&mut frame.breaks);
            }
        }
        self.frames.pop();
        Env::join_all(exits.iter())
    }

    fn while_stmt(&mut self, condition: &'a Expr, body: &'a [Stmt]) {
        let entry = self.env.clone();
        let exit = self.fixpoint(&entry, &mut |me| {
            me.expr(condition);
            let (yes, no) = narrow(&me.ctx, condition, &me.env);
            me.env = yes;
            me.block(body);
            let back = me.join_continues();
            (back, no)
        });
        self.env = exit;
    }

    fn do_while_stmt(&mut self, body: &'a [Stmt], condition: &'a Expr) {
        let entry = self.env.clone();
        let exit = self.fixpoint(&entry, &mut |me| {
            me.block(body);
            me.env = me.join_continues();
            me.expr(condition);
            let (yes, no) = narrow(&me.ctx, condition, &me.env);
            (yes, no)
        });
        self.env = exit;
    }

    fn for_stmt(
        &mut self,
        init: &'a [Expr],
        condition: &'a [Expr],
        step: &'a [Expr],
        body: &'a [Stmt],
    ) {
        for e in init {
            self.expr(e);
        }
        let entry = self.env.clone();
        let exit = self.fixpoint(&entry, &mut |me| {
            let mut last = None;
            for c in condition {
                me.expr(c);
                last = Some(c);
            }
            let (yes, no) = match last {
                Some(c) => narrow(&me.ctx, c, &me.env),
                None => (me.env.clone(), Env::unreachable()),
            };
            me.env = yes;
            me.block(body);
            me.env = me.join_continues();
            for s in step {
                me.expr(s);
            }
            (me.env.clone(), no)
        });
        self.env = exit;
    }

    fn foreach_stmt(
        &mut self,
        subject: &'a Expr,
        key: Option<&'a Expr>,
        value: &'a Expr,
        body: &'a [Stmt],
    ) {
        let subject_ty = self.expr(subject);
        let (key_ty, value_ty) = self.iteration_types(&subject_ty);
        let entry = self.env.clone();
        let exit = self.fixpoint(&entry, &mut |me| {
            let head = me.env.clone();
            if let Some(k) = key {
                me.assign_target(k, key_ty.clone());
            }
            me.assign_target(value, value_ty.clone());
            me.block(body);
            let back = me.join_continues();
            (back, head)
        });
        self.env = exit;
    }

    fn jump(&mut self, levels: Option<&'a Expr>, is_break: bool) {
        let count = levels
            .and_then(|e| {
                self.expr(e);
                match &e.kind {
                    ExprKind::Literal(Literal::Int { raw }) => raw.parse::<usize>().ok(),
                    _ => None,
                }
            })
            .unwrap_or(1)
            .max(1);
        if !self.frames.is_empty() {
            let index = self.frames.len().saturating_sub(count);
            let snapshot = self.env.clone();
            let frame = &mut self.frames[index];
            if is_break || frame.switch {
                frame.breaks.push(snapshot);
            } else {
                frame.continues.push(snapshot);
            }
        }
        self.env.dead = true;
    }

    fn switch_stmt(&mut self, subject: &'a Expr, cases: &'a [SwitchCase]) {
        self.expr(subject);
        let after_subject = self.env.clone();
        self.frames.push(Frame {
            switch: true,
            ..Frame::default()
        });
        let mut fall = Env::unreachable();
        let mut has_default = false;
        for case in cases {
            self.env = after_subject.clone();
            match &case.test {
                Some(test) => {
                    self.expr(test);
                }
                None => has_default = true,
            }
            self.env = self.env.join(&fall);
            self.block(&case.body);
            fall = self.env.clone();
        }
        let frame = self.frames.pop().unwrap_or_default();
        let mut exits = frame.breaks;
        exits.push(fall);
        if !has_default {
            exits.push(after_subject);
        }
        self.env = Env::join_all(exits.iter());
    }

    fn try_stmt(&mut self, body: &'a [Stmt], catches: &'a [Catch], finally: Option<&'a [Stmt]>) {
        let mut anywhere = self.env.clone();
        for stmt in body {
            self.stmt(stmt);
            anywhere = anywhere.join(&self.env);
        }
        let mut normal = vec![self.env.clone()];
        let mut all = vec![anywhere.clone()];
        for catch in catches {
            self.env = anywhere.clone();
            if let Some(var) = &catch.variable {
                let scope = self.ctx.scope_at(catch.span);
                let ty = native_type(&catch.types, scope, self.ctx.class);
                self.env.set(var.trim_start_matches('$'), ty);
            }
            self.block(&catch.body);
            normal.push(self.env.clone());
            all.push(self.env.clone());
        }
        let after = Env::join_all(normal.iter());
        match finally {
            Some(body) => {
                self.env = Env::join_all(all.iter().chain(normal.iter()));
                self.block(body);
                if after.dead {
                    self.env.dead = true;
                }
            }
            None => self.env = after,
        }
    }
}
