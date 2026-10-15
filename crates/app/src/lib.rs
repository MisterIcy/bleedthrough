//! **Layer:** Application.
//!
//! **Responsibility:** scan use cases and the driven ports they need (sources, cache, lock files, emitters).
//!
//! **Permitted workspace dependencies:** `bleedthrough-engine`, `bleedthrough-findings`, `bleedthrough-syntax`, `bleedthrough-symbols`, `bleedthrough-rules`, `bleedthrough-packs`.

#![warn(clippy::exhaustive_enums, clippy::exhaustive_structs)]

#[cfg(test)]
mod tests {
    #[test]
    fn crate_is_part_of_the_workspace() {
        assert_eq!(env!("CARGO_PKG_NAME"), "bleedthrough-app");
    }
}
