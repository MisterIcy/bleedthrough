//! **Layer:** Infrastructure.
//!
//! **Responsibility:** report emitters implementing application ports.
//!
//! **Permitted workspace dependencies:** `bleedthrough-app`, `bleedthrough-findings`.
//!
//! The one public entry is [`emitter_for`]; everything else is reached through the
//! `bleedthrough_app::Emitter` port.

mod diff;
mod gha;
mod json;
mod sarif;
mod summary;
#[cfg(test)]
mod testgen;
mod tsv;
mod wire;

use bleedthrough_app::{Emitter, Format};

/// The emitter that writes `format`, or `None` when no emitter is wired for it.
///
/// `Format` is non-exhaustive, so a format added to the port without an emitter here is
/// caught by the smoke test over `Format::NAMES`, not by the compiler.
pub fn emitter_for(format: Format) -> Option<Box<dyn Emitter>> {
    match format {
        Format::Json => Some(Box::new(json::JsonEmitter)),
        Format::Ndjson => Some(Box::new(json::NdjsonEmitter)),
        Format::Sarif => Some(Box::new(sarif::SarifEmitter)),
        Format::Tsv => Some(Box::new(tsv::TsvEmitter)),
        Format::GithubActions => Some(Box::new(gha::GithubActionsEmitter)),
        Format::Diff => Some(Box::new(diff::DiffEmitter)),
        _ => None,
    }
}
