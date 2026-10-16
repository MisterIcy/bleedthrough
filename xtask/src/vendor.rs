//! `cargo xtask vendor-php-src`: copies the stub files of one php-src release into
//! `third_party/php-src/<tag>/` and records them in `stubs/MANIFEST.toml`. It uses the network and
//! is never part of a gate; verification (`check-builtins`) needs none.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::manifest::{self, MANIFEST_PATH, Release, SOURCE, VENDOR_DIR, sha256_hex};

pub const HELP: &str = "\
usage: cargo xtask vendor-php-src --tag <php-8.x.y> [--root <dir>] [--archive <file> --commit <sha>]

Fetches the release (network) and vendors its *.stub.php files and the LICENSE files above them
under <root>/third_party/php-src/<tag>/, then updates <root>/stubs/MANIFEST.toml. <root> defaults
to the repository; give a scratch directory to try it without touching the repository.
--archive uses a local .tar.gz of the tree at --commit instead of the network.";

struct Args {
    tag: String,
    root: PathBuf,
    archive: Option<PathBuf>,
    commit: Option<String>,
}

fn parse_args(rest: &[String], default_root: &Path) -> Result<Args, String> {
    let (mut tag, mut root, mut archive, mut commit) = (None, None, None, None);
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let mut value = || it.next().cloned().ok_or(format!("{flag} needs a value"));
        match flag.as_str() {
            "--tag" => tag = Some(value()?),
            "--root" => root = Some(PathBuf::from(value()?)),
            "--archive" => archive = Some(PathBuf::from(value()?)),
            "--commit" => commit = Some(value()?),
            other => return Err(format!("unknown argument `{other}`\n{HELP}")),
        }
    }
    let tag = tag.ok_or(format!("--tag is required\n{HELP}"))?;
    Ok(Args {
        tag,
        root: root.unwrap_or_else(|| default_root.to_path_buf()),
        archive,
        commit,
    })
}

/// `php-8.5.11` -> `8.5`.
fn php_version(tag: &str) -> Result<String, String> {
    let numbers = tag.strip_prefix("php-").unwrap_or_default();
    let mut parts = numbers.split('.');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("8"), Some(minor), Some(patch), None)
            if [minor, patch]
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())) =>
        {
            Ok(format!("8.{minor}"))
        }
        _ => Err(format!("`{tag}` is not a php-8.x.y release tag")),
    }
}

fn run(program: &str, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run {program}: {e}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// The commit a tag points at (peeling an annotated tag).
fn resolve_commit(tag: &str) -> Result<String, String> {
    let reference = format!("refs/tags/{tag}");
    let peeled = format!("{reference}^{{}}");
    let out = run(
        "git",
        &["ls-remote", &format!("{SOURCE}.git"), &reference, &peeled],
    )?;
    let text = String::from_utf8_lossy(&out);
    let mut direct = None;
    for line in text.lines() {
        let Some((sha, name)) = line.split_once('\t') else {
            continue;
        };
        if name == peeled {
            return Ok(sha.to_string());
        }
        if name == reference {
            direct = Some(sha.to_string());
        }
    }
    direct.ok_or_else(|| format!("tag {tag} not found in {SOURCE}"))
}

fn is_full_sha(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn files_under(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            files_under(&path, base, out)?;
        } else if let Ok(relative) = path.strip_prefix(base) {
            out.push(relative.to_path_buf());
        }
    }
    Ok(())
}

/// The stub files of an extracted tree plus the LICENSE files in any directory above a stub.
fn select(tree: &Path) -> Result<Vec<PathBuf>, String> {
    let mut all = Vec::new();
    files_under(tree, tree, &mut all)?;
    let stubs: Vec<&PathBuf> = all
        .iter()
        .filter(|p| p.to_string_lossy().ends_with(".stub.php"))
        .collect();
    let mut chosen: Vec<PathBuf> = stubs.iter().map(|p| (*p).clone()).collect();
    for stub in &stubs {
        for dir in stub.ancestors().skip(1) {
            let licence = dir.join("LICENSE");
            if tree.join(&licence).is_file() && !chosen.contains(&licence) {
                chosen.push(licence);
            }
        }
    }
    chosen.sort();
    if stubs.is_empty() {
        return Err("the archive holds no *.stub.php file".to_string());
    }
    Ok(chosen)
}

fn extract(archive: &Path, into: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(into).map_err(|e| e.to_string())?;
    run(
        "tar",
        &[
            "-xzf",
            &archive.to_string_lossy(),
            "-C",
            &into.to_string_lossy(),
            "--strip-components=1",
        ],
    )?;
    Ok(into.to_path_buf())
}

fn fetch(commit: &str, scratch: &Path) -> Result<PathBuf, String> {
    let archive = scratch.join("source.tar.gz");
    let url = format!("https://codeload.github.com/php/php-src/tar.gz/{commit}");
    run("curl", &["-fsSL", "-o", &archive.to_string_lossy(), &url])?;
    Ok(archive)
}

fn vendor(args: &Args) -> Result<Release, String> {
    let php = php_version(&args.tag)?;
    let scratch = std::env::temp_dir().join(format!("vendor-php-src-{}", std::process::id()));
    let _ = fs::remove_dir_all(&scratch);
    fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    let result = (|| {
        let (archive, commit) = match (&args.archive, &args.commit) {
            (Some(archive), Some(commit)) => (archive.clone(), commit.clone()),
            (Some(_), None) => return Err("--archive needs --commit".to_string()),
            (None, given) => {
                let commit = given
                    .clone()
                    .map_or_else(|| resolve_commit(&args.tag), Ok)?;
                (fetch(&commit, &scratch)?, commit)
            }
        };
        if !is_full_sha(&commit) {
            return Err(format!("`{commit}` is not a full commit SHA"));
        }
        let tree = extract(&archive, &scratch.join("tree"))?;
        let selected = select(&tree)?;
        let target = args.root.join(VENDOR_DIR).join(&args.tag);
        let _ = fs::remove_dir_all(&target);
        let mut files = std::collections::BTreeMap::new();
        for relative in selected {
            let bytes = fs::read(tree.join(&relative)).map_err(|e| e.to_string())?;
            let destination = target.join(&relative);
            fs::create_dir_all(destination.parent().unwrap_or(&target))
                .map_err(|e| e.to_string())?;
            fs::write(&destination, &bytes).map_err(|e| e.to_string())?;
            files.insert(
                relative.to_string_lossy().replace('\\', "/"),
                sha256_hex(&bytes),
            );
        }
        Ok(Release {
            php,
            tag: args.tag.clone(),
            commit,
            files,
        })
    })();
    let _ = fs::remove_dir_all(&scratch);
    result
}

/// Runs the command; returns the lines to print.
pub fn run_command(rest: &[String], default_root: &Path) -> Result<Vec<String>, String> {
    let args = parse_args(rest, default_root)?;
    let release = vendor(&args)?;
    let mut releases = manifest::load(&args.root)?;
    releases.retain(|r| r.php != release.php && r.tag != release.tag);
    let summary = format!(
        "vendored {} ({}): {} files, commit {}",
        release.tag,
        release.php,
        release.files.len(),
        release.commit
    );
    releases.push(release);
    let path = args.root.join(MANIFEST_PATH);
    fs::create_dir_all(path.parent().unwrap_or(&args.root)).map_err(|e| e.to_string())?;
    fs::write(&path, manifest::render(&releases)).map_err(|e| e.to_string())?;
    Ok(vec![summary, format!("updated {}", path.display())])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_map_to_versions() {
        assert_eq!(php_version("php-8.5.11").unwrap(), "8.5");
        assert_eq!(php_version("php-8.0.30").unwrap(), "8.0");
        for bad in [
            "8.5.11",
            "php-7.4.1",
            "php-8.5",
            "php-8.5.11-rc1",
            "php-8..1",
        ] {
            assert!(php_version(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn full_shas_only() {
        assert!(is_full_sha(&"ab".repeat(20)));
        assert!(!is_full_sha("abc"));
        assert!(!is_full_sha(&"AB".repeat(20)));
    }

    #[test]
    fn selection_takes_stubs_and_the_licences_above_them() {
        let dir = std::env::temp_dir().join(format!("xtask-select-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        for (path, body) in [
            ("LICENSE", "root"),
            ("Zend/LICENSE", "zend"),
            ("Zend/zend.stub.php", "<?php"),
            ("Zend/other.c", "int x;"),
            ("TSRM/LICENSE", "unrelated"),
            ("ext/a/a.stub.php", "<?php"),
            ("ext/a/lib/LICENSE", "library"),
            ("sapi/fpm/LICENSE", "fpm"),
            ("sapi/fpm/fpm/fpm_main.stub.php", "<?php"),
        ] {
            let p = dir.join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, body).unwrap();
        }
        let chosen: Vec<String> = select(&dir)
            .unwrap()
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        assert_eq!(
            chosen,
            [
                "LICENSE",
                "Zend/LICENSE",
                "Zend/zend.stub.php",
                "ext/a/a.stub.php",
                "sapi/fpm/LICENSE",
                "sapi/fpm/fpm/fpm_main.stub.php"
            ]
        );
        let _ = fs::remove_dir_all(dir);
    }
}
