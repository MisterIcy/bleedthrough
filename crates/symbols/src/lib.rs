//! **Layer:** Domain.
//!
//! **Responsibility:** symbol tables and type information derived from the syntax tree.
//!
//! **Permitted workspace dependencies:** `bleedthrough-syntax`.

#![warn(clippy::exhaustive_enums, clippy::exhaustive_structs)]

#[cfg(test)]
mod tests {
    #[test]
    fn crate_is_part_of_the_workspace() {
        assert_eq!(env!("CARGO_PKG_NAME"), "bleedthrough-symbols");
    }
}
