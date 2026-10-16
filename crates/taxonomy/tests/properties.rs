//! Properties of the taxonomy: tag text round trips, canonical ordering, and the severity model's range and
//! monotonicity. Generators of this crate's own types live here (docs/testing.md).

use bleedthrough_taxonomy::{
    AttackComplexity, AttackVector, Cwe, Impact, Metrics, OwaspRef, PrivilegesRequired, Registry,
    Scope, Severity, Taxonomy, UserInteraction, level_for_tenths,
};
use bleedthrough_testkit::proptest::prelude::*;
use bleedthrough_testkit::{Settings, check, check_with};

fn cwe() -> impl Strategy<Value = Cwe> {
    let numbers: Vec<u32> = Registry::builtin()
        .weaknesses()
        .iter()
        .map(|w| w.number)
        .collect();
    prop::sample::select(numbers).prop_map(|n| Cwe::new(n).expect("registered"))
}

fn owasp() -> impl Strategy<Value = OwaspRef> {
    let ids: Vec<(u16, &'static str)> = Registry::builtin()
        .categories()
        .iter()
        .map(|c| (c.edition, c.id))
        .collect();
    prop::sample::select(ids).prop_map(|(e, id)| OwaspRef::new(e, id).expect("registered"))
}

macro_rules! metric_strategy {
    ($name:ident, $ty:ty) => {
        fn $name() -> impl Strategy<Value = $ty> {
            prop::sample::select(<$ty>::words())
                .prop_map(|word| <$ty>::parse(word).expect("a listed word parses"))
        }
    };
}
metric_strategy!(attack_vector, AttackVector);
metric_strategy!(attack_complexity, AttackComplexity);
metric_strategy!(privileges, PrivilegesRequired);
metric_strategy!(interaction, UserInteraction);
metric_strategy!(scope, Scope);
metric_strategy!(impact, Impact);

fn metrics() -> impl Strategy<Value = Metrics> {
    (
        (
            attack_vector(),
            attack_complexity(),
            privileges(),
            interaction(),
        ),
        (scope(), impact(), impact(), impact()),
    )
        .prop_map(|((av, ac, pr, ui), (s, c, i, a))| Metrics {
            attack_vector: av,
            attack_complexity: ac,
            privileges_required: pr,
            user_interaction: ui,
            scope: s,
            confidentiality: c,
            integrity: i,
            availability: a,
        })
}

/// spec: security-taxonomy/Tags have stable text forms
#[test]
fn tag_text_round_trips() {
    check((cwe(), owasp()), |(cwe, owasp)| {
        prop_assert_eq!(Cwe::parse(&cwe.to_string()), Ok(cwe));
        prop_assert_eq!(cwe.to_string().parse::<Cwe>(), Ok(cwe));
        prop_assert_eq!(OwaspRef::parse(&owasp.to_string()), Ok(owasp.clone()));
        // A second rendering is byte-identical.
        let again = OwaspRef::parse(&owasp.to_string()).unwrap().to_string();
        prop_assert_eq!(again, owasp.to_string());
        Ok(())
    });
}

#[test]
fn parsing_arbitrary_text_never_panics_and_accepted_text_is_a_registered_tag() {
    check(".{0,24}", |text| {
        if let Ok(cwe) = Cwe::parse(&text) {
            prop_assert!(Registry::builtin().weakness(cwe.number()).is_some());
            prop_assert_eq!(Cwe::parse(&cwe.to_string()), Ok(cwe));
        }
        if let Ok(owasp) = OwaspRef::parse(&text) {
            prop_assert_eq!(OwaspRef::parse(&owasp.to_string()), Ok(owasp));
        }
        Ok(())
    });
}

#[test]
fn a_taxonomy_is_sorted_deduplicated_and_independent_of_input_order() {
    let tags = (
        prop::collection::vec(cwe(), 1..6),
        prop::collection::vec(owasp(), 1..6),
        any::<prop::sample::Index>(),
    );
    check(tags, |(cwe, owasp, shuffle)| {
        let taxonomy = Taxonomy::new(cwe.clone(), owasp.clone()).unwrap();
        prop_assert!(taxonomy.cwe().windows(2).all(|w| w[0] < w[1]));
        prop_assert!(taxonomy.owasp().windows(2).all(|w| w[0] < w[1]));
        let (mut cwe2, mut owasp2) = (cwe, owasp);
        let n = shuffle.index(cwe2.len());
        cwe2.rotate_left(n);
        cwe2.extend(cwe2.clone());
        owasp2.reverse();
        let other = Taxonomy::new(cwe2, owasp2).unwrap();
        prop_assert_eq!(&other, &taxonomy);
        // Idempotent: rebuilding from its own parts changes nothing.
        let rebuilt = Taxonomy::new(taxonomy.cwe().to_vec(), taxonomy.owasp().to_vec()).unwrap();
        prop_assert_eq!(rebuilt, taxonomy);
        Ok(())
    });
}

/// spec: severity-scoring/A fixed formula derives the score
#[test]
fn a_score_stays_in_range_and_agrees_with_its_level_and_vector() {
    check(metrics(), |m| {
        let score = m.score_tenths();
        prop_assert!(score <= 100);
        prop_assert_eq!(m.level(), level_for_tenths(score));
        let severity = Severity::from_metrics(&m);
        prop_assert_eq!(severity.score_tenths(), Some(score));
        let vector = m.vector();
        prop_assert_eq!(severity.vector(), Some(vector.as_str()));
        prop_assert_eq!(severity.level(), m.level());
        Ok(())
    });
}

#[test]
fn levels_never_decrease_as_the_score_rises() {
    check((0u8..=100, 0u8..=100), |(a, b)| {
        let (low, high) = (a.min(b), a.max(b));
        prop_assert!(level_for_tenths(low) <= level_for_tenths(high));
        Ok(())
    });
}

/// Declared order of each metric's variants is from the most to the least severe for the exploitability
/// metrics (network first, low complexity first, no privileges first, no interaction first) and from the
/// least to the most severe for the impact metrics (none first).
#[test]
fn a_worse_metric_never_lowers_the_score() {
    check((metrics(), 0usize..7), |(m, which)| {
        let base = m.score_tenths();
        let mut worse = m;
        match which {
            0 => worse.attack_vector = AttackVector::Network,
            1 => worse.attack_complexity = AttackComplexity::Low,
            2 => worse.privileges_required = PrivilegesRequired::None,
            3 => worse.user_interaction = UserInteraction::None,
            4 => worse.confidentiality = Impact::High,
            5 => worse.integrity = Impact::High,
            _ => worse.availability = Impact::High,
        }
        prop_assert!(
            worse.score_tenths() >= base,
            "{} -> {}",
            m.vector(),
            worse.vector()
        );
        Ok(())
    });
}

#[test]
fn stepping_one_metric_through_its_order_moves_the_score_one_way() {
    check(metrics(), |m| {
        let mut previous = None;
        for word in Impact::words() {
            let mut next = m;
            next.confidentiality = Impact::parse(word).unwrap();
            let score = next.score_tenths();
            if let Some(p) = previous {
                prop_assert!(score >= p, "impact words are listed from none to high");
            }
            previous = Some(score);
        }
        let mut previous = None;
        for word in AttackVector::words() {
            let mut next = m;
            next.attack_vector = AttackVector::parse(word).unwrap();
            let score = next.score_tenths();
            if let Some(p) = previous {
                prop_assert!(
                    score <= p,
                    "attack vectors are listed from network to physical"
                );
            }
            previous = Some(score);
        }
        Ok(())
    });
}

#[test]
fn the_property_notices_a_scoring_rule_that_rises_when_it_should_fall() {
    let result = check_with(Settings { seed: 9, cases: 64 }, None, metrics(), |m| {
        let mut worse = m;
        worse.confidentiality = Impact::None;
        // A deliberately inverted claim: removing impact never lowers the score.
        prop_assert!(worse.score_tenths() >= m.score_tenths());
        Ok(())
    });
    assert!(result.is_err());
}
