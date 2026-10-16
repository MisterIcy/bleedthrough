## Why

Bleedthrough's two promises, "finds real vulnerabilities" and "fast", are unmeasured. The engine and the rule catalog are the next big changes; without a labelled vulnerability corpus, a scorer, a real-world code corpus and a benchmark harness in place first, every rule and every engine optimisation is judged by feel and regressions go unnoticed. The syntax fixture corpus already proves the parser; this change does the same for analysis quality and speed, before there is anything to regress.

## What Changes

- Add a **vulnerability fixture corpus** under `fixtures/vuln/`: small PHP cases labelled by CWE, each vulnerable case paired with a safe twin, with backend-neutral expectations (CWE, line range, minimum confidence) and tracked known gaps. Kept apart from the syntax corpus in `fixtures/php/`.
- Add an **accuracy evaluator**: scores a scan's machine-readable report against the corpus labels (true positives, false positives, false negatives per CWE and per rule), compares against committed expectations, and fails on regression. Black-box: it consumes the `json` report of the `bleedthrough` binary, so it also exercises the agent-facing contract. The gate reports "not evaluated" explicitly while no scan command exists.
- Add a **real-world corpus**: a pinned list of open-source PHP projects (URL, commit, archive hash, licence, size tier), fetched on demand into a cache, never vendored. Used for parser robustness (parse-failure budget), crash and determinism checks, and as benchmark input.
- Add a **deterministic synthetic corpus generator** producing realistic PHP trees of a chosen size from a seed, so scale benchmarks run offline in CI.
- Add a **benchmark harness**: stage-based measurements (parse now; later stages register themselves; end-to-end through the CLI once a scan command exists), wall time, throughput, peak memory, and deterministic allocation counters; versioned result files; a same-machine base-versus-head comparison; committed baselines only for deterministic counters.
- Add CI wiring: offline accuracy and corpus checks in the required status; real-world fetches, wall-time benchmarks and regression comparison in a separate scheduled and on-demand workflow that is not a required status.
- Two new tooling crates (`bleedthrough-eval`, `bleedthrough-bench`) and `cargo xtask` commands to drive them; documentation under `docs/`.

## Capabilities

### New Capabilities
- `vulnerability-fixture-corpus`: the labelled vulnerability cases, their manifest, pairing rule, known-gap tracking, completeness and provenance checks.
- `accuracy-evaluation`: scoring a report against the labels, metrics per CWE and per rule, expectation file, ratchet, and the explicit not-evaluated state.
- `real-world-corpus`: pinned third-party projects, on-demand verified fetch, parse-failure budget, crash and determinism checks, and the synthetic generator.
- `performance-benchmarks`: stages, metrics, result format, comparison and regression policy, and what is gated where.

### Modified Capabilities
- `build-and-quality-gates`: the "Continuous integration runs every gate" requirement gains the offline accuracy and corpus checks, and states that networked and timing-sensitive jobs are outside the required status.

## Impact

- New: `fixtures/vuln/`, `corpora/real-world.toml`, `benchmarks/` (baselines), `crates/eval`, `crates/bench`, an example in `adapters/syntax-mago`, `xtask` subcommands, a scheduled workflow, `docs/evaluation.md` and `docs/benchmarks.md`.
- New dev-time dependencies in tooling crates only (hash and archive reading, JSON, allocation statistics); each reviewed against `deny.toml`. No product crate changes and no runtime dependency changes.
- Layering: both new crates are tooling, depend only on domain crates, and are driven by xtask and adapter examples. The end-to-end stage runs the built binary as a subprocess, so no tooling crate depends on the CLI.
- Depends on nothing unapplied: it measures what exists (parser) and defines the contract for what comes next. `symbols-and-types` and later changes add their stages and cases without changing this harness.
