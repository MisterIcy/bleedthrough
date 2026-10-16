//! **Layer:** Tooling.
//!
//! **Responsibility:** the benchmark harness (stages, metrics, versioned result documents, base-versus-head
//! comparison, committed deterministic baselines) and the seeded generator of synthetic PHP trees.
//!
//! **Permitted workspace dependencies:** `bleedthrough-syntax`.

pub mod baseline;
pub mod cli;
pub mod compare;
pub mod memory;
pub mod parse_stage;
pub mod realworld;
pub mod result;
pub mod stage;
pub mod synth;
pub mod timing;
