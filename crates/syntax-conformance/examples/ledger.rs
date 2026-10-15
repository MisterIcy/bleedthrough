//! Records or verifies the per-version verdict ledger of the PHP fixture corpus.
//!
//! ```text
//! cargo run -p bleedthrough-syntax-conformance --example ledger -- record [--docker] [CORPUS]
//! cargo run -p bleedthrough-syntax-conformance --example ledger -- verify [--docker] [CORPUS]
//! ```
//!
//! `record` rewrites `ledger.toml` from the interpreters it can run; `verify` compares them with
//! the committed ledger and exits non-zero on any disagreement. Versions without an interpreter
//! are listed as unverified and are never reported as verified.

use std::path::PathBuf;
use std::process::ExitCode;

use bleedthrough_syntax_conformance::Corpus;
use bleedthrough_syntax_conformance::corpus::render_ledger;
use bleedthrough_syntax_conformance::interpreter::{obtain, record, unverified, verify};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let docker = args.iter().any(|a| a == "--docker");
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let (Some(command), root) = (positional.first(), positional.get(1)) else {
        eprintln!("usage: ledger (record|verify) [--docker] [CORPUS]");
        return ExitCode::from(2);
    };
    let root = root.map_or_else(|| PathBuf::from("fixtures/php"), PathBuf::from);
    let corpus = match Corpus::load(&root) {
        Ok(c) => c,
        Err(e) if command.as_str() == "record" && e.starts_with("ledger.toml") => {
            // A first recording starts from an empty ledger.
            let _ = std::fs::write(root.join("ledger.toml"), "");
            Corpus::load(&root).expect("the corpus loads once an empty ledger exists")
        }
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let obtained = obtain(&corpus, docker);
    for (version, reason) in unverified(&obtained) {
        println!("PHP {version}: unverified ({reason})");
    }
    match command.as_str() {
        "record" => {
            let ledger = record(&obtained, &corpus.ledger);
            std::fs::write(root.join("ledger.toml"), render_ledger(&ledger))
                .expect("ledger.toml is writable");
            println!("ledger.toml written");
            ExitCode::SUCCESS
        }
        "verify" => {
            let failures = verify(&corpus, &obtained);
            for f in &failures {
                println!("{f}");
            }
            if failures.is_empty() {
                println!("ledger agrees with every interpreter that was run");
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        other => {
            eprintln!("unknown command {other}");
            ExitCode::from(2)
        }
    }
}
