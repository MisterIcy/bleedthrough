//! The atoms a type is a union of.

use crate::name::Fqn;
use crate::types::Type;

/// Term: Type
///
/// The key of a shape entry: a literal integer or string key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ShapeKey {
    /// `0`, `1`, ...
    Int(i64),
    /// `'id'`
    Str(Box<str>),
}

/// Term: Type
///
/// One entry of an array shape such as `array{id: int, name?: string}`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct ShapeEntry {
    /// The literal key.
    pub key: ShapeKey,
    /// The type of the value.
    pub value: Type,
    /// Whether the key may be absent.
    pub optional: bool,
}

/// Term: Type
///
/// A parameter in a callable's signature.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct CallableParam {
    /// The parameter type.
    pub ty: Type,
    /// Whether it may be omitted (`=`).
    pub optional: bool,
    /// Whether it takes the rest of the arguments (`...`).
    pub variadic: bool,
}

/// Term: Type
///
/// The signature of `callable(int, string=): void`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct CallableSig {
    /// Parameters in order.
    pub params: Vec<CallableParam>,
    /// The return type.
    pub ret: Type,
}

/// Term: Type
///
/// An object type: a class-like name with optional type arguments.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct NamedObject {
    /// The class-like.
    pub name: Fqn,
    /// Type arguments, empty when none were given.
    pub args: Vec<Type>,
}

impl NamedObject {
    /// An object type without type arguments.
    pub fn plain(name: Fqn) -> Self {
        Self {
            name,
            args: Vec::new(),
        }
    }
}

/// Term: Type
///
/// One atomic type. There is no `never` atom (the empty union is `never`) and no `?T` (it is
/// `T|null`). The derived order is the canonical order of a union.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Atom {
    /// `null`
    Null,
    /// `void`
    Void,
    /// Arbitrary: the analysis determined that nothing is known to hold.
    Mixed,
    /// The analysis could not determine the type; never absorbed, so consumers can tell.
    Unknown,
    /// `true`
    True,
    /// `false`
    False,
    /// `bool`
    Bool,
    /// `int`
    Int,
    /// A literal integer.
    IntLiteral(i64),
    /// `int<min, max>`, `positive-int` and friends; a missing bound is unbounded.
    IntRange {
        /// Lowest value, if bounded.
        min: Option<i64>,
        /// Highest value, if bounded.
        max: Option<i64>,
    },
    /// `float`
    Float,
    /// `string`
    String,
    /// `non-empty-string`
    NonEmptyString,
    /// `numeric-string`
    NumericString,
    /// A literal string.
    StringLiteral(Box<str>),
    /// `class-string` with an optional bound.
    ClassString(Option<Fqn>),
    /// A keyed array: `array<K, V>`, `T[]`, `non-empty-array<K, V>`.
    Array {
        /// Key type.
        key: Box<Type>,
        /// Value type.
        value: Box<Type>,
        /// Whether at least one element exists.
        non_empty: bool,
    },
    /// `list<T>`: keys are `0..n`.
    List {
        /// Value type.
        value: Box<Type>,
        /// Whether at least one element exists.
        non_empty: bool,
    },
    /// `array{...}`, entries sorted by key.
    Shape {
        /// The entries.
        entries: Vec<ShapeEntry>,
        /// Whether no other keys exist (`...` makes a shape unsealed).
        sealed: bool,
    },
    /// `object`
    Object,
    /// A class-like name with optional type arguments.
    Named(NamedObject),
    /// A case of an enum.
    EnumCase {
        /// The enum.
        enum_name: Fqn,
        /// The case.
        case: Box<str>,
    },
    /// `static`, bound to the class it was written in.
    Static(Fqn),
    /// An intersection of object types that is kept as such.
    Intersection(Vec<Atom>),
    /// A template parameter (`@template T`) not yet substituted.
    Template(Box<str>),
    /// `callable`, with an optional signature.
    Callable(Option<Box<CallableSig>>),
    /// `iterable<K, V>`
    Iterable {
        /// Key type.
        key: Box<Type>,
        /// Value type.
        value: Box<Type>,
    },
    /// `resource`
    Resource,
}

impl Atom {
    /// An object atom for a class-like name.
    pub fn named(name: Fqn) -> Self {
        Self::Named(NamedObject::plain(name))
    }

    /// Whether the atom is some kind of string.
    pub fn is_string(&self) -> bool {
        matches!(
            self,
            Self::String
                | Self::NonEmptyString
                | Self::NumericString
                | Self::StringLiteral(_)
                | Self::ClassString(_)
        )
    }

    /// Whether the atom is some kind of integer.
    pub fn is_int(&self) -> bool {
        matches!(
            self,
            Self::Int | Self::IntLiteral(_) | Self::IntRange { .. }
        )
    }

    /// Whether the atom is some kind of boolean.
    pub fn is_bool(&self) -> bool {
        matches!(self, Self::True | Self::False | Self::Bool)
    }

    /// Whether the atom is some kind of array.
    pub fn is_array(&self) -> bool {
        matches!(
            self,
            Self::Array { .. } | Self::List { .. } | Self::Shape { .. }
        )
    }

    /// Whether the atom is some kind of object (a class-like, enum case, `static`, `object` or an
    /// intersection).
    pub fn is_object(&self) -> bool {
        matches!(
            self,
            Self::Object
                | Self::Named(_)
                | Self::EnumCase { .. }
                | Self::Static(_)
                | Self::Intersection(_)
        )
    }
}
