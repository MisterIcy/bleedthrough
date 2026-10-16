//! Narrowing: from a condition and a state, the state when the condition is true and when it is
//! false. Only the documented subset is recognised; every other condition leaves both unchanged.
//!
//! Recognised, on simple variables (`$x`):
//!
//! - `$x instanceof C`
//! - `$x === null`, `!== null`, `== null`, `!= null`, `is_null($x)`
//! - `is_string`, `is_int`, `is_float`, `is_bool`, `is_array`, `is_object`, `is_callable`,
//!   `is_numeric` (and the aliases `is_integer`, `is_long`, `is_double`)
//! - `isset($x, ...)` and `empty($x)`
//! - `$x === <literal>` and `!==` for string, integer, boolean and null literals
//! - `!` of any of these, and `&&`, `||`, `and`, `or` of any of these
//!
//! Property fetches, array elements and calls to user functions are not narrowed, and neither is a
//! bare truthiness test.

use bleedthrough_syntax::{ArgKind, BinaryOp, CallTarget, Expr, ExprKind, Literal, UnaryOp};

use super::ctx::Ctx;
use super::env::Env;
use super::ops::{falsy_part, truthy_part};
use crate::name::Fqn;
use crate::summary::{int_value, string_value};
use crate::table::Subtyping;
use crate::types::{Atom, Type};

type Pair = (Env, Env);

/// The states when `cond` is true and when it is false, starting from `env`.
pub(super) fn narrow(ctx: &Ctx<'_>, cond: &Expr, env: &Env) -> Pair {
    let unchanged = || (env.clone(), env.clone());
    match &cond.kind {
        ExprKind::Parenthesized(inner) => narrow(ctx, inner, env),
        ExprKind::Unary {
            op: UnaryOp::Not,
            operand,
        } => {
            let (t, f) = narrow(ctx, operand, env);
            (f, t)
        }
        ExprKind::Binary {
            op: BinaryOp::And | BinaryOp::LowAnd,
            lhs,
            rhs,
        } => {
            let (t1, f1) = narrow(ctx, lhs, env);
            let (t2, f2) = narrow(ctx, rhs, &t1);
            (t2, f1.join(&f2))
        }
        ExprKind::Binary {
            op: BinaryOp::Or | BinaryOp::LowOr,
            lhs,
            rhs,
        } => {
            let (t1, f1) = narrow(ctx, lhs, env);
            let (t2, f2) = narrow(ctx, rhs, &f1);
            (t1.join(&t2), f2)
        }
        ExprKind::Binary {
            op: BinaryOp::Instanceof,
            lhs,
            rhs,
        } => match (variable(lhs), ctx.class_of_expr(rhs)) {
            (Some(name), Some(class)) => match class.fqn() {
                Some(c) => with_var(env, name, |t| {
                    (instanceof_true(ctx, t, c), instanceof_false(ctx, t, c))
                }),
                None => unchanged(),
            },
            _ => unchanged(),
        },
        ExprKind::Binary {
            op:
                op @ (BinaryOp::Identical
                | BinaryOp::NotIdentical
                | BinaryOp::Eq
                | BinaryOp::NotEq
                | BinaryOp::AngledNotEq),
            lhs,
            rhs,
        } => compare(*op, lhs, rhs, env).unwrap_or_else(unchanged),
        ExprKind::Isset(items) => isset(items, env),
        ExprKind::Empty(inner) => match variable(inner) {
            Some(name) => with_var(env, name, |t| (falsy_part(t), truthy_part(t))),
            None => unchanged(),
        },
        ExprKind::Call {
            target: CallTarget::Function(callee),
            args,
        } => type_test(ctx, callee, args, env).unwrap_or_else(unchanged),
        _ => unchanged(),
    }
}

pub(super) fn variable(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Variable(name) => Some(name.trim_start_matches('$')),
        ExprKind::Parenthesized(inner) => variable(inner),
        _ => None,
    }
}

/// Applies `f` to the variable's type, producing its type in the true and the false state.
fn with_var(env: &Env, name: &str, f: impl Fn(&Type) -> (Type, Type)) -> Pair {
    let current = env.get(name);
    let (yes, no) = f(&current);
    let mut t = env.clone();
    t.set(name, yes);
    let mut e = env.clone();
    e.set(name, no);
    (t, e)
}

pub(super) fn literal(expr: &Expr) -> Option<Atom> {
    match &expr.kind {
        ExprKind::Literal(Literal::Int { raw }) => int_value(raw).map(Atom::IntLiteral),
        ExprKind::Literal(Literal::String { .. }) => {
            string_value(expr).map(|s| Atom::StringLiteral(s.into()))
        }
        ExprKind::Literal(Literal::Bool(b)) => Some(if *b { Atom::True } else { Atom::False }),
        ExprKind::Literal(Literal::Null) => Some(Atom::Null),
        ExprKind::ConstFetch(name) => match name.text.to_ascii_lowercase().as_str() {
            "true" => Some(Atom::True),
            "false" => Some(Atom::False),
            "null" => Some(Atom::Null),
            _ => None,
        },
        ExprKind::Unary {
            op: UnaryOp::Neg,
            operand,
        } => match &operand.kind {
            ExprKind::Literal(Literal::Int { raw }) => int_value(raw)
                .and_then(i64::checked_neg)
                .map(Atom::IntLiteral),
            _ => None,
        },
        ExprKind::Parenthesized(inner) => literal(inner),
        _ => None,
    }
}

fn compare(op: BinaryOp, lhs: &Expr, rhs: &Expr, env: &Env) -> Option<Pair> {
    let (name, lit) = match (variable(lhs), literal(rhs), variable(rhs), literal(lhs)) {
        (Some(v), Some(l), _, _) | (_, _, Some(v), Some(l)) => (v, l),
        _ => return None,
    };
    let negated = matches!(
        op,
        BinaryOp::NotIdentical | BinaryOp::NotEq | BinaryOp::AngledNotEq
    );
    let strict = matches!(op, BinaryOp::Identical | BinaryOp::NotIdentical);
    let (yes, no) = if lit == Atom::Null {
        with_var(env, name, |t| {
            let when_null = if strict {
                only_null(t)
            } else {
                t.filter(|a| !is_object(a))
            };
            (when_null, t.without_null())
        })
    } else if strict {
        with_var(env, name, |t| {
            (restrict_to(t, &lit), remove_literal(t, &lit))
        })
    } else {
        return None;
    };
    Some(if negated { (no, yes) } else { (yes, no) })
}

fn is_object(atom: &Atom) -> bool {
    matches!(
        atom,
        Atom::Named(_)
            | Atom::Static(_)
            | Atom::EnumCase { .. }
            | Atom::Object
            | Atom::Intersection(_)
    )
}

/// What remains of a type when the value is known to be null.
fn only_null(ty: &Type) -> Type {
    ty.flat_map(|a| match a {
        Atom::Null | Atom::Void => Type::null(),
        Atom::Mixed | Atom::Unknown | Atom::Template(_) => Type::null(),
        _ => Type::never(),
    })
}

fn literal_within(general: &Atom, lit: &Atom) -> bool {
    match (general, lit) {
        (Atom::Int, Atom::IntLiteral(_)) | (Atom::Bool, Atom::True | Atom::False) => true,
        (Atom::IntRange { min, max }, Atom::IntLiteral(i)) => {
            min.is_none_or(|m| *i >= m) && max.is_none_or(|m| *i <= m)
        }
        (Atom::String, Atom::StringLiteral(_)) => true,
        (Atom::NonEmptyString, Atom::StringLiteral(s)) => !s.is_empty(),
        (Atom::NumericString, Atom::StringLiteral(s)) => s.trim().parse::<f64>().is_ok(),
        _ => false,
    }
}

fn restrict_to(ty: &Type, lit: &Atom) -> Type {
    ty.flat_map(|a| {
        if a == lit || literal_within(a, lit) || matches!(a, Atom::Mixed | Atom::Unknown) {
            Type::of(lit.clone())
        } else {
            Type::never()
        }
    })
}

fn remove_literal(ty: &Type, lit: &Atom) -> Type {
    ty.flat_map(|a| match (a, lit) {
        _ if a == lit => Type::never(),
        (Atom::Bool, Atom::True) => Type::of(Atom::False),
        (Atom::Bool, Atom::False) => Type::of(Atom::True),
        _ => Type::of(a.clone()),
    })
}

fn isset(items: &[Expr], env: &Env) -> Pair {
    let names: Vec<&str> = items.iter().filter_map(variable).collect();
    if names.len() != items.len() || names.is_empty() {
        return (env.clone(), env.clone());
    }
    let mut yes = env.clone();
    for name in &names {
        let current = env.lookup(name).cloned().unwrap_or_else(Type::mixed);
        yes.set(name, current.without_null());
    }
    let no = if let [name] = names.as_slice() {
        let mut no = env.clone();
        let current = env.lookup(name).cloned().unwrap_or_else(Type::mixed);
        no.set(name, only_null(&current));
        no
    } else {
        env.clone()
    };
    (yes, no)
}

fn type_test(
    ctx: &Ctx<'_>,
    callee: &Expr,
    args: &[bleedthrough_syntax::Arg],
    env: &Env,
) -> Option<Pair> {
    let ExprKind::Name(name) = &callee.kind else {
        return None;
    };
    let [arg] = args else {
        return None;
    };
    let ArgKind::Positional(value) = &arg.kind else {
        return None;
    };
    let var = variable(value)?;
    let function = ctx.resolve_function(name)?;
    if ctx
        .table
        .function(&function)
        .is_some_and(|f| f.builtin.is_none())
    {
        return None;
    }
    let test = function.key();
    let pair = match test {
        "is_null" => with_var(env, var, |t| (only_null(t), t.without_null())),
        "is_string" => refine(env, var, Atom::is_string, Type::string()),
        "is_int" | "is_integer" | "is_long" => refine(env, var, Atom::is_int, Type::int()),
        "is_float" | "is_double" => refine(env, var, |a| *a == Atom::Float, Type::float()),
        "is_bool" => refine(env, var, Atom::is_bool, Type::bool()),
        "is_array" => refine(
            env,
            var,
            is_array,
            Type::array_of(Type::array_key(), Type::mixed()),
        ),
        "is_object" => refine(env, var, is_object, Type::of(Atom::Object)),
        "is_callable" => with_var(env, var, |t| {
            let yes = t.flat_map(|a| match a {
                Atom::Callable(_) | Atom::Named(_) | Atom::Static(_) | Atom::Object => {
                    Type::of(a.clone())
                }
                a if a.is_string() || is_array(a) => Type::of(a.clone()),
                Atom::Mixed | Atom::Unknown | Atom::Template(_) => Type::of(Atom::Callable(None)),
                _ => Type::never(),
            });
            (yes, t.filter(|a| !matches!(a, Atom::Callable(_))))
        }),
        "is_numeric" => with_var(env, var, |t| {
            let yes = t.flat_map(|a| match a {
                a if a.is_int() => Type::of(a.clone()),
                Atom::Float | Atom::NumericString => Type::of(a.clone()),
                Atom::StringLiteral(s) if s.trim().parse::<f64>().is_ok() => Type::of(a.clone()),
                Atom::String | Atom::NonEmptyString => Type::of(Atom::NumericString),
                Atom::Mixed | Atom::Unknown | Atom::Template(_) => {
                    Type::from_atoms([Atom::Int, Atom::Float, Atom::NumericString])
                }
                _ => Type::never(),
            });
            let no = t.filter(|a| {
                !(a.is_int()
                    || matches!(a, Atom::Float | Atom::NumericString)
                    || matches!(a, Atom::StringLiteral(s) if s.trim().parse::<f64>().is_ok()))
            });
            (yes, no)
        }),
        _ => return None,
    };
    Some(pair)
}

fn is_array(atom: &Atom) -> bool {
    matches!(
        atom,
        Atom::Array { .. } | Atom::List { .. } | Atom::Shape { .. }
    )
}

fn refine(env: &Env, var: &str, keep: impl Fn(&Atom) -> bool, widen: Type) -> Pair {
    with_var(env, var, |t| {
        let yes = t.flat_map(|a| {
            if keep(a) {
                Type::of(a.clone())
            } else if matches!(a, Atom::Mixed | Atom::Unknown | Atom::Template(_)) {
                widen.clone()
            } else {
                Type::never()
            }
        });
        (yes, t.filter(|a| !keep(a)))
    })
}

fn sub(ctx: &Ctx<'_>, a: &Fqn, b: &Fqn) -> Subtyping {
    ctx.table.is_subtype(a, b)
}

fn instanceof_true(ctx: &Ctx<'_>, ty: &Type, class: &Fqn) -> Type {
    let narrowed = |name: &Fqn, original: &Atom| -> Type {
        if sub(ctx, name, class) == Subtyping::Yes {
            return Type::of(original.clone());
        }
        if sub(ctx, class, name) == Subtyping::Yes {
            return Type::object(class.clone());
        }
        let both_known_classes = ctx.table.class(name).is_some()
            && ctx.table.class(class).is_some()
            && !ctx.table.is_interface(name)
            && !ctx.table.is_interface(class);
        if both_known_classes && sub(ctx, name, class) == Subtyping::No {
            Type::never()
        } else if ctx.table.class(name).is_none() {
            Type::object(class.clone())
        } else {
            Type::of(Atom::Intersection(vec![
                Atom::named(name.clone()),
                Atom::named(class.clone()),
            ]))
        }
    };
    ty.flat_map(|a| match a {
        Atom::Named(n) => narrowed(&n.name, a),
        Atom::Static(f) => narrowed(f, a),
        Atom::EnumCase { enum_name, .. } => {
            if sub(ctx, enum_name, class) == Subtyping::No {
                Type::never()
            } else {
                Type::of(a.clone())
            }
        }
        Atom::Intersection(members) => {
            let mut members = members.clone();
            members.push(Atom::named(class.clone()));
            Type::of(Atom::Intersection(members))
        }
        Atom::Mixed | Atom::Unknown | Atom::Object | Atom::Template(_) | Atom::Callable(_) => {
            Type::object(class.clone())
        }
        _ => Type::never(),
    })
}

fn instanceof_false(ctx: &Ctx<'_>, ty: &Type, class: &Fqn) -> Type {
    ty.filter(|a| match a {
        Atom::Named(n) => sub(ctx, &n.name, class) != Subtyping::Yes,
        Atom::Static(f) => sub(ctx, f, class) != Subtyping::Yes,
        Atom::EnumCase { enum_name, .. } => sub(ctx, enum_name, class) != Subtyping::Yes,
        _ => true,
    })
}

/// For `match ($x) { 1, 2 => ..., }`: the state inside an arm whose conditions are all literals,
/// and the state when none of them matched. `None` when the subject is not a simple variable or a
/// condition is not a literal.
pub(super) fn match_literals(subject: &Expr, conditions: &[Expr], env: &Env) -> Option<Pair> {
    let name = variable(subject)?;
    let lits: Vec<Atom> = conditions.iter().map(literal).collect::<Option<_>>()?;
    if lits.is_empty() {
        return None;
    }
    Some(with_var(env, name, |t| {
        let yes: Vec<Type> = lits.iter().map(|l| restrict_to(t, l)).collect();
        let no = lits
            .iter()
            .fold(t.clone(), |acc, l| remove_literal(&acc, l));
        (Type::union_all(yes.iter()), no)
    }))
}
