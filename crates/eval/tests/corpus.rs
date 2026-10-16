//! The completeness and label checks of the corpus, one test per failure scenario of the spec.

mod common;

use bleedthrough_eval::corpus::Corpus;
use common::*;

fn errors(name: &str, cases: &str, setup: impl FnOnce(&std::path::Path)) -> Vec<String> {
    let root = scratch(name);
    setup(&root);
    corpus(&root, cases).check()
}

fn has(errors: &[String], needle: &str) -> bool {
    errors.iter().any(|e| e.contains(needle))
}

#[test]
fn a_correct_pair_passes() {
    let e = errors("ok", &pair(EXPECT), pair_files);
    assert!(e.is_empty(), "{e:?}");
}

#[test]
fn the_real_corpus_passes() {
    let corpus = Corpus::load(&real_root()).expect("loads");
    assert!(corpus.check().is_empty(), "{:?}", corpus.check());
    assert!(corpus.cases.len() >= 100);
}

#[test]
fn orphan_file_is_named() {
    let e = errors("orphan", &pair(EXPECT), |r| {
        pair_files(r);
        write(r, "a/stray.php", "<?php\n");
    });
    assert!(
        has(&e, "file `a/stray.php` is not listed by any case"),
        "{e:?}"
    );
}

#[test]
fn missing_file_is_named() {
    let e = errors("missing", &pair(EXPECT), |r| {
        write(r, "a/vuln.php", "<?php\n$a = 1;\n$b = 2;\n");
    });
    assert!(has(&e, "case `s`: file `a/safe.php` is missing"), "{e:?}");
}

#[test]
fn duplicate_ids_are_reported() {
    let cases = format!("{}{}", pair(EXPECT), pair(EXPECT));
    let e = errors("dup", &cases, pair_files);
    assert!(has(&e, "duplicate case id `v`"), "{e:?}");
}

#[test]
fn unpaired_vulnerable_case_fails() {
    let cases = pair(EXPECT).replace("twin = \"s\"\n", "");
    let e = errors("unpaired", &cases, pair_files);
    assert!(
        has(&e, "case `v`: has no twin and is not a standalone negative"),
        "{e:?}"
    );
}

#[test]
fn twin_must_reference_back_and_have_the_other_label() {
    let cases = pair(EXPECT).replace("twin = \"v\"", "twin = \"s\"");
    let e = errors("twinback", &cases, pair_files);
    assert!(
        has(&e, "has the same label") || has(&e, "does not reference it back"),
        "{e:?}"
    );
}

#[test]
fn standalone_negative_needs_no_twin_but_a_reason() {
    let ok = format!(
        "{}\n[[case]]\nid = \"n\"\ncwe = \"CWE-89\"\nlabel = \"safe\"\ndescription = \"looks dangerous\"\nphp = \"8.0+\"\nshapes = [\"direct\"]\nstandalone = \"a constant query\"\nprovenance = \"original\"\nlicence = \"MIT\"\nfiles = [\"a/n.php\"]\n",
        pair(EXPECT)
    );
    let setup = |r: &std::path::Path| {
        pair_files(r);
        write(r, "a/n.php", "<?php\n$a = 1;\n");
    };
    assert!(errors("standalone-ok", &ok, setup).is_empty());
    let bad = ok.replace("standalone = \"a constant query\"", "standalone = \"\"");
    assert!(has(
        &errors("standalone-bad", &bad, setup),
        "standalone negative has no reason"
    ));
}

#[test]
fn unknown_cwe_is_named() {
    let cases = pair(EXPECT).replace("CWE-89", "CWE-99999");
    let e = errors("cwe", &cases, pair_files);
    assert!(has(&e, "case `v`: weakness `CWE-99999`"), "{e:?}");
}

#[test]
fn unknown_cwe_in_an_expectation_is_named() {
    let expect = format!("{EXPECT}cwe = \"CWE-99999\"\n");
    let e = errors("cwe-expect", &pair(&expect), pair_files);
    assert!(has(&e, "expected finding `CWE-99999`"), "{e:?}");
}

#[test]
fn vulnerable_case_without_expectation_fails() {
    let e = errors("noexpect", &pair(""), pair_files);
    assert!(
        has(
            &e,
            "case `v`: is vulnerable but declares no expected finding"
        ),
        "{e:?}"
    );
}

#[test]
fn range_outside_the_file_fails() {
    let expect = EXPECT.replace("[3, 3]", "[3, 9]");
    let e = errors("range", &pair(&expect), pair_files);
    assert!(
        has(&e, "lines 3-9 are outside `a/vuln.php` (3 lines)"),
        "{e:?}"
    );
}

#[test]
fn known_gap_needs_reason_and_tracking() {
    let cases = pair(EXPECT).replace(
        "files = [\"a/vuln.php\"]",
        "files = [\"a/vuln.php\"]\nknown_gap = { reason = \"\", tracking = \"\" }",
    );
    let e = errors("gap", &cases, pair_files);
    assert!(
        has(
            &e,
            "case `v`: known gap needs a reason and a tracking reference"
        ),
        "{e:?}"
    );
}

#[test]
fn oversized_file_is_named() {
    let big = format!("<?php\n{}", "$a = 1;\n".repeat(150));
    let e = errors("big", &pair(EXPECT), |r| {
        pair_files(r);
        write(r, "a/vuln.php", &big);
    });
    assert!(
        has(&e, "`a/vuln.php` has 151 lines; the limit is 100"),
        "{e:?}"
    );
}

#[test]
fn case_depending_on_a_package_is_rejected() {
    let e = errors("package", &pair(EXPECT), |r| {
        pair_files(r);
        write(
            r,
            "a/vuln.php",
            "<?php\nrequire 'vendor/autoload.php';\n$b = 2;\n",
        );
    });
    assert!(has(&e, "depends on an installed package"), "{e:?}");
}

#[test]
fn licence_must_be_allowed() {
    let cases = pair(EXPECT).replacen("licence = \"MIT\"", "licence = \"GPL-3.0-only\"", 1);
    let e = errors("licence", &cases, pair_files);
    assert!(
        has(&e, "licence `GPL-3.0-only` is not on the allow-list"),
        "{e:?}"
    );
}

#[test]
fn a_broken_copy_of_the_real_corpus_fails() {
    let root = scratch("broken-copy");
    copy_dir(&real_root(), &root);
    assert!(Corpus::load(&root).expect("loads").check().is_empty());
    std::fs::remove_file(root.join("cwe-89/cwe89-direct/index.php")).expect("remove");
    let e = Corpus::load(&root).expect("loads").check();
    assert!(
        has(&e, "file `cwe-89/cwe89-direct/index.php` is missing"),
        "{e:?}"
    );
}

#[test]
fn coverage_lists_a_missing_wrong_sanitizer_case() {
    use bleedthrough_eval::coverage::{coverage, render};
    let cases = pair(EXPECT);
    let root = scratch("coverage");
    pair_files(&root);
    let corpus = corpus(&root, &cases);
    let report = coverage(&corpus);
    assert_eq!(report[0].missing, vec!["wrong-sanitizer".to_string()]);
    assert!(render(&report, &corpus.shapes).contains("CWE-89 has no case for: wrong-sanitizer"));
}
