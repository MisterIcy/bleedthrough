//! **Layer:** Domain.
//!
//! **Responsibility:** the owned PHP syntax tree, source spans, and the parser-backend port that adapters implement.
//!
//! **Permitted workspace dependencies:** none.

#![warn(clippy::exhaustive_enums, clippy::exhaustive_structs)]

#[cfg(test)]
mod tests {
    #[test]
    fn crate_is_part_of_the_workspace() {
        assert_eq!(env!("CARGO_PKG_NAME"), "bleedthrough-syntax");
    }
}
