//! Canonicalisation of unions: dedup, absorption, ordering and the size cap.

use super::relation::{Relation, atom_relation};
use super::{Atom, DEFAULT_LITERAL_CAP, Type};

/// Above this many atoms the quadratic absorption pass is skipped.
const ABSORB_LIMIT: usize = 512;

pub(super) fn normalise(atoms: Vec<Atom>, cap: usize, widened: bool) -> Type {
    let mut widened = widened;
    let mut atoms: Vec<Atom> = atoms.into_iter().map(canonical_atom).collect();
    if atoms.len() > ABSORB_LIMIT {
        atoms = degrade_literals(atoms);
        widened = true;
    }
    settle(&mut atoms);
    if atoms.len() > cap {
        widened = true;
        atoms = degrade_literals(atoms);
        settle(&mut atoms);
    }
    if atoms.len() > cap {
        let unknown = atoms.contains(&Atom::Unknown);
        atoms = vec![Atom::Mixed];
        if unknown {
            atoms.push(Atom::Unknown);
        }
    }
    Type { atoms, widened }
}

fn settle(atoms: &mut Vec<Atom>) {
    atoms.sort();
    atoms.dedup();
    if atoms.contains(&Atom::Mixed) {
        atoms.retain(|a| matches!(a, Atom::Mixed | Atom::Unknown));
        return;
    }
    if atoms.contains(&Atom::True) && atoms.contains(&Atom::False) {
        atoms.push(Atom::Bool);
        atoms.sort();
    }
    if atoms.len() <= ABSORB_LIMIT {
        absorb(atoms);
    }
}

fn absorb(atoms: &mut Vec<Atom>) {
    let keep: Vec<bool> = atoms
        .iter()
        .map(|a| *a == Atom::Unknown || !atoms.iter().any(|b| absorbed(a, b)))
        .collect();
    let mut flags = keep.into_iter();
    atoms.retain(|_| flags.next().unwrap_or(true));
}

/// Whether `a` adds nothing to a union containing `b`. When the two are equivalent the greater one
/// is dropped, so the result does not depend on the input order.
fn absorbed(a: &Atom, b: &Atom) -> bool {
    a != b
        && atom_relation(a, b) == Relation::Subtype
        && (atom_relation(b, a) != Relation::Subtype || a > b)
}

fn degrade_literals(atoms: Vec<Atom>) -> Vec<Atom> {
    atoms
        .into_iter()
        .map(|atom| match atom {
            Atom::IntLiteral(_) => Atom::Int,
            Atom::StringLiteral(_) => Atom::String,
            other => other,
        })
        .collect()
}

fn canonical_atom(atom: Atom) -> Atom {
    match atom {
        Atom::StringLiteral(text) if text.len() > DEFAULT_LITERAL_CAP => {
            if text.is_empty() {
                Atom::String
            } else {
                Atom::NonEmptyString
            }
        }
        Atom::IntRange { min, max } => canonical_range(min, max),
        Atom::Shape {
            mut entries,
            sealed,
        } => {
            entries.sort_by(|a, b| a.key.cmp(&b.key));
            entries.dedup_by(|later, first| later.key == first.key);
            Atom::Shape { entries, sealed }
        }
        Atom::Intersection(parts) => canonical_intersection(parts),
        other => other,
    }
}

fn canonical_range(min: Option<i64>, max: Option<i64>) -> Atom {
    match (min, max) {
        (None, None) => Atom::Int,
        (Some(a), Some(b)) if a == b => Atom::IntLiteral(a),
        (Some(a), Some(b)) if a > b => Atom::Int,
        _ => Atom::IntRange { min, max },
    }
}

fn canonical_intersection(parts: Vec<Atom>) -> Atom {
    let mut flat = Vec::new();
    for part in parts {
        match part {
            Atom::Intersection(inner) => flat.extend(inner),
            other => flat.push(other),
        }
    }
    flat.sort();
    flat.dedup();
    match flat.len() {
        0 => Atom::Object,
        1 => flat.remove(0),
        _ => Atom::Intersection(flat),
    }
}
