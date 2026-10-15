## Why

Bleedthrough is a greenfield, Rust-based SAST tool for PHP 8, aimed at CI use by software and appsec engineers and consumed by agents. It will be developed with Domain-Driven Design and SOLID principles: a pure domain (source model, semantics, flow, analysis, knowledge, findings) with parsers, emitters, file access and caches as replaceable infrastructure behind ports. Those boundaries are cheap to set at the start and expensive to retrofit, and a principle that is only written down erodes, so the workspace, its dependency rules and the CI that checks them come first, before any analysis code exists.

## What Changes

- Add a Cargo workspace organised in four layers, one crate per concern, each an empty documented skeleton:
  - **Domain** (pure): `bleedthrough-syntax` (owned AST, spans, parser-backend port), `bleedthrough-symbols`, `bleedthrough-ir`, `bleedthrough-engine`, `bleedthrough-rules`, `bleedthrough-packs`, `bleedthrough-findings`.
  - **Application**: `bleedthrough-app` (scan use cases and the driven ports they need).
  - **Infrastructure**: `bleedthrough-emit`, `bleedthrough-fs`, `bleedthrough-cache`; parser adapters (e.g. `bleedthrough-syntax-mago`) are added by a later change.
  - **Delivery**: `bleedthrough-cli` (the `bleedthrough` binary and composition root). An LSP crate is reserved, not created.
- Add a tooling crate `xtask` that runs the architecture checks.
- Add architecture CI: layer dependency rule, domain purity, parser isolation, stable-only public-API snapshots for domain and application crates (source-derived, with documented limits), complexity and size lints, and a glossary check. State explicitly which SOLID properties are machine-checked and which are covered by a review checklist.
- Add a ubiquitous-language glossary as a first-class artifact.
- Pin the toolchain exactly to Rust `1.99.0` with `rust-toolchain.toml`, with one workspace MSRV of 1.99 (edition 2024), required by the planned Mago adapter.
- License the project `MIT OR Apache-2.0`, via workspace-inherited metadata and a dependency-license allow-list.
- Add quality gates: `rustfmt`, `clippy` (warnings denied), `cargo-deny`, `unsafe_code` forbidden by default, and a CI workflow that runs all of them.
- Give the project its own git repository with a GitHub remote and GitHub Actions CI, and mark every crate `publish = false` (distribution is by binaries and a Docker image, defined by a later change). All CI jobs use the pinned stable toolchain; there is no nightly.
- Add contributor basics: README, CONTRIBUTING, an architecture note, and an architecture review checklist.

No analysis behaviour, parser integration, rule format, or output schema is introduced here; those are separate changes.

## Capabilities

### New Capabilities
- `workspace-layout`: the crate set and layers, each crate's responsibility, permitted dependency directions, port ownership, parser-adapter isolation, and reserved locations.
- `architecture-boundaries`: domain purity, layer encapsulation, public-API stability, extensibility and size limits, and the explicit split between CI-verified and review-verified design principles.
- `domain-glossary`: the ubiquitous-language glossary and the requirement that public domain types name the term they represent.
- `build-and-quality-gates`: toolchain pin and MSRV policy, licensing, dependency policy, lint/format/`unsafe` policy, and the CI checks that enforce them.

### Modified Capabilities
<!-- None: there are no existing specs. -->

## Impact

- New files at the repository root: `Cargo.toml` (workspace), `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `deny.toml`, `architecture.toml` (layer declaration), `LICENSE-MIT`, `LICENSE-APACHE`, `README.md`, `CONTRIBUTING.md`, `docs/` (glossary, architecture note, review checklist), `.github/workflows/ci.yml`, `crates/*/` skeletons, `xtask/`.
- Developer environment: Rust 1.99.0 (already installed locally); the pin makes this automatic for others.
- Downstream changes build on this one: `finding-model-and-emitters`, `syntax-facade-and-mago-adapter`, `rule-schema-and-taxonomy`, `symbols-and-types`, `ir-and-taint-engine`, `pack-system`, `ci-ergonomics`.
- No runtime dependencies are added yet; in particular the Mago crates are not a dependency of this change.
