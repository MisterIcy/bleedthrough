//! **Layer:** Application.
//!
//! **Responsibility:** scan use cases and the driven ports they need (sources, cache, lock files, emitters).
//!
//! **Permitted workspace dependencies:** `bleedthrough-engine`, `bleedthrough-findings`, `bleedthrough-syntax`, `bleedthrough-symbols`, `bleedthrough-rules`, `bleedthrough-packs`.

#![warn(clippy::exhaustive_enums, clippy::exhaustive_structs)]

mod emit;
mod format;

pub use emit::{
    DEFAULT_ANNOTATION_CAP, EmitContext, EmitError, EmitOutcome, Emitter, SkippedFix, SourceText,
};
pub use format::{Format, UnknownFormat};
