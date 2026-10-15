//! **Layer:** Domain.
//!
//! **Responsibility:** the intermediate representation and flow graphs analysed by the engine.
//!
//! **Permitted workspace dependencies:** `bleedthrough-symbols`, `bleedthrough-syntax`.

#![warn(clippy::exhaustive_enums, clippy::exhaustive_structs)]

#[cfg(test)]
mod tests {
    #[test]
    fn crate_is_part_of_the_workspace() {
        assert_eq!(env!("CARGO_PKG_NAME"), "bleedthrough-ir");
    }
}
