# adapters/

Parser-adapter crates. An adapter is the only kind of crate allowed to depend on a third-party PHP
parser, directly or through another crate; it implements the `ParserBackend` port owned by
`bleedthrough-syntax`, pins the parser exactly (`=x.y.z`), and is declared with `adapter = true` in
`architecture.toml`. Adapter crates are workspace members (`adapters/*`).

| Crate | Wraps | Notes |
|---|---|---|
| `syntax-mago` | `mago-syntax` 1.57.0 (exact pins on `mago-syntax`, `mago-span`, `mago-database`, `mago-allocator`) | Converts Mago's CST into the owned tree; runs each parse on a dedicated thread with a large stack; upgrade steps in `docs/mago-upgrade.md` |

## Adding or testing an adapter

An adapter runs the shared suite as a dev-dependency on `bleedthrough-syntax-conformance`
(see `syntax-mago/tests/conformance.rs`) against `fixtures/php/`, without changing any fixture.
Fixtures an adapter cannot handle are listed with a reason in its own `tests/known-gaps.toml`; the
test fails when a listed gap stops failing.

## Reserved: LSP front-end

A language-server front-end (`bleedthrough-lsp`) is reserved and **not created**. Like the CLI it would
be a thin delivery crate that wires ports; nothing may depend on it.
