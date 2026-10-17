//! The type model through its public API: atoms, normalisation caps, the structural relation and
//! the class names a type mentions. Written against the surviving mutants of `cargo xtask mutants`.

use bleedthrough_symbols::*;

use Relation::{Disjoint, Subtype, Undetermined};

fn lit(text: &str) -> Atom {
    Atom::StringLiteral(text.into())
}

fn class(name: &str) -> Atom {
    Atom::named(Fqn::new(name))
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

fn int_shape(n: i64, sealed: bool) -> Atom {
    Atom::Shape {
        entries: (0..n)
            .map(|i| ShapeEntry {
                key: ShapeKey::Int(i),
                value: Type::int(),
                optional: false,
            })
            .collect(),
        sealed,
    }
}

fn array(key: Type, value: Type, non_empty: bool) -> Atom {
    Atom::Array {
        key: Box::new(key),
        value: Box::new(value),
        non_empty,
    }
}

fn list(value: Type, non_empty: bool) -> Atom {
    Atom::List {
        value: Box::new(value),
        non_empty,
    }
}

fn iterable(key: Type, value: Type) -> Atom {
    Atom::Iterable {
        key: Box::new(key),
        value: Box::new(value),
    }
}

fn range(min: Option<i64>, max: Option<i64>) -> Atom {
    Atom::IntRange { min, max }
}

fn rel(a: Atom, b: Atom) -> Relation {
    Type::of(a).relation_to(&Type::of(b))
}

fn names(t: &Type) -> Vec<String> {
    let mut out = Vec::new();
    t.collect_class_names(&mut out);
    out.iter().map(|f| f.as_str().to_string()).collect()
}

// ---- atoms and type basics ----

/// spec: type-model/The atoms cover PHP's type system
#[test]
fn atom_predicates_classify_each_family() {
    let strings = [
        Atom::String,
        Atom::NonEmptyString,
        Atom::NumericString,
        lit("a"),
        Atom::ClassString(None),
    ];
    let ints = [Atom::Int, Atom::IntLiteral(1), range(Some(0), None)];
    let bools = [Atom::True, Atom::False, Atom::Bool];
    let arrays = [
        array(Type::int(), Type::int(), false),
        list(Type::int(), false),
        shape(vec![], true),
    ];
    let objects = [
        Atom::Object,
        class("A"),
        Atom::EnumCase {
            enum_name: Fqn::new("E"),
            case: "A".into(),
        },
        Atom::Static(Fqn::new("A")),
        Atom::Intersection(vec![class("A"), class("B")]),
    ];
    type Family<'a> = (&'a [Atom], fn(&Atom) -> bool);
    let families: [Family<'_>; 5] = [
        (&strings, Atom::is_string),
        (&ints, Atom::is_int),
        (&bools, Atom::is_bool),
        (&arrays, Atom::is_array),
        (&objects, Atom::is_object),
    ];
    for (i, (members, check)) in families.iter().enumerate() {
        for (j, (others, _)) in families.iter().enumerate() {
            for atom in *others {
                assert_eq!(check(atom), i == j, "{atom:?} in family {i}");
            }
        }
        assert!(members.iter().all(check));
    }
    for atom in [Atom::Null, Atom::Float, Atom::Mixed, Atom::Resource] {
        assert!(!atom.is_string() && !atom.is_int() && !atom.is_bool());
        assert!(!atom.is_array() && !atom.is_object());
    }
}

/// spec: type-model/Types are canonical unions of atoms
#[test]
fn never_is_empty_and_other_types_are_not() {
    assert!(Type::never().is_never());
    assert!(!Type::int().is_never());
    assert!(!Type::mixed().is_never());
    assert_eq!(Type::never(), Type::from_atoms([]));
}

/// spec: type-model/Unions are bounded
#[test]
fn only_a_widened_union_reports_widening() {
    assert!(!Type::int().is_widened());
    let wide = Type::from_atoms_capped([Atom::Int, Atom::Null, Atom::Float], 2);
    assert!(wide.is_widened());
    assert!(wide.union(&Type::int()).is_widened());
    assert!(Type::int().union(&wide).is_widened());
}

/// spec: type-model/Unions are bounded
#[test]
fn a_union_at_its_cap_is_kept_and_one_over_widens() {
    let at_cap = Type::from_atoms_capped([Atom::Int, Atom::Null, Atom::Float], 3);
    assert!(!at_cap.is_widened());
    assert_eq!(at_cap.atoms().len(), 3);
    let over = Type::from_atoms_capped([Atom::Int, Atom::Null, Atom::Float, Atom::Resource], 3);
    assert!(over.is_widened());
    assert!(over.is_mixed());
}

#[test]
fn degraded_literals_that_fit_the_cap_are_kept_but_otherwise_become_mixed() {
    let atoms = [
        Atom::Float,
        Atom::Null,
        Atom::IntLiteral(1),
        Atom::IntLiteral(2),
    ];
    let fits = Type::from_atoms_capped(atoms.clone(), 3);
    assert!(fits.is_widened());
    assert_eq!(fits.to_string(), "int|float|null");
    let spills = Type::from_atoms_capped(atoms, 2);
    assert!(spills.is_widened() && spills.is_mixed());
    let with_unknown =
        Type::from_atoms_capped([Atom::Float, Atom::Null, Atom::Unknown, Atom::Resource], 2);
    assert_eq!(with_unknown.atoms(), [Atom::Mixed, Atom::Unknown]);
}

#[test]
fn the_absorption_limit_degrades_literals_before_the_cap_is_applied() {
    let literals = |n: i64| (0..n).map(Atom::IntLiteral);
    let at_limit = Type::from_atoms_capped(literals(512), 1000);
    assert!(!at_limit.is_widened());
    assert_eq!(at_limit.atoms().len(), 512);
    let over = Type::from_atoms_capped(literals(513), 1000);
    assert!(over.is_widened());
    assert_eq!(over.atoms(), [Atom::Int]);
}

/// spec: type-model/Types are canonical unions of atoms
#[test]
fn int_ranges_are_canonicalised() {
    assert_eq!(Type::of(range(None, None)), Type::int());
    assert_eq!(
        Type::of(range(Some(3), Some(3))),
        Type::of(Atom::IntLiteral(3))
    );
    assert_eq!(Type::of(range(Some(5), Some(2))), Type::int());
    assert_eq!(Type::of(range(Some(2), Some(5))).to_string(), "int<2, 5>");
    assert_eq!(Type::of(range(Some(2), None)).to_string(), "int<2, max>");
    assert_eq!(Type::of(range(None, Some(2))).to_string(), "int<min, 2>");
}

#[test]
fn overlong_literals_degrade_and_shapes_and_intersections_are_canonical() {
    let long = "x".repeat(DEFAULT_LITERAL_CAP + 1);
    assert_eq!(Type::of(lit(&long)).to_string(), "non-empty-string");
    let exact = "x".repeat(DEFAULT_LITERAL_CAP);
    assert_eq!(Type::of(lit(&exact)).atoms(), [lit(&exact)]);

    let unordered = shape(
        vec![("b", Type::int(), false), ("a", Type::int(), false)],
        true,
    );
    assert_eq!(Type::of(unordered).to_string(), "array{a: int, b: int}");

    let nested = Atom::Intersection(vec![
        Atom::Intersection(vec![class("B"), class("A")]),
        class("A"),
    ]);
    assert_eq!(Type::of(nested).to_string(), "A&B");
    assert_eq!(Type::of(Atom::Intersection(vec![])), Type::of(Atom::Object));
    assert_eq!(
        Type::of(Atom::Intersection(vec![class("A"), class("A")])),
        Type::of(class("A"))
    );
}

#[test]
fn equivalent_atoms_are_absorbed_independent_of_input_order() {
    let pool = [
        Atom::Int,
        Atom::IntLiteral(1),
        range(Some(0), None),
        Atom::String,
        lit("a"),
        Atom::NonEmptyString,
        Atom::NumericString,
        Atom::ClassString(None),
        Atom::ClassString(Some(Fqn::new("A"))),
        Atom::True,
        Atom::Bool,
        class("A"),
        Atom::Object,
        array(Type::int(), Type::int(), false),
        list(Type::int(), false),
        shape(vec![], true),
        iterable(Type::mixed(), Type::mixed()),
        Atom::Callable(None),
    ];
    for a in &pool {
        for b in &pool {
            let forward = Type::from_atoms([a.clone(), b.clone()]);
            let backward = Type::from_atoms([b.clone(), a.clone()]);
            assert_eq!(forward, backward, "{a:?} and {b:?}");
        }
    }
}

/// spec: type-model/Types render stably
#[test]
fn identifiers_in_shape_keys_are_unquoted_and_others_quoted() {
    let t = Type::of(shape(
        vec![
            ("_ok9", Type::int(), false),
            ("1a", Type::int(), false),
            ("", Type::int(), false),
            ("a b", Type::int(), false),
        ],
        true,
    ));
    assert_eq!(
        t.to_string(),
        "array{'': int, '1a': int, _ok9: int, 'a b': int}"
    );
}

// ---- class names ----

#[test]
fn a_type_lists_every_class_name_it_mentions() {
    assert_eq!(names(&Type::int()), Vec::<String>::new());
    let generic = Atom::Named(NamedObject {
        name: Fqn::new("Box"),
        args: vec![Type::object(Fqn::new("Item"))],
    });
    assert_eq!(names(&Type::of(generic)), ["Box", "Item"]);
    let enum_case = Atom::EnumCase {
        enum_name: Fqn::new("Suit"),
        case: "Hearts".into(),
    };
    assert_eq!(names(&Type::of(enum_case)), ["Suit"]);
    assert_eq!(names(&Type::of(Atom::Static(Fqn::new("Me")))), ["Me"]);
    assert_eq!(
        names(&Type::of(Atom::ClassString(Some(Fqn::new("Bound"))))),
        ["Bound"]
    );
    assert_eq!(
        names(&Type::of(Atom::ClassString(None))),
        Vec::<String>::new()
    );
    let both = Atom::Intersection(vec![class("A"), class("B")]);
    assert_eq!(names(&Type::of(both)), ["A", "B"]);
    let arr = array(
        Type::object(Fqn::new("K")),
        Type::object(Fqn::new("V")),
        false,
    );
    assert_eq!(names(&Type::of(arr)), ["K", "V"]);
    let it = iterable(Type::object(Fqn::new("K")), Type::object(Fqn::new("V")));
    assert_eq!(names(&Type::of(it)), ["K", "V"]);
    assert_eq!(names(&Type::list_of(Type::object(Fqn::new("L")))), ["L"]);
    let entries = shape(vec![("a", Type::object(Fqn::new("S")), false)], true);
    assert_eq!(names(&Type::of(entries)), ["S"]);
    let sig = CallableSig {
        params: vec![CallableParam {
            ty: Type::object(Fqn::new("P")),
            optional: false,
            variadic: false,
        }],
        ret: Type::object(Fqn::new("R")),
    };
    assert_eq!(
        names(&Type::of(Atom::Callable(Some(Box::new(sig))))),
        ["P", "R"]
    );
}

// ---- relations between categories ----

/// spec: type-model/Types are canonical unions of atoms
#[test]
fn atoms_of_different_categories_are_disjoint_unless_a_conversion_is_possible() {
    assert_eq!(rel(Atom::Int, Atom::String), Disjoint);
    assert_eq!(rel(Atom::Null, Atom::Int), Disjoint);
    assert_eq!(rel(Atom::Int, Atom::Float), Undetermined);
    assert_eq!(rel(Atom::Float, Atom::Int), Undetermined);
    for other in [
        Atom::String,
        array(Type::int(), Type::int(), false),
        class("A"),
    ] {
        assert_eq!(rel(other.clone(), Atom::Callable(None)), Undetermined);
        assert_eq!(rel(Atom::Callable(None), other), Undetermined);
    }
    assert_eq!(rel(Atom::Int, Atom::Callable(None)), Disjoint);
    assert_eq!(rel(Atom::Resource, Atom::Callable(None)), Disjoint);
    assert_eq!(
        rel(class("A"), iterable(Type::mixed(), Type::int())),
        Undetermined
    );
    assert_eq!(
        rel(iterable(Type::mixed(), Type::int()), class("A")),
        Undetermined
    );
    assert_eq!(
        rel(
            iterable(Type::mixed(), Type::int()),
            array(Type::int(), Type::int(), false)
        ),
        Undetermined
    );
    assert_eq!(
        rel(Atom::Int, iterable(Type::mixed(), Type::int())),
        Disjoint
    );
}

#[test]
fn mixed_unknown_and_templates_are_handled_before_categories() {
    assert_eq!(rel(Atom::Int, Atom::Mixed), Subtype);
    assert_eq!(rel(Atom::Mixed, Atom::Int), Undetermined);
    assert_eq!(rel(Atom::Unknown, Atom::Int), Undetermined);
    assert_eq!(rel(Atom::Int, Atom::Unknown), Undetermined);
    assert_eq!(rel(Atom::Template("T".into()), Atom::Int), Undetermined);
    assert_eq!(rel(Atom::Int, Atom::Template("T".into())), Undetermined);
    assert_eq!(
        rel(Atom::Template("T".into()), Atom::Template("T".into())),
        Subtype
    );
    assert_eq!(Type::never().relation_to(&Type::int()), Subtype);
    let apart = Type::from_atoms([Atom::Int, Atom::Null]);
    assert_eq!(apart.relation_to(&Type::string()), Disjoint);
    assert_eq!(apart.relation_to(&Type::int()), Undetermined);
}

#[test]
fn booleans_relate_by_literal() {
    assert_eq!(rel(Atom::True, Atom::Bool), Subtype);
    assert_eq!(rel(Atom::False, Atom::Bool), Subtype);
    assert_eq!(rel(Atom::Bool, Atom::True), Undetermined);
    assert_eq!(rel(Atom::True, Atom::False), Disjoint);
    assert_eq!(rel(Atom::Bool, Atom::False), Undetermined);
}

#[test]
fn integers_relate_by_range() {
    assert_eq!(rel(Atom::IntLiteral(3), Atom::Int), Subtype);
    assert_eq!(rel(Atom::IntLiteral(3), range(Some(1), Some(5))), Subtype);
    assert_eq!(rel(Atom::IntLiteral(5), range(Some(1), Some(5))), Subtype);
    assert_eq!(rel(Atom::IntLiteral(1), range(Some(1), Some(5))), Subtype);
    assert_eq!(rel(Atom::IntLiteral(6), range(Some(1), Some(5))), Disjoint);
    assert_eq!(rel(Atom::IntLiteral(0), range(Some(1), Some(5))), Disjoint);
    assert_eq!(
        rel(range(Some(1), Some(5)), Atom::IntLiteral(3)),
        Undetermined
    );
    assert_eq!(
        rel(range(Some(1), Some(5)), range(Some(4), Some(9))),
        Undetermined
    );
    assert_eq!(rel(range(Some(10), None), range(None, Some(5))), Disjoint);
    assert_eq!(rel(range(None, Some(5)), range(Some(10), None)), Disjoint);
    assert_eq!(rel(Atom::Int, range(Some(1), None)), Undetermined);
    assert_eq!(rel(range(Some(1), None), Atom::Int), Subtype);
    assert_eq!(rel(range(Some(1), None), range(Some(0), None)), Subtype);
}

#[test]
fn strings_relate_by_kind_and_literal_content() {
    assert_eq!(rel(lit("a"), Atom::String), Subtype);
    assert_eq!(rel(Atom::NumericString, Atom::String), Subtype);
    assert_eq!(rel(Atom::String, Atom::NonEmptyString), Undetermined);
    assert_eq!(rel(lit("a"), Atom::NonEmptyString), Subtype);
    assert_eq!(rel(lit(""), Atom::NonEmptyString), Disjoint);
    assert_eq!(rel(Atom::NumericString, Atom::NonEmptyString), Subtype);
    assert_eq!(rel(Atom::ClassString(None), Atom::NonEmptyString), Subtype);
    assert_eq!(rel(Atom::NonEmptyString, Atom::NumericString), Undetermined);
    assert_eq!(rel(Atom::ClassString(None), Atom::NumericString), Disjoint);
    assert_eq!(rel(Atom::NumericString, Atom::ClassString(None)), Disjoint);
    assert_eq!(
        rel(
            Atom::ClassString(Some(Fqn::new("A"))),
            Atom::ClassString(None)
        ),
        Subtype
    );
    assert_eq!(
        rel(
            Atom::ClassString(None),
            Atom::ClassString(Some(Fqn::new("A")))
        ),
        Undetermined
    );
    assert_eq!(rel(lit("a"), lit("b")), Disjoint);
    assert_eq!(rel(lit("a"), Atom::ClassString(None)), Undetermined);
}

#[test]
fn literal_strings_are_numeric_when_they_parse_as_finite_numbers() {
    for numeric in ["12", "-1.5", "1e3", " 5 ", "0"] {
        assert_eq!(
            rel(lit(numeric), Atom::NumericString),
            Subtype,
            "{numeric:?}"
        );
    }
    for other in ["abc", "", "   ", "inf", "NaN", "1 2"] {
        assert_eq!(rel(lit(other), Atom::NumericString), Disjoint, "{other:?}");
    }
}

// ---- arrays, shapes, iterables, callables, objects ----

#[test]
fn arrays_relate_by_key_value_and_emptiness() {
    let plain = |k: Type, v: Type| array(k, v, false);
    assert_eq!(
        rel(
            plain(Type::int(), Type::int()),
            plain(Type::array_key(), Type::mixed())
        ),
        Subtype
    );
    assert_eq!(
        rel(
            plain(Type::array_key(), Type::int()),
            plain(Type::int(), Type::int())
        ),
        Undetermined
    );
    // possibly empty arrays never become disjoint on their components
    assert_eq!(
        rel(
            plain(Type::int(), Type::int()),
            plain(Type::string(), Type::int())
        ),
        Undetermined
    );
    assert_eq!(
        rel(
            array(Type::int(), Type::int(), true),
            plain(Type::string(), Type::int())
        ),
        Disjoint
    );
    assert_eq!(
        rel(
            array(Type::int(), Type::int(), true),
            array(Type::int(), Type::int(), false)
        ),
        Subtype
    );
    assert_eq!(
        rel(
            plain(Type::int(), Type::int()),
            array(Type::int(), Type::int(), true)
        ),
        Undetermined
    );
    assert_eq!(
        rel(
            array(Type::int(), Type::int(), true),
            array(Type::int(), Type::int(), true)
        ),
        Subtype
    );
}

#[test]
fn lists_relate_to_lists_and_to_arrays() {
    assert_eq!(
        rel(list(Type::int(), false), list(Type::mixed(), false)),
        Subtype
    );
    assert_eq!(
        rel(list(Type::mixed(), false), list(Type::int(), false)),
        Undetermined
    );
    assert_eq!(
        rel(list(Type::int(), true), list(Type::int(), false)),
        Subtype
    );
    assert_eq!(
        rel(list(Type::int(), false), list(Type::int(), true)),
        Undetermined
    );
    assert_eq!(
        rel(list(Type::int(), true), list(Type::int(), true)),
        Subtype
    );
    assert_eq!(
        rel(list(Type::int(), false), list(Type::string(), false)),
        Undetermined
    );
    assert_eq!(
        rel(list(Type::int(), true), list(Type::string(), true)),
        Disjoint
    );

    assert_eq!(
        rel(
            list(Type::int(), false),
            array(Type::int(), Type::int(), false)
        ),
        Subtype
    );
    assert_eq!(
        rel(
            list(Type::int(), true),
            array(Type::string(), Type::int(), false)
        ),
        Disjoint
    );
    assert_eq!(
        rel(
            list(Type::int(), false),
            array(Type::string(), Type::int(), false)
        ),
        Undetermined
    );
    assert_eq!(
        rel(
            array(Type::int(), Type::int(), false),
            list(Type::int(), false)
        ),
        Undetermined
    );
}

#[test]
fn shapes_relate_to_arrays_by_their_keys_values_and_required_entries() {
    let required = shape(vec![("a", Type::int(), false)], true);
    let optional = shape(vec![("a", Type::int(), true)], true);
    let target = |non_empty| array(Type::string(), Type::int(), non_empty);
    assert_eq!(rel(required.clone(), target(true)), Subtype);
    assert_eq!(rel(required.clone(), target(false)), Subtype);
    assert_eq!(rel(optional.clone(), target(false)), Subtype);
    assert_eq!(rel(optional, target(true)), Undetermined);
    let mixed_keys = Atom::Shape {
        entries: vec![
            ShapeEntry {
                key: ShapeKey::Int(0),
                value: Type::int(),
                optional: false,
            },
            ShapeEntry {
                key: ShapeKey::Str("k".into()),
                value: Type::int(),
                optional: false,
            },
        ],
        sealed: true,
    };
    assert_eq!(
        rel(mixed_keys, array(Type::array_key(), Type::int(), true)),
        Subtype
    );
    assert_eq!(
        rel(required, array(Type::int(), Type::int(), true)),
        Disjoint
    );
}

#[test]
fn only_sealed_sequential_shapes_are_lists() {
    let l = |non_empty| list(Type::int(), non_empty);
    assert_eq!(rel(int_shape(2, true), l(false)), Subtype);
    assert_eq!(rel(int_shape(2, true), l(true)), Subtype);
    assert_eq!(rel(int_shape(0, true), l(false)), Subtype);
    assert_eq!(rel(int_shape(0, true), l(true)), Undetermined);
    assert_eq!(rel(int_shape(2, false), l(false)), Undetermined);
    let skipping = Atom::Shape {
        entries: vec![ShapeEntry {
            key: ShapeKey::Int(1),
            value: Type::int(),
            optional: false,
        }],
        sealed: true,
    };
    assert_eq!(rel(skipping, l(false)), Undetermined);
    let optional = Atom::Shape {
        entries: vec![ShapeEntry {
            key: ShapeKey::Int(0),
            value: Type::int(),
            optional: true,
        }],
        sealed: true,
    };
    assert_eq!(rel(optional.clone(), l(false)), Undetermined);
    assert_eq!(
        rel(int_shape(2, true), list(Type::string(), false)),
        Disjoint
    );
    assert_eq!(
        rel(int_shape(2, true), list(Type::string(), true)),
        Disjoint
    );
    let reordered = Atom::Shape {
        entries: vec![
            ShapeEntry {
                key: ShapeKey::Int(0),
                value: Type::int(),
                optional: false,
            },
            ShapeEntry {
                key: ShapeKey::Str("x".into()),
                value: Type::int(),
                optional: false,
            },
        ],
        sealed: true,
    };
    assert_eq!(rel(reordered, l(false)), Undetermined);
    assert_eq!(rel(l(false), int_shape(2, true)), Undetermined);
}

#[test]
fn shapes_relate_to_shapes_by_entries_and_openness() {
    let a = |optional| ("a", Type::int(), optional);
    let b = |optional| ("b", Type::string(), optional);
    let sealed = |entries| shape(entries, true);
    let open = |entries| shape(entries, false);

    // identical, narrower value, extra key
    assert_eq!(rel(sealed(vec![a(false)]), sealed(vec![a(false)])), Subtype);
    assert_eq!(
        rel(
            sealed(vec![("a", Type::of(Atom::IntLiteral(1)), false)]),
            sealed(vec![a(false)])
        ),
        Subtype
    );
    assert_eq!(
        rel(sealed(vec![a(false), b(false)]), open(vec![a(false)])),
        Subtype
    );
    assert_eq!(
        rel(sealed(vec![a(false), b(false)]), sealed(vec![a(false)])),
        Undetermined
    );
    assert_eq!(
        rel(open(vec![a(false), b(false)]), open(vec![a(false)])),
        Subtype
    );
    // an unsealed source may carry keys a sealed target forbids
    assert_eq!(
        rel(open(vec![a(false)]), sealed(vec![a(false)])),
        Undetermined
    );
    assert_eq!(rel(open(vec![a(false)]), open(vec![a(false)])), Subtype);
}

#[test]
fn shape_entries_must_match_in_requirement_optionality_and_value() {
    let a = |optional| ("a", Type::int(), optional);
    let b = |optional| ("b", Type::string(), optional);
    let sealed = |entries| shape(entries, true);
    let open = |entries| shape(entries, false);
    // required entries
    assert_eq!(
        rel(sealed(vec![a(false)]), sealed(vec![a(false), b(false)])),
        Undetermined
    );
    assert_eq!(
        rel(sealed(vec![a(false)]), sealed(vec![a(false), b(true)])),
        Subtype
    );
    assert_eq!(
        rel(open(vec![a(false)]), open(vec![a(false), b(true)])),
        Undetermined
    );
    // optionality
    assert_eq!(
        rel(sealed(vec![a(true)]), sealed(vec![a(false)])),
        Undetermined
    );
    assert_eq!(rel(sealed(vec![a(false)]), sealed(vec![a(true)])), Subtype);
    assert_eq!(rel(sealed(vec![a(true)]), sealed(vec![a(true)])), Subtype);
    // value types
    assert_eq!(
        rel(
            sealed(vec![("a", Type::string(), false)]),
            sealed(vec![a(false)])
        ),
        Undetermined
    );
    // a shape is not related to a list or array target by shape rules
    assert_eq!(
        rel(sealed(vec![a(false)]), Atom::Callable(None)),
        Undetermined
    );
}

#[test]
fn iterables_and_arrays_relate_through_their_parameters() {
    let it = |k, v| iterable(k, v);
    assert_eq!(
        rel(
            it(Type::int(), Type::int()),
            it(Type::array_key(), Type::mixed())
        ),
        Subtype
    );
    assert_eq!(
        rel(
            it(Type::array_key(), Type::mixed()),
            it(Type::int(), Type::int())
        ),
        Undetermined
    );
    assert_eq!(
        rel(
            it(Type::int(), Type::int()),
            it(Type::int(), Type::string())
        ),
        Undetermined
    );
    assert_eq!(
        rel(
            array(Type::int(), Type::int(), false),
            it(Type::array_key(), Type::int())
        ),
        Subtype
    );
    assert_eq!(
        rel(
            array(Type::int(), Type::int(), true),
            it(Type::string(), Type::int())
        ),
        Disjoint
    );
    assert_eq!(
        rel(
            array(Type::int(), Type::int(), false),
            it(Type::string(), Type::int())
        ),
        Undetermined
    );
    assert_eq!(
        rel(list(Type::int(), false), it(Type::int(), Type::int())),
        Subtype
    );
}

#[test]
fn callables_relate_by_their_signatures_only_when_unconstrained() {
    let sig = Atom::Callable(Some(Box::new(CallableSig {
        params: vec![],
        ret: Type::int(),
    })));
    assert_eq!(rel(sig.clone(), Atom::Callable(None)), Subtype);
    assert_eq!(rel(Atom::Callable(None), sig.clone()), Undetermined);
    let other = Atom::Callable(Some(Box::new(CallableSig {
        params: vec![],
        ret: Type::string(),
    })));
    assert_eq!(rel(sig, other), Undetermined);
}

#[test]
fn objects_relate_by_name_arguments_and_enum_case() {
    assert_eq!(rel(class("A"), Atom::Object), Subtype);
    assert_eq!(rel(Atom::Object, class("A")), Undetermined);
    assert_eq!(rel(class("A"), class("A")), Subtype);
    assert_eq!(rel(class("A"), class("B")), Undetermined);
    assert_eq!(rel(Atom::Static(Fqn::new("A")), class("A")), Subtype);
    assert_eq!(rel(Atom::Static(Fqn::new("A")), class("B")), Undetermined);
    let case = |e: &str, c: &str| Atom::EnumCase {
        enum_name: Fqn::new(e),
        case: c.into(),
    };
    assert_eq!(rel(case("E", "A"), class("E")), Subtype);
    assert_eq!(rel(case("E", "A"), class("F")), Undetermined);
    assert_eq!(rel(case("E", "A"), case("E", "B")), Disjoint);
    assert_eq!(rel(case("E", "A"), case("E", "A")), Subtype);
    assert_eq!(rel(case("E", "A"), case("F", "B")), Undetermined);
    assert_eq!(rel(class("E"), case("E", "A")), Undetermined);
}

#[test]
fn generic_objects_relate_by_arguments() {
    let generic = |name: &str, args: Vec<Type>| {
        Atom::Named(NamedObject {
            name: Fqn::new(name),
            args,
        })
    };
    assert_eq!(rel(generic("L", vec![Type::int()]), class("L")), Subtype);
    assert_eq!(
        rel(
            generic("L", vec![Type::int()]),
            generic("L", vec![Type::mixed()])
        ),
        Subtype
    );
    assert_eq!(
        rel(
            generic("L", vec![Type::mixed()]),
            generic("L", vec![Type::int()])
        ),
        Undetermined
    );
    assert_eq!(
        rel(class("L"), generic("L", vec![Type::int()])),
        Undetermined
    );
    assert_eq!(
        rel(
            generic("L", vec![Type::int()]),
            generic("L", vec![Type::int(), Type::int()])
        ),
        Undetermined
    );
    assert_eq!(
        rel(
            generic("L", vec![Type::int()]),
            generic("M", vec![Type::int()])
        ),
        Undetermined
    );
}

#[test]
fn intersections_relate_part_by_part() {
    let both = Atom::Intersection(vec![class("A"), class("B")]);
    assert_eq!(rel(both.clone(), class("A")), Subtype);
    assert_eq!(rel(both.clone(), class("B")), Subtype);
    assert_eq!(rel(both.clone(), class("C")), Undetermined);
    assert_eq!(rel(class("A"), both.clone()), Undetermined);
    assert_eq!(rel(both.clone(), both.clone()), Subtype);
    let enum_pair = Atom::Intersection(vec![
        Atom::EnumCase {
            enum_name: Fqn::new("E"),
            case: "A".into(),
        },
        class("E"),
    ]);
    let other_case = Atom::EnumCase {
        enum_name: Fqn::new("E"),
        case: "B".into(),
    };
    assert_eq!(rel(other_case, enum_pair), Disjoint);
}

/// spec: type-model/Types render stably
#[test]
fn renders_parse_back_for_every_relation_fixture() {
    for t in [
        Type::of(shape(vec![("a", Type::int(), true)], false)),
        Type::of(shape(vec![], false)),
        Type::of(range(Some(1), None)),
        Type::of(range(Some(0), None)),
        Type::of(range(None, Some(-1))),
        Type::of(range(None, Some(0))),
    ] {
        let parsed = parse_type(&t.to_string(), &TypeContext::new(), 0);
        assert_eq!(parsed.ty.as_ref(), Some(&t), "{t}");
    }
}

#[test]
fn an_intersection_is_a_subtype_of_one_with_fewer_parts() {
    let wide = Atom::Intersection(vec![class("A"), class("B"), class("C")]);
    let narrow = Atom::Intersection(vec![class("A"), class("B")]);
    assert_eq!(rel(wide.clone(), narrow.clone()), Subtype);
    assert_eq!(rel(narrow, wide), Undetermined);
}

/// spec: type-model/Types are canonical unions of atoms
#[test]
fn atoms_that_differ_only_in_a_nested_widened_flag_keep_the_unwidened_one() {
    let widened = Type::from_atoms_capped([Atom::Int, Atom::Null, Atom::Float], 2);
    assert!(widened.is_widened());
    let with_widened = array(Type::int(), widened.clone(), false);
    let plain = array(Type::int(), Type::mixed(), false);
    assert_ne!(with_widened, plain);
    for order in [
        [with_widened.clone(), plain.clone()],
        [plain.clone(), with_widened.clone()],
    ] {
        assert_eq!(Type::from_atoms(order), Type::of(plain.clone()));
    }
}
