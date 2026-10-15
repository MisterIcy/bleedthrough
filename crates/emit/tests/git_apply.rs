//! External-tool check: the produced patch applies with `git apply` and makes the edits.

mod common;

use std::process::Command;

use bleedthrough_app::Format;

fn git(dir: &std::path::Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git is installed");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn check(report: &bleedthrough_findings::Report, expected: &[(&str, &str)]) {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in &common::sources().0 {
        let file = dir.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    git(dir.path(), &["init", "--quiet"]);
    let (patch, _) = common::emit(Format::Diff, report);
    std::fs::write(dir.path().join("fixes.patch"), &patch).unwrap();
    git(dir.path(), &["apply", "--check", "fixes.patch"]);
    git(dir.path(), &["apply", "fixes.patch"]);
    for (path, text) in expected {
        assert_eq!(
            &std::fs::read_to_string(dir.path().join(path)).unwrap(),
            text,
            "{path}"
        );
    }
}

#[test]
fn mixed_patch_applies_cleanly_including_a_file_without_trailing_newline() {
    check(
        &common::mixed(),
        &[
            (
                "lib/Shell.php",
                "<?php\necho shell_exec(escapeshellarg($_GET['cmd']));",
            ),
            (
                "src/Db.php",
                &common::DB_TEXT.replace("mysqli_query", "mysqli_execute"),
            ),
        ],
    );
}

#[test]
fn full_patch_applies() {
    check(
        &common::full(),
        &[(
            "src/Db.php",
            &common::DB_TEXT.replace("$_GET['id']", "(int) $_GET['id']"),
        )],
    );
}

#[test]
fn patch_for_a_path_with_spaces_colons_and_non_ascii_applies() {
    check(
        &common::hostile(),
        &[(
            common::HOSTILE,
            &common::HOSTILE_TEXT.replace("$_GET['a']", "htmlspecialchars($_GET['a'])"),
        )],
    );
}
