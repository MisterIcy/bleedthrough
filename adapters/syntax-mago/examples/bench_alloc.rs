//! Allocation counters of the parse stage with the Mago backend: deterministic for a fixed input, so
//! they can be committed as a baseline (`benchmarks/baselines.toml`) and gated offline.
//!
//! ```text
//! cargo run -p bleedthrough-syntax-mago --example bench_alloc -- \
//!     [--input synthetic:...] [--baseline benchmarks/baselines.toml [--update]]
//! ```
//!
//! The counting allocator is `dhat`'s; it is installed only in this program and counts one
//! single-threaded pass over an input that is already loaded. Never use this program for wall time.

use std::process::ExitCode;

use bleedthrough_bench::cli::alloc_main;
use bleedthrough_bench::stage::Allocation;
use bleedthrough_syntax_mago::MagoBackend;

#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

/// Runs `pass` under a fresh profiler and returns what it allocated. Everything allocated before the
/// profiler starts (the loaded input) is not counted.
fn measure(pass: &mut dyn FnMut()) -> Allocation {
    let _profiler = dhat::Profiler::builder().testing().build();
    let before = dhat::HeapStats::get();
    pass();
    let after = dhat::HeapStats::get();
    Allocation {
        count: after.total_blocks - before.total_blocks,
        total_bytes: after.total_bytes - before.total_bytes,
        peak_bytes: (after.max_bytes.saturating_sub(before.curr_bytes)) as u64,
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match alloc_main(&MagoBackend::new(), &args, &measure) {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
