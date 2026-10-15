## Why

Everything downstream (symbols, types, IR, taint) reads PHP source through one syntax model. Parsing PHP 8 correctly is expensive to build, so we adopt Mago's parser, but we decided not to let its types leak: the owned AST and a parser-backend port make the backend swappable, give one place where PHP semantics are lowered, and let us test backends against each other. This change builds that seam and the first adapter, and it doubles as the spike that answers the open questions about the parser (PHP 8.0 to 8.5 coverage, behaviour on broken code, cost of conversion, dependency footprint) before the larger symbols and engine changes are designed on top of it.

Inspection of `mago-syntax` 1.57.0 already shows what the design must handle:
- On a syntax error the parser drops the whole failing statement (a class with one bad method disappears from the tree) and only records an error. Left as is, this would silently remove code from analysis, so the adapter must make every gap explicit.
- The parser has no PHP-version setting: it accepts all syntax, and its CST already models property hooks, the pipe operator and partial application.
- Its dependency closure is large (`mago-database`, `mago-reporting`, `mago-span`, `mago-allocator`, and through them file-watching, regex, SARIF and terminal crates), which matters for supply-chain audit and build time.
- It requires Rust 1.99 and edition 2024, matching our toolchain.

## What Changes

- Add the **owned syntax model** in `bleedthrough-syntax`: a third-party-type-free AST covering the PHP 8.0 to 8.5 constructs that symbol collection and flow lowering need, with byte-offset spans, raw comments and docblock lookup, per-file parse diagnostics and parse quality (clean, recovered, failed), and explicit `Unsupported` and `Error` nodes so that nothing is ever silently dropped.
- Add the **`ParserBackend` port** in `bleedthrough-syntax`: neutral input and options, a result that never panics or aborts on any bytes, backend identity (name and version) for coverage and cache keys, and thread-safety.
- Add the **Mago adapter** crate `bleedthrough-syntax-mago` under `adapters/`: exact pins of every direct `mago-*` dependency, `Cargo.lock` committed and builds run `--locked`, conversion of the Mago CST into the owned AST, **gap filling** (error nodes for every source region the parser dropped), mapping of Mago errors to our diagnostics, and a panic and recursion safety net.
- Add a **parser-independent PHP fixture corpus** at the repository root: original valid fixtures for the syntax introduced in each PHP version from 7.0 to 8.5, legacy-removed fixtures (valid in 7.x, rejected by 8.x, parsed leniently by policy), malformed fixtures with backend-neutral expectations, and realistic files, with a manifest and a per-version **verdict ledger** that a CI Docker matrix re-verifies against the official PHP interpreters.
- Add a **backend-agnostic conformance suite** that runs that corpus through the port: valid and malformed expectations, agreement with the ledger, stable textual AST dumps as golden files, truncation and mutation resilience tests on stable Rust, and a differential harness that compares backends once more than one is registered.
- Run the **spike** and record its results: PHP 8.0 to 8.5 coverage, recovery behaviour, conversion cost against raw parse cost, memory, and the transitive dependency and licence footprint, with go/no-go criteria reviewed by the user.
- Extend the layering and policy declarations: register the adapter crate in `architecture.toml`, extend `deny.toml` so every `mago-*` crate is reachable only through the adapter, and update API snapshots and docs. The CLI wiring is not part of this change.

Scope note: analysis targets PHP 8.0 to 8.5, but the corpus and the parser also cover PHP 7.x, because legacy code is common in real CI targets and must at least parse; syntax removed in 8.x is accepted leniently and pinned by the corpus.

Out of scope: name resolution, symbol tables, types and docblock-type parsing (`symbols-and-types`), lowering to IR, PHP-version gating of syntax (the adapter parses permissively and only records), a second backend such as tree-sitter (reserved for differential testing later), the broader fixture corpus and performance benchmark suite (`corpus-and-benchmarks`), and selecting the backend in the CLI (`scan-pipeline`).

## Capabilities

### New Capabilities
- `syntax-model`: the owned AST contract: what it represents, spans, comments and docblocks, diagnostics and parse quality, lenient acceptance of legacy syntax, and the rule that nothing is silently dropped.
- `parser-backend-port`: the contract any parser backend implements: input, options, result, robustness on arbitrary input, determinism, identity and thread-safety.
- `parser-adapter-mago`: the Mago adapter's obligations: pinning, isolation, faithful conversion, gap filling, error mapping and safety net.
- `php-fixture-corpus`: the parser-independent fixture corpus: layout by PHP version 7.0 to 8.5, valid, legacy-removed, malformed and realistic groups, the per-version verdict ledger and its interpreter verification, backend-neutral malformed expectations, and provenance rules.
- `syntax-conformance`: the backend-agnostic test contract over that corpus: valid and malformed expectations, ledger agreement, golden AST dumps, resilience tests, differential comparison and the spike report.

### Modified Capabilities
<!-- None. The existing layering rules already place the port in the syntax crate and adapters in infrastructure; the new crate is a declaration entry, not a rule change. -->

## Impact

- `crates/syntax` gains the AST, spans, diagnostics and the port; no workspace dependency and no third-party parser type is added to it.
- New crate `adapters/syntax-mago` (infrastructure, `adapter = true`), a workspace member; `Cargo.toml` members, `architecture.toml` and `deny.toml` are updated; `api/bleedthrough-syntax.txt` is regenerated.
- New dependencies (adapter only, all `MIT OR Apache-2.0` at the pinned version as checked on crates.io): `mago-syntax`, `mago-span`, `mago-database`, `mago-allocator` and their transitive crates. `cargo deny check` must pass for the full closure, including advisories; any unmaintained or vulnerable transitive crate is surfaced by the spike.
- Test and spike assets: the corpus under `fixtures/php/` (manifest and ledger included), golden AST dumps, a spike report under `docs/`, and a measurement example in the adapter crate. CI gains a job that re-verifies the ledger in official PHP container images for each version.
- Downstream: `symbols-and-types` and `ir-and-taint-engine` consume the AST; `corpus-and-benchmarks` extends the fixtures and measures performance; `scan-pipeline` wires the adapter into the CLI.
