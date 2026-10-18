# Release checklist

Work through this before tagging a release. Items that a command can check name the command.

- [ ] **Mutation testing is clean.** Dispatch the `mutants` workflow on the release commit (it is not part of
      routine CI). Every job passes: each survivor is killed or in `testing/mutants-survivors.toml` with its
      analysis, and no ledger entry is stale. See [testing.md](testing.md).
- [ ] `cargo xtask check-all` and the rest of the gates in [CONTRIBUTING.md](../CONTRIBUTING.md) pass.
- [ ] **Licensing of the built-in data is decided.** The project owner has recorded a decision in
      [licensing-builtins.md](licensing-builtins.md). `cargo xtask check-release` fails while the
      status there is `unconfirmed`. This blocks the first public release.
- [ ] The release archive and the Docker image contain `NOTICE` (`cargo xtask check-builtins`
      verifies the definitions in `release/archive.toml` and the `Dockerfile`).
- [ ] The built-in data is current: `cargo xtask check-builtins` is clean, and the PHP release
      tags in `stubs/MANIFEST.toml` are the latest patch releases wanted
      (see [builtin-data.md](builtin-data.md)).
