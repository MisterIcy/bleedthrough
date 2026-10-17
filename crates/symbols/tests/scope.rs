//! Scopes by source offset, the function/constant fallback and the class context.

mod common;
use common::*;

use bleedthrough_symbols::{
    ClassContext, DeclaredNames, Existence, FileScopes, NameContext, NameFailure, ResolvedName,
    Scope,
};
use bleedthrough_syntax::{ClassLikeKind, ClassRefKind, Name, Node, Span, StmtKind};

struct Declares {
    function: Existence,
    constant: Existence,
}

impl DeclaredNames for Declares {
    fn function(&self, _name: &Fqn) -> Existence {
        self.function
    }

    fn constant(&self, _name: &Fqn) -> Existence {
        self.constant
    }
}

fn written(text: &str) -> Name {
    Name::from_written(Span::new(0, 1), text)
}

/// spec: name-resolution/Functions and constants fall back to the global namespace
#[test]
fn the_function_and_the_constant_question_are_asked_separately() {
    let scope = Scope::in_namespace("App");
    let name = written("thing");
    let only_function = Declares {
        function: Existence::Declared,
        constant: Existence::Absent,
    };
    let only_constant = Declares {
        function: Existence::Absent,
        constant: Existence::Declared,
    };
    assert_eq!(
        scope.resolve(&name, NameContext::Function, &only_function),
        ResolvedName::Resolved(Fqn::new("App\\thing"))
    );
    assert_eq!(
        scope.resolve(&name, NameContext::Function, &only_constant),
        ResolvedName::Resolved(Fqn::new("thing"))
    );
    assert_eq!(
        scope.resolve(&name, NameContext::Const, &only_function),
        ResolvedName::Resolved(Fqn::new("thing"))
    );
    assert_eq!(
        scope.resolve(&name, NameContext::Const, &only_constant),
        ResolvedName::Resolved(Fqn::new("App\\thing"))
    );
    let unsure = Declares {
        function: Existence::Unknown,
        constant: Existence::Unknown,
    };
    assert_eq!(
        scope.resolve(&name, NameContext::Function, &unsure),
        ResolvedName::FunctionOrConst {
            namespaced: Fqn::new("App\\thing"),
            global: Fqn::new("thing")
        }
    );
}

/// spec: name-resolution/Class names resolve through namespace and imports
#[test]
fn offsets_find_the_namespace_block_they_are_in() {
    let src = "namespace A { } namespace B { }";
    let scopes = FileScopes::build(parse(src).statements());
    let a_end = src.find("} namespace").unwrap() as u32 + 1;
    assert_eq!(scopes.at(0).namespace(), "A");
    assert_eq!(scopes.at(a_end - 1).namespace(), "A");
    // the space between the blocks belongs to neither
    assert_eq!(scopes.at(a_end).namespace(), "");
    assert_eq!(scopes.at(a_end + 1).namespace(), "B");
    assert_eq!(scopes.at(src.len() as u32 - 1).namespace(), "B");
    assert_eq!(scopes.at(src.len() as u32).namespace(), "");
    assert_eq!(scopes.at(10_000).namespace(), "");
}

/// spec: name-resolution/Class names resolve through namespace and imports
#[test]
fn an_empty_block_still_covers_its_own_offset() {
    let stmt = Node::new(
        Span::new(5, 5),
        StmtKind::Namespace {
            name: Some(written("Z")),
            braced: true,
            body: Vec::new(),
        },
    );
    let scopes = FileScopes::build(&[stmt]);
    assert_eq!(scopes.at(5).namespace(), "Z");
    assert_eq!(scopes.at(4).namespace(), "");
    assert_eq!(scopes.at(6).namespace(), "");
}

/// spec: name-resolution/Group, multiple and conditional imports are handled
#[test]
fn duplicate_import_diagnostics_are_collected_from_every_block() {
    let src = "use A\\X; use B\\X; namespace N { use C\\Y; use D\\Y; }";
    let scopes = FileScopes::build(parse(src).statements());
    assert_eq!(scopes.diagnostics().len(), 2);
    assert!(scopes.diagnostics()[0].span.start < scopes.diagnostics()[1].span.start);
}

/// spec: name-resolution/Special class names are resolved in context
#[test]
fn self_static_and_parent_resolve_against_the_class() {
    let class = ClassContext::new(
        Fqn::new("App\\Kid"),
        ClassLikeKind::Class,
        Some(Fqn::new("App\\Base")),
    );
    let span = Span::new(1, 2);
    assert_eq!(
        class.resolve(ClassRefKind::SelfRef, span),
        ResolvedName::Resolved(Fqn::new("App\\Kid"))
    );
    assert_eq!(
        class.resolve(ClassRefKind::Static, span),
        ResolvedName::LateStatic(Fqn::new("App\\Kid"))
    );
    assert_eq!(
        class.resolve(ClassRefKind::Parent, span),
        ResolvedName::Resolved(Fqn::new("App\\Base"))
    );
    assert_eq!(class.parent(), Some(&Fqn::new("App\\Base")));
    assert_eq!(class.current(), Some(&Fqn::new("App\\Kid")));
    let orphan = ClassContext::new(Fqn::new("App\\Kid"), ClassLikeKind::Class, None);
    assert_eq!(orphan.parent(), None);
    assert_eq!(
        orphan.resolve(ClassRefKind::Parent, span),
        ResolvedName::Failed {
            reason: NameFailure::ParentWithoutParent,
            span
        }
    );
    let tr = ClassContext::new(Fqn::new("App\\T"), ClassLikeKind::Trait, None);
    assert_eq!(
        tr.resolve(ClassRefKind::SelfRef, span),
        ResolvedName::LateStatic(Fqn::new("App\\T"))
    );
    let none = ClassContext::none();
    for (kind, reason) in [
        (ClassRefKind::SelfRef, NameFailure::SelfOutsideClass),
        (ClassRefKind::Static, NameFailure::StaticOutsideClass),
        (ClassRefKind::Parent, NameFailure::ParentOutsideClass),
    ] {
        assert_eq!(
            none.resolve(kind, span),
            ResolvedName::Failed { reason, span }
        );
    }
    assert_eq!(none.current(), None);
    assert_eq!(none.parent(), None);
}
