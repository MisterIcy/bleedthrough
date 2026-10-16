//! The parse stage, generic over the parser backend.
//!
//! **Timed region:** `ParserBackend::parse` for every file of the input, one after the other on one
//! thread, with the default options, including the backend's conversion into the owned syntax tree.
//! **Outside:** reading or generating the files (they are loaded first) and counting nodes (done in a
//! separate untimed call). Each tree is dropped as soon as its file is parsed, inside the region.

use bleedthrough_syntax::{ParseOptions, ParseQuality, ParserBackend, visit_tree};

use crate::stage::{Counters, InputDesc, Stage};

/// One file of the input, held in memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    /// Path relative to the input root.
    pub path: String,
    /// The bytes.
    pub bytes: Vec<u8>,
}

/// Describes an input made of `files`.
pub fn describe(name: &str, files: &[SourceFile]) -> InputDesc {
    InputDesc {
        name: name.to_string(),
        files: files.len() as u64,
        bytes: files.iter().map(|f| f.bytes.len() as u64).sum(),
    }
}

/// Parses every file with `backend`; the counters are the same on every pass.
pub struct ParseStage<'a, B: ParserBackend> {
    backend: &'a B,
    files: &'a [SourceFile],
    options: ParseOptions,
}

impl<'a, B: ParserBackend> ParseStage<'a, B> {
    /// A parse stage over `files`.
    pub fn new(backend: &'a B, files: &'a [SourceFile]) -> Self {
        Self {
            backend,
            files,
            options: ParseOptions::default(),
        }
    }
}

impl<B: ParserBackend> Stage for ParseStage<'_, B> {
    fn name(&self) -> String {
        "parse".to_string()
    }

    fn pass(&mut self) {
        for file in self.files {
            let parsed = self.backend.parse(&file.path, &file.bytes, &self.options);
            std::hint::black_box(&parsed);
        }
    }

    fn counters(&mut self) -> Counters {
        let mut counters = Counters::new();
        let mut add = |name: &str, n: u64| *counters.entry(name.to_string()).or_default() += n;
        for file in self.files {
            let parsed = self.backend.parse(&file.path, &file.bytes, &self.options);
            let mut nodes = 0u64;
            visit_tree(parsed.tree(), &mut |_| nodes += 1);
            add("files", 1);
            add("bytes", file.bytes.len() as u64);
            add("nodes", nodes);
            add("errors", u64::from(parsed.quality() != ParseQuality::Clean));
        }
        counters
    }
}
