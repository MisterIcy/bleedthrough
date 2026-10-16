## Why

The taint engine can only follow a flow from `$request->get()` into `$pdo->query()` if it knows which class `$pdo` is, which method a call reaches, and what that method's parameters and return type are. The owned syntax tree deliberately records names as written and docblocks as raw text, so someone must turn them into resolved names, a project-wide symbol table and types. This change builds that layer, on the accepted syntax model, before the IR and engine are designed on top of it. It keeps PHP semantics (namespaces, imports, inheritance, traits, late static binding, docblock types) in one tested place instead of leaking them into the engine.

## What Changes

- Add **name resolution** in `bleedthrough-symbols`: per-file namespace and `use` handling that resolves class, function and constant names to fully qualified names, with PHP's fallback rules for unqualified functions and constants, `self`/`static`/`parent`, `namespace\` names, group uses and aliases. Failures are explicit, never guessed.
- Add the **symbol table**: classes, interfaces, traits and enums with constants, properties (including promoted and hooked), methods and cases; functions and constants; inheritance, interface and trait composition (with `insteadof` and `as` adaptations); method and property lookup through the hierarchy; stable ordering. Built from per-file symbol summaries that are pure functions of one syntax tree, so they can be cached per file.
- Add the **type model**: a canonical, normalised union of atomic types (scalars with literal and refined forms, arrays with shapes and list forms, objects with optional generic arguments, class-string, callable, iterable, null, void, never, mixed, and an explicit *unknown* that is distinct from *mixed*), with a size cap that widens and records that it widened.
- Add **docblock type parsing**: an owned parser for the core PHPDoc and Psalm subset (`@param`, `@return`, `@var`, `@property`, `@method`, `@template` recorded), turning raw docblock text into types with diagnostics instead of failures, merged with native types by explicit rules.
- Add **intra-procedural type inference**: per function body, assignment-driven and flow-sensitive over branches with a small set of narrowing conditions, producing a type for each expression and a resolution for each call (resolved candidates, dynamic, or unknown with a reason). Callee return types come from declarations only; inferring them across functions belongs to the later inter-procedural step.
- Consume the **built-in data** produced by the `php-builtin-data` change (mechanically derived from php-src, complete for PHP 8.0 to 8.5, with per-version availability) through its `BuiltinDataSource` port: built-in functions, classes, interfaces, enums and constants enter the symbol table as `builtin` origin, with version-filtered lookup, so the table knows `PDO`, `Exception` and string functions. This change writes no stubs.
- Add **Composer autoload support**: a model of `composer.json` and `composer.lock` autoload rules (PSR-4, PSR-0, classmap, files) in infrastructure, implementing a class-locator port owned by the symbols crate, plus lazy loading of vendor classes by worklist until the referenced closure is complete or a limit is hit. The lock-file reading is reusable by pack activation.
- Extend layering, API snapshots, glossary and docs.

Out of scope: producing the built-in data (`php-builtin-data`), lowering to IR and taint propagation (`ir-and-taint-engine`), inter-procedural return-type inference, generics beyond recording templates and simple substitution, full Psalm/PHPStan type language, PHP-version gating of syntax, packs themselves, the CLI, and the cache store (this change only keeps per-file summaries cacheable).

## Capabilities

### New Capabilities
- `name-resolution`: namespaces, imports, special class names, fallbacks, and explicit failure.
- `symbol-table`: declarations, hierarchy, lookup, per-file summaries, deterministic closure building and limits.
- `type-model`: atomic types, canonical unions, widening, native and docblock type parsing and merging.
- `type-inference`: local flow-sensitive inference, narrowing subset, call resolution and its reasons.
- `builtin-symbols`: loading the built-in data into the symbol table, type conversion of its type text, version-filtered lookup, and the unknown-built-in report.
- `composer-autoload`: reading Composer autoload rules, the class-locator port, and lazy vendor loading.

### Modified Capabilities
<!-- None. Existing layering rules already allow symbols to depend on syntax and fs to implement ports. -->

## Impact

- `crates/symbols` gains names, summaries, table, types, docblock parser and inference; it depends only on `bleedthrough-syntax` (and the standard library).
- `crates/fs` gains the Composer reader, the class locator and the embedded built-in data (`stubs/builtin.data`, produced by `php-builtin-data`); `architecture.toml` lets it depend on symbols. `crates/app` will orchestrate loading in `scan-pipeline`; here a test harness does the orchestration.
- Depends on the `php-builtin-data` change being applied first (crate `bleedthrough-builtins`, `stubs/builtin.data`); `fixtures/` gains symbol and type fixtures.
- `serde_json` may be needed to read `composer.json`/`composer.lock`; it is added in `bleedthrough-fs` only and checked by `cargo deny`.
- Downstream: `ir-and-taint-engine` consumes resolved names, call resolutions and types; `pack-system` reuses the Composer lock reader; `analysis-cache` stores per-file summaries.
