//! Wall-time benchmark of the parse stage (and, with `--binary`, the end-to-end stage) with the Mago
//! backend. Writes a result document (`schemas/bench-result.schema.json`).
//!
//! ```text
//! cargo run --release -p bleedthrough-syntax-mago --example bench_parse -- \
//!     [--input synthetic:seed=1,files=200 | --input dir:PATH --name real:ID] [--tree DIR] \
//!     [--iterations N] [--warmups N] [--binary PATH] [--commit REV] [--out FILE]
//! ```
//!
//! Allocation counting is a separate program (`bench_alloc`): it replaces the global allocator, which
//! would distort every time measured here.

use std::process::ExitCode;

use bleedthrough_bench::cli::time_main;
use bleedthrough_syntax_mago::MagoBackend;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match time_main(&MagoBackend::new(), &args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
