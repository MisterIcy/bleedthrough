//! Expression lowering.

use bleedthrough_symbols::{CallResolution, ExprKey, Target, TargetKind};
use bleedthrough_syntax::{
    ArgKind, ArrayItemKind, AssignOp, BinaryOp, CallTarget, Expr, ExprKind, IncludeKind, Literal,
    MatchArm, MemberName, Name, NameKind, NodeRef, Span, StringKind, StringPartKind, UnaryOp,
};

use super::builder::{Fb, JobBody};
use super::captures::{arrow_captures, flatten_variables};
use super::types::constant;
use crate::graph::{Terminator, UnitKind};
use crate::ids::{LocalId, ValueId};
use crate::instr::{
    ArrayElem, CallArg, Callee, CaptureBinding, ClassForm, ClassOperand, ConstValue, InstrKind,
    InterpPart, MemberRef, Operand, Place, Resolution, TestKind, TypeClass,
};

const SUPERGLOBALS: [&str; 8] = [
    "_GET", "_POST", "_COOKIE", "_FILES", "_REQUEST", "_SERVER", "_ENV", "_SESSION",
];

fn plain(name: &str) -> &str {
    name.strip_prefix('$').unwrap_or(name)
}

fn is_superglobal(name: &str) -> bool {
    SUPERGLOBALS.contains(&name)
}

fn short_circuit(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::And
            | BinaryOp::Or
            | BinaryOp::LowAnd
            | BinaryOp::LowOr
            | BinaryOp::Coalesce
            | BinaryOp::Instanceof
    )
}

fn assign_binary(op: AssignOp) -> Option<BinaryOp> {
    Some(match op {
        AssignOp::Add => BinaryOp::Add,
        AssignOp::Sub => BinaryOp::Sub,
        AssignOp::Mul => BinaryOp::Mul,
        AssignOp::Div => BinaryOp::Div,
        AssignOp::Mod => BinaryOp::Mod,
        AssignOp::Pow => BinaryOp::Pow,
        AssignOp::Concat => BinaryOp::Concat,
        AssignOp::BitAnd => BinaryOp::BitAnd,
        AssignOp::BitOr => BinaryOp::BitOr,
        AssignOp::BitXor => BinaryOp::BitXor,
        AssignOp::Shl => BinaryOp::Shl,
        AssignOp::Shr => BinaryOp::Shr,
        _ => return None,
    })
}

impl<'a> Fb<'a, '_, '_> {
    /// Lowers an expression and returns the value it produces.
    pub fn expr(&mut self, e: &'a Expr) -> ValueId {
        if self.depth >= self.sh.config.max_depth {
            return self.flatten(e);
        }
        self.depth += 1;
        let value = self.expr_inner(e);
        self.depth -= 1;
        value
    }

    /// The conservative stand-in for an expression nested too deeply: an unknown instruction
    /// whose operands are the variables mentioned inside it.
    fn flatten(&mut self, e: &'a Expr) -> ValueId {
        self.hit.depth = true;
        let mut operands = Vec::new();
        for (name, span) in flatten_variables(NodeRef::Expr(e)) {
            operands.push(self.variable(span, TypeClass::Unknown, &name));
        }
        self.unknown(e.span, "depth-limit", operands)
    }

    #[allow(
        clippy::too_many_lines,
        reason = "a dispatch over every expression kind"
    )]
    fn expr_inner(&mut self, e: &'a Expr) -> ValueId {
        let ty = self.ty_of(e);
        match &e.kind {
            ExprKind::Literal(l) => self.literal(e.span, ty, l),
            ExprKind::Interpolated { kind, parts } => self.interpolated(e, *kind, parts),
            ExprKind::Variable(name) => self.variable(e.span, ty, plain(name)),
            ExprKind::VariableVariable(inner) => {
                let v = self.expr(inner);
                self.unknown(e.span, "variable-variable", vec![v])
            }
            ExprKind::Name(n) => self.emit(
                e.span,
                TypeClass::Data,
                InstrKind::Const(ConstValue::Str(Some(n.text.clone()))),
            ),
            ExprKind::ConstFetch(n) => self.const_fetch(e.span, ty, n),
            ExprKind::ClassRef(kind) => {
                let class = ClassOperand::relative(*kind, self.classes_of(e));
                let name = MemberRef::Named("class".into());
                self.emit(
                    e.span,
                    TypeClass::Data,
                    InstrKind::ClassConst { class, name },
                )
            }
            ExprKind::MagicConstant(kind) => self.emit(e.span, ty, InstrKind::Magic(*kind)),
            ExprKind::Array { items, .. } => self.array(e.span, ty, items),
            ExprKind::Index { array, index } => self.index(e, array, index.as_deref()),
            ExprKind::Property {
                object,
                member,
                nullsafe,
            } => self.property(e, object, member, *nullsafe),
            ExprKind::StaticProperty { .. } => {
                let (place, _) = self.place_of(e);
                self.emit(e.span, ty, InstrKind::Load(place))
            }
            ExprKind::ClassConst { class, member } => {
                let class = self.class_operand(class);
                let name = self.member_ref(member);
                self.emit(e.span, ty, InstrKind::ClassConst { class, name })
            }
            ExprKind::Call { target, args } => self.call(e, target, args),
            ExprKind::CallableRef { target } => self.callable_ref(e, target),
            ExprKind::New { class, args } => {
                let class_op = self.class_operand(class);
                self.new_object(e, class_op, args)
            }
            ExprKind::AnonymousClass { args, class } => {
                if class
                    .members
                    .iter()
                    .any(|m| matches!(m.kind, bleedthrough_syntax::MemberKind::Method(_)))
                {
                    *self
                        .unknowns
                        .entry("anonymous-class-body".into())
                        .or_insert(0) += 1;
                }
                let class_op = ClassOperand {
                    form: ClassForm::Anonymous,
                    classes: Vec::new(),
                };
                self.new_object(e, class_op, args)
            }
            ExprKind::Unary { op, operand } => self.unary(e, ty, *op, operand),
            ExprKind::Cast { kind, operand } => {
                let v = self.expr(operand);
                self.emit(
                    e.span,
                    ty,
                    InstrKind::Cast {
                        kind: *kind,
                        operand: v,
                    },
                )
            }
            ExprKind::Binary { .. } => self.binary(e),
            ExprKind::Assign { op, target, value } => self.assign(e, *op, target, value),
            ExprKind::Conditional {
                condition,
                then,
                otherwise,
            } => self.conditional(e, condition, then.as_deref(), otherwise),
            ExprKind::Match { subject, arms } => self.match_expr(e, subject, arms),
            ExprKind::Closure {
                params,
                uses,
                is_static,
                body,
                ..
            } => {
                let captures: Vec<(Box<str>, bool)> =
                    uses.iter().map(|u| (u.name.clone(), u.by_ref)).collect();
                self.closure(
                    e,
                    UnitKind::Closure,
                    params,
                    captures,
                    *is_static,
                    JobBody::Stmts(body),
                )
            }
            ExprKind::ArrowFn {
                params,
                is_static,
                body,
                ..
            } => {
                let captures = arrow_captures(params, body);
                self.closure(
                    e,
                    UnitKind::ArrowFn,
                    params,
                    captures,
                    *is_static,
                    JobBody::Expr(body),
                )
            }
            ExprKind::Yield { key, value } => {
                let k = key.as_deref().map(|x| self.expr(x));
                let v = value.as_deref().map(|x| self.expr(x));
                self.emit(e.span, ty, InstrKind::Yield { key: k, value: v })
            }
            ExprKind::YieldFrom(inner) => {
                let v = self.expr(inner);
                self.emit(e.span, ty, InstrKind::YieldFrom(v))
            }
            ExprKind::Throw(inner) => {
                let v = self.expr(inner);
                self.terminate_into_dead(Terminator::Throw(v));
                self.null(e.span)
            }
            ExprKind::Clone(inner) => {
                let v = self.expr(inner);
                self.emit(e.span, ty, InstrKind::Copy(v))
            }
            ExprKind::Print(inner) => {
                let v = self.expr(inner);
                self.emit(e.span, TypeClass::Inert, InstrKind::Print(v))
            }
            ExprKind::Include { kind, path } => self.include(e.span, *kind, path),
            ExprKind::Eval(inner) => {
                let v = self.expr(inner);
                self.emit(e.span, ty, InstrKind::Eval(v))
            }
            ExprKind::Exit { args, .. } => {
                let first = args.iter().find_map(|a| match &a.kind {
                    ArgKind::Positional(x) | ArgKind::Spread(x) => Some(x),
                    ArgKind::Named { value, .. } => Some(value),
                    _ => None,
                });
                let v = first.map(|x| self.expr(x));
                self.emit_effect(e.span, InstrKind::Exit(v));
                self.terminate_into_dead(Terminator::Unreachable);
                self.null(e.span)
            }
            ExprKind::Isset(items) => {
                let operands = items.iter().map(|x| self.expr(x)).collect();
                self.test(e.span, TestKind::Isset, operands)
            }
            ExprKind::Empty(inner) => {
                let v = self.expr(inner);
                self.test(e.span, TestKind::Empty, vec![v])
            }
            ExprKind::Pipe { input, callable } => {
                let i = self.expr(input);
                let c = self.expr(callable);
                let arg = CallArg {
                    value: i,
                    name: None,
                    spread: false,
                    span: input.span,
                };
                self.emit(
                    e.span,
                    ty,
                    InstrKind::Call {
                        callee: Callee::Value(c),
                        args: vec![arg],
                        resolution: Resolution::Missing,
                    },
                )
            }
            ExprKind::Parenthesized(inner) => self.expr(inner),
            ExprKind::Error { .. } => {
                self.recovered = true;
                self.unknown(e.span, "error", Vec::new())
            }
            ExprKind::Unsupported { kind } => {
                self.unknown(e.span, &format!("unsupported:{kind}"), Vec::new())
            }
            _ => self.unknown(e.span, "unmodelled-expression", Vec::new()),
        }
    }

    // ---- leaves ------------------------------------------------------------------------

    pub fn null(&mut self, span: Span) -> ValueId {
        self.emit(span, TypeClass::Inert, InstrKind::Const(ConstValue::Null))
    }

    pub fn test(&mut self, span: Span, kind: TestKind, operands: Vec<ValueId>) -> ValueId {
        self.emit(span, TypeClass::Inert, InstrKind::Test { kind, operands })
    }

    fn literal(&mut self, span: Span, ty: TypeClass, literal: &Literal) -> ValueId {
        self.emit(span, ty, InstrKind::Const(constant(literal)))
    }

    fn const_fetch(&mut self, span: Span, ty: TypeClass, name: &Name) -> ValueId {
        let kind = match name.text.to_ascii_lowercase().as_str() {
            "true" => InstrKind::Const(ConstValue::Bool(true)),
            "false" => InstrKind::Const(ConstValue::Bool(false)),
            "null" => InstrKind::Const(ConstValue::Null),
            _ => InstrKind::ConstFetch(name.clone()),
        };
        self.emit(span, ty, kind)
    }

    pub(super) fn variable(&mut self, span: Span, ty: TypeClass, name: &str) -> ValueId {
        if is_superglobal(name) {
            return self.emit(span, ty, InstrKind::Superglobal(name.into()));
        }
        if self.global_names.contains(name) {
            return self.emit(span, ty, InstrKind::Load(Place::Global(name.into())));
        }
        let local = self.local(name);
        self.emit(span, ty, InstrKind::Load(Place::Local(local)))
    }

    fn interpolated(
        &mut self,
        e: &'a Expr,
        kind: StringKind,
        parts: &'a [bleedthrough_syntax::StringPart],
    ) -> ValueId {
        let mut out = Vec::with_capacity(parts.len());
        for part in parts {
            match &part.kind {
                StringPartKind::Text { raw } => out.push(InterpPart::Text {
                    raw: raw.clone(),
                    span: part.span,
                }),
                StringPartKind::Interpolation(inner) => {
                    let value = self.expr(inner);
                    out.push(InterpPart::Value(Operand {
                        value,
                        span: part.span,
                    }));
                }
                _ => {}
            }
        }
        let ty = self.ty_of(e);
        let built = self.emit(e.span, ty, InstrKind::Interp { kind, parts: out });
        if kind != StringKind::ShellExec {
            return built;
        }
        let target = Target {
            kind: TargetKind::Function,
            owner: None,
            name: "shell_exec".into(),
            implementors: Vec::new(),
        };
        let arg = CallArg {
            value: built,
            name: None,
            spread: false,
            span: e.span,
        };
        self.emit(
            e.span,
            ty,
            InstrKind::Call {
                callee: Callee::Function(Name {
                    span: e.span,
                    kind: NameKind::FullyQualified,
                    text: "shell_exec".into(),
                }),
                args: vec![arg],
                resolution: Resolution::Facts(CallResolution::resolved(vec![target])),
            },
        )
    }

    fn array(
        &mut self,
        span: Span,
        ty: TypeClass,
        items: &'a [bleedthrough_syntax::ArrayItem],
    ) -> ValueId {
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            match &item.kind {
                ArrayItemKind::Value(v) => {
                    let value = self.expr(v);
                    out.push(ArrayElem {
                        key: None,
                        value,
                        spread: false,
                    });
                }
                ArrayItemKind::KeyValue { key, value } => {
                    let k = self.expr(key);
                    let v = self.expr(value);
                    out.push(ArrayElem {
                        key: Some(k),
                        value: v,
                        spread: false,
                    });
                }
                ArrayItemKind::Spread(v) => {
                    let value = self.expr(v);
                    out.push(ArrayElem {
                        key: None,
                        value,
                        spread: true,
                    });
                }
                _ => {}
            }
        }
        self.emit(span, ty, InstrKind::Array(out))
    }

    fn index(&mut self, e: &'a Expr, array: &'a Expr, index: Option<&'a Expr>) -> ValueId {
        if let (ExprKind::Variable(name), Some(idx)) = (&array.kind, index)
            && plain(name) == "GLOBALS"
            && let ExprKind::Literal(Literal::String { .. }) = &idx.kind
            && let ConstValue::Str(Some(key)) = constant_of(idx)
        {
            return self.emit(e.span, self.ty_of(e), InstrKind::Load(Place::Global(key)));
        }
        let base = self.expr(array);
        let idx = index.map(|i| self.expr(i));
        self.emit(
            e.span,
            self.ty_of(e),
            InstrKind::LoadElem { base, index: idx },
        )
    }

    // ---- members, classes, places -------------------------------------------------------

    pub fn member_ref(&mut self, member: &'a MemberName) -> MemberRef {
        match member {
            MemberName::Identifier { name, .. } => MemberRef::Named(name.clone()),
            MemberName::Dynamic(inner) => MemberRef::Dynamic(self.expr(inner)),
            _ => MemberRef::Missing,
        }
    }

    pub fn class_operand(&mut self, class: &'a Expr) -> ClassOperand {
        match &class.kind {
            ExprKind::Name(n) => ClassOperand {
                form: ClassForm::Named(n.clone()),
                classes: self.classes_of(class),
            },
            ExprKind::ClassRef(kind) => ClassOperand::relative(*kind, self.classes_of(class)),
            _ => {
                let v = self.expr(class);
                ClassOperand {
                    form: ClassForm::Value(v),
                    classes: self.classes_of(class),
                }
            }
        }
    }

    fn property(
        &mut self,
        e: &'a Expr,
        object: &'a Expr,
        member: &'a MemberName,
        nullsafe: bool,
    ) -> ValueId {
        let obj = self.expr(object);
        let owners = self.classes_of(object);
        let ty = self.ty_of(e);
        let span = e.span;
        let access = |fb: &mut Self| {
            let name = fb.member_ref(member);
            fb.emit(
                span,
                ty,
                InstrKind::Load(Place::Prop {
                    object: obj,
                    name,
                    owners,
                }),
            )
        };
        if nullsafe {
            self.guarded(span, obj, access)
        } else {
            access(self)
        }
    }

    /// Runs `build` only on the path where `guard` is not null; the result is null otherwise.
    fn guarded(
        &mut self,
        span: Span,
        guard: ValueId,
        build: impl FnOnce(&mut Self) -> ValueId,
    ) -> ValueId {
        let result = self.fresh_local();
        let test = self.test(span, TestKind::Isset, vec![guard]);
        let present = self.new_block();
        let absent = self.new_block();
        let join = self.new_block();
        self.terminate(Terminator::Branch {
            cond: test,
            then: present,
            otherwise: absent,
        });
        self.cur = present;
        let v = build(self);
        self.store_local(span, result, v);
        self.goto(join);
        self.cur = absent;
        let n = self.null(span);
        self.store_local(span, result, n);
        self.goto(join);
        self.cur = join;
        self.load_local(span, result)
    }

    /// The place an assignable expression denotes, evaluating the operands it needs. The flag is
    /// true when a write is weak (an element write).
    pub fn place_of(&mut self, target: &'a Expr) -> (Place, bool) {
        match &target.kind {
            ExprKind::Variable(name) => (self.variable_place(plain(name)), false),
            ExprKind::Index { array, index } => {
                if let Some(i) = index {
                    self.expr(i);
                }
                let (place, _) = self.place_of(array);
                (place, true)
            }
            ExprKind::Property { object, member, .. } => {
                let obj = self.expr(object);
                let owners = self.classes_of(object);
                let name = self.member_ref(member);
                (
                    Place::Prop {
                        object: obj,
                        name,
                        owners,
                    },
                    false,
                )
            }
            ExprKind::StaticProperty { class, name } => {
                let class = self.class_operand(class);
                let name = match &name.kind {
                    ExprKind::Variable(n) => MemberRef::Named(plain(n).into()),
                    _ => MemberRef::Dynamic(self.expr(name)),
                };
                (Place::StaticProp { class, name }, false)
            }
            ExprKind::Parenthesized(inner)
            | ExprKind::Unary {
                op: UnaryOp::Reference,
                operand: inner,
            } => self.place_of(inner),
            _ => {
                self.expr(target);
                (Place::Local(self.fresh_local()), true)
            }
        }
    }

    fn variable_place(&mut self, name: &str) -> Place {
        if is_superglobal(name) || self.global_names.contains(name) {
            return Place::Global(name.into());
        }
        Place::Local(self.local(name))
    }

    // ---- calls --------------------------------------------------------------------------

    fn resolution(&self, e: &'a Expr) -> Resolution {
        self.facts
            .and_then(|f| f.call_at(&ExprKey::of(e)))
            .map_or(Resolution::Missing, |r| Resolution::Facts(r.clone()))
    }

    fn call_args(&mut self, args: &'a [bleedthrough_syntax::Arg]) -> Vec<CallArg> {
        args.iter()
            .map(|arg| {
                let (value, name, spread) = match &arg.kind {
                    ArgKind::Positional(x) => (self.expr(x), None, false),
                    ArgKind::Named { name, value } => (self.expr(value), Some(name.clone()), false),
                    ArgKind::Spread(x) => (self.expr(x), None, true),
                    _ => (self.null(arg.span), None, false),
                };
                CallArg {
                    value,
                    name,
                    spread,
                    span: arg.span,
                }
            })
            .collect()
    }

    /// Builds the callee of a call, evaluating the receiver or class operand. The returned flag
    /// is the receiver and whether the access is null-safe.
    fn callee(&mut self, target: &'a CallTarget) -> (Callee, Option<ValueId>) {
        match target {
            CallTarget::Function(inner) => match &inner.kind {
                ExprKind::Name(n) => (Callee::Function(n.clone()), None),
                _ => (Callee::Value(self.expr(inner)), None),
            },
            CallTarget::Method {
                object,
                member,
                nullsafe,
            } => {
                let receiver = self.expr(object);
                let name = self.member_ref(member);
                let guard = nullsafe.then_some(receiver);
                (
                    Callee::Method {
                        receiver,
                        name,
                        nullsafe: *nullsafe,
                    },
                    guard,
                )
            }
            CallTarget::Static { class, member } => {
                let class = self.class_operand(class);
                let name = self.member_ref(member);
                (Callee::Static { class, name }, None)
            }
            _ => {
                let v = self.unknown(Span::default(), "unmodelled-call-target", Vec::new());
                (Callee::Value(v), None)
            }
        }
    }

    fn call(
        &mut self,
        e: &'a Expr,
        target: &'a CallTarget,
        args: &'a [bleedthrough_syntax::Arg],
    ) -> ValueId {
        let ty = self.ty_of(e);
        let resolution = self.resolution(e);
        let (callee, guard) = self.callee(target);
        let span = e.span;
        let build = |fb: &mut Self| {
            let call_args = fb.call_args(args);
            fb.emit(
                span,
                ty,
                InstrKind::Call {
                    callee,
                    args: call_args,
                    resolution,
                },
            )
        };
        match guard {
            Some(g) => self.guarded(span, g, build),
            None => build(self),
        }
    }

    fn callable_ref(&mut self, e: &'a Expr, target: &'a CallTarget) -> ValueId {
        let resolution = self.resolution(e);
        let (callee, guard) = self.callee(target);
        let ty = self.ty_of(e);
        let span = e.span;
        let build =
            |fb: &mut Self| fb.emit(span, ty, InstrKind::CallableRef { callee, resolution });
        match guard {
            Some(g) => self.guarded(span, g, build),
            None => build(self),
        }
    }

    fn new_object(
        &mut self,
        e: &'a Expr,
        class: ClassOperand,
        args: &'a [bleedthrough_syntax::Arg],
    ) -> ValueId {
        let resolution = self.resolution(e);
        let call_args = self.call_args(args);
        let ty = self.ty_of(e);
        self.emit(
            e.span,
            ty,
            InstrKind::New {
                class,
                args: call_args,
                resolution,
            },
        )
    }

    fn include(&mut self, span: Span, kind: IncludeKind, path: &'a Expr) -> ValueId {
        let p = self.expr(path);
        self.emit(
            span,
            TypeClass::Unknown,
            InstrKind::Include { kind, path: p },
        )
    }

    // ---- operators ----------------------------------------------------------------------

    fn unary(&mut self, e: &'a Expr, ty: TypeClass, op: UnaryOp, operand: &'a Expr) -> ValueId {
        match op {
            UnaryOp::ErrorSuppress | UnaryOp::Reference => self.expr(operand),
            UnaryOp::PreInc | UnaryOp::PreDec | UnaryOp::PostInc | UnaryOp::PostDec => {
                let (place, _) = self.place_of(operand);
                let old = self.emit(operand.span, ty, InstrKind::Load(place.clone()));
                let new = self.emit(e.span, ty, InstrKind::Unary { op, operand: old });
                self.emit_effect(
                    e.span,
                    InstrKind::Store {
                        place,
                        value: new,
                        weak: false,
                    },
                );
                if matches!(op, UnaryOp::PostInc | UnaryOp::PostDec) {
                    old
                } else {
                    new
                }
            }
            _ => {
                let v = self.expr(operand);
                self.emit(e.span, ty, InstrKind::Unary { op, operand: v })
            }
        }
    }

    fn binary(&mut self, e: &'a Expr) -> ValueId {
        // Walk the left spine iteratively so a long chain of one operator cannot exhaust the stack.
        let mut spine: Vec<(&Expr, BinaryOp, &Expr)> = Vec::new();
        let mut current = e;
        while let ExprKind::Binary { op, lhs, rhs } = &current.kind
            && !short_circuit(*op)
        {
            spine.push((current, *op, rhs));
            current = lhs;
        }
        if spine.is_empty() {
            return self.short_circuit(e);
        }
        let mut acc = self.expr(current);
        let mut acc_span = current.span;
        let mut pending: Option<Vec<Operand>> = None;
        for (outer, op, rhs) in spine.into_iter().rev() {
            let r = self.expr(rhs);
            if op == BinaryOp::Concat {
                let list = pending.get_or_insert_with(|| {
                    vec![Operand {
                        value: acc,
                        span: acc_span,
                    }]
                });
                list.push(Operand {
                    value: r,
                    span: rhs.span,
                });
                acc_span = outer.span;
                continue;
            }
            if let Some(list) = pending.take() {
                acc = self.emit(acc_span, TypeClass::Data, InstrKind::Concat(list));
            }
            let ty = self.ty_of(outer);
            acc = self.emit(
                outer.span,
                ty,
                InstrKind::Binary {
                    op,
                    lhs: acc,
                    rhs: r,
                },
            );
            acc_span = outer.span;
        }
        if let Some(list) = pending.take() {
            acc = self.emit(acc_span, TypeClass::Data, InstrKind::Concat(list));
        }
        acc
    }

    fn short_circuit(&mut self, e: &'a Expr) -> ValueId {
        let ExprKind::Binary { op, lhs, rhs } = &e.kind else {
            return self.unknown(e.span, "unmodelled-binary", Vec::new());
        };
        match op {
            BinaryOp::Coalesce => self.coalesce(e, lhs, rhs),
            BinaryOp::Instanceof => {
                let l = self.expr(lhs);
                let r = match &rhs.kind {
                    ExprKind::Name(_) | ExprKind::ClassRef(_) => self.emit(
                        rhs.span,
                        TypeClass::Data,
                        InstrKind::Const(ConstValue::Str(None)),
                    ),
                    _ => self.expr(rhs),
                };
                self.emit(
                    e.span,
                    TypeClass::Inert,
                    InstrKind::Binary {
                        op: *op,
                        lhs: l,
                        rhs: r,
                    },
                )
            }
            _ => self.logical(e, *op, lhs, rhs),
        }
    }

    /// `&&`, `||`, `and`, `or`: the right operand runs only on the path the left allows.
    fn logical(&mut self, e: &'a Expr, op: BinaryOp, lhs: &'a Expr, rhs: &'a Expr) -> ValueId {
        let l = self.expr(lhs);
        let evaluate = self.new_block();
        let join = self.new_block();
        let (then, otherwise) = if matches!(op, BinaryOp::And | BinaryOp::LowAnd) {
            (evaluate, join)
        } else {
            (join, evaluate)
        };
        self.terminate(Terminator::Branch {
            cond: l,
            then,
            otherwise,
        });
        self.cur = evaluate;
        self.expr(rhs);
        self.goto(join);
        self.cur = join;
        self.test(e.span, TestKind::Logical, Vec::new())
    }

    fn coalesce(&mut self, e: &'a Expr, lhs: &'a Expr, rhs: &'a Expr) -> ValueId {
        let result = self.fresh_local();
        let l = self.expr(lhs);
        self.store_local(lhs.span, result, l);
        let test = self.test(lhs.span, TestKind::Isset, vec![l]);
        let fallback = self.new_block();
        let join = self.new_block();
        self.terminate(Terminator::Branch {
            cond: test,
            then: join,
            otherwise: fallback,
        });
        self.cur = fallback;
        let r = self.expr(rhs);
        self.store_local(rhs.span, result, r);
        self.goto(join);
        self.cur = join;
        self.load_local(e.span, result)
    }

    fn conditional(
        &mut self,
        e: &'a Expr,
        condition: &'a Expr,
        then: Option<&'a Expr>,
        otherwise: &'a Expr,
    ) -> ValueId {
        let result = self.fresh_local();
        let c = self.expr(condition);
        let yes = self.new_block();
        let no = self.new_block();
        let join = self.new_block();
        self.terminate(Terminator::Branch {
            cond: c,
            then: yes,
            otherwise: no,
        });
        self.cur = yes;
        let t = match then {
            Some(x) => self.expr(x),
            None => c,
        };
        self.store_local(e.span, result, t);
        self.goto(join);
        self.cur = no;
        let o = self.expr(otherwise);
        self.store_local(e.span, result, o);
        self.goto(join);
        self.cur = join;
        self.load_local(e.span, result)
    }

    fn match_expr(&mut self, e: &'a Expr, subject: &'a Expr, arms: &'a [MatchArm]) -> ValueId {
        let result = self.fresh_local();
        let s = self.expr(subject);
        let end = self.new_block();
        for arm in arms
            .iter()
            .filter(|a| !a.is_default && !a.conditions.is_empty())
        {
            let body = self.new_block();
            for condition in &arm.conditions {
                let c = self.expr(condition);
                let eq = self.emit(
                    condition.span,
                    TypeClass::Inert,
                    InstrKind::Binary {
                        op: BinaryOp::Identical,
                        lhs: s,
                        rhs: c,
                    },
                );
                let next = self.new_block();
                self.terminate(Terminator::Branch {
                    cond: eq,
                    then: body,
                    otherwise: next,
                });
                self.cur = next;
            }
            let after = self.cur;
            self.cur = body;
            let v = self.expr(&arm.body);
            self.store_local(arm.body.span, result, v);
            self.goto(end);
            self.cur = after;
        }
        match arms
            .iter()
            .find(|a| a.is_default || a.conditions.is_empty())
        {
            Some(arm) => {
                let v = self.expr(&arm.body);
                self.store_local(arm.body.span, result, v);
                self.goto(end);
            }
            None => {
                let n = self.null(e.span);
                self.terminate(Terminator::Throw(n));
            }
        }
        self.cur = end;
        self.load_local(e.span, result)
    }

    // ---- assignment ---------------------------------------------------------------------

    fn assign(&mut self, e: &'a Expr, op: AssignOp, target: &'a Expr, value: &'a Expr) -> ValueId {
        if op == AssignOp::Assign {
            if let ExprKind::Array { .. } = &target.kind {
                let rhs = self.expr(value);
                self.destructure(target, rhs);
                return rhs;
            }
            let (place, weak) = self.place_of(target);
            let rhs = self.expr(value);
            self.emit_effect(
                e.span,
                InstrKind::Store {
                    place,
                    value: rhs,
                    weak,
                },
            );
            return rhs;
        }
        let (place, weak) = self.place_of(target);
        if op == AssignOp::Coalesce {
            return self.coalesce_assign(e, place, weak, target, value);
        }
        let ty = self.ty_of(e);
        let old = self.emit(target.span, ty, InstrKind::Load(place.clone()));
        let rhs = self.expr(value);
        let new = match assign_binary(op) {
            Some(BinaryOp::Concat) => self.emit(
                e.span,
                TypeClass::Data,
                InstrKind::Concat(vec![
                    Operand {
                        value: old,
                        span: target.span,
                    },
                    Operand {
                        value: rhs,
                        span: value.span,
                    },
                ]),
            ),
            Some(binary) => self.emit(
                e.span,
                ty,
                InstrKind::Binary {
                    op: binary,
                    lhs: old,
                    rhs,
                },
            ),
            None => self.unknown(e.span, "unmodelled-assignment", vec![old, rhs]),
        };
        self.emit_effect(
            e.span,
            InstrKind::Store {
                place,
                value: new,
                weak,
            },
        );
        new
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "lowering threads the AST pieces it was handed"
    )]
    fn coalesce_assign(
        &mut self,
        e: &'a Expr,
        place: Place,
        weak: bool,
        target: &'a Expr,
        value: &'a Expr,
    ) -> ValueId {
        let old = self.emit(
            target.span,
            self.ty_of(target),
            InstrKind::Load(place.clone()),
        );
        let test = self.test(target.span, TestKind::Isset, vec![old]);
        let assign = self.new_block();
        let join = self.new_block();
        self.terminate(Terminator::Branch {
            cond: test,
            then: join,
            otherwise: assign,
        });
        self.cur = assign;
        let rhs = self.expr(value);
        self.emit_effect(
            e.span,
            InstrKind::Store {
                place: place.clone(),
                value: rhs,
                weak,
            },
        );
        self.goto(join);
        self.cur = join;
        self.emit(e.span, self.ty_of(e), InstrKind::Load(place))
    }

    /// Assigns the elements of `source` to the targets of a list pattern.
    pub fn destructure(&mut self, pattern: &'a Expr, source: ValueId) {
        let ExprKind::Array { items, .. } = &pattern.kind else {
            return;
        };
        for (position, item) in items.iter().enumerate() {
            let (target, key) = match &item.kind {
                ArrayItemKind::Value(t) => (t, None),
                ArrayItemKind::KeyValue { key, value } => (value, Some(key)),
                _ => continue,
            };
            let index = match key {
                Some(k) => self.expr(k),
                None => self.emit(
                    item.span,
                    TypeClass::Inert,
                    InstrKind::Const(ConstValue::Int(position.to_string().into())),
                ),
            };
            let element = self.emit(
                item.span,
                TypeClass::Unknown,
                InstrKind::LoadElem {
                    base: source,
                    index: Some(index),
                },
            );
            self.assign_to(target, element);
        }
    }

    /// Stores `value` into an assignable expression, or destructures it into a list pattern.
    pub fn assign_to(&mut self, target: &'a Expr, value: ValueId) {
        match &target.kind {
            ExprKind::Array { .. } => self.destructure(target, value),
            ExprKind::Unary {
                op: UnaryOp::Reference,
                operand,
            } => self.assign_to(operand, value),
            _ => {
                let (place, weak) = self.place_of(target);
                self.emit_effect(target.span, InstrKind::Store { place, value, weak });
            }
        }
    }

    // ---- closures -----------------------------------------------------------------------

    #[allow(
        clippy::too_many_arguments,
        reason = "lowering threads the AST pieces it was handed"
    )]
    fn closure(
        &mut self,
        e: &'a Expr,
        kind: UnitKind,
        params: &'a [bleedthrough_syntax::Param],
        captures: Vec<(Box<str>, bool)>,
        is_static: bool,
        body: JobBody<'a>,
    ) -> ValueId {
        let bindings: Vec<CaptureBinding> = captures
            .iter()
            .enumerate()
            .map(|(i, (name, by_ref))| CaptureBinding {
                outer: self.local(name),
                inner: LocalId::new(u32::try_from(params.len() + i).unwrap_or(u32::MAX)),
                by_ref: *by_ref,
            })
            .collect();
        let name = format!("{{closure}}@{}", e.span.start);
        let unit = crate::graph::UnitKey {
            span: e.span,
            name: name.clone().into(),
        };
        self.enqueue_closure(e, kind, params, captures, is_static, body, name);
        self.emit(
            e.span,
            TypeClass::Data,
            InstrKind::Closure {
                unit,
                captures: bindings,
            },
        )
    }
}

/// The constant a literal expression denotes.
fn constant_of(e: &Expr) -> ConstValue {
    match &e.kind {
        ExprKind::Literal(l) => constant(l),
        _ => ConstValue::Null,
    }
}
