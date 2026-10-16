//! Converting native type declarations and merging them with docblock types.

use bleedthrough_syntax::{BuiltinType, ClassRefKind, Span, TypeHint, TypeKind};

use crate::diag::{SymbolDiagnostic, SymbolDiagnosticCode};
use crate::name::ResolvedName;
use crate::scope::{NoDeclarations, Scope};
use crate::special::ClassContext;
use crate::types::{Atom, Relation, Type};

/// Term: Type
///
/// Where an effective type came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum TypeSource {
    /// The native declaration.
    Native,
    /// A docblock that refines (or supplies) the type.
    Docblock,
    /// Neither exists: the type is unknown.
    Absent,
}

impl TypeSource {
    /// The stable lowercase spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Docblock => "docblock",
            Self::Absent => "absent",
        }
    }
}

/// Term: Type
///
/// The type used for a parameter, return value or property, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct EffectiveType {
    /// The type.
    pub ty: Type,
    /// Which source produced it.
    pub source: TypeSource,
}

impl EffectiveType {
    /// The type of something with no declaration at all.
    pub fn absent() -> Self {
        Self {
            ty: Type::unknown(),
            source: TypeSource::Absent,
        }
    }
}

/// Converts a native type declaration, resolving names in `scope` and `class`. Constructs the
/// syntax tree could not represent become `unknown`.
pub fn native_type(hint: &TypeHint, scope: &Scope, class: &ClassContext) -> Type {
    match &hint.kind {
        TypeKind::Named(name) => resolved_class(class.resolve_name(scope, name, &NoDeclarations)),
        TypeKind::Builtin(builtin) => builtin_type(*builtin, hint.span, class),
        TypeKind::Nullable(inner) => native_type(inner, scope, class).union(&Type::null()),
        TypeKind::Union(parts) => Type::union_all(
            parts
                .iter()
                .map(|p| native_type(p, scope, class))
                .collect::<Vec<_>>()
                .iter(),
        ),
        TypeKind::Intersection(parts) => intersection(parts, scope, class),
        TypeKind::Parenthesized(inner) => native_type(inner, scope, class),
        _ => Type::unknown(),
    }
}

fn resolved_class(resolved: ResolvedName) -> Type {
    match resolved {
        ResolvedName::Resolved(fqn) => Type::object(fqn),
        ResolvedName::LateStatic(fqn) => Type::of(Atom::Static(fqn)),
        _ => Type::unknown(),
    }
}

fn intersection(parts: &[TypeHint], scope: &Scope, class: &ClassContext) -> Type {
    let atoms: Vec<Atom> = parts
        .iter()
        .flat_map(|p| native_type(p, scope, class).atoms().to_vec())
        .collect();
    if atoms.contains(&Atom::Unknown) {
        return Type::unknown();
    }
    Type::of(Atom::Intersection(atoms))
}

fn builtin_type(builtin: BuiltinType, span: Span, class: &ClassContext) -> Type {
    match builtin {
        BuiltinType::Null => Type::null(),
        BuiltinType::True => Type::of(Atom::True),
        BuiltinType::False => Type::of(Atom::False),
        BuiltinType::Array => Type::array_of(Type::array_key(), Type::mixed()),
        BuiltinType::Callable => Type::of(Atom::Callable(None)),
        BuiltinType::Static => resolved_class(class.resolve(ClassRefKind::Static, span)),
        BuiltinType::SelfType => resolved_class(class.resolve(ClassRefKind::SelfRef, span)),
        BuiltinType::Parent => resolved_class(class.resolve(ClassRefKind::Parent, span)),
        BuiltinType::Void => Type::void(),
        BuiltinType::Never => Type::never(),
        BuiltinType::Float => Type::float(),
        BuiltinType::Bool => Type::bool(),
        BuiltinType::Int => Type::int(),
        BuiltinType::String => Type::string(),
        BuiltinType::Object => Type::of(Atom::Object),
        BuiltinType::Mixed => Type::mixed(),
        BuiltinType::Iterable => Type::of(Atom::Iterable {
            key: Box::new(Type::mixed()),
            value: Box::new(Type::mixed()),
        }),
        _ => Type::unknown(),
    }
}

/// Merges a native type with a docblock type. The docblock wins when it is a refinement (a subtype)
/// of the native type, or when there is no native type; otherwise the native type stays, with a
/// diagnostic when the two are provably disjoint and an informational one when the relation could
/// not be shown (class hierarchies are not consulted).
pub fn merge_types(
    native: Option<Type>,
    docblock: Option<Type>,
    span: Span,
    diagnostics: &mut Vec<SymbolDiagnostic>,
) -> EffectiveType {
    match (native, docblock) {
        (None, None) => EffectiveType::absent(),
        (Some(ty), None) => EffectiveType {
            ty,
            source: TypeSource::Native,
        },
        (None, Some(ty)) => EffectiveType {
            ty,
            source: TypeSource::Docblock,
        },
        (Some(native), Some(doc)) => match doc.relation_to(&native) {
            Relation::Subtype if doc == native => EffectiveType {
                ty: native,
                source: TypeSource::Native,
            },
            Relation::Subtype => EffectiveType {
                ty: doc,
                source: TypeSource::Docblock,
            },
            relation => {
                diagnostics.push(disagreement(relation, &native, &doc, span));
                EffectiveType {
                    ty: native,
                    source: TypeSource::Native,
                }
            }
        },
    }
}

fn disagreement(relation: Relation, native: &Type, doc: &Type, span: Span) -> SymbolDiagnostic {
    if relation == Relation::Disjoint {
        SymbolDiagnostic::new(
            SymbolDiagnosticCode::DocblockContradiction,
            format!("docblock type `{doc}` contradicts the native type `{native}`"),
            span,
        )
    } else {
        SymbolDiagnostic::new(
            SymbolDiagnosticCode::DocblockUnverified,
            format!("docblock type `{doc}` is not shown to refine the native type `{native}`"),
            span,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::name::Fqn;
    use bleedthrough_syntax::{Name, Node};

    fn hint(kind: TypeKind) -> TypeHint {
        Node::new(Span::new(0, 1), kind)
    }

    fn named(text: &str) -> TypeHint {
        hint(TypeKind::Named(Name::from_written(Span::new(0, 1), text)))
    }

    fn convert(h: &TypeHint) -> String {
        let scope = Scope::in_namespace("App");
        let class = ClassContext::new(
            Fqn::new("App\\Me"),
            bleedthrough_syntax::ClassLikeKind::Class,
            Some(Fqn::new("App\\Base")),
        );
        native_type(h, &scope, &class).to_string()
    }

    #[test]
    fn nullable_names_are_resolved() {
        let h = hint(TypeKind::Nullable(Box::new(named("Foo"))));
        assert_eq!(convert(&h), "App\\Foo|null");
    }

    #[test]
    fn unions_and_dnf_keep_intersections() {
        let dnf = hint(TypeKind::Union(vec![
            hint(TypeKind::Parenthesized(Box::new(hint(
                TypeKind::Intersection(vec![named("A"), named("B")]),
            )))),
            hint(TypeKind::Builtin(BuiltinType::Null)),
        ]));
        assert_eq!(convert(&dnf), "(App\\A&App\\B)|null");
        let plain = hint(TypeKind::Union(vec![
            hint(TypeKind::Builtin(BuiltinType::Int)),
            hint(TypeKind::Builtin(BuiltinType::String)),
        ]));
        assert_eq!(convert(&plain), "int|string");
    }

    #[test]
    fn special_names_use_the_class_context() {
        for (builtin, expected) in [
            (BuiltinType::SelfType, "App\\Me"),
            (BuiltinType::Static, "static(App\\Me)"),
            (BuiltinType::Parent, "App\\Base"),
            (BuiltinType::Iterable, "iterable"),
            (BuiltinType::Never, "never"),
            (BuiltinType::Array, "array<array-key, mixed>"),
        ] {
            assert_eq!(convert(&hint(TypeKind::Builtin(builtin))), expected);
        }
    }

    #[test]
    fn error_nodes_are_unknown() {
        let h = hint(TypeKind::Error { diagnostic: None });
        assert_eq!(convert(&h), "unknown");
    }

    fn merge(native: Option<&str>, doc: Option<&str>) -> (EffectiveType, Vec<SymbolDiagnostic>) {
        use crate::docblock::{TypeContext, parse_type};
        let parse = |t: &str| parse_type(t, &TypeContext::new(), 0).ty.unwrap();
        let mut diagnostics = Vec::new();
        let result = merge_types(
            native.map(parse),
            doc.map(parse),
            Span::new(0, 1),
            &mut diagnostics,
        );
        (result, diagnostics)
    }

    #[test]
    fn a_refining_docblock_wins() {
        let (t, d) = merge(Some("array"), Some("list<string>"));
        assert_eq!(t.ty.to_string(), "list<string>");
        assert_eq!(t.source, TypeSource::Docblock);
        assert!(d.is_empty());
    }

    #[test]
    fn a_contradicting_docblock_loses_with_a_diagnostic() {
        let (t, d) = merge(Some("int"), Some("string"));
        assert_eq!(t.ty.to_string(), "int");
        assert_eq!(t.source, TypeSource::Native);
        assert_eq!(d[0].code, SymbolDiagnosticCode::DocblockContradiction);
    }

    #[test]
    fn an_unverifiable_docblock_leaves_the_native_type() {
        let (t, d) = merge(Some("Foo"), Some("Bar"));
        assert_eq!(t.source, TypeSource::Native);
        assert_eq!(d[0].code, SymbolDiagnosticCode::DocblockUnverified);
    }

    #[test]
    fn a_missing_native_type_takes_the_docblock() {
        let (t, d) = merge(None, Some("int|null"));
        assert_eq!(
            (t.ty.to_string(), t.source),
            ("int|null".into(), TypeSource::Docblock)
        );
        assert!(d.is_empty());
        let (none, _) = merge(None, None);
        assert_eq!(none, EffectiveType::absent());
        assert!(none.ty.has_unknown());
    }

    #[test]
    fn an_identical_docblock_keeps_the_native_source() {
        let (t, d) = merge(Some("?int"), Some("int|null"));
        assert_eq!(t.source, TypeSource::Native);
        assert!(d.is_empty());
    }
}
