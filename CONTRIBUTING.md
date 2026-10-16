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
- **Review:** the checklist in [docs/architecture-review.md](docs/architecture-review.md) covers what CI cannot prove.
