## Context

See proposal.md. The ledger (`testing/mutants-survivors.toml`) holds 1371 entries, all `baseline = true`, 1109 of them in `bleedthrough-symbols` and 134 in `bleedthrough-syntax`; the proposal's figure of 1482 survivors was measured at recording time and is the sum of the entries' `count`s (1370 entries, 1482 survivors), so the first task re-runs the mutants to get the real starting set. `xtask/src/mutants.rs` parses the ledger (a `baseline` field), compares it with a run, and today tolerates `baseline` entries in critical code (`compare`, with a test for exactly that). Critical globs are in `testing/mutants-critical.toml`. `mutants.yml` and `determinism.yml` both have `workflow_dispatch`.

## Goals / Non-Goals

**Goals:**
- Leave a ledger of reasoned entries only, with critical code holding equivalent mutants only.
- Fix defects that surviving mutants expose, where they are real.

**Non-Goals:**
- A mutation-score threshold (rejected in the testing-strategy design) or new mutation tooling.
- Changing public API or behavior beyond defect fixes found on the way.

## Decisions

### D1. Work by tier, critical code first
Order: critical files (taxonomy, findings, symbols `types/` `table/` `name.rs`), then the remaining crates, then close-out. Critical code gets tests and no tolerated survivors, because the spec admits only equivalent mutants there. **Alternative:** go crate by crate: rejected, it leaves the security-relevant survivors for last.

### D2. Kill with behavior tests, not mutant-shaped tests
Each test asserts a spec scenario or an invariant (a property where one exists, using the `testkit` strategies), so one test usually kills several mutants. Tests are written per surviving function by reading the mutant list from `cargo xtask mutants --package P`, and carry the `/// spec:` marker while touched. A test that only pins a mutant's arithmetic is not accepted.

### D3. What counts as a reasoned ledger entry
Allowed reasons: provably equivalent (the proof is stated), unreachable behind a bound tested elsewhere (the test is named), or a timeout-only mutant. "Hard to test" is not a reason. Mutants in the large `symbols` `infer/` and `summary/` code that cannot be killed cheaply are grouped by function but each keeps its own entry, since the key is the mutant.

### D4. Loader refusal
The `baseline` field is removed from ledger parsing; an entry containing it is an error naming the entry, and so is a reason matching the "not yet analysed" wording. The existing xtask test that tolerates baseline entries is replaced by one that refuses them. This is done last (task 3.1) so the ledger is valid at every intermediate commit.

### D5. Ratcheting and commits
Entries are deleted as mutants die, in the same commit as the tests (the stale-entry rule enforces this). Coverage floors are raised once at the end from a fresh `cargo xtask coverage`, each with a reason, keeping the existing one-point margin.

## Risks / Trade-offs

- [Volume: over a thousand mutants, mostly in symbols] → tier ordering, per-crate shards, and killing by invariants; the remaining count is reported at each step.
- [A real defect surfaces and widens scope] → fix it small with a regression test, or open a separate change if it needs API change.
- [Slow mutation runs] → single-package and in-diff runs while working; full runs only at close-out dispatch.
- [Equivalence claims are wrong] → each critical equivalence carries a written proof, reviewed in the PR.
