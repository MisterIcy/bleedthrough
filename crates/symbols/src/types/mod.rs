//! The type model: canonical unions of atoms, their relations and their text form.

mod atom;
mod normalise;
mod relation;
mod render;

pub use atom::{Atom, CallableParam, CallableSig, NamedObject, ShapeEntry, ShapeKey};
pub use relation::Relation;

use crate::name::Fqn;

/// The default limit on the number of atoms in a union before it widens.
pub const DEFAULT_UNION_CAP: usize = 64;

/// The default longest literal string kept as a literal, in bytes.
pub const DEFAULT_LITERAL_CAP: usize = 256;

/// Term: Type
///
/// A canonical union of atoms: duplicates removed, subsumed atoms absorbed, order fixed. The empty
/// union is `never`; `mixed` absorbs every atom except `unknown`, which is never absorbed so a
/// result that depends on missing knowledge stays identifiable. A union that exceeded its size cap
/// is widened to `mixed` (literals first degrade to their general kind) and carries the widened flag.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Type {
    atoms: Vec<Atom>,
    widened: bool,
}

impl Type {
    /// A union of `atoms`, normalised with the default caps.
    pub fn from_atoms(atoms: impl IntoIterator<Item = Atom>) -> Self {
        Self::from_atoms_capped(atoms, DEFAULT_UNION_CAP)
    }

    /// A union of `atoms`, normalised with a union cap of `cap` atoms.
    pub fn from_atoms_capped(atoms: impl IntoIterator<Item = Atom>, cap: usize) -> Self {
        normalise::normalise(atoms.into_iter().collect(), cap, false)
    }

    /// A type of one atom.
    pub fn of(atom: Atom) -> Self {
        Self::from_atoms([atom])
    }

    /// `never`: the empty union.
    pub fn never() -> Self {
        Self::default()
    }

    /// `mixed`.
    pub fn mixed() -> Self {
        Self::of(Atom::Mixed)
    }

    /// `unknown`.
    pub fn unknown() -> Self {
        Self::of(Atom::Unknown)
    }

    /// `null`.
    pub fn null() -> Self {
        Self::of(Atom::Null)
    }

    /// `void`.
    pub fn void() -> Self {
        Self::of(Atom::Void)
    }

    /// `bool`.
    pub fn bool() -> Self {
        Self::of(Atom::Bool)
    }

    /// `int`.
    pub fn int() -> Self {
        Self::of(Atom::Int)
    }

    /// `float`.
    pub fn float() -> Self {
        Self::of(Atom::Float)
    }

    /// `string`.
    pub fn string() -> Self {
        Self::of(Atom::String)
    }

    /// `array-key`: `int|string`.
    pub fn array_key() -> Self {
        Self::from_atoms([Atom::Int, Atom::String])
    }

    /// An object type of one class-like.
    pub fn object(name: Fqn) -> Self {
        Self::of(Atom::named(name))
    }

    /// `array<key, value>`.
    pub fn array_of(key: Type, value: Type) -> Self {
        Self::of(Atom::Array {
            key: Box::new(key),
            value: Box::new(value),
            non_empty: false,
        })
    }

    /// `list<value>`.
    pub fn list_of(value: Type) -> Self {
        Self::of(Atom::List {
            value: Box::new(value),
            non_empty: false,
        })
    }

    /// The atoms in canonical order.
    pub fn atoms(&self) -> &[Atom] {
        &self.atoms
    }

    /// Whether this is `never`.
    pub fn is_never(&self) -> bool {
        self.atoms.is_empty()
    }

    /// Whether the union contains `mixed`.
    pub fn is_mixed(&self) -> bool {
        self.atoms.contains(&Atom::Mixed)
    }

    /// Whether the union contains `unknown`: the result depends on something the analysis could not
    /// determine, so consumers lower their confidence.
    pub fn has_unknown(&self) -> bool {
        self.atoms.contains(&Atom::Unknown)
    }

    /// Whether the union was widened because it exceeded its size cap.
    pub fn is_widened(&self) -> bool {
        self.widened
    }

    /// Whether the union contains `null` (or `mixed`).
    pub fn contains_null(&self) -> bool {
        self.atoms
            .iter()
            .any(|a| matches!(a, Atom::Null | Atom::Mixed))
    }

    /// The union of two types with the default cap; the widened flag is inherited.
    pub fn union(&self, other: &Type) -> Type {
        self.union_capped(other, DEFAULT_UNION_CAP)
    }

    /// The union of two types with a cap of `cap` atoms.
    pub fn union_capped(&self, other: &Type, cap: usize) -> Type {
        let atoms = self.atoms.iter().chain(&other.atoms).cloned().collect();
        normalise::normalise(atoms, cap, self.widened || other.widened)
    }

    /// The union of many types with the default cap.
    pub fn union_all<'a>(types: impl IntoIterator<Item = &'a Type>) -> Type {
        types.into_iter().fold(Type::never(), |acc, t| acc.union(t))
    }

    /// The atoms satisfying `keep`, as a type (the widened flag is kept).
    pub fn filter(&self, keep: impl Fn(&Atom) -> bool) -> Type {
        let atoms = self.atoms.iter().filter(|a| keep(a)).cloned().collect();
        normalise::normalise(atoms, DEFAULT_UNION_CAP, self.widened)
    }

    /// The type with its atoms replaced by `map` (an atom may map to a whole type).
    pub fn flat_map(&self, map: impl Fn(&Atom) -> Type) -> Type {
        let mut atoms = Vec::new();
        for atom in &self.atoms {
            atoms.extend(map(atom).atoms);
        }
        normalise::normalise(atoms, DEFAULT_UNION_CAP, self.widened)
    }

    /// The type without `null`.
    pub fn without_null(&self) -> Type {
        self.filter(|a| *a != Atom::Null)
    }

    /// Whether every value of this type is a value of `other`, whether they are disjoint, or
    /// neither can be shown. Class hierarchies are not consulted here, so two different class names
    /// are undetermined.
    pub fn relation_to(&self, other: &Type) -> Relation {
        relation::type_relation(self, other)
    }

    /// Appends every class-like name this type mentions to `out`.
    pub fn collect_class_names(&self, out: &mut Vec<Fqn>) {
        for atom in &self.atoms {
            collect_atom_names(atom, out);
        }
    }

    /// Replaces template parameters by the types in `bindings` (by name).
    pub fn substitute(&self, bindings: &[(&str, Type)]) -> Type {
        self.flat_map(|atom| substitute_atom(atom, bindings))
    }
}

fn collect_atom_names(atom: &Atom, out: &mut Vec<Fqn>) {
    match atom {
        Atom::Named(object) => {
            out.push(object.name.clone());
            object.args.iter().for_each(|t| t.collect_class_names(out));
        }
        Atom::EnumCase {
            enum_name: name, ..
        }
        | Atom::Static(name) => out.push(name.clone()),
        Atom::ClassString(Some(name)) => out.push(name.clone()),
        Atom::Intersection(parts) => parts.iter().for_each(|a| collect_atom_names(a, out)),
        Atom::Array { key, value, .. } | Atom::Iterable { key, value } => {
            key.collect_class_names(out);
            value.collect_class_names(out);
        }
        Atom::List { value, .. } => value.collect_class_names(out),
        Atom::Shape { entries, .. } => entries
            .iter()
            .for_each(|e| e.value.collect_class_names(out)),
        Atom::Callable(Some(sig)) => {
            sig.params
                .iter()
                .for_each(|p| p.ty.collect_class_names(out));
            sig.ret.collect_class_names(out);
        }
        _ => {}
    }
}

fn substitute_atom(atom: &Atom, bindings: &[(&str, Type)]) -> Type {
    let sub = |t: &Type| t.substitute(bindings);
    match atom {
        Atom::Template(name) => bindings
            .iter()
            .find(|(n, _)| *n == &**name)
            .map_or_else(|| Type::of(atom.clone()), |(_, t)| t.clone()),
        Atom::Array {
            key,
            value,
            non_empty,
        } => Type::of(Atom::Array {
            key: Box::new(sub(key)),
            value: Box::new(sub(value)),
            non_empty: *non_empty,
        }),
        Atom::List { value, non_empty } => Type::of(Atom::List {
            value: Box::new(sub(value)),
            non_empty: *non_empty,
        }),
        Atom::Iterable { key, value } => Type::of(Atom::Iterable {
            key: Box::new(sub(key)),
            value: Box::new(sub(value)),
        }),
        Atom::Named(object) => Type::of(Atom::Named(NamedObject {
            name: object.name.clone(),
            args: object.args.iter().map(sub).collect(),
        })),
        other => Type::of(other.clone()),
    }
}

#[cfg(test)]
mod tests;
