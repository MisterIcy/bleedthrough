//! Built-in data seen through the symbol model: availability and the converted summary.
#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::type_complexity,
    reason = "tests are long tables of cases and the test parser is a dispatch over PHP syntax"
)]

mod common;
use common::*;

use bleedthrough_builtins::{
    BuiltinData, CaseDetail, ClassDetail, ConstantDetail, Deprecation, Detail, ExtensionRecord,
    FunctionDetail, MethodDetail, Param, PhpVersion, PropertyDetail, Record, Signature,
    VersionMask, Visibility,
};
use bleedthrough_symbols::{Availability, Builtins, dump_summary};

fn mask(text: &str) -> VersionMask {
    VersionMask::parse(text).unwrap()
}

fn param(name: &str, ty: &str) -> Param {
    Param {
        name: name.into(),
        type_text: ty.into(),
        ..Param::default()
    }
}

fn function(name: &str, m: &str, ext: &str, params: Vec<Param>, ret: &str) -> Record {
    Record::new(
        name,
        mask(m),
        ext,
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

fn class_like(name: &str, m: &str, make: fn(ClassDetail) -> Detail, detail: ClassDetail) -> Record {
    Record::new(name, mask(m), "core", make(detail))
}

fn method(owner: &str, name: &str, detail: MethodDetail) -> Record {
    Record::new(
        format!("{owner}::{name}"),
        mask("8.0-8.5"),
        "core",
        Detail::Method(detail),
    )
}

fn data() -> BuiltinData {
    let mut records = vec![
        Record::new(
            "E_ALL",
            mask("8.0-8.5"),
            "core",
            Detail::Constant(ConstantDetail {
                type_text: "int".into(),
                ..ConstantDetail::default()
            }),
        ),
        Record::new(
            "BROKEN",
            mask("8.0-8.5"),
            "core",
            Detail::Constant(ConstantDetail {
                type_text: "<<".into(),
                ..ConstantDetail::default()
            }),
        ),
        function(
            "strlen",
            "8.0-8.5",
            "core",
            vec![param("string", "string")],
            "int",
        ),
        function("new_fn", "8.4-8.5", "core", vec![], "void"),
        function("mysql_connect", "8.0-8.1", "mysql", vec![], "mixed"),
        function("bare", "8.0-8.5", "core", vec![], ""),
        Record {
            guards: vec!["HAVE_X".into(), "HAVE_Y".into()],
            deprecation: Some(Deprecation {
                since: "8.1".into(),
                message: "gone".into(),
            }),
            ..function("old_fn", "8.0-8.5", "standard", vec![], "string")
        },
        Record::new(
            "mapper",
            mask("8.0-8.5"),
            "standard",
            Detail::Function(FunctionDetail {
                signature: Signature {
                    params: vec![
                        Param {
                            name: "callback".into(),
                            type_text: "?callable".into(),
                            optional: true,
                            ..Param::default()
                        },
                        Param {
                            name: "arr".into(),
                            type_text: "array".into(),
                            doc_type: "list<int>".into(),
                            by_ref: true,
                            ..Param::default()
                        },
                        Param {
                            name: "rest".into(),
                            variadic: true,
                            optional: true,
                            ..Param::default()
                        },
                        Param {
                            name: "withdefault".into(),
                            type_text: "int".into(),
                            default: Some("5".into()),
                            optional: true,
                            ..Param::default()
                        },
                    ],
                    return_type: "array".into(),
                    doc_return_type: "list<string>".into(),
                    return_by_ref: true,
                    ..Signature::default()
                },
                ..FunctionDetail::default()
            }),
        ),
        function("tentative", "8.0-8.5", "core", vec![], ""),
        Record::new(
            "checked",
            mask("8.0-8.5"),
            "core",
            Detail::Function(FunctionDetail {
                signature: Signature {
                    params: vec![
                        Param {
                            name: "clash".into(),
                            type_text: "int".into(),
                            doc_type: "string".into(),
                            ..Param::default()
                        },
                        Param {
                            name: "vague".into(),
                            type_text: "Exception".into(),
                            doc_type: "Other".into(),
                            ..Param::default()
                        },
                    ],
                    return_type: "void".into(),
                    ..Signature::default()
                },
                ..FunctionDetail::default()
            }),
        ),
        Record::new(
            "Dual",
            mask("8.0-8.1"),
            "legacy",
            Detail::Class(ClassDetail::default()),
        ),
        Record::new(
            "Dual",
            mask("8.4-8.5"),
            "core",
            Detail::Interface(ClassDetail::default()),
        ),
        class_like(
            "Countable",
            "8.0-8.5",
            Detail::Interface,
            ClassDetail::default(),
        ),
        class_like(
            "Stringable",
            "8.0-8.5",
            Detail::Interface,
            ClassDetail::default(),
        ),
        class_like(
            "Both",
            "8.0-8.5",
            Detail::Interface,
            ClassDetail {
                extends: vec!["Countable".into(), "Stringable".into()],
                ..ClassDetail::default()
            },
        ),
        class_like(
            "Exception",
            "8.0-8.5",
            Detail::Class,
            ClassDetail {
                implements: vec!["Stringable".into()],
                ..ClassDetail::default()
            },
        ),
        class_like(
            "Child",
            "8.0-8.5",
            Detail::Class,
            ClassDetail {
                is_abstract: true,
                is_final: false,
                extends: vec!["Exception".into(), "Ignored".into()],
                implements: vec!["Countable".into(), "Both".into()],
                ..ClassDetail::default()
            },
        ),
        class_like(
            "Sealed",
            "8.0-8.5",
            Detail::Class,
            ClassDetail {
                is_final: true,
                is_readonly: true,
                ..ClassDetail::default()
            },
        ),
        class_like("Helper", "8.0-8.5", Detail::Trait, ClassDetail::default()),
        class_like(
            "Suit",
            "8.1-8.5",
            Detail::Enum,
            ClassDetail {
                backing_type: "string".into(),
                ..ClassDetail::default()
            },
        ),
        class_like("Future", "8.5", Detail::Class, ClassDetail::default()),
        Record::new(
            "Suit::Hearts",
            mask("8.1-8.5"),
            "core",
            Detail::EnumCase(CaseDetail {
                value: Some("'h'".into()),
            }),
        ),
        Record::new(
            "Exception::CODE",
            mask("8.0-8.5"),
            "core",
            Detail::ClassConstant(ConstantDetail {
                type_text: "int".into(),
                is_final: true,
                visibility: Visibility::Protected,
                ..ConstantDetail::default()
            }),
        ),
        Record::new(
            "Exception::SECRET",
            mask("8.0-8.5"),
            "core",
            Detail::ClassConstant(ConstantDetail {
                type_text: "string".into(),
                visibility: Visibility::Private,
                ..ConstantDetail::default()
            }),
        ),
        Record::new(
            "Exception::message",
            mask("8.0-8.5"),
            "core",
            Detail::Property(PropertyDetail {
                type_text: "string".into(),
                visibility: Visibility::Protected,
                default: Some("''".into()),
                ..PropertyDetail::default()
            }),
        ),
        Record::new(
            "Exception::trace",
            mask("8.0-8.5"),
            "core",
            Detail::Property(PropertyDetail {
                type_text: "array".into(),
                doc_type: "list<int>".into(),
                visibility: Visibility::Private,
                is_static: true,
                is_readonly: true,
                ..PropertyDetail::default()
            }),
        ),
        method(
            "Exception",
            "getMessage",
            MethodDetail {
                signature: Signature {
                    return_type: "string".into(),
                    ..Signature::default()
                },
                is_final: true,
                ..MethodDetail::default()
            },
        ),
        method(
            "Exception",
            "make",
            MethodDetail {
                signature: Signature {
                    return_type: "static".into(),
                    ..Signature::default()
                },
                visibility: Visibility::Private,
                is_static: true,
                ..MethodDetail::default()
            },
        ),
        method(
            "Exception",
            "tent",
            MethodDetail {
                signature: Signature {
                    tentative_return_type: "int".into(),
                    ..Signature::default()
                },
                visibility: Visibility::Protected,
                is_abstract: true,
                ..MethodDetail::default()
            },
        ),
        method(
            "Helper",
            "me",
            MethodDetail {
                signature: Signature {
                    return_type: "self".into(),
                    ..Signature::default()
                },
                ..MethodDetail::default()
            },
        ),
        method(
            "Child",
            "count",
            MethodDetail {
                signature: Signature {
                    params: vec![param("self", "self"), param("p", "parent")],
                    return_type: "int".into(),
                    ..Signature::default()
                },
                ..MethodDetail::default()
            },
        ),
    ];
    records.sort_by(|a, b| a.name.cmp(&b.name));
    BuiltinData::new(
        vec![
            ExtensionRecord {
                name: "mysql".into(),
                unbundled: mask("8.2-8.5"),
            },
            ExtensionRecord {
                name: "legacy".into(),
                unbundled: mask("8.2-8.5"),
            },
        ],
        records,
    )
    .unwrap()
}

fn builtins() -> Builtins {
    Builtins::new(data(), PhpVersion::new(2).unwrap())
}

/// spec: builtin-symbols/Lookup is filtered by the modelled PHP version
#[test]
fn availability_distinguishes_present_absent_unbundled_and_unknown() {
    let b = builtins();
    let found = |ext: &str, guards: &[&str], deprecated: bool| Availability::Available {
        extension: ext.into(),
        guards: guards.iter().map(|g| (*g).into()).collect(),
        deprecated,
    };
    assert_eq!(b.function(&Fqn::new("strlen")), found("core", &[], false));
    assert_eq!(b.function(&Fqn::new("STRLEN")), found("core", &[], false));
    assert_eq!(b.function(&Fqn::new("\\strlen")), found("core", &[], false));
    assert_eq!(
        b.function(&Fqn::new("old_fn")),
        found("standard", &["HAVE_X", "HAVE_Y"], true)
    );
    assert_eq!(b.function(&Fqn::new("new_fn")), Availability::NotInVersion);
    assert_eq!(
        b.function(&Fqn::new("mysql_connect")),
        Availability::Unbundled {
            extension: "mysql".into()
        }
    );
    assert_eq!(b.function(&Fqn::new("nothing")), Availability::Unknown);
    assert_eq!(b.constant(&Fqn::new("E_ALL")), found("core", &[], false));
    assert_eq!(b.constant(&Fqn::new("strlen")), Availability::Unknown);
    assert_eq!(b.class(&Fqn::new("exception")), found("core", &[], false));
    assert_eq!(b.class(&Fqn::new("Countable")), found("core", &[], false));
    assert_eq!(b.class(&Fqn::new("Helper")), found("core", &[], false));
    assert_eq!(b.class(&Fqn::new("Suit")), found("core", &[], false));
    assert_eq!(b.class(&Fqn::new("Future")), Availability::NotInVersion);
    // an unbundled class-like is not overridden by a later kind that is merely absent
    assert_eq!(
        b.class(&Fqn::new("Dual")),
        Availability::Unbundled {
            extension: "legacy".into()
        }
    );
    assert_eq!(b.version(), PhpVersion::new(2).unwrap());
    assert_eq!(b.data().records().len(), data().records().len());
}

/// spec: builtin-symbols/Built-in types are converted by the project's type model
#[test]
fn the_summary_converts_every_kind_of_record() {
    let b = builtins();
    let summary = b.summary();
    assert_eq!(summary.path.as_ref(), "<builtin>");
    assert!(!summary.incomplete);
    let dump = dump_summary(&summary);
    assert_eq!(
        dump.trim_end(),
        include_str!("golden/builtins.txt").trim_end()
    );
}

/// spec: builtin-symbols/Built-in types are converted by the project's type model
#[test]
fn every_converted_declaration_carries_its_origin() {
    let summary = builtins().summary();
    let meta = |ext: &str, guards: &[&str], deprecated: bool| {
        Some(bleedthrough_symbols::BuiltinMeta {
            extension: ext.into(),
            guards: guards.iter().map(|g| (*g).into()).collect(),
            deprecated,
        })
    };
    let old = summary
        .functions
        .iter()
        .find(|f| f.name.as_str() == "old_fn")
        .unwrap();
    assert_eq!(old.builtin, meta("standard", &["HAVE_X", "HAVE_Y"], true));
    let strlen = summary
        .functions
        .iter()
        .find(|f| f.name.as_str() == "strlen")
        .unwrap();
    assert_eq!(strlen.builtin, meta("core", &[], false));
    let e_all = summary
        .constants
        .iter()
        .find(|c| c.name.as_str() == "E_ALL")
        .unwrap();
    assert_eq!(e_all.builtin, meta("core", &[], false));
    let exception = summary
        .classes
        .iter()
        .find(|c| c.name.as_str() == "Exception")
        .unwrap();
    assert_eq!(exception.builtin, meta("core", &[], false));
    assert!(
        summary
            .functions
            .iter()
            .all(|f| f.span.start == 0 && f.span.end == 0)
    );
    assert!(
        summary.diagnostics.iter().any(|d| {
            d.code == bleedthrough_symbols::SymbolDiagnosticCode::DocblockContradiction
        })
    );
}
