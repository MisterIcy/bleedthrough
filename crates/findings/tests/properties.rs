//! Properties of fingerprints: stable for equal input, different for different input, and a text form that
//! round trips. Generators live here (docs/testing.md).

use bleedthrough_findings::{FINGERPRINT_PREFIX, Fingerprint, FingerprintInput, RuleId};
use bleedthrough_testkit::check;
use bleedthrough_testkit::proptest::prelude::*;

type Parts = (String, String, Vec<String>, u32);

fn parts() -> impl Strategy<Value = Parts> {
    (
        "[a-z][a-z.]{0,10}",
        ".{0,12}",
        prop::collection::vec(".{0,12}", 0..4),
        0u32..4,
    )
}

fn input(p: &Parts) -> FingerprintInput {
    FingerprintInput::new(
        RuleId::new(p.0.clone()).unwrap(),
        p.1.clone(),
        p.2.clone(),
        p.3,
    )
}

fn fingerprint(p: &Parts) -> Fingerprint {
    Fingerprint::compute(&input(p))
}

/// spec: finding-model/Fingerprints are stable
#[test]
fn equal_inputs_give_equal_fingerprints_that_round_trip_through_text() {
    check(parts(), |p| {
        let fp = fingerprint(&p);
        prop_assert_eq!(&fp, &fingerprint(&p.clone()));
        prop_assert!(fp.as_str().starts_with(FINGERPRINT_PREFIX));
        prop_assert_eq!(Fingerprint::parse(fp.as_str()), Ok(fp));
        Ok(())
    });
}

#[test]
fn different_inputs_give_different_fingerprints() {
    check((parts(), parts()), |(a, b)| {
        prop_assume!(a != b);
        prop_assert_ne!(fingerprint(&a), fingerprint(&b));
        Ok(())
    });
}

#[test]
fn changing_any_single_field_changes_the_fingerprint() {
    check((parts(), "[a-z]{1,4}"), |(base, extra)| {
        let original = fingerprint(&base);
        let mut symbol = base.clone();
        symbol.1.push_str(&extra);
        let mut snippets = base.clone();
        snippets.2.push(extra.clone());
        let mut occurrence = base.clone();
        occurrence.3 += 1;
        let mut rule = base.clone();
        rule.0.push_str(&extra);
        for (what, changed) in [
            ("symbol", symbol),
            ("snippets", snippets),
            ("occurrence", occurrence),
            ("rule", rule),
        ] {
            prop_assert_ne!(&original, &fingerprint(&changed), "{}", what);
        }
        Ok(())
    });
}

#[test]
fn parsing_arbitrary_text_never_panics_and_accepted_text_is_canonical() {
    check(".{0,40}", |text| {
        if let Ok(fp) = Fingerprint::parse(&text) {
            prop_assert_eq!(fp.as_str(), text.as_str());
        }
        Ok(())
    });
}
