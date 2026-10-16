# Testing

How the project tests itself, and what a change must bring. The specifications in `openspec/specs/`
(`testing-strategy`, `property-testing`, `coverage-reporting`, `mutation-testing`) are the contract; this
page is the practical guide. A passing suite for a security tool proves little unless the tests would
notice broken logic, so this page covers four things: what tests exist, whether they run the code
(coverage), whether they would catch a fault (mutation) and whether each specified behaviour has a test
(traceability).

## Test layers

| Layer | Lives in | For | Must not |
|---|---|---|---|
| Unit | `#[cfg(test)] mod tests` beside the code | one function or type, its edge cases and its error paths | touch the file system outside a temp directory, start processes, or depend on other crates' internals |
| Integration | `<crate>/tests/*.rs` | a crate's behaviour through its **public** interface only | reach into private modules or rely on another crate's test code except `bleedthrough-testkit` |
| Corpus and conformance | `fixtures/`, `crates/syntax-conformance`, the `vuln_corpus` and `conformance` tests | shared fixtures that every backend or the whole pipeline must agree on; labelled expectations | embed expectations in code that belong in the fixture's manifest, or need a network |
| End to end | tests that run the built binary (`bleedthrough-cli`, `bench`, `xtask eval`) | the argument, output and exit-code contract of the real executable | assert on internals, or run before the binary is built |
| Property | beside the unit or integration tests it complements, using `proptest` | invariants over generated input: round trips, ranges, ordering, no panics | replace the example test of a specified scenario (see below) |

## Every specified scenario is tested

Every scenario in `openspec/specs/*/spec.md` is covered by at least one automated test, and a pull request
that adds or changes a scenario adds or changes its test. A test covering a scenario carries a marker doc
line naming the capability and the requirement title:

```rust
/// spec: finding-model/Findings have a stable fingerprint
#[test]
fn the_same_finding_gets_the_same_fingerprint() { /* ... */ }
```

## Traceability report

`cargo xtask spec-trace` reads the markers and the requirement titles and reports uncovered scenarios, stale
markers (a requirement that was removed or renamed) and requirements covered only by properties. The report
is advisory and is read when a change is archived (`docs/architecture-review.md`). A scenario that cannot be
tested automatically is listed with its reason in the change's tasks and verified by a recorded manual
procedure; the report lists it as manual.

Markers are added as tests are touched and for every new change; there is no back-fill obligation.

## Hermeticity

Tests in the required status do not use the network; need a PHP interpreter, a container runtime or any tool
outside the pinned toolchain; read or write outside a temporary directory or the repository; or depend on the
current time, the machine's locale, the working directory or environment variables the test did not set.

A test that needs such a resource is ignored by default with a reason, and runs only in a labelled job:

```rust
#[test]
#[ignore = "needs a local PHP interpreter; run with `cargo test -- --ignored` where php is installed"]
fn the_interpreter_agrees() { /* ... */ }
```

The labelled job is `determinism.yml` (its `interpreter` job runs `cargo test --workspace -- --ignored` with
PHP installed). Time and locale independence are review-checklist items, not machine-checked.

## Ignored tests

Every ignored test carries a reason string. `cargo xtask check-tests` (required, offline) fails on an
`#[ignore]` without a reason and on a default test that runs `php`, `docker` or the interpreter helpers, and
prints how many tests are ignored and why.

## Determinism

A test gives the same result on every run, in any order and with any thread count. Randomised tests use a seed
that is fixed by default, printed on failure and settable to reproduce it (`BLEEDTHROUGH_PROPTEST_SEED`, below).
The scheduled `determinism.yml` workflow runs the suite with one thread and with the default thread count and
fails on any difference.

## Negative tests

Code that decides whether data is tainted, sanitised, trusted or reachable, that resolves names or types, or
that parses untrusted text has tests for the case where the decision is "not". A change adding such a decision
includes the opposite-outcome test beside each positive test: if a sanitizer is recognised, a test shows an
unsuitable sanitizer is not accepted.

## Property tests

Properties use [`proptest`](https://docs.rs/proptest) through the shared dev-only crate `bleedthrough-testkit`
(`crates/testkit`). It depends on no workspace crate, so any crate can use it as a `[dev-dependencies]` entry
without a cycle, and `architecture.toml` keeps it out of every product crate's normal dependencies. A release
build contains neither it nor `proptest` (`cargo tree -p bleedthrough-cli -e normal` lists neither).

```rust
use bleedthrough_testkit::proptest::prelude::*;
use bleedthrough_testkit::{check, php_identifier};

#[test]
fn names_round_trip() {
    check(php_identifier(), |name| {
        prop_assert_eq!(parse(&render(&name)), Ok(name));
        Ok(())
    });
}
```

- **Generators.** Generic strategies (`php_identifier`, `php_qualified_name`, `php_source`, `relative_path`,
  `version_triple`, `byte_vec`, and `mutated` for near-valid input) live in the testkit. Strategies for a
  crate's own types live in that crate's tests (see `crates/builtins/tests/properties.rs`); this duplicates a
  little code and keeps the dependency graph acyclic and the public API snapshots untouched.
- **Seeds.** Runs use a fixed seed by default, so the suite is deterministic. `BLEEDTHROUGH_PROPTEST_SEED=<u64>`
  repeats a seed and `BLEEDTHROUGH_PROPTEST_CASES=<n>` raises the case count (default 256 per property).
- **Failures.** `check` prints the reason, the minimised input, the seed and the settings that reproduce it. A
  failing input found by a longer run is fixed together with a plain regression test containing that input;
  proptest also saves it in a `proptest-regressions/` directory beside the crate's tests, which is committed.
- **Budget.** The whole property suite runs in about 3 seconds at the default case count; the budget in
  required CI is 60 seconds. Long runs (for example `BLEEDTHROUGH_PROPTEST_CASES=10000`) happen in the scheduled
  `determinism.yml` workflow.
- **What needs properties.** New pure logic of these kinds carries properties: normalisation or ordering of
  values (idempotent, order-independent, absorbing as specified), name or path resolution (never panics, stable,
  results within the declared universe), and parsers of untrusted text (arbitrary bytes and near-valid
  mutations never panic, never exceed stated limits, report errors). The review checklist has an item for it.
- **Examples still needed.** Properties complement example tests; every specified scenario keeps an example
  test with a readable expected value. The traceability report lists a scenario covered only by a property.

## Coverage

`cargo xtask coverage` runs the workspace tests under instrumentation (`cargo llvm-cov`, which needs the
`llvm-tools` rustup component) and writes to `target/coverage/`: `coverage.json` (per crate and per file),
`summary.md` (the table), `lcov.info`, and with `--base REF` the lines added since `REF` that no test executes
(`changed-lines.txt`). `--compare FILE` adds the change against an earlier `coverage.json`, `--check` enforces
the floors, and `--from-lcov FILE` reuses a previous instrumented run.

- **Exclusions** are listed in one place, `testing/coverage.toml`: test directories, examples, benches, the
  `xtask` crate and vendored or generated files. `#[cfg(test)]` items and `#[test]` functions inside product
  files are excluded as well (found by parsing each file), so test code never counts as covered code.
- **Branch coverage is unavailable** on the pinned 1.99.0 toolchain: `cargo llvm-cov --branch` passes
  `-Z coverage-options=branch`, which only a nightly compiler accepts, and the project has no nightly job. The
  report says so and does not show branch coverage as zero. (Checked when `llvm-tools` was added, with
  cargo-llvm-cov 0.9.1.)
- **Floors** (`testing/coverage-floors.toml`): each domain crate has a minimum line coverage and a stated
  reason. A crate below its floor fails, naming the crate, floor and figure; a crate more than `margin`
  points above its floor also fails until the floor is raised, so floors track reality. Raise a floor with
  `cargo xtask coverage --update-floors` or by hand, and review the reason in the diff. A placeholder crate
  with no executable lines has floor 0 and cannot satisfy the stale rule once code lands.
- **Where it runs.** `coverage.yml` publishes the table, the comparison with the target branch and the added
  lines no test executes as a job summary on each pull request, and runs the floor check. None of it is in the
  required `ci` status. The measured cost of one instrumented run, workspace tests included, was 51 seconds on
  a development machine with a warm build cache (and about 74 seconds for the first run). Because the CI
  figure on a cold runner has not been measured yet, the floor check stays in `coverage.yml`; it moves into
  `ci.yml` (a job in `ci`'s `needs`) once `coverage.yml` shows a total below the 10 minute budget on CI.
- **Pins.** `cargo-llvm-cov` is installed at an exact version (`CARGO_LLVM_COV_VERSION` in `coverage.yml`,
  0.9.1 today) and is never a build dependency. Adding the `llvm-tools` component to `rust-toolchain.toml` is a
  reviewed edit of the pin file on the same channel, 1.99.0: it installs the LLVM profile tools matching that
  compiler and changes nothing else.
- **Coverage is not a quality claim.** It shows only which lines the tests execute, not whether they check
  anything: a test that calls a function and asserts nothing covers it. Mutation testing (below) is the measure
  of whether the tests detect faults. A coverage figure is not a release criterion and is not used in public
  claims.

## Mutation testing

`cargo xtask mutants` runs [`cargo-mutants`](https://mutants.rs) (pinned in `mutants.yml`, 27.1.0 today) over the
domain crates listed in `testing/mutants.toml` and writes `report.json` (caught, missed, timeout and unviable
counts per crate and file, and each missed mutant with its line and change) and `outcomes.txt` (sorted, no line
numbers: two runs on one commit are byte-identical) to `target/mutants/`. Options: `--package P`, `--in-diff REF`
(only code changed since `REF`), `--shard K/N`, `--jobs N`, `--from-outcomes FILE`, `--no-check`.

- **Score** is caught / (caught + missed). Timeouts and unviable mutants (code that does not compile) are
  reported but count neither way. The timeout is fixed (`timeout_secs`) so runs are reproducible.
- **Exclusions** live in `testing/mutants.toml`: `Display`/`Debug` implementations, logging, and test, example,
  bench and generated files.
- **Survivor ledger** (`testing/mutants-survivors.toml`): every tolerated survivor has a crate, file, mutant
  description (no line number), a reason and an `equivalent` flag, and an optional `count` for identical
  repeats. The run fails on a survivor that is not listed, and on a listed entry whose mutant is now caught or
  gone (so the ledger shrinks as tests improve). A partial run (`--in-diff`, `--shard`) skips the stale check.
- **Critical code** (`testing/mutants-critical.toml`): severity and confidence, fingerprints, limits, taxonomy
  lookups, name and type resolution. A survivor there is killed with a test; the ledger accepts only mutants
  proved equivalent, and says why.
- **Where it runs.** `mutants.yml`: weekly, on demand, and on a pull request labelled `run-mutants` (limited to the
  changed code); one job per crate. It is never part of the required `ci` status. A full run of the six crates
  took about 20 minutes with 8 parallel jobs on a development machine.
- **Baseline debt.** When mutation testing was introduced, 1482 survivors were recorded in the ledger with
  `baseline = true` and no analysis. Such entries are tolerated even in critical code, still go stale when their
  mutant is caught, and are never added to. The `mutation-hardening` change removes them.
