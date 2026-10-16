//! Every file of the vulnerability corpus is valid PHP: the Mago backend parses it cleanly and, where
//! an interpreter is available, `php -l` accepts it. A case that does not parse proves nothing about
//! the analysis, so this runs before anything is scored.

use std::path::{Path, PathBuf};
use std::process::Command;

use bleedthrough_eval::corpus::Corpus;
use bleedthrough_syntax::{ParseOptions, ParseQuality, ParserBackend};
use bleedthrough_syntax_conformance::interpreter::{PHP_FLAGS, local_version};
use bleedthrough_syntax_mago::MagoBackend;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/vuln")
}

fn files(corpus: &Corpus) -> Vec<String> {
    corpus.cases.iter().flat_map(|c| c.files.clone()).collect()
}

#[test]
fn every_case_file_parses_cleanly() {
    let corpus = Corpus::load(&root()).expect("the corpus loads");
    let backend = MagoBackend::new();
    let mut bad = Vec::new();
    for file in files(&corpus) {
        let bytes = std::fs::read(root().join(&file)).expect("case file");
        let parsed = backend.parse(&file, &bytes, &ParseOptions::default());
        if parsed.quality() != ParseQuality::Clean {
            bad.push(format!("{file}: {:?}", parsed.quality()));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn every_case_file_passes_php_lint_when_an_interpreter_exists() {
    let Some((minor, full)) = local_version() else {
        println!("no local php: skipping the interpreter check");
        return;
    };
    println!("php -l with PHP {full} ({minor})");
    let corpus = Corpus::load(&root()).expect("the corpus loads");
    let mut bad = Vec::new();
    for file in files(&corpus) {
        let output = Command::new("php")
            .args(PHP_FLAGS.split_whitespace().skip(1))
            .arg(root().join(&file))
            .output()
            .expect("run php");
        if !output.status.success() {
            bad.push(format!(
                "{file}: {}",
                String::from_utf8_lossy(&output.stdout).trim()
            ));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
