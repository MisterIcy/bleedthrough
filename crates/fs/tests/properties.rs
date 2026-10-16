//! The file readers of untrusted project input: rule files and Composer files never panic on arbitrary
//! bytes or near-valid mutations; they report problems instead.

use bleedthrough_fs::{ComposerProject, MemFs, load_rules};
use bleedthrough_testkit::proptest::prelude::*;
use bleedthrough_testkit::{byte_vec, check, mutated};

const RULE: &str = r#"schema = 1

[[rule]]
id = "BT0001"
title = "SQL injection"
description = "Tainted data reaches an SQL query."
remediation = "Use prepared statements."
cwe = ["CWE-89"]
owasp = ["OWASP:2021:A03"]

[rule.metrics]
attack_vector = "network"
attack_complexity = "low"
privileges_required = "none"
user_interaction = "none"
scope = "unchanged"
confidentiality = "high"
integrity = "high"
availability = "high"

[rule.taint]
sources = ["$_GET", "$_POST"]

[[rule.taint.sinks]]
symbol = "mysqli_query"
args = [2]
class = "sql"
"#;

const MANIFEST: &str = r#"{"autoload":{"psr-4":{"App\\":"src/"},"classmap":["gen/"],"files":["a.php"]},"config":{"vendor-dir":"vendor"}}"#;
const LOCK: &str = r#"{"packages":[{"name":"a/b","version":"1.0.0","autoload":{"psr-4":{"A\\":"src/"}}}],"packages-dev":[]}"#;

fn load_text(bytes: &[u8]) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("rules.toml"), bytes).unwrap();
    // Either the rules load or the problems are reported; a panic fails the property.
    let _ = load_rules(dir.path());
}

#[test]
fn rule_files_of_arbitrary_bytes_are_rejected_or_loaded_without_panics() {
    check(byte_vec(300), |bytes| {
        load_text(&bytes);
        Ok(())
    });
}

#[test]
fn near_valid_rule_files_are_rejected_or_loaded_without_panics() {
    check(mutated(Just(RULE.to_string())), |bytes| {
        load_text(&bytes);
        Ok(())
    });
}

#[test]
fn composer_files_of_arbitrary_text_give_diagnostics_not_panics() {
    check((byte_vec(200), byte_vec(200)), |(manifest, lock)| {
        let fs = MemFs::new()
            .with_file("composer.json", &String::from_utf8_lossy(&manifest))
            .with_file("composer.lock", &String::from_utf8_lossy(&lock));
        let _ = ComposerProject::read(&fs);
        Ok(())
    });
}

#[test]
fn near_valid_composer_files_give_diagnostics_not_panics() {
    check(
        (
            mutated(Just(MANIFEST.to_string())),
            mutated(Just(LOCK.to_string())),
        ),
        |(manifest, lock)| {
            let fs = MemFs::new()
                .with_file("composer.json", &String::from_utf8_lossy(&manifest))
                .with_file("composer.lock", &String::from_utf8_lossy(&lock))
                .with_file("src/X.php", "<?php");
            let _ = ComposerProject::read(&fs);
            Ok(())
        },
    );
}

#[test]
fn the_unmutated_rule_file_loads_so_the_mutations_reach_the_parser() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("rules.toml"), RULE).unwrap();
    let loaded = load_rules(dir.path()).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(loaded.set.len(), 1);
}
