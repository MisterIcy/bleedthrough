//! Statements.
#![allow(
    missing_docs,
    reason = "variants and fields are named after the PHP grammar"
)]

use crate::decl::{ClassLike, ConstItem, FunctionLike};
use crate::expr::Expr;
use crate::node::{Name, Node};
use crate::span::Span;
use crate::types::TypeHint;

/// Term: exempt
///
/// The kind of a `use` import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum UseKind {
    Class,
    Function,
    Const,
}

/// Term: exempt
///
/// One imported name. For a group `use A\{B, C}` the name is joined (`A\B`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct UseItem {
    pub span: Span,
    pub kind: UseKind,
    pub name: Name,
    pub alias: Option<Box<str>>,
}

/// Term: exempt
///
/// An `elseif` clause.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct ElseIf {
    pub span: Span,
    pub condition: Expr,
    pub body: Vec<Stmt>,
}

/// Term: exempt
///
/// A `case` or `default` of a `switch`; `test` is `None` for `default`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct SwitchCase {
    pub span: Span,
    pub test: Option<Expr>,
    pub body: Vec<Stmt>,
}

/// Term: exempt
///
/// A `catch` clause.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct Catch {
    pub span: Span,
    pub types: TypeHint,
    pub variable: Option<Box<str>>,
    pub body: Vec<Stmt>,
}

/// Term: exempt
///
/// A `static $x = ...;` variable.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct StaticVar {
    pub span: Span,
    pub name: Box<str>,
    pub init: Option<Expr>,
}

/// Term: exempt
///
/// One `name = value` of a `declare`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct Directive {
    pub span: Span,
    pub name: Box<str>,
    pub value: Expr,
}

/// Term: exempt
///
/// The shape of a statement. Bodies are lists: a braced block is unwrapped into its statements and
/// an unbraced body is a one-element list.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum StmtKind {
    OpenTag {
        short: bool,
    },
    CloseTag,
    InlineHtml {
        shebang: bool,
        raw: Box<str>,
    },
    Namespace {
        name: Option<Name>,
        braced: bool,
        body: Vec<Stmt>,
    },
    Use {
        items: Vec<UseItem>,
    },
    ClassLike(Box<ClassLike>),
    Function(Box<FunctionLike>),
    Const {
        items: Vec<ConstItem>,
    },
    Block(Vec<Stmt>),
    Expr(Expr),
    Echo {
        values: Vec<Expr>,
        short_tag: bool,
    },
    Return(Option<Expr>),
    If {
        condition: Expr,
        then: Vec<Stmt>,
        else_ifs: Vec<ElseIf>,
        otherwise: Option<Vec<Stmt>>,
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },
    DoWhile {
        body: Vec<Stmt>,
        condition: Expr,
    },
    For {
        init: Vec<Expr>,
        condition: Vec<Expr>,
        step: Vec<Expr>,
        body: Vec<Stmt>,
    },
    Foreach {
        subject: Expr,
        key: Option<Expr>,
        value: Expr,
        body: Vec<Stmt>,
    },
    Switch {
        subject: Expr,
        cases: Vec<SwitchCase>,
    },
    Break(Option<Expr>),
    Continue(Option<Expr>),
    Try {
        body: Vec<Stmt>,
        catches: Vec<Catch>,
        finally: Option<Vec<Stmt>>,
    },
    Goto(Box<str>),
    Label(Box<str>),
    Declare {
        directives: Vec<Directive>,
        body: Option<Vec<Stmt>>,
    },
    Global(Vec<Expr>),
    StaticVars(Vec<StaticVar>),
    Unset(Vec<Expr>),
    HaltCompiler,
    Noop,
    Error {
        diagnostic: Option<usize>,
    },
    Unsupported {
        kind: Box<str>,
    },
}

/// A statement with its span.
pub type Stmt = Node<StmtKind>;
