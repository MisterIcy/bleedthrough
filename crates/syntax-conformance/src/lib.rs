//! **Layer:** Tooling.
//!
//! **Responsibility:** the backend-agnostic conformance suite for parser backends, run through the
//! `ParserBackend` port over the PHP fixture corpus in `fixtures/php/`.
//!
//! **Permitted workspace dependencies:** `bleedthrough-syntax`.

pub mod corpus;
pub mod differential;
pub mod dump;
pub mod interpreter;
pub mod mutate;
pub mod suite;

pub use corpus::{Corpus, Expectation, Feature, Fixture, Group, Ledger, Verdict, VersionInfo};
pub use differential::{Allowances, compare};
pub use dump::dump_tree;
pub use mutate::{
    Mutation, MutationKind, Prng, RobustnessOptions, derive_seed, mutate, run_robustness,
};
pub use suite::{Failure, SuiteOptions, SuiteReport, run_fixture, run_suite};
