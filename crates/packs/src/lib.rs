//! **Layer:** Domain.
//!
//! **Responsibility:** per-library knowledge packs expressed as plain serializable data.
//!
//! **Permitted workspace dependencies:** `bleedthrough-rules`.

#![warn(clippy::exhaustive_enums, clippy::exhaustive_structs)]

#[cfg(test)]
mod tests {
    #[test]
    fn crate_is_part_of_the_workspace() {
        assert_eq!(env!("CARGO_PKG_NAME"), "bleedthrough-packs");
    }
}
