## Why

About 500 tests exist, but nothing says what testing a change must bring, whether the tests would notice broken logic, or how much code they reach. For a security tool, a green suite proves little: an inverted sanitizer check or a mis-ordered type union can pass every example-based test. `symbols-and-types` and the taint engine add exactly the kind of logic where this matters, so the practice should be set before they land.

## What Changes

- Define the **testing strategy**: test layers and where each lives, a rule that every spec scenario is covered by a test, and hermetic, deterministic tests (no network, no interpreter, no wall-clock dependence outside explicitly labelled tests).
- Add **property-based testing**: a shared dev-only set of generators and a fixed-seed policy, with first properties on existing code (canonical text formats round-trip, fingerprint stability, taxonomy and severity invariants, emitter determinism) and a standing requirement for new pure logic (types, name resolution, parsers of untrusted text).
- Add **coverage reporting**: line and branch coverage per crate as an advisory report published from CI, plus a per-crate floor for domain crates that can only be raised deliberately.
- Add **mutation testing** of domain crates on a schedule and on demand, with a committed list of tolerated survivors, each with a reason, so the score ratchets.
- Add an advisory **spec-to-test traceability** report and a hygiene check on ignored tests.
- Update `CONTRIBUTING.md` and the architecture review checklist.

## Capabilities

### New Capabilities
- `testing-strategy`: layers, spec-scenario coverage, hermeticity, determinism, ignored-test hygiene, traceability report.
- `property-testing`: shared generators, seeds and reproduction, mandatory properties for classes of code.
- `coverage-reporting`: how coverage is measured, published and floored.
- `mutation-testing`: scope, scheduling, survivor ledger and ratchet.

### Modified Capabilities
<!-- none: gates outside the required status are specified in the new capabilities -->

## Impact

- New dev-dependencies (`proptest`) in tooling and domain crates' `[dev-dependencies]` only; reviewed against `deny.toml`. Domain purity rules are unaffected (dev-dependencies are test-only).
- New external tools, installed in CI and optional locally: `cargo-llvm-cov` (needs the `llvm-tools` rustup component added to `rust-toolchain.toml`, a reviewed pin edit on the same version 1.99.0) and `cargo-mutants`. Neither is a build dependency.
- New: `xtask` subcommands, `testing/` data (coverage floors, mutation survivors), a scheduled workflow, `docs/testing.md`.
- No product behaviour changes. Required CI gains only the fast checks (coverage floors via a single instrumented test run if its cost is acceptable, ignored-test hygiene); mutation testing never blocks merges.
