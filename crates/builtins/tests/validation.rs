//! What a record may hold, the version spelling and the reading of the data file, through the
//! public API only. Written against the surviving mutants of `cargo xtask mutants`.

use bleedthrough_builtins::{
    BuiltinData, ClassDetail, ConstantDetail, Detail, FunctionDetail, Kind, MethodDetail, Param,
    PhpVersion, Record, Signature, VersionMask, check_record, format,
};

fn function_with(name: &str, signature: Signature, alias_of: Option<&str>) -> Record {
    Record::new(
        name,
        VersionMask::ALL,
        "standard",
        Detail::Function(FunctionDetail {
            signature,
            alias_of: alias_of.map(str::to_string),
            ..FunctionDetail::default()
        }),
    )
}

fn plain(name: &str) -> Record {
    function_with(name, Signature::default(), None)
}

fn returning(ret: &str) -> Signature {
    Signature {
        return_type: ret.into(),
        ..Signature::default()
    }
}

fn method(name: &str, detail: MethodDetail) -> Record {
    Record::new(name, VersionMask::ALL, "core", Detail::Method(detail))
}

fn class(name: &str, detail: ClassDetail) -> Record {
    Record::new(name, VersionMask::ALL, "core", Detail::Class(detail))
}

fn rejected(record: &Record, needle: &str) {
    let error = check_record(record).expect_err("the record should be rejected");
    assert!(
        error.contains(needle),
        "`{error}` should mention `{needle}`"
    );
}

/// spec: builtin-data-model/Records cover every kind of built-in symbol
#[test]
fn symbol_names_must_be_identifiers() {
    for ok in ["strlen", "_x", "A\\b", "é", "a1"] {
        assert_eq!(check_record(&plain(ok)), Ok(()), "{ok}");
    }
    for bad in ["1abc", "-a", "a b", "A\\\\b", "", "a\\", "\\a"] {
        assert!(check_record(&plain(bad)).is_err(), "{bad}");
    }
}

#[test]
fn a_member_name_needs_a_valid_class_and_a_valid_member() {
    let ok = method("Foo::bar", MethodDetail::default());
    assert_eq!(check_record(&ok), Ok(()));
    for bad in ["Foo::1x", "1Foo::bar", "Foo::", "::bar", "Foo"] {
        let record = method(bad, MethodDetail::default());
        assert!(check_record(&record).is_err(), "{bad}");
    }
}

#[test]
fn an_alias_target_must_be_a_symbol_reference() {
    for ok in ["other", "A\\other", "Foo::bar"] {
        let record = function_with("f", Signature::default(), Some(ok));
        assert_eq!(check_record(&record), Ok(()), "{ok}");
    }
    for bad in ["not valid!", "Foo::1x", "1Foo::bar", "", "a b"] {
        let record = function_with("f", Signature::default(), Some(bad));
        rejected(&record, "symbol reference");
    }
}

#[test]
fn methods_are_checked_like_functions_and_cannot_be_final_and_abstract() {
    let bad_alias = method(
        "Foo::bar",
        MethodDetail {
            alias_of: Some("not valid!".into()),
            ..MethodDetail::default()
        },
    );
    rejected(&bad_alias, "symbol reference");

    let bad_type = method(
        "Foo::bar",
        MethodDetail {
            signature: returning("the number of bytes"),
            ..MethodDetail::default()
        },
    );
    rejected(&bad_type, "prose");

    let both = method(
        "Foo::bar",
        MethodDetail {
            is_final: true,
            is_abstract: true,
            ..MethodDetail::default()
        },
    );
    rejected(&both, "final and abstract");
}

#[test]
fn classes_are_checked_for_parents_interfaces_and_backing_type() {
    let ok = class(
        "Foo",
        ClassDetail {
            extends: vec!["Base".into()],
            implements: vec!["A\\B".into()],
            ..ClassDetail::default()
        },
    );
    assert_eq!(check_record(&ok), Ok(()));

    let bad_parent = class(
        "Foo",
        ClassDetail {
            extends: vec!["1x".into()],
            ..ClassDetail::default()
        },
    );
    rejected(&bad_parent, "symbol reference");
    let bad_interface = class(
        "Foo",
        ClassDetail {
            implements: vec!["ok".into(), "not valid!".into()],
            ..ClassDetail::default()
        },
    );
    rejected(&bad_interface, "symbol reference");
    let bad_backing = class(
        "Foo",
        ClassDetail {
            backing_type: "the backing".into(),
            ..ClassDetail::default()
        },
    );
    rejected(&bad_backing, "prose");
}

#[test]
fn extension_and_sapi_names_are_lower_case_words() {
    for ok in ["standard", "zend-opcache", "mysql_i", "pdo2"] {
        let mut record = plain("f");
        record.extension = ok.into();
        assert_eq!(check_record(&record), Ok(()), "{ok}");
    }
    for bad in ["", "Standard", "a b", "a!b", "é"] {
        let mut record = plain("f");
        record.extension = bad.into();
        rejected(&record, "extension name");
    }
    let mut record = plain("f");
    record.sapi = vec!["cli".into(), "Fpm".into()];
    rejected(&record, "SAPI name");
}

#[test]
fn sapis_and_guards_are_sorted_and_distinct() {
    let mut record = plain("f");
    record.sapi = vec!["cli".into(), "fpm".into()];
    record.guards = vec!["A".into(), "B".into()];
    assert_eq!(check_record(&record), Ok(()));

    record.sapi = vec!["cli".into(), "cli".into()];
    rejected(&record, "SAPIs must be sorted and distinct");
    record.sapi = vec!["fpm".into(), "cli".into()];
    rejected(&record, "SAPIs must be sorted and distinct");

    record.sapi = vec![];
    record.guards = vec!["A".into(), "A".into()];
    rejected(&record, "guard alternatives must be sorted and distinct");
    record.guards = vec!["B".into(), "A".into()];
    rejected(&record, "guard alternatives must be sorted and distinct");
}

#[test]
fn a_deprecation_version_is_major_dot_minor_or_unknown() {
    let with = |since: &str| {
        let mut record = plain("f");
        record.deprecation = Some(bleedthrough_builtins::Deprecation {
            since: since.into(),
            message: String::new(),
        });
        record
    };
    for ok in ["", "8.4", "8.10"] {
        assert_eq!(check_record(&with(ok)), Ok(()), "{ok:?}");
    }
    for bad in ["8", "8.", ".4", "8.x", "x.4", "8.4.1", "8.-4"] {
        rejected(&with(bad), "deprecation version");
    }
}

#[test]
fn field_text_has_a_length_limit_that_is_inclusive() {
    let at_limit = "a".repeat(512);
    let over = "a".repeat(513);
    assert_eq!(check_record(&plain("f").with_return(&at_limit)), Ok(()));
    rejected(&plain("f").with_return(&over), "longer than 512");
}

trait WithReturn {
    fn with_return(self, ret: &str) -> Self;
}

impl WithReturn for Record {
    fn with_return(mut self, ret: &str) -> Self {
        if let Detail::Function(d) = &mut self.detail {
            d.signature.return_type = ret.into();
        }
        self
    }
}

#[test]
fn a_word_after_a_type_prefix_is_still_adjacent_to_the_next_word() {
    for bad in ["?int float", "\\Foo bar", "int float"] {
        assert!(
            check_record(&plain("f").with_return(bad)).is_err(),
            "{bad:?}"
        );
    }
}

#[test]
fn a_parameter_default_needs_the_optional_flag_and_a_valid_expression() {
    let param = |optional: bool, default: &str| Param {
        name: "p".into(),
        type_text: "int".into(),
        optional,
        default: Some(default.into()),
        ..Param::default()
    };
    let with = |p: Param| {
        function_with(
            "f",
            Signature {
                params: vec![p],
                ..Signature::default()
            },
            None,
        )
    };
    assert_eq!(check_record(&with(param(true, "-1"))), Ok(()));
    assert_eq!(check_record(&with(param(true, "$a $b"))), Ok(()));
    assert_eq!(check_record(&with(param(true, "-A"))), Ok(()));
    rejected(&with(param(false, "0")), "not optional");
    rejected(&with(param(true, "the default")), "prose");
    // a `$`- or `-`-prefixed token is not a word, so a word after it is not adjacent to a word
    assert_eq!(check_record(&with(param(true, "$a b"))), Ok(()));
    assert_eq!(check_record(&with(param(true, "-A b"))), Ok(()));
    // a trailing `-` or `.` stays part of its word
    rejected(&with(param(true, "a- b")), "prose");
    rejected(&with(param(true, "a. b")), "prose");
    let bad_name = Param {
        name: "1p".into(),
        ..Param::default()
    };
    rejected(&with(bad_name), "parameter name");
}

#[test]
fn constants_with_values_are_checked_as_expressions() {
    let constant = |value: &str| {
        Record::new(
            "C",
            VersionMask::ALL,
            "core",
            Detail::Constant(ConstantDetail {
                value: Some(value.into()),
                ..ConstantDetail::default()
            }),
        )
    };
    assert_eq!(check_record(&constant("E_ALL & ~E_NOTICE")), Ok(()));
    rejected(&constant("the value of it"), "prose");
}

#[test]
fn class_likes_and_members_are_told_apart() {
    let class_like: Vec<_> = Kind::ALL
        .into_iter()
        .filter(|k| k.is_class_like())
        .collect();
    assert_eq!(
        class_like,
        [Kind::Class, Kind::Interface, Kind::Trait, Kind::Enum]
    );
}

/// spec: builtin-data-model/Version availability is a mask, and signatures are per distinct form
#[test]
fn version_spelling_is_8_dot_a_minor_number() {
    assert_eq!(PhpVersion::parse("8.0").map(PhpVersion::minor), Some(0));
    assert_eq!(PhpVersion::parse("8.3").map(PhpVersion::minor), Some(3));
    assert_eq!(PhpVersion::parse("8.5").map(PhpVersion::minor), Some(5));
    assert_eq!(PhpVersion::parse("8.05").map(PhpVersion::minor), Some(5));
    for bad in [
        "8.", "8", "8.x", "8.+1", "8.005", "8.1000", "8.6", "7.4", "8.-1", "",
    ] {
        assert_eq!(PhpVersion::parse(bad), None, "{bad:?}");
    }
    for v in PhpVersion::all() {
        assert_eq!(PhpVersion::parse(&v.to_string()), Some(v));
    }
}

/// spec: builtin-data-model/The text format is canonical and round-trips
#[test]
fn a_trait_survives_a_round_trip_through_the_text_format() {
    let record = Record::new(
        "Stringable",
        VersionMask::ALL,
        "core",
        Detail::Trait(ClassDetail::default()),
    );
    let data = BuiltinData::new(vec![], vec![record]).unwrap();
    let back = format::read(&format::write(&data)).unwrap();
    assert_eq!(back, data);
    assert!(matches!(
        back.records().first().map(|r| &r.detail),
        Some(Detail::Trait(_))
    ));
}

#[test]
fn the_header_error_distinguishes_another_version_from_a_foreign_file() {
    let other_version = format::read("bleedthrough-builtins 99\n").unwrap_err();
    assert_eq!(other_version.line, 1);
    assert!(other_version.message.contains("unsupported format"));
    let foreign = format::read("something else\n").unwrap_err();
    assert_eq!(foreign.line, 1);
    assert!(foreign.message.contains("expected the header"));
}

#[test]
fn escapes_need_exactly_two_hex_digits() {
    let line = |name: &str| format!("{}\nextension\t{name}\t8.0\n", format::HEADER);
    for bad in ["a\\x+1", "a\\x1", "a\\xzz", "a\\q"] {
        let error = format::read(&line(bad)).unwrap_err();
        assert!(error.message.contains("bad escape"), "{bad}: {error:?}");
    }
}
