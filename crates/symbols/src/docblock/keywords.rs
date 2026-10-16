//! Type keywords of the supported PHPDoc and Psalm subset that stand for a fixed type.

use crate::types::{Atom, Type};

fn range(min: Option<i64>, max: Option<i64>) -> Type {
    Type::of(Atom::IntRange { min, max })
}

/// The type a keyword (already lower-cased) stands for, if it is one that takes no arguments.
pub(super) fn simple_keyword(lower: &str) -> Option<Type> {
    let atom = |a: Atom| Some(Type::of(a));
    match lower {
        "null" => atom(Atom::Null),
        "void" => atom(Atom::Void),
        "mixed" => atom(Atom::Mixed),
        "unknown" => atom(Atom::Unknown),
        "never" | "no-return" | "never-return" | "noreturn" => Some(Type::never()),
        "true" => atom(Atom::True),
        "false" => atom(Atom::False),
        "bool" | "boolean" => atom(Atom::Bool),
        "int" | "integer" => atom(Atom::Int),
        "float" | "double" | "real" => atom(Atom::Float),
        "string" | "literal-string" | "lowercase-string" => atom(Atom::String),
        "non-empty-string"
        | "non-falsy-string"
        | "truthy-string"
        | "callable-string"
        | "non-empty-lowercase-string" => atom(Atom::NonEmptyString),
        "numeric-string" => atom(Atom::NumericString),
        "trait-string" | "interface-string" | "enum-string" => atom(Atom::ClassString(None)),
        "object" | "callable-object" => atom(Atom::Object),
        "resource" | "open-resource" | "closed-resource" => atom(Atom::Resource),
        "array-key" => Some(Type::array_key()),
        "scalar" => Some(Type::from_atoms([
            Atom::Int,
            Atom::Float,
            Atom::String,
            Atom::Bool,
        ])),
        "numeric" => Some(Type::from_atoms([
            Atom::Int,
            Atom::Float,
            Atom::NumericString,
        ])),
        "positive-int" => Some(range(Some(1), None)),
        "non-negative-int" => Some(range(Some(0), None)),
        "negative-int" => Some(range(None, Some(-1))),
        "non-positive-int" => Some(range(None, Some(0))),
        "callable-array" => Some(Type::array_of(Type::array_key(), Type::mixed())),
        _ => None,
    }
}

/// Constructs of the wider Psalm and PHPStan language that are recognised and ignored with a note.
pub(super) fn is_unsupported_generic(lower: &str) -> bool {
    matches!(
        lower,
        "key-of"
            | "value-of"
            | "int-mask"
            | "int-mask-of"
            | "properties-of"
            | "class-string-map"
            | "template-type"
            | "new"
            | "min"
            | "max"
    )
}
