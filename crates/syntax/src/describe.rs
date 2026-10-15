//! Kind names and scalar labels of nodes, used by dumps and diagnostics of the tree itself.

use crate::decl::{HookBody, MemberKind, TraitAdaptationKind};
use crate::expr::{ArgKind, ArrayItemKind, CallTarget, ExprKind, MemberName};
use crate::node::{Literal, StringPartKind};
use crate::stmt::StmtKind;
use crate::types::TypeKind;
use crate::walk::NodeRef;

macro_rules! names {
    ($value:expr, $ty:ident, [$($variant:ident),* $(,)?]) => {
        match $value { $($ty::$variant { .. } => stringify!($variant)),* }
    };
}

fn expr_name(kind: &ExprKind) -> &'static str {
    use ExprKind as E;
    names!(
        kind,
        E,
        [
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
            Unsupported
        ]
    )
}

fn stmt_name(kind: &StmtKind) -> &'static str {
    use StmtKind as S;
    names!(
        kind,
        S,
        [
            OpenTag,
            CloseTag,
            InlineHtml,
            Namespace,
            Use,
            ClassLike,
            Function,
            Const,
            Block,
            Expr,
            Echo,
            Return,
            If,
            While,
            DoWhile,
            For,
            Foreach,
            Switch,
            Break,
            Continue,
            Try,
            Goto,
            Label,
            Declare,
            Global,
            StaticVars,
            Unset,
            HaltCompiler,
            Noop,
            Error,
            Unsupported
        ]
    )
}

fn type_name(kind: &TypeKind) -> &'static str {
    use TypeKind as T;
    names!(
        kind,
        T,
        [
            Named,
            Builtin,
            Nullable,
            Union,
            Intersection,
            Parenthesized,
            Error,
            Unsupported
        ]
    )
}

fn member_name(kind: &MemberKind) -> &'static str {
    use MemberKind as M;
    names!(
        kind,
        M,
        [
            TraitUse,
            Const,
            Property,
            Method,
            EnumCase,
            Error,
            Unsupported
        ]
    )
}

fn member_label(name: &MemberName) -> String {
    match name {
        MemberName::Identifier { name, .. } => format!("{name:?}"),
        MemberName::Dynamic(_) => "dynamic".into(),
        MemberName::Missing(_) => "missing".into(),
    }
}

fn target_label(target: &CallTarget) -> String {
    match target {
        CallTarget::Function(_) => "function".into(),
        CallTarget::Method {
            member, nullsafe, ..
        } => {
            format!("method {} nullsafe={nullsafe}", member_label(member))
        }
        CallTarget::Static { member, .. } => format!("static {}", member_label(member)),
    }
}

fn literal_label(literal: &Literal) -> String {
    match literal {
        Literal::Int { raw } => format!("int {raw:?}"),
        Literal::Float { raw } => format!("float {raw:?}"),
        Literal::String { raw, quote } => format!("string {quote:?} {raw:?}"),
        Literal::Bool(b) => format!("bool {b}"),
        Literal::Null => "null".into(),
    }
}

fn expr_label(kind: &ExprKind) -> Option<String> {
    use ExprKind as E;
    Some(match kind {
        E::Literal(l) => literal_label(l),
        E::Interpolated { kind, .. } => format!("{kind:?}"),
        E::Variable(name) => format!("{name:?}"),
        E::Name(n) | E::ConstFetch(n) => format!("{:?} {:?}", n.kind, n.text),
        E::ClassRef(k) => format!("{k:?}"),
        E::MagicConstant(k) => format!("{k:?}"),
        E::Array { style, .. } => format!("{style:?}"),
        E::Property {
            member, nullsafe, ..
        } => {
            format!("{} nullsafe={nullsafe}", member_label(member))
        }
        E::ClassConst { member, .. } => member_label(member),
        E::Call { target, .. } | E::CallableRef { target } => target_label(target),
        E::Unary { op, .. } => format!("{op:?}"),
        E::Cast { kind, .. } => format!("{kind:?}"),
        E::Binary { op, .. } => format!("{op:?}"),
        E::Assign { op, .. } => format!("{op:?}"),
        E::Closure {
            is_static, by_ref, ..
        }
        | E::ArrowFn {
            is_static, by_ref, ..
        } => {
            format!("static={is_static} by_ref={by_ref}")
        }
        E::Include { kind, .. } => format!("{kind:?}"),
        E::Exit { die, .. } => format!("die={die}"),
        E::Unsupported { kind } => format!("{kind:?}"),
        _ => return None,
    })
}

fn stmt_label(kind: &StmtKind) -> Option<String> {
    use StmtKind as S;
    Some(match kind {
        S::OpenTag { short } => format!("short={short}"),
        S::InlineHtml { shebang, raw } => format!("shebang={shebang} {raw:?}"),
        S::Namespace { name, braced, .. } => format!(
            "{} braced={braced}",
            name.as_ref()
                .map_or("none".into(), |n| format!("{:?}", n.text))
        ),
        S::ClassLike(c) => format!(
            "{:?} {:?} modifiers={:?}",
            c.kind,
            c.name.as_deref().unwrap_or(""),
            c.modifiers
        ),
        S::Function(f) => format!(
            "{:?} by_ref={} modifiers={:?}",
            f.name, f.by_ref, f.modifiers
        ),
        S::Echo { short_tag, .. } => format!("short_tag={short_tag}"),
        S::Goto(l) | S::Label(l) => format!("{l:?}"),
        S::Unsupported { kind } => format!("{kind:?}"),
        _ => return None,
    })
}

fn member_node_label(kind: &MemberKind) -> Option<String> {
    Some(match kind {
        MemberKind::Const { modifiers, .. } | MemberKind::Property { modifiers, .. } => {
            format!("modifiers={modifiers:?}")
        }
        MemberKind::Method(f) => format!(
            "{:?} by_ref={} modifiers={:?}",
            f.name, f.by_ref, f.modifiers
        ),
        MemberKind::EnumCase { name, .. } => format!("{name:?}"),
        MemberKind::Unsupported { kind } => format!("{kind:?}"),
        _ => return None,
    })
}

fn small_label(node: NodeRef<'_>) -> Option<String> {
    Some(match node {
        NodeRef::Type(t) => match &t.kind {
            TypeKind::Builtin(b) => format!("{b:?}"),
            TypeKind::Unsupported { kind } => format!("{kind:?}"),
            _ => return None,
        },
        NodeRef::Arg(a) => match &a.kind {
            ArgKind::Named { name, .. } => format!("named {name:?}"),
            ArgKind::Spread(_) => "spread".into(),
            ArgKind::Positional(_) => return None,
        },
        NodeRef::ArrayItem(i) => match i.kind {
            ArrayItemKind::Spread(_) => "spread".into(),
            ArrayItemKind::Missing => "missing".into(),
            _ => return None,
        },
        NodeRef::StringPart(p) => match &p.kind {
            StringPartKind::Text { raw } => format!("text {raw:?}"),
            StringPartKind::Interpolation(_) => return None,
        },
        NodeRef::MatchArm(m) => format!("default={}", m.is_default),
        NodeRef::Param(p) => format!(
            "{:?} by_ref={} variadic={} modifiers={:?}",
            p.name, p.by_ref, p.variadic, p.modifiers
        ),
        NodeRef::Name(n) => format!("{:?} {:?}", n.kind, n.text),
        NodeRef::UseItem(u) => format!("{:?} alias={:?}", u.kind, u.alias),
        NodeRef::Catch(c) => format!("variable={:?}", c.variable),
        NodeRef::StaticVar(v) => format!("{:?}", v.name),
        NodeRef::Directive(d) => format!("{:?}", d.name),
        NodeRef::ConstItem(c) => format!("{:?}", c.name),
        NodeRef::PropertyItem(p) => format!("{:?}", p.name),
        NodeRef::Hook(h) => format!(
            "{:?} by_ref={} modifiers={:?} {}",
            h.name,
            h.by_ref,
            h.modifiers,
            match h.body {
                HookBody::Abstract => "abstract",
                HookBody::Block(_) => "block",
                HookBody::Expr(_) => "expr",
            }
        ),
        NodeRef::ClosureUse(u) => format!("{:?} by_ref={}", u.name, u.by_ref),
        NodeRef::TraitAdaptation(t) => match &t.kind {
            TraitAdaptationKind::Precedence { method, .. } => format!("insteadof {method:?}"),
            TraitAdaptationKind::Alias {
                method,
                visibility,
                alias,
                ..
            } => {
                format!("alias {method:?} {visibility:?} {alias:?}")
            }
        },
        NodeRef::Attribute(_) | NodeRef::ElseIf(_) | NodeRef::SwitchCase(_) => return None,
        NodeRef::Stmt(_) | NodeRef::Expr(_) | NodeRef::Member(_) => return None,
    })
}

impl NodeRef<'_> {
    /// The node's type or variant name, for example `Binary` or `Param`.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Stmt(n) => stmt_name(&n.kind),
            Self::Expr(n) => expr_name(&n.kind),
            Self::Type(n) => type_name(&n.kind),
            Self::Member(n) => member_name(&n.kind),
            Self::Arg(_) => "Arg",
            Self::ArrayItem(_) => "ArrayItem",
            Self::StringPart(_) => "StringPart",
            Self::MatchArm(_) => "MatchArm",
            Self::Param(_) => "Param",
            Self::Attribute(_) => "Attribute",
            Self::Name(_) => "Name",
            Self::UseItem(_) => "UseItem",
            Self::ElseIf(_) => "ElseIf",
            Self::SwitchCase(_) => "SwitchCase",
            Self::Catch(_) => "Catch",
            Self::StaticVar(_) => "StaticVar",
            Self::Directive(_) => "Directive",
            Self::ConstItem(_) => "ConstItem",
            Self::PropertyItem(_) => "PropertyItem",
            Self::Hook(_) => "Hook",
            Self::ClosureUse(_) => "ClosureUse",
            Self::TraitAdaptation(_) => "TraitAdaptation",
        }
    }

    /// The node's scalar attributes (names, operators, flags, raw text) as one line, if any.
    /// Text is escaped with Rust's `Debug` quoting, so the result is a single deterministic line.
    pub fn label(&self) -> Option<String> {
        match self {
            Self::Stmt(n) => stmt_label(&n.kind),
            Self::Expr(n) => expr_label(&n.kind),
            Self::Member(n) => member_node_label(&n.kind),
            other => small_label(*other),
        }
    }
}
