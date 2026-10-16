//! **Layer:** Tooling.
//!
//! **Responsibility:** generates the built-in data from the vendored php-src stub files, through
//! the parser-backend port.
//!
//! **Permitted workspace dependencies:** `bleedthrough-syntax`, `bleedthrough-builtins`.

#![allow(
    missing_docs,
    reason = "plain records of public fields; modules and functions are documented"
)]

pub mod annotations;
pub mod bundling;
pub mod conflicts;
pub mod count;
pub mod extract;
pub mod fixture;
pub mod generate;
pub mod guards;
pub mod merge;
pub mod oracle;
pub mod origin;
pub mod release;

pub use bleedthrough_builtins as model;
pub use bleedthrough_builtins::PhpVersion;
pub use bleedthrough_builtins::check_record;
