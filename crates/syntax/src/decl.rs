//! Declarations: classes and their members, functions, parameters, attributes.
#![allow(
    missing_docs,
    reason = "variants and fields are named after the PHP grammar"
)]

use crate::expr::{Arg, Expr};
use crate::node::{Name, Node};
use crate::span::Span;
use crate::stmt::Stmt;
use crate::types::TypeHint;

/// Term: exempt
///
/// A declaration modifier. `PublicSet` and friends are asymmetric-visibility modifiers such as
/// `private(set)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Modifier {
    Static,
    Final,
    Abstract,
    Readonly,
    Public,
    PublicSet,
    Protected,
    ProtectedSet,
    Private,
    PrivateSet,
}

/// Term: exempt
///
/// An attribute such as `#[Route('/x')]`; a group `#[A, B]` yields one per entry.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct Attribute {
    pub span: Span,
    pub name: Name,
    pub args: Vec<Arg>,
}

/// Term: exempt
///
/// A function, method or closure parameter, including promoted constructor properties.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct Param {
    pub span: Span,
    pub attributes: Vec<Attribute>,
    pub modifiers: Vec<Modifier>,
    pub type_hint: Option<TypeHint>,
    pub by_ref: bool,
    pub variadic: bool,
    pub name: Box<str>,
    pub default: Option<Expr>,
    pub hooks: Vec<PropertyHook>,
}

/// Term: exempt
///
/// The body of a property hook.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum HookBody {
    Abstract,
    Block(Vec<Stmt>),
    Expr(Box<Expr>),
}

/// Term: exempt
///
/// A property hook such as `get` or `set`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct PropertyHook {
    pub span: Span,
    pub attributes: Vec<Attribute>,
    pub modifiers: Vec<Modifier>,
    pub by_ref: bool,
    pub name: Box<str>,
    pub params: Vec<Param>,
    pub body: HookBody,
}

/// Term: exempt
///
/// A function or method declaration. `body` is `None` for abstract and interface methods.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct FunctionLike {
    pub name: Box<str>,
    pub attributes: Vec<Attribute>,
    pub modifiers: Vec<Modifier>,
    pub by_ref: bool,
    pub params: Vec<Param>,
    pub return_type: Option<TypeHint>,
    pub body: Option<Vec<Stmt>>,
}

/// Term: exempt
///
/// Which kind of class-like declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ClassLikeKind {
    Class,
    Interface,
    Trait,
    Enum,
}

/// Term: exempt
///
/// A class, interface, trait, enum or anonymous class. `extends` holds the parent of a class or
/// the parents of an interface.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct ClassLike {
    pub kind: ClassLikeKind,
    pub name: Option<Box<str>>,
    pub attributes: Vec<Attribute>,
    pub modifiers: Vec<Modifier>,
    pub extends: Vec<Name>,
    pub implements: Vec<Name>,
    pub backing_type: Option<TypeHint>,
    pub members: Vec<Member>,
}

/// Term: exempt
///
/// A `const` entry of a class-like or of a `const` statement.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct ConstItem {
    pub span: Span,
    pub name: Box<str>,
    pub value: Expr,
}

/// Term: exempt
///
/// One declared property.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct PropertyItem {
    pub span: Span,
    pub name: Box<str>,
    pub default: Option<Expr>,
}

/// Term: exempt
///
/// A trait adaptation inside `use T { ... }`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TraitAdaptationKind {
    Precedence {
        trait_name: Name,
        method: Box<str>,
        instead_of: Vec<Name>,
    },
    Alias {
        trait_name: Option<Name>,
        method: Box<str>,
        visibility: Option<Modifier>,
        alias: Option<Box<str>>,
    },
}

/// A trait adaptation with its span.
pub type TraitAdaptation = Node<TraitAdaptationKind>;

/// Term: exempt
///
/// A member of a class-like.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MemberKind {
    TraitUse {
        traits: Vec<Name>,
        adaptations: Vec<TraitAdaptation>,
    },
    Const {
        attributes: Vec<Attribute>,
        modifiers: Vec<Modifier>,
        type_hint: Option<TypeHint>,
        items: Vec<ConstItem>,
    },
    Property {
        attributes: Vec<Attribute>,
        modifiers: Vec<Modifier>,
        type_hint: Option<TypeHint>,
        items: Vec<PropertyItem>,
        hooks: Vec<PropertyHook>,
    },
    Method(Box<FunctionLike>),
    EnumCase {
        attributes: Vec<Attribute>,
        name: Box<str>,
        value: Option<Expr>,
    },
    Error {
        diagnostic: Option<usize>,
    },
    Unsupported {
        kind: Box<str>,
    },
}

/// A class-like member with its span.
pub type Member = Node<MemberKind>;
