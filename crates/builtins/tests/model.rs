//! Lookups, construction rules and the source port, through the public API only.

#![allow(
    clippy::too_many_lines,
    reason = "fixtures that build whole records read best in one piece"
)]

use bleedthrough_builtins::{
    BuiltinData, BuiltinDataSource, ClassDetail, ConstantDetail, Deprecation, Detail,
    ExtensionRecord, FunctionDetail, InMemorySource, Kind, Lookup, MethodDetail, Param, PhpVersion,
    PropertyDetail, Record, Signature, VersionMask, Visibility, format,
};

fn v(minor: u8) -> PhpVersion {
    PhpVersion::new(minor).unwrap()
}

fn mask(first: u8, last: u8) -> VersionMask {
    VersionMask::range(v(first), v(last))
}

fn param(name: &str, type_text: &str) -> Param {
    Param {
        name: name.into(),
        type_text: type_text.into(),
        ..Param::default()
    }
}

fn function(name: &str, m: VersionMask, params: Vec<Param>, ret: &str) -> Record {
    Record::new(
        name,
        m,
        "standard",
        Detail::Function(FunctionDetail {
            signature: Signature {
                params,
                return_type: ret.into(),
                ..Signature::default()
            },
            ..FunctionDetail::default()
        }),
    )
}

/// Records exercising versions, signatures and bundling.
pub fn sample() -> BuiltinData {
    let mut sort = function("sort", mask(0, 5), vec![param("array", "array")], "bool");
    if let Detail::Function(d) = &mut sort.detail {
        d.signature.params[0].by_ref = true;
    }
    let mut query = Record::new(
        "PDO::query",
        VersionMask::ALL,
        "pdo",
        Detail::Method(MethodDetail {
            signature: Signature {
                params: vec![param("query", "string")],
                tentative_return_type: "PDOStatement|false".into(),
                ..Signature::default()
            },
            visibility: Visibility::Public,
            ..MethodDetail::default()
        }),
    );
    query.sapi = vec![];
    let records = vec![
        // Signature changed in 8.1: two records, adjacent masks.
        function(
            "str_contains_like",
            mask(0, 0),
            vec![param("a", "string")],
            "bool",
        ),
        function(
            "str_contains_like",
            mask(1, 5),
            vec![param("a", "string"), param("b", "string")],
            "bool",
        ),
        function(
            "json_validate",
            mask(3, 5),
            vec![param("json", "string")],
            "bool",
        ),
        sort,
        query,
        Record::new(
            "PDO",
            VersionMask::ALL,
            "pdo",
            Detail::Class(ClassDetail::default()),
        ),
        Record::new(
            "PDO::ATTR_ERRMODE",
            VersionMask::ALL,
            "pdo",
            Detail::ClassConstant(ConstantDetail {
                type_text: "int".into(),
                ..ConstantDetail::default()
            }),
        ),
        Record::new(
            "PDO::errorCode",
            mask(0, 2).union(mask(4, 5)),
            "pdo",
            Detail::Method(MethodDetail::default()),
        ),
        Record::new(
            "imap_open",
            mask(0, 3),
            "imap",
            Detail::Function(FunctionDetail::default()),
        ),
        Record::new(
            "E_ALL",
            VersionMask::ALL,
            "core",
            Detail::Constant(ConstantDetail {
                value: Some("32767".into()),
                ..ConstantDetail::default()
            }),
        ),
    ];
    let extensions = vec![ExtensionRecord {
        name: "imap".into(),
        unbundled: mask(4, 5),
    }];
    BuiltinData::new(extensions, records).unwrap()
}

#[test]
fn a_changed_signature_has_one_record_per_form() {
    let data = sample();
    let before = data.lookup(Kind::Function, "STR_CONTAINS_LIKE", v(0));
    let after = data.lookup(Kind::Function, "str_contains_like", v(3));
    let (Lookup::Found(a), Lookup::Found(b)) = (before, after) else {
        panic!("both versions are found");
    };
    assert_eq!(a.mask, mask(0, 0));
    assert_eq!(b.mask, mask(1, 5));
    let Detail::Function(old) = &a.detail else {
        panic!()
    };
    let Detail::Function(new) = &b.detail else {
        panic!()
    };
    assert_eq!(old.signature.params.len(), 1);
    assert_eq!(new.signature.params.len(), 2);
    assert_eq!(data.symbol(Kind::Function, "str_contains_like").len(), 2);
}

#[test]
fn not_available_is_not_unknown() {
    let data = sample();
    assert_eq!(
        data.lookup(Kind::Function, "json_validate", v(2)),
        Lookup::NotInVersion
    );
    assert!(matches!(
        data.lookup(Kind::Function, "json_validate", v(3)),
        Lookup::Found(_)
    ));
    assert_eq!(
        data.lookup(Kind::Function, "no_such_function", v(3)),
        Lookup::Unknown
    );
}

#[test]
fn unbundled_is_not_removed() {
    let data = sample();
    assert!(matches!(
        data.lookup(Kind::Function, "imap_open", v(3)),
        Lookup::Found(_)
    ));
    assert_eq!(
        data.lookup(Kind::Function, "imap_open", v(4)),
        Lookup::Unbundled
    );
    assert_eq!(
        data.lookup(Kind::Function, "imap_open", v(5)),
        Lookup::Unbundled
    );
}

#[test]
fn masks_may_be_non_contiguous() {
    let data = sample();
    assert!(matches!(
        data.lookup(Kind::Method, "pdo::ERRORCODE", v(2)),
        Lookup::Found(_)
    ));
    assert_eq!(
        data.lookup(Kind::Method, "PDO::errorCode", v(3)),
        Lookup::NotInVersion
    );
    assert!(matches!(
        data.lookup(Kind::Method, "PDO::errorCode", v(4)),
        Lookup::Found(_)
    ));
}

#[test]
fn case_rules_follow_php() {
    let data = sample();
    assert!(matches!(
        data.lookup(Kind::Class, "\\pdo", v(0)),
        Lookup::Found(_)
    ));
    assert!(matches!(
        data.lookup(Kind::Constant, "E_ALL", v(0)),
        Lookup::Found(_)
    ));
    assert_eq!(data.lookup(Kind::Constant, "e_all", v(0)), Lookup::Unknown);
    assert!(matches!(
        data.lookup(Kind::ClassConstant, "pdo::ATTR_ERRMODE", v(0)),
        Lookup::Found(_)
    ));
    assert_eq!(
        data.lookup(Kind::ClassConstant, "PDO::attr_errmode", v(0)),
        Lookup::Unknown
    );
}

#[test]
fn members_belong_to_their_class() {
    let data = sample();
    let names: Vec<&str> = data.members("PDO", v(3)).map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["PDO::ATTR_ERRMODE", "PDO::query"]);
    let names: Vec<&str> = data.members("pdo", v(4)).map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["PDO::ATTR_ERRMODE", "PDO::errorCode", "PDO::query"]);
}

#[test]
fn tentative_and_by_reference_are_kept() {
    let data = sample();
    let Lookup::Found(query) = data.lookup(Kind::Method, "PDO::query", v(4)) else {
        panic!()
    };
    let Detail::Method(m) = &query.detail else {
        panic!()
    };
    assert_eq!(m.signature.tentative_return_type, "PDOStatement|false");
    assert_eq!(m.signature.return_type, "");
    let Lookup::Found(sort) = data.lookup(Kind::Function, "sort", v(0)) else {
        panic!()
    };
    let Detail::Function(f) = &sort.detail else {
        panic!()
    };
    assert!(f.signature.params[0].by_ref);
}

#[test]
fn overlapping_masks_are_rejected_naming_the_symbol() {
    let records = vec![
        function("dup", mask(0, 2), vec![], "int"),
        function("DUP", mask(2, 5), vec![], "int"),
    ];
    let error = BuiltinData::new(vec![], records).unwrap_err();
    assert!(
        error.to_string().contains("DUP") || error.to_string().contains("dup"),
        "{error}"
    );
    assert!(error.problem.contains("overlap"));
}

#[test]
fn construction_rejects_bad_fields() {
    let mut r = function("f", VersionMask::ALL, vec![], "int");
    r.mask = VersionMask::EMPTY;
    assert!(BuiltinData::new(vec![], vec![r]).is_err());

    let both = Record::new(
        "f",
        VersionMask::ALL,
        "standard",
        Detail::Function(FunctionDetail {
            signature: Signature {
                return_type: "int".into(),
                tentative_return_type: "int".into(),
                ..Signature::default()
            },
            ..FunctionDetail::default()
        }),
    );
    assert!(BuiltinData::new(vec![], vec![both]).is_err());

    let prose = function("f", VersionMask::ALL, vec![], "the number of items");
    assert!(BuiltinData::new(vec![], vec![prose]).is_err());

    let mut reflected = function("f", mask(0, 1), vec![], "int");
    reflected.reflected = mask(0, 2);
    assert!(BuiltinData::new(vec![], vec![reflected]).is_err());

    let twice = vec![
        ExtensionRecord {
            name: "imap".into(),
            unbundled: mask(4, 5),
        },
        ExtensionRecord {
            name: "imap".into(),
            unbundled: mask(5, 5),
        },
    ];
    assert!(BuiltinData::new(twice, vec![]).is_err());

    let mut bad_default = param("p", "int");
    bad_default.default = Some("0".into());
    let r = function("f", VersionMask::ALL, vec![bad_default], "int");
    assert!(
        BuiltinData::new(vec![], vec![r]).is_err(),
        "a default needs the optional flag"
    );
}

#[test]
fn deprecation_and_provenance_are_recorded() {
    let mut constant = Record::new(
        "E_STRICT",
        mask(0, 5),
        "core",
        Detail::Constant(ConstantDetail::default()),
    );
    constant.deprecation = Some(Deprecation {
        since: "8.4".into(),
        message: "the error level was removed".into(),
    });
    constant.reflected = mask(0, 1);
    let data = BuiltinData::new(vec![], vec![constant]).unwrap();
    let Lookup::Found(found) = data.lookup(Kind::Constant, "E_STRICT", v(0)) else {
        panic!()
    };
    assert_eq!(found.reflected, mask(0, 1));
    assert_eq!(found.deprecation.as_ref().unwrap().since, "8.4");
    let text = format::write(&data);
    assert_eq!(format::write(&format::read(&text).unwrap()), text);
}

#[test]
fn property_records_hold_their_modifiers() {
    let record = Record::new(
        "Exception::message",
        VersionMask::ALL,
        "core",
        Detail::Property(PropertyDetail {
            type_text: "string".into(),
            visibility: Visibility::Protected,
            ..PropertyDetail::default()
        }),
    );
    let data = BuiltinData::new(vec![], vec![record]).unwrap();
    assert!(matches!(
        data.lookup(Kind::Property, "exception::message", v(2)),
        Lookup::Found(_)
    ));
}

/// A consumer that knows only the port and the model.
fn count_functions(source: &dyn BuiltinDataSource, version: PhpVersion) -> usize {
    let data = source.load().unwrap();
    data.records()
        .iter()
        .filter(|r| r.kind() == Kind::Function && r.mask.contains(version))
        .count()
}

#[test]
fn a_consumer_works_with_an_in_memory_source_and_no_file() {
    let source = InMemorySource::new(sample());
    assert_eq!(count_functions(&source, v(0)), 3);
    assert_eq!(count_functions(&source, v(5)), 3);
}

#[test]
fn the_source_can_be_built_from_text() {
    let text = format::write(&sample());
    let source = InMemorySource::from_text(&text).unwrap();
    assert_eq!(source.load().unwrap(), sample());
    assert!(InMemorySource::from_text("nonsense\n").is_err());
}

#[test]
fn loading_twice_gives_identical_iteration() {
    let source = InMemorySource::new(sample());
    let a = source.load().unwrap();
    let b = source.load().unwrap();
    assert_eq!(a, b);
    let names = |d: &BuiltinData| {
        d.records()
            .iter()
            .map(|r| (r.kind(), r.name.clone(), r.mask))
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&a), names(&b));
}

#[test]
fn input_order_does_not_change_the_data() {
    let mut records: Vec<Record> = sample().records().to_vec();
    let forward = BuiltinData::new(sample().extensions().to_vec(), records.clone()).unwrap();
    records.reverse();
    let backward = BuiltinData::new(sample().extensions().to_vec(), records).unwrap();
    assert_eq!(forward, backward);
    assert_eq!(format::write(&forward), format::write(&backward));
}
