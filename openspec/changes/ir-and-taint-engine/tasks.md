## 1. IR crate

- [x] 1.1 Define graph types (`Graph`, `Block`, `Instr`, `Terminator`, `ValueId`, `LocalId`, spans, call-resolution slot) in `bleedthrough-ir`, with a textual dump, and verify with golden dump tests of hand-built graphs
- [x] 1.2 Implement the structured builder (blocks, break/continue stack, `finally` continuation stack) and verify tests for if/else, loops with levels, switch, match, try/catch/finally with return, goto, and short-circuit operators
- [x] 1.3 Lower expressions: values, locals, properties, static properties, array elements, globals, casts, binary/unary, concatenation and interpolation with per-part spans, closures and arrow functions with captures; verify against the dump goldens
- [x] 1.4 Lower calls of every form attaching the `CallResolution` from `FunctionFacts` (or the "none" marker), plus `new`, `include*`, `eval`, `echo`/`print`, `yield`, `throw`, `exit`; verify with tests that resolution and spans are carried
- [x] 1.5 Make lowering total: unsupported kinds and error nodes become counted unknown instructions and mark the graph recovered; verify with a test over every `ExprKind` and `StmtKind` and with the syntax conformance fixtures (no panic, deterministic dump)
- [x] 1.6 Lower units: functions, methods, closures, arrow functions, property hooks and file top-level; verify in `adapters/syntax-mago/tests/ir.rs` over `fixtures/inference` and `fixtures/php/realistic` with golden dumps
- [x] 1.7 Add properties (lowering never panics on mutated fixtures; dumps are stable; block and value numbering is in source order) and `/// spec:` markers; verify `cargo test -p bleedthrough-ir` passes and update `api/bleedthrough-ir.txt`

## 2. Call graph and includes

- [ ] 2.1 Define the engine's include-resolver port and the constant folder (literals, concatenation, `__DIR__`, `__FILE__`, `dirname(__FILE__)`, known constants); verify folding and non-folding cases with unit tests including negative cases
- [ ] 2.2 Build the call graph from lowered units and resolutions (single, multi-target with cap and truncation flag, built-in, unresolved with reasons); verify with tests for each scenario in the `call-graph` spec
- [ ] 2.3 Implement iterative Tarjan SCC with deterministic order and verify mutual recursion, self-recursion, file-order independence and a 100,000-node chain without stack overflow
- [ ] 2.4 Record unresolved calls and includes in the coverage structure and verify counts by reason in a test

## 3. Intra-procedural taint

- [ ] 3.1 Build the rule index (sources, sinks, sanitizers, propagators keyed by resolved symbol, with `+` subclass matching through the table) and verify lookups by function, static and instance method, constructor and superglobal
- [ ] 3.2 Implement the label domain (origin, cleared bitset over `SinkClass`, capped origin set, join rules) and verify join and clearing properties with proptest (commutative, associative, idempotent, clearing never adds taint)
- [ ] 3.3 Implement the forward dataflow over blocks with strong updates and join by union, array collapsing, and the abstract locations for properties, static properties and globals; verify the flow-sensitivity scenarios (overwrite, one-branch taint)
- [ ] 3.4 Implement sinks with vulnerable-argument selection, sanitizer clearing by class, partial sanitisation and applied-sanitizer tracking; verify right-sanitizer, wrong-sanitizer, partial and non-vulnerable-argument scenarios with negative tests
- [ ] 3.5 Implement the default call policy, propagators, the type prune and the cast rules; verify `strlen`, `strtoupper`, int cast, unknown type scenarios
- [ ] 3.6 Handle `foreach`, `list` destructuring, `yield`, closure captures, by-reference arguments and `match`/ternary/coalesce; verify each with a small program in the Mago-hosted tests
- [ ] 3.7 Add the engine test rule sets under `crates/engine/tests/rules/` and an intra-procedural test over `fixtures/vuln` cases that need no calls; verify the expected findings and no findings on safe twins

## 4. Summaries and inter-procedural flow

- [ ] 4.1 Implement summaries (returns, writes, sinks per symbolic origin, introduced sources) and substitution at calls; verify helper-flow and sanitising-helper scenarios
- [ ] 4.2 Implement component-ordered summarisation with fixpoint inside recursive components and the iteration limit; verify the recursive-function scenario and non-convergence widening
- [ ] 4.3 Implement multi-target application with the several-candidates mark and unresolved-callee propagation with its mark; verify the confidence reasons attach
- [ ] 4.4 Implement the outer property/global rounds (pure, bounded, deterministic) and the shared-scope include modelling; verify property flow across methods and a literal-include flow across files
- [ ] 4.5 Run the whole `fixtures/vuln` corpus through lowering, call graph and summaries in `adapters/syntax-mago/tests/engine.rs`, record expectations per CWE with known gaps, and verify no unexplained failure

## 5. Pattern rules

- [ ] 5.1 Implement `call` patterns with argument conditions on folded values and resolved targets; verify the function-match, same-short-name and absent-argument scenarios
- [ ] 5.2 Implement `use` patterns (calls, `new`, static access, literal callable strings, first-class callables) and `include-non-literal`; verify the callable-string and folded-path scenarios
- [ ] 5.3 Apply version and paranoid gating to taint and pattern rules and verify both gating scenarios

## 6. Findings, determinism and limits

- [ ] 6.1 Produce findings from sink hits with traces assembled from label provenance (source, propagation, call, considered sanitizer, sink), de-duplication per (rule, source, sink) and the trace length cut; verify the two-sources, two-paths and long-trace scenarios
- [ ] 6.2 Implement the confidence mapping and reasons, severity and taxonomy from rules, fingerprints via the findings model, and coverage counters; verify the clean, multiple-reason, medium-only and fingerprint-stability scenarios
- [ ] 6.3 Implement the order-independent work-unit API (`lower_file`, `summarise_component`, `scan_unit`, `merge`) and canonical ordering; verify with a test that shuffles files and unit order and compares results byte for byte
- [ ] 6.4 Implement every limit with widening, counters and the widening reason, using explicit work stacks in graph algorithms and bounded expression-depth lowering; verify one test per limit kind and the deep-nesting, long-chain and 1,000-function-cycle adversarial cases
- [ ] 6.5 Convert internal faults in one unit into a skipped-unit record without aborting the run and verify with an injected fault test
- [ ] 6.6 Add properties: results unchanged by inserting a dead assignment or unrelated function; adding sanitisation never increases findings; adding a source never removes findings; arbitrary mutated PHP never panics; verify they pass with the default seed

## 7. Validation and integration

- [ ] 7.1 Add the in-process scoring entry point to `bleedthrough-eval` (score `Finding` values against the corpus labels) and verify it agrees with the JSON path on a sample
- [ ] 7.2 Register the `analysis` stage in `bleedthrough-bench` using the synthetic generator and verify the stage reports file, unit and finding counts and an allocation baseline, updating `benchmarks/baselines.toml` deliberately
- [ ] 7.3 Write `docs/engine.md` covering the IR, the taint model, the default call policy and its known weaknesses, confidence mapping, every limit with its default, and the work-unit API; verify the `analysis-limits` documentation scenario
- [ ] 7.4 Update `architecture.toml` comments, `docs/architecture.md`, `docs/glossary.md` (terms: Label, Summary if public) and API snapshots; verify `cargo xtask check-layering`, `check-glossary` and `check-api` pass
- [ ] 7.5 Run the mutation baseline on `bleedthrough-ir` and `bleedthrough-engine`, kill survivors in taint, sanitiser, confidence and limit code, and ledger the rest with specific reasons; record coverage floors
- [ ] 7.6 Final check: `cargo build --workspace --all-targets --locked`, `cargo test --workspace --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all --check`, `cargo deny check`, `cargo xtask check-all` all pass
