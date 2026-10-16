//! Prints how the generated data differs from the Reflection fixtures, before any allowance.
//!
//! ```text
//! cargo run -p bleedthrough-builtins-gen --example oracle_report -- [--all]
//! ```
//!
//! Run from the repository root. Without `--all` it prints the number of differences per version
//! and per kind of difference; with it, every difference.

use std::collections::BTreeMap;
use std::path::Path;

use bleedthrough_builtins_gen::oracle::check_repository;

fn main() {
    let all = std::env::args().any(|a| a == "--all");
    let (differences, _) = match check_repository(Path::new(".")) {
        Ok(found) => found,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    let mut groups: BTreeMap<String, usize> = BTreeMap::new();
    for d in &differences {
        let field: String = d
            .field
            .chars()
            .filter(|c| !c.is_ascii_digit() && *c != '$')
            .collect();
        *groups
            .entry(format!("PHP {} {field}", d.version))
            .or_default() += 1;
        if all {
            println!("{d}");
        }
    }
    for (group, count) in groups {
        println!("{count:6}  {group}");
    }
    println!("{:6}  total", differences.len());
}
