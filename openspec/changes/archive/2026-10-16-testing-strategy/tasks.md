## 1. Hygiene and documentation

- [x] 1.1 Write `docs/testing.md`: the four layers with purpose, location and prohibitions; hermeticity and determinism rules; negative-test rule; how to run each tool; and verify every requirement of the `testing-strategy` spec is addressed by a heading
- [x] 1.2 Add `cargo xtask check-tests`: fails on an `#[ignore]` without a reason and on non-ignored tests that invoke the interpreter or the container runtime, prints the ignored count with reasons; verify with unit tests on sample sources and a clean run on the workspace
- [x] 1.3 Fix every violation `check-tests` finds in existing tests (add reasons, ignore interpreter-dependent tests with the label) and verify `cargo test --workspace` still passes offline
- [x] 1.4 Add the `check-tests` job to `ci.yml` and to the `ci` job's `needs`, and update `CONTRIBUTING.md` and `docs/architecture-review.md` (negative tests, properties for new pure logic, markers, determinism) and verify the layering and glossary checks still pass

## 2. Property testing

- [x] 2.1 Add `proptest` as a dev-dependency check in `deny.toml` review and create the tooling crate `bleedthrough-testkit` with no workspace dependencies; register it in `architecture.toml`, and verify `cargo xtask check-layering` passes with no cycle
- [x] 2.2 Implement generic strategies (PHP names, bounded PHP-ish source, relative paths, version triples, byte vectors) and the seed and case-count environment settings; verify with tests that the same seed yields the same values and that a failing property prints the seed and a minimised input
- [x] 2.3 Add round-trip properties for the built-in data text format and the taxonomy canonical forms (read what was written, identical bytes on second write) and verify they pass and a deliberately broken writer in a test makes one fail
- [x] 2.4 Add properties for fingerprint stability and difference, severity scoring range and monotonicity, and report emitter determinism and schema validity over generated reports; verify they pass
- [x] 2.5 Add no-panic and limit-respecting properties for every existing parser of untrusted text (rule files, Composer-free inputs available today, the built-in data reader) over arbitrary bytes and near-valid mutations; verify they pass or the findings are fixed with regression tests
- [x] 2.6 Commit proptest regression files, confirm the property suite adds no more than the documented budget to `cargo test`, and verify a release build of the CLI contains no testkit or proptest code (inspect `cargo tree -p bleedthrough-cli -e normal`)

## 3. Coverage

- [x] 3.1 Add the `llvm-tools` component to `rust-toolchain.toml` on the same version, record the pin-change note, and verify whether branch coverage is available on 1.99.0, recording the result in `docs/testing.md`
- [x] 3.2 Add `testing/coverage.toml` (exclusions) and `cargo xtask coverage` wrapping `cargo llvm-cov` with JSON and summary output; verify a run produces per-crate and per-file figures and that branch data is reported as unavailable if so
- [x] 3.3 Implement changed-lines reporting against a base ref and verify it lists the lines of a test branch that no test executes
- [x] 3.4 Record `testing/coverage-floors.toml` from a first measured run for every domain crate, implement `--check` with floor and stale-floor rules, and verify both failure scenarios with unit tests
- [x] 3.5 Add `.github/workflows/coverage.yml` publishing the summary and changed lines; measure the instrumented run time and either add the floor check to `ci.yml` (under the 10 minute budget) or keep it in `coverage.yml`, and record the decision in `docs/testing.md`

## 4. Mutation testing

- [x] 4.1 Add `testing/mutants.toml` (mutated crates, excluded functions, timeout) and `cargo xtask mutants` wrapping `cargo mutants` with `--package` and `--in-diff` options; verify a run on one small crate completes and produces caught, missed, timeout and unviable counts
- [x] 4.2 Implement the survivor ledger comparison (`testing/mutants-survivors.toml`, key without line numbers, reason, equivalent flag, critical globs in `testing/mutants-critical.toml`) and verify new-survivor, stale-entry and critical-nonequivalent failures with unit tests
- [x] 4.3 (baseline run done and recorded; killing the survivors moved to change `mutation-hardening` at the user's direction) Run the baseline on every domain crate; for survivors in critical areas (severity and confidence, taxonomy lookups, limits, fingerprint) write tests that kill them, and verify the re-run no longer reports them
- [x] 4.4 (the baseline is ledgered as `baseline = true` debt; the full run passes against it; reasoned entries move to `mutation-hardening`) Ledger the remaining survivors with specific reasons and verify the full run passes against the ledger and gives identical results on a second run
- [x] 4.5 Add `.github/workflows/mutants.yml` (schedule and dispatch, sharded by crate, publishes JSON) and verify with a manual dispatch run that it is not in the `ci` job's `needs`

## 5. Traceability and determinism

- [x] 5.1 Implement `cargo xtask spec-trace`: parse requirement titles from `openspec/specs/*/spec.md` and `spec:` markers from test sources; report uncovered scenarios, stale markers and property-only coverage in JSON and text; verify with a fixture spec and test file
- [x] 5.2 Add markers to the tests of the most recently added capabilities (finding model, emitters, rule schema, taxonomy, corpus, evaluation) and verify the report shows them covered
- [x] 5.3 Add a scheduled determinism job running the suite with one thread and the default thread count plus long property runs (`BLEEDTHROUGH_PROPTEST_CASES` raised), failing on any difference; verify by a dispatch run
- [x] 5.4 Add a line to the archive checklist in `docs/architecture-review.md` to read the traceability report for the change being archived

## 6. Final verification

- [x] 6.1 Run `cargo build --workspace --all-targets --locked`, `cargo test --workspace --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all --check`, `cargo deny check`, `cargo xtask check-all` and confirm all pass
- [x] 6.2 Confirm the required `ci` status contains only the fast offline checks and that coverage, mutation and determinism jobs are outside it
