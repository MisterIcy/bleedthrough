//! Type declarations.

use crate::node::{Name, Node};

/// Term: exempt
///
/// A built-in or keyword type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
#[allow(missing_docs, reason = "variants are the PHP type keywords")]
pub enum BuiltinType {
    Null,
    True,
    False,
    Array,
    Callable,
    Static,
    SelfType,
    Parent,
    Void,
    Never,
    Float,
    Bool,
    Int,
    String,
    Object,
    Mixed,
    Iterable,
}

/// Term: exempt
///
/// The shape of a type declaration. A DNF type is a `Union` holding `Intersection`s.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
#[allow(missing_docs, reason = "variants are the PHP type forms")]
pub enum TypeKind {
    Named(Name),
    Builtin(BuiltinType),
    Nullable(Box<TypeHint>),
    Union(Vec<TypeHint>),
    Intersection(Vec<TypeHint>),
    Parenthesized(Box<TypeHint>),
    Error { diagnostic: Option<usize> },
    Unsupported { kind: Box<str> },
}

/// A type declaration with its span.
pub type TypeHint = Node<TypeKind>;
