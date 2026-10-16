## 1. Vulnerability corpus

- [x] 1.1 Define the manifest format (cases, shapes vocabulary, twins, expectations, protected ranges, known gaps, provenance) in `fixtures/vuln/README.md` and `schemas/`, and verify with a sample manifest that parses
- [x] 1.2 Implement the corpus loader and completeness check in `bleedthrough-eval` (orphans, missing files, duplicate ids, twin pairing, CWE validity via the taxonomy, size limit, licence allow-list, line ranges inside files) with unit tests for each failure scenario in the spec
- [x] 1.3 Add `cargo xtask check-corpus` (offline) and verify it passes on the sample and fails on a deliberately broken copy in a test
- [x] 1.4 Write the shape-coverage report per CWE and verify it lists a missing `wrong-sanitizer` case in a test fixture
- [x] 1.5 Write the initial cases with safe twins for SQL injection, command injection, XSS, path traversal, unsafe deserialization, SSRF and open redirect across the shapes in the spec, and verify `check-corpus` passes and the coverage report is committed to `docs/evaluation.md`
- [x] 1.6 Verify every case file parses with the Mago backend and, where an interpreter is available, passes `php -l` (reuse the syntax suite's interpreter helper); add as a test in the adapter

## 2. Accuracy evaluation

- [x] 2.1 Add the crate `bleedthrough-eval` (tooling, domain dependencies only), register it in `architecture.toml`, and verify `cargo xtask check-layering` passes
- [x] 2.2 Implement report reading with schema validation against `schemas/report.schema.json`, and verify an invalid report is rejected with the validation error
- [x] 2.3 Implement deterministic matching (TP, FP, FN, unlabelled) and metrics with undefined values, and verify with hand-written reports covering every matching rule, order independence and known gaps
- [x] 2.4 Implement the result document (JSON with `schema_version`, plus a table) and verify byte-identical output on repeated runs
- [x] 2.5 Implement expectations (`min_tp`, `max_fp`, known gaps, `scan-switch`), the ratchet (including "known gap now passes") and `--update` with a diff printout, and verify each spec scenario in tests
- [x] 2.6 Add `cargo xtask eval` running the binary as a subprocess, mapping verdicts to exit codes 0, 1 and 3, and verify that today it prints `not evaluated` with the reason and exits 3
- [x] 2.7 Add the `corpus` and `accuracy` jobs to `ci.yml` (and to the `ci` job's `needs`), with `not-evaluated` accepted only while the switch is false, and verify by a CI dry run or a local script that flipping the switch makes it fail

## 3. Synthetic corpus generator

- [x] 3.1 Implement the seeded generator in `bleedthrough-bench` (parameters per the spec, no new RNG dependency) and verify two runs with one seed produce identical tree hashes
- [x] 3.2 Verify through the Mago adapter example that a generated tree of 2,000 files parses with no errors, and that 100,000 files can be generated within a documented time and disk budget
- [x] 3.3 Implement the label-file output for planted patterns in the corpus expectation format and verify the evaluator loads it

## 4. Benchmark harness

- [x] 4.1 Add the crate `bleedthrough-bench` (tooling), register it in `architecture.toml`, and verify layering passes
- [x] 4.2 Implement stage registration, input description and the result document (with schema in `schemas/`), and verify an unavailable stage serialises with its reason
- [x] 4.3 Implement the parse stage generic over `ParserBackend` and the `bench_parse` example in `adapters/syntax-mago`; verify it reports file, byte and node counts for a synthetic tree
- [x] 4.4 Implement timing (warm-ups, iterations, median, spread, minimum-iteration refusal) and `/proc/self/status` peak RSS with an unavailable fallback; verify with unit tests on injected samples
- [x] 4.5 Implement the allocation mode using the `dhat` allocator, review the dependency against `deny.toml`, and verify the counters are identical across three runs on the reference input
- [x] 4.6 Implement the end-to-end stage that runs the binary as a subprocess and verify it reports unavailable with the reason today
- [x] 4.7 Implement the committed baseline for structural and allocation counters (`benchmarks/baselines.toml`), the offline check, and `--update` with a diff; verify that a deliberate doubling in a test is reported with stage, counter and both values
- [x] 4.8 Add the `bench-counters` job to `ci.yml` and to the `ci` job's `needs`, and verify it passes on the reference input and runs in under two minutes

## 5. Comparison and workflows

- [x] 5.1 Implement result comparison (medians, noise from base samples, threshold, environment match, `--force`) and verify the spec scenarios (within noise, clear regression, different machines) with unit tests on synthetic samples
- [x] 5.2 Implement `cargo xtask bench-compare <base> <head>` building both revisions into separate target directories and running alternately; verify on two local commits
- [ ] 5.3 Add `.github/workflows/bench.yml` (schedule and manual dispatch): fetch cached corpus, run the comparison against the last successful scheduled commit, upload result documents, write the step summary; verify with a manual dispatch run and confirm it is not in the `ci` job's `needs`
- [x] 5.4 Document each stage's timed region and the three metric kinds in `docs/benchmarks.md`, and verify the document states what is inside and outside every stage

## 6. Real-world corpus

- [x] 6.1 Define `corpora/real-world.toml` and the checker (pin fields, tier coverage, framework/non-framework/legacy mix, licence recorded), and verify the check fails on a missing hash in a test
- [x] 6.2 Implement `cargo xtask fetch-corpus` with hash verification, safe extraction (no traversal, no escaping symlinks) and the cache directory; verify with a local fixture archive including a traversal attempt and a hash mismatch
- [ ] 6.3 Choose and pin the initial projects (at least one per tier) from the documented criteria, record commit, archive hash, licence and reason, and verify the fetch command succeeds and the pins are reviewed
- [x] 6.4 Implement the parse-failure budget check (`corpora/parse-budget.toml`), excluding files PHP itself rejects with the recorded verdict, and verify a regression and an improvement are both reported with file lists
- [x] 6.5 Implement the robustness and determinism check (twice, different worker counts, byte-identical reports; time limit; panic detection) that runs when a scan exists, and verify it reports unavailable today
- [x] 6.6 Verify offline behaviour: with an empty cache and no network, all required-CI checks pass and corpus-dependent checks report unavailable

## 7. Documentation and handoff

- [x] 7.1 Write `docs/evaluation.md`: how to add a case and a twin, how matching works, how to read and update expectations, the `scan-switch`, and the rule that the future scan change must flip it
- [x] 7.2 Update `CONTRIBUTING.md` and `docs/architecture.md` (tooling list, gates table) and verify `cargo xtask check-glossary` and `check-layering` still pass
- [x] 7.3 Final check: `cargo build --workspace --all-targets --locked`, `cargo test --workspace --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all --check`, `cargo deny check`, `cargo xtask check-api` all pass
