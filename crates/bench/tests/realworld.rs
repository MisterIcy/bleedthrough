//! The pinned list, hash-verified fetching with safe extraction, and the parse-failure budget.

use std::io::Write;
use std::path::PathBuf;

use bleedthrough_bench::realworld::{
    Budgets, List, Observed, Project, install, is_fetched, php_files, sha256_hex, tree_dir,
};

fn entry(id: &str, tier: &str, style: &str) -> String {
    format!(
        "[[project]]\nid = \"{id}\"\nrepository = \"https://github.com/o/{id}\"\ncommit = \"{c}\"\nsha256 = \"{h}\"\nlicence = \"MIT\"\nphp = [\"8.2\"]\ntier = \"{tier}\"\nstyle = \"{style}\"\nreason = \"r\"\n",
        c = "a".repeat(40),
        h = "b".repeat(64)
    )
}

fn good_list() -> String {
    [
        entry("s", "small", "framework"),
        entry("m", "medium", "non-framework"),
        entry("l", "large", "legacy"),
    ]
    .join("\n")
}

#[test]
fn a_complete_list_passes() {
    assert_eq!(
        List::parse(&good_list()).expect("parses").check(),
        Vec::<String>::new()
    );
}

#[test]
fn a_missing_hash_names_the_project() {
    let broken = good_list().replacen(&format!("sha256 = \"{}\"\n", "b".repeat(64)), "", 1);
    let errors = List::parse(&broken).expect("parses").check();
    assert!(
        errors
            .iter()
            .any(|e| e.contains("`s`") && e.contains("SHA-256")),
        "{errors:?}"
    );
}

#[test]
fn a_missing_commit_or_licence_names_the_project() {
    let broken = good_list().replacen("licence = \"MIT\"\n", "", 1);
    assert!(
        List::parse(&broken)
            .expect("parses")
            .check()
            .iter()
            .any(|e| e.contains("`s`") && e.contains("licence"))
    );
    let broken = good_list().replacen(
        &format!("commit = \"{}\"\n", "a".repeat(40)),
        "commit = \"main\"\n",
        1,
    );
    assert!(
        List::parse(&broken)
            .expect("parses")
            .check()
            .iter()
            .any(|e| e.contains("`s`") && e.contains("commit"))
    );
}

#[test]
fn tiers_and_styles_must_be_covered() {
    let no_large = [
        entry("s", "small", "framework"),
        entry("m", "medium", "non-framework"),
        entry("l", "medium", "legacy"),
    ]
    .join("\n");
    assert!(
        List::parse(&no_large)
            .expect("parses")
            .check()
            .iter()
            .any(|e| e.contains("`large` tier"))
    );
    let no_legacy = [
        entry("s", "small", "framework"),
        entry("m", "medium", "non-framework"),
        entry("l", "large", "framework"),
    ]
    .join("\n");
    assert!(
        List::parse(&no_legacy)
            .expect("parses")
            .check()
            .iter()
            .any(|e| e.contains("`legacy` project"))
    );
}

// A tar archive written byte by byte, so that headers `tar::Builder` refuses to write (traversal) can
// be produced.
fn header(name: &str, size: usize, kind: u8, link: &str) -> [u8; 512] {
    let mut h = [0u8; 512];
    h[..name.len()].copy_from_slice(name.as_bytes());
    h[100..107].copy_from_slice(b"0000644");
    h[108..115].copy_from_slice(b"0000000");
    h[116..123].copy_from_slice(b"0000000");
    h[124..135].copy_from_slice(format!("{size:011o}").as_bytes());
    h[136..147].copy_from_slice(b"00000000000");
    h[156] = kind;
    h[157..157 + link.len()].copy_from_slice(link.as_bytes());
    h[257..263].copy_from_slice(b"ustar\0");
    h[263..265].copy_from_slice(b"00");
    h[148..156].copy_from_slice(b"        ");
    let sum: u32 = h.iter().map(|b| u32::from(*b)).sum();
    h[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    h
}

/// Name, body, tar type flag and link target of one entry.
type Entry<'a> = (&'a str, &'a [u8], u8, &'a str);

fn archive(entries: &[Entry<'_>]) -> Vec<u8> {
    let mut tar = Vec::new();
    for (name, body, kind, link) in entries {
        tar.extend_from_slice(&header(name, body.len(), *kind, link));
        tar.extend_from_slice(body);
        tar.resize(tar.len().div_ceil(512) * 512, 0);
    }
    tar.extend_from_slice(&[0u8; 1024]);
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(&tar).expect("compresses");
    gz.finish().expect("finishes")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bt-realworld-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

fn project_for(archive: &[u8]) -> Project {
    let mut p = List::parse(&entry("p", "small", "framework"))
        .expect("parses")
        .projects
        .remove(0);
    p.sha256 = sha256_hex(archive);
    p
}

#[test]
fn a_good_archive_is_verified_extracted_and_cached() {
    let cache = scratch("good");
    let bytes = archive(&[
        ("top/", b"", b'5', ""),
        ("top/src/a.php", b"<?php echo 1;", b'0', ""),
        ("top/readme.md", b"x", b'0', ""),
    ]);
    let project = project_for(&bytes);
    assert!(!is_fetched(&cache, &project));
    assert_eq!(install(&cache, &project, &bytes).expect("installs"), 2);
    assert!(is_fetched(&cache, &project));
    assert_eq!(
        php_files(&tree_dir(&cache, &project)).expect("walks"),
        vec!["src/a.php".to_string()]
    );
}

#[test]
fn a_hash_mismatch_fails_naming_both_hashes_and_keeps_the_verified_tree() {
    let cache = scratch("mismatch");
    let good = archive(&[("top/a.php", b"<?php", b'0', "")]);
    let project = project_for(&good);
    install(&cache, &project, &good).expect("installs");
    let altered = archive(&[("top/a.php", b"<?php evil();", b'0', "")]);
    let error = install(&cache, &project, &altered).expect_err("refused");
    assert!(
        error.contains("`p`")
            && error.contains(&project.sha256)
            && error.contains(&sha256_hex(&altered)),
        "{error}"
    );
    assert!(
        is_fetched(&cache, &project),
        "the verified tree is untouched"
    );
    assert_eq!(
        std::fs::read(tree_dir(&cache, &project).join("a.php")).expect("file"),
        b"<?php"
    );
}

#[test]
fn path_traversal_is_refused_and_writes_nothing() {
    let cache = scratch("traversal");
    let bytes = archive(&[
        ("top/ok.php", b"<?php", b'0', ""),
        ("top/../../escaped.php", b"<?php", b'0', ""),
    ]);
    let project = project_for(&bytes);
    let error = install(&cache, &project, &bytes).expect_err("refused");
    assert!(
        error.contains("unsafe path") && error.contains("escaped.php"),
        "{error}"
    );
    assert!(!is_fetched(&cache, &project));
    assert!(
        !cache.join("escaped.php").exists()
            && !cache.parent().expect("parent").join("escaped.php").exists()
    );
    assert!(
        !tree_dir(&cache, &project).exists(),
        "nothing is left behind"
    );
}

#[test]
fn absolute_paths_and_escaping_links_are_refused() {
    let cache = scratch("links");
    let absolute = archive(&[("/etc/passwd", b"x", b'0', "")]);
    assert!(install(&cache, &project_for(&absolute), &absolute).is_err());
    let escaping = archive(&[("top/link", b"", b'2', "../../outside")]);
    let error = install(&cache, &project_for(&escaping), &escaping).expect_err("refused");
    assert!(
        error.contains("symlink") && error.contains("outside"),
        "{error}"
    );
    let absolute_link = archive(&[("top/link", b"", b'2', "/etc")]);
    assert!(install(&cache, &project_for(&absolute_link), &absolute_link).is_err());
    let hard = archive(&[("top/a", b"x", b'0', ""), ("top/h", b"", b'1', "top/a")]);
    assert!(install(&cache, &project_for(&hard), &hard).is_err());
    let inside = archive(&[
        ("top/src/a.php", b"<?php", b'0', ""),
        ("top/src/link", b"", b'2', "a.php"),
    ]);
    assert!(
        install(&cache, &project_for(&inside), &inside).is_ok(),
        "a link inside the tree is fine"
    );
}

fn budgets() -> Budgets {
    Budgets::parse("[project.p]\nrecovered = 1\nfailed = 1\n\n[project.p.excluded]\n\"bad.php\" = \"php -l: Parse error\"\n").expect("parses")
}

fn observed(recovered: &[&str], failed: &[&str]) -> Observed {
    let own = |s: &[&str]| s.iter().map(|f| (*f).to_string()).collect();
    Observed {
        recovered: own(recovered),
        failed: own(failed),
    }
}

#[test]
fn a_budget_holds_when_counts_match_and_excluded_files_do_not_count() {
    assert!(
        budgets()
            .check("p", &observed(&["a.php"], &["b.php", "bad.php"]))
            .is_empty()
    );
}

#[test]
fn a_parser_regression_lists_the_files() {
    let errors = budgets().check(
        "p",
        &observed(&["a.php"], &["b.php", "c.php", "d.php", "e.php"]),
    );
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0].contains("regressed") && errors[0].contains("budget 1, now 4"),
        "{errors:?}"
    );
    for file in ["b.php", "c.php", "d.php", "e.php"] {
        assert!(errors[0].contains(file));
    }
}

#[test]
fn a_parser_improvement_fails_until_the_budget_is_lowered() {
    let errors = budgets().check("p", &observed(&["a.php"], &[]));
    assert!(
        errors[0].contains("improved") && errors[0].contains("--update"),
        "{errors:?}"
    );
}

#[test]
fn a_project_without_a_budget_is_reported_and_budgets_round_trip() {
    assert!(budgets().check("other", &observed(&[], &[]))[0].contains("no parse budget"));
    let b = budgets();
    assert_eq!(Budgets::parse(&b.render()).expect("round trip"), b);
}
