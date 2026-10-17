# Benchmarks

The benchmark harness measures the pipeline stage by stage on a generated input, so a performance
change shows up where it happened. It is tooling: nothing here ships.

## Three kinds of metric

| Kind | What | Where it is checked | Why |
|---|---|---|---|
| Input counters | files, bytes, nodes, parse errors | `benchmarks/baselines.toml`, exact | Prove the input did not drift. |
| Allocation counters | allocation count, total and peak bytes of one parse pass | `benchmarks/baselines.toml`, within a 2% tolerance, in CI on every change | Deterministic and machine-independent, so they can gate. |
| Wall time and peak RSS | median of at least 5 samples with a median-absolute-deviation spread; `VmHWM` from `/proc/self/status` (absent elsewhere, recorded as `null`) | `cargo xtask bench-compare` and the `bench` workflow, never per pull request | Noisy: only meaningful between two builds on one machine. |

## Timed regions

* **parse**: parsing every file of the input with the backend, from bytes already in memory. File
  reading, input generation and node counting are outside the timed region (counters are taken
  untimed, before and after sampling, and must agree).
* **analysis**: for every file, parsing, summarising, inferring and lowering, then the symbol table, the
  call graph, the whole-project taint analysis and finding production, on one thread under the default
  limits (see [engine.md](engine.md)). The built-in data and the rules are loaded outside the timed region.
  Counters: `files`, `units`, `findings`. `bench-counters` also records its allocation counters, on a
  smaller input with planted vulnerabilities (`cli::ANALYSIS_INPUT`), so the finding count is not zero.
* **end-to-end**: running `bleedthrough scan <dir> --format json` as a subprocess, start to exit.
  It reports `unavailable` with its reason until the binary has a `scan` command.

A stage that cannot run is recorded as `unavailable` with a reason; it is never omitted and never
zero. Fewer than five iterations are refused.

## Commands

```text
cargo xtask bench-counters [--update]     check (or re-record) the committed counters; offline, seconds
cargo xtask bench-compare BASE HEAD       build both, measure alternately (3 rounds), compare
cargo run --release -p bleedthrough-syntax-mago --example bench_parse -- [options]
```

`bench_parse` options: `--input synthetic:<parameters>` or `dir:<path> --name real:<id>`, `--tree DIR`
(also write the synthetic tree), `--iterations`, `--warmups`, `--binary PATH`, `--commit REV`,
`--out FILE`. `--update` of the baselines prints a diff; review it like any change.

## Comparison rule

A stage regresses when its median is more than `max(10%, 3 x the base's relative spread)` slower.
Documents from different machines (OS, CPU model, core count, profile) are refused unless `--force`,
and a forced comparison is labelled as such. Inputs must be byte-identical (file and byte counts).

## Synthetic input

`bleedthrough-bench gen <parameters> --out DIR` writes a deterministic tree: `seed`, `files`,
`avg_bytes`, `class_pm`, `depth`, `include_pm`, `vuln_pm`. Each file depends only on its index, so
trees are reproducible and cheap to hash (`bleedthrough-bench hash`). With `vuln_pm > 0` it plants
known vulnerable patterns, each with a safe twin, and writes `labels.toml` in the corpus manifest
format; `cargo run -p bleedthrough-eval -- eval --labels --root DIR --report FILE` scores a report
against it.

Measured on the development machine: 100,000 files (about 102,000 with planted patterns, 265 MB of
source, 413 MB on disk) generate in under 2 seconds. A 2,215-file tree parses with no errors in
about 0.5 s (release). Generate large trees under `target/` or a temporary directory and delete them.

## Real-world projects

`corpora/real-world.toml` pins each project by exact commit and by the SHA-256 of the GitHub archive of
that commit, and records its licence, PHP versions, size tier, style (framework, non-framework,
legacy) and the reason it was chosen. The selection criteria are in that file's header; tiers count
`.php` files (small < 100, medium 100 to 999, large 1000 or more). Replacing a project is a reviewed edit.

```text
cargo xtask check-corpora                 offline: pins, licences, tier and style coverage
cargo xtask fetch-corpus [--tier T]       download, verify, extract into $BLEEDTHROUGH_CORPUS_DIR or target/corpus
cargo xtask parse-budget [--update]       files the parser recovers from or fails on, vs corpora/parse-budget.toml
cargo xtask check-robustness              scan twice with different worker counts (needs a scan command)
```

* Archives are hashed before they are read; a mismatch fails naming the project and both hashes and
  leaves any verified tree untouched. Extraction rejects absolute and `..` paths, hard links and
  symlinks that leave the tree, and archives that expand beyond a size limit; a refused archive
  leaves nothing behind. Fetched code is never executed, committed or redistributed.
* The parse budget is exact in both directions: more failures than recorded fails and lists the
  files; fewer fails until the budget is lowered. Files real PHP rejects (`php -l`) are excluded
  with the interpreter's verdict recorded.
* Every check that needs a fetched project reports `unavailable` (exit status 3), never a pass, when
  the cache is empty. Required CI fetches nothing; the `bench` workflow fetches and runs them.
* The robustness check runs `bleedthrough scan <dir> --format json --jobs N` twice with one and
  four workers and again with one, requires identical bytes, no panic and completion within ten
  minutes. `--jobs` is the contract the scan change must provide; until then it reports
  `unavailable`.
