//! Expression typing: PHP's result types for each expression form, flow-sensitively.

use bleedthrough_syntax::{
    ArrayItemKind, ArrayStyle, AssignOp, BinaryOp, Expr, ExprKind, MagicConstantKind, MatchArm,
    MemberName, StringKind, StringPartKind, UnaryOp,
};

use super::ctx::ClassRes;
use super::env::Env;
use super::interp::{Body, Interp, Unit};
use super::narrow::{literal, match_literals, narrow};
use super::ops::{
    ArrayItem, ItemKey, arith, build_array, cast, concat, elements, incdec, index_read,
    literal_key, truthy_part, update_array,
};
use crate::name::Fqn;
use crate::summary::literal_type;
use crate::types::{Atom, Type};

const SUPERGLOBALS: [&str; 9] = [
    "GLOBALS", "_GET", "_POST", "_COOKIE", "_FILES", "_REQUEST", "_SERVER", "_ENV", "_SESSION",
];

fn plain(name: &str) -> &str {
    name.trim_start_matches('$')
}

impl<'a> Interp<'a> {
    /// The type of an expression in the current state. Every expression gets a type; where the
    /// work limits stop the analysis it is `unknown`.
    pub fn expr(&mut self, e: &'a Expr) -> Type {
        if !self.enter() {
            self.fill_unknown(bleedthrough_syntax::NodeRef::Expr(e));
            return Type::unknown();
        }
        self.depth += 1;
        let ty = self.expr_inner(e);
        self.depth -= 1;
        self.record(e, &ty);
        ty
    }

    #[allow(
        clippy::too_many_lines,
        clippy::cognitive_complexity,
        reason = "a dispatch over every expression kind"
    )]
    fn expr_inner(&mut self, e: &'a Expr) -> Type {
        match &e.kind {
            ExprKind::Literal(_) => literal_type(e),
            ExprKind::Interpolated { kind, parts } => {
                let mut non_empty = false;
                for part in parts {
                    match &part.kind {
                        StringPartKind::Text { raw } => non_empty |= !raw.is_empty(),
                        StringPartKind::Interpolation(inner) => {
                            self.expr(inner);
                        }
                        _ => {}
                    }
                }
                match kind {
                    StringKind::ShellExec => {
                        Type::from_atoms([Atom::String, Atom::False, Atom::Null])
                    }
                    _ if non_empty => Type::of(Atom::NonEmptyString),
                    _ => Type::string(),
                }
            }
            ExprKind::Variable(name) => self.variable_type(plain(name)),
            ExprKind::VariableVariable(inner) => {
                self.expr(inner);
                Type::unknown()
            }
            ExprKind::Name(name) => match self.ctx.class_of_name(name) {
                ClassRes::Class(f) | ClassRes::Late(f) => Type::of(Atom::ClassString(Some(f))),
                _ => Type::unknown(),
            },
            ExprKind::ConstFetch(name) => self.constant_type(name),
            ExprKind::ClassRef(_) => match self.ctx.class_of_expr(e) {
                Some(ClassRes::Class(f) | ClassRes::Late(f)) => {
                    Type::of(Atom::ClassString(Some(f)))
                }
                _ => Type::unknown(),
            },
            ExprKind::MagicConstant(kind) => match kind {
                MagicConstantKind::Line => Type::int(),
                MagicConstantKind::Class => self.ctx.class.current().map_or_else(
                    || Type::of(Atom::ClassString(None)),
                    |c| Type::of(Atom::ClassString(Some(c.clone()))),
                ),
                _ => Type::string(),
            },
            ExprKind::Array { style, items } => self.array_literal(*style, items),
            ExprKind::Index { array, index } => {
                let base = self.expr(array);
                let key = index.as_deref().map(|i| self.expr(i));
                index_read(&base, key.as_ref())
            }
            ExprKind::Property {
                object,
                member,
                nullsafe,
            } => {
                let object_ty = self.expr(object);
                self.property_type(&object_ty, member, *nullsafe)
            }
            ExprKind::StaticProperty { class, name } => {
                self.expr(class);
                if let ExprKind::Variable(n) = &name.kind {
                    self.record(name, &Type::of(Atom::StringLiteral(plain(n).into())));
                    self.static_property_type(class, plain(n))
                } else {
                    self.expr(name);
                    Type::unknown()
                }
            }
            ExprKind::ClassConst { class, member } => {
                self.expr(class);
                self.class_constant_type(class, member)
            }
            ExprKind::Call { .. } | ExprKind::CallableRef { .. } | ExprKind::New { .. } => {
                self.call_like(e)
            }
            ExprKind::AnonymousClass { args, class } => {
                for arg in args {
                    self.arg(arg);
                }
                for member in &class.members {
                    self.fill_unknown(bleedthrough_syntax::NodeRef::Member(member));
                }
                Type::of(Atom::Object)
            }
            ExprKind::Unary { op, operand } => self.unary(*op, operand),
            ExprKind::Cast { kind, operand } => {
                let t = self.expr(operand);
                cast(*kind, &t)
            }
            ExprKind::Binary { op, lhs, rhs } => self.binary(*op, lhs, rhs),
            ExprKind::Assign { op, target, value } => self.assign(*op, target, value),
            ExprKind::Conditional {
                condition,
                then,
                otherwise,
            } => self.conditional(condition, then.as_deref(), otherwise),
            ExprKind::Match { subject, arms } => self.match_expr(subject, arms),
            ExprKind::Closure { .. } | ExprKind::ArrowFn { .. } => self.closure(e),
            ExprKind::Yield { key, value } => {
                if let Some(k) = key {
                    self.expr(k);
                }
                if let Some(v) = value {
                    self.expr(v);
                }
                Type::mixed()
            }
            ExprKind::YieldFrom(inner) => {
                self.expr(inner);
                Type::mixed()
            }
            ExprKind::Throw(inner) => {
                self.expr(inner);
                self.env.dead = true;
                Type::never()
            }
            ExprKind::Clone(inner) | ExprKind::Parenthesized(inner) => self.expr(inner),
            ExprKind::Print(inner) => {
                self.expr(inner);
                Type::of(Atom::IntLiteral(1))
            }
            ExprKind::Include { path, .. } => {
                self.expr(path);
                Type::mixed()
            }
            ExprKind::Eval(inner) => {
                self.expr(inner);
                Type::mixed()
            }
            ExprKind::Exit { args, .. } => {
                for arg in args {
                    self.arg(arg);
                }
                self.env.dead = true;
                Type::never()
            }
            ExprKind::Isset(items) => {
                for item in items {
                    self.expr(item);
                }
                Type::bool()
            }
            ExprKind::Empty(inner) => {
                self.expr(inner);
                Type::bool()
            }
            ExprKind::Pipe { input, callable } => {
                self.expr(input);
                self.expr(callable);
                Type::unknown()
            }
            _ => Type::unknown(),
        }
    }

    fn variable_type(&self, name: &str) -> Type {
        if let Some(t) = self.env.lookup(name) {
            return t.clone();
        }
        if SUPERGLOBALS.contains(&name) {
            return Type::array_of(Type::array_key(), Type::mixed());
        }
        Type::unknown()
    }

    fn constant_type(&self, name: &bleedthrough_syntax::Name) -> Type {
        match name.text.to_ascii_lowercase().as_str() {
            "true" => return Type::of(Atom::True),
            "false" => return Type::of(Atom::False),
            "null" => return Type::null(),
            _ => {}
        }
        self.ctx
            .resolve_constant(name)
            .and_then(|f| self.ctx.table.constant(&f))
            .map_or_else(Type::unknown, |c| c.ty.clone())
    }

    fn array_literal(
        &mut self,
        _style: ArrayStyle,
        items: &'a [bleedthrough_syntax::ArrayItem],
    ) -> Type {
        let mut parts: Vec<ArrayItem> = Vec::new();
        let mut spreads: Vec<Type> = Vec::new();
        for item in items {
            match &item.kind {
                ArrayItemKind::Value(v) => {
                    let value = self.expr(v);
                    parts.push(ArrayItem {
                        key: ItemKey::Implicit,
                        value,
                    });
                }
                ArrayItemKind::KeyValue { key, value } => {
                    let key_ty = self.expr(key);
                    let value = self.expr(value);
                    let key =
                        literal_key(&key_ty).map_or(ItemKey::Dynamic(key_ty), ItemKey::Literal);
                    parts.push(ArrayItem { key, value });
                }
                ArrayItemKind::Spread(s) => spreads.push(self.expr(s)),
                _ => {}
            }
        }
        build_array(parts, &spreads)
    }

    // ---- operators -------------------------------------------------------------------------

    fn unary(&mut self, op: UnaryOp, operand: &'a Expr) -> Type {
        let t = self.expr(operand);
        match op {
            UnaryOp::ErrorSuppress | UnaryOp::Reference => t,
            UnaryOp::BitNot => {
                if !t.atoms().is_empty() && t.atoms().iter().all(Atom::is_string) {
                    Type::string()
                } else {
                    Type::int()
                }
            }
            UnaryOp::Not => Type::bool(),
            UnaryOp::Plus => arith(BinaryOp::Mul, &t, &Type::of(Atom::IntLiteral(1))),
            UnaryOp::Neg => arith(BinaryOp::Mul, &t, &Type::of(Atom::IntLiteral(-1))),
            UnaryOp::PreInc | UnaryOp::PreDec | UnaryOp::PostInc | UnaryOp::PostDec => {
                let inc = matches!(op, UnaryOp::PreInc | UnaryOp::PostInc);
                let updated = incdec(&t, inc);
                self.store_lvalue(operand, updated.clone());
                if matches!(op, UnaryOp::PreInc | UnaryOp::PreDec) {
                    updated
                } else {
                    t
                }
            }
            _ => Type::unknown(),
        }
    }

    fn binary(&mut self, op: BinaryOp, lhs: &'a Expr, rhs: &'a Expr) -> Type {
        match op {
            BinaryOp::And | BinaryOp::LowAnd => {
                self.expr(lhs);
                let (yes, no) = narrow(&self.ctx, lhs, &self.env);
                self.env = yes;
                self.expr(rhs);
                self.env = no.join(&self.env);
                Type::bool()
            }
            BinaryOp::Or | BinaryOp::LowOr => {
                self.expr(lhs);
                let (yes, no) = narrow(&self.ctx, lhs, &self.env);
                self.env = no;
                self.expr(rhs);
                self.env = yes.join(&self.env);
                Type::bool()
            }
            BinaryOp::Coalesce => {
                let l = self.expr(lhs);
                let after_left = self.env.clone();
                let r = self.expr(rhs);
                self.env = after_left.join(&self.env);
                coalesce(&l, &r)
            }
            _ => {
                let l = self.expr(lhs);
                let r = self.expr(rhs);
                match op {
                    BinaryOp::Concat => concat(&l, &r),
                    BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::Div
                    | BinaryOp::Mod
                    | BinaryOp::Pow
                    | BinaryOp::BitAnd
                    | BinaryOp::BitOr
                    | BinaryOp::BitXor
                    | BinaryOp::Shl
                    | BinaryOp::Shr => arith(op, &l, &r),
                    BinaryOp::Spaceship => Type::of(Atom::IntRange {
                        min: Some(-1),
                        max: Some(1),
                    }),
                    BinaryOp::Eq
                    | BinaryOp::NotEq
                    | BinaryOp::AngledNotEq
                    | BinaryOp::Identical
                    | BinaryOp::NotIdentical
                    | BinaryOp::Lt
                    | BinaryOp::Le
                    | BinaryOp::Gt
                    | BinaryOp::Ge
                    | BinaryOp::Instanceof
                    | BinaryOp::LowXor => Type::bool(),
                    _ => Type::unknown(),
                }
            }
        }
    }

    fn conditional(
        &mut self,
        condition: &'a Expr,
        then: Option<&'a Expr>,
        otherwise: &'a Expr,
    ) -> Type {
        let c = self.expr(condition);
        match then {
            Some(then) => {
                let (yes, no) = narrow(&self.ctx, condition, &self.env);
                self.env = yes;
                let a = self.expr(then);
                let after_then = self.env.clone();
                self.env = no;
                let b = self.expr(otherwise);
                self.env = after_then.join(&self.env);
                a.union(&b)
            }
            None => {
                let after_condition = self.env.clone();
                let b = self.expr(otherwise);
                self.env = after_condition.join(&self.env);
                truthy_part(&c).union(&b)
            }
        }
    }

    fn match_expr(&mut self, subject: &'a Expr, arms: &'a [MatchArm]) -> Type {
        self.expr(subject);
        let true_subject = literal(subject) == Some(Atom::True);
        let mut rest = self.env.clone();
        let mut ends: Vec<Env> = Vec::new();
        let mut results: Vec<Type> = Vec::new();
        for arm in arms {
            self.env = rest.clone();
            for condition in &arm.conditions {
                self.expr(condition);
            }
            if !arm.is_default && !arm.conditions.is_empty() {
                if true_subject {
                    let mut inside = Env::unreachable();
                    let mut remaining = self.env.clone();
                    for condition in &arm.conditions {
                        let (yes, no) = narrow(&self.ctx, condition, &remaining);
                        inside = inside.join(&yes);
                        remaining = no;
                    }
                    self.env = inside;
                    rest = remaining;
                } else if let Some((yes, no)) = match_literals(subject, &arm.conditions, &self.env)
                {
                    self.env = yes;
                    rest = no;
                }
            }
            results.push(self.expr(&arm.body));
            ends.push(self.env.clone());
        }
        self.env = Env::join_all(ends.iter());
        Type::union_all(results.iter())
    }

    // ---- assignment ------------------------------------------------------------------------

    fn assign(&mut self, op: AssignOp, target: &'a Expr, value: &'a Expr) -> Type {
        match op {
            AssignOp::Assign => {
                let v = self.expr(value);
                self.assign_target(target, v.clone());
                v
            }
            AssignOp::Coalesce => {
                let current = self.expr(target);
                let after_target = self.env.clone();
                let v = self.expr(value);
                self.env = after_target.join(&self.env);
                let new = coalesce(&current, &v);
                self.store_lvalue(target, new.clone());
                self.record(target, &new);
                new
            }
            _ => {
                let current = self.expr(target);
                let v = self.expr(value);
                let new = match op {
                    AssignOp::Concat => concat(&current, &v),
                    AssignOp::Add => arith(BinaryOp::Add, &current, &v),
                    AssignOp::Sub => arith(BinaryOp::Sub, &current, &v),
                    AssignOp::Mul => arith(BinaryOp::Mul, &current, &v),
                    AssignOp::Div => arith(BinaryOp::Div, &current, &v),
                    AssignOp::Mod => arith(BinaryOp::Mod, &current, &v),
                    AssignOp::Pow => arith(BinaryOp::Pow, &current, &v),
                    AssignOp::BitAnd => arith(BinaryOp::BitAnd, &current, &v),
                    AssignOp::BitOr => arith(BinaryOp::BitOr, &current, &v),
                    AssignOp::BitXor => arith(BinaryOp::BitXor, &current, &v),
                    AssignOp::Shl => arith(BinaryOp::Shl, &current, &v),
                    AssignOp::Shr => arith(BinaryOp::Shr, &current, &v),
                    _ => Type::unknown(),
                };
                self.store_lvalue(target, new.clone());
                self.record(target, &new);
                new
            }
        }
    }

    /// Assigns `ty` to a target, typing the target's own sub-expressions on the way.
    pub fn assign_target(&mut self, target: &'a Expr, ty: Type) {
        match &target.kind {
            ExprKind::Variable(name) => {
                self.env.set(plain(name), ty.clone());
                self.record(target, &ty);
            }
            ExprKind::Index { array, index } => {
                self.expr(array);
                if let Some(i) = index {
                    self.expr(i);
                }
                self.record(target, &ty);
                self.store_lvalue(target, ty);
            }
            ExprKind::Property { object, member, .. } => {
                self.expr(object);
                if let MemberName::Dynamic(d) = member {
                    self.expr(d);
                }
                self.record(target, &ty);
            }
            ExprKind::StaticProperty { class, name } => {
                self.expr(class);
                if let ExprKind::Variable(n) = &name.kind {
                    self.record(name, &Type::of(Atom::StringLiteral(plain(n).into())));
                } else {
                    self.expr(name);
                }
                self.record(target, &ty);
            }
            ExprKind::Array { items, .. } => {
                self.record(target, &ty);
                for (position, item) in (0_i64..).zip(items.iter()) {
                    match &item.kind {
                        ArrayItemKind::Value(v) => {
                            let elem = index_read(&ty, Some(&Type::of(Atom::IntLiteral(position))));
                            self.assign_target(v, elem);
                        }
                        ArrayItemKind::KeyValue { key, value } => {
                            let key_ty = self.expr(key);
                            let elem = index_read(&ty, Some(&key_ty));
                            self.assign_target(value, elem);
                        }
                        _ => {}
                    }
                }
            }
            ExprKind::Unary {
                op: UnaryOp::Reference,
                operand,
            } => {
                self.record(target, &ty);
                self.assign_target(operand, ty);
            }
            ExprKind::Parenthesized(inner) => {
                self.record(target, &ty);
                self.assign_target(inner, ty);
            }
            _ => {
                self.expr(target);
            }
        }
    }

    /// Updates the state for an assignment to `target` without typing anything: variables take
    /// the type, array element writes update the array's type, other targets are untracked.
    pub fn store_lvalue(&mut self, target: &'a Expr, ty: Type) {
        match &target.kind {
            ExprKind::Variable(name) => self.env.set(plain(name), ty),
            ExprKind::Index { array, index } => {
                let key = index.as_deref().map(|i| self.quietly(|me| me.expr(i)));
                let old = self.lvalue_current(array);
                let updated = update_array(&old, key.as_ref(), &ty);
                self.store_lvalue(array, updated);
            }
            ExprKind::Parenthesized(inner) => self.store_lvalue(inner, ty),
            ExprKind::Unary {
                op: UnaryOp::Reference,
                operand,
            } => self.store_lvalue(operand, ty),
            _ => {}
        }
    }

    fn lvalue_current(&mut self, e: &'a Expr) -> Type {
        match &e.kind {
            ExprKind::Variable(name) => self
                .env
                .lookup(plain(name))
                .cloned()
                .unwrap_or_else(Type::null),
            _ => self.quietly(|me| me.expr(e)),
        }
    }

    // ---- closures --------------------------------------------------------------------------

    fn closure(&mut self, e: &'a Expr) -> Type {
        let (params, return_type, is_static, uses, body) = match &e.kind {
            ExprKind::Closure {
                params,
                return_type,
                is_static,
                uses,
                body,
                ..
            } => (
                params,
                return_type,
                *is_static,
                Some(uses),
                Body::Stmts(body),
            ),
            ExprKind::ArrowFn {
                params,
                return_type,
                is_static,
                body,
                ..
            } => (params, return_type, *is_static, None, Body::Expr(body)),
            _ => return Type::unknown(),
        };
        let (sig, param_types) = self.ctx.signature(params, return_type.as_deref(), e.span);
        let ty = Type::of(Atom::Callable(Some(Box::new(sig))));
        if self.is_quiet() {
            return ty;
        }
        if self.unit_depth + 1 > self.limits.max_closure_depth {
            self.fill_children(e);
            return ty;
        }
        let mut env = match uses {
            None => self.env.clone(),
            Some(_) => Env::new(),
        };
        if is_static {
            env.remove("this");
        } else if let Some(this) = self.env.lookup("this") {
            env.set("this", this.clone());
        }
        if let Some(uses) = uses {
            for used in uses {
                let name = plain(&used.name);
                let outer = self.env.lookup(name).cloned();
                env.set(name, outer.clone().unwrap_or_else(Type::unknown));
                if used.by_ref {
                    let widened = outer.unwrap_or_else(Type::null).union(&Type::unknown());
                    self.env.set(name, widened);
                }
            }
        }
        let unit = Unit {
            name: format!("{{closure}}@{}", e.span.start),
            span: e.span,
            params,
            param_types,
            body,
            env,
            depth: self.unit_depth + 1,
        };
        self.pending.push(unit);
        ty
    }

    // ---- iteration -------------------------------------------------------------------------

    /// The key and value types produced by iterating a value of type `ty`.
    pub fn iteration_types(&mut self, ty: &Type) -> (Type, Type) {
        let mut keys = Vec::new();
        let mut values = Vec::new();
        for atom in ty.atoms() {
            let (k, v) = match atom {
                Atom::Named(n) => self.object_iteration(&n.name, &n.args, 0),
                Atom::Static(f) => self.object_iteration(f, &[], 0),
                other => elements(&Type::of(other.clone())),
            };
            keys.push(k);
            values.push(v);
        }
        (Type::union_all(keys.iter()), Type::union_all(values.iter()))
    }

    fn object_iteration(&mut self, class: &Fqn, args: &[Type], depth: usize) -> (Type, Type) {
        use crate::table::Subtyping;
        let unknown = || (Type::unknown(), Type::unknown());
        if depth > 3 {
            return unknown();
        }
        let table = self.ctx.table;
        if table.is_subtype(class, &Fqn::new("Traversable")) != Subtyping::No {
            match args {
                [value] => return (Type::mixed(), value.clone()),
                [key, value, ..] => return (key.clone(), value.clone()),
                [] => {}
            }
        }
        let caller = self.ctx.class.current();
        let ret = |name: &str| {
            table
                .method(class, name, caller)
                .accessible()
                .map(|c| c.member.ret.ty.clone())
                .filter(|t| !t.has_unknown())
        };
        if let (Some(key), Some(value)) = (ret("key"), ret("current")) {
            return (key, value);
        }
        if let Some(iterator) = ret("getIterator") {
            let mut keys = Vec::new();
            let mut values = Vec::new();
            for atom in iterator.atoms() {
                let (k, v) = match atom {
                    Atom::Named(n) => self.object_iteration(&n.name, &n.args, depth + 1),
                    _ => unknown(),
                };
                keys.push(k);
                values.push(v);
            }
            return (Type::union_all(keys.iter()), Type::union_all(values.iter()));
        }
        unknown()
    }
}

/// The type of `l ?? r`.
fn coalesce(l: &Type, r: &Type) -> Type {
    let can_be_null = l.contains_null() || l.has_unknown() || l.is_mixed();
    if can_be_null {
        l.without_null().union(r)
    } else {
        l.clone()
    }
}
