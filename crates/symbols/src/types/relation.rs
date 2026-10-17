//! The structural subtype relation. Class hierarchies are not consulted: two different class names
//! are `Undetermined`, which callers treat conservatively.

use super::{Atom, NamedObject, ShapeEntry, ShapeKey, Type};

/// Term: Type
///
/// How one type relates to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Relation {
    /// Every value of the first is a value of the second.
    Subtype,
    /// No value is in both.
    Disjoint,
    /// Neither could be shown (overlap, class hierarchy needed, or unknown involved).
    Undetermined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cat {
    Null,
    Void,
    Mixed,
    Unknown,
    Template,
    Bool,
    Int,
    Float,
    Str,
    Array,
    Object,
    Callable,
    Iterable,
    Resource,
}

fn category(atom: &Atom) -> Cat {
    match atom {
        Atom::Null => Cat::Null,
        Atom::Void => Cat::Void,
        Atom::Mixed => Cat::Mixed,
        Atom::Unknown => Cat::Unknown,
        Atom::Template(_) => Cat::Template,
        Atom::True | Atom::False | Atom::Bool => Cat::Bool,
        Atom::Int | Atom::IntLiteral(_) | Atom::IntRange { .. } => Cat::Int,
        Atom::Float => Cat::Float,
        Atom::String
        | Atom::NonEmptyString
        | Atom::NumericString
        | Atom::StringLiteral(_)
        | Atom::ClassString(_) => Cat::Str,
        Atom::Array { .. } | Atom::List { .. } | Atom::Shape { .. } => Cat::Array,
        Atom::Object
        | Atom::Named(_)
        | Atom::EnumCase { .. }
        | Atom::Static(_)
        | Atom::Intersection(_) => Cat::Object,
        Atom::Callable(_) => Cat::Callable,
        Atom::Iterable { .. } => Cat::Iterable,
        Atom::Resource => Cat::Resource,
    }
}

pub(super) fn type_relation(a: &Type, b: &Type) -> Relation {
    if a.atoms().is_empty() {
        return Relation::Subtype;
    }
    let covered = a.atoms().iter().all(|x| {
        b.atoms()
            .iter()
            .any(|y| atom_relation(x, y) == Relation::Subtype)
    });
    if covered {
        return Relation::Subtype;
    }
    let apart = a.atoms().iter().all(|x| {
        b.atoms()
            .iter()
            .all(|y| atom_relation(x, y) == Relation::Disjoint)
    });
    if apart {
        Relation::Disjoint
    } else {
        Relation::Undetermined
    }
}

pub(super) fn atom_relation(a: &Atom, b: &Atom) -> Relation {
    if a == b {
        return Relation::Subtype;
    }
    let (ca, cb) = (category(a), category(b));
    match (ca, cb) {
        (_, Cat::Mixed) => Relation::Subtype,
        (Cat::Mixed | Cat::Unknown | Cat::Template, _) | (_, Cat::Unknown | Cat::Template) => {
            Relation::Undetermined
        }
        _ if ca == cb => same_category(a, b, ca),
        _ => cross_category(a, b, ca, cb),
    }
}

fn cross_category(a: &Atom, b: &Atom, ca: Cat, cb: Cat) -> Relation {
    use Cat::{Array, Callable, Float, Int, Iterable, Object, Str};
    match (ca, cb) {
        (Int, Float) | (Float, Int) => Relation::Undetermined,
        (Str | Array | Object, Callable) | (Callable, Str | Array | Object) => {
            Relation::Undetermined
        }
        (Array, Iterable) => array_in_iterable(a, b),
        (Object, Iterable) | (Iterable, Array | Object) => Relation::Undetermined,
        _ => Relation::Disjoint,
    }
}

fn same_category(a: &Atom, b: &Atom, category: Cat) -> Relation {
    match category {
        Cat::Bool => match (a, b) {
            (Atom::True | Atom::False, Atom::Bool) => Relation::Subtype,
            (Atom::Bool, _) => Relation::Undetermined,
            _ => Relation::Disjoint,
        },
        Cat::Int => int_relation(a, b),
        Cat::Str => string_relation(a, b),
        Cat::Array => array_relation(a, b),
        Cat::Object => object_relation(a, b),
        Cat::Callable => callable_relation(a, b),
        Cat::Iterable => iterable_relation(a, b),
        _ => Relation::Disjoint,
    }
}

fn int_bounds(atom: &Atom) -> (Option<i64>, Option<i64>) {
    match atom {
        Atom::IntLiteral(n) => (Some(*n), Some(*n)),
        Atom::IntRange { min, max } => (*min, *max),
        _ => (None, None),
    }
}

fn int_relation(a: &Atom, b: &Atom) -> Relation {
    let ((lo_a, hi_a), (lo_b, hi_b)) = (int_bounds(a), int_bounds(b));
    let low_ok = lo_b.is_none_or(|l| lo_a.is_some_and(|x| x >= l));
    let high_ok = hi_b.is_none_or(|h| hi_a.is_some_and(|x| x <= h));
    if low_ok && high_ok {
        return Relation::Subtype;
    }
    let below = matches!((hi_a, lo_b), (Some(x), Some(l)) if x < l);
    let above = matches!((lo_a, hi_b), (Some(x), Some(h)) if x > h);
    if below || above {
        Relation::Disjoint
    } else {
        Relation::Undetermined
    }
}

fn is_numeric_literal(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty() && text.parse::<f64>().is_ok_and(f64::is_finite)
}

fn string_relation(a: &Atom, b: &Atom) -> Relation {
    use Atom::{ClassString, NonEmptyString, NumericString, String as Str, StringLiteral};
    match (a, b) {
        (_, Str) => Relation::Subtype,
        (StringLiteral(s), NonEmptyString) => literal_is(!s.is_empty()),
        (NumericString | ClassString(_), NonEmptyString) => Relation::Subtype,
        (StringLiteral(s), NumericString) => literal_is(is_numeric_literal(s)),
        (ClassString(_), NumericString) | (NumericString, ClassString(_)) => Relation::Disjoint,
        (ClassString(_), ClassString(None)) => Relation::Subtype,
        (StringLiteral(_), StringLiteral(_)) => Relation::Disjoint,
        _ => Relation::Undetermined,
    }
}

const fn literal_is(holds: bool) -> Relation {
    if holds {
        Relation::Subtype
    } else {
        Relation::Disjoint
    }
}

/// An array seen as keys, values and emptiness, whatever its form.
struct ArrayView {
    key: Type,
    value: Type,
    non_empty: bool,
}

fn array_view(atom: &Atom) -> ArrayView {
    match atom {
        Atom::Array {
            key,
            value,
            non_empty,
        } => ArrayView {
            key: (**key).clone(),
            value: (**value).clone(),
            non_empty: *non_empty,
        },
        Atom::List { value, non_empty } => ArrayView {
            key: Type::of(Atom::IntRange {
                min: Some(0),
                max: None,
            }),
            value: (**value).clone(),
            non_empty: *non_empty,
        },
        Atom::Shape { entries, .. } => ArrayView {
            key: Type::from_atoms(entries.iter().map(|e| match &e.key {
                ShapeKey::Int(n) => Atom::IntLiteral(*n),
                ShapeKey::Str(s) => Atom::StringLiteral(s.clone()),
            })),
            value: Type::union_all(entries.iter().map(|e| &e.value)),
            non_empty: entries.iter().any(|e| !e.optional),
        },
        _ => ArrayView {
            key: Type::never(),
            value: Type::never(),
            non_empty: false,
        },
    }
}

fn array_relation(a: &Atom, b: &Atom) -> Relation {
    match (a, b) {
        (
            _,
            Atom::Array {
                key,
                value,
                non_empty,
            },
        ) => {
            let view = array_view(a);
            let parts = [
                type_relation(&view.key, key),
                type_relation(&view.value, value),
            ];
            parts_relation(&parts, view.non_empty, !*non_empty || view.non_empty)
        }
        (
            Atom::List {
                value: av,
                non_empty: ae,
            },
            Atom::List { value, non_empty },
        ) => {
            let parts = [type_relation(av, value)];
            parts_relation(&parts, *ae, !*non_empty || *ae)
        }
        (Atom::Shape { entries, sealed }, Atom::List { value, non_empty }) => {
            let view = array_view(a);
            let sequential = *sealed && is_sequential(entries);
            let parts = [type_relation(&view.value, value)];
            if sequential {
                parts_relation(&parts, view.non_empty, !*non_empty || view.non_empty)
            } else {
                Relation::Undetermined
            }
        }
        (Atom::Shape { .. }, Atom::Shape { .. }) => shape_relation(a, b),
        _ => Relation::Undetermined,
    }
}

fn is_sequential(entries: &[ShapeEntry]) -> bool {
    entries
        .iter()
        .enumerate()
        .all(|(i, e)| !e.optional && e.key == ShapeKey::Int(i64::try_from(i).unwrap_or(-1)))
}

/// Combines the component relations of an array comparison: it is a subtype when all components
/// are and emptiness is compatible; disjoint only when a non-empty array has a disjoint component.
fn parts_relation(parts: &[Relation], non_empty: bool, emptiness_ok: bool) -> Relation {
    if parts.iter().all(|r| *r == Relation::Subtype) && emptiness_ok {
        Relation::Subtype
    } else if non_empty && parts.contains(&Relation::Disjoint) {
        Relation::Disjoint
    } else {
        Relation::Undetermined
    }
}

fn shape_relation(a: &Atom, b: &Atom) -> Relation {
    let (
        Atom::Shape {
            entries: ea,
            sealed: sa,
        },
        Atom::Shape {
            entries: eb,
            sealed: sb,
        },
    ) = (a, b)
    else {
        return Relation::Undetermined;
    };
    let required_ok = eb.iter().all(|want| {
        ea.iter()
            .find(|have| have.key == want.key)
            .is_some_and(|have| {
                (want.optional || !have.optional)
                    && type_relation(&have.value, &want.value) == Relation::Subtype
            })
            || (want.optional && *sa && !ea.iter().any(|h| h.key == want.key))
    });
    // A sealed target admits only the keys it lists, so the source must be sealed and add none.
    let extras_ok = !*sb || (*sa && ea.iter().all(|h| eb.iter().any(|w| w.key == h.key)));
    if required_ok && extras_ok {
        Relation::Subtype
    } else {
        Relation::Undetermined
    }
}

fn array_in_iterable(a: &Atom, b: &Atom) -> Relation {
    let Atom::Iterable { key, value } = b else {
        return Relation::Undetermined;
    };
    let view = array_view(a);
    let parts = [
        type_relation(&view.key, key),
        type_relation(&view.value, value),
    ];
    parts_relation(&parts, view.non_empty, true)
}

fn iterable_relation(a: &Atom, b: &Atom) -> Relation {
    match (a, b) {
        (Atom::Iterable { key: ka, value: va }, Atom::Iterable { key: kb, value: vb }) => {
            parts_relation(&[type_relation(ka, kb), type_relation(va, vb)], false, true)
        }
        _ => Relation::Undetermined,
    }
}

fn callable_relation(a: &Atom, b: &Atom) -> Relation {
    match (a, b) {
        (Atom::Callable(_), Atom::Callable(None)) => Relation::Subtype,
        _ => Relation::Undetermined,
    }
}

fn object_relation(a: &Atom, b: &Atom) -> Relation {
    if let Atom::Intersection(parts) = b {
        return all_parts(a, parts);
    }
    if let Atom::Intersection(parts) = a {
        let any = parts
            .iter()
            .any(|p| atom_relation(p, b) == Relation::Subtype);
        return if any {
            Relation::Subtype
        } else {
            Relation::Undetermined
        };
    }
    match (a, b) {
        (_, Atom::Object) => Relation::Subtype,
        (Atom::Named(x), Atom::Named(y)) => named_relation(x, y),
        (Atom::Static(x), Atom::Named(y)) if *x == y.name => Relation::Subtype,
        (Atom::EnumCase { enum_name, .. }, Atom::Named(y)) if *enum_name == y.name => {
            Relation::Subtype
        }
        (
            Atom::EnumCase {
                enum_name: ea,
                case: ca,
            },
            Atom::EnumCase {
                enum_name: eb,
                case: cb,
            },
        ) if ea == eb && ca != cb => Relation::Disjoint,
        _ => Relation::Undetermined,
    }
}

fn all_parts(a: &Atom, parts: &[Atom]) -> Relation {
    let relations: Vec<Relation> = parts.iter().map(|p| atom_relation(a, p)).collect();
    if relations.iter().all(|r| *r == Relation::Subtype) {
        Relation::Subtype
    } else if relations.contains(&Relation::Disjoint) {
        Relation::Disjoint
    } else {
        Relation::Undetermined
    }
}

fn named_relation(x: &NamedObject, y: &NamedObject) -> Relation {
    if x.name != y.name {
        return Relation::Undetermined;
    }
    if y.args.is_empty() {
        return Relation::Subtype;
    }
    if x.args.len() != y.args.len() {
        return Relation::Undetermined;
    }
    let same = x
        .args
        .iter()
        .zip(&y.args)
        .all(|(p, q)| type_relation(p, q) == Relation::Subtype);
    if same {
        Relation::Subtype
    } else {
        Relation::Undetermined
    }
}
