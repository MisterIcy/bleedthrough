//! Pure type operators: PHP's result types for arithmetic, concatenation, casts, array reads
//! and writes. None of them looks at the symbol table.

use bleedthrough_syntax::{BinaryOp, CastKind};

use crate::name::Fqn;
use crate::types::{Atom, ShapeEntry, ShapeKey, Type};

/// The most entries a known array shape keeps before it degrades to a general array.
const MAX_SHAPE_ENTRIES: usize = 32;

/// The single literal array key a type denotes, if it denotes exactly one. A numeric string key
/// is the integer it spells, as in PHP.
pub(super) fn literal_key(ty: &Type) -> Option<ShapeKey> {
    match ty.atoms() {
        [Atom::IntLiteral(i)] => Some(ShapeKey::Int(*i)),
        [Atom::StringLiteral(s)] => {
            Some(canonical_int(s).map_or_else(|| ShapeKey::Str(s.clone()), ShapeKey::Int))
        }
        _ => None,
    }
}

fn canonical_int(text: &str) -> Option<i64> {
    let parsed: i64 = text.parse().ok()?;
    (parsed.to_string() == text).then_some(parsed)
}

/// The type of an array key given the type of the key expression.
pub(super) fn key_type(ty: &Type) -> Type {
    ty.flat_map(|atom| match atom {
        a if a.is_int() => Type::int(),
        a if a.is_string() => Type::string(),
        Atom::Bool | Atom::True | Atom::False | Atom::Float => Type::int(),
        Atom::Null => Type::string(),
        _ => Type::array_key(),
    })
}

fn shape_key_type(key: &ShapeKey) -> Type {
    match key {
        ShapeKey::Int(_) => Type::int(),
        ShapeKey::Str(_) => Type::string(),
    }
}

/// The key and value types of iterating or spreading `ty`; `unknown` for what is not an array.
pub(super) fn elements(ty: &Type) -> (Type, Type) {
    let mut keys = Vec::new();
    let mut values = Vec::new();
    for atom in ty.atoms() {
        match atom {
            Atom::List { value, .. } => {
                keys.push(Type::int());
                values.push((**value).clone());
            }
            Atom::Array { key, value, .. } | Atom::Iterable { key, value } => {
                keys.push((**key).clone());
                values.push((**value).clone());
            }
            Atom::Shape { entries, sealed } => {
                for entry in entries {
                    keys.push(shape_key_type(&entry.key));
                    values.push(entry.value.clone());
                }
                if !sealed {
                    keys.push(Type::array_key());
                    values.push(Type::unknown());
                }
            }
            Atom::Mixed => {
                keys.push(Type::mixed());
                values.push(Type::mixed());
            }
            _ => {
                keys.push(Type::unknown());
                values.push(Type::unknown());
            }
        }
    }
    (Type::union_all(keys.iter()), Type::union_all(values.iter()))
}

/// One element of an array literal.
pub(super) enum ItemKey {
    Implicit,
    Literal(ShapeKey),
    Dynamic(Type),
}

pub(super) struct ArrayItem {
    pub key: ItemKey,
    pub value: Type,
}

/// The type of an array literal from its items, plus the types spread into it.
pub(super) fn build_array(items: Vec<ArrayItem>, spreads: &[Type]) -> Type {
    if items.is_empty() && spreads.is_empty() {
        return Type::of(Atom::Shape {
            entries: Vec::new(),
            sealed: true,
        });
    }
    let dynamic = items.iter().any(|i| matches!(i.key, ItemKey::Dynamic(_)));
    let implicit = items.iter().all(|i| matches!(i.key, ItemKey::Implicit));
    if spreads.is_empty() && implicit {
        let value = Type::union_all(items.iter().map(|i| &i.value));
        return Type::of(Atom::List {
            value: Box::new(value),
            non_empty: true,
        });
    }
    if spreads.is_empty() && !dynamic && items.len() <= MAX_SHAPE_ENTRIES {
        let mut next: i64 = 0;
        let mut entries: Vec<ShapeEntry> = Vec::new();
        for item in items {
            let key = match item.key {
                ItemKey::Literal(ShapeKey::Int(i)) => {
                    next = next.max(i.saturating_add(1));
                    ShapeKey::Int(i)
                }
                ItemKey::Literal(k) => k,
                ItemKey::Implicit | ItemKey::Dynamic(_) => {
                    let k = next;
                    next = next.saturating_add(1);
                    ShapeKey::Int(k)
                }
            };
            entries.retain(|e| e.key != key);
            entries.push(ShapeEntry {
                key,
                value: item.value,
                optional: false,
            });
        }
        return Type::of(Atom::Shape {
            entries,
            sealed: true,
        });
    }
    let mut keys: Vec<Type> = Vec::new();
    let mut values: Vec<Type> = Vec::new();
    let non_empty = !items.is_empty();
    for item in items {
        keys.push(match &item.key {
            ItemKey::Implicit => Type::int(),
            ItemKey::Literal(k) => shape_key_type(k),
            ItemKey::Dynamic(t) => key_type(t),
        });
        values.push(item.value);
    }
    for spread in spreads {
        let (k, v) = elements(spread);
        // Integer keys of a spread are renumbered; string keys are kept.
        keys.push(k.flat_map(|a| {
            if a.is_int() {
                Type::int()
            } else {
                Type::of(a.clone())
            }
        }));
        values.push(v);
    }
    Type::of(Atom::Array {
        key: Box::new(Type::union_all(keys.iter())),
        value: Box::new(Type::union_all(values.iter())),
        non_empty,
    })
}

/// The type of `base[key]` when read. `key` is `None` for `$a[]`.
pub(super) fn index_read(base: &Type, key: Option<&Type>) -> Type {
    let literal = key.and_then(literal_key);
    base.flat_map(|atom| match atom {
        Atom::List { value, .. } | Atom::Array { value, .. } | Atom::Iterable { value, .. } => {
            (**value).clone()
        }
        Atom::Shape { entries, sealed } => match &literal {
            Some(k) => entries.iter().find(|e| &e.key == k).map_or_else(
                || {
                    if *sealed {
                        Type::null()
                    } else {
                        Type::unknown()
                    }
                },
                |e| e.value.clone(),
            ),
            None => {
                let mut all: Vec<Type> = entries.iter().map(|e| e.value.clone()).collect();
                if !sealed {
                    all.push(Type::unknown());
                }
                Type::union_all(all.iter())
            }
        },
        a if a.is_string() => Type::string(),
        Atom::Mixed => Type::mixed(),
        Atom::Null | Atom::Void | Atom::Float | Atom::Resource => Type::null(),
        a if a.is_int() || a.is_bool() => Type::null(),
        _ => Type::unknown(),
    })
}

/// The type of an array after `base[key] = value`. `key` is `None` for `$a[] = value`. Atoms that
/// are not arrays (strings, objects implementing `ArrayAccess`, `mixed`) are kept as they are.
pub(super) fn update_array(old: &Type, key: Option<&Type>, value: &Type) -> Type {
    let literal = key.and_then(literal_key);
    let key_ty = key.map(key_type);
    let empty = Atom::Shape {
        entries: Vec::new(),
        sealed: true,
    };
    let mut out: Vec<Type> = Vec::new();
    let atoms: Vec<&Atom> = if old.is_never() {
        vec![&empty]
    } else {
        old.atoms().iter().collect()
    };
    for atom in atoms {
        let atom = if matches!(atom, Atom::Null | Atom::Void) {
            &empty
        } else {
            atom
        };
        out.push(match atom {
            Atom::List { value: v, .. } if key.is_none() => Type::of(Atom::List {
                value: Box::new(v.union(value)),
                non_empty: true,
            }),
            Atom::List { value: v, .. } => Type::of(Atom::Array {
                key: Box::new(Type::int().union(&key_ty.clone().unwrap_or_else(Type::int))),
                value: Box::new(v.union(value)),
                non_empty: true,
            }),
            Atom::Array {
                key: k, value: v, ..
            } => Type::of(Atom::Array {
                key: Box::new(k.union(&key_ty.clone().unwrap_or_else(Type::int))),
                value: Box::new(v.union(value)),
                non_empty: true,
            }),
            Atom::Shape { entries, sealed } => update_shape(
                entries,
                *sealed,
                &Write {
                    literal: literal.as_ref(),
                    append: key.is_none(),
                    key_ty: key_ty.as_ref(),
                },
                value,
            ),
            other => Type::of(other.clone()),
        });
    }
    Type::union_all(out.iter())
}

/// The key of an element write.
struct Write<'a> {
    literal: Option<&'a ShapeKey>,
    append: bool,
    key_ty: Option<&'a Type>,
}

fn update_shape(entries: &[ShapeEntry], sealed: bool, write: &Write<'_>, value: &Type) -> Type {
    let Write {
        literal,
        append,
        key_ty,
    } = *write;
    let general = || {
        let mut keys: Vec<Type> = entries.iter().map(|e| shape_key_type(&e.key)).collect();
        let mut values: Vec<Type> = entries.iter().map(|e| e.value.clone()).collect();
        keys.push(key_ty.cloned().unwrap_or_else(Type::int));
        values.push(value.clone());
        Type::of(Atom::Array {
            key: Box::new(Type::union_all(keys.iter())),
            value: Box::new(Type::union_all(values.iter())),
            non_empty: true,
        })
    };
    let target = if append {
        let next = entries
            .iter()
            .filter_map(|e| match e.key {
                ShapeKey::Int(i) => Some(i.saturating_add(1)),
                ShapeKey::Str(_) => None,
            })
            .max()
            .unwrap_or(0);
        ShapeKey::Int(next)
    } else if let Some(k) = literal {
        k.clone()
    } else {
        return general();
    };
    let mut updated: Vec<ShapeEntry> = entries
        .iter()
        .filter(|e| e.key != target)
        .cloned()
        .collect();
    updated.push(ShapeEntry {
        key: target,
        value: value.clone(),
        optional: false,
    });
    if updated.len() > MAX_SHAPE_ENTRIES {
        return general();
    }
    Type::of(Atom::Shape {
        entries: updated,
        sealed,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Num {
    Int,
    Float,
    Str,
    Arr,
    Unk,
}

fn num(atom: &Atom) -> Num {
    match atom {
        a if a.is_int() || a.is_bool() => Num::Int,
        Atom::Null | Atom::Void => Num::Int,
        Atom::Float => Num::Float,
        a if a.is_string() => Num::Str,
        Atom::Array { .. } | Atom::List { .. } | Atom::Shape { .. } => Num::Arr,
        _ => Num::Unk,
    }
}

fn int_or_float() -> Type {
    Type::from_atoms([Atom::Int, Atom::Float])
}

/// The result type of an arithmetic, bitwise or shift operator.
pub(super) fn arith(op: BinaryOp, l: &Type, r: &Type) -> Type {
    if l.is_never() || r.is_never() {
        return Type::never();
    }
    if let Some(folded) = fold_int(op, l, r) {
        return folded;
    }
    let mut out: Vec<Type> = Vec::new();
    for la in l.atoms() {
        for ra in r.atoms() {
            out.push(arith_pair(op, la, ra));
        }
    }
    Type::union_all(out.iter())
}

fn fold_int(op: BinaryOp, l: &Type, r: &Type) -> Option<Type> {
    let ([Atom::IntLiteral(a)], [Atom::IntLiteral(b)]) = (l.atoms(), r.atoms()) else {
        return None;
    };
    let value = match op {
        BinaryOp::Add => a.checked_add(*b),
        BinaryOp::Sub => a.checked_sub(*b),
        BinaryOp::Mul => a.checked_mul(*b),
        _ => None,
    }?;
    Some(Type::of(Atom::IntLiteral(value)))
}

fn arith_pair(op: BinaryOp, la: &Atom, ra: &Atom) -> Type {
    let (a, b) = (num(la), num(ra));
    if a == Num::Unk || b == Num::Unk {
        return Type::unknown();
    }
    match op {
        BinaryOp::Mod | BinaryOp::Shl | BinaryOp::Shr => return Type::int(),
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor => {
            return if la.is_string() && ra.is_string() {
                Type::string()
            } else {
                Type::int()
            };
        }
        _ => {}
    }
    if a == Num::Arr || b == Num::Arr {
        return if op == BinaryOp::Add && a == b {
            merge_arrays(la, ra)
        } else {
            Type::unknown()
        };
    }
    let float = a == Num::Float || b == Num::Float;
    let string = a == Num::Str || b == Num::Str;
    match op {
        BinaryOp::Div | BinaryOp::Pow => {
            if float {
                Type::float()
            } else {
                int_or_float()
            }
        }
        _ if float => Type::float(),
        _ if string => int_or_float(),
        _ => Type::int(),
    }
}

fn merge_arrays(a: &Atom, b: &Atom) -> Type {
    let (ka, va) = elements(&Type::of(a.clone()));
    let (kb, vb) = elements(&Type::of(b.clone()));
    let non_empty = [a, b].iter().any(|x| {
        matches!(
            x,
            Atom::Array {
                non_empty: true,
                ..
            } | Atom::List {
                non_empty: true,
                ..
            }
        )
    });
    if let (Atom::List { .. }, Atom::List { .. }) = (a, b) {
        return Type::of(Atom::List {
            value: Box::new(va.union(&vb)),
            non_empty,
        });
    }
    Type::of(Atom::Array {
        key: Box::new(ka.union(&kb)),
        value: Box::new(va.union(&vb)),
        non_empty,
    })
}

fn literal_text(atom: &Atom) -> Option<String> {
    match atom {
        Atom::StringLiteral(s) => Some(s.to_string()),
        Atom::IntLiteral(i) => Some(i.to_string()),
        Atom::True => Some("1".to_string()),
        Atom::False | Atom::Null => Some(String::new()),
        _ => None,
    }
}

fn surely_non_empty(ty: &Type) -> bool {
    !ty.atoms().is_empty()
        && ty.atoms().iter().all(|a| match a {
            Atom::NonEmptyString | Atom::NumericString | Atom::Float => true,
            Atom::StringLiteral(s) => !s.is_empty(),
            Atom::IntLiteral(_) | Atom::Int | Atom::IntRange { .. } | Atom::True => true,
            _ => false,
        })
}

/// The result type of `.`.
pub(super) fn concat(l: &Type, r: &Type) -> Type {
    if l.is_never() || r.is_never() {
        return Type::never();
    }
    if let ([a], [b]) = (l.atoms(), r.atoms())
        && let (Some(x), Some(y)) = (literal_text(a), literal_text(b))
    {
        return Type::of(Atom::StringLiteral(format!("{x}{y}").into()));
    }
    if surely_non_empty(l) || surely_non_empty(r) {
        Type::of(Atom::NonEmptyString)
    } else {
        Type::string()
    }
}

/// The type of an operand after `++` (`inc`) or `--`.
pub(super) fn incdec(ty: &Type, inc: bool) -> Type {
    ty.flat_map(|atom| match atom {
        a if a.is_int() => Type::int(),
        Atom::Float => Type::float(),
        Atom::Null => {
            if inc {
                Type::of(Atom::IntLiteral(1))
            } else {
                Type::null()
            }
        }
        a if a.is_bool() => Type::of(a.clone()),
        a if a.is_string() => Type::from_atoms([Atom::String, Atom::Int, Atom::Float]),
        Atom::Mixed => Type::mixed(),
        _ => Type::unknown(),
    })
}

/// The type of `(kind) operand`.
pub(super) fn cast(kind: CastKind, ty: &Type) -> Type {
    match kind {
        CastKind::Int => Type::int(),
        CastKind::Float => Type::float(),
        CastKind::Bool => Type::bool(),
        CastKind::String | CastKind::Binary => {
            if !ty.atoms().is_empty() && ty.atoms().iter().all(Atom::is_string) {
                ty.clone()
            } else {
                Type::string()
            }
        }
        CastKind::Array => ty.flat_map(|atom| match atom {
            Atom::Array { .. } | Atom::List { .. } | Atom::Shape { .. } => Type::of(atom.clone()),
            Atom::Null | Atom::Void => Type::of(Atom::Shape {
                entries: Vec::new(),
                sealed: true,
            }),
            Atom::Mixed | Atom::Unknown => Type::array_of(Type::array_key(), Type::mixed()),
            Atom::Named(_) | Atom::Static(_) | Atom::Object | Atom::EnumCase { .. } => {
                Type::array_of(Type::string(), Type::mixed())
            }
            other => Type::of(Atom::List {
                value: Box::new(Type::of(other.clone())),
                non_empty: true,
            }),
        }),
        CastKind::Object => ty.flat_map(|atom| match atom {
            Atom::Named(_) | Atom::Static(_) | Atom::Object | Atom::EnumCase { .. } => {
                Type::of(atom.clone())
            }
            _ => Type::object(Fqn::new("stdClass")),
        }),
        _ => Type::null(),
    }
}

/// The part of a type that may be falsy (what remains when `empty($x)` is true).
pub(super) fn falsy_part(ty: &Type) -> Type {
    ty.flat_map(|atom| match atom {
        Atom::True
        | Atom::NonEmptyString
        | Atom::Named(_)
        | Atom::Static(_)
        | Atom::EnumCase { .. }
        | Atom::Object
        | Atom::Intersection(_)
        | Atom::Callable(_)
        | Atom::Resource
        | Atom::Array {
            non_empty: true, ..
        }
        | Atom::List {
            non_empty: true, ..
        } => Type::never(),
        Atom::StringLiteral(s) if !s.is_empty() && &**s != "0" => Type::never(),
        Atom::IntLiteral(i) if *i != 0 => Type::never(),
        Atom::IntRange { min, max } if min.is_some_and(|m| m > 0) || max.is_some_and(|m| m < 0) => {
            Type::never()
        }
        Atom::Shape { entries, .. } if entries.iter().any(|e| !e.optional) => Type::never(),
        Atom::Bool => Type::of(Atom::False),
        other => Type::of(other.clone()),
    })
}

/// The part of a type that may be truthy (what remains when `empty($x)` is false).
pub(super) fn truthy_part(ty: &Type) -> Type {
    ty.flat_map(|atom| match atom {
        Atom::Null | Atom::Void | Atom::False => Type::never(),
        Atom::IntLiteral(0) => Type::never(),
        Atom::StringLiteral(s) if s.is_empty() || &**s == "0" => Type::never(),
        Atom::Shape {
            entries,
            sealed: true,
        } if entries.is_empty() => Type::never(),
        Atom::Bool => Type::of(Atom::True),
        Atom::String => Type::of(Atom::NonEmptyString),
        other => Type::of(other.clone()),
    })
}
