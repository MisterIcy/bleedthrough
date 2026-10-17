//! The abstract state of a body: variable types at a program point.

use std::collections::BTreeMap;

use crate::types::{Atom, Relation, ShapeEntry, ShapeKey, Type};

/// The variable types in force at one point, and whether the point is reachable.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct Env {
    vars: BTreeMap<Box<str>, Type>,
    /// The point is not reachable (after `return`, `throw`, `break` ...). A dead state is the
    /// identity of [`Env::join`].
    pub dead: bool,
}

impl Env {
    pub fn new() -> Self {
        Self::default()
    }

    /// An unreachable state: the identity of [`Env::join`].
    pub fn unreachable() -> Self {
        Self {
            vars: BTreeMap::new(),
            dead: true,
        }
    }

    /// Whether this state adds nothing to `head`: every variable's type is within the type `head`
    /// has for it. A dead state is within every state.
    pub fn is_within(&self, head: &Self) -> bool {
        if self.dead {
            return true;
        }
        if head.dead {
            return false;
        }
        self.vars.iter().all(|(name, ty)| {
            head.vars
                .get(name)
                .is_some_and(|h| ty == h || ty.relation_to(h) == Relation::Subtype)
        })
    }

    /// The type of a variable, `None` when it has not been assigned on this path.
    pub fn lookup(&self, name: &str) -> Option<&Type> {
        self.vars.get(name)
    }

    /// The type of a variable; `unknown` when it has not been assigned.
    pub fn get(&self, name: &str) -> Type {
        self.vars.get(name).cloned().unwrap_or_else(Type::unknown)
    }

    pub fn set(&mut self, name: &str, ty: Type) {
        self.vars.insert(name.into(), ty);
    }

    pub fn remove(&mut self, name: &str) {
        self.vars.remove(name);
    }

    /// The state after either path. A variable missing on one path may be undefined there, so it
    /// is united with `unknown`.
    #[must_use]
    pub fn join(&self, other: &Self) -> Self {
        if self.dead {
            return other.clone();
        }
        if other.dead {
            return self.clone();
        }
        let mut vars = BTreeMap::new();
        for (name, ty) in &self.vars {
            let joined = match other.vars.get(name) {
                Some(o) => ty.union(o),
                None => ty.union(&Type::unknown()),
            };
            vars.insert(name.clone(), joined);
        }
        for (name, ty) in &other.vars {
            if !self.vars.contains_key(name) {
                vars.insert(name.clone(), ty.union(&Type::unknown()));
            }
        }
        Self { vars, dead: false }
    }

    /// The join of any number of states; a dead state when there are none.
    pub fn join_all<'e>(states: impl IntoIterator<Item = &'e Self>) -> Self {
        let mut joined = Self {
            vars: BTreeMap::new(),
            dead: true,
        };
        for state in states {
            joined = joined.join(state);
        }
        joined
    }

    /// Replaces the types of the variables that differ from `previous` by their generalisation
    /// (literals become their kind). Used at loop heads so literal-growing values converge.
    pub fn generalise_changes(&mut self, previous: &Self) {
        for (name, ty) in &mut self.vars {
            if previous.vars.get(name) != Some(ty) {
                *ty = generalise(ty);
            }
        }
    }

    /// Replaces the types of the variables that differ from `previous` by `mixed`.
    pub fn widen_changes(&mut self, previous: &Self) {
        for (name, ty) in &mut self.vars {
            if previous.vars.get(name) != Some(ty) {
                *ty = Type::mixed();
            }
        }
    }
}

/// A non-empty shape that keeps changing at a loop head, as the list or array it grows into, so
/// that appending in a loop converges instead of adding an entry every pass.
fn generalise_shape(entries: &[ShapeEntry]) -> Type {
    let value = Type::union_all(
        entries
            .iter()
            .map(|e| generalise(&e.value))
            .collect::<Vec<_>>()
            .iter(),
    );
    let non_empty = entries.iter().any(|e| !e.optional);
    let is_list = entries.iter().enumerate().all(
        |(i, e)| matches!(&e.key, ShapeKey::Int(k) if usize::try_from(*k).is_ok_and(|k| k == i)),
    );
    if is_list {
        return Type::of(Atom::List {
            value: Box::new(value),
            non_empty,
        });
    }
    let key = Type::union_all(
        entries
            .iter()
            .map(|e| match e.key {
                ShapeKey::Int(_) => Type::int(),
                _ => Type::string(),
            })
            .collect::<Vec<_>>()
            .iter(),
    );
    Type::of(Atom::Array {
        key: Box::new(key),
        value: Box::new(value),
        non_empty,
    })
}

/// A type with literals replaced by their general kind.
pub(super) fn generalise(ty: &Type) -> Type {
    ty.flat_map(|atom| match atom {
        Atom::StringLiteral(_) => Type::string(),
        Atom::IntLiteral(_) => Type::int(),
        Atom::True | Atom::False => Type::bool(),
        Atom::Shape { entries, .. } if !entries.is_empty() => generalise_shape(entries),
        other => Type::of(other.clone()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(name: &str, ty: Type) -> Env {
        let mut env = Env::new();
        env.set(name, ty);
        env
    }

    #[test]
    fn a_state_is_within_a_head_that_holds_every_type_of_it() {
        let wide = with("x", Type::int().union(&Type::string()));
        // equal, and strictly narrower
        assert!(with("x", Type::int().union(&Type::string())).is_within(&wide));
        assert!(with("x", Type::int()).is_within(&wide));
        // wider, unrelated, or a variable the head lacks
        assert!(!wide.is_within(&with("x", Type::int())));
        assert!(!with("x", Type::float()).is_within(&wide));
        assert!(!with("y", Type::int()).is_within(&wide));
        // a variable only the head has adds nothing
        assert!(Env::new().is_within(&wide));
    }

    #[test]
    fn a_dead_state_is_within_everything_and_nothing_is_within_a_dead_head() {
        let live = with("x", Type::int());
        assert!(Env::unreachable().is_within(&live));
        assert!(Env::unreachable().is_within(&Env::unreachable()));
        assert!(!live.is_within(&Env::unreachable()));
        assert!(!Env::new().is_within(&Env::unreachable()));
    }
}
