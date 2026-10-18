//! The benchmark `analysis` stage over a synthetic tree with planted vulnerabilities.

use bleedthrough_bench::analysis_stage::{AnalysisSetup, AnalysisStage};
use bleedthrough_bench::parse_stage::SourceFile;
use bleedthrough_bench::stage::Stage;
use bleedthrough_bench::synth::{Generator, Params};
use bleedthrough_engine_suite::engine::{Gate, RuleIndex};
use bleedthrough_engine_suite::{real_builtins, workspace_root};
use bleedthrough_syntax_mago::MagoBackend;

#[test]
fn the_stage_reports_file_unit_and_finding_counts_deterministically() {
    let root = workspace_root().join("crates/engine/tests/rules");
    let rules = bleedthrough_fs::load_rules(&root).expect("rules").set;
    let index = RuleIndex::build(&rules, &Gate::default());
    let setup = AnalysisSetup {
        builtins: real_builtins("8.3").summary(),
        rules: &rules,
        index: &index,
    };
    let params = Params::parse("seed=1,files=20,vuln_pm=300").expect("parameters");
    let mut files = Vec::new();
    Generator::new(params).walk(&mut |f, _| {
        files.push(SourceFile {
            path: f.path.clone(),
            bytes: f.content.clone().into_bytes(),
        });
    });
    let backend = MagoBackend::new();
    let mut stage = AnalysisStage::new(&backend, &files, &setup);
    assert_eq!(stage.name(), "analysis");
    let first = stage.counters();
    assert_eq!(first.get("files"), Some(&(files.len() as u64)));
    assert!(first["units"] >= first["files"], "{first:?}");
    assert!(first["findings"] > 0, "{first:?}");
    stage.pass();
    assert_eq!(stage.counters(), first);
}
