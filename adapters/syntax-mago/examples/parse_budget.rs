//! The parse-failure budget over the fetched real-world projects, with the Mago backend.
//!
//! ```text
//! cargo xtask parse-budget [--project ID] [--update]
//! ```
//!
//! Exit status: 0 within budget, 1 above or below it, 3 when no project is fetched.

use std::process::ExitCode;

use bleedthrough_bench::cli::budget_main;
use bleedthrough_syntax_mago::MagoBackend;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match budget_main(&MagoBackend::new(), &args) {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
