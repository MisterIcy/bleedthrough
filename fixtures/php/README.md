# PHP fixture corpus

Parser-independent PHP source files, a manifest describing them, and a ledger of what real PHP
interpreters say about each one. Any parser backend is judged against the same files; nothing here
names a particular parser. Fixtures for syntax checking live here, apart from any later fixtures
for vulnerability analysis.

| Path | Holds |
|---|---|
| `v7.0/` … `v8.5/` | Valid syntax introduced by that PHP version, one feature per file |
| `legacy-removed/` | Syntax valid in 7.x and rejected by 8.x; backends accept it leniently |
| `malformed/` | One kind of breakage per file, with a backend-neutral expectation |
| `realistic/` | Application-shaped files (controller, repository, template, legacy script) |
| `manifest.toml` | Features (with the php.net page), fixtures, groups, provenance, expectations |
| `ledger.toml` | Interpreter verdicts per fixture and PHP version (generated, do not edit) |

Golden dumps of the owned tree live beside the suite in `crates/syntax-conformance/goldens/`.
Files here are binary-exact (`.gitattributes` marks them `-text`): NUL bytes and invalid UTF-8
are part of the corpus.

## Rules

- Original content, or content under an allowed licence; the manifest records `provenance` and
  `licence` for every fixture.
- At most 200 lines, except `realistic/` files, which need a stated `purpose`.
- Valid and legacy fixtures map to a feature; each feature cites the PHP manual page that
  describes it (`doc` must be on php.net). Keep only grammar changes, not functions or classes.
- A malformed fixture declares, without parser messages: `class` (one of `breakage_classes`),
  `codes` (any one must be reported), `lines` (an error node must cover them), and `survivors`
  (declarations that must still be in the tree). Put `function ok_before()` and `ok_after()` around
  the breakage, but only list `ok_after` when any parser could reasonably recover it (an
  unterminated string or comment swallows the rest of the file).

## Add a fixture

1. Write the file, for example `v8.4/my-feature.php`, starting with `<?php`.
2. Add a `[[fixture]]` block to `manifest.toml` (and a `[[feature]]` row, or add the path to an
   existing feature's `fixtures`). Malformed fixtures also need a `[fixture.expect]` block.
3. Record the verdicts: `cargo run -p bleedthrough-syntax-conformance --example ledger -- record --docker`.
   This runs `php -n -d short_open_tag=0 -l` on a temporary copy of every fixture in the official
   `php:<version>-cli` image for each version (or the local `php` for its own version). Versions
   with no interpreter are written as unverified with a reason, never as verified.
4. Generate the golden dump: `BLESS=1 cargo test -p bleedthrough-syntax-mago --test conformance`,
   then read the new file under `crates/syntax-conformance/goldens/` before committing it.
5. Run `cargo test -p bleedthrough-syntax-mago --test conformance`. The completeness check fails
   on an orphan fixture, a feature without a fixture, a fixture without a ledger entry, an
   oversized fixture, missing provenance, or a licence that is not on the allow-list.

## Verify the ledger

`cargo run -p bleedthrough-syntax-conformance --example ledger -- verify --docker` re-runs the
interpreters and fails on any disagreement, naming the fixture, the version and both verdicts.
Without `--docker` only a local `php` is used and every other version is listed as unverified.

## Remove a fixture

Delete the file, its manifest entries and its golden, then `record` again.

## Known backend gaps

A backend that cannot handle a fixture lists it with the reason in its own test directory (for
the Mago adapter, `adapters/syntax-mago/tests/known-gaps.toml`). The test fails if a listed gap
stops failing, so the list cannot go stale. The corpus itself never changes to fit a backend.
