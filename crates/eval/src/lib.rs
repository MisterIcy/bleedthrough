//! **Layer:** Tooling.
//!
//! **Responsibility:** the vulnerability fixture corpus (loading and completeness checks) and the
//! accuracy evaluator: it reads the `json` report of a scan, matches findings to the corpus labels,
//! computes metrics, and compares them with the committed expectations.
//!
//! **Permitted workspace dependencies:** `bleedthrough-taxonomy`, `bleedthrough-findings`.

pub mod corpus;
pub mod coverage;
pub mod expectations;
pub mod matching;
pub mod report;
pub mod result;
pub mod run;
