//! Seeds, case counts and the runner that reports a failure reproducibly.

use std::panic::Location;

use proptest::prelude::*;
use proptest::test_runner::{
    Config, FailurePersistence, FileFailurePersistence, RngSeed, TestError, TestRunner,
};

/// Environment variable that repeats a seed.
pub const SEED_VAR: &str = "BLEEDTHROUGH_PROPTEST_SEED";
/// Environment variable that raises (or lowers) the case count.
pub const CASES_VAR: &str = "BLEEDTHROUGH_PROPTEST_CASES";
/// Seed used when `BLEEDTHROUGH_PROPTEST_SEED` is unset: runs are identical until someone asks otherwise.
pub const DEFAULT_SEED: u64 = 0x424c_4545_4454_4852;
/// Cases per property in required CI; the whole property suite is budgeted in docs/testing.md.
pub const DEFAULT_CASES: u32 = 256;

/// The seed and the number of cases of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// Seed of the random generator.
    pub seed: u64,
    /// Cases per property.
    pub cases: u32,
}

/// Parses the two settings from their raw text; `None` means unset (the default applies).
pub fn settings_from(seed: Option<&str>, cases: Option<&str>) -> Result<Settings, String> {
    let seed = match seed {
        None => DEFAULT_SEED,
        Some(text) => text
            .trim()
            .parse::<u64>()
            .map_err(|e| format!("{SEED_VAR}={text:?} is not a u64: {e}"))?,
    };
    let cases = match cases {
        None => DEFAULT_CASES,
        Some(text) => match text.trim().parse::<u32>() {
            Ok(n) if n > 0 => n,
            _ => return Err(format!("{CASES_VAR}={text:?} is not a positive integer")),
        },
    };
    Ok(Settings { seed, cases })
}

/// The settings of this process, from the environment. An unusable value is a failure, not a silent default.
pub fn settings() -> Settings {
    let seed = std::env::var(SEED_VAR).ok();
    let cases = std::env::var(CASES_VAR).ok();
    settings_from(seed.as_deref(), cases.as_deref()).unwrap_or_else(|e| panic!("{e}"))
}

fn config(settings: Settings, source: Option<&'static str>) -> Config {
    let persistence: Option<Box<dyn FailurePersistence>> = source.map(|_| {
        Box::new(FileFailurePersistence::WithSource("proptest-regressions"))
            as Box<dyn FailurePersistence>
    });
    Config {
        cases: settings.cases,
        rng_seed: RngSeed::Fixed(settings.seed),
        failure_persistence: persistence,
        source_file: source,
        // Shrinking is bounded so a failure always terminates.
        max_shrink_iters: 4096,
        ..Config::default()
    }
}

/// Runs a property under the process settings. On failure panics with the reason, the minimised input, the
/// seed and how to repeat the run. Regression inputs found by longer runs are saved beside the calling file in
/// `proptest-regressions/` and are committed.
#[track_caller]
pub fn check<S>(strategy: S, test: impl Fn(S::Value) -> Result<(), TestCaseError>)
where
    S: Strategy,
    S::Value: std::fmt::Debug,
{
    let source = Location::caller().file();
    if let Err(message) = check_with(settings(), Some(source), strategy, test) {
        panic!("{message}");
    }
}

/// Like [`check`] with explicit settings, returning the failure report instead of panicking.
/// `source` enables the regression file for the calling source file.
pub fn check_with<S>(
    settings: Settings,
    source: Option<&'static str>,
    strategy: S,
    test: impl Fn(S::Value) -> Result<(), TestCaseError>,
) -> Result<(), String>
where
    S: Strategy,
    S::Value: std::fmt::Debug,
{
    let mut runner = TestRunner::new(config(settings, source));
    runner.run(&strategy, test).map_err(|e| match e {
        TestError::Fail(reason, input) => format!(
            "property failed: {reason}\nminimal failing input: {input:#?}\nseed: {seed}\nrepeat with: {SEED_VAR}={seed} {CASES_VAR}={cases} cargo test <name>",
            seed = settings.seed,
            cases = settings.cases,
        ),
        TestError::Abort(reason) => format!("property aborted: {reason}\nseed: {}", settings.seed),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_settings_use_the_defaults() {
        assert_eq!(
            settings_from(None, None),
            Ok(Settings {
                seed: DEFAULT_SEED,
                cases: DEFAULT_CASES
            })
        );
    }

    #[test]
    fn set_settings_are_parsed_and_bad_ones_rejected() {
        assert_eq!(
            settings_from(Some("7"), Some("1000")),
            Ok(Settings {
                seed: 7,
                cases: 1000
            })
        );
        assert!(settings_from(Some("x"), None).is_err());
        assert!(settings_from(None, Some("0")).is_err());
        assert!(settings_from(None, Some("-3")).is_err());
    }

    fn sample(seed: u64) -> Vec<u32> {
        let seen = std::cell::RefCell::new(Vec::new());
        let settings = Settings { seed, cases: 20 };
        check_with(settings, None, 0..1_000_000u32, |v| {
            seen.borrow_mut().push(v);
            Ok(())
        })
        .expect("always passes");
        seen.into_inner()
    }

    #[test]
    fn the_same_seed_yields_the_same_values() {
        assert_eq!(sample(11), sample(11));
        assert_ne!(sample(11), sample(12));
    }

    fn failing(seed: u64) -> String {
        check_with(Settings { seed, cases: 256 }, None, 0..10_000u32, |v| {
            prop_assert!(v < 100, "{v} is not below 100");
            Ok(())
        })
        .expect_err("the property is false")
    }

    #[test]
    fn a_failure_prints_the_seed_and_a_minimised_input() {
        let message = failing(5);
        assert!(message.contains("seed: 5"), "{message}");
        assert!(message.contains("minimal failing input: 100"), "{message}");
        assert!(
            message.contains("BLEEDTHROUGH_PROPTEST_SEED=5"),
            "{message}"
        );
    }

    #[test]
    fn repeating_the_seed_fails_identically() {
        assert_eq!(failing(5), failing(5));
    }
}
