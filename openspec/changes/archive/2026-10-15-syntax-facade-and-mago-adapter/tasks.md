## 1. Workspace wiring and layering

- [x] 1.1 Add `adapters/*` to workspace members, create `adapters/syntax-mago` (`bleedthrough-syntax-mago`, infrastructure, `adapter = true`, may depend only on `bleedthrough-syntax`) and create the tooling crate `bleedthrough-syntax-conformance` (may depend only on `bleedthrough-syntax`); declare both in `architecture.toml`; verify `cargo xtask check-layering` passes
- [x] 1.2 Check how `check-layering` treats dev-dependency edges; declare the adapter-to-conformance dev-dependency explicitly or make the exemption of dev-dependencies explicit and documented; verify an undeclared normal-dependency edge into the conformance crate still fails
- [x] 1.3 Verify the release profile unwinds on panic (no `panic = "abort"`) and record this in a comment next to the profile settings; verify with a throwaway test that a caught panic does not abort

## 2. Syntax model in `bleedthrough-syntax`

- [x] 2.1 Add `Span`, `LineIndex` (1-based lines, columns in scalar values, each invalid byte one column) and the position conversions; verify unit tests for ASCII, multi-byte, CRLF, final line without newline, and invalid UTF-8
- [x] 2.2 Add diagnostics (stable codes: `unexpected_token`, `unexpected_eof`, `unclosed_string`, `recursion_limit`, `backend_failure`, `input_too_large`, `unsupported_construct`) and `ParseQuality` with the clean / recovered / failed rule; verify the quality rule with unit tests and add the `Term:` doc line naming "Parse quality"
- [x] 2.3 Add `Name` (as written, with qualification kind) and `Literal` (raw text, interpolation parts) types; verify `\Foo\Bar`, `Foo\Bar`, `namespace\Foo`, heredoc and nowdoc cases in unit tests
- [x] 2.4 Add the statement and declaration nodes (namespaces, imports, class-likes and members, properties with readonly, asymmetric visibility and hooks, promoted parameters, functions, closures, arrow functions, attributes, control flow including match, try, switch, goto, declare, echo, global, static, inline HTML); verify the type compiles with no third-party dependency in `Cargo.toml`
- [x] 2.5 Add the expression and type nodes (all operators and assignment forms, destructuring, references, calls of every kind including nullsafe and first-class callable, `new`, arrays and lists, include/require/eval/exit, pipe, clone, throw expression, yield, magic constants, variable variables, dynamic member and constant access; union, intersection, DNF and nullable types); verify against the construct list in the `syntax-model` spec, ticking each item
- [x] 2.6 Add `Unsupported` and `Error` variants in every position that can hold arbitrary syntax, plus counters for both; verify a tree built by hand with each reports the counts
- [x] 2.7 Add trivia (comments with kind and span) and the docblock lookup for a declaration (immediately preceding, whitespace only between); verify the detached-comment and adjacent-docblock cases
- [x] 2.8 Add the children iterator, a visitor, and the public invariant checker (whole-file coverage, bounds, containment, sibling order); verify with hand-built valid trees and trees that violate each invariant in turn
- [x] 2.9 Mark public enums `#[non_exhaustive]` and add `Term:` lines (exempt for pure structural nodes); verify `cargo xtask check-glossary` and clippy pass

## 3. The `ParserBackend` port

- [x] 3.1 Add `ParseOptions` (`short_tags`, `recovery` strict or editor, `max_bytes` with the provisional 8 MiB default), `ParsedFile` (tree, diagnostics, quality, counts) and `BackendInfo` (name, version); verify defaults in unit tests
- [x] 3.2 Add the `ParserBackend` trait (`Send + Sync`, input as relative path plus bytes) with the input-too-large behaviour documented on it; verify a stub implementation compiles and a doc-test shows its use
- [x] 3.3 Regenerate `api/bleedthrough-syntax.txt` with `cargo xtask check-api --update` and review it; verify `cargo xtask check-all` passes

## 4. Conformance crate

- [x] 4.1 Implement the stable AST dump (kind, span, escaped literal text, deterministic order); verify golden tests on hand-built trees, including text with quotes, newlines and non-ASCII
- [x] 4.2 Implement a stub backend in the conformance crate's own tests that returns canned trees, and use it to test the suite itself; verify the suite fails for a stub that drops a statement, overlaps siblings, parses a malformed fixture clean, loses a declared surviving declaration, or panics
- [x] 4.3 Implement the corpus loader over `fixtures/php/` (manifest, groups, malformed expectation blocks, ledger) and the completeness checks (feature without fixture, orphan fixture, fixture without ledger entry, oversized fixture, missing provenance or disallowed licence); verify each check fails on a deliberately broken temporary corpus
- [x] 4.4 Implement the suite assertions: valid fixtures parse clean, legacy fixtures follow their pinned policy, malformed fixtures meet their declared expectations, and backend verdicts agree with the ledger; verify with the stub backend as in 4.2
- [x] 4.5 Implement the prefix-truncation test in both recovery modes and the seeded mutation generator (byte flip, delete, insert, duplicate; own deterministic PRNG, no new dependency) with seeds in failure messages; verify reproducibility by re-running a recorded seed
- [x] 4.6 Implement the ledger recorder and verifier: run `php -l` on a temporary copy of each fixture with the interpreters found, write or compare verdicts with the interpreter version and date, and list versions that could not be checked as unverified with the reason; verify with and without PHP on the path and that an unverifiable version is never reported as verified
- [x] 4.7 Implement the differential harness and the allowance file (entry plus reason); verify two stub backends that differ fail unless the difference is allow-listed

## 5. Mago adapter

- [x] 5.1 Add dependencies on `mago-syntax`, `mago-span`, `mago-database` and `mago-allocator`, each `=1.57.0`, and commit the updated `Cargo.lock`; verify `cargo build --locked` and `cargo xtask check-layering` (exact pins) pass
- [x] 5.2 Implement the parse call (fresh arena per call, strict and editor recovery mapped to the Mago settings, short-tag option, size limit before parsing); verify an oversized input returns `input_too_large` without calling Mago
- [x] 5.3 Implement the converter module per node family (statements, declarations, expressions, types, names, literals, attributes) with every `match` over a Mago enum exhaustive and `clippy::wildcard_enum_match_arm` denied in those modules; verify the lint passes and each function respects the size and complexity limits
- [x] 5.4 Convert constructs without an owned node to `Unsupported` with their kind and span; verify the unsupported count is non-zero for a hand-picked construct and appears in the result
- [x] 5.5 Implement the error mapper per design D6 (all Mago `ParseError` variants); verify one test per variant
- [x] 5.6 Implement the gap filler (design D5) including the one-token skip case and the no-matching-error case; verify the whole-file coverage invariant on a class with a broken method, an orphan keyword, and a file with valid code around a broken statement
- [x] 5.7 Implement the safety net (`catch_unwind`, recursion-limit mapping) and the quality computation; verify with a forced panic injected through a test hook and with a deeply nested expression
- [x] 5.8 Implement the backend identity from the pinned version and add a test comparing it with `Cargo.lock`; verify the test fails if either is changed alone
- [x] 5.9 Implement `Send + Sync` use and verify with a concurrent test that many files parsed in parallel equal their single-threaded results

## 6. Fixture corpus and goldens

- [x] 6.1 Check which official `php:<version>-cli` images exist for 7.0 to 8.5 and which versions can be obtained, and record the result in the ledger plan; verify every version is either marked obtainable or recorded as unverified with a reason
- [x] 6.2 Build the feature list for PHP 7.0 to 8.5 from the official PHP documentation, one row per syntax feature with the page cited, in the manifest; verify every row names a source
- [x] 6.3 Write the valid fixtures for every row in `fixtures/php/v7.0 ... v8.5/`, original and small, one feature each; verify each parses clean with no `Unsupported` or `Error` node on the adapter, and list in the spike report anything Mago cannot parse instead of hiding it
- [x] 6.4 (owner accepted the `brace-string-offset` gap, 2026-10-15) Write the legacy-removed fixtures (for example the `(unset)` cast, brace-style string and array offsets, other syntax PHP 8 removed) with their pinned lenient policy; verify the adapter accepts them and the 8.x interpreters reject them
- [x] 6.5 Write the malformed fixtures for each class of breakage in the spec, with backend-neutral expectation blocks (diagnostic family, error lines, surviving declarations); verify each yields a diagnostic, an error node on the declared lines, and its surviving declarations
- [x] 6.6 Write the realistic fixtures (controller, repository, inline-HTML template, legacy procedural script), each with a stated purpose; verify they parse clean
- [x] 6.7 Record the ledger with the recorder (task 4.6) for every version obtainable and mark the rest unverified; verify the committed ledger matches the interpreters available locally
- [x] 6.8 Generate and commit golden dumps; verify a deliberate converter change makes the suite fail until the goldens are updated
- [x] 6.9 Write the corpus documentation (how to add a fixture, record its verdicts, and run the verifier); verify the steps work as written by adding and removing a throwaway fixture

## 7. Policy and CI

- [x] 7.1 Extend the layering check with the reachability test over the resolved dependency graph (no `mago-*` crate reachable from any workspace crate other than the adapter); verify a temporary dependency of another crate on `mago-span` fails with a message naming the path
- [x] 7.2 Update `deny.toml` for the direct Mago dependencies and run `cargo deny check` on the full closure; verify it passes, or record each failing advisory or licence in the spike report instead of ignoring it
- [x] 7.3 Add a CI job that re-verifies the ledger with `php -l` in the official container image for each obtainable PHP version (images pinned by digest where available), listing unobtainable versions as unverified, plus a job running the conformance suite; verify both run on a pull request and that a deliberately wrong ledger entry fails the verification job
- [x] 7.4 Update `docs/architecture.md`, `adapters/README.md` and the diagram with the adapter and the tooling crate; verify the diagram matches `architecture.toml`

## 8. Spike and review

- [x] 8.1 Write `examples/spike.rs` in the adapter measuring parse-only and parse-plus-conversion throughput and peak resident memory over the fixtures, a generated large file and, locally and uncommitted, at least one real open-source PHP project; verify it prints reproducible numbers
- [x] 8.2 Write `docs/spikes/syntax-mago.md` covering fixture coverage, recovery behaviour (including the count of regions made explicit), conversion cost against the provisional 2x limit, memory, and the dependency and licence footprint, each with its method; verify every criterion in design D11 has a result
- [x] 8.3 Present the report to the project owner with a recommendation, and confirm or revise the provisional criteria; verify the owner's decision is recorded in the report before `symbols-and-types` is planned

## 9. Documentation and final gates

- [x] 9.1 Write the Mago upgrade procedure in `docs/` (design D12); verify it was followed once by bumping to the same version in a dry run
- [x] 9.2 Regenerate `api/` snapshots, review the diffs, and run `cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo deny check`, `cargo xtask check-all`, and `cargo test --workspace --locked`; verify all pass on the pinned toolchain
