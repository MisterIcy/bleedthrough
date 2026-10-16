//! Parsers of rule text must never panic on arbitrary input, and what they accept must be consistent.

use bleedthrough_rules::{ArgSel, PhpVersion, SinkClass, SymbolRef};
use bleedthrough_testkit::check;
use bleedthrough_testkit::proptest::prelude::*;

/// spec: rule-schema/Unknown keys and bad values are errors
#[test]
fn symbol_references_never_panic_on_arbitrary_text() {
    check(".{0,40}", |text| {
        let _ = SymbolRef::parse(&text);
        let _ = ArgSel::parse(&text);
        let _ = SinkClass::parse(&text);
        Ok(())
    });
}

#[test]
fn near_valid_symbol_references_are_accepted_or_rejected_without_panics() {
    let valid = prop::sample::select(vec![
        "mysqli_query",
        "App\\Db::find",
        "new App\\Db",
        "Foo::$bar",
        "Foo::BAR",
        "\\strlen",
        "static Foo::run",
    ])
    .prop_map(str::to_string);
    check(bleedthrough_testkit::mutated(valid), |bytes| {
        let _ = SymbolRef::parse(&String::from_utf8_lossy(&bytes));
        Ok(())
    });
}

#[test]
fn php_versions_round_trip_and_arbitrary_text_is_rejected_cleanly() {
    check((0u8..=20, 0u8..=99), |(major, minor)| {
        let text = format!("{major}.{minor}");
        let version = PhpVersion::parse(&text).map_err(TestCaseError::fail)?;
        prop_assert_eq!((version.major(), version.minor()), (major, minor));
        prop_assert_eq!(version.to_string(), text);
        Ok(())
    });
    check(".{0,12}", |text| {
        if let Ok(version) = PhpVersion::parse(&text) {
            prop_assert_eq!(PhpVersion::parse(&version.to_string()), Ok(version));
        }
        Ok(())
    });
}
