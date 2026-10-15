//! **Layer:** Domain.
//!
//! **Responsibility:** the taint-analysis engine, usable as a library under any front-end.
//!
//! **Permitted workspace dependencies:** `bleedthrough-ir`, `bleedthrough-symbols`, `bleedthrough-syntax`, `bleedthrough-rules`, `bleedthrough-packs`, `bleedthrough-findings`.

#![warn(clippy::exhaustive_enums, clippy::exhaustive_structs)]

#[cfg(test)]
mod tests {
    #[test]
    fn crate_is_part_of_the_workspace() {
        assert_eq!(env!("CARGO_PKG_NAME"), "bleedthrough-engine");
    }
}
