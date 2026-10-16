//! The ledger recorder and verifier: asks real PHP interpreters (`php -l`) for their verdicts.
//!
//! An interpreter is either the local `php` or the official `php:<version>-cli` image run through
//! Docker. A version that cannot be obtained is reported as unverified with the reason, never as
//! verified.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use crate::corpus::{Corpus, Ledger, VERSIONS, Verdict, VersionInfo};
use crate::suite::{Failure, failure};

/// Flags passed to every interpreter: no ini file (so the compiled defaults apply, whatever the
/// host has installed) and short tags off, matching the backends' default.
pub const PHP_FLAGS: &str = "php -n -d short_open_tag=0 -l";

/// Where a PHP version can be run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Runner {
    /// The `php` found on the path.
    Local,
    /// The official image for the version, through Docker.
    Docker,
}

fn run(command: &mut Command) -> Result<String, String> {
    let output = command.output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// The major.minor version of the local `php`, if there is one.
pub fn local_version() -> Option<(String, String)> {
    let version = run(Command::new("php").args([
        "-n",
        "-r",
        "echo PHP_MAJOR_VERSION.'.'.PHP_MINOR_VERSION;",
    ]))
    .ok()?;
    let full = run(Command::new("php").args(["-n", "-r", "echo PHP_VERSION;"])).ok()?;
    Some((version, full))
}

/// Output of one interpreter run: a verdict per file, keyed by path relative to the directory.
pub fn parse_report(report: &str) -> Result<BTreeMap<String, Verdict>, String> {
    let mut out = BTreeMap::new();
    for line in report.lines().filter(|l| !l.trim().is_empty()) {
        let (path, code) = line
            .rsplit_once(' ')
            .ok_or_else(|| format!("unreadable line {line:?}"))?;
        let path = path.trim_start_matches("./").to_string();
        let verdict = match code.trim() {
            "0" => Verdict::Valid,
            "255" => Verdict::Invalid,
            other => return Err(format!("{path}: unexpected exit status {other}")),
        };
        out.insert(path, verdict);
    }
    Ok(out)
}

const SCRIPT: &str = "cd /w && for f in $(find . -name '*.php' | sort); do php -n -d short_open_tag=0 -l \"$f\" >/dev/null 2>&1; echo \"$f $?\"; done";

fn copy_fixtures(corpus: &Corpus, into: &Path) -> Result<(), String> {
    for fixture in &corpus.fixtures {
        let target = into.join(&fixture.path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::copy(corpus.root.join(&fixture.path), &target)
            .map_err(|e| format!("{}: {e}", fixture.path))?;
    }
    Ok(())
}

/// The image for `version`: the digest pinned in the corpus's `images.toml`, else the floating tag.
pub fn image_for(corpus: &Corpus, version: &str) -> String {
    std::fs::read_to_string(corpus.root.join("images.toml"))
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .and_then(|root| {
            root.get("images")?
                .get(version)?
                .as_str()
                .map(str::to_string)
        })
        .unwrap_or_else(|| format!("php:{version}-cli"))
}

/// Runs `php -l` over a temporary copy of every fixture with the interpreter for `version`.
pub fn run_version(
    corpus: &Corpus,
    version: &str,
    runner: &Runner,
) -> Result<BTreeMap<String, Verdict>, String> {
    let dir = std::env::temp_dir().join(format!(
        "bleedthrough-ledger-{}-{version}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let result = (|| {
        copy_fixtures(corpus, &dir)?;
        let report = match runner {
            Runner::Local => run(Command::new("sh")
                .arg("-c")
                .arg(SCRIPT.replace("cd /w", &format!("cd {:?}", dir.display().to_string()))))?,
            Runner::Docker => run(Command::new("docker").args([
                "run",
                "--rm",
                "-v",
                &format!("{}:/w:ro", dir.display()),
                &image_for(corpus, version),
                "sh",
                "-c",
                SCRIPT,
            ]))?,
        };
        parse_report(&report)
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// The date as `YYYY-MM-DD` (UTC), computed from the system clock.
pub fn today() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// What was obtained for each version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Obtained {
    /// The interpreter ran; verdicts per fixture.
    Verdicts {
        /// The interpreter that produced them.
        interpreter: String,
        /// A verdict per fixture path.
        verdicts: BTreeMap<String, Verdict>,
    },
    /// No interpreter could be run.
    Unavailable {
        /// Why not.
        reason: String,
    },
}

/// Tries to obtain verdicts for every version: the local `php` for its own version, Docker for the rest
/// when `docker` is set.
pub fn obtain(corpus: &Corpus, docker: bool) -> BTreeMap<String, Obtained> {
    obtain_with(corpus, docker, local_version())
}

/// [`obtain`] with the local interpreter given explicitly, so both cases can be tested.
pub fn obtain_with(
    corpus: &Corpus,
    docker: bool,
    local: Option<(String, String)>,
) -> BTreeMap<String, Obtained> {
    let mut out = BTreeMap::new();
    for version in VERSIONS {
        let from_docker = |corpus: &Corpus| {
            run_version(corpus, version, &Runner::Docker).map(|verdicts| Obtained::Verdicts {
                interpreter: format!("official image {}", image_for(corpus, version)),
                verdicts,
            })
        };
        let from_local = |corpus: &Corpus, full: &str| {
            run_version(corpus, version, &Runner::Local).map(|verdicts| Obtained::Verdicts {
                interpreter: format!("PHP {full} (local)"),
                verdicts,
            })
        };
        let local_here = local.as_ref().filter(|(v, _)| v == version);
        // Docker is preferred when asked for, so the committed ledger and CI use the same images.
        let outcome = match (docker, local_here) {
            (true, Some((_, full))) => from_docker(corpus).or_else(|_| from_local(corpus, full)),
            (true, None) => from_docker(corpus),
            (false, Some((_, full))) => from_local(corpus, full),
            (false, None) => Err(
                "no local interpreter for this version and Docker was not requested".to_string(),
            ),
        };
        out.insert(
            version.to_string(),
            outcome.unwrap_or_else(|reason| Obtained::Unavailable { reason }),
        );
    }
    out
}

/// Builds a ledger from what was obtained; unobtainable versions are recorded as unverified.
pub fn record(obtained: &BTreeMap<String, Obtained>, previous: &Ledger) -> Ledger {
    let mut ledger = Ledger {
        flags: PHP_FLAGS.to_string(),
        ..Ledger::default()
    };
    for (version, what) in obtained {
        match what {
            Obtained::Verdicts {
                interpreter,
                verdicts,
            } => {
                ledger.versions.insert(
                    version.clone(),
                    VersionInfo {
                        interpreter: interpreter.clone(),
                        recorded: today(),
                        verified: true,
                        reason: String::new(),
                    },
                );
                for (path, verdict) in verdicts {
                    ledger
                        .verdicts
                        .entry(path.clone())
                        .or_default()
                        .insert(version.clone(), *verdict);
                }
            }
            Obtained::Unavailable { reason } => {
                ledger.versions.insert(
                    version.clone(),
                    VersionInfo {
                        interpreter: String::new(),
                        recorded: String::new(),
                        verified: false,
                        reason: reason.clone(),
                    },
                );
                // Keep earlier verdicts for the version; they stay unverified.
                for (path, row) in &previous.verdicts {
                    if let Some(verdict) = row.get(version) {
                        ledger
                            .verdicts
                            .entry(path.clone())
                            .or_default()
                            .insert(version.clone(), *verdict);
                    }
                }
            }
        }
    }
    ledger
}

/// Compares fresh interpreter output with the committed ledger. Versions that could not be
/// obtained are not failures; the caller lists them as unverified.
pub fn verify(corpus: &Corpus, obtained: &BTreeMap<String, Obtained>) -> Vec<Failure> {
    let mut out = Vec::new();
    for (version, what) in obtained {
        let Obtained::Verdicts { verdicts, .. } = what else {
            continue;
        };
        for (path, found) in verdicts {
            let recorded = corpus
                .ledger
                .verdicts
                .get(path)
                .and_then(|row| row.get(version));
            if recorded != Some(found) {
                let was = recorded.map_or("nothing", |v| v.as_str());
                out.push(failure(
                    path,
                    "ledger-drift",
                    format!(
                        "PHP {version}: the interpreter says {}, the ledger says {was}",
                        found.as_str()
                    ),
                ));
            }
        }
    }
    out
}

/// Versions that could not be obtained, with the reason.
pub fn unverified(obtained: &BTreeMap<String, Obtained>) -> Vec<(String, String)> {
    obtained
        .iter()
        .filter_map(|(v, o)| match o {
            Obtained::Unavailable { reason } => Some((v.clone(), reason.clone())),
            Obtained::Verdicts { .. } => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_map_to_verdicts() {
        let report = "./v8.0/a.php 0\n./malformed/b.php 255\n";
        let parsed = parse_report(report).unwrap();
        assert_eq!(parsed["v8.0/a.php"], Verdict::Valid);
        assert_eq!(parsed["malformed/b.php"], Verdict::Invalid);
    }

    #[test]
    fn an_unexpected_exit_status_is_an_error_not_a_verdict() {
        assert!(parse_report("./a.php 1\n").is_err());
        assert!(parse_report("garbage").is_err());
    }

    fn tiny_corpus() -> (std::path::PathBuf, Corpus) {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("bleedthrough-interp-{}-{n}", std::process::id()));
        std::fs::create_dir_all(dir.join("v7.0")).unwrap();
        std::fs::write(dir.join("v7.0/a.php"), "<?php echo 1;\n").unwrap();
        std::fs::write(dir.join("v7.0/b.php"), "<?php echo ;;; (\n").unwrap();
        std::fs::write(
            dir.join("manifest.toml"),
            "breakage_classes = []\n[[fixture]]\npath = \"v7.0/a.php\"\ngroup = \"valid\"\n[[fixture]]\npath = \"v7.0/b.php\"\ngroup = \"valid\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("ledger.toml"), "").unwrap();
        let corpus = Corpus::load(&dir).unwrap();
        (dir, corpus)
    }

    #[test]
    fn without_any_interpreter_every_version_is_unverified_with_a_reason() {
        let (dir, corpus) = tiny_corpus();
        let obtained = obtain_with(&corpus, false, None);
        let _ = std::fs::remove_dir_all(dir);
        assert_eq!(obtained.len(), VERSIONS.len());
        let missing = unverified(&obtained);
        assert_eq!(missing.len(), VERSIONS.len());
        assert!(missing.iter().all(|(_, reason)| !reason.is_empty()));
        let ledger = record(&obtained, &Ledger::default());
        assert!(
            ledger
                .versions
                .values()
                .all(|v| !v.verified && !v.reason.is_empty())
        );
    }

    #[test]
    #[ignore = "needs a local PHP interpreter; run with `cargo test -- --ignored` where php is installed"]
    fn the_local_interpreter_gives_real_verdicts_when_php_is_installed() {
        let Some((version, full)) = local_version() else {
            eprintln!("php is not installed here; skipping");
            return;
        };
        let (dir, corpus) = tiny_corpus();
        let obtained = obtain_with(&corpus, false, Some((version.clone(), full)));
        let _ = std::fs::remove_dir_all(dir);
        let Obtained::Verdicts { verdicts, .. } = &obtained[&version] else {
            panic!("the local version was not run");
        };
        assert_eq!(verdicts["v7.0/a.php"], Verdict::Valid);
        assert_eq!(verdicts["v7.0/b.php"], Verdict::Invalid);
        // Every other version is unverified, never silently verified.
        assert_eq!(unverified(&obtained).len(), VERSIONS.len() - 1);
    }

    #[test]
    fn a_disagreeing_interpreter_is_reported_with_both_verdicts() {
        let (dir, mut corpus) = tiny_corpus();
        let _ = std::fs::remove_dir_all(dir);
        corpus
            .ledger
            .verdicts
            .entry("v7.0/a.php".into())
            .or_default()
            .insert("7.0".into(), Verdict::Invalid);
        let mut verdicts = BTreeMap::new();
        verdicts.insert("v7.0/a.php".to_string(), Verdict::Valid);
        let mut obtained = BTreeMap::new();
        obtained.insert(
            "7.0".to_string(),
            Obtained::Verdicts {
                interpreter: "x".into(),
                verdicts,
            },
        );
        let drift = verify(&corpus, &obtained);
        assert_eq!(drift.len(), 1);
        assert_eq!(drift[0].subject, "v7.0/a.php");
        assert!(drift[0].message.contains("valid") && drift[0].message.contains("invalid"));
        assert!(drift[0].message.contains("7.0"));
    }

    #[test]
    fn a_pinned_digest_wins_over_the_floating_tag() {
        let (dir, corpus) = tiny_corpus();
        assert_eq!(image_for(&corpus, "8.4"), "php:8.4-cli");
        std::fs::write(
            dir.join("images.toml"),
            "[images]\n\"8.4\" = \"php@sha256:abc\"\n",
        )
        .unwrap();
        assert_eq!(image_for(&corpus, "8.4"), "php@sha256:abc");
        assert_eq!(image_for(&corpus, "7.0"), "php:7.0-cli");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn today_has_the_iso_shape() {
        let t = today();
        assert_eq!(t.len(), 10);
        assert_eq!(t.as_bytes()[4], b'-');
    }

    #[test]
    fn an_unobtainable_version_is_recorded_unverified_and_never_verified() {
        let mut obtained = BTreeMap::new();
        obtained.insert(
            "8.5".to_string(),
            Obtained::Unavailable {
                reason: "no image".into(),
            },
        );
        let ledger = record(&obtained, &Ledger::default());
        let info = &ledger.versions["8.5"];
        assert!(!info.verified);
        assert_eq!(info.reason, "no image");
        assert_eq!(
            unverified(&obtained),
            vec![("8.5".to_string(), "no image".to_string())]
        );
    }

    #[test]
    fn a_verified_version_is_recorded_with_interpreter_and_date() {
        let mut verdicts = BTreeMap::new();
        verdicts.insert("a.php".to_string(), Verdict::Valid);
        let mut obtained = BTreeMap::new();
        obtained.insert(
            "8.4".to_string(),
            Obtained::Verdicts {
                interpreter: "PHP 8.4.1".into(),
                verdicts,
            },
        );
        let ledger = record(&obtained, &Ledger::default());
        assert!(ledger.versions["8.4"].verified);
        assert_eq!(ledger.versions["8.4"].interpreter, "PHP 8.4.1");
        assert_eq!(ledger.verdicts["a.php"]["8.4"], Verdict::Valid);
    }
}
