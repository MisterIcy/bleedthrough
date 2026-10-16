//! Lookups over the generated data (`stubs/builtin.data`) for well-known symbols, checked against
//! the PHP manual pages cited next to each assertion.

#![allow(missing_docs, reason = "test fixtures need no documentation")]

use std::sync::OnceLock;

use bleedthrough_builtins::format;
use bleedthrough_builtins::{BuiltinData, Detail, Kind, Lookup, PhpVersion};

fn data() -> &'static BuiltinData {
    static DATA: OnceLock<BuiltinData> = OnceLock::new();
    DATA.get_or_init(|| {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../stubs/builtin.data");
        let text = std::fs::read_to_string(path).expect("stubs/builtin.data exists");
        format::read(&text).expect("the generated data reads back")
    })
}

fn v(text: &str) -> PhpVersion {
    PhpVersion::parse(text).unwrap()
}

fn found(kind: Kind, name: &str, version: &str) -> &'static bleedthrough_builtins::Record {
    match data().lookup(kind, name, v(version)) {
        Lookup::Found(record) => record,
        other => panic!("{name} in {version}: {other:?}"),
    }
}

#[test]
fn the_generated_data_round_trips_byte_for_byte() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../stubs/builtin.data");
    let text = std::fs::read_to_string(path).unwrap();
    assert_eq!(format::write(data()), text);
}

#[test]
fn strlen_exists_everywhere_with_one_string_parameter() {
    // https://www.php.net/manual/en/function.strlen.php : strlen(string $string): int
    for version in ["8.0", "8.3", "8.5"] {
        let Detail::Function(f) = &found(Kind::Function, "strlen", version).detail else {
            panic!()
        };
        assert_eq!(f.signature.params.len(), 1);
        assert_eq!(f.signature.params[0].name, "string");
        assert_eq!(f.signature.params[0].type_text, "string");
        assert_eq!(f.signature.return_type, "int");
    }
    // Names are case-insensitive for functions.
    assert!(matches!(
        data().lookup(Kind::Function, "STRLEN", v("8.0")),
        Lookup::Found(_)
    ));
}

#[test]
fn pdo_query_has_a_tentative_return_type_from_8_1() {
    // https://www.php.net/manual/en/pdo.query.php :
    // PDO::query(string $query, ?int $fetchMode = null, mixed ...$fetchModeArgs): PDOStatement|false
    let Detail::Method(m) = &found(Kind::Method, "PDO::query", "8.0").detail else {
        panic!()
    };
    assert_eq!(m.signature.return_type, "");
    assert_eq!(m.signature.doc_return_type, "PDOStatement|false");
    let Detail::Method(m) = &found(Kind::Method, "PDO::query", "8.2").detail else {
        panic!()
    };
    assert_eq!(m.signature.return_type, "");
    assert_eq!(m.signature.tentative_return_type, "PDOStatement|false");
    assert_eq!(m.signature.params[0].name, "query");
    assert!(m.signature.params[2].variadic);
}

#[test]
fn exception_constructor_has_three_optional_parameters() {
    // https://www.php.net/manual/en/exception.construct.php :
    // Exception::__construct(string $message = "", int $code = 0, ?Throwable $previous = null)
    let Detail::Method(m) = &found(Kind::Method, "Exception::__construct", "8.4").detail else {
        panic!()
    };
    let names: Vec<&str> = m.signature.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["message", "code", "previous"]);
    assert!(m.signature.params.iter().all(|p| p.optional));
    assert_eq!(m.signature.params[2].type_text, "?Throwable");
}

#[test]
fn json_validate_was_added_in_8_3() {
    // https://www.php.net/manual/en/function.json-validate.php : "Version 8.3.0: Added"
    assert_eq!(
        data().lookup(Kind::Function, "json_validate", v("8.2")),
        Lookup::NotInVersion
    );
    assert_eq!(
        found(Kind::Function, "json_validate", "8.3")
            .mask
            .to_string(),
        "8.3-8.5"
    );
}

#[test]
fn array_find_and_array_first_are_new_in_8_4_and_8_5() {
    // https://www.php.net/manual/en/function.array-find.php (8.4) and
    // https://www.php.net/manual/en/function.array-first.php (8.5)
    assert_eq!(
        data().lookup(Kind::Function, "array_find", v("8.3")),
        Lookup::NotInVersion
    );
    assert!(matches!(
        data().lookup(Kind::Function, "array_find", v("8.4")),
        Lookup::Found(_)
    ));
    assert_eq!(
        data().lookup(Kind::Function, "array_first", v("8.4")),
        Lookup::NotInVersion
    );
    assert!(matches!(
        data().lookup(Kind::Function, "array_first", v("8.5")),
        Lookup::Found(_)
    ));
}

#[test]
fn utf8_encode_is_deprecated_from_8_2() {
    // https://www.php.net/manual/en/function.utf8-encode.php : "Version 8.2.0: This function has
    // been deprecated."
    assert!(
        found(Kind::Function, "utf8_encode", "8.1")
            .deprecation
            .is_none()
    );
    let deprecation = found(Kind::Function, "utf8_encode", "8.3")
        .deprecation
        .as_ref()
        .unwrap();
    assert_eq!(deprecation.since, "");
    let later = found(Kind::Function, "utf8_encode", "8.4")
        .deprecation
        .as_ref()
        .unwrap();
    assert_eq!(later.since, "8.2");
}

#[test]
fn imap_is_unbundled_from_8_4_and_not_unknown() {
    // https://www.php.net/manual/en/migration84.removed-extensions.php
    assert!(matches!(
        data().lookup(Kind::Function, "imap_open", v("8.3")),
        Lookup::Found(_)
    ));
    assert_eq!(
        data().lookup(Kind::Function, "imap_open", v("8.4")),
        Lookup::Unbundled
    );
    assert_eq!(
        data().lookup(Kind::Function, "no_such_function_anywhere", v("8.4")),
        Lookup::Unknown
    );
}

#[test]
fn imap_open_gained_a_class_return_type_in_8_1() {
    // https://www.php.net/manual/en/function.imap-open.php : "8.1.0: Returns an IMAP\Connection
    // instance now; previously, a resource was returned."
    let Detail::Function(f) = &found(Kind::Function, "imap_open", "8.0").detail else {
        panic!()
    };
    assert_eq!(f.signature.doc_return_type, "resource|false");
    let Detail::Function(f) = &found(Kind::Function, "imap_open", "8.1").detail else {
        panic!()
    };
    assert_eq!(f.signature.return_type, "IMAP\\Connection|false");
}

#[test]
fn members_of_a_class_depend_on_the_version() {
    // https://www.php.net/manual/en/class.datetimeinterface.php lists ATOM among the constants.
    let names: Vec<&str> = data()
        .members("DateTimeInterface", v("8.2"))
        .map(|r| r.name.as_str())
        .collect();
    assert!(names.contains(&"DateTimeInterface::ATOM"));
    assert!(names.contains(&"DateTimeInterface::format"));
}

#[test]
fn constants_carry_reflection_values_and_the_mark() {
    // https://www.php.net/manual/en/errorfunc.constants.php : E_ALL is 32767 before 8.4, 30719 from 8.4.
    let value = |version: &str| match &found(Kind::Constant, "E_ALL", version).detail {
        Detail::Constant(d) => d.value.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(value("8.0").as_deref(), Some("32767"));
    assert_eq!(value("8.4").as_deref(), Some("30719"));
    // The 8.0 stubs declare no constants: the record exists only through the fixture.
    assert!(
        found(Kind::Constant, "E_ALL", "8.0")
            .reflected
            .contains(v("8.0"))
    );
    // A value that depends on the build is left empty.
    match &found(Kind::Constant, "PHP_OS", "8.0").detail {
        Detail::Constant(d) => assert_eq!(d.value, None),
        other => panic!("{other:?}"),
    }
}
