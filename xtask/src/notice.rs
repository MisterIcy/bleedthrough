//! The `NOTICE` file: the licence texts of the vendored php-src files, generated from them so the
//! texts can never drift from the sources (`cargo xtask gen-notice`, checked by `check-builtins`).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use crate::manifest::{Release, VENDOR_DIR};

pub const NOTICE_PATH: &str = "NOTICE";

const PREAMBLE: &str = "\
Bleedthrough NOTICE
===================

The built-in symbol data distributed with Bleedthrough (stubs/builtin.data) is derived from the
`*.stub.php` files of the PHP source code (https://github.com/php/php-src), releases {RELEASES}.
Those files are distributed under the licence texts reproduced below; the files themselves are
vendored unmodified under third_party/php-src/.

The data holds facts only (names, types, flags, version availability, default expression text and
constant values), not documentation prose. This notice is included as a conservative measure; it
is not legal advice and the licensing position of the derived data is recorded in
docs/licensing-builtins.md.
";

/// One distinct licence text and where it appears.
struct Group {
    text: String,
    /// Path inside a release tree to the tags that carry exactly this text there.
    places: BTreeMap<String, Vec<String>>,
}

fn groups(root: &Path, releases: &[Release]) -> Result<Vec<Group>, String> {
    let mut by_text: BTreeMap<String, Group> = BTreeMap::new();
    for release in releases {
        for path in release
            .files
            .keys()
            .filter(|p| p.rsplit('/').next() == Some("LICENSE"))
        {
            let file = root.join(VENDOR_DIR).join(&release.tag).join(path);
            let text = fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            let group = by_text.entry(text.clone()).or_insert_with(|| Group {
                text,
                places: BTreeMap::new(),
            });
            group
                .places
                .entry(path.clone())
                .or_default()
                .push(release.tag.clone());
        }
    }
    let mut groups: Vec<Group> = by_text.into_values().collect();
    groups.sort_by_key(|g| {
        let (path, tags) = g
            .places
            .iter()
            .next()
            .map(|(p, t)| (p.clone(), t.clone()))
            .unwrap_or_default();
        (path, tags)
    });
    Ok(groups)
}

/// The NOTICE text for the vendored releases under `root`.
pub fn render(root: &Path, releases: &[Release]) -> Result<String, String> {
    let tags: Vec<&str> = releases.iter().map(|r| r.tag.as_str()).collect();
    let mut out = PREAMBLE.replace("{RELEASES}", &tags.join(", "));
    for group in groups(root, releases)? {
        let _ = write!(
            out,
            "\n{}\nLicence file(s) in the vendored releases:\n",
            "=".repeat(70)
        );
        for (path, tags) in &group.places {
            let _ = writeln!(out, "  - {path}: {}", tags.join(", "));
        }
        let _ = writeln!(out, "{}\n", "-".repeat(70));
        out.push_str(&group.text);
        if !group.text.ends_with('\n') {
            out.push('\n');
        }
    }
    Ok(out)
}

/// Writes NOTICE under `root`.
pub fn write(root: &Path, releases: &[Release]) -> Result<(), String> {
    fs::write(root.join(NOTICE_PATH), render(root, releases)?).map_err(|e| e.to_string())
}

/// Checks that NOTICE exists and equals what the vendored licence files require.
pub fn verify(root: &Path, releases: &[Release]) -> Result<Vec<String>, String> {
    if releases.is_empty() {
        return Ok(Vec::new());
    }
    let expected = render(root, releases)?;
    Ok(match fs::read_to_string(root.join(NOTICE_PATH)) {
        Err(_) => vec![format!(
            "{NOTICE_PATH} is missing; run `cargo xtask gen-notice`"
        )],
        Ok(actual) if actual == expected => Vec::new(),
        Ok(_) => vec![format!(
            "{NOTICE_PATH} does not match the vendored licence files; run `cargo xtask gen-notice`"
        )],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, licence: &str) -> (Release, String) {
        let release = Release {
            php: "8.0".into(),
            tag: tag.into(),
            commit: "a".repeat(40),
            files: BTreeMap::from([("LICENSE".to_string(), String::new())]),
        };
        (release, licence.to_string())
    }

    fn scratch() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("xtask-notice-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_notice_carries_each_distinct_text_verbatim_and_detects_drift() {
        let dir = scratch();
        let (a, text_a) = release(
            "php-8.0.30",
            "The PHP License, version 3.01\nCopyright 2021\n",
        );
        let (b, text_b) = release(
            "php-8.2.34",
            "The PHP License, version 3.01\nCopyright 2024\n",
        );
        let (c, _) = release(
            "php-8.3.35",
            "The PHP License, version 3.01\nCopyright 2024\n",
        );
        for (r, text) in [(&a, &text_a), (&b, &text_b), (&c, &text_b)] {
            let path = dir.join(VENDOR_DIR).join(&r.tag);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("LICENSE"), text).unwrap();
        }
        let releases = [a, b, c];
        let notice = render(&dir, &releases).unwrap();
        assert!(notice.contains(&text_a) && notice.contains(&text_b));
        assert!(
            notice.contains("LICENSE: php-8.2.34, php-8.3.35"),
            "{notice}"
        );
        assert!(verify(&dir, &releases).unwrap()[0].contains("missing"));
        write(&dir, &releases).unwrap();
        assert!(verify(&dir, &releases).unwrap().is_empty());
        fs::write(dir.join(NOTICE_PATH), notice.replace("2024", "2025")).unwrap();
        assert!(verify(&dir, &releases).unwrap()[0].contains("does not match"));
        let _ = fs::remove_dir_all(dir);
    }
}
