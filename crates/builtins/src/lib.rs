//! **Layer:** Domain.
//!
//! **Responsibility:** the data model of PHP's built-in functions, classes and constants with their
//! version availability, the canonical text format that stores it, and the `BuiltinDataSource`
//! port through which consumers obtain it.
//!
//! **Permitted workspace dependencies:** none.
//!
//! The crate does no I/O and knows nothing about PHP parsing. Types and defaults are kept as PHP
//! source text; documentation prose is never stored.

#![warn(clippy::exhaustive_enums, clippy::exhaustive_structs)]

mod data;
pub mod format;
mod record;
mod source;
mod validate;
mod version;

pub use data::{BuiltinData, DataError, Lookup, canonical_order};
pub use format::FormatError;
pub use record::{
    CaseDetail, ClassDetail, ConstantDetail, Deprecation, Detail, ExtensionRecord, FunctionDetail,
    Kind, MethodDetail, Param, PropertyDetail, Record, Signature, Visibility, normalise_name,
};
pub use source::{BuiltinDataSource, InMemorySource, SourceError};
pub use validate::check_record;
pub use version::{LAST_MINOR, MaskError, PhpVersion, VersionMask};
