//! **Layer:** Domain.
//!
//! **Responsibility:** the taint-analysis engine, usable as a library under any front-end: the call
//! graph over lowered units, constant folding for include paths and patterns, and the taint analysis
//! built on them. Every entry point is a pure function of its inputs.
//!
//! **Permitted workspace dependencies:** `bleedthrough-ir`, `bleedthrough-symbols`, `bleedthrough-syntax`, `bleedthrough-rules`, `bleedthrough-packs`, `bleedthrough-findings`, `bleedthrough-taxonomy`.

#![warn(clippy::exhaustive_enums, clippy::exhaustive_structs)]

mod callgraph;
mod coverage;
mod dataflow;
mod fold;
mod include;
mod label;
mod rule_index;
mod scc;

pub use callgraph::{
    CallGraph, CallGraphConfig, CallOutcome, CallSite, Component, IncludeOutcome, IncludeSite,
    SourceUnit, UnitId, UnitInfo, Unresolved,
};
pub use coverage::CallCoverage;
pub use dataflow::{Context, FlowLimits, Heap, Limits, SinkHit, UnitFlow, UnitInput, analyse_unit};
pub use fold::{Folded, Folder};
pub use include::{FoldedPath, IncludeResolver, NoIncludes, normalise_path};
pub use label::{Applied, ClassSet, HeapKey, Label, Marks, Origin, Taint};
pub use rule_index::{
    Gate, Matches, PropagatorEntry, RuleIndex, SanitizerEntry, SinkEntry, SourceDef, Subject,
};
pub use scc::strongly_connected;
