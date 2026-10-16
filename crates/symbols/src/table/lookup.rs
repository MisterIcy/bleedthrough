//! Member lookup in PHP's order: the class, its traits, its parents, then interfaces.

use std::collections::BTreeSet;

use super::SymbolTable;
use crate::name::Fqn;
use crate::summary::{
    Adaptation, CaseDecl, ClassDecl, ClassKind, ConstDecl, MethodDecl, PropertyDecl, TraitUse,
    Visibility,
};

/// Term: Symbol table
///
/// One declaration a lookup found.
#[derive(Debug, Clone, Copy)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct Candidate<'t, M> {
    /// The declaration.
    pub member: &'t M,
    /// The class-like whose body declares it (a trait, for trait members).
    pub owner: &'t ClassDecl,
    /// The class-like that has it: the using class for a trait member, else the owner.
    pub context: &'t ClassDecl,
    /// The visibility in force (a trait `as` adaptation can change it).
    pub visibility: Visibility,
    /// Whether the calling context may access it.
    pub accessible: bool,
}

/// Term: Symbol table
///
/// Which magic methods would catch an access that found nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct Magic {
    /// `__call` exists.
    pub call: bool,
    /// `__callStatic` exists.
    pub call_static: bool,
    /// `__get` exists.
    pub get: bool,
    /// `__set` exists.
    pub set: bool,
}

impl Magic {
    /// Whether any magic method applies.
    pub fn any(&self) -> bool {
        self.call || self.call_static || self.get || self.set
    }
}

/// Term: Symbol table
///
/// The result of looking a member up on a class.
#[derive(Debug, Clone)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct Lookup<'t, M> {
    /// Every declaration found: the one PHP would use first, then interface declarations.
    pub candidates: Vec<Candidate<'t, M>>,
    /// Magic methods that would catch the access; set only when nothing was found.
    pub magic: Magic,
    /// The class is not loaded, or has an unresolved ancestor: not finding something proves nothing.
    pub incomplete: bool,
}

impl<'t, M> Lookup<'t, M> {
    /// The first candidate the caller may access.
    pub fn accessible(&self) -> Option<&Candidate<'t, M>> {
        self.candidates.iter().find(|c| c.accessible)
    }
}

/// A class constant or an enum case, both reachable as `Class::NAME`.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum ClassConstant<'t> {
    /// A declared constant.
    Const(&'t ConstDecl),
    /// An enum case.
    Case(&'t CaseDecl),
}

/// How a member kind is found inside one class-like body.
trait Kind {
    type Item;
    fn own<'t>(class: &'t ClassDecl, name: &str) -> Option<(&'t Self::Item, Visibility)>;
    fn is_method() -> bool {
        false
    }
}

struct Methods;
struct Properties;
struct Constants;

impl Kind for Methods {
    type Item = MethodDecl;
    fn own<'t>(class: &'t ClassDecl, name: &str) -> Option<(&'t MethodDecl, Visibility)> {
        class
            .methods
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(name))
            .map(|m| (m, m.visibility))
    }
    fn is_method() -> bool {
        true
    }
}

impl Kind for Properties {
    type Item = PropertyDecl;
    fn own<'t>(class: &'t ClassDecl, name: &str) -> Option<(&'t PropertyDecl, Visibility)> {
        class
            .properties
            .iter()
            .find(|p| &*p.name == name)
            .map(|p| (p, p.visibility))
    }
}

impl Kind for Constants {
    type Item = ConstDecl;
    fn own<'t>(class: &'t ClassDecl, name: &str) -> Option<(&'t ConstDecl, Visibility)> {
        class
            .constants
            .iter()
            .find(|c| &*c.name == name)
            .map(|c| (c, c.visibility))
    }
}

/// A member found in a trait: the member, its visibility under the adaptations, the trait.
type TraitMember<'t, M> = (&'t M, Visibility, &'t ClassDecl);

/// How deep trait-in-trait nesting is followed.
const MAX_TRAIT_DEPTH: usize = 16;

impl SymbolTable {
    /// Looks a method up on `class` for code running in `caller` (`None` outside any class).
    pub fn method<'t>(
        &'t self,
        class: &Fqn,
        name: &str,
        caller: Option<&Fqn>,
    ) -> Lookup<'t, MethodDecl> {
        let mut result = self.lookup::<Methods>(class, name, caller);
        if result.candidates.is_empty() {
            result.magic.call = self.has_method(class, "__call");
            result.magic.call_static = self.has_method(class, "__callStatic");
        }
        result
    }

    /// Looks a property up on `class` for code running in `caller`.
    pub fn property<'t>(
        &'t self,
        class: &Fqn,
        name: &str,
        caller: Option<&Fqn>,
    ) -> Lookup<'t, PropertyDecl> {
        let mut result = self.lookup::<Properties>(class, name, caller);
        if result.candidates.is_empty() {
            result.magic.get = self.has_method(class, "__get");
            result.magic.set = self.has_method(class, "__set");
        }
        result
    }

    /// Looks a class constant or enum case up on `class`.
    pub fn class_constant<'t>(
        &'t self,
        class: &Fqn,
        name: &str,
        caller: Option<&Fqn>,
    ) -> Lookup<'t, ConstDecl> {
        self.lookup::<Constants>(class, name, caller)
    }

    /// The enum case `name` of the enum `class`, if it is one.
    pub fn enum_case<'t>(&'t self, class: &Fqn, name: &str) -> Option<ClassConstant<'t>> {
        let decl = self.class(class)?;
        decl.cases
            .iter()
            .find(|c| &*c.name == name)
            .map(ClassConstant::Case)
    }

    fn has_method(&self, class: &Fqn, name: &str) -> bool {
        self.lookup::<Methods>(class, name, None)
            .candidates
            .iter()
            .any(|_| true)
    }

    fn lookup<'t, K: Kind>(
        &'t self,
        class: &Fqn,
        name: &str,
        caller: Option<&Fqn>,
    ) -> Lookup<'t, K::Item> {
        let Some(start) = self.class(class) else {
            return Lookup {
                candidates: Vec::new(),
                magic: Magic::default(),
                incomplete: true,
            };
        };
        let mut candidates: Vec<Candidate<'t, K::Item>> = Vec::new();
        if let Some(found) = self.find_in_chain::<K>(start, name) {
            candidates.push(found);
        }
        for interface in self.interfaces_of(&start.name) {
            if let Some((member, visibility)) = K::own(interface, name)
                && candidates.iter().all(|c| c.owner.name != interface.name)
            {
                candidates.push(Candidate {
                    member,
                    owner: interface,
                    context: interface,
                    visibility,
                    accessible: true,
                });
            }
        }
        for c in &mut candidates {
            c.accessible = self.accessible(c, caller);
        }
        Lookup {
            candidates,
            magic: Magic::default(),
            incomplete: self.has_unresolved_ancestor(&start.name),
        }
    }

    /// The class, then its traits, then its parent, and so on up the parent chain.
    fn find_in_chain<'t, K: Kind>(
        &'t self,
        start: &'t ClassDecl,
        name: &str,
    ) -> Option<Candidate<'t, K::Item>> {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut current = Some(start);
        while let Some(class) = current {
            if !seen.insert(class.name.key()) {
                break;
            }
            if let Some((member, visibility)) = K::own(class, name) {
                return Some(Candidate {
                    member,
                    owner: class,
                    context: class,
                    visibility,
                    accessible: true,
                });
            }
            if let Some(found) = self.find_in_traits::<K>(class, name) {
                return Some(found);
            }
            current = self.parent_of(&class.name);
        }
        None
    }

    fn find_in_traits<'t, K: Kind>(
        &'t self,
        class: &'t ClassDecl,
        name: &str,
    ) -> Option<Candidate<'t, K::Item>> {
        for usage in &class.trait_uses {
            for trait_name in &usage.traits {
                let Some(trait_decl) = self.class(trait_name) else {
                    continue;
                };
                if let Some((member, visibility, owner)) =
                    self.trait_member::<K>(usage, trait_decl, name, 0)
                {
                    return Some(Candidate {
                        member,
                        owner,
                        context: class,
                        visibility,
                        accessible: true,
                    });
                }
            }
        }
        None
    }

    /// A member of `trait_decl` (or of traits it uses) under the adaptations of `usage`.
    fn trait_member<'t, K: Kind>(
        &'t self,
        usage: &TraitUse,
        trait_decl: &'t ClassDecl,
        name: &str,
        depth: usize,
    ) -> Option<TraitMember<'t, K::Item>> {
        if depth > MAX_TRAIT_DEPTH {
            return None;
        }
        if K::is_method() {
            if let Some(found) = self.aliased::<K>(usage, trait_decl, name) {
                return Some(found);
            }
            let excluded = usage.adaptations.iter().any(|a| match a {
                Adaptation::InsteadOf {
                    method, excluded, ..
                } => method.eq_ignore_ascii_case(name) && excluded.contains(&trait_decl.name),
                _ => false,
            });
            if excluded {
                return None;
            }
        }
        if let Some((member, visibility)) = K::own(trait_decl, name) {
            let visibility = if K::is_method() {
                restyled(usage, &trait_decl.name, name).unwrap_or(visibility)
            } else {
                visibility
            };
            return Some((member, visibility, trait_decl));
        }
        trait_decl
            .trait_uses
            .iter()
            .flat_map(|u| u.traits.iter().map(move |t| (u, t)))
            .filter_map(|(inner, t)| self.class(t).map(|d| (inner, d)))
            .find_map(|(inner, d)| self.trait_member::<K>(inner, d, name, depth + 1))
    }

    /// `m as alias` makes the trait method reachable under a new name.
    fn aliased<'t, K: Kind>(
        &'t self,
        usage: &TraitUse,
        trait_decl: &'t ClassDecl,
        name: &str,
    ) -> Option<TraitMember<'t, K::Item>> {
        usage.adaptations.iter().find_map(|a| match a {
            Adaptation::Alias {
                trait_name,
                method,
                visibility,
                alias: Some(alias),
            } if alias.eq_ignore_ascii_case(name)
                && trait_name.as_ref().is_none_or(|t| *t == trait_decl.name) =>
            {
                K::own(trait_decl, method).map(|(m, v)| (m, visibility.unwrap_or(v), trait_decl))
            }
            _ => None,
        })
    }

    /// The caller's right to use a candidate.
    fn accessible<M>(&self, candidate: &Candidate<'_, M>, caller: Option<&Fqn>) -> bool {
        match candidate.visibility {
            Visibility::Public => true,
            Visibility::Protected => caller.is_some_and(|c| {
                [&candidate.context.name, &candidate.owner.name]
                    .iter()
                    .any(|home| {
                        self.is_subtype(c, home) == super::Subtyping::Yes
                            || self.is_subtype(home, c) == super::Subtyping::Yes
                    })
            }),
            Visibility::Private => caller.is_some_and(|c| *c == candidate.context.name),
        }
    }

    /// Whether `class` is declared as an interface.
    pub fn is_interface(&self, class: &Fqn) -> bool {
        self.class(class)
            .is_some_and(|c| c.kind == ClassKind::Interface)
    }
}

/// A visibility-only adaptation `m as protected;` for the trait method `name`.
fn restyled(usage: &TraitUse, trait_name: &Fqn, name: &str) -> Option<Visibility> {
    usage.adaptations.iter().find_map(|a| match a {
        Adaptation::Alias {
            trait_name: from,
            method,
            visibility: Some(visibility),
            alias: None,
        } if method.eq_ignore_ascii_case(name) && from.as_ref().is_none_or(|t| t == trait_name) => {
            Some(*visibility)
        }
        _ => None,
    })
}
