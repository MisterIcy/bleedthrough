# Bleedthrough

A static application security testing (SAST) tool for PHP 8, written in Rust. It is built for CI use
by software and appsec engineers and for consumption by agents. The priorities are *safe, sound and
trustworthy* over fast to ship.

**Status:** workspace scaffold only. The architecture, dependency rules and CI exist; there is no
analysis, parsing, rule format or output schema yet. Those arrive as separate changes.

## Build

The toolchain is pinned exactly in `rust-toolchain.toml` (Rust 1.99.0, edition 2024). With a standard
[rustup](https://rustup.rs) installation, the first `cargo` command in this directory installs it.

```sh
cargo build --workspace
cargo test --workspace
cargo run -p bleedthrough-cli -- --version
```

## Layout

| Path | Contents |
| --- | --- |
| `crates/` | One crate per concern, in four layers (domain, application, infrastructure, delivery) |
| `xtask/` | Architecture checks, run with `cargo xtask <command>` |
| `architecture.toml` | The layer of each crate and the dependency edges it may have |
| `api/` | Public-API snapshots of domain and application crates |
| `docs/` | Glossary, architecture note, architecture review checklist |
| `packs/`, `rules/`, `adapters/` | Reserved for knowledge packs, built-in rules and parser adapters (empty; the LSP crate is reserved and not created) |

See [docs/architecture.md](docs/architecture.md) for the design and [CONTRIBUTING.md](CONTRIBUTING.md)
before opening a pull request.

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option. Crates
are not published to a registry (`publish = false`).
