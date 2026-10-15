//! **Layer:** Infrastructure.
//!
//! **Responsibility:** report emitters implementing application ports.
//!
//! **Permitted workspace dependencies:** `bleedthrough-app`, `bleedthrough-findings`.

#[cfg(test)]
mod tests {
    #[test]
    fn crate_is_part_of_the_workspace() {
        assert_eq!(env!("CARGO_PKG_NAME"), "bleedthrough-emit");
    }
}
