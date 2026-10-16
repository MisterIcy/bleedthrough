//! The results of inference: a type and a call resolution per expression, and the uncertainty
//! counts a function's analysis ended with. Everything is keyed by [`ExprKey`] and sorted, so two
//! runs over the same input produce identical facts.

use std::collections::BTreeMap;
use std::fmt;

use bleedthrough_syntax::{Expr, ExprKind, Span};

use crate::name::Fqn;
use crate::types::Type;

/// Term: exempt
///
/// The syntactic shape of an expression, as part of its key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
#[allow(missing_docs, reason = "one variant per expression kind")]
pub enum ExprTag {
    Literal,
    Interpolated,
    Variable,
    VariableVariable,
    Name,
    ConstFetch,
    ClassRef,
    MagicConstant,
    Array,
    Index,
    Property,
    StaticProperty,
    ClassConst,
    Call,
    CallableRef,
    New,
    AnonymousClass,
    Unary,
    Cast,
    Binary,
    Assign,
    Conditional,
    Match,
    Closure,
    ArrowFn,
    Yield,
    YieldFrom,
    Throw,
    Clone,
    Print,
    Include,
    Eval,
    Exit,
    Isset,
    Empty,
    Pipe,
    Parenthesized,
    Error,
    Unsupported,
}

impl ExprTag {
    /// The tag of an expression kind.
    pub fn of(kind: &ExprKind) -> Self {
        match kind {
            ExprKind::Literal(_) => Self::Literal,
            ExprKind::Interpolated { .. } => Self::Interpolated,
            ExprKind::Variable(_) => Self::Variable,
            ExprKind::VariableVariable(_) => Self::VariableVariable,
            ExprKind::Name(_) => Self::Name,
            ExprKind::ConstFetch(_) => Self::ConstFetch,
            ExprKind::ClassRef(_) => Self::ClassRef,
            ExprKind::MagicConstant(_) => Self::MagicConstant,
            ExprKind::Array { .. } => Self::Array,
            ExprKind::Index { .. } => Self::Index,
            ExprKind::Property { .. } => Self::Property,
            ExprKind::StaticProperty { .. } => Self::StaticProperty,
            ExprKind::ClassConst { .. } => Self::ClassConst,
            ExprKind::Call { .. } => Self::Call,
            ExprKind::CallableRef { .. } => Self::CallableRef,
            ExprKind::New { .. } => Self::New,
            ExprKind::AnonymousClass { .. } => Self::AnonymousClass,
            ExprKind::Unary { .. } => Self::Unary,
            ExprKind::Cast { .. } => Self::Cast,
            ExprKind::Binary { .. } => Self::Binary,
            ExprKind::Assign { .. } => Self::Assign,
            ExprKind::Conditional { .. } => Self::Conditional,
            ExprKind::Match { .. } => Self::Match,
            ExprKind::Closure { .. } => Self::Closure,
            ExprKind::ArrowFn { .. } => Self::ArrowFn,
            ExprKind::Yield { .. } => Self::Yield,
            ExprKind::YieldFrom(_) => Self::YieldFrom,
            ExprKind::Throw(_) => Self::Throw,
            ExprKind::Clone(_) => Self::Clone,
            ExprKind::Print(_) => Self::Print,
            ExprKind::Include { .. } => Self::Include,
            ExprKind::Eval(_) => Self::Eval,
            ExprKind::Exit { .. } => Self::Exit,
            ExprKind::Isset(_) => Self::Isset,
            ExprKind::Empty(_) => Self::Empty,
            ExprKind::Pipe { .. } => Self::Pipe,
            ExprKind::Parenthesized(_) => Self::Parenthesized,
            ExprKind::Error { .. } => Self::Error,
            _ => Self::Unsupported,
        }
    }

    /// The stable lowercase name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Literal => "literal",
            Self::Interpolated => "interpolated",
            Self::Variable => "variable",
            Self::VariableVariable => "variable-variable",
            Self::Name => "name",
            Self::ConstFetch => "const-fetch",
            Self::ClassRef => "class-ref",
            Self::MagicConstant => "magic-constant",
            Self::Array => "array",
            Self::Index => "index",
            Self::Property => "property",
            Self::StaticProperty => "static-property",
            Self::ClassConst => "class-const",
            Self::Call => "call",
            Self::CallableRef => "callable-ref",
            Self::New => "new",
            Self::AnonymousClass => "anonymous-class",
            Self::Unary => "unary",
            Self::Cast => "cast",
            Self::Binary => "binary",
            Self::Assign => "assign",
            Self::Conditional => "conditional",
            Self::Match => "match",
            Self::Closure => "closure",
            Self::ArrowFn => "arrow-fn",
            Self::Yield => "yield",
            Self::YieldFrom => "yield-from",
            Self::Throw => "throw",
            Self::Clone => "clone",
            Self::Print => "print",
            Self::Include => "include",
            Self::Eval => "eval",
            Self::Exit => "exit",
            Self::Isset => "isset",
            Self::Empty => "empty",
            Self::Pipe => "pipe",
            Self::Parenthesized => "parenthesized",
            Self::Error => "error",
            Self::Unsupported => "unsupported",
        }
    }
}

/// Term: exempt
///
/// Where an expression is and what it is: byte span plus shape. Keys sort by start, then end, then
/// shape, which is the order every fact list uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct ExprKey {
    /// Start offset.
    pub start: u32,
    /// End offset.
    pub end: u32,
    /// The expression's shape.
    pub tag: ExprTag,
}

impl ExprKey {
    /// The key of an expression.
    pub fn of(expr: &Expr) -> Self {
        Self {
            start: expr.span.start,
            end: expr.span.end,
            tag: ExprTag::of(&expr.kind),
        }
    }
}

impl fmt::Display for ExprKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}:{}", self.start, self.end, self.tag.as_str())
    }
}

/// Term: Call resolution
///
/// What kind of declaration a call resolved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum TargetKind {
    /// A function.
    Function,
    /// A method, instance or static.
    Method,
    /// A constructor, reached through `new`.
    Constructor,
    /// A closure value whose signature is known but whose identity is not tracked.
    Closure,
}

impl TargetKind {
    /// The stable lowercase name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Constructor => "constructor",
            Self::Closure => "closure",
        }
    }
}

/// Term: Call resolution
///
/// One declaration a call may reach.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct Target {
    /// What kind of declaration it is.
    pub kind: TargetKind,
    /// The class-like that declares it; `None` for functions and closures.
    pub owner: Option<Fqn>,
    /// The function or method name as declared.
    pub name: Box<str>,
    /// For an interface or abstract method: the loaded classes that implement it, in name order.
    pub implementors: Vec<Fqn>,
}

/// Term: Call resolution
///
/// Why a call could not be resolved. The vocabulary is closed; the finding model's confidence
/// reasons are derived from it (see `docs/symbols-and-types.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum UnresolvedReason {
    /// The receiver's type is unknown, `mixed`, `object` or a template parameter.
    UnknownReceiverType,
    /// The class is not loaded, or an ancestor of it is not.
    ClassNotLoaded,
    /// The class is fully known and has no such method (or the receiver is not an object).
    MethodNotFound,
    /// Only a magic method (`__call`, `__callStatic`) would handle the call.
    MagicMethod,
    /// The method exists but the calling context may not access it.
    NotAccessible,
    /// No function of that name is declared.
    FunctionNotFound,
    /// Part of the callee is computed at run time.
    DynamicName,
    /// A work limit stopped the resolution.
    LimitHit,
}

impl UnresolvedReason {
    /// The stable snake_case code.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnknownReceiverType => "unknown_receiver_type",
            Self::ClassNotLoaded => "class_not_loaded",
            Self::MethodNotFound => "method_not_found",
            Self::MagicMethod => "magic_method",
            Self::NotAccessible => "not_accessible",
            Self::FunctionNotFound => "function_not_found",
            Self::DynamicName => "dynamic_name",
            Self::LimitHit => "limit_hit",
        }
    }
}

/// Term: Call resolution
///
/// Which part of a call is computed at run time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum DynamicKind {
    /// `$fn(...)` where `$fn` is not a known closure or a string literal.
    VariableFunction,
    /// `$object->$name(...)` or `Class::$name(...)` with a name that is not a literal.
    VariableMethod,
    /// `new $class` where `$class` is not a `class-string<T>`.
    VariableClass,
    /// A callable string such as `'Class::method'`.
    CallableString,
}

impl DynamicKind {
    /// The stable snake_case code.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VariableFunction => "variable_function",
            Self::VariableMethod => "variable_method",
            Self::VariableClass => "variable_class",
            Self::CallableString => "callable_string",
        }
    }
}

/// Term: Call resolution
///
/// The outcome of resolving one call, method call, static call, `new` or first-class-callable
/// expression. A resolution is never a silent guess: what cannot be decided says why.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CallResolution {
    /// The candidate declarations, in a fixed order. More than one when the receiver type has
    /// several classes. `targets` is empty only for `new` on a class without a constructor.
    /// `residual` lists reasons for the parts of the receiver that did not resolve.
    Resolved {
        /// The declarations the call may reach.
        targets: Vec<Target>,
        /// Reasons some part of the receiver type did not resolve, sorted and distinct.
        residual: Vec<UnresolvedReason>,
    },
    /// The callee is computed at run time.
    Dynamic(DynamicKind),
    /// The callee could not be found.
    Unresolved(UnresolvedReason),
}

impl CallResolution {
    /// A resolution to exactly the given targets.
    pub fn resolved(targets: Vec<Target>) -> Self {
        Self::Resolved {
            targets,
            residual: Vec::new(),
        }
    }

    /// Whether any part of the callee is unresolved.
    pub fn is_unresolved(&self) -> bool {
        match self {
            Self::Resolved { residual, .. } => !residual.is_empty(),
            Self::Unresolved(_) => true,
            Self::Dynamic(_) => false,
        }
    }

    /// Whether the callee is computed at run time.
    pub fn is_dynamic(&self) -> bool {
        matches!(self, Self::Dynamic(_))
    }

    /// Whether the call resolved to more than one candidate (a polymorphic call).
    pub fn is_polymorphic(&self) -> bool {
        match self {
            Self::Resolved { targets, .. } => {
                targets.len() > 1 || targets.iter().any(|t| !t.implementors.is_empty())
            }
            _ => false,
        }
    }

    /// Combines two resolutions of the same expression (found on different loop passes or
    /// branches): the result covers both. Dynamic dominates; resolved targets are united; an
    /// unresolved part becomes a residual of a resolved one.
    #[must_use]
    pub fn merge(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Dynamic(a), Self::Dynamic(b)) => Self::Dynamic(*a.min(b)),
            (Self::Dynamic(a), _) | (_, Self::Dynamic(a)) => Self::Dynamic(*a),
            (
                Self::Resolved {
                    targets: ta,
                    residual: ra,
                },
                Self::Resolved {
                    targets: tb,
                    residual: rb,
                },
            ) => {
                let mut targets = ta.clone();
                targets.extend(tb.iter().cloned());
                targets.sort();
                targets.dedup();
                let mut residual = ra.clone();
                residual.extend(rb.iter().copied());
                residual.sort();
                residual.dedup();
                Self::Resolved { targets, residual }
            }
            (Self::Resolved { targets, residual }, Self::Unresolved(r))
            | (Self::Unresolved(r), Self::Resolved { targets, residual }) => {
                let mut residual = residual.clone();
                residual.push(*r);
                residual.sort();
                residual.dedup();
                Self::Resolved {
                    targets: targets.clone(),
                    residual,
                }
            }
            (Self::Unresolved(a), Self::Unresolved(b)) => Self::Unresolved(*a.min(b)),
        }
    }
}

/// Term: exempt
///
/// What one function's analysis was unsure of, for coverage and finding confidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct InferStats {
    /// Expressions with a recorded type.
    pub expressions: u32,
    /// Expressions whose type contains `unknown`.
    pub unknown_expressions: u32,
    /// Expressions whose type was widened by the union cap.
    pub widened_types: u32,
    /// Calls that resolved with no unresolved part.
    pub resolved_calls: u32,
    /// Calls that did not resolve, or resolved only in part.
    pub unresolved_calls: u32,
    /// Calls whose callee is computed at run time.
    pub dynamic_calls: u32,
    /// Loops that did not converge within the iteration limit and were widened.
    pub loop_limit_hits: u32,
    /// Expression nesting exceeded the depth limit; the deeper parts are `unknown`.
    pub depth_limit_hit: bool,
    /// The work budget ran out; the rest of the body is `unknown`.
    pub budget_exhausted: bool,
}

impl InferStats {
    /// Whether any limit was hit: a loop limit, depth, budget, or a union-cap widening.
    pub fn limit_hit(&self) -> bool {
        self.loop_limit_hits > 0
            || self.depth_limit_hit
            || self.budget_exhausted
            || self.widened_types > 0
    }
}

/// Term: Call resolution
///
/// Everything inference concluded about one function, method, closure or top-level script body.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct FunctionFacts {
    /// A display name: `App\Foo::bar`, `App\helper`, `{main}` or `{closure}@<offset>`.
    pub name: Box<str>,
    /// The span of the whole body's owner (the declaration, or the file for `{main}`).
    pub span: Span,
    /// The type of every expression in the body, sorted by key.
    pub expr_types: Vec<(ExprKey, Type)>,
    /// The resolution of every call-like expression, sorted by key.
    pub calls: Vec<(ExprKey, CallResolution)>,
    /// Uncertainty counts.
    pub stats: InferStats,
}

impl FunctionFacts {
    /// The recorded type of the expression with this key.
    pub fn type_of(&self, key: &ExprKey) -> Option<&Type> {
        self.expr_types
            .binary_search_by(|(k, _)| k.cmp(key))
            .ok()
            .map(|i| &self.expr_types[i].1)
    }

    /// The recorded resolution of the call-like expression with this key.
    pub fn call_at(&self, key: &ExprKey) -> Option<&CallResolution> {
        self.calls
            .binary_search_by(|(k, _)| k.cmp(key))
            .ok()
            .map(|i| &self.calls[i].1)
    }

    /// Builds the facts from the maps an analysis filled, deriving the counts from them so a
    /// re-evaluated expression is counted once.
    pub(super) fn assemble(
        name: String,
        span: Span,
        types: BTreeMap<ExprKey, Type>,
        calls: BTreeMap<ExprKey, CallResolution>,
        mut stats: InferStats,
    ) -> Self {
        stats.expressions = count(types.len());
        stats.unknown_expressions = count(types.values().filter(|t| t.has_unknown()).count());
        stats.widened_types = count(types.values().filter(|t| t.is_widened()).count());
        for resolution in calls.values() {
            if resolution.is_dynamic() {
                stats.dynamic_calls += 1;
            } else if resolution.is_unresolved() {
                stats.unresolved_calls += 1;
            } else {
                stats.resolved_calls += 1;
            }
        }
        Self {
            name: name.into(),
            span,
            expr_types: types.into_iter().collect(),
            calls: calls.into_iter().collect(),
            stats,
        }
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.owner {
            Some(owner) => write!(f, "{} {}::{}", self.kind.as_str(), owner, self.name)?,
            None => write!(f, "{} {}", self.kind.as_str(), self.name)?,
        }
        if !self.implementors.is_empty() {
            let names: Vec<&str> = self.implementors.iter().map(Fqn::as_str).collect();
            write!(f, " implementors({})", names.join(", "))?;
        }
        Ok(())
    }
}

impl fmt::Display for CallResolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Resolved { targets, residual } => {
                let listed: Vec<String> = targets.iter().map(ToString::to_string).collect();
                write!(f, "resolved [{}]", listed.join("; "))?;
                if !residual.is_empty() {
                    let reasons: Vec<&str> = residual.iter().map(|r| r.as_str()).collect();
                    write!(f, " residual({})", reasons.join(", "))?;
                }
                Ok(())
            }
            Self::Dynamic(kind) => write!(f, "dynamic({})", kind.as_str()),
            Self::Unresolved(reason) => write!(f, "unresolved({})", reason.as_str()),
        }
    }
}
