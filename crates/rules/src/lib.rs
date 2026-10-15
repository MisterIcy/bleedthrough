//! **Layer:** Domain.
//!
//! **Responsibility:** the rule model: sources, sinks, sanitizers and propagators as data.
//!
//! **Permitted workspace dependencies:** none.

#![warn(clippy::exhaustive_enums, clippy::exhaustive_structs)]

#[cfg(test)]
mod tests {
    #[test]
    fn crate_is_part_of_the_workspace() {
        assert_eq!(env!("CARGO_PKG_NAME"), "bleedthrough-rules");
    }
}
