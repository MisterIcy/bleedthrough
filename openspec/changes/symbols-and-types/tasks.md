## 1. Setup

- [ ] 1.1 Allow `bleedthrough-fs` to depend on `bleedthrough-symbols` in `architecture.toml` and add glossary terms (symbol table, fully qualified name, type, call resolution, summary); verify `cargo xtask check-layering` and `check-glossary` pass
- [ ] 1.2 Decide and record how the symbols crate hashes summaries (use `sha2` as findings does, or an explicit domain-purity allow-list entry); verify the domain-purity check passes
- [ ] 1.3 Create fixture directories `fixtures/symbols/` and `fixtures/types/` with a README on the golden-text dump format; verify a throwaway case round-trips through the dump

## 2. Name resolution

- [ ] 2.1 Implement `Fqn` (declared spelling plus case-insensitive key) and `ResolvedName`; verify case-insensitive equality and preserved display spelling
- [ ] 2.2 Implement per-file `Scope` building from namespace blocks and `use` items (aliases, group, function and const kinds, duplicate alias diagnostics); verify the group-use, alias and block-isolation scenarios
- [ ] 2.3 Implement class, function and constant resolution including relative names and the undecidable-fallback result; verify every scenario of the `name-resolution` spec
- [ ] 2.4 Implement `self`/`static`/`parent` resolution given the enclosing class-like, including closures and the failure cases; verify parent-without-parent and closure binding tests

## 3. Summaries

- [ ] 3.1 Define `FileSummary` and the declaration records (classes, interfaces, traits, enums with cases, functions, constants, `define`), with promoted parameters and hooks; verify the promoted-parameter scenario
- [ ] 3.2 Implement summary extraction as a single pass over the tree, tolerant of `Error` and `Unsupported` nodes; verify the summary of a file with an error node still contains the intact declarations
- [ ] 3.3 Record `referenced_classes` per file (declarations plus a body pre-pass); verify a class referenced only in a method body is listed
- [ ] 3.4 Implement the declaration fingerprint over a canonical form; verify body-only change leaves it unchanged and a signature change alters it
- [ ] 3.5 Add summary golden fixtures covering classes, traits with adaptations, enums, interfaces, hooks and nested namespaces; verify goldens and order-independence

## 4. Symbol table

- [ ] 4.1 Implement `SymbolTable::build` with sorted indexing, duplicate diagnostics and first-by-path lookup; verify supply-order independence and duplicate handling
- [ ] 4.2 Implement hierarchy linking (parents, interfaces, traits with `insteadof`/`as`, enum interfaces) with cycle detection and unresolved-ancestor marking; verify cyclic and missing-parent scenarios terminate
- [ ] 4.3 Implement member lookup (methods, properties, constants) in PHP order with visibility and magic-method reporting; verify inherited, trait and magic scenarios
- [ ] 4.4 Implement subtype, subclasses and implementors queries with deterministic order; verify the three-implementors scenario
- [ ] 4.5 Define `ClassLocator` and `ClassSource` ports (the built-in data port comes from `bleedthrough-builtins`) and implement `TableBuilder` with the worklist closure, limits and `LoadReport`; verify limit-hit and not-found scenarios using in-memory ports

## 5. Types

- [ ] 5.1 Implement atoms and the normalising `Type` constructor (absorption, ordering, `mixed`, `never`, literal cap); verify the normalisation and order-independence scenarios
- [ ] 5.2 Implement `unknown`, the `has_unknown` flag, the union cap with the `widened` flag and the subtype relation; verify the unknown-propagation and large-union scenarios
- [ ] 5.3 Implement stable rendering; verify rendering of every atom kind
- [ ] 5.4 Implement native-type conversion from `TypeHint` with name resolution (nullable, union, intersection, DNF, self/static/parent, enum backing); verify the nullable scenario and intersections kept
- [ ] 5.5 Implement the docblock lexer and parser for the core subset with depth limit and diagnostics; verify generic array, shape, class-string, callable, literal, and broken-type scenarios
- [ ] 5.6 Implement tag extraction (`@param`, `@return`, `@var`, `@property*`, `@method`, `@template` recording) and simple template substitution; verify multi-line tags and variadics
- [ ] 5.7 Implement native-versus-docblock merging with provenance; verify refinement, contradiction and missing-native scenarios
- [ ] 5.8 Add the parse-render round-trip test over a corpus of type strings; verify it covers every atom kind
- [ ] 5.9 Add a robustness test feeding random and truncated docblock text to the parser using the seeded generator pattern from the syntax conformance suite; verify no panic or hang

## 6. Built-in symbols

- [ ] 6.1 Confirm `php-builtin-data` is applied (crate `bleedthrough-builtins`, `stubs/builtin.data`); add the `symbols` to `builtins` edge in `architecture.toml`; verify `cargo xtask check-layering` passes
- [ ] 6.2 Load built-in records into the table with `builtin` origin, converting declared, tentative and docblock type text through the type parser and merging by the refinement rule; verify a method with a tentative return type and a function with a refined docblock type
- [ ] 6.3 Implement version-filtered lookup returning found, not-available-in-version or unknown, and surface guard text and unbundled state; verify the version-specific function and unbundled-extension scenarios
- [ ] 6.4 Report project declarations that collide with built-ins as diagnostics without shadowing; verify a project class named like a built-in
- [ ] 6.5 Implement the unknown-built-in report for referenced functions and classes absent from the data; verify with a fixture calling a nonexistent function
- [ ] 6.6 Embed `stubs/builtin.data` in `bleedthrough-fs` behind the `BuiltinDataSource` port; verify the table builds from the embedded data without file access at run time

## 7. Composer

- [ ] 7.1 Add `serde_json` to `bleedthrough-fs` only; verify `cargo deny check` and layering pass
- [ ] 7.2 Implement the `composer.json` and `composer.lock` reader (autoload, autoload-dev, packages, packages-dev) with diagnostics for malformed or missing input; verify the malformed-lock scenario
- [ ] 7.3 Implement `AutoloadMap` and the `ClassLocator` with Composer precedence, longest prefix, ordered directories, exclusions and path normalisation behind a small `Fs` trait; verify longest-prefix, escape-attempt and enumeration-order independence
- [ ] 7.4 Expose the ordered package list and `files` entries; verify the package-list and bootstrap-file scenarios
- [ ] 7.5 Add an integration test building a table from a fixture project with a fake `vendor/` through locator, source and builder; verify the vendor parent class is loaded and an absent class is reported unresolved

## 8. Inference

- [ ] 8.1 Define `FunctionFacts`, `ExprKey`, `CallResolution`, reason codes and statistics; verify keys sort deterministically
- [ ] 8.2 Implement the environment and expression typing for literals, variables, `new`, arrays, casts, interpolation, operators, null coalescing, ternary, `match` and closures; verify the new-expression and concatenation scenarios
- [ ] 8.3 Implement statement interpretation with branch joins, switch, try/catch/finally and loops with the iteration limit; verify the branch-join and loop-limit scenarios
- [ ] 8.4 Implement the narrowing subset and document it; verify instanceof, null comparison, `is_*`, `isset`/`empty`, literal comparison, negation and the unrecognised-condition scenario
- [ ] 8.5 Implement call, method call, static call, `new` and first-class-callable resolution against the table with all outcomes and reasons; verify interface-receiver, unknown-receiver and variable-function scenarios
- [ ] 8.6 Add per-function uncertainty counts and the mapping document to confidence reasons; verify the counts scenario
- [ ] 8.7 Add termination and robustness tests (error and unsupported nodes, deep nesting, huge unions); verify completion within limits
- [ ] 8.8 Add end-to-end inference fixtures (typical controller, repository, service with interfaces) with golden dumps of resolved calls; verify the goldens and review them for incorrect resolutions

## 9. Documentation and gates

- [ ] 9.1 Write `docs/symbols-and-types.md` (name rules, summary and table model, type syntax, narrowing subset, limits and defaults, how built-in data is consumed); verify examples in it are exercised by tests
- [ ] 9.2 Regenerate API snapshots, update `docs/architecture.md` and its diagram, and confirm the provisional defaults (vendor limits, union cap, loop passes, literal cap) are listed for review; verify the diagram matches `architecture.toml`
- [ ] 9.3 Run `cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo deny check`, `cargo xtask check-all` and `cargo test --workspace --locked`; verify all pass on the pinned toolchain
