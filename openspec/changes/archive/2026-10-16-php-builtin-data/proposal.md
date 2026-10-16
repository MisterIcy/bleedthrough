## Why

The symbol table must know PHP's built-in functions, classes, interfaces, enums and constants, with exact signatures and version availability, or call resolution and taint propagation through built-ins will be wrong. The first plan (hand-written minimal stubs) was rejected: a hand-typed subset drifts from the real runtime and a wrong by-reference flag or return type silently breaks taint flow. Built-in knowledge must instead be produced mechanically from PHP's own source and checked against real interpreters, and be complete for the supported runtime. Research against the php-src tags 8.0.30 to 8.5.11 shows this is feasible but has traps: the stub files are not valid standalone PHP (19 fail `php -l`, 29 in 8.5 with an 8.4 interpreter), `#if/#else` guards are the only build-configuration information, constants are absent from the 8.0 and 8.1 stubs and constant values are absent everywhere, and no version availability is stored in the stubs.

## What Changes

- Add a pure domain crate **`bleedthrough-builtins`**: the record model for built-in functions, classes, interfaces, traits, enums and constants (parameters with by-reference, optional and variadic flags, default expression text, declared, tentative and docblock-refined type text, deprecation, aliases, owning extension, SAPI, build-guard text), a **version mask** over PHP 8.0 to 8.5 per record, deterministic lookup for a chosen version, a canonical line-oriented text format with reader and writer, and the `BuiltinDataSource` port. No I/O and no PHP-parser dependency.
- Add a **generator** (tooling crate `bleedthrough-builtins-gen`, generic over `ParserBackend`, run through an example binary in the Mago adapter and invoked by `cargo xtask gen-builtins`): parses the vendored php-src `*.stub.php` files of the six pinned releases with the project's own pipeline, handles `#if/#else` guards and the PHP annotations, extracts declarations, merges the six per-version extractions into records with version masks, and writes one checked-in data file. Nothing is silently dropped: a coverage report compares declarations found with records produced.
- Vendor the **six pinned stub trees** (only `*.stub.php` and the licence files they fall under) under `third_party/php-src/<tag>/`, with a manifest of tag, full commit SHA and a content hash per file, so regeneration is offline and pinned by content.
- Add **Reflection fixtures** as an independent source and oracle: a reviewed PHP script run once in the official `php:8.x-cli` images (pinned by digest) produces checked-in JSON dumps; they fill what the stubs lack (all constants and properties for 8.0 and 8.1, constant values for all versions) and drive a cross-check of functions, classes, parameters, by-reference flags, return types and constants against the generated data, with a reviewed allowance list. Running the images is manual and opt-in; required CI stays offline.
- Add **provenance and licensing**: a `NOTICE` carrying the PHP License 3.01 and Zend Engine License 2.00 texts and copyright lines, shipped in release archives and the Docker image; the data stores facts only (names, types, flags, versions, default expression text), never documentation prose; the manifest records sources; a pre-release task requires the project owner or counsel to confirm the licence position.
- Add `cargo xtask check-builtins` (regenerate and diff, manifest hash verification, NOTICE presence, format validity) to the CI gates, and document the update procedure for a new PHP release.
- Update layering declarations, glossary, API snapshots and docs.

Out of scope: consuming the data in the symbol table and type model (`symbols-and-types`), sources/sinks/sanitizers (rules and packs own those), PHP extensions outside php-src (PECL), user-facing CLI, and evaluating default-value expressions.

## Capabilities

### New Capabilities
- `builtin-data-model`: records, version masks, text format, port and lookup semantics.
- `builtin-data-extraction`: the generator: inputs, parsing, guards, annotations, merging across versions and the no-silent-drop coverage report.
- `builtin-data-oracle`: Reflection fixtures, gap filling for 8.0 and 8.1, constant values, cross-check and allowances.
- `builtin-data-provenance`: vendored sources, manifest hashes, NOTICE, facts-only rule, regeneration check and the update procedure.

### Modified Capabilities
- `workspace-layout`: the domain layer gains the built-ins crate and the tooling layer gains the generator crate.

## Impact

- New crates `crates/builtins` (domain, no dependencies) and `crates/builtins-gen` (tooling; may depend on syntax and builtins, and on `serde_json` for fixtures); `architecture.toml` and the example binary in `adapters/syntax-mago` (dev-dependency on the generator crate, same pattern as the spike).
- New repository content: `third_party/php-src/` (about 5 MB of vendored stubs plus licences), `stubs/builtin.data` (generated), `stubs/MANIFEST.toml`, `stubs/reflection/*.json` fixtures, `NOTICE`, `docs/builtin-data.md`.
- `bleedthrough-fs` later embeds `stubs/builtin.data` through the port (in `symbols-and-types`).
- Mago must parse the 8.5 stub grammar; this is verified first and may need a recorded known gap or a Mago pin upgrade through the existing upgrade procedure.
- Downstream: `symbols-and-types` consumes the data and drops its hand-written stub plan; `v1-rule-catalog` can validate that every function named in a rule exists in the data.
