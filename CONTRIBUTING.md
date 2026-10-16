# Contributing

Run these before opening a pull request; CI runs the same gates and all are required.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check                 # install once with: cargo install cargo-deny --locked
cargo xtask check-layering       # layer rule, workspace inheritance, domain purity, parser isolation
cargo xtask check-glossary       # public domain types name a glossary term
cargo xtask check-api            # public-API snapshots (use --update for intended changes)
cargo xtask check-builtins       # built-in data regenerates byte for byte (see docs/builtin-data.md)
cargo xtask check-tests          # ignored tests carry reasons; no default test needs PHP or Docker (docs/testing.md)
cargo xtask spec-trace           # advisory: which spec requirements have a marked test (docs/testing.md)
cargo xtask coverage             # line coverage and floors (needs cargo-llvm-cov; docs/testing.md)
cargo xtask mutants --package <crate> --in-diff origin/main   # mutation testing of your diff against the survivor ledger (needs cargo-mutants)
cargo xtask check-corpus         # vulnerability corpus: files, labels, twins, CWEs, licences (docs/evaluation.md)
cargo xtask check-corpora        # pinned real-world project list (docs/benchmarks.md)
cargo xtask bench-counters       # deterministic allocation/input counters vs benchmarks/baselines.toml
cargo xtask eval                 # accuracy evaluation; `not evaluated` until the scan exists (docs/evaluation.md)
```

`cargo xtask check-all` runs the offline `xtask` checks together. Wall-time benchmarks (`bench-compare`) and the
real-world corpus (`fetch-corpus`, `parse-budget`, `check-robustness`) need the network or a quiet machine and are
not required checks; see [docs/benchmarks.md](docs/benchmarks.md).

## Rules of the road

- **Toolchain:** only the version pinned in `rust-toolchain.toml`. Changing the pin is its own reviewed edit.
- **Layers:** a new dependency between workspace crates needs an entry, with a justifying comment, in `architecture.toml`.
- **Public API:** items are private by default. Making one public changes `api/<crate>.txt`; update it with `cargo xtask check-api --update` and expect review.
- **Language:** a public type in a domain crate carries a doc line `Term: <glossary term>` or `Term: exempt`. Terms live in `docs/glossary.md`.
- **No `unsafe`**, no new licenses outside the `deny.toml` allow-list (MIT-0 is allowed for the schema validator's `borrow-or-share`), and parser backends only through an adapter crate at an exact version.
- **Tests:** the bar is that tests would notice a broken implementation, not that they run. See [docs/testing.md](docs/testing.md).
  - Every spec scenario gets an example test with a `/// spec: <capability>/<Requirement>` marker; read `cargo xtask spec-trace` for your change before you finish.
  - Every security decision (taint, sanitiser, trust, reachability, name or type resolution, parsing of untrusted text) gets a negative test beside the positive one.
  - New pure logic (normalisation, ordering, resolution, parsers of untrusted text) gets properties through `bleedthrough-testkit`; properties complement example tests and never replace them.
  - Tests are hermetic and deterministic: no network, clock, locale or interpreter. A test needing PHP, Docker or the network is `#[ignore = "reason"]`. A fixed-seed failure is reproduced with `BLEEDTHROUGH_PROPTEST_SEED`.
  - A bug fix starts with a test that fails without the fix.
  - Coverage only shows what ran. New or changed domain code must also hold up under mutation: run `cargo xtask mutants --package <crate> --in-diff origin/main` and kill every survivor in your diff with a test, or add a reasoned entry to `testing/mutants-survivors.toml`. Survivors in critical code (`testing/mutants-critical.toml`) are only accepted when proven equivalent. Never add a `baseline = true` entry; those are pre-existing debt being removed by the `mutation-hardening` change.
  - Do not lower a coverage floor to make a change pass; raise floors when coverage grows (`cargo xtask coverage --update-floors`).
- **Review:** the checklist in [docs/architecture-review.md](docs/architecture-review.md) covers what CI cannot prove.
