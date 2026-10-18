## Why

Bleedthrough can parse PHP and knows names, types and call targets, but it cannot yet find a vulnerability. The `bleedthrough-ir` and `bleedthrough-engine` crates are empty. This change builds the heart of the tool: a control-flow IR and an inter-procedural taint engine that turns the rule data (sources, sinks, sanitizers, propagators, patterns) into findings with traces, confidence and severity. Everything user-visible that remains (the rule catalog, packs, the scan command, caching, the LSP) sits on top of it.

## What Changes

- Add an **IR** (`bleedthrough-ir`): a per-function control-flow graph of basic blocks and simple instructions lowered from the owned syntax tree, carrying spans and the call resolutions the type inference already computes. Total over the syntax model: constructs it cannot model become explicit, conservative instructions, never errors.
- Add a **call graph** over resolved calls (including hierarchy-based dispatch, literal `include` resolution and strongly connected components), with every unresolved edge recorded and counted.
- Add the **taint engine** (`bleedthrough-engine`): flow-sensitive, field- and array-collapsing taint analysis over the IR, with per-function summaries composed bottom-up over the call graph, class-aware sanitisation (a sanitizer clears only the sink classes its rule names), type-based pruning, and a defined policy for unresolved and built-in calls.
- Execute **pattern rules** (call, use, include-non-literal) with argument conditions.
- **Produce findings**: traces of source, propagation, call, considered-sanitizer and sink steps; confidence tier and reasons from the existing vocabulary; severity from the rule; fingerprints; coverage counters. Output is deterministic and independent of evaluation order.
- Enforce **analysis limits** with widening, explicit counters and no unbounded recursion, so adversarial or enormous code cannot hang or crash the engine.
- Expose the engine as **pure, order-independent work units** (lower a file, summarise a function group, scan a function) so the application layer can schedule them in parallel without the domain crates owning threads.
- Register an `analysis` stage in the benchmark harness and run the vulnerability corpus through the engine in-process to measure accuracy before the scan command exists.

## Capabilities

### New Capabilities
- `ir-lowering`: the IR's shape, totality over the syntax model, determinism, and span fidelity.
- `call-graph`: call edges, dispatch candidates, include resolution, SCCs, and unresolved accounting.
- `taint-analysis`: sources, propagation, sanitisation by sink class, summaries, type pruning, unresolved policy, entry data.
- `pattern-matching`: execution of pattern rules and their argument conditions.
- `finding-production`: findings, traces, confidence, severity, fingerprints, coverage, determinism and work-unit independence.
- `analysis-limits`: budgets, widening, counters, termination and stack safety.

### Modified Capabilities
<!-- none -->

## Impact

- Code: `crates/ir`, `crates/engine` (currently placeholders); small additions to `bleedthrough-eval` (score domain findings directly) and `bleedthrough-bench` (analysis stage); hosting integration tests in `adapters/syntax-mago/tests` (the pattern used for symbols) because they need a parser.
- Layering: no new edges are needed (`ir` already allows `symbols` and `syntax`; `engine` allows `ir`, `symbols`, `syntax`, `rules`, `packs`, `findings`, `taxonomy`). The engine does no I/O and no threading; include resolution is a port owned by the engine.
- Dependencies: none new in product crates. Dev-only: `proptest` (already admitted) in the new crates' tests.
- Public API snapshots, coverage floors and the mutation ledger gain entries for both crates (the `testing-strategy` rules apply: negative tests, properties for new pure logic, survivors only with reasons, strict standard for taint/sanitiser code).
- Depends on `symbols-and-types` (applied). Unblocks `v1-rule-catalog`, `pack-system` and `scan-pipeline`. The scan switch of `accuracy-evaluation` is flipped by `scan-pipeline`, not here.
