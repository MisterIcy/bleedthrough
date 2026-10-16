# Release checklist

Work through this before tagging a release. Items that a command can check name the command.

- [ ] `cargo xtask check-all` and the rest of the gates in [CONTRIBUTING.md](../CONTRIBUTING.md) pass.
- [ ] **Licensing of the built-in data is decided.** The project owner has recorded a decision in
      [licensing-builtins.md](licensing-builtins.md). `cargo xtask check-release` fails while the
      status there is `unconfirmed`. This blocks the first public release.
- [ ] The release archive and the Docker image contain `NOTICE` (`cargo xtask check-builtins`
      verifies the definitions in `release/archive.toml` and the `Dockerfile`).
- [ ] The built-in data is current: `cargo xtask check-builtins` is clean, and the PHP release
      tags in `stubs/MANIFEST.toml` are the latest patch releases wanted
      (see [builtin-data.md](builtin-data.md)).
