//! **Layer:** Tooling.
//!
//! **Responsibility:** PHP-source scenarios and golden text dumps for the symbols crate, run through
//! the `ParserBackend` port so the domain crates are tested end to end without depending on a parser.
//!
//! **Permitted workspace dependencies:** `bleedthrough-syntax`, `bleedthrough-symbols`, `bleedthrough-builtins`.

use std::path::{Path, PathBuf};

pub use bleedthrough_builtins as builtins;
pub use bleedthrough_symbols as symbols;
pub use bleedthrough_symbols::FileSummary;
use bleedthrough_symbols::{FunctionFacts, InferLimits, dump_summary, infer_file, summarize};
use bleedthrough_syntax::{ParseOptions, ParserBackend};

/// The workspace root, found from this crate's manifest directory.
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Parses `source` with `backend` and summarises it.
pub fn summarize_source(backend: &dyn ParserBackend, path: &str, source: &str) -> FileSummary {
    let parsed = backend.parse(path, source.as_bytes(), &ParseOptions::default());
    summarize(path, parsed.tree(), source)
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

/// Compares `actual` with the golden file at `golden`. With `BLEEDTHROUGH_BLESS=1` the golden is
/// written instead. Returns a description of the difference, if any.
pub fn check_golden(golden: &Path, actual: &str) -> Result<(), String> {
    if std::env::var_os("BLEEDTHROUGH_BLESS").is_some() {
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

/// Turns a fixture's file name and source into its golden text.
pub type Render<'a> = dyn Fn(&str, &str) -> String + 'a;

/// Runs every `*.php` fixture of `dir` through `render` and checks the result against the sibling
/// `*.txt` golden. Returns all failures.
pub fn run_golden_dir(dir: &Path, render: &Render<'_>) -> Vec<String> {
    php_files(dir)
        .iter()
        .filter_map(|file| {
            let source = std::fs::read_to_string(file).ok()?;
            let name = file.file_name()?.to_string_lossy().into_owned();
            let text = render(&name, &source);
            check_golden(&file.with_extension("txt"), &text).err()
        })
        .collect()
}

/// The golden text of one file's summary.
pub fn summary_text(backend: &dyn ParserBackend, name: &str, source: &str) -> String {
    dump_summary(&summarize_source(backend, name, source))
}

pub use bleedthrough_symbols::SymbolTable;

/// Builds a table from `(path, source)` pairs.
pub fn table_from_sources(backend: &dyn ParserBackend, files: &[(&str, &str)]) -> SymbolTable {
    SymbolTable::build(
        files
            .iter()
            .map(|(path, source)| summarize_source(backend, path, source))
            .collect(),
    )
}

/// Parses `source` with `backend` and infers every body in it against `table`.
pub fn infer_source(
    backend: &dyn ParserBackend,
    table: &SymbolTable,
    path: &str,
    source: &str,
    limits: &InferLimits,
) -> Vec<FunctionFacts> {
    let parsed = backend.parse(path, source.as_bytes(), &ParseOptions::default());
    infer_file(parsed.tree(), table, limits)
}

/// The source text of a key, collapsed to one line and shortened for dumps.
pub fn snippet(source: &str, start: u32, end: u32) -> String {
    let text = source
        .get(start as usize..end as usize)
        .unwrap_or("?")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if text.chars().count() > 70 {
        let cut: String = text.chars().take(67).collect();
        format!("{cut}...")
    } else {
        text
    }
}

/// The golden text of the call resolutions of a file: one block per body, one line per call-like
/// expression with its source, resolution and result type.
pub fn calls_text(facts: &[FunctionFacts], source: &str) -> String {
    let mut out = String::new();
    for f in facts {
        if f.calls.is_empty() {
            continue;
        }
        out.push_str(&format!("== {}\n", f.name));
        for (key, resolution) in &f.calls {
            let ty = f
                .type_of(key)
                .map_or_else(|| "?".to_string(), ToString::to_string);
            out.push_str(&format!(
                "{} :: {} -> {} : {}\n",
                snippet(source, key.start, key.end),
                key.tag.as_str(),
                resolution,
                ty
            ));
        }
        let s = &f.stats;
        out.push_str(&format!(
            "stats: calls resolved={} unresolved={} dynamic={} unknown_exprs={}/{} limit_hit={}\n",
            s.resolved_calls,
            s.unresolved_calls,
            s.dynamic_calls,
            s.unknown_expressions,
            s.expressions,
            s.limit_hit()
        ));
    }
    out
}

/// The real built-in data for PHP `version`, read from `stubs/builtin.data`.
pub fn real_builtins(version: &str) -> symbols::Builtins {
    let text = std::fs::read_to_string(workspace_root().join("stubs/builtin.data"))
        .expect("stubs/builtin.data");
    let source = builtins::InMemorySource::from_text(&text).expect("built-in data parses");
    symbols::Builtins::load(
        &source,
        builtins::PhpVersion::parse(version).expect("version"),
    )
    .expect("built-in data loads")
}

/// Like [`table_from_sources`], with the built-in declarations of PHP 8.3 in the table.
pub fn table_with_builtins(backend: &dyn ParserBackend, files: &[(&str, &str)]) -> SymbolTable {
    let mut all = vec![real_builtins("8.3").summary()];
    all.extend(
        files
            .iter()
            .map(|(path, source)| summarize_source(backend, path, source)),
    );
    SymbolTable::build(all)
}
