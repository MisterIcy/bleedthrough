//! The analysis stage, generic over the parser backend.
//!
//! **Timed region:** for every file, parsing, summarising, inferring and lowering; then the symbol
//! table, the call graph, the whole-project taint analysis and finding production, on one thread with
//! the default limits. **Outside:** reading or generating the files, loading the built-in data and the
//! rules (the caller passes them ready), and the counters (computed in a separate untimed call).

use std::collections::BTreeMap;

use bleedthrough_engine::{
    CallGraph, CallGraphConfig, FindingsInput, FoldedPath, IncludeResolver, Limits, ProjectInput,
    RuleIndex, SourceUnit, analyse_project, build_findings, normalise_path,
};
use bleedthrough_ir::{Graph, LowerConfig, lower_file_with};
use bleedthrough_rules::RuleSet;
use bleedthrough_symbols::{FileSummary, InferLimits, SymbolTable, infer_file, summarize};
use bleedthrough_syntax::{ParseOptions, ParserBackend};

use crate::parse_stage::SourceFile;
use crate::stage::{Counters, Stage};

/// What the analysis needs besides the files: the built-in declarations and the rules, ready.
pub struct AnalysisSetup<'a> {
    /// The built-in declarations of the PHP version analysed.
    pub builtins: FileSummary,
    /// The rules the index was built from.
    pub rules: &'a RuleSet,
    /// The active rules.
    pub index: &'a RuleIndex,
}

/// Resolves an include to the input file with that exact or directory-relative path.
struct Files(Vec<Box<str>>);

impl IncludeResolver for Files {
    fn resolve(&self, from: &str, path: &FoldedPath) -> Vec<Box<str>> {
        let relative = from.rsplit_once('/').map_or_else(
            || path.as_str().to_string(),
            |(dir, _)| normalise_path(&format!("{dir}/{}", path.as_str())),
        );
        let mut found: Vec<Box<str>> = self
            .0
            .iter()
            .filter(|f| f.as_ref() == path.as_str() || f.as_ref() == relative)
            .cloned()
            .collect();
        found.sort();
        found
    }
}

/// What one pass counted.
#[derive(Debug, Clone, Copy, Default)]
struct Tally {
    files: u64,
    units: u64,
    findings: u64,
}

/// Analyses every file as one project; the counters are the same on every pass.
pub struct AnalysisStage<'a, B: ParserBackend> {
    backend: &'a B,
    files: &'a [SourceFile],
    setup: &'a AnalysisSetup<'a>,
}

impl<'a, B: ParserBackend> AnalysisStage<'a, B> {
    /// An analysis stage over `files`.
    pub fn new(backend: &'a B, files: &'a [SourceFile], setup: &'a AnalysisSetup<'a>) -> Self {
        Self {
            backend,
            files,
            setup,
        }
    }

    fn run(&self) -> Tally {
        let options = ParseOptions::default();
        let texts: Vec<(&str, String)> = self
            .files
            .iter()
            .map(|f| {
                (
                    f.path.as_str(),
                    String::from_utf8_lossy(&f.bytes).into_owned(),
                )
            })
            .collect();
        let parsed: Vec<_> = self
            .files
            .iter()
            .map(|f| self.backend.parse(&f.path, &f.bytes, &options))
            .collect();
        let mut summaries = vec![self.setup.builtins.clone()];
        summaries.extend(
            parsed
                .iter()
                .zip(&texts)
                .map(|(p, (path, text))| summarize(path, p.tree(), text)),
        );
        let table = SymbolTable::build(summaries);
        let lowered: Vec<(&str, Vec<Graph>)> = parsed
            .iter()
            .zip(&texts)
            .map(|(p, (path, _))| {
                let facts = infer_file(p.tree(), &table, &InferLimits::default());
                (
                    *path,
                    lower_file_with(p.tree(), &facts, &LowerConfig::default()),
                )
            })
            .collect();
        let units: Vec<SourceUnit<'_>> = lowered
            .iter()
            .flat_map(|(file, graphs)| graphs.iter().map(move |graph| SourceUnit { file, graph }))
            .collect();
        let resolver = Files(lowered.iter().map(|(f, _)| (*f).into()).collect());
        let graph = CallGraph::build(&units, &table, &resolver, &CallGraphConfig::default());
        let limits = Limits::default();
        let flow = analyse_project(&ProjectInput {
            table: &table,
            index: self.setup.index,
            limits,
            graph: &graph,
            units: &units,
            fail_units: None,
        });
        let by_path: BTreeMap<&str, &str> = texts.iter().map(|(p, t)| (*p, t.as_str())).collect();
        let findings = build_findings(
            &FindingsInput {
                rules: self.setup.rules,
                index: self.setup.index,
                graph: &graph,
                units: &units,
                texts: &by_path,
                limits,
            },
            &flow,
        );
        Tally {
            files: lowered.len() as u64,
            units: units.len() as u64,
            findings: findings.len() as u64,
        }
    }
}

impl<B: ParserBackend> Stage for AnalysisStage<'_, B> {
    fn name(&self) -> String {
        "analysis".to_string()
    }

    fn pass(&mut self) {
        std::hint::black_box(self.run());
    }

    fn counters(&mut self) -> Counters {
        let tally = self.run();
        let mut counters = Counters::new();
        counters.insert("files".to_string(), tally.files);
        counters.insert("units".to_string(), tally.units);
        counters.insert("findings".to_string(), tally.findings);
        counters
    }
}
