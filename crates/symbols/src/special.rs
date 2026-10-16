//! `self`, `static` and `parent`, resolved against the enclosing class-like.

use bleedthrough_syntax::{ClassLikeKind, ClassRefKind, Name, NameKind, Span};

use crate::name::{Fqn, NameContext, NameFailure, ResolvedName};
use crate::scope::{DeclaredNames, Scope};

/// Term: Fully qualified name
///
/// The class-like that encloses some code: what `self`, `static` and `parent` mean there. A closure
/// has the context of the code that defines it, so it is the same value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassContext {
    current: Option<(Fqn, ClassLikeKind)>,
    parent: Option<Fqn>,
}

impl ClassContext {
    /// Code outside any class-like.
    pub fn none() -> Self {
        Self {
            current: None,
            parent: None,
        }
    }

    /// Code inside a class-like with an optional declared parent (resolved).
    pub fn new(current: Fqn, kind: ClassLikeKind, parent: Option<Fqn>) -> Self {
        Self {
            current: Some((current, kind)),
            parent,
        }
    }

    /// The context seen inside a closure or arrow function defined here: the defining scope's.
    pub fn for_closure(&self) -> Self {
        self.clone()
    }

    /// The enclosing class-like, if any.
    pub fn current(&self) -> Option<&Fqn> {
        self.current.as_ref().map(|(fqn, _)| fqn)
    }

    /// The declared parent of the enclosing class-like, if any.
    pub fn parent(&self) -> Option<&Fqn> {
        self.parent.as_ref()
    }

    /// Resolves `self`, `static` or `parent`. In a trait, `self` and `static` both denote the using
    /// class, which is not known here, so both are late-static markers bound to the trait.
    pub fn resolve(&self, kind: ClassRefKind, span: Span) -> ResolvedName {
        let fail = |reason| ResolvedName::Failed { reason, span };
        let Some((current, class_kind)) = &self.current else {
            return fail(match kind {
                ClassRefKind::SelfRef => NameFailure::SelfOutsideClass,
                ClassRefKind::Static => NameFailure::StaticOutsideClass,
                _ => NameFailure::ParentOutsideClass,
            });
        };
        match kind {
            ClassRefKind::SelfRef if *class_kind == ClassLikeKind::Trait => {
                ResolvedName::LateStatic(current.clone())
            }
            ClassRefKind::SelfRef => ResolvedName::Resolved(current.clone()),
            ClassRefKind::Static => ResolvedName::LateStatic(current.clone()),
            _ => self.parent.clone().map_or_else(
                || fail(NameFailure::ParentWithoutParent),
                ResolvedName::Resolved,
            ),
        }
    }

    /// Resolves a written class name: the special names first, then the scope.
    pub fn resolve_name(
        &self,
        scope: &Scope,
        name: &Name,
        declared: &dyn DeclaredNames,
    ) -> ResolvedName {
        if name.kind == NameKind::Unqualified {
            let special = [
                ("self", ClassRefKind::SelfRef),
                ("static", ClassRefKind::Static),
                ("parent", ClassRefKind::Parent),
            ]
            .into_iter()
            .find(|(text, _)| name.text.eq_ignore_ascii_case(text));
            if let Some((_, kind)) = special {
                return self.resolve(kind, name.span);
            }
        }
        scope.resolve(name, NameContext::Class, declared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scope::NoDeclarations;

    fn in_class(parent: Option<&str>) -> ClassContext {
        ClassContext::new(
            Fqn::new("App\\Child"),
            ClassLikeKind::Class,
            parent.map(Fqn::new),
        )
    }

    #[test]
    fn self_static_and_parent() {
        let ctx = in_class(Some("App\\Base"));
        let span = Span::new(0, 4);
        assert_eq!(
            ctx.resolve(ClassRefKind::SelfRef, span),
            ResolvedName::Resolved(Fqn::new("App\\Child"))
        );
        assert_eq!(
            ctx.resolve(ClassRefKind::Static, span),
            ResolvedName::LateStatic(Fqn::new("App\\Child"))
        );
        assert_eq!(
            ctx.resolve(ClassRefKind::Parent, span),
            ResolvedName::Resolved(Fqn::new("App\\Base"))
        );
    }

    #[test]
    fn parent_without_a_parent_fails_naming_the_construct() {
        let r = in_class(None).resolve(ClassRefKind::Parent, Span::new(3, 9));
        let ResolvedName::Failed { reason, span } = r else {
            panic!("{r:?}")
        };
        assert_eq!(reason, NameFailure::ParentWithoutParent);
        assert!(reason.message().contains("parent"));
        assert_eq!(span, Span::new(3, 9));
    }

    #[test]
    fn outside_a_class_everything_fails() {
        let ctx = ClassContext::none();
        for kind in [
            ClassRefKind::SelfRef,
            ClassRefKind::Static,
            ClassRefKind::Parent,
        ] {
            assert!(matches!(
                ctx.resolve(kind, Span::new(0, 1)),
                ResolvedName::Failed { .. }
            ));
        }
    }

    #[test]
    fn a_closure_has_the_binding_of_its_defining_scope() {
        let ctx = in_class(Some("B"));
        let closure = ctx.for_closure();
        assert_eq!(closure, ctx);
        assert_eq!(
            closure.resolve(ClassRefKind::SelfRef, Span::new(0, 1)),
            ResolvedName::Resolved(Fqn::new("App\\Child"))
        );
    }

    #[test]
    fn in_a_trait_self_is_late_bound() {
        let ctx = ClassContext::new(Fqn::new("T"), ClassLikeKind::Trait, None);
        assert_eq!(
            ctx.resolve(ClassRefKind::SelfRef, Span::new(0, 1)),
            ResolvedName::LateStatic(Fqn::new("T"))
        );
    }

    #[test]
    fn written_special_names_resolve_before_the_scope() {
        let ctx = in_class(Some("Base"));
        let scope = Scope::in_namespace("App");
        let written = Name::from_written(Span::new(0, 6), "PARENT");
        assert_eq!(
            ctx.resolve_name(&scope, &written, &NoDeclarations),
            ResolvedName::Resolved(Fqn::new("Base"))
        );
        let other = Name::from_written(Span::new(0, 3), "Foo");
        assert_eq!(
            ctx.resolve_name(&scope, &other, &NoDeclarations),
            ResolvedName::Resolved(Fqn::new("App\\Foo"))
        );
    }
}
