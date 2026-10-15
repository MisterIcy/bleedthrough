//! **Layer:** Domain.
//!
//! **Responsibility:** findings and their value objects (fingerprint, severity, confidence, coverage).
//!
//! **Permitted workspace dependencies:** none.

#![warn(clippy::exhaustive_enums, clippy::exhaustive_structs)]

#[cfg(test)]
mod tests {
    #[test]
    fn crate_is_part_of_the_workspace() {
        assert_eq!(env!("CARGO_PKG_NAME"), "bleedthrough-findings");
    }
}
