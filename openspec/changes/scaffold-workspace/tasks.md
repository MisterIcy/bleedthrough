## 1. Repository, toolchain and policy files

- [ ] 1.0 Initialize `bleedthrough/` as its own git repository on `main` (it is currently untracked inside the parent repo) with a `.gitignore` for Rust; verify `git rev-parse --show-toplevel` returns the bleedthrough directory. Creating the GitHub remote is outward-facing: confirm with the user before running it
- [ ] 1.1 Add `rust-toolchain.toml` pinning `channel = "1.99.0"` with `rustfmt` and `clippy`; verify `cargo --version` inside the repo reports 1.99.0
- [ ] 1.2 Add `LICENSE-MIT` and `LICENSE-APACHE`; verify both exist and name the correct copyright holder
- [ ] 1.3 Add `rustfmt.toml` and a root `clippy.toml` with complexity, length and argument limits; verify `cargo fmt --check` and `cargo clippy` run clean on the empty workspace

## 2. Workspace and skeleton crates

- [ ] 2.1 Create the root `Cargo.toml` with `[workspace.package]` (edition 2024, `rust-version = "1.99"`, license `MIT OR Apache-2.0`, `publish = false`) and `[workspace.lints]` (`unsafe_code = "forbid"`, clippy levels); verify `cargo metadata` resolves
- [ ] 2.2 Create domain skeleton crates under `crates/`: syntax, symbols, ir, engine, rules, packs, findings; verify `cargo build --workspace` succeeds
- [ ] 2.3 Create the application crate `app` and infrastructure crates `emit`, `fs`, `cache`; verify the workspace builds
- [ ] 2.4 Create the delivery crate `cli` producing a `bleedthrough` binary that prints its version and exits 0; verify with `cargo run -p bleedthrough-cli -- --version`
- [ ] 2.5 Give each crate a crate-level doc comment stating its layer, responsibility and permitted workspace dependencies, plus one smoke test; verify `cargo test --workspace` passes and `cargo doc --workspace` shows the descriptions
- [ ] 2.6 Create reserved directories `packs/`, `rules/` and a documented adapters location, each with a README; verify the LSP crate is documented as reserved and absent

## 3. Layering enforcement

- [ ] 3.1 Write `architecture.toml` (layer and permitted workspace dependencies per crate) matching design D1; verify it parses
- [ ] 3.2 Add the `xtask` crate with `check-layering` reading `cargo metadata`; verify it passes on the current workspace
- [ ] 3.3 Add negative tests for the check (domain depending outward, application depending on a concrete infrastructure crate, undeclared infra-to-infra edge, unlisted crate, cycle); verify each fails with a message naming both crates
- [ ] 3.4 Add a check that every crate inherits workspace `rust-version`, edition, license and lints; verify an overriding crate fails

## 4. Domain purity and extensibility

- [ ] 4.1 Add per-domain-crate `clippy.toml` with `disallowed-types`/`disallowed-methods` (fs, net, process, env, clock, sleep); verify a temporary `std::fs` call in a domain crate fails clippy
- [ ] 4.2 Enable `clippy::exhaustive_enums` and `clippy::exhaustive_structs` for domain and application crates; verify an unmarked public enum fails and a `#[non_exhaustive]` one passes
- [ ] 4.3 Add the infrastructure deny-list check for domain manifests to `xtask`; verify adding a banned crate to a domain manifest fails

## 5. Public-API snapshots

- [ ] 5.1 Implement `cargo xtask check-api` per design D6: a stable-only, `syn`-based extractor of each crate's `pub` items and re-exports into `api/<crate>.txt`; verify it runs locally on the pinned toolchain with no nightly
- [ ] 5.2 Document the extractor's known limits (macro-generated items, glob re-exports) in the check's help output and `docs/architecture.md`; verify both mention them
- [ ] 5.3 Generate and commit initial `api/` snapshots for domain and application crates; verify the check passes
- [ ] 5.4 Verify that making a private item public without updating the snapshot fails the check and that updating both passes

## 6. Glossary

- [ ] 6.1 Write `docs/glossary.md` defining at least Source, Sink, Sanitizer, Propagator, Taint, Entry point, Rule, Pack, Finding, Fingerprint, Severity, Confidence, Coverage, Summary, Baseline, Parse quality, Fix, Suppression; verify each appears exactly once
- [ ] 6.2 Implement `xtask check-glossary` (design D7); verify it fails for a public domain type with no `Term:` line, for an unknown term, and passes for `Term: exempt`
- [ ] 6.3 Verify removing a term still in use fails the check

## 7. Dependency policy

- [ ] 7.1 Add `deny.toml` with the license allow-list, advisory checks, source restrictions, and the parser-backend ban with the adapter wrapper; verify `cargo deny check` passes
- [ ] 7.2 Add the exact-pin check for parser-backend crates; verify a range requirement on a listed backend crate fails
- [ ] 7.3 Verify a temporary disallowed-license dependency (not committed) makes `cargo deny check licenses` fail

## 8. Continuous integration

- [ ] 8.1 Add `.github/workflows/ci.yml` with required jobs: build+test, fmt, clippy (`-D warnings`), deny, layering, glossary, and public-API; every job takes the toolchain from the pin file, with no nightly
- [ ] 8.2 Verify locally that each job's command succeeds, and that an introduced clippy warning and an unformatted file each fail their job
- [ ] 8.3 Confirm in the first pushed pull request that the workflow runs and reports a single overall status naming any failing gate

## 9. Documentation and review process

- [ ] 9.1 Write `README.md` (what Bleedthrough is, status, build instructions including the toolchain pin) and `CONTRIBUTING.md` (gates to run before a PR); verify the commands run as written
- [ ] 9.2 Write `docs/architecture.md` with the layer diagram (Mermaid) and a table of each SOLID/DDD principle with its verification method (CI check or review); verify it matches `architecture.toml` and design D4
- [ ] 9.3 Write `docs/architecture-review.md` (checklist for SRP, ISP, LSP, OCP and DDD boundaries) and a pull-request template that references it; verify the template renders on a pull request
