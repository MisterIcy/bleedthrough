//! Generated trees use only constructs the Mago backend parses without errors.

use bleedthrough_bench::synth::{Generator, Params};
use bleedthrough_syntax::{ParseOptions, ParseQuality, ParserBackend};
use bleedthrough_syntax_mago::MagoBackend;

#[test]
fn a_generated_tree_parses_with_no_errors() {
    let params = Params {
        seed: 9,
        files: 400,
        vuln_pm: 150,
        ..Params::default()
    };
    let backend = MagoBackend::new();
    let (mut parsed, mut bad) = (0, Vec::new());
    Generator::new(params).walk(&mut |file, _| {
        let result = backend.parse(
            &file.path,
            file.content.as_bytes(),
            &ParseOptions::default(),
        );
        parsed += 1;
        if result.quality() != ParseQuality::Clean {
            bad.push(format!("{}: {:?}", file.path, result.quality()));
        }
    });
    assert!(parsed >= 400);
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
