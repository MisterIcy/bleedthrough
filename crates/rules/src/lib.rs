//! **Layer:** Domain.
//!
//! **Responsibility:** the rule model: sources, sinks, sanitizers and propagators as data, with
//! strict validation, and the port through which rules are loaded.
//!
//! **Permitted workspace dependencies:** `bleedthrough-taxonomy` only.
//!
//! The model has no file, parser or serialization dependency. A rule enters through a
//! [`RuleSource`]; a [`RuleDraft`] is validated into a [`Rule`], so an invalid rule cannot exist.

#![warn(clippy::exhaustive_enums, clippy::exhaustive_structs)]

mod body;
mod draft;
mod error;
mod id;
mod regex;
mod rule;
mod set;
mod source;
mod symbol;

pub use body::{
    ArgCondition, ArgTest, PatternBody, PatternShape, Propagator, RuleBody, Sanitizer, Sink,
    TaintBody,
};
pub use draft::{
    ArgConditionDraft, BodyDraft, ExtraCategoryDraft, MetricsDraft, PatternDraft, PropagatorDraft,
    RuleDraft, SanitizerDraft, SinkDraft, TaintDraft,
};
pub use error::RuleError;
pub use id::RuleId;
pub use regex::validate_regex;
pub use rule::{Origin, PhpVersion, Rule};
pub use set::{RuleSet, SCHEMA_VERSION};
pub use source::{LocatedError, LocatedRule, RuleSource, SourceOutput, load_rule_set};
pub use symbol::{ArgSel, SinkClass, SymbolKind, SymbolRef};
