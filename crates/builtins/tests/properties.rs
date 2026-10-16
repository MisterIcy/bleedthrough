//! Properties of the text format: what the writer writes the reader reads back, and the second write is
//! byte-identical. Generators of this crate's own types live here (docs/testing.md).

use bleedthrough_builtins::{
    BuiltinData, ClassDetail, ConstantDetail, Deprecation, Detail, ExtensionRecord, FunctionDetail,
    MethodDetail, Param, PhpVersion, Record, Signature, VersionMask, Visibility, format,
};
use bleedthrough_testkit::proptest::prelude::*;
use bleedthrough_testkit::{Settings, check, check_with, php_identifier};

fn mask() -> impl Strategy<Value = VersionMask> {
    (0u8..=5, 0u8..=5).prop_map(|(a, b)| {
        let (first, last) = (a.min(b), a.max(b));
        VersionMask::range(
            PhpVersion::new(first).unwrap(),
            PhpVersion::new(last).unwrap(),
        )
    })
}

fn type_text() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "",
        "int",
        "?string",
        "array|false",
        "(A&B)|null",
        "\\Foo\\Bar",
        "array<int, string>",
        "int<0, max>",
        "callable(): void",
        "non-empty-string",
    ])
    .prop_map(str::to_string)
}

fn expr() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "null",
        "[]",
        "PHP_INT_MAX",
        "E_ALL & ~E_NOTICE",
        "'a b\\tc'",
        "\"x;y,z@\"",
        "-1",
        "Foo::BAR",
    ])
    .prop_map(str::to_string)
}

fn param() -> impl Strategy<Value = Param> {
    (
        php_identifier(),
        type_text(),
        type_text(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        prop::option::of(expr()),
    )
        .prop_map(
            |(name, type_text, doc_type, by_ref, variadic, sensitive, optional, default)| {
                let optional = optional || variadic || default.is_some();
                Param {
                    name,
                    type_text,
                    doc_type,
                    by_ref,
                    optional,
                    default: if variadic { None } else { default },
                    variadic,
                    sensitive,
                }
            },
        )
}

fn signature() -> impl Strategy<Value = Signature> {
    (
        prop::collection::vec(param(), 0..4),
        type_text(),
        type_text(),
        type_text(),
        any::<bool>(),
    )
        .prop_map(
            |(mut params, return_type, tentative, doc_return_type, return_by_ref)| {
                // An optional parameter may not be followed by a required one in real PHP; the model does not
                // care, so the generator does not either.
                params.dedup_by(|a, b| a.name == b.name);
                let (return_type, tentative_return_type) = if return_type.is_empty() {
                    (String::new(), tentative)
                } else {
                    (return_type, String::new())
                };
                Signature {
                    params,
                    return_type,
                    tentative_return_type,
                    doc_return_type,
                    return_by_ref,
                }
            },
        )
}

fn detail(index: usize) -> impl Strategy<Value = (String, Detail)> {
    let function = (php_identifier(), signature(), any::<bool>(), any::<bool>()).prop_map(
        move |(id, signature, no_discard, alias)| {
            (
                format!("f{index}_{id}"),
                Detail::Function(FunctionDetail {
                    signature,
                    alias_of: alias.then(|| "other_fn".to_string()),
                    no_discard,
                }),
            )
        },
    );
    let constant = (php_identifier(), type_text(), prop::option::of(expr())).prop_map(
        move |(id, type_text, value)| {
            (
                format!("C{index}_{id}"),
                Detail::Constant(ConstantDetail {
                    type_text,
                    value,
                    ..ConstantDetail::default()
                }),
            )
        },
    );
    let class = (
        php_identifier(),
        any::<bool>(),
        any::<bool>(),
        prop::collection::vec(php_identifier(), 0..3),
    )
        .prop_map(move |(id, is_final, is_abstract, implements)| {
            (
                format!("K{index}_{id}"),
                Detail::Class(ClassDetail {
                    is_final,
                    is_abstract,
                    implements,
                    ..ClassDetail::default()
                }),
            )
        });
    let method = (
        php_identifier(),
        php_identifier(),
        signature(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(move |(class, id, signature, is_static, is_final)| {
            (
                format!("M{index}_{class}::{id}"),
                Detail::Method(MethodDetail {
                    signature,
                    is_static,
                    is_final,
                    visibility: Visibility::Public,
                    ..MethodDetail::default()
                }),
            )
        });
    prop_oneof![function, constant, class, method]
}

fn record(index: usize) -> impl Strategy<Value = Record> {
    (
        detail(index),
        mask(),
        "[a-z][a-z0-9_]{0,6}",
        prop::collection::btree_set("[a-z]{1,5}", 0..3),
        prop::option::of(("[0-9]\\.[0-9]", "[a-z]{1,6}( [a-z]{1,6}){0,2}")),
    )
        .prop_map(|((name, detail), mask, extension, sapi, deprecation)| {
            let mut record = Record::new(name, mask, extension, detail);
            record.sapi = sapi.into_iter().collect();
            record.reflected = mask.intersection(VersionMask::range(
                PhpVersion::new(0).unwrap(),
                PhpVersion::new(2).unwrap(),
            ));
            record.deprecation = deprecation.map(|(since, message)| Deprecation {
                since,
                message: message.replace(' ', "_"),
            });
            record
        })
}

/// Valid built-in data: records of distinct symbols, so no two masks overlap.
fn data() -> impl Strategy<Value = BuiltinData> {
    let records = (0usize..6).prop_flat_map(|n| (0..n).map(record).collect::<Vec<_>>());
    (
        records,
        prop::collection::btree_set("[a-z]{1,6}", 0..3),
        mask(),
    )
        .prop_filter_map(
            "the records form valid data",
            |(records, extensions, unbundled)| {
                let extensions = extensions
                    .into_iter()
                    .map(|name| ExtensionRecord { name, unbundled })
                    .collect();
                BuiltinData::new(extensions, records).ok()
            },
        )
}

/// spec: builtin-data-model/The text format is canonical and round-trips
#[test]
fn the_reader_returns_what_the_writer_wrote_and_the_second_write_is_identical() {
    check(data(), |data| {
        let first = format::write(&data);
        let read = format::read(&first).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(&read, &data);
        prop_assert_eq!(format::write(&read), first);
        Ok(())
    });
}

#[test]
fn the_property_notices_a_writer_that_loses_information() {
    // A writer that drops the last line cannot round trip any non-empty data.
    let result = check_with(Settings { seed: 3, cases: 64 }, None, data(), |data| {
        let text = format::write(&data);
        let broken: Vec<&str> = text.lines().collect();
        let broken = broken[..broken.len().saturating_sub(1)].join("\n") + "\n";
        match format::read(&broken) {
            Ok(read) => prop_assert_eq!(read, data),
            Err(e) => prop_assert!(false, "{e}"),
        }
        Ok(())
    });
    let message = result.expect_err("a lossy writer must fail the property");
    assert!(message.contains("seed: 3"), "{message}");
}

#[test]
fn the_generator_produces_non_trivial_data() {
    let most = std::cell::Cell::new(0);
    check(data(), |data| {
        most.set(most.get().max(data.records().len()));
        Ok(())
    });
    assert!(
        most.get() >= 3,
        "largest generated data had {} records",
        most.get()
    );
}

/// spec: builtin-data-model/The text format is canonical and round-trips
#[test]
fn the_reader_never_panics_on_arbitrary_bytes_and_accepts_only_canonical_text() {
    check(bleedthrough_testkit::byte_vec(256), |bytes| {
        let text = String::from_utf8_lossy(&bytes);
        if let Ok(data) = format::read(&text) {
            prop_assert_eq!(format::write(&data), text.as_ref());
        }
        Ok(())
    });
}

#[test]
fn near_valid_data_is_read_or_rejected_and_what_is_read_is_canonical() {
    let written = data().prop_map(|d| format::write(&d));
    check(bleedthrough_testkit::mutated(written), |bytes| {
        let text = String::from_utf8_lossy(&bytes);
        if let Ok(data) = format::read(&text) {
            prop_assert_eq!(format::write(&data), text.as_ref());
        }
        Ok(())
    });
}

#[test]
fn version_and_kind_names_never_panic_when_parsed() {
    check(".{0,12}", |text| {
        let _ = PhpVersion::parse(&text);
        let _ = VersionMask::parse(&text);
        let _ = bleedthrough_builtins::Kind::parse(&text);
        Ok(())
    });
}
