//! The cross-check: the generated data against the Reflection fixtures of every version, apart
//! from the reviewed allowances in `stubs/oracle-allow.toml`.

#![allow(missing_docs, reason = "test fixtures need no documentation")]

use std::path::Path;

use bleedthrough_builtins_gen::oracle::{apply, check_repository};

#[test]
fn the_data_agrees_with_reflection_apart_from_reviewed_allowances() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (differences, allowances) = check_repository(&root).expect("the repository files read");
    let outcome = apply(&differences, &allowances);
    let mut problems: Vec<String> = outcome
        .unexplained
        .iter()
        .map(|d| format!("not allowed: {d}"))
        .collect();
    problems.extend(outcome.stale.iter().map(|a| {
        format!(
            "stale allowance for `{}` ({}): it matches no difference; remove it",
            a.symbol, a.reason
        )
    }));
    assert!(
        problems.is_empty(),
        "{} problem(s):\n{}",
        problems.len(),
        problems
            .iter()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
