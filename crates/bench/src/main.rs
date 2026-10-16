//! The benchmark tooling's command line: `gen` writes a synthetic tree, `compare` compares two
//! result documents. `cargo xtask bench-compare` and the scheduled workflow drive it.

use std::path::PathBuf;
use std::process::ExitCode;

use bleedthrough_bench::compare::{Options, compare};
use bleedthrough_bench::realworld::{List, cache_dir, install, is_fetched, tree_dir};
use bleedthrough_bench::result::Document;
use bleedthrough_bench::stage::{EndToEnd, Stage};
use bleedthrough_bench::synth::{Generator, Params};

const USAGE: &str = "\
usage: bleedthrough-bench <command>

commands:
  gen <parameters> --out DIR     write a synthetic tree (and labels.toml when vuln_pm > 0);
                                 <parameters> is `seed=1,files=200,...` (see docs/benchmarks.md)
  hash <parameters>              print files, bytes and the tree hash without writing anything
  merge OUT IN...                merge result documents measured on one machine (samples are
                                 concatenated) into OUT
  check-corpora [--list FILE]    check the pinned real-world list (offline): pins, licences, tiers
  fetch [--list FILE] [--tier T] [--project ID]
                                 download, verify and extract pinned projects into the cache
  robustness [--binary PATH]     scan each fetched project twice with different worker counts;
                                 reports `unavailable` while there is no scan or no fetched project
  compare BASE HEAD [--threshold F] [--k F] [--force]
                                 compare two result documents; exit 1 on a regression";

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn load_list(rest: &[String]) -> Result<List, String> {
    let path = flag(rest, "--list").unwrap_or_else(|| "corpora/real-world.toml".to_string());
    List::parse(&std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?)
}

/// Downloads a project's archive with `curl` (as `cargo xtask vendor-builtins` does) into memory.
fn download(url: &str) -> Result<Vec<u8>, String> {
    let output = std::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "600", url])
        .output()
        .map_err(|e| format!("cannot run curl: {e}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(format!(
            "downloading {url} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn fetch(rest: &[String]) -> Result<u8, String> {
    let list = load_list(rest)?;
    let problems = list.check();
    if !problems.is_empty() {
        problems.iter().for_each(|e| eprintln!("error: {e}"));
        return Err("the real-world list is invalid; nothing was fetched".to_string());
    }
    let (tier, only) = (flag(rest, "--tier"), flag(rest, "--project"));
    let cache = cache_dir(std::path::Path::new("."));
    std::fs::create_dir_all(&cache).map_err(|e| format!("{}: {e}", cache.display()))?;
    let mut failed = false;
    for project in list.projects.iter().filter(|p| {
        tier.as_ref().is_none_or(|t| *t == p.tier) && only.as_ref().is_none_or(|i| *i == p.id)
    }) {
        if is_fetched(&cache, project) {
            println!("fetch-corpus: `{}` already verified", project.id);
            continue;
        }
        match download(&project.archive_url()).and_then(|bytes| install(&cache, project, &bytes)) {
            Ok(files) => println!("fetch-corpus: `{}` verified, {files} files", project.id),
            Err(e) => {
                eprintln!("error: {e}");
                failed = true;
            }
        }
    }
    Ok(u8::from(failed))
}

/// The robustness and determinism check. It needs a scan command, which does not exist yet, so it
/// reports why it cannot run (exit 3) instead of passing.
fn robustness(rest: &[String]) -> Result<u8, String> {
    let list = load_list(rest)?;
    let cache = cache_dir(std::path::Path::new("."));
    let fetched: Vec<_> = list
        .projects
        .iter()
        .filter(|p| is_fetched(&cache, p))
        .collect();
    if fetched.is_empty() {
        println!(
            "robustness: unavailable: no real-world project is fetched (run `cargo xtask fetch-corpus`)"
        );
        return Ok(3);
    }
    let binary = flag(rest, "--binary").ok_or("robustness needs --binary PATH")?;
    let probe = EndToEnd::new(binary.clone().into(), tree_dir(&cache, fetched[0]));
    if let Some(reason) = probe.unavailable() {
        println!("robustness: unavailable: {reason}");
        return Ok(3);
    }
    // The scan exists: the check below is the contract the scan change must satisfy.
    let mut failed = false;
    for project in fetched {
        match probe_project(&binary, &tree_dir(&cache, project)) {
            Ok(()) => println!("robustness: `{}` ok", project.id),
            Err(e) => {
                eprintln!("error: project `{}`: {e}", project.id);
                failed = true;
            }
        }
    }
    Ok(u8::from(failed))
}

/// Scans `dir` twice, with one and with four workers, and requires byte-identical JSON reports, no
/// panic, and completion within the time limit.
fn probe_project(binary: &str, dir: &std::path::Path) -> Result<(), String> {
    const LIMIT: std::time::Duration = std::time::Duration::from_secs(600);
    let run = |jobs: &str| -> Result<Vec<u8>, String> {
        let started = std::time::Instant::now();
        let output = std::process::Command::new(binary)
            .arg("scan")
            .arg(dir)
            .args(["--format", "json", "--jobs", jobs])
            .output()
            .map_err(|e| format!("cannot run the binary: {e}"))?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.code().is_none() || stderr.contains("panicked at") {
            return Err(format!(
                "the scan crashed or panicked: {}",
                stderr
                    .lines()
                    .find(|l| l.contains("panicked"))
                    .unwrap_or("signal")
            ));
        }
        if started.elapsed() > LIMIT {
            return Err(format!("the scan took longer than {} s", LIMIT.as_secs()));
        }
        Ok(output.stdout)
    };
    let (first, second) = (run("1")?, run("4")?);
    if first != second {
        let at = first
            .iter()
            .zip(&second)
            .position(|(a, b)| a != b)
            .unwrap_or(first.len().min(second.len()));
        return Err(format!(
            "reports differ between worker counts; first difference at byte {at}"
        ));
    }
    if run("1")? != first {
        return Err("two runs with the same worker count differ".to_string());
    }
    Ok(())
}

fn run(args: &[String]) -> Result<u8, String> {
    let (command, rest) = args.split_first().ok_or(USAGE)?;
    match command.as_str() {
        "gen" | "hash" => {
            let params = Params::parse(rest.first().map_or("", String::as_str))?;
            let generator = Generator::new(params);
            let stats = if command == "gen" {
                let out = flag(rest, "--out").ok_or("gen needs --out DIR")?;
                generator.write(&PathBuf::from(out))?
            } else {
                generator.tree_hash()
            };
            println!(
                "files={} bytes={} hash={}",
                stats.files, stats.bytes, stats.hash
            );
            Ok(0)
        }
        "check-corpora" => {
            let problems = load_list(rest)?.check();
            problems.iter().for_each(|e| eprintln!("error: {e}"));
            if problems.is_empty() {
                println!("check-corpora: ok");
            }
            Ok(u8::from(!problems.is_empty()))
        }
        "fetch" => fetch(rest),
        "robustness" => robustness(rest),
        "merge" => {
            let [out, inputs @ ..] = rest else {
                return Err(USAGE.to_string());
            };
            let docs = inputs
                .iter()
                .map(|path| {
                    std::fs::read_to_string(path)
                        .map_err(|e| format!("{path}: {e}"))
                        .and_then(|text| {
                            Document::from_json(&text).map_err(|e| format!("{path}: {e}"))
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            std::fs::write(out, Document::merge(&docs)?.to_json())
                .map_err(|e| format!("{out}: {e}"))?;
            Ok(0)
        }
        "compare" => {
            let [base, head, ..] = rest else {
                return Err(USAGE.to_string());
            };
            let read = |path: &String| {
                std::fs::read_to_string(path)
                    .map_err(|e| format!("{path}: {e}"))
                    .and_then(|text| Document::from_json(&text).map_err(|e| format!("{path}: {e}")))
            };
            let mut options = Options {
                force: rest.iter().any(|a| a == "--force"),
                ..Options::default()
            };
            let number = |name: &str| {
                flag(rest, name)
                    .map(|v| {
                        v.parse::<f64>()
                            .map_err(|_| format!("{name} needs a number"))
                    })
                    .transpose()
            };
            if let Some(t) = number("--threshold")? {
                options.threshold = t;
            }
            if let Some(k) = number("--k")? {
                options.k = k;
            }
            let comparison = compare(&read(base)?, &read(head)?, &options)?;
            print!("{}", comparison.report());
            Ok(u8::from(!comparison.regressions().is_empty()))
        }
        other => Err(format!("unknown command `{other}`\n{USAGE}")),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}
