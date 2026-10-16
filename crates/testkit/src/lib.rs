//! Shared test support: generic `proptest` strategies and the seed and case-count settings.
//!
//! Dev-dependency only: no product build links this crate (docs/testing.md). It depends on no workspace
//! crate, so any crate may use it in its tests without a cycle; generators of a crate's own types live in
//! that crate's tests.

mod run;
mod strategies;

pub use proptest;
pub use run::{
    CASES_VAR, DEFAULT_CASES, DEFAULT_SEED, SEED_VAR, Settings, check, check_with, settings,
    settings_from,
};
pub use strategies::{
    byte_vec, mutated, php_identifier, php_qualified_name, php_source, relative_path,
    version_triple,
};
