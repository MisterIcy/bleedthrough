//! Commands that drive the tooling crates (`bleedthrough-eval`, `bleedthrough-bench`).
//!
//! xtask may not depend on them (it is declared with no workspace dependencies), so each command
//! starts them through `cargo run`, the same way the built-in data generator is started. Exit
//! statuses of the tools are passed through, which is how `eval` reports 0, 1 and 3.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The directory cargo builds into.
fn target_dir(root: &Path) -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from)
}

/// Runs `cargo run -p <package> [--example <name>] -- <args>` in the repository root and returns the
/// process's exit status (101 when it could not be started).
pub fn cargo_run(root: &Path, package: &str, example: Option<&str>, args: &[String]) -> i32 {
    let mut command = Command::new("cargo");
    command.args(["run", "--quiet", "--locked", "-p", package]);
    if let Some(example) = example {
        command.args(["--example", example]);
    }
    command.arg("--").args(args).current_dir(root);
    match command.status() {
        Ok(status) => status.code().unwrap_or(101),
        Err(e) => {
            eprintln!("error: cannot run cargo: {e}");
            101
        }
    }
}

/// Builds the `bleedthrough` binary and returns its path.
pub fn build_binary(root: &Path) -> Result<PathBuf, String> {
    let status = Command::new("cargo")
        .args(["build", "--quiet", "--locked", "-p", "bleedthrough-cli"])
        .current_dir(root)
        .status()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    if !status.success() {
        return Err("building the bleedthrough binary failed".to_string());
    }
    Ok(target_dir(root).join("debug").join("bleedthrough"))
}

/// `cargo xtask check-corpus`: offline completeness and label checks of `fixtures/vuln`.
pub fn check_corpus(root: &Path) -> i32 {
    let args = [
        "check-corpus".to_string(),
        "--root".into(),
        "fixtures/vuln".into(),
    ];
    cargo_run(root, "bleedthrough-eval", None, &args)
}

/// `cargo xtask eval [options]`: scores the binary's report; 0 pass, 1 fail, 3 not evaluated.
pub fn eval(root: &Path, rest: &[String]) -> i32 {
    let binary = match build_binary(root) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    let mut args = vec![
        "eval".to_string(),
        "--root".into(),
        "fixtures/vuln".into(),
        "--binary".into(),
        binary.to_string_lossy().into_owned(),
    ];
    args.extend(rest.iter().cloned());
    cargo_run(root, "bleedthrough-eval", None, &args)
}

/// `cargo xtask bench-counters [--update]`: allocation and input counters of the parse stage against
/// `benchmarks/baselines.toml`. Deterministic and offline; wall time is never checked here.
pub fn bench_counters(root: &Path, rest: &[String]) -> i32 {
    let mut args = vec!["--baseline".to_string(), "benchmarks/baselines.toml".into()];
    args.extend(rest.iter().filter(|a| a.as_str() == "--update").cloned());
    cargo_run(root, "bleedthrough-syntax-mago", Some("bench_alloc"), &args)
}

/// How many alternating base/head rounds `bench-compare` measures.
const COMPARE_ROUNDS: usize = 3;

fn run_checked(command: &mut Command, what: &str) -> Result<(), String> {
    match command.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("{what} failed ({status})")),
        Err(e) => Err(format!("{what}: {e}")),
    }
}

/// A detached checkout of `revision` with its release example built; removed on drop.
struct Checkout {
    root: PathBuf,
    dir: PathBuf,
}

impl Checkout {
    fn new(root: &Path, revision: &str, dir: PathBuf) -> Result<Self, String> {
        let _ = Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&dir)
            .current_dir(root)
            .output();
        run_checked(
            Command::new("git")
                .args(["worktree", "add", "--detach"])
                .arg(&dir)
                .arg(revision)
                .current_dir(root),
            &format!("checking out `{revision}`"),
        )?;
        let checkout = Self {
            root: root.to_path_buf(),
            dir,
        };
        if !checkout
            .dir
            .join("adapters/syntax-mago/examples/bench_parse.rs")
            .exists()
        {
            return Err(format!(
                "`{revision}` has no benchmark harness (adapters/syntax-mago/examples/bench_parse.rs); compare commits after it"
            ));
        }
        run_checked(
            Command::new("cargo")
                .args([
                    "build",
                    "--release",
                    "--quiet",
                    "-p",
                    "bleedthrough-syntax-mago",
                    "--example",
                    "bench_parse",
                ])
                .current_dir(&checkout.dir),
            &format!("building `{revision}`"),
        )?;
        Ok(checkout)
    }

    fn measure(&self, out: &Path, extra: &[String]) -> Result<(), String> {
        let mut command = Command::new(self.dir.join("target/release/examples/bench_parse"));
        command
            .args(extra)
            .arg("--out")
            .arg(out)
            .current_dir(&self.dir);
        run_checked(&mut command, "measuring")
    }
}

impl Drop for Checkout {
    fn drop(&mut self) {
        let _ = Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&self.dir)
            .current_dir(&self.root)
            .output();
    }
}

/// `cargo xtask bench-compare <base> <head> [--input SPEC] [--force] [--threshold F] [--k F]`.
///
/// Builds both revisions in throwaway worktrees, measures them in alternating rounds on this machine
/// (so drift hits both equally), merges the rounds and compares. Exit 1 on a regression.
pub fn bench_compare(root: &Path, rest: &[String]) -> i32 {
    match bench_compare_inner(root, rest) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            2
        }
    }
}

fn bench_compare_inner(root: &Path, rest: &[String]) -> Result<i32, String> {
    let [base, head, options @ ..] = rest else {
        return Err("usage: cargo xtask bench-compare <base> <head> [--input SPEC] [--force] [--threshold F] [--k F]".into());
    };
    let work = target_dir(root).join("bench-compare");
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let base_checkout = Checkout::new(root, base, work.join("base"))?;
    let head_checkout = Checkout::new(root, head, work.join("head"))?;
    let measure_args: Vec<String> = options
        .windows(2)
        .filter(|w| w[0] == "--input")
        .flat_map(|w| w.to_vec())
        .chain(["--iterations".to_string(), "10".into()])
        .collect();
    let (mut base_docs, mut head_docs) = (Vec::new(), Vec::new());
    for round in 0..COMPARE_ROUNDS {
        for (checkout, docs, label) in [
            (&base_checkout, &mut base_docs, "base"),
            (&head_checkout, &mut head_docs, "head"),
        ] {
            let out = work.join(format!("{label}-{round}.json"));
            checkout.measure(&out, &measure_args)?;
            docs.push(out);
        }
    }
    let merge = |docs: &[PathBuf], out: PathBuf| -> Result<PathBuf, String> {
        let mut args = vec!["merge".to_string(), out.display().to_string()];
        args.extend(docs.iter().map(|d| d.display().to_string()));
        match cargo_run(root, "bleedthrough-bench", None, &args) {
            0 => Ok(out),
            _ => Err("merging the rounds failed".into()),
        }
    };
    let base_doc = merge(&base_docs, work.join("base.json"))?;
    let head_doc = merge(&head_docs, work.join("head.json"))?;
    let mut args = vec![
        "compare".to_string(),
        base_doc.display().to_string(),
        head_doc.display().to_string(),
    ];
    args.extend(options.iter().filter(|a| a.as_str() != "--input").cloned());
    // `--input SPEC` was consumed above; drop its value too.
    if let Some(value) = options
        .iter()
        .position(|a| a == "--input")
        .and_then(|i| options.get(i + 1))
    {
        args.retain(|a| a != value);
    }
    Ok(cargo_run(root, "bleedthrough-bench", None, &args))
}

/// `cargo xtask check-corpora`: the pinned real-world list (offline, no fetching).
pub fn check_corpora(root: &Path) -> i32 {
    cargo_run(
        root,
        "bleedthrough-bench",
        None,
        &["check-corpora".to_string()],
    )
}

/// `cargo xtask fetch-corpus [--tier T] [--project ID]`: downloads, verifies and extracts the pinned
/// projects into `$BLEEDTHROUGH_CORPUS_DIR` or `target/corpus/`. Never run by required CI.
pub fn fetch_corpus(root: &Path, rest: &[String]) -> i32 {
    let mut args = vec!["fetch".to_string()];
    args.extend(rest.iter().cloned());
    cargo_run(root, "bleedthrough-bench", None, &args)
}

/// `cargo xtask parse-budget [--project ID] [--update]`: parse failures on fetched projects against
/// `corpora/parse-budget.toml`; exit 3 when nothing is fetched.
pub fn parse_budget(root: &Path, rest: &[String]) -> i32 {
    cargo_run(root, "bleedthrough-syntax-mago", Some("parse_budget"), rest)
}

/// `cargo xtask check-robustness`: scans fetched projects twice with different worker counts; exit 3
/// (unavailable) while there is no scan command or nothing is fetched.
pub fn check_robustness(root: &Path, rest: &[String]) -> i32 {
    let binary = match build_binary(root) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    let mut args = vec![
        "robustness".to_string(),
        "--binary".into(),
        binary.display().to_string(),
    ];
    args.extend(rest.iter().cloned());
    cargo_run(root, "bleedthrough-bench", None, &args)
}
