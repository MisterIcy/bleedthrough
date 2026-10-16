//! Per-version extraction: parses every stub file of one vendored release and extracts records.

use std::fs;
use std::path::{Path, PathBuf};

use bleedthrough_builtins::PhpVersion;
use bleedthrough_syntax::{ParseOptions, ParseQuality, ParserBackend};

use crate::count::{Coverage, compare};
use crate::extract::{Extracted, GenError, extract_file};
use crate::origin::{Exclusion, excluded, origin_of};

/// The stub files of one vendored release.
#[derive(Debug, Clone)]
pub struct ReleaseInput {
    pub version: PhpVersion,
    /// The vendored tree, for example `third_party/php-src/php-8.5.11`.
    pub root: PathBuf,
}

/// What happened to one stub file.
#[derive(Debug, Clone)]
pub struct FileReport {
    /// Relative to the release tree.
    pub path: String,
    /// Set when the file was excluded; then nothing was extracted from it.
    pub excluded: Option<&'static Exclusion>,
    /// Found versus produced declarations; empty for excluded files.
    pub coverage: Coverage,
}

/// The records of one release and the report on how they were obtained.
#[derive(Debug, Default)]
pub struct ReleaseExtraction {
    pub items: Vec<Extracted>,
    pub files: Vec<FileReport>,
    pub errors: Vec<GenError>,
}

/// The `*.stub.php` files under `root`, as sorted paths relative to it.
pub fn stub_files(root: &Path) -> std::io::Result<Vec<String>> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) -> std::io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                walk(&path, root, out)?;
            } else if path.to_string_lossy().ends_with(".stub.php") {
                let relative = path.strip_prefix(root).unwrap_or(&path);
                out.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    out.sort();
    Ok(out)
}

/// Parses and extracts every stub file of the release.
pub fn extract_release(backend: &dyn ParserBackend, input: &ReleaseInput) -> ReleaseExtraction {
    let mut out = ReleaseExtraction::default();
    let files = match stub_files(&input.root) {
        Ok(files) => files,
        Err(e) => {
            out.errors.push(GenError {
                file: input.root.display().to_string(),
                offset: 0,
                message: format!("cannot list the stub files: {e}"),
            });
            return out;
        }
    };
    for path in files {
        if let Some(rule) = excluded(&path) {
            out.files.push(FileReport {
                path,
                excluded: Some(rule),
                coverage: Coverage::default(),
            });
            continue;
        }
        let source = match fs::read(input.root.join(&path)) {
            Ok(source) => source,
            Err(e) => {
                out.errors.push(GenError {
                    file: path,
                    offset: 0,
                    message: format!("cannot read the file: {e}"),
                });
                continue;
            }
        };
        let (items, coverage, errors) = extract_source(backend, &path, &source, input.version);
        out.files.push(FileReport {
            path,
            excluded: None,
            coverage,
        });
        out.items.extend(items);
        out.errors.extend(errors);
    }
    out
}

/// Parses and extracts one stub file whose path (relative to the release tree) decides its origin.
pub fn extract_source(
    backend: &dyn ParserBackend,
    path: &str,
    source: &[u8],
    version: PhpVersion,
) -> (Vec<Extracted>, Coverage, Vec<GenError>) {
    let fail = |offset: u32, message: String| {
        (
            Vec::new(),
            Coverage::default(),
            vec![GenError {
                file: path.to_string(),
                offset,
                message,
            }],
        )
    };
    let Some(origin) = origin_of(path) else {
        return fail(
            0,
            "the file is in no known place (Zend, main, ext, sapi)".to_string(),
        );
    };
    let parsed = backend.parse(path, source, &ParseOptions::default());
    if parsed.quality() != ParseQuality::Clean {
        let first = parsed.diagnostics().first();
        return fail(
            first.map_or(0, |d| d.span.start),
            format!(
                "the file does not parse cleanly ({:?}; {} diagnostics, {} errors, {} unsupported): {}",
                parsed.quality(),
                parsed.diagnostics().len(),
                parsed.error_count(),
                parsed.unsupported_count(),
                first.map_or(String::new(), |d| d.message.clone()),
            ),
        );
    }
    let extraction = extract_file(path, source, &parsed, version, &origin);
    let coverage = compare(parsed.tree(), &extraction.items);
    let mut errors = extraction.errors;
    for (kind, name) in &coverage.missing {
        errors.push(GenError {
            file: path.to_string(),
            offset: 0,
            message: format!(
                "the {} `{name}` was found but no record was produced",
                kind.as_str()
            ),
        });
    }
    for (kind, name) in &coverage.unexpected {
        errors.push(GenError {
            file: path.to_string(),
            offset: 0,
            message: format!(
                "a record for the {} `{name}` was produced but the parser found none",
                kind.as_str()
            ),
        });
    }
    (extraction.items, coverage, errors)
}
