//! **Layer:** Tooling.
//!
//! **Responsibility:** PHP-source scenarios and golden text dumps for the IR and the taint engine, run
//! through the `ParserBackend` port so the domain crates are tested end to end without depending on a
//! parser.
//!
//! **Permitted workspace dependencies:** `bleedthrough-syntax`, `bleedthrough-symbols`,
//! `bleedthrough-builtins`, `bleedthrough-ir`, `bleedthrough-engine`, `bleedthrough-rules`,
//! `bleedthrough-findings`.

use std::path::{Path, PathBuf};

pub use bleedthrough_engine as engine;
pub use bleedthrough_findings as findings;
pub use bleedthrough_ir as ir;
pub use bleedthrough_rules as rules;
pub use bleedthrough_symbols as symbols;
use bleedthrough_symbols::{
    FileSummary, FunctionFacts, InferLimits, SymbolTable, infer_file, summarize,
};
pub use bleedthrough_syntax as syntax;
use bleedthrough_syntax::{ParseOptions, ParsedFile, ParserBackend};

/// The workspace root, found from this crate's manifest directory.
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Parses `source`.
pub fn parse(backend: &dyn ParserBackend, path: &str, source: &str) -> ParsedFile {
    backend.parse(path, source.as_bytes(), &ParseOptions::default())
}

/// Summarises `source`.
pub fn summary_of(backend: &dyn ParserBackend, path: &str, source: &str) -> FileSummary {
    let parsed = parse(backend, path, source);
    summarize(path, parsed.tree(), source)
}

/// Builds a table from `(path, source)` pairs.
pub fn table_from_sources(backend: &dyn ParserBackend, files: &[(&str, &str)]) -> SymbolTable {
    SymbolTable::build(
        files
            .iter()
            .map(|(path, source)| summary_of(backend, path, source))
            .collect(),
    )
}

/// The real built-in data for PHP `version`, read from `stubs/builtin.data`.
pub fn real_builtins(version: &str) -> symbols::Builtins {
    let text = std::fs::read_to_string(workspace_root().join("stubs/builtin.data"))
        .expect("stubs/builtin.data");
    let source =
        bleedthrough_builtins::InMemorySource::from_text(&text).expect("built-in data parses");
    symbols::Builtins::load(
        &source,
        bleedthrough_builtins::PhpVersion::parse(version).expect("version"),
    )
    .expect("built-in data loads")
}

/// Builds a table from `(path, source)` pairs with the built-in declarations of PHP 8.3.
pub fn table_with_builtins(backend: &dyn ParserBackend, files: &[(&str, &str)]) -> SymbolTable {
    let mut all = vec![real_builtins("8.3").summary()];
    all.extend(
        files
            .iter()
            .map(|(path, source)| summary_of(backend, path, source)),
    );
    SymbolTable::build(all)
}

/// Lowers every unit of `source` against `table`.
pub fn lower_source(
    backend: &dyn ParserBackend,
    table: &SymbolTable,
    path: &str,
    source: &str,
) -> Vec<ir::Graph> {
    lower_source_with(backend, table, path, source, &ir::LowerConfig::default())
}

/// [`lower_source`] under the given lowering bounds.
pub fn lower_source_with(
    backend: &dyn ParserBackend,
    table: &SymbolTable,
    path: &str,
    source: &str,
    config: &ir::LowerConfig,
) -> Vec<ir::Graph> {
    let parsed = parse(backend, path, source);
    let facts: Vec<FunctionFacts> = infer_file(parsed.tree(), table, &InferLimits::default());
    ir::lower_file_with(parsed.tree(), &facts, config)
}

/// Lowers `source` alone (its own declarations and the built-ins form the table).
pub fn lower_alone(backend: &dyn ParserBackend, source: &str) -> Vec<ir::Graph> {
    let table = table_with_builtins(backend, &[("main.php", source)]);
    lower_source(backend, &table, "main.php", source)
}

/// The dump of every graph of `source`, with source snippets, joined by blank lines.
pub fn dump_source(backend: &dyn ParserBackend, source: &str) -> String {
    dump_graphs(&lower_alone(backend, source), source)
}

/// The dump of the given graphs, with source snippets, joined by blank lines.
pub fn dump_graphs(graphs: &[ir::Graph], source: &str) -> String {
    graphs
        .iter()
        .map(|g| ir::dump_with_source(g, source))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Compares `actual` with the golden file at `golden`. With `BLEEDTHROUGH_BLESS=1` the golden is
/// written instead. Returns a description of the difference, if any.
pub fn check_golden(golden: &Path, actual: &str) -> Result<(), String> {
    if std::env::var_os("BLEEDTHROUGH_BLESS").is_some() {
        if let Some(parent) = golden.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        return std::fs::write(golden, actual).map_err(|e| format!("{}: {e}", golden.display()));
    }
    let expected = std::fs::read_to_string(golden).map_err(|e| {
        format!(
            "{}: {e} (set BLEEDTHROUGH_BLESS=1 to create it)",
            golden.display()
        )
    })?;
    if expected == actual {
        return Ok(());
    }
    Err(format!(
        "{} differs\n--- expected\n{expected}\n--- actual\n{actual}",
        golden.display()
    ))
}

/// Every `*.php` file directly in `dir`, sorted by name.
pub fn php_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "php"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

/// An include resolver over a fixed file list: a path matches a file by its exact normalised name, or
/// relative to the directory of the including file.
#[derive(Debug, Clone)]
pub struct FileList(pub Vec<Box<str>>);

impl engine::IncludeResolver for FileList {
    fn resolve(&self, from: &str, path: &engine::FoldedPath) -> Vec<Box<str>> {
        let relative = from.rsplit_once('/').map_or_else(
            || path.as_str().to_string(),
            |(dir, _)| engine::normalise_path(&format!("{dir}/{}", path.as_str())),
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

/// The lowered units of a project: the graphs of every file, in file order.
pub struct Project {
    /// The table the files were analysed against.
    pub table: SymbolTable,
    /// Each file's path and text, in the order the files were given.
    pub sources: Vec<(String, String)>,
    /// Each file's path and graphs, in the order the files were given.
    pub units: Vec<(String, Vec<ir::Graph>)>,
    /// The bounds the call graph is built under.
    pub call_config: engine::CallGraphConfig,
    /// A fault injected into the analysis of the units it returns true for.
    pub fail_units: Option<engine::FaultHook>,
}

impl Project {
    /// Parses, summarises, infers and lowers every file; the built-ins are part of the table.
    pub fn load(backend: &dyn ParserBackend, files: &[(&str, &str)]) -> Self {
        Self::load_with(backend, files, &ir::LowerConfig::default())
    }

    /// [`Self::load`] under the given lowering bounds.
    pub fn load_with(
        backend: &dyn ParserBackend,
        files: &[(&str, &str)],
        config: &ir::LowerConfig,
    ) -> Self {
        let table = table_with_builtins(backend, files);
        let units = files
            .iter()
            .map(|(path, source)| {
                (
                    (*path).to_string(),
                    lower_source_with(backend, &table, path, source, config),
                )
            })
            .collect();
        let sources = files
            .iter()
            .map(|(p, t)| ((*p).to_string(), (*t).to_string()))
            .collect();
        Self {
            table,
            sources,
            units,
            call_config: engine::CallGraphConfig::default(),
            fail_units: None,
        }
    }

    /// The 1-based line of a byte offset in `file`.
    pub fn line(&self, file: &str, offset: u32) -> usize {
        let text = self
            .sources
            .iter()
            .find(|(p, _)| p == file)
            .map_or("", |(_, t)| t.as_str());
        let end = (offset as usize).min(text.len());
        text.as_bytes()[..end]
            .iter()
            .filter(|b| **b == b'\n')
            .count()
            + 1
    }

    /// The flows from concrete sources to sinks found by analysing every unit on its own, one
    /// line each, sorted: `RULE class: source@file:line -> sink@file:line`.
    pub fn intra_flows(&self, index: &engine::RuleIndex) -> Vec<String> {
        let graph = self.call_graph();
        let heap = engine::Heap::new();
        let ctx = engine::Context {
            table: &self.table,
            index,
            limits: engine::Limits::default(),
            heap: &heap,
            inter: None,
        };
        let mut lines: Vec<String> = Vec::new();
        for (file, graphs) in &self.units {
            let refs: Vec<&ir::Graph> = graphs.iter().collect();
            for g in graphs {
                let Some(id) = graph.find(file, &g.key) else {
                    continue;
                };
                let flow = engine::analyse_unit(
                    &ctx,
                    &engine::UnitInput {
                        id,
                        graph: g,
                        file_graphs: &refs,
                        shared_scope: false,
                    },
                );
                for hit in &flow.hits {
                    let engine::Origin::Source { .. } = hit.label.origin else {
                        continue;
                    };
                    lines.push(self.describe_hit(index, &graph, hit));
                }
            }
        }
        lines.sort();
        lines.dedup();
        lines
    }

    /// The whole-project analysis: every flow from a concrete source to a sink, with calls and
    /// includes followed, one line each in the format of [`Self::intra_flows`].
    pub fn project_flows(&self, index: &engine::RuleIndex) -> Vec<String> {
        self.project_flows_with(index, engine::Limits::default())
    }

    /// [`Self::project_flows`] with the given bounds.
    pub fn project_flows_with(
        &self,
        index: &engine::RuleIndex,
        limits: engine::Limits,
    ) -> Vec<String> {
        let (graph, result) = self.analyse(index, limits);
        let mut lines: Vec<String> = result
            .hits
            .iter()
            .map(|hit| self.describe_hit(index, &graph, hit))
            .collect();
        lines.sort();
        lines.dedup();
        lines
    }

    /// The findings of the whole-project analysis under `limits`.
    pub fn findings(
        &self,
        rules: &rules::RuleSet,
        index: &engine::RuleIndex,
        limits: engine::Limits,
    ) -> Vec<findings::Finding> {
        let (graph, flow) = self.analyse(index, limits);
        let units = self.source_units();
        let texts: std::collections::BTreeMap<&str, &str> = self
            .sources
            .iter()
            .map(|(p, t)| (p.as_str(), t.as_str()))
            .collect();
        engine::build_findings(
            &engine::FindingsInput {
                rules,
                index,
                graph: &graph,
                units: &units,
                texts: &texts,
                limits,
            },
            &flow,
        )
    }

    /// The coverage of the project analysis for PHP `version`, and the calls left unresolved by
    /// reason.
    pub fn coverage(
        &self,
        index: &engine::RuleIndex,
        rules: &rules::RuleSet,
        version: &str,
    ) -> (findings::Coverage, engine::CallCoverage) {
        self.coverage_with(index, rules, version, engine::Limits::default())
    }

    /// [`Self::coverage`] under the given bounds.
    pub fn coverage_with(
        &self,
        index: &engine::RuleIndex,
        rules: &rules::RuleSet,
        version: &str,
        limits: engine::Limits,
    ) -> (findings::Coverage, engine::CallCoverage) {
        let (graph, flow) = self.analyse(index, limits);
        let units = self.source_units();
        let texts: std::collections::BTreeMap<&str, &str> = self
            .sources
            .iter()
            .map(|(p, t)| (p.as_str(), t.as_str()))
            .collect();
        engine::build_coverage(
            &engine::FindingsInput {
                rules,
                index,
                graph: &graph,
                units: &units,
                texts: &texts,
                limits,
            },
            &flow,
            version,
        )
    }

    /// The call graph and the whole-project analysis.
    pub fn analyse(
        &self,
        index: &engine::RuleIndex,
        limits: engine::Limits,
    ) -> (engine::CallGraph, engine::ProjectFlow) {
        let graph = self.call_graph();
        let units = self.source_units();
        let result = engine::analyse_project(&engine::ProjectInput {
            table: &self.table,
            index,
            limits,
            graph: &graph,
            units: &units,
            fail_units: self.fail_units,
        });
        (graph, result)
    }

    fn describe_hit(
        &self,
        index: &engine::RuleIndex,
        graph: &engine::CallGraph,
        hit: &engine::SinkHit,
    ) -> String {
        let sink = &index.sinks()[hit.sink as usize];
        let engine::Origin::Source { source, unit, span } = hit.label.origin else {
            unreachable!("concrete origins only");
        };
        let source_file = &graph.unit(unit).file;
        let file = &graph.unit(hit.unit).file;
        format!(
            "{} {}: {}@{}:{} -> {}@{}:{}",
            index.rules()[sink.rule as usize],
            sink.class,
            index.sources()[source as usize].symbol,
            source_file,
            self.line(source_file, span.start),
            sink.symbol,
            file,
            self.line(file, hit.span.start),
        )
    }

    /// Every unit with its file, in the order the files were given.
    pub fn source_units(&self) -> Vec<engine::SourceUnit<'_>> {
        self.units
            .iter()
            .flat_map(|(file, graphs)| {
                graphs
                    .iter()
                    .map(move |graph| engine::SourceUnit { file, graph })
            })
            .collect()
    }

    /// The call graph, resolving includes among the project's own files.
    pub fn call_graph(&self) -> engine::CallGraph {
        self.call_graph_with(&self.call_config)
    }

    /// The call graph with the given bounds.
    pub fn call_graph_with(&self, config: &engine::CallGraphConfig) -> engine::CallGraph {
        let resolver = FileList(self.units.iter().map(|(f, _)| f.as_str().into()).collect());
        engine::CallGraph::build(&self.source_units(), &self.table, &resolver, config)
    }
}
