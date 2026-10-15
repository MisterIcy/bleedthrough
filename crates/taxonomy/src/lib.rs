//! **Layer:** Domain.
//!
//! **Responsibility:** the shared vocabulary of weaknesses (CWE), risk categories (OWASP) and
//! severity that rules and findings are tagged with.
//!
//! **Permitted workspace dependencies:** none.

#![warn(clippy::exhaustive_enums, clippy::exhaustive_structs)]

mod error;
mod registry;
mod scoring;
mod severity;
mod tags;

pub use error::TaxonomyError;
pub use registry::{CATALOG, CategoryEntry, Registry, WeaknessEntry};
pub use scoring::{
    AttackComplexity, AttackVector, Impact, Metrics, PrivilegesRequired, SEVERITY_SCHEME, Scope,
    UserInteraction, level_for_tenths,
};
pub use severity::{Severity, SeverityLevel};
pub use tags::{Cwe, OwaspRef, Taxonomy};
