//! What the benchmark examples share: the analysis stage's inputs.

use bleedthrough_bench::analysis_stage::AnalysisSetup;
use bleedthrough_engine_suite::engine::{Gate, RuleIndex};
use bleedthrough_engine_suite::rules::RuleSet;
use bleedthrough_engine_suite::{real_builtins, workspace_root};

/// The engine's test rules, loaded from `crates/engine/tests/rules`.
pub fn rules() -> RuleSet {
    let root = workspace_root().join("crates/engine/tests/rules");
    match bleedthrough_fs::load_rules(&root) {
        Ok(loaded) => loaded.set,
        Err(errors) => {
            let lines: Vec<String> = errors.iter().map(ToString::to_string).collect();
            panic!("the test rules are invalid:\n{}", lines.join("\n"));
        }
    }
}

/// The analysis setup over `rules` and their `index`, for PHP 8.3.
pub fn setup<'a>(rules: &'a RuleSet, index: &'a RuleIndex) -> AnalysisSetup<'a> {
    AnalysisSetup {
        builtins: real_builtins("8.3").summary(),
        rules,
        index,
    }
}

/// The index of the active rules under the default gate.
pub fn index(rules: &RuleSet) -> RuleIndex {
    RuleIndex::build(rules, &Gate::default())
}
