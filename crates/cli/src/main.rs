//! **Layer:** Delivery.
//!
//! **Responsibility:** the `bleedthrough` binary and composition root: it chooses and wires
//! concrete implementations of ports, and contains presentation only.
//!
//! **Permitted workspace dependencies:** any crate.

use std::process::ExitCode;

fn main() -> ExitCode {
    let wants_version = std::env::args()
        .skip(1)
        .any(|a| a == "--version" || a == "-V");
    if wants_version {
        println!("bleedthrough {}", env!("CARGO_PKG_VERSION"));
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    #[test]
    fn crate_is_part_of_the_workspace() {
        assert_eq!(env!("CARGO_PKG_NAME"), "bleedthrough-cli");
    }
}
