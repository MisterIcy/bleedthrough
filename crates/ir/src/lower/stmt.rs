//! Statement lowering.

use bleedthrough_syntax::{
    BinaryOp, Catch, ClassLike, ElseIf, Expr, ExprKind, FunctionLike, HookBody, Literal,
    MemberKind, Modifier, NodeRef, Span, Stmt, StmtKind, SwitchCase,
};

use super::builder::{Fb, Job, JobBody, JumpKind, Scope};
use super::captures::flatten_variables;
use crate::graph::{Terminator, UnitKind};
use crate::instr::{InstrKind, IterPart, Operand, Place, TestKind, TypeClass};

impl<'a> Fb<'a, '_, '_> {
    pub fn stmts(&mut self, stmts: &'a [Stmt]) {
        for stmt in stmts {
            self.stmt(stmt);
        }
    }

    pub fn stmt(&mut self, stmt: &'a Stmt) {
        if self.depth >= self.sh.config.max_depth {
            self.hit.depth = true;
            let mut operands = Vec::new();
            for (name, span) in flatten_variables(NodeRef::Stmt(stmt)) {
                let local = self.local(&name);
                operands.push(self.emit(
                    span,
                    TypeClass::Unknown,
                    InstrKind::Load(Place::Local(local)),
                ));
            }
            self.unknown(stmt.span, "depth-limit", operands);
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
                for value in values {
                    let v = self.expr(value);
                    self.emit_effect(
                        value.span,
                        InstrKind::Echo(Operand {
                            value: v,
                            span: value.span,
                        }),
                    );
                }
            }
            StmtKind::Return(value) => {
                let v = value.as_ref().map(|x| self.expr(x));
                self.do_return(stmt.span, v);
            }
            StmtKind::Block(body) => self.stmts(body),
            StmtKind::Namespace { body, .. } => self.stmts(body),
            StmtKind::Declare { body, .. } => {
                if let Some(body) = body {
                    self.stmts(body);
                }
            }
            StmtKind::If {
                condition,
                then,
                else_ifs,
                otherwise,
            } => self.if_stmt(condition, then, else_ifs, otherwise.as_deref()),
            StmtKind::While { condition, body } => self.while_loop(condition, body),
            StmtKind::DoWhile { body, condition } => self.do_while(body, condition),
            StmtKind::For {
                init,
                condition,
                step,
                body,
            } => self.for_loop(init, condition, step, body),
            StmtKind::Foreach {
                subject,
                key,
                value,
                body,
            } => self.foreach(subject, key.as_ref(), value, body),
            StmtKind::Switch { subject, cases } => self.switch(subject, cases),
            StmtKind::Break(level) => self.jump_stmt(JumpKind::Break, level.as_ref()),
            StmtKind::Continue(level) => self.jump_stmt(JumpKind::Continue, level.as_ref()),
            StmtKind::Try {
                body,
                catches,
                finally,
            } => self.try_stmt(body, catches, finally.as_deref()),
            StmtKind::Goto(label) => {
                let target = self.label_block(label);
                self.terminate_into_dead(Terminator::Goto(target));
            }
            StmtKind::Label(label) => {
                let target = self.label_block(label);
                self.goto(target);
                self.cur = target;
            }
            StmtKind::Global(names) => {
                for name in names {
                    if let ExprKind::Variable(n) = &name.kind {
                        self.global_names
                            .insert(n.strip_prefix('$').unwrap_or(n).into());
                    }
                }
            }
            StmtKind::StaticVars(vars) => {
                for var in vars {
                    let init = match &var.init {
                        Some(e) => self.expr(e),
                        None => self.null(var.span),
                    };
                    let local = self.local(&var.name);
                    self.store_local(var.span, local, init);
                }
            }
            StmtKind::Unset(targets) => {
                for target in targets {
                    self.unset(target);
                }
            }
            StmtKind::Function(f) => self.declare_function(stmt.span, f),
            StmtKind::ClassLike(class) => self.declare_class(stmt.span, class),
            StmtKind::Error { .. } => {
                self.recovered = true;
                self.unknown(stmt.span, "error", Vec::new());
            }
            StmtKind::Unsupported { kind } => {
                self.unknown(stmt.span, &format!("unsupported:{kind}"), Vec::new());
            }
            StmtKind::OpenTag { .. }
            | StmtKind::CloseTag
            | StmtKind::InlineHtml { .. }
            | StmtKind::Use { .. }
            | StmtKind::Const { .. }
            | StmtKind::HaltCompiler
            | StmtKind::Noop => {}
            _ => {
                self.unknown(stmt.span, "unmodelled-statement", Vec::new());
            }
        }
    }

    fn unset(&mut self, target: &'a Expr) {
        match &target.kind {
            ExprKind::Variable(_) => {
                let (place, _) = self.place_of(target);
                let n = self.null(target.span);
                self.emit_effect(
                    target.span,
                    InstrKind::Store {
                        place,
                        value: n,
                        weak: false,
                    },
                );
            }
            _ => {
                self.place_of(target);
            }
        }
    }

    fn label_block(&mut self, label: &str) -> crate::BlockId {
        if let Some(block) = self.labels.get(label) {
            return *block;
        }
        let block = self.new_block();
        self.labels.insert(label.into(), block);
        block
    }

    // ---- structured control flow --------------------------------------------------------

    fn if_stmt(
        &mut self,
        condition: &'a Expr,
        then: &'a [Stmt],
        else_ifs: &'a [ElseIf],
        otherwise: Option<&'a [Stmt]>,
    ) {
        let join = self.new_block();
        let mut cond = condition;
        let mut body = then;
        let mut rest = else_ifs.iter();
        loop {
            let c = self.expr(cond);
            let yes = self.new_block();
            let no = self.new_block();
            self.terminate(Terminator::Branch {
                cond: c,
                then: yes,
                otherwise: no,
            });
            self.cur = yes;
            self.stmts(body);
            self.goto(join);
            self.cur = no;
            match rest.next() {
                Some(next) => {
                    cond = &next.condition;
                    body = &next.body;
                }
                None => break,
            }
        }
        if let Some(rest) = otherwise {
            self.stmts(rest);
        }
        self.goto(join);
        self.cur = join;
    }

    fn while_loop(&mut self, condition: &'a Expr, body: &'a [Stmt]) {
        let head = self.new_block();
        let inside = self.new_block();
        let exit = self.new_block();
        self.goto(head);
        self.cur = head;
        let c = self.expr(condition);
        self.terminate(Terminator::Branch {
            cond: c,
            then: inside,
            otherwise: exit,
        });
        self.cur = inside;
        self.scopes.push(Scope::Loop {
            brk: exit,
            cont: head,
        });
        self.stmts(body);
        self.scopes.pop();
        self.goto(head);
        self.cur = exit;
    }

    fn do_while(&mut self, body: &'a [Stmt], condition: &'a Expr) {
        let inside = self.new_block();
        let test = self.new_block();
        let exit = self.new_block();
        self.goto(inside);
        self.cur = inside;
        self.scopes.push(Scope::Loop {
            brk: exit,
            cont: test,
        });
        self.stmts(body);
        self.scopes.pop();
        self.goto(test);
        self.cur = test;
        let c = self.expr(condition);
        self.terminate(Terminator::Branch {
            cond: c,
            then: inside,
            otherwise: exit,
        });
        self.cur = exit;
    }

    fn for_loop(
        &mut self,
        init: &'a [Expr],
        condition: &'a [Expr],
        step: &'a [Expr],
        body: &'a [Stmt],
    ) {
        for e in init {
            self.expr(e);
        }
        let head = self.new_block();
        let inside = self.new_block();
        let advance = self.new_block();
        let exit = self.new_block();
        self.goto(head);
        self.cur = head;
        let mut last = None;
        for e in condition {
            last = Some(self.expr(e));
        }
        match last {
            Some(c) => self.terminate(Terminator::Branch {
                cond: c,
                then: inside,
                otherwise: exit,
            }),
            None => self.goto(inside),
        }
        self.cur = inside;
        self.scopes.push(Scope::Loop {
            brk: exit,
            cont: advance,
        });
        self.stmts(body);
        self.scopes.pop();
        self.goto(advance);
        self.cur = advance;
        for e in step {
            self.expr(e);
        }
        self.goto(head);
        self.cur = exit;
    }

    fn foreach(
        &mut self,
        subject: &'a Expr,
        key: Option<&'a Expr>,
        value: &'a Expr,
        body: &'a [Stmt],
    ) {
        let s = self.expr(subject);
        let head = self.new_block();
        let inside = self.new_block();
        let exit = self.new_block();
        self.goto(head);
        self.cur = head;
        let more = self.test(subject.span, TestKind::Synthetic, vec![s]);
        self.terminate(Terminator::Branch {
            cond: more,
            then: inside,
            otherwise: exit,
        });
        self.cur = inside;
        let item = self.emit(
            value.span,
            TypeClass::Unknown,
            InstrKind::Iter {
                subject: s,
                part: IterPart::Value,
            },
        );
        self.assign_to(value, item);
        if let Some(k) = key {
            let key_value = self.emit(
                k.span,
                TypeClass::Unknown,
                InstrKind::Iter {
                    subject: s,
                    part: IterPart::Key,
                },
            );
            self.assign_to(k, key_value);
        }
        self.scopes.push(Scope::Loop {
            brk: exit,
            cont: head,
        });
        self.stmts(body);
        self.scopes.pop();
        self.goto(head);
        self.cur = exit;
    }

    fn switch(&mut self, subject: &'a Expr, cases: &'a [SwitchCase]) {
        let s = self.expr(subject);
        let end = self.new_block();
        let bodies: Vec<crate::BlockId> = cases.iter().map(|_| self.new_block()).collect();
        for (case, body) in cases.iter().zip(&bodies) {
            let Some(test) = &case.test else { continue };
            let t = self.expr(test);
            let eq = self.emit(
                test.span,
                TypeClass::Inert,
                InstrKind::Binary {
                    op: BinaryOp::Eq,
                    lhs: s,
                    rhs: t,
                },
            );
            let next = self.new_block();
            self.terminate(Terminator::Branch {
                cond: eq,
                then: *body,
                otherwise: next,
            });
            self.cur = next;
        }
        let default = cases
            .iter()
            .zip(&bodies)
            .find(|(c, _)| c.test.is_none())
            .map(|(_, b)| *b);
        self.goto(default.unwrap_or(end));
        self.scopes.push(Scope::Loop {
            brk: end,
            cont: end,
        });
        for (index, case) in cases.iter().enumerate() {
            self.cur = bodies[index];
            self.stmts(&case.body);
            self.goto(bodies.get(index + 1).copied().unwrap_or(end));
        }
        self.scopes.pop();
        self.cur = end;
    }

    fn jump_stmt(&mut self, kind: JumpKind, level: Option<&'a Expr>) {
        let levels = level.map_or(1, |e| match &e.kind {
            ExprKind::Literal(Literal::Int { raw }) => {
                raw.parse::<usize>().unwrap_or(usize::MAX).max(1)
            }
            _ => 1,
        });
        self.jump(kind, levels);
    }

    fn try_stmt(&mut self, body: &'a [Stmt], catches: &'a [Catch], finally: Option<&'a [Stmt]>) {
        let after = self.new_block();
        let outer = self.handlers.clone();
        let fin_entry = finally.map(|_| self.new_block());
        let catch_blocks: Vec<crate::BlockId> = catches.iter().map(|_| self.new_block()).collect();
        let continue_to = fin_entry.unwrap_or(after);

        let mut body_handlers = catch_blocks.clone();
        match fin_entry {
            Some(f) => body_handlers.push(f),
            None => body_handlers.extend(outer.iter().copied()),
        }
        if let Some(entry) = fin_entry {
            self.scopes.push(Scope::Finally {
                entry,
                pending: Vec::new(),
            });
        }
        self.handlers = body_handlers;
        let start = self.new_block();
        self.goto(start);
        self.cur = start;
        self.stmts(body);
        self.goto(continue_to);

        self.handlers = fin_entry.map_or_else(|| outer.clone(), |f| vec![f]);
        for (catch, block) in catches.iter().zip(&catch_blocks) {
            self.set_handlers(*block, self.handlers.clone());
            self.cur = *block;
            if let Some(var) = &catch.variable {
                let local = self.local(var);
                let n = self.null(catch.span);
                self.store_local(catch.span, local, n);
            }
            self.stmts(&catch.body);
            self.goto(continue_to);
        }
        self.handlers = outer.clone();

        if let (Some(entry), Some(fin_body)) = (fin_entry, finally) {
            let pending = match self.scopes.pop() {
                Some(Scope::Finally { pending, .. }) => pending,
                _ => Vec::new(),
            };
            self.cur = entry;
            self.stmts(fin_body);
            let mut targets = vec![after];
            for t in pending.into_iter().chain(outer) {
                if !targets.contains(&t) {
                    targets.push(t);
                }
            }
            self.terminate(Terminator::Choice(targets));
        }
        self.cur = after;
    }

    // ---- declarations -------------------------------------------------------------------

    fn declare_function(&mut self, span: Span, f: &'a FunctionLike) {
        let Some(body) = &f.body else { return };
        let name = self.sh.scopes.at(span.start).qualify(&f.name);
        self.sh.jobs.push(Job {
            kind: UnitKind::Function,
            span,
            fallback_name: name.as_str().to_string(),
            owner: None,
            is_static: true,
            params: &f.params,
            implicit_params: Vec::new(),
            captures: Vec::new(),
            body: JobBody::Stmts(body),
            promote: false,
        });
    }

    fn declare_class(&mut self, span: Span, class: &'a ClassLike) {
        let Some(short) = &class.name else { return };
        let fqn = self.sh.scopes.at(span.start).qualify(short);
        let owner = fqn.as_str().to_string();
        for member in &class.members {
            match &member.kind {
                MemberKind::Method(m) => {
                    if let Some(body) = &m.body {
                        self.sh.jobs.push(Job {
                            kind: UnitKind::Method,
                            span: member.span,
                            fallback_name: format!("{owner}::{}", m.name),
                            owner: Some(owner.clone()),
                            is_static: m.modifiers.contains(&Modifier::Static),
                            params: &m.params,
                            implicit_params: Vec::new(),
                            captures: Vec::new(),
                            body: JobBody::Stmts(body),
                            promote: m.name.eq_ignore_ascii_case("__construct"),
                        });
                    }
                    for param in &m.params {
                        self.hooks(&owner, &param.name, &param.hooks);
                    }
                }
                MemberKind::Property { items, hooks, .. } => {
                    for item in items {
                        self.hooks(&owner, &item.name, hooks);
                    }
                }
                _ => {}
            }
        }
    }

    fn hooks(
        &mut self,
        owner: &str,
        property: &str,
        hooks: &'a [bleedthrough_syntax::PropertyHook],
    ) {
        for hook in hooks {
            let body = match &hook.body {
                HookBody::Block(stmts) => JobBody::Stmts(stmts),
                HookBody::Expr(e) => JobBody::Expr(e),
                _ => continue,
            };
            let implicit = if hook.name.eq_ignore_ascii_case("set") && hook.params.is_empty() {
                vec!["value"]
            } else {
                Vec::new()
            };
            self.sh.jobs.push(Job {
                kind: UnitKind::Hook,
                span: hook.span,
                fallback_name: format!(
                    "{owner}::${}::{}",
                    property.trim_start_matches('$'),
                    hook.name
                ),
                owner: Some(owner.to_string()),
                is_static: false,
                params: &hook.params,
                implicit_params: implicit,
                captures: Vec::new(),
                body,
                promote: false,
            });
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "lowering threads the AST pieces it was handed"
    )]
    pub fn enqueue_closure(
        &mut self,
        e: &'a Expr,
        kind: UnitKind,
        params: &'a [bleedthrough_syntax::Param],
        captures: Vec<(Box<str>, bool)>,
        is_static: bool,
        body: JobBody<'a>,
        name: String,
    ) {
        self.sh.jobs.push(Job {
            kind,
            span: e.span,
            fallback_name: name,
            owner: None,
            is_static,
            params,
            implicit_params: Vec::new(),
            captures,
            body,
            promote: false,
        });
    }
}
