# Upgrading Mago

Mago releases often and the adapter converts its tree exhaustively, so an upgrade is deliberate.

1. **Change every pin together.** In `adapters/syntax-mago/Cargo.toml` all `mago-*` dependencies use `=x.y.z`; set them to the same new version, and update `MAGO_VERSION` in `adapters/syntax-mago/src/lib.rs` (a test compares it to `Cargo.lock`).
2. **Rebuild.** A new enum variant fails to compile in `src/convert/` (the wildcard lint is denied there). `Statement` and `Expression` are `#[non_exhaustive]`, so a new variant of those two shows up as an `Unsupported` node in the suite instead; look for it there.
3. **Update `deny.toml`.** Check the `[bans]` wrappers still match the direct Mago parents of each `mago-*` crate (`cargo tree -i <crate>`), then run `cargo deny check`; record any new licence or advisory failure in the spike report rather than ignoring it.
4. **Run the suite.** `cargo test -p bleedthrough-syntax-mago --test conformance`, then the whole workspace. Fixtures that now pass are removed from `tests/known-gaps.toml` (the test fails until they are).
5. **Review golden diffs.** Re-bless with `BLESS=1 cargo test -p bleedthrough-syntax-mago --test conformance` and read `git diff crates/syntax-conformance/goldens`; every change must be explained by the Mago release notes.
6. **Re-measure.** `cargo run --release -p bleedthrough-syntax-mago --example spike -- --recovery --project <dir>` (run `--phase parse` and `--phase full` in separate processes for memory), compare with `docs/spikes/syntax-mago.md`, and update it.
7. **Run the checks.** `cargo xtask check-all`, `cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and refresh `api/` snapshots if the public surface moved.
