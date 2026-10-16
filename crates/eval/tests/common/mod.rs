//! Shared helpers: scratch corpora under the target directory.
#![allow(
    dead_code,
    reason = "each test binary uses a different subset of the helpers"
)]

use std::fs;
use std::path::{Path, PathBuf};

use bleedthrough_eval::corpus::Corpus;

/// The repository's real corpus.
pub fn real_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/vuln")
}

/// A fresh empty directory for one test.
pub fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("eval-tests")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Writes a file (creating parents).
pub fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("parent")).expect("create dirs");
    fs::write(path, text).expect("write file");
}

/// A manifest header plus `cases` (raw TOML), parsed against `root`.
pub fn corpus(root: &Path, cases: &str) -> Corpus {
    let manifest =
        format!("shapes = [\"direct\", \"wrong-sanitizer\", \"correct-sanitizer\"]\n{cases}");
    Corpus::parse(root, &manifest).expect("manifest parses")
}

/// A twin pair for CWE-89 over `a/vuln.php` and `a/safe.php`, each three lines long.
pub fn pair(extra_vuln: &str) -> String {
    format!(
        r#"
[[case]]
id = "v"
cwe = "CWE-89"
label = "vulnerable"
description = "d"
php = "8.0+"
shapes = ["direct"]
twin = "s"
fix = "prepared statement"
provenance = "original"
licence = "MIT"
files = ["a/vuln.php"]
{extra_vuln}

[[case]]
id = "s"
cwe = "CWE-89"
label = "safe"
description = "d"
php = "8.0+"
shapes = ["correct-sanitizer"]
twin = "v"
provenance = "original"
licence = "MIT"
files = ["a/safe.php"]
"#
    )
}

/// Writes the two files of [`pair`].
pub fn pair_files(root: &Path) {
    write(root, "a/vuln.php", "<?php\n$a = 1;\n$b = 2;\n");
    write(root, "a/safe.php", "<?php\n$a = 1;\n$b = 2;\n");
}

/// The default expectation block for [`pair`].
pub const EXPECT: &str =
    "[[case.expect]]\nfile = \"a/vuln.php\"\nlines = [3, 3]\nmin_confidence = \"medium\"\n";

/// Recursively copies a directory.
pub fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create dir");
    for entry in fs::read_dir(from).expect("read dir") {
        let path = entry.expect("entry").path();
        let target = to.join(path.file_name().expect("name"));
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            fs::copy(&path, &target).expect("copy");
        }
    }
}
