# adapters/

Reserved location for parser-adapter crates, for example `bleedthrough-syntax-mago`, added by the
`syntax-facade-and-mago-adapter` change. An adapter is the only kind of crate allowed to depend on a
third-party PHP parser; it implements the `ParserBackend` port owned by `bleedthrough-syntax`,
pins the parser exactly, and is declared with `adapter = true` in `architecture.toml`. Adapter crates
are workspace members and will be added to `members` in the root `Cargo.toml`.

## Reserved: LSP front-end

A language-server front-end (`bleedthrough-lsp`) is reserved and **not created**. Like the CLI it would
be a thin delivery crate that wires ports; nothing may depend on it.
