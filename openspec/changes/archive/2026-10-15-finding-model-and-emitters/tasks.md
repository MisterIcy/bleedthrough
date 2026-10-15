## 1. Domain model in `bleedthrough-findings`

- [x] 1.1 Add value types with validating constructors: rule id, message, location (relative forward-slash path, 1-based line and column in scalar values, byte offsets), taxonomy (CWE ids, edition-qualified OWASP refs); verify unit tests for absolute path, parent segment, and multi-byte column cases pass
- [x] 1.2 Enforce the mandatory-tag invariant at construction; verify a finding with no CWE or no OWASP category fails and one with two OWASP editions keeps both
- [x] 1.3 Add severity (level plus optional score as integer tenths and opaque vector) and confidence (tier, reasons, the closed `ReasonCode` vocabulary); verify a sub-high tier with no reason fails and the vocabulary includes every code the spec lists
- [x] 1.4 Add trace (source-to-sink validation, middle truncation keeping both ends, truncated marker); verify the ends are preserved for an over-limit trace
- [x] 1.5 Add fix and edit types with overlap and non-empty-range validation and the expected-original text; verify overlapping edits fail
- [x] 1.6 Add `FingerprintInput` and fingerprint computation (canonical length-prefixed encoding, SHA-256, 128 bits hex, `bt1:` prefix); verify the stability suite (moved code, whitespace-only changes, changed sink, occurrence index) and that hostile field boundaries cannot collide
- [x] 1.7 Add coverage (file counts by parse quality, skipped with reasons, limits hit, unresolved dynamic includes and calls, packs, PHP version, optional cache stats) and suppressed counts; verify an empty-findings report still holds coverage
- [x] 1.8 Add `RunInfo` and an immutable `Report` that sorts findings by path, start, rule, fingerprint; verify ordering, tie-breaking, and that shuffled input yields an identical report
- [x] 1.9 Mark public enums `#[non_exhaustive]`, add `Term:` doc lines to public types, and add `Report` and `Trace` to `docs/glossary.md`; verify `cargo xtask check-glossary` and clippy pass

## 2. Application port

- [x] 2.1 Add `Format` (names, `jsonl` alias, valid-name list in the error) to `bleedthrough-app`; verify alias and unknown-name unit tests
- [x] 2.2 Add the `Emitter` port, `EmitContext` (options, source-text access) and `EmitOutcome` (coverage summary, skipped fixes) to `bleedthrough-app`; verify `cargo xtask check-layering` still passes and no edge was added

## 3. Wire types and schema

- [x] 3.1 Add the wire types for the `json` document and `ndjson` records in `bleedthrough-emit` with a domain-to-wire mapping and ordered keys only; verify a mapping test covers every finding field
- [x] 3.2 Write `schemas/report.schema.json` (JSON Schema 2020-12) including the reason-code enumeration and the schema version; verify it is itself valid
- [x] 3.3 Add a schema-validation dev-dependency and a test harness validating emitter output; verify `cargo deny check` accepts it and the harness rejects a deliberately malformed document

## 4. Emitters

- [x] 4.1 Implement the `json` emitter; verify golden and schema tests for the fixed report set
- [x] 4.2 Implement the `ndjson` emitter (`run`, `finding`, `summary` records, no embedded newlines) with the `jsonl` alias; verify goldens, the interrupted-stream test (no summary, no partial line), and alias byte-equality
- [x] 4.3 Implement the `sarif` emitter with the 2.1.0 subset (rules, CWE and OWASP taxonomies, results, code flows, fixes, partial fingerprints, properties, notifications, invocation status, severity-to-level mapping); verify goldens and, if the schema's licence allows vendoring it, validation against the official SARIF 2.1.0 schema, otherwise structural assertions
- [x] 4.4 Implement the `tsv` emitter (fixed columns, optional header, escapes); verify goldens and the escape round-trip property test
- [x] 4.5 Implement the `github-actions` emitter (severity mapping, escaping, cap ordering, omitted-count notice, coverage notice); look up GitHub's current annotation limits and set the default cap from them; verify goldens and the escape round-trip property test
- [x] 4.6 Implement the `diff` emitter (apply edits to original text, `git apply`-compatible hunks, overlap and stale-edit skipping with reasons); verify by applying the produced patch with `git apply --check` on a temporary checkout in a test
- [x] 4.7 Implement the coverage-conveyance rules for formats without a native place (`tsv`, `diff`) via the outcome summary; verify an incomplete report yields a summary line

## 5. Cross-cutting verification

- [x] 5.1 Build the fixed report set in a test-support module (zero findings, full-featured finding, mixed severities, incomplete run, hostile strings); verify every emitter produces valid empty and non-empty output for each
- [x] 5.2 Add determinism tests: emit twice and emit from shuffled input, assert byte-equality for every format
- [x] 5.3 Add write-failure tests with a failing writer; verify each emitter returns an error and never reports success
- [x] 5.4 Add integration tests under `crates/emit/tests/` that go through the public `app` port only (build a `Report`, resolve each `Format` name including `jsonl`, emit, inspect the outcome); verify they compile without importing any `emit` internals and fail if a format is unwired
- [x] 5.5 Add a smoke test that, for every name returned by the valid-name list, resolves an emitter and emits a minimal report to valid non-empty output; verify adding a name without an emitter makes it fail
- [x] 5.6 Record the e2e handoff: add a note in the `scan-pipeline` change notes (or `docs/`) that each format needs an e2e test running the real binary against fixture projects and comparing to the golden files from this change; verify the note exists and names the golden directory

## 6. Bookkeeping and gates

- [x] 6.1 Regenerate `api/bleedthrough-findings.txt` and `api/bleedthrough-app.txt` with `cargo xtask check-api --update` and review the diff; verify `cargo xtask check-all` passes
- [x] 6.2 Run the full gate set (`cargo fmt --check`, `cargo clippy`, `cargo deny check`, `cargo test --workspace`); verify all pass on the pinned toolchain
- [x] 6.3 Document the output formats, the report schema version and the reason vocabulary in `docs/`; verify the examples in the docs match the golden files
