//! Expressions.
#![allow(
    missing_docs,
    reason = "variants and fields are named after the PHP grammar"
)]

use crate::decl::{Attribute, ClassLike, Param};
use crate::node::{Literal, Name, Node, StringKind, StringPart};
use crate::span::Span;
use crate::stmt::Stmt;
use crate::types::TypeHint;

/// Term: exempt
///
/// A binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Coalesce,
    Eq,
    NotEq,
    AngledNotEq,
    Identical,
    NotIdentical,
    Lt,
    Le,
    Gt,
    Ge,
    Spaceship,
    Concat,
    Instanceof,
    And,
    Or,
    LowAnd,
    LowOr,
    LowXor,
}

/// Term: exempt
///
/// An assignment operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AssignOp {
    Assign,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Concat,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Coalesce,
}

/// Term: exempt
///
/// A unary operator other than a cast.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum UnaryOp {
    ErrorSuppress,
    Reference,
    BitNot,
    Not,
    Plus,
    Neg,
    PreInc,
    PreDec,
    PostInc,
    PostDec,
}

/// Term: exempt
///
/// The target type of a cast.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CastKind {
    Array,
    Bool,
    Float,
    Int,
    Object,
    String,
    Unset,
    Binary,
    Void,
}

/// Term: exempt
///
/// Which include-like construct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum IncludeKind {
    Include,
    IncludeOnce,
    Require,
    RequireOnce,
}

/// Term: exempt
///
/// A magic constant such as `__LINE__`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MagicConstantKind {
    Line,
    File,
    Dir,
    Trait,
    Method,
    Function,
    Property,
    Namespace,
    Class,
}

/// Term: exempt
///
/// `self`, `parent` or `static` used as an expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ClassRefKind {
    SelfRef,
    Parent,
    Static,
}

/// Term: exempt
///
/// The member named after `->`, `?->` or `::`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MemberName {
    Identifier { name: Box<str>, span: Span },
    Dynamic(Box<Expr>),
    Missing(Span),
}

/// Term: exempt
///
/// What a call or callable reference targets.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CallTarget {
    Function(Box<Expr>),
    Method {
        object: Box<Expr>,
        member: MemberName,
        nullsafe: bool,
    },
    Static {
        class: Box<Expr>,
        member: MemberName,
    },
}

/// Term: exempt
///
/// The form of an array-like expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ArrayStyle {
    Short,
    Long,
    List,
}

/// Term: exempt
///
/// A call argument.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArgKind {
    Positional(Expr),
    Named { name: Box<str>, value: Expr },
    Spread(Expr),
}

/// A call argument with its span.
pub type Arg = Node<ArgKind>;

/// Term: exempt
///
/// An element of an array or list expression.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArrayItemKind {
    Value(Expr),
    KeyValue { key: Expr, value: Expr },
    Spread(Expr),
    Missing,
}

/// An array element with its span.
pub type ArrayItem = Node<ArrayItemKind>;

/// Term: exempt
///
/// An arm of a `match`; an empty condition list is the `default` arm.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    clippy::exhaustive_structs,
    reason = "a plain record of conditions and body"
)]
pub struct MatchArm {
    pub span: Span,
    pub conditions: Vec<Expr>,
    pub is_default: bool,
    pub body: Expr,
}

/// Term: exempt
///
/// A variable captured by a closure's `use` clause.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct ClosureUse {
    pub span: Span,
    pub name: Box<str>,
    pub by_ref: bool,
}

/// Term: exempt
///
/// The shape of an expression.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExprKind {
    Literal(Literal),
    Interpolated {
        kind: StringKind,
        parts: Vec<StringPart>,
    },
    Variable(Box<str>),
    VariableVariable(Box<Expr>),
    Name(Name),
    ConstFetch(Name),
    ClassRef(ClassRefKind),
    MagicConstant(MagicConstantKind),
    Array {
        style: ArrayStyle,
        items: Vec<ArrayItem>,
    },
    Index {
        array: Box<Expr>,
        index: Option<Box<Expr>>,
    },
    Property {
        object: Box<Expr>,
        member: MemberName,
        nullsafe: bool,
    },
    StaticProperty {
        class: Box<Expr>,
        name: Box<Expr>,
    },
    ClassConst {
        class: Box<Expr>,
        member: MemberName,
    },
    Call {
        target: CallTarget,
        args: Vec<Arg>,
    },
    CallableRef {
        target: CallTarget,
    },
    New {
        class: Box<Expr>,
        args: Vec<Arg>,
    },
    AnonymousClass {
        args: Vec<Arg>,
        class: Box<ClassLike>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Cast {
        kind: CastKind,
        operand: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Assign {
        op: AssignOp,
        target: Box<Expr>,
        value: Box<Expr>,
    },
    Conditional {
        condition: Box<Expr>,
        then: Option<Box<Expr>>,
        otherwise: Box<Expr>,
    },
    Match {
        subject: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    Closure {
        attributes: Vec<Attribute>,
        is_static: bool,
        by_ref: bool,
        params: Vec<Param>,
        uses: Vec<ClosureUse>,
        return_type: Option<Box<TypeHint>>,
        body: Vec<Stmt>,
    },
    ArrowFn {
        attributes: Vec<Attribute>,
        is_static: bool,
        by_ref: bool,
        params: Vec<Param>,
        return_type: Option<Box<TypeHint>>,
        body: Box<Expr>,
    },
    Yield {
        key: Option<Box<Expr>>,
        value: Option<Box<Expr>>,
    },
    YieldFrom(Box<Expr>),
    Throw(Box<Expr>),
    Clone(Box<Expr>),
    Print(Box<Expr>),
    Include {
        kind: IncludeKind,
        path: Box<Expr>,
    },
    Eval(Box<Expr>),
    Exit {
        die: bool,
        args: Vec<Arg>,
    },
    Isset(Vec<Expr>),
    Empty(Box<Expr>),
    Pipe {
        input: Box<Expr>,
        callable: Box<Expr>,
    },
    Parenthesized(Box<Expr>),
    Error {
        diagnostic: Option<usize>,
    },
    Unsupported {
        kind: Box<str>,
    },
}

/// An expression with its span.
pub type Expr = Node<ExprKind>;
