## Why

Bleedthrough's output is its product: engineers read it in CI and agents act on it. Every other part of the tool (engine, rules, baseline, gating, cache) produces or consumes findings, so the shape of a finding, of a whole report, and of each output format is the contract that all later changes build on. It has no dependency on parsing or analysis, almost every decision is already made, and fixing it first lets later changes target a stable, versioned schema instead of inventing ad hoc output.

## What Changes

- Add the domain model in `bleedthrough-findings`: `Finding` (rule, taxonomy tags, location, trace, severity, confidence with reasons, fingerprint, optional fix), `Report` (findings plus run metadata, `Coverage`, and suppressed counts), with a versioned schema. Construction enforces the mandatory CWE and OWASP tags.
- Define the **stable fingerprint** contract: derived from the rule, enclosing symbol and normalized key snippets, never from line numbers; versioned by scheme.
- Define **confidence** as an ordinal tier (`high | medium | low`) plus machine-readable reasons from a documented vocabulary, and **severity** as a level with room for the derived score added later. Derivation of both stays in later changes; this change defines their representation.
- Define the **coverage** block, which states what the run did and did not analyze (files by parse quality, skipped files and reasons, limits hit, unresolved dynamic constructs, active packs, cache use), and **suppressed counts** (below the confidence threshold, by baseline).
- Add the `Emitter` port in `bleedthrough-app` and implement six emitters in `bleedthrough-emit`: `json`, `sarif` (2.1.0), `ndjson` (with `jsonl` as an alias), `tsv`, `github-actions`, and `diff` (a `git apply`-able patch of suggested fixes).
- Guarantee **deterministic output**: a total order over findings, no wall-clock data, byte-identical output for identical inputs regardless of thread count.
- Publish a **JSON Schema** for the JSON and NDJSON records and verify emitter output against it, plus golden-file tests for every format.
- Add `Report` and `Trace` to the glossary; update the public-API snapshots of `findings` and `app`.

Out of scope: how findings are produced, how confidence and severity are derived, the baseline file format, CLI flags that select formats (`ci-ergonomics`, `scan-pipeline`), and the fix templates in rules (`rule-schema-and-taxonomy`).

## Capabilities

### New Capabilities
- `finding-model`: the content and invariants of a finding and of a report: identity (fingerprint), taxonomy tags, location and trace, severity, confidence and reasons, fixes, coverage, suppressed counts, schema versioning, and deterministic ordering.
- `report-emitters`: the output formats and their contracts: the structure, escaping and mapping rules of each of the six formats, that coverage is never silently dropped, and the byte-determinism and schema-conformance guarantees.

### Modified Capabilities
<!-- None. The layering and glossary specs are unchanged: the Emitter port follows the existing rule that application owns driven ports, and new glossary terms are an explicit edit to docs/glossary.md. -->

## Impact

- `crates/findings` gains real types with a pure hashing dependency for fingerprints and no serialization dependency at all; the wire formats are defined in `emit`. Domain purity rules still apply.
- `crates/app` gains the `Emitter` port and its context type; `crates/emit` gains the six emitters and the format crates they need (`serde_json`, permitted in infrastructure).
- New files: `schemas/report.schema.json` (or equivalent), golden fixtures under `crates/emit/tests/`, glossary additions in `docs/glossary.md`, refreshed `api/bleedthrough-findings.txt` and `api/bleedthrough-app.txt`.
- New dependencies must pass `cargo deny` (MIT / Apache-2.0 compatible); no parser or engine dependency is added.
- Downstream: `ir-and-taint-engine` produces these findings, `rule-schema-and-taxonomy` and `v1-rule-catalog` supply the tags and fix templates, `scan-pipeline` wires the emitters, and `ci-ergonomics` adds the baseline and gating that read the report.
