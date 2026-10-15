//! Runs the shared conformance suite and the robustness checks against the Mago backend.
//!
//! `BLESS=1 cargo test -p bleedthrough-syntax-mago --test conformance` rewrites the golden dumps.

use std::path::{Path, PathBuf};

use bleedthrough_syntax_conformance::mutate::{RobustnessOptions, run_robustness};
use bleedthrough_syntax_conformance::suite::{SuiteOptions, default_golden_dir, run_suite};
use bleedthrough_syntax_conformance::{Allowances, Corpus, Failure};
use bleedthrough_syntax_mago::MagoBackend;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/php")
}

fn conformance_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/syntax-conformance")
}

fn corpus() -> Corpus {
    Corpus::load(&root()).expect("the corpus loads")
}

fn report(failures: &[Failure]) -> String {
    failures
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_corpus_is_complete() {
    let problems = corpus().check();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn every_fixture_conforms() {
    let mut options = SuiteOptions::new(default_golden_dir(&conformance_dir()));
    options.bless = std::env::var_os("BLESS").is_some();
    let result = run_suite(&MagoBackend::new(), &corpus(), &options);
    assert!(result.fixtures > 0);
    for (version, reason) in &result.unverified {
        println!("PHP {version}: unverified ({reason})");
    }
    let (known, unexpected) = split_known(result.failures);
    assert!(unexpected.is_empty(), "{}", report(&unexpected));
    for (fixture, reason) in listed_gaps().entries() {
        assert!(
            known.iter().any(|f| f.subject == fixture),
            "{fixture} is listed as a known gap ({reason}) but no longer fails; remove the entry"
        );
    }
}

fn listed_gaps() -> Allowances {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/known-gaps.toml"),
    )
    .expect("known-gaps.toml is readable");
    Allowances::parse(&text).expect("known-gaps.toml parses")
}

fn split_known(failures: Vec<Failure>) -> (Vec<Failure>, Vec<Failure>) {
    let gaps = listed_gaps();
    failures
        .into_iter()
        .partition(|f| gaps.reason(&f.subject).is_some())
}

#[test]
fn prefixes_and_mutations_never_break_the_backend() {
    let mut options = RobustnessOptions::default();
    if let Some(stride) = std::env::var("PREFIX_STRIDE")
        .ok()
        .and_then(|s| s.parse().ok())
    {
        options.prefix_stride = stride;
    }
    if let Some(mutations) = std::env::var("MUTATIONS").ok().and_then(|s| s.parse().ok()) {
        options.mutations = mutations;
    }
    if let Some(seed) = std::env::var("SEED").ok().and_then(|s| s.parse().ok()) {
        options.seed = seed;
    }
    let failures = run_robustness(&MagoBackend::new(), &corpus(), &options);
    assert!(failures.is_empty(), "{}", report(&failures));
}
