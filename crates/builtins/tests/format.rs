//! The text format: canonical writing, strict reading and round trips.

#![allow(
    clippy::too_many_lines,
    reason = "fixtures that build whole records read best in one piece"
)]

use bleedthrough_builtins::{
    BuiltinData, ClassDetail, ConstantDetail, Deprecation, Detail, ExtensionRecord, FunctionDetail,
    MethodDetail, Param, PhpVersion, PropertyDetail, Record, Signature, VersionMask, Visibility,
    format,
};

fn v(minor: u8) -> PhpVersion {
    PhpVersion::new(minor).unwrap()
}

fn rich() -> BuiltinData {
    let mut curl = Record::new(
        "curl_exec",
        VersionMask::ALL,
        "curl",
        Detail::Function(FunctionDetail {
            signature: Signature {
                params: vec![
                    Param {
                        name: "handle".into(),
                        type_text: "CurlHandle".into(),
                        ..Param::default()
                    },
                    Param {
                        name: "flags".into(),
                        type_text: "int".into(),
                        doc_type: "int-mask<1, 2>".into(),
                        optional: true,
                        default: Some("E_ALL & ~E_NOTICE".into()),
                        ..Param::default()
                    },
                    Param {
                        name: "rest".into(),
                        type_text: "mixed".into(),
                        optional: true,
                        variadic: true,
                        by_ref: true,
                        sensitive: true,
                        ..Param::default()
                    },
                    Param {
                        name: "sep".into(),
                        type_text: "string".into(),
                        optional: true,
                        default: Some("';\\t@:'".into()),
                        ..Param::default()
                    },
                ],
                return_type: "string|bool".into(),
                doc_return_type: "non-empty-string|false".into(),
                return_by_ref: true,
                ..Signature::default()
            },
            alias_of: Some("curl_other".into()),
            no_discard: true,
        }),
    );
    curl.sapi = vec!["cli".into(), "fpm".into()];
    curl.guards = vec!["HAVE_A".into(), "defined(B) && !defined(C)".into()];
    curl.deprecation = Some(Deprecation {
        since: "8.4".into(),
        message: "use other: it is better".into(),
    });
    curl.reflected = VersionMask::range(v(0), v(1));

    let class = Record::new(
        "ArrayObject",
        VersionMask::ALL,
        "spl",
        Detail::Class(ClassDetail {
            is_final: true,
            extends: vec!["Base".into()],
            implements: vec!["IteratorAggregate".into(), "Countable".into()],
            ..ClassDetail::default()
        }),
    );
    let en = Record::new(
        "Suit",
        VersionMask::range(v(1), v(5)),
        "core",
        Detail::Enum(ClassDetail {
            backing_type: "string".into(),
            ..ClassDetail::default()
        }),
    );
    let case = Record::new(
        "Suit::Hearts",
        VersionMask::range(v(1), v(5)),
        "core",
        Detail::EnumCase(bleedthrough_builtins::CaseDetail {
            value: Some("'H'".into()),
        }),
    );
    let prop = Record::new(
        "Exception::code",
        VersionMask::ALL,
        "core",
        Detail::Property(PropertyDetail {
            type_text: "int".into(),
            default: Some("0".into()),
            visibility: Visibility::Protected,
            is_readonly: true,
            ..PropertyDetail::default()
        }),
    );
    let method = Record::new(
        "ArrayObject::__construct",
        VersionMask::of(v(0)).union(VersionMask::range(v(2), v(5))),
        "spl",
        Detail::Method(MethodDetail {
            signature: Signature {
                params: vec![],
                tentative_return_type: "void".into(),
                ..Signature::default()
            },
            is_static: true,
            is_final: true,
            ..MethodDetail::default()
        }),
    );
    let constant = Record::new(
        "PHP_EOL",
        VersionMask::ALL,
        "core",
        Detail::Constant(ConstantDetail {
            type_text: "string".into(),
            value: Some("\"\\n\"".into()),
            ..ConstantDetail::default()
        }),
    );
    let cc = Record::new(
        "ArrayObject::STD_PROP_LIST",
        VersionMask::ALL,
        "spl",
        Detail::ClassConstant(ConstantDetail {
            is_final: true,
            value: Some("1".into()),
            ..ConstantDetail::default()
        }),
    );
    BuiltinData::new(
        vec![
            ExtensionRecord {
                name: "imap".into(),
                unbundled: VersionMask::range(v(4), v(5)),
            },
            ExtensionRecord {
                name: "curl".into(),
                unbundled: VersionMask::EMPTY,
            },
        ],
        vec![curl, class, en, case, prop, method, constant, cc],
    )
    .unwrap()
}

#[test]
fn round_trip_is_byte_identical() {
    let data = rich();
    let text = format::write(&data);
    let again = format::read(&text).expect("the writer's output reads");
    assert_eq!(again, data);
    assert_eq!(format::write(&again), text);
}

#[test]
fn the_empty_data_round_trips() {
    let text = format::write(&BuiltinData::empty());
    assert_eq!(text, "bleedthrough-builtins 1\n");
    assert_eq!(format::read(&text).unwrap(), BuiltinData::empty());
}

#[test]
fn the_layout_is_stable() {
    let text = format::write(&rich());
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "bleedthrough-builtins 1");
    assert_eq!(lines[1], "extension\tcurl\t");
    assert_eq!(lines[2], "extension\timap\t8.4-8.5");
    assert!(
        lines[3].starts_with("const\tPHP_EOL\t8.0-8.5\t\tcore\t\t\t\tstring\t"),
        "{}",
        lines[3]
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("method\tArrayObject::__construct\t8.0,8.2-8.5\t"))
    );
    // Special characters in a default are escaped and the delimiters of the context too.
    assert!(text.contains("sep@o@string@@'\\x3b\\\\t\\x40:'"), "{text}");
}

fn mutated(line_pattern: &str, from: &str, to: &str) -> String {
    let text = format::write(&rich());
    text.lines()
        .map(|l| {
            if l.contains(line_pattern) {
                l.replacen(from, to, 1)
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn error_of(text: &str) -> format::FormatError {
    format::read(text).expect_err("the text must be rejected")
}

#[test]
fn rejects_unsupported_version_and_missing_header() {
    let e = error_of("bleedthrough-builtins 2\n");
    assert_eq!((e.line, e.message.contains("unsupported")), (1, true));
    assert_eq!(error_of("hello\n").line, 1);
    assert_eq!(error_of("").line, 1);
    assert!(
        error_of("bleedthrough-builtins 1")
            .message
            .contains("newline")
    );
}

#[test]
fn rejects_unknown_kind_naming_the_line() {
    let e = error_of("bleedthrough-builtins 1\nwidget\tx\n");
    assert_eq!(e.line, 2);
    assert!(e.message.contains("widget"));
}

#[test]
fn rejects_a_line_with_the_wrong_number_of_fields() {
    let text = mutated("const\tPHP_EOL", "\tstring\t", "\t");
    let e = error_of(&text);
    assert!(e.message.contains("fields"), "{e}");
    assert!(e.line > 1);
}

#[test]
fn rejects_duplicates_and_overlaps() {
    let text = format::write(&rich());
    let line = text
        .lines()
        .find(|l| l.starts_with("const\tPHP_EOL"))
        .unwrap();
    let dup = text.replace(line, &format!("{line}\n{line}"));
    let e = error_of(&dup);
    assert!(e.message.contains("PHP_EOL"), "{e}");

    let overlap = text.replace(
        line,
        &format!("{line}\n{}", line.replacen("8.0-8.5", "8.3", 1)),
    );
    assert!(
        error_of(&overlap).message.contains("overlap")
            || error_of(&overlap).message.contains("order")
    );
}

#[test]
fn rejects_unknown_versions_and_noncanonical_masks() {
    assert!(
        error_of(&mutated("PHP_EOL", "8.0-8.5", "8.0-8.6"))
            .message
            .contains("unknown PHP version")
    );
    assert!(
        error_of(&mutated("PHP_EOL", "8.0-8.5", "8.0,8.1-8.5"))
            .message
            .contains("canonical")
    );
}

#[test]
fn rejects_non_canonical_spelling() {
    // An escape the writer would not produce.
    assert!(
        error_of(&mutated("PHP_EOL", "core", "c\\x6fre"))
            .message
            .contains("canonical")
    );
    // Flags in the wrong order.
    assert!(
        error_of(&mutated("curl_exec", "byref,nodiscard", "nodiscard,byref"))
            .message
            .contains("canonical")
    );
    // Records out of order.
    let text = format::write(&rich());
    let mut lines: Vec<&str> = text.lines().collect();
    let last = lines.len() - 1;
    lines.swap(last, last - 1);
    let swapped = lines.join("\n") + "\n";
    assert!(error_of(&swapped).message.contains("out of order"));
}

#[test]
fn rejects_prose_in_type_and_default_fields() {
    let e = error_of(&mutated(
        "PHP_EOL",
        "\tstring\t",
        "\tthe text that is returned\t",
    ));
    assert!(
        e.message.contains("prose") || e.message.contains("type"),
        "{e}"
    );
    let e = error_of(&mutated(
        "Exception::code",
        "\t\t0",
        "\t\tthe default is zero",
    ));
    assert!(
        e.message.contains("prose") || e.message.contains("expression"),
        "{e}"
    );
}

#[test]
fn rejects_bad_escapes_and_bad_flags() {
    assert!(
        error_of(&mutated("PHP_EOL", "core", "co\\qre"))
            .message
            .contains("escape")
    );
    assert!(
        error_of(&mutated("curl_exec", "byref", "bogus"))
            .message
            .contains("unknown flag")
    );
    assert!(
        error_of(&mutated("ArrayObject::STD_PROP_LIST", "public", "open"))
            .message
            .contains("flag")
    );
}

#[test]
fn rejects_extensions_after_records_and_out_of_order() {
    let text = "bleedthrough-builtins 1\nextension\tb\t\nextension\ta\t\n";
    assert!(error_of(text).message.contains("out of order"));
}
