//! **Layer:** Tooling.
//!
//! **Responsibility:** PHP-source scenarios and golden text dumps for the IR and the taint engine, run
//! through the `ParserBackend` port so the domain crates are tested end to end without depending on a
//! parser.
//!
//! **Permitted workspace dependencies:** `bleedthrough-syntax`, `bleedthrough-symbols`,
//! `bleedthrough-builtins`, `bleedthrough-ir`, `bleedthrough-engine`.

use std::path::{Path, PathBuf};

pub use bleedthrough_engine as engine;
pub use bleedthrough_ir as ir;
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
    let parsed = parse(backend, path, source);
    let facts: Vec<FunctionFacts> = infer_file(parsed.tree(), table, &InferLimits::default());
    ir::lower_file(parsed.tree(), &facts)
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
    /// Each file's path and graphs, in the order the files were given.
    pub units: Vec<(String, Vec<ir::Graph>)>,
}

impl Project {
    /// Parses, summarises, infers and lowers every file; the built-ins are part of the table.
    pub fn load(backend: &dyn ParserBackend, files: &[(&str, &str)]) -> Self {
        let table = table_with_builtins(backend, files);
        let units = files
            .iter()
            .map(|(path, source)| {
                (
                    (*path).to_string(),
                    lower_source(backend, &table, path, source),
                )
            })
            .collect();
        Self { table, units }
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
        self.call_graph_with(&engine::CallGraphConfig::default())
    }

    /// The call graph with the given bounds.
    pub fn call_graph_with(&self, config: &engine::CallGraphConfig) -> engine::CallGraph {
        let resolver = FileList(self.units.iter().map(|(f, _)| f.as_str().into()).collect());
        engine::CallGraph::build(&self.source_units(), &self.table, &resolver, config)
    }
}
