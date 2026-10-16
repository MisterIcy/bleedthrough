//! Call resolution: calls, method calls, static calls, `new` and first-class callables are
//! resolved against the symbol table with the receiver type, and every outcome says why.

use bleedthrough_syntax::{Arg, ArgKind, CallTarget, Expr, ExprKind, MemberName};

use super::ctx::ClassRes;
use super::facts::{CallResolution, DynamicKind, Target, TargetKind, UnresolvedReason};
use super::interp::Interp;
use crate::name::Fqn;
use crate::summary::{ClassKind, ParamDecl};
use crate::types::{Atom, CallableParam, CallableSig, Type};

/// A class a member is looked up on.
#[derive(Debug, Clone)]
struct ClassUse {
    class: Fqn,
    /// The receiver is `static`/`$this`: `static` in return types stays late-bound.
    late: bool,
    args: Vec<Type>,
}

/// One part of a receiver type.
#[derive(Debug, Clone)]
enum Receiver {
    /// Look the member up on these classes (several for an intersection: the first that has it).
    Classes(Vec<ClassUse>),
    Null,
    NotObject,
    Unknown(UnresolvedReason),
}

struct Found<'a> {
    target: Target,
    ret: Type,
    params: &'a [ParamDecl],
    truncated: bool,
}

/// What resolving a call produced.
pub(super) struct Outcome<'a> {
    resolution: CallResolution,
    ret: Type,
    params: Option<&'a [ParamDecl]>,
    sig: Option<CallableSig>,
}

impl<'a> Outcome<'a> {
    fn unresolved(reason: UnresolvedReason) -> Self {
        Self {
            resolution: CallResolution::Unresolved(reason),
            ret: Type::unknown(),
            params: None,
            sig: None,
        }
    }

    fn dynamic(kind: DynamicKind) -> Self {
        Self {
            resolution: CallResolution::Dynamic(kind),
            ret: Type::unknown(),
            params: None,
            sig: None,
        }
    }
}

fn literal_names(ty: &Type) -> Option<Vec<String>> {
    if ty.atoms().is_empty() {
        return None;
    }
    ty.atoms()
        .iter()
        .map(|a| match a {
            Atom::StringLiteral(s) => Some(s.to_string()),
            _ => None,
        })
        .collect()
}

fn plain(name: &str) -> &str {
    name.trim_start_matches('$')
}

impl<'a> Interp<'a> {
    pub fn arg(&mut self, arg: &'a Arg) -> Type {
        match &arg.kind {
            ArgKind::Positional(e) | ArgKind::Spread(e) | ArgKind::Named { value: e, .. } => {
                self.expr(e)
            }
            _ => Type::unknown(),
        }
    }

    /// Types and resolves a call, a first-class callable or a `new` expression.
    pub fn call_like(&mut self, e: &'a Expr) -> Type {
        match &e.kind {
            ExprKind::Call { target, args } => {
                let outcome = self.resolve_target(target);
                for arg in args {
                    self.arg(arg);
                }
                if let Some(params) = outcome.params {
                    self.apply_by_ref(params, args);
                }
                self.record_call(e, outcome.resolution);
                outcome.ret
            }
            ExprKind::CallableRef { target } => {
                let outcome = self.resolve_target(target);
                self.record_call(e, outcome.resolution);
                Type::of(Atom::Callable(outcome.sig.map(Box::new)))
            }
            ExprKind::New { class, args } => self.new_expr(e, class, args),
            _ => Type::unknown(),
        }
    }

    // ---- targets ---------------------------------------------------------------------------

    fn resolve_target(&mut self, target: &'a CallTarget) -> Outcome<'a> {
        match target {
            CallTarget::Function(callee) => self.function_call(callee),
            CallTarget::Method {
                object,
                member,
                nullsafe,
            } => {
                let object_ty = self.expr(object);
                let receivers = receivers_of(&object_ty);
                self.member_call(&receivers, member, *nullsafe)
            }
            CallTarget::Static { class, member } => {
                let class_ty = self.expr(class);
                let receivers = self.static_receivers(class, &class_ty);
                self.member_call(&receivers, member, false)
            }
            _ => Outcome::unresolved(UnresolvedReason::DynamicName),
        }
    }

    fn static_receivers(&self, class: &Expr, class_ty: &Type) -> Vec<Receiver> {
        match self.ctx.class_of_expr(class) {
            Some(ClassRes::Class(f)) => vec![Receiver::Classes(vec![ClassUse {
                class: f,
                late: false,
                args: Vec::new(),
            }])],
            Some(ClassRes::Late(f)) => vec![Receiver::Classes(vec![ClassUse {
                class: f,
                late: true,
                args: Vec::new(),
            }])],
            Some(_) => vec![Receiver::Unknown(UnresolvedReason::UnknownReceiverType)],
            None => class_ty
                .atoms()
                .iter()
                .map(|atom| match atom {
                    Atom::ClassString(Some(f)) => Receiver::Classes(vec![ClassUse {
                        class: f.clone(),
                        late: false,
                        args: Vec::new(),
                    }]),
                    other => receiver_of(other),
                })
                .collect(),
        }
    }

    fn function_call(&mut self, callee: &'a Expr) -> Outcome<'a> {
        if let ExprKind::Name(name) = &callee.kind {
            self.record(callee, &Type::of(Atom::Callable(None)));
            return self.named_function(self.ctx.resolve_function(name).as_ref());
        }
        let callee_ty = self.expr(callee);
        self.callable_value(&callee_ty)
    }

    fn named_function(&self, name: Option<&Fqn>) -> Outcome<'a> {
        let table = self.ctx.table;
        let Some(decl) = name.and_then(|n| table.function(n)) else {
            return Outcome::unresolved(UnresolvedReason::FunctionNotFound);
        };
        let ret = decl.ret.ty.flat_map(|a| match a {
            Atom::Template(_) => Type::unknown(),
            other => Type::of(other.clone()),
        });
        Outcome {
            resolution: CallResolution::resolved(vec![Target {
                kind: TargetKind::Function,
                owner: None,
                name: decl.name.as_str().into(),
                implementors: Vec::new(),
            }]),
            ret: ret.clone(),
            params: Some(&decl.params),
            sig: Some(signature_of(&decl.params, ret)),
        }
    }

    /// `$fn(...)`: resolved through what the value is known to be.
    #[allow(
        clippy::too_many_lines,
        reason = "one pass over the atoms of the callee's type"
    )]
    fn callable_value(&self, ty: &Type) -> Outcome<'a> {
        let mut targets: Vec<Target> = Vec::new();
        let mut rets: Vec<Type> = Vec::new();
        let mut residual: Vec<UnresolvedReason> = Vec::new();
        let mut dynamic: Option<DynamicKind> = None;
        let mut single_params: Option<&'a [ParamDecl]> = None;
        let mut sig = None;
        for atom in ty.atoms() {
            match atom {
                Atom::StringLiteral(s) if s.contains("::") => {
                    dynamic.get_or_insert(DynamicKind::CallableString);
                    rets.push(Type::unknown());
                }
                Atom::StringLiteral(s) => {
                    let fqn = Fqn::new(s.trim_start_matches('\\'));
                    let outcome = self.named_function(Some(&fqn));
                    match outcome.resolution {
                        CallResolution::Resolved { targets: t, .. } => {
                            targets.extend(t);
                            rets.push(outcome.ret);
                            single_params = outcome.params;
                            sig = outcome.sig;
                        }
                        _ => {
                            residual.push(UnresolvedReason::FunctionNotFound);
                            rets.push(Type::unknown());
                        }
                    }
                }
                Atom::Callable(Some(s)) => {
                    targets.push(Target {
                        kind: TargetKind::Closure,
                        owner: None,
                        name: "{closure}".into(),
                        implementors: Vec::new(),
                    });
                    rets.push(s.ret.clone());
                    sig = Some((**s).clone());
                }
                Atom::Named(_) | Atom::Static(_) => {
                    let receivers = [receiver_of(atom)];
                    let outcome =
                        self.method_outcome(&receivers, &["__invoke"], false, TargetKind::Method);
                    match outcome.resolution {
                        CallResolution::Resolved {
                            targets: t,
                            residual: r,
                        } => {
                            targets.extend(t);
                            residual.extend(r);
                            rets.push(outcome.ret);
                        }
                        CallResolution::Unresolved(r) => {
                            residual.push(r);
                            rets.push(Type::unknown());
                        }
                        CallResolution::Dynamic(d) => {
                            dynamic.get_or_insert(d);
                        }
                    }
                }
                _ => {
                    dynamic.get_or_insert(DynamicKind::VariableFunction);
                    rets.push(Type::unknown());
                }
            }
        }
        if ty.atoms().is_empty() {
            dynamic = Some(DynamicKind::VariableFunction);
        }
        if targets.is_empty() {
            return match dynamic {
                Some(kind) => Outcome::dynamic(kind),
                None => Outcome::unresolved(
                    residual
                        .iter()
                        .copied()
                        .min()
                        .unwrap_or(UnresolvedReason::UnknownReceiverType),
                ),
            };
        }
        if dynamic.is_some() {
            residual.push(UnresolvedReason::DynamicName);
        }
        targets.sort();
        targets.dedup();
        residual.sort();
        residual.dedup();
        let mono = targets.len() == 1;
        Outcome {
            resolution: CallResolution::Resolved { targets, residual },
            ret: Type::union_all(rets.iter()),
            params: if mono { single_params } else { None },
            sig: if mono { sig } else { None },
        }
    }

    fn member_call(
        &mut self,
        receivers: &[Receiver],
        member: &'a MemberName,
        nullsafe: bool,
    ) -> Outcome<'a> {
        match member {
            MemberName::Identifier { name, .. } => {
                self.method_outcome(receivers, &[name], nullsafe, TargetKind::Method)
            }
            MemberName::Dynamic(expr) => {
                let name_ty = self.expr(expr);
                match literal_names(&name_ty) {
                    Some(names) => {
                        let names: Vec<&str> = names.iter().map(String::as_str).collect();
                        self.method_outcome(receivers, &names, nullsafe, TargetKind::Method)
                    }
                    None => Outcome::dynamic(DynamicKind::VariableMethod),
                }
            }
            _ => Outcome::unresolved(UnresolvedReason::DynamicName),
        }
    }

    /// Resolves `names` on every part of the receiver and combines the outcomes.
    fn method_outcome(
        &self,
        receivers: &[Receiver],
        names: &[&str],
        nullsafe: bool,
        kind: TargetKind,
    ) -> Outcome<'a> {
        let mut targets: Vec<Target> = Vec::new();
        let mut residual: Vec<UnresolvedReason> = Vec::new();
        let mut rets: Vec<Type> = Vec::new();
        let mut params: Option<&'a [ParamDecl]> = None;
        let mut saw_null = false;
        for receiver in receivers {
            match receiver {
                Receiver::Null => saw_null = true,
                Receiver::NotObject => {
                    residual.push(UnresolvedReason::MethodNotFound);
                    rets.push(Type::unknown());
                }
                Receiver::Unknown(reason) => {
                    residual.push(*reason);
                    rets.push(Type::unknown());
                }
                Receiver::Classes(uses) => {
                    for name in names {
                        match self.lookup_method(uses, name, kind) {
                            Ok(found) => {
                                if found.truncated {
                                    residual.push(UnresolvedReason::LimitHit);
                                }
                                params = Some(found.params);
                                targets.push(found.target);
                                rets.push(found.ret);
                            }
                            Err(MethodMiss::NoConstructor) => {}
                            Err(MethodMiss::Reason(reason)) => {
                                residual.push(reason);
                                rets.push(Type::unknown());
                            }
                        }
                    }
                }
            }
        }
        let ret_of = |rets: &[Type]| {
            let mut ret = Type::union_all(rets.iter());
            if nullsafe && saw_null {
                ret = ret.union(&Type::null());
            }
            if rets.is_empty() && !nullsafe {
                ret = Type::unknown();
            }
            ret
        };
        let resolved_anything = !targets.is_empty()
            || (kind == TargetKind::Constructor && residual.is_empty() && !receivers.is_empty());
        if !resolved_anything {
            let reason = residual.iter().copied().min().unwrap_or(if saw_null {
                UnresolvedReason::MethodNotFound
            } else {
                UnresolvedReason::UnknownReceiverType
            });
            return Outcome {
                ret: ret_of(&rets),
                ..Outcome::unresolved(reason)
            };
        }
        targets.sort();
        targets.dedup();
        residual.sort();
        residual.dedup();
        let mono = targets.len() == 1;
        let sig = if mono {
            params.map(|p| signature_of(p, ret_of(&rets)))
        } else {
            None
        };
        Outcome {
            resolution: CallResolution::Resolved { targets, residual },
            ret: ret_of(&rets),
            params: if mono { params } else { None },
            sig,
        }
    }

    fn lookup_method(
        &self,
        uses: &[ClassUse],
        name: &str,
        kind: TargetKind,
    ) -> Result<Found<'a>, MethodMiss> {
        let table = self.ctx.table;
        let caller = self.ctx.class.current();
        let mut miss: Option<UnresolvedReason> = None;
        for class_use in uses {
            let lookup = table.method(&class_use.class, name, caller);
            if let Some(candidate) = lookup.accessible() {
                let (member, owner) = (candidate.member, candidate.owner);
                let mut implementors: Vec<Fqn> = Vec::new();
                let mut truncated = false;
                if owner.kind == ClassKind::Interface || member.is_abstract {
                    implementors = table
                        .implementors(&owner.name)
                        .iter()
                        .map(|c| c.name.clone())
                        .collect();
                    if implementors.len() > self.limits.max_candidates {
                        implementors.truncate(self.limits.max_candidates);
                        truncated = true;
                    }
                }
                let mut bindings: Vec<(&str, Type)> = Vec::new();
                for (i, template) in owner.templates.iter().enumerate() {
                    let bound = if owner.name == class_use.class {
                        class_use.args.get(i).cloned()
                    } else {
                        None
                    };
                    bindings.push((template, bound.unwrap_or_else(Type::unknown)));
                }
                for template in &member.templates {
                    bindings.push((template, Type::unknown()));
                }
                return Ok(Found {
                    target: Target {
                        kind,
                        owner: Some(owner.name.clone()),
                        name: member.name.clone(),
                        implementors,
                    },
                    ret: adapt_return(&member.ret.ty, class_use, &bindings),
                    params: &member.params,
                    truncated,
                });
            }
            let reason = if !lookup.candidates.is_empty() {
                UnresolvedReason::NotAccessible
            } else if lookup.magic.any() {
                if kind == TargetKind::Constructor {
                    return Err(MethodMiss::NoConstructor);
                }
                UnresolvedReason::MagicMethod
            } else if lookup.incomplete {
                UnresolvedReason::ClassNotLoaded
            } else if kind == TargetKind::Constructor {
                return Err(MethodMiss::NoConstructor);
            } else {
                UnresolvedReason::MethodNotFound
            };
            miss = miss.or(Some(reason));
        }
        Err(MethodMiss::Reason(
            miss.unwrap_or(UnresolvedReason::UnknownReceiverType),
        ))
    }

    // ---- new -------------------------------------------------------------------------------

    fn new_expr(&mut self, e: &'a Expr, class: &'a Expr, args: &'a [Arg]) -> Type {
        let class_ty = self.expr(class);
        let (object, receivers, dynamic) = match self.ctx.class_of_expr(class) {
            Some(ClassRes::Class(f)) => (
                Type::object(f.clone()),
                vec![Receiver::Classes(vec![ClassUse {
                    class: f,
                    late: false,
                    args: Vec::new(),
                }])],
                false,
            ),
            Some(ClassRes::Late(f)) => (
                Type::of(Atom::Static(f.clone())),
                vec![Receiver::Classes(vec![ClassUse {
                    class: f,
                    late: true,
                    args: Vec::new(),
                }])],
                false,
            ),
            Some(_) => (
                Type::of(Atom::Object),
                vec![Receiver::Unknown(UnresolvedReason::UnknownReceiverType)],
                false,
            ),
            None => self.dynamic_new(&class_ty),
        };
        let outcome = if dynamic {
            Outcome::dynamic(DynamicKind::VariableClass)
        } else {
            self.method_outcome(&receivers, &["__construct"], false, TargetKind::Constructor)
        };
        for arg in args {
            self.arg(arg);
        }
        if let Some(params) = outcome.params {
            self.apply_by_ref(params, args);
        }
        self.record_call(e, outcome.resolution);
        object
    }

    /// `new $class`: resolvable when `$class` is a `class-string<T>` or an object of known class.
    fn dynamic_new(&self, class_ty: &Type) -> (Type, Vec<Receiver>, bool) {
        let mut objects: Vec<Type> = Vec::new();
        let mut receivers: Vec<Receiver> = Vec::new();
        for atom in class_ty.atoms() {
            let class = match atom {
                Atom::ClassString(Some(f)) => Some(f.clone()),
                Atom::Named(n) => Some(n.name.clone()),
                _ => None,
            };
            match class {
                Some(f) => {
                    objects.push(Type::object(f.clone()));
                    receivers.push(Receiver::Classes(vec![ClassUse {
                        class: f,
                        late: false,
                        args: Vec::new(),
                    }]));
                }
                None => return (Type::of(Atom::Object), Vec::new(), true),
            }
        }
        if objects.is_empty() {
            return (Type::of(Atom::Object), Vec::new(), true);
        }
        (Type::union_all(objects.iter()), receivers, false)
    }

    // ---- by-reference arguments ------------------------------------------------------------

    fn apply_by_ref(&mut self, params: &'a [ParamDecl], args: &'a [Arg]) {
        for (i, arg) in args.iter().enumerate() {
            let (expr, name) = match &arg.kind {
                ArgKind::Positional(e) => (e, None),
                ArgKind::Named { name, value } => (value, Some(name)),
                _ => continue,
            };
            let param = match name {
                Some(n) => params.iter().find(|p| plain(&p.name) == plain(n)),
                None => params
                    .get(i)
                    .or_else(|| params.last().filter(|p| p.variadic)),
            };
            if let Some(p) = param
                && p.by_ref
            {
                self.store_lvalue(expr, p.ty.ty.clone());
            }
        }
    }

    // ---- property and constant access ------------------------------------------------------

    pub fn property_type(
        &mut self,
        object_ty: &Type,
        member: &'a MemberName,
        nullsafe: bool,
    ) -> Type {
        let names: Vec<String> = match member {
            MemberName::Identifier { name, .. } => vec![name.to_string()],
            MemberName::Dynamic(expr) => match literal_names(&self.expr(expr)) {
                Some(names) => names,
                None => return Type::unknown(),
            },
            _ => return Type::unknown(),
        };
        let table = self.ctx.table;
        let caller = self.ctx.class.current();
        let mut parts: Vec<Type> = Vec::new();
        let mut saw_null = false;
        for receiver in receivers_of(object_ty) {
            match receiver {
                Receiver::Null => saw_null = true,
                Receiver::Classes(uses) => {
                    for name in &names {
                        let found = uses.iter().find_map(|u| {
                            let lookup = table.property(&u.class, name, caller);
                            let candidate = lookup.accessible().or(lookup.candidates.first())?;
                            let owner = candidate.owner;
                            let ty = candidate.member.ty.ty.clone();
                            Some((u, owner, ty))
                        });
                        parts.push(match found {
                            Some((u, owner, ty)) => {
                                let mut bindings: Vec<(&str, Type)> = Vec::new();
                                for (i, template) in owner.templates.iter().enumerate() {
                                    let bound = (owner.name == u.class)
                                        .then(|| u.args.get(i).cloned())
                                        .flatten();
                                    bindings.push((template, bound.unwrap_or_else(Type::unknown)));
                                }
                                adapt_return(&ty, u, &bindings)
                            }
                            None => Type::unknown(),
                        });
                    }
                }
                _ => parts.push(Type::unknown()),
            }
        }
        if nullsafe && saw_null {
            parts.push(Type::null());
        }
        if parts.is_empty() {
            return Type::unknown();
        }
        Type::union_all(parts.iter())
    }

    pub fn static_property_type(&self, class: &Expr, name: &str) -> Type {
        let Some(fqn) = self.ctx.class_of_expr(class).and_then(|r| r.fqn().cloned()) else {
            return Type::unknown();
        };
        let lookup = self
            .ctx
            .table
            .property(&fqn, name, self.ctx.class.current());
        lookup
            .accessible()
            .or(lookup.candidates.first())
            .map_or_else(Type::unknown, |c| c.member.ty.ty.clone())
    }

    pub fn class_constant_type(&mut self, class: &'a Expr, member: &'a MemberName) -> Type {
        let name = match member {
            MemberName::Identifier { name, .. } => name,
            MemberName::Dynamic(expr) => {
                self.expr(expr);
                return Type::unknown();
            }
            _ => return Type::unknown(),
        };
        let resolved = self.ctx.class_of_expr(class);
        let fqn = resolved.as_ref().and_then(|r| r.fqn().cloned());
        if name.eq_ignore_ascii_case("class") {
            return Type::of(Atom::ClassString(fqn));
        }
        let Some(fqn) = fqn else {
            return Type::unknown();
        };
        let table = self.ctx.table;
        if table.enum_case(&fqn, name).is_some() {
            return Type::of(Atom::EnumCase {
                enum_name: fqn,
                case: name.clone(),
            });
        }
        table
            .class_constant(&fqn, name, self.ctx.class.current())
            .candidates
            .first()
            .map_or_else(Type::unknown, |c| c.member.ty.clone())
    }
}

enum MethodMiss {
    Reason(UnresolvedReason),
    /// A class without a constructor: `new` has nothing to call.
    NoConstructor,
}

fn receiver_of(atom: &Atom) -> Receiver {
    let plain_class = |class: Fqn, late: bool, args: Vec<Type>| {
        Receiver::Classes(vec![ClassUse { class, late, args }])
    };
    match atom {
        Atom::Named(n) => plain_class(n.name.clone(), false, n.args.clone()),
        Atom::Static(f) => plain_class(f.clone(), true, Vec::new()),
        Atom::EnumCase { enum_name, .. } => plain_class(enum_name.clone(), false, Vec::new()),
        Atom::Callable(_) => plain_class(Fqn::new("Closure"), false, Vec::new()),
        Atom::Intersection(members) => {
            let uses: Vec<ClassUse> = members
                .iter()
                .filter_map(|m| match m {
                    Atom::Named(n) => Some(ClassUse {
                        class: n.name.clone(),
                        late: false,
                        args: n.args.clone(),
                    }),
                    Atom::Static(f) => Some(ClassUse {
                        class: f.clone(),
                        late: true,
                        args: Vec::new(),
                    }),
                    _ => None,
                })
                .collect();
            if uses.is_empty() {
                Receiver::Unknown(UnresolvedReason::UnknownReceiverType)
            } else {
                Receiver::Classes(uses)
            }
        }
        Atom::Null | Atom::Void => Receiver::Null,
        Atom::Mixed | Atom::Unknown | Atom::Object | Atom::Template(_) => {
            Receiver::Unknown(UnresolvedReason::UnknownReceiverType)
        }
        _ => Receiver::NotObject,
    }
}

fn receivers_of(ty: &Type) -> Vec<Receiver> {
    if ty.atoms().is_empty() {
        return vec![Receiver::Unknown(UnresolvedReason::UnknownReceiverType)];
    }
    ty.atoms().iter().map(receiver_of).collect()
}

/// A declared return or property type seen from a receiver: template parameters replaced,
/// `static` bound to the receiver, and what stays unbound made `unknown`.
fn adapt_return(ret: &Type, receiver: &ClassUse, bindings: &[(&str, Type)]) -> Type {
    ret.substitute(bindings).flat_map(|atom| match atom {
        Atom::Static(_) => {
            if receiver.late {
                Type::of(Atom::Static(receiver.class.clone()))
            } else {
                Type::object(receiver.class.clone())
            }
        }
        Atom::Template(_) => Type::unknown(),
        other => Type::of(other.clone()),
    })
}

fn signature_of(params: &[ParamDecl], ret: Type) -> CallableSig {
    CallableSig {
        params: params
            .iter()
            .map(|p| CallableParam {
                ty: p.ty.ty.clone(),
                optional: p.has_default || p.variadic,
                variadic: p.variadic,
            })
            .collect(),
        ret,
    }
}
