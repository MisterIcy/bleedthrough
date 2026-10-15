//! **Layer:** Infrastructure.
//!
//! **Responsibility:** the cache store implementing application ports.
//!
//! **Permitted workspace dependencies:** `bleedthrough-app`, `bleedthrough-engine`, `bleedthrough-symbols`.

#[cfg(test)]
mod tests {
    #[test]
    fn crate_is_part_of_the_workspace() {
        assert_eq!(env!("CARGO_PKG_NAME"), "bleedthrough-cache");
    }
}
