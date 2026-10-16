use super::*;

fn lit(text: &str) -> Atom {
    Atom::StringLiteral(text.into())
}

fn class(name: &str) -> Type {
    Type::object(Fqn::new(name))
}

fn shape(entries: Vec<(&str, Type, bool)>, sealed: bool) -> Atom {
    Atom::Shape {
        entries: entries
            .into_iter()
            .map(|(k, value, optional)| ShapeEntry {
                key: ShapeKey::Str(k.into()),
                value,
                optional,
            })
            .collect(),
        sealed,
    }
}

#[test]
fn literals_are_absorbed_by_their_general_kind() {
    let t = Type::from_atoms([Atom::String, lit("a"), Atom::String]);
    assert_eq!(t, Type::string());
    assert_eq!(t.to_string(), "string");
}

#[test]
fn union_order_does_not_matter() {
    let a = Type::from_atoms([Atom::Int, Atom::Null]);
    let b = Type::from_atoms([Atom::Null, Atom::Int]);
    assert_eq!(a, b);
    assert_eq!(a.to_string(), "int|null");
}

#[test]
fn true_and_false_make_bool() {
    assert_eq!(Type::from_atoms([Atom::True, Atom::False]), Type::bool());
    assert_eq!(Type::from_atoms([Atom::Bool, Atom::True]), Type::bool());
}

#[test]
fn mixed_absorbs_everything_but_unknown() {
    let t = Type::from_atoms([Atom::Int, Atom::Mixed, Atom::Null]);
    assert_eq!(t, Type::mixed());
    let with_unknown = Type::from_atoms([Atom::Mixed, Atom::Unknown, Atom::Int]);
    assert_eq!(with_unknown.atoms(), [Atom::Mixed, Atom::Unknown]);
    assert!(with_unknown.has_unknown());
}

#[test]
fn never_is_the_empty_union() {
    assert!(Type::never().is_never());
    assert_eq!(Type::never().to_string(), "never");
    assert_eq!(Type::never().union(&Type::int()), Type::int());
}

#[test]
fn unknown_united_with_a_type_is_marked() {
    let t = Type::unknown().union(&Type::string());
    assert!(t.has_unknown());
    assert_eq!(t.atoms().len(), 2);
    assert!(!Type::string().has_unknown());
}

#[test]
fn unknown_is_not_mixed() {
    assert_ne!(Type::unknown(), Type::mixed());
    assert!(!Type::mixed().has_unknown());
}

#[test]
fn objects_are_absorbed_by_object() {
    let t = Type::from_atoms([Atom::Object, Atom::named(Fqn::new("Foo"))]);
    assert_eq!(t.to_string(), "object");
}

#[test]
fn int_ranges_absorb_literals_and_narrower_ranges() {
    let positive = Atom::IntRange {
        min: Some(1),
        max: None,
    };
    let t = Type::from_atoms([positive.clone(), Atom::IntLiteral(5), Atom::IntLiteral(-5)]);
    assert_eq!(t.to_string(), "-5|positive-int");
    assert_eq!(Type::from_atoms([positive, Atom::Int]).to_string(), "int");
    let single = Atom::IntRange {
        min: Some(3),
        max: Some(3),
    };
    assert_eq!(Type::of(single).to_string(), "3");
}

#[test]
fn long_literals_degrade_to_a_refined_kind() {
    let long = "x".repeat(DEFAULT_LITERAL_CAP + 1);
    assert_eq!(Type::of(lit(&long)).to_string(), "non-empty-string");
    assert_eq!(Type::of(lit("abc")).to_string(), "'abc'");
}

#[test]
fn a_large_union_widens_and_says_so() {
    let atoms: Vec<Atom> = (0..100)
        .map(|i| Atom::named(Fqn::new(&format!("C{i}"))))
        .collect();
    let t = Type::from_atoms(atoms);
    assert!(t.is_widened());
    assert_eq!(t.to_string(), "mixed");
    let again = Type::from_atoms(
        (0..100)
            .rev()
            .map(|i| Atom::named(Fqn::new(&format!("C{i}")))),
    );
    assert_eq!(t, again);
}

#[test]
fn many_literals_degrade_before_they_widen_to_mixed() {
    let t = Type::from_atoms((0..100).map(Atom::IntLiteral));
    assert!(t.is_widened());
    assert_eq!(t.to_string(), "int");
}

#[test]
fn the_widened_flag_is_inherited() {
    let wide = Type::from_atoms((0..100).map(Atom::IntLiteral));
    assert!(wide.union(&Type::null()).is_widened());
}

#[test]
fn a_custom_cap_is_honoured() {
    let t = Type::from_atoms_capped([Atom::Int, Atom::String, Atom::Float], 2);
    assert!(t.is_widened());
    assert!(t.is_mixed());
}

#[test]
fn intersections_are_kept() {
    let a = Atom::Intersection(vec![Atom::named(Fqn::new("B")), Atom::named(Fqn::new("A"))]);
    let t = Type::from_atoms([a, Atom::Null]);
    assert_eq!(t.to_string(), "(A&B)|null");
}

#[test]
fn shapes_sort_their_keys() {
    let a = shape(
        vec![("name", Type::string(), true), ("id", Type::int(), false)],
        true,
    );
    assert_eq!(Type::of(a).to_string(), "array{id: int, name?: string}");
}

fn assert_renders(cases: Vec<(Atom, &str)>) {
    for (atom, expected) in cases {
        assert_eq!(atom.to_string(), expected);
    }
}

#[test]
fn rendering_covers_the_scalar_atoms() {
    assert_renders(vec![
        (Atom::Null, "null"),
        (Atom::Void, "void"),
        (Atom::Mixed, "mixed"),
        (Atom::Unknown, "unknown"),
        (Atom::True, "true"),
        (Atom::False, "false"),
        (Atom::Bool, "bool"),
        (Atom::Int, "int"),
        (Atom::IntLiteral(-3), "-3"),
        (
            Atom::IntRange {
                min: Some(0),
                max: None,
            },
            "non-negative-int",
        ),
        (
            Atom::IntRange {
                min: None,
                max: Some(-1),
            },
            "negative-int",
        ),
        (
            Atom::IntRange {
                min: Some(2),
                max: Some(9),
            },
            "int<2, 9>",
        ),
        (
            Atom::IntRange {
                min: None,
                max: Some(9),
            },
            "int<min, 9>",
        ),
        (Atom::Float, "float"),
        (Atom::String, "string"),
        (Atom::NonEmptyString, "non-empty-string"),
        (Atom::NumericString, "numeric-string"),
        (lit("it's"), "'it\\'s'"),
        (Atom::ClassString(None), "class-string"),
        (
            Atom::ClassString(Some(Fqn::new("Foo"))),
            "class-string<Foo>",
        ),
    ]);
}

#[test]
fn rendering_covers_the_compound_atoms() {
    assert_renders(vec![
        (
            Atom::Array {
                key: Box::new(Type::array_key()),
                value: Box::new(Type::mixed()),
                non_empty: false,
            },
            "array<array-key, mixed>",
        ),
        (
            Atom::Array {
                key: Box::new(Type::string()),
                value: Box::new(Type::int()),
                non_empty: true,
            },
            "non-empty-array<string, int>",
        ),
        (
            Atom::List {
                value: Box::new(Type::string()),
                non_empty: false,
            },
            "list<string>",
        ),
        (
            Atom::List {
                value: Box::new(Type::string()),
                non_empty: true,
            },
            "non-empty-list<string>",
        ),
        (shape(vec![], true), "array{}"),
    ]);
}

#[test]
fn rendering_covers_objects_callables_and_iterables() {
    assert_renders(vec![
        (
            shape(vec![("a b", Type::int(), false)], false),
            "array{'a b': int, ...}",
        ),
        (Atom::Object, "object"),
        (Atom::named(Fqn::new("App\\Foo")), "App\\Foo"),
        (
            Atom::Named(NamedObject {
                name: Fqn::new("Box"),
                args: vec![Type::int(), Type::string()],
            }),
            "Box<int, string>",
        ),
        (
            Atom::EnumCase {
                enum_name: Fqn::new("Suit"),
                case: "Hearts".into(),
            },
            "Suit::Hearts",
        ),
        (Atom::Static(Fqn::new("Foo")), "static(Foo)"),
        (Atom::Template("T".into()), "T"),
        (Atom::Callable(None), "callable"),
        (callable_atom(), "callable(int, string=, ...bool): void"),
        (Atom::Resource, "resource"),
    ]);
}

#[test]
fn rendering_covers_iterables() {
    let iterable = |key: Type, value: Type| Atom::Iterable {
        key: Box::new(key),
        value: Box::new(value),
    };
    assert_renders(vec![
        (iterable(Type::mixed(), Type::mixed()), "iterable"),
        (
            iterable(Type::int(), Type::string()),
            "iterable<int, string>",
        ),
    ]);
}

fn callable_atom() -> Atom {
    let param = |ty: Type, optional: bool, variadic: bool| CallableParam {
        ty,
        optional,
        variadic,
    };
    Atom::Callable(Some(Box::new(CallableSig {
        params: vec![
            param(Type::int(), false, false),
            param(Type::string(), true, false),
            param(Type::bool(), false, true),
        ],
        ret: Type::void(),
    })))
}

#[test]
fn relations_between_scalars() {
    use Relation::{Disjoint, Subtype, Undetermined};
    assert_eq!(Type::of(lit("a")).relation_to(&Type::string()), Subtype);
    assert_eq!(Type::int().relation_to(&Type::string()), Disjoint);
    assert_eq!(Type::int().relation_to(&Type::float()), Undetermined);
    assert_eq!(Type::null().relation_to(&Type::mixed()), Subtype);
    assert_eq!(Type::unknown().relation_to(&Type::int()), Undetermined);
    assert_eq!(Type::never().relation_to(&Type::int()), Subtype);
    let nullable = Type::int().union(&Type::null());
    assert_eq!(Type::int().relation_to(&nullable), Subtype);
    assert_eq!(nullable.relation_to(&Type::int()), Undetermined);
}

#[test]
fn relations_between_arrays_and_classes() {
    use Relation::{Disjoint, Subtype, Undetermined};
    let list = Type::list_of(Type::string());
    assert_eq!(
        list.relation_to(&Type::of(Atom::Array {
            key: Box::new(Type::array_key()),
            value: Box::new(Type::string()),
            non_empty: false,
        })),
        Subtype
    );
    let plain = Type::array_of(Type::array_key(), Type::mixed());
    assert_eq!(list.relation_to(&plain), Subtype);
    assert_eq!(plain.relation_to(&list), Undetermined);
    assert_eq!(class("Foo").relation_to(&Type::of(Atom::Object)), Subtype);
    assert_eq!(class("Foo").relation_to(&class("foo")), Subtype);
    assert_eq!(class("Foo").relation_to(&class("Bar")), Undetermined);
    assert_eq!(class("Foo").relation_to(&Type::string()), Disjoint);
    let array = Type::of(Atom::Array {
        key: Box::new(Type::string()),
        value: Box::new(Type::int()),
        non_empty: true,
    });
    assert_eq!(array.relation_to(&Type::string()), Disjoint);
}

#[test]
fn shape_subtyping() {
    let narrow = Type::of(shape(
        vec![("id", Type::int(), false), ("n", Type::string(), false)],
        true,
    ));
    let wide = Type::of(shape(
        vec![("id", Type::int(), false), ("n", Type::string(), true)],
        true,
    ));
    assert_eq!(narrow.relation_to(&wide), Relation::Subtype);
    assert_eq!(wide.relation_to(&narrow), Relation::Undetermined);
}

#[test]
fn substitution_replaces_template_parameters() {
    let list = Type::list_of(Type::of(Atom::Template("T".into())));
    let substituted = list.substitute(&[("T", Type::string())]);
    assert_eq!(substituted.to_string(), "list<string>");
    let kept = list.substitute(&[("U", Type::int())]);
    assert_eq!(kept.to_string(), "list<T>");
}

#[test]
fn filter_and_without_null() {
    let t = Type::from_atoms([Atom::Int, Atom::Null, Atom::String]);
    assert_eq!(t.without_null().to_string(), "int|string");
    assert!(t.contains_null());
    assert!(!t.without_null().contains_null());
}
