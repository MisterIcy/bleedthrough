//! Hierarchy queries: ancestors, subtype tests, subclasses and implementors.

use std::collections::BTreeSet;

use super::SymbolTable;
use crate::name::Fqn;
use crate::summary::{ClassDecl, ClassKind};

/// Term: Symbol table
///
/// The answer to "is this a subtype of that": a definite no needs the whole ancestry to be known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Subtyping {
    /// It is a subtype (or the same type).
    Yes,
    /// It is not, and every ancestor is known.
    No,
    /// It cannot be told: the class is not loaded or has an unresolved ancestor.
    Unknown,
}

impl SymbolTable {
    /// The direct parent of a class, when linked.
    pub fn parent_of(&self, class: &Fqn) -> Option<&ClassDecl> {
        let parent = self.links.get(class.key())?.parent.as_ref()?;
        self.class(parent)
    }

    /// Whether the class-like has an ancestor that is not loaded (so absence of a member proves
    /// nothing).
    pub fn has_unresolved_ancestor(&self, class: &Fqn) -> bool {
        self.links.get(class.key()).is_some_and(|l| l.incomplete)
    }

    /// The ancestors named but not loaded, directly declared by `class`.
    pub fn unresolved_ancestors(&self, class: &Fqn) -> &[Fqn] {
        self.links.get(class.key()).map_or(&[], |l| &l.missing)
    }

    /// Every class-like `class` inherits from or uses, transitively, in lookup order: for each
    /// level the parent chain first, then interfaces, then traits; each listed once.
    pub fn ancestors(&self, class: &Fqn) -> Vec<&ClassDecl> {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut out = Vec::new();
        let mut stack: Vec<&Fqn> = Vec::new();
        if let Some(l) = self.links.get(class.key()) {
            stack.extend(l.targets().collect::<Vec<_>>().into_iter().rev());
        }
        while let Some(next) = stack.pop() {
            if !seen.insert(next.key()) {
                continue;
            }
            let Some(decl) = self.class(next) else {
                continue;
            };
            out.push(decl);
            if let Some(l) = self.links.get(next.key()) {
                stack.extend(l.targets().collect::<Vec<_>>().into_iter().rev());
            }
        }
        out
    }

    /// The interfaces `class` implements, transitively, in name order.
    pub fn interfaces_of(&self, class: &Fqn) -> Vec<&ClassDecl> {
        let mut found: Vec<&ClassDecl> = self
            .ancestors(class)
            .into_iter()
            .filter(|c| c.kind == ClassKind::Interface)
            .collect();
        found.sort_by(|a, b| a.name.cmp(&b.name));
        found
    }

    /// Whether `sub` is `sup` or inherits from it (through parents, interfaces or traits).
    pub fn is_subtype(&self, sub: &Fqn, sup: &Fqn) -> Subtyping {
        if sub == sup {
            return Subtyping::Yes;
        }
        if self.class(sub).is_none() {
            return Subtyping::Unknown;
        }
        if self.ancestors(sub).iter().any(|c| c.name == *sup) {
            return Subtyping::Yes;
        }
        if self.has_unresolved_ancestor(sub) {
            Subtyping::Unknown
        } else {
            Subtyping::No
        }
    }

    /// Every loaded class-like that inherits from `of`, transitively, in name order.
    pub fn subtypes(&self, of: &Fqn) -> Vec<&ClassDecl> {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut stack: Vec<&str> = vec![of.key()];
        let mut found: Vec<&ClassDecl> = Vec::new();
        while let Some(key) = stack.pop() {
            for child in self.children.get(key).map_or(&[][..], Vec::as_slice) {
                if seen.insert(child) {
                    stack.push(child);
                    if let Some(decl) = self.classes.get(child).map(|s| self.class_at(*s)) {
                        found.push(decl);
                    }
                }
            }
        }
        found.sort_by(|a, b| a.name.cmp(&b.name));
        found
    }

    /// The classes and enums that implement the interface `of`, transitively, in name order.
    pub fn implementors(&self, of: &Fqn) -> Vec<&ClassDecl> {
        self.subtypes(of)
            .into_iter()
            .filter(|c| matches!(c.kind, ClassKind::Class | ClassKind::Enum))
            .collect()
    }
}
