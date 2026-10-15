//! **Layer:** Domain.
//!
//! **Responsibility:** findings and their value objects (fingerprint, severity, confidence, coverage).
//!
//! **Permitted workspace dependencies:** `bleedthrough-taxonomy` only.
//!
//! Every value type validates at construction, so an invalid finding cannot reach an emitter.
//! The domain carries no serialization: wire formats live in `bleedthrough-emit`.

#![warn(clippy::exhaustive_enums, clippy::exhaustive_structs)]

mod confidence;
mod coverage;
mod error;
mod finding;
mod fingerprint;
mod fix;
mod ids;
mod location;
mod report;
mod trace;

pub use confidence::{Confidence, ConfidenceTier, Reason, ReasonCode};
pub use coverage::{
    CacheStats, Coverage, LimitHit, LimitKind, PackRef, SkipReason, SkippedFile, Suppressed,
};
pub use error::ModelError;
pub use finding::{Finding, FindingParts};
pub use fingerprint::{FINGERPRINT_PREFIX, Fingerprint, FingerprintInput};
pub use fix::{Applicability, Edit, Fix};
pub use ids::{Message, RuleId};
pub use location::{Location, Position, RelPath};
pub use report::{REPORT_SCHEMA_VERSION, Report, RunInfo, RunOptions};
pub use trace::{Step, StepKind, Trace};
