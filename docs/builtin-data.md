# Built-in data

`stubs/builtin.data` says which PHP functions, classes, interfaces, traits, enums, constants,
properties, methods and enum cases exist in PHP 8.0 to 8.5, with their signatures. It is
generated, checked in, and consumed through the `BuiltinDataSource` port of
`bleedthrough-builtins`. Licensing is in [licensing-builtins.md](licensing-builtins.md).

## Where it comes from

| Input | Role |
| --- | --- |
| `third_party/php-src/php-8.N.x/**/*.stub.php`, hashed in `stubs/MANIFEST.toml` | Primary source: names, types, flags, defaults, deprecations. |
| `stubs/reflection/php-8.N.json` | Reflection of the pinned official image. Gives constant values, fills what the stubs do not declare, and is the oracle for the cross-check. |
| `stubs/conflicts.toml` | Reviewed choice where one version declares a symbol twice with different details. |
| `stubs/bundling.toml` | Reviewed list of extensions that are not bundled in some versions, each with its source. |
| `stubs/oracle-allow.toml` | Reviewed, reasoned differences from Reflection. |

Outputs: `stubs/builtin.data` and `stubs/coverage.txt` (what was extracted per file, what was
filled from Reflection, which symbols the oracle cannot vouch for). Neither contains
documentation prose. The product reads only `builtin.data`; no runtime path reads fixtures or
runs PHP.

## Format

Line oriented, tab separated, canonical (reading then writing gives the same bytes):

```text
bleedthrough-builtins 1
extension  <name>  <unbundled mask>
<kind>  <name>  <mask>  <reflected mask>  <extension>  <sapis>  <guards>  <deprecation>  <details...>
```

- A **mask** is the set of versions, such as `8.0,8.2-8.5`. A symbol whose signature changes
  between versions has one record per form, with non-overlapping masks.
- The **reflected mask** is the part of the mask for which the record, or its constant value,
  came from Reflection instead of the stubs.
- **Guards** are the build conditions from the stubs (`defined(HAVE_X)`), as alternatives
  separated by `;`. **SAPIs** is empty when every SAPI has the symbol.
- A parameter is `name@flags@type@doctype@default`. Escapes are `\\`, `\t`, `\n`, `\r`, `\xHH`.
- The exact grammar and the reader's rejections are in `crates/builtins/src/format.rs`.

Lookups say `Found`, `NotInVersion`, `Unbundled` (the extension exists but is not bundled in that
version) or `Unknown`.

## Reading it

```text
grep -P '^function\tstrlen\t' stubs/builtin.data
cargo run -q -p bleedthrough-builtins-gen --example oracle_report   # differences from Reflection
```

## Regenerating

```text
cargo xtask gen-builtins     # rewrites stubs/builtin.data and stubs/coverage.txt
cargo xtask check-builtins   # offline gate: manifest, NOTICE, packaging, byte-identical regeneration, format validity
```

Generation is deterministic and offline. Review the diff of `builtin.data` and `coverage.txt`.
Unknown docblock tags or attributes, a declaration the extractor did not turn into a record, an
unlisted same-version conflict, a stale conflict entry, or a bundling entry that disagrees with
the stubs each fail the run.

## Refreshing the fixtures

```text
cargo xtask refresh-reflection             # dry run: prints images, digests and commands
cargo xtask refresh-reflection --run       # needs Docker; manual, never run by CI
cargo xtask refresh-reflection --run --version 8.4
```

Images are pinned by digest in `fixtures/php/images.toml` (shared with the syntax ledger); the
script is `stubs/reflection/dump.php`. Review the script before changing a digest. Then run
`cargo xtask gen-builtins` and `cargo test -p bleedthrough-builtins-gen --test oracle`.

## Adding a PHP version (8.6, say)

1. `cargo xtask vendor-php-src` for the new tag (see `--help`); this updates `stubs/MANIFEST.toml`,
   and `cargo xtask gen-notice` refreshes `NOTICE`.
2. Raise `LAST_MINOR` in `crates/builtins/src/version.rs` (the mask is a `u16`, so up to 8.15 fits).
3. Add the image digest for the version to `fixtures/php/images.toml`, add the version to
   `VERSIONS` in `xtask/src/reflection.rs`, and run `refresh-reflection --run --version 8.6`.
4. Update `stubs/bundling.toml` if an extension left the distribution, citing the source.
5. `cargo xtask gen-builtins`; review the diff and `coverage.txt`; fix new conflicts in
   `conflicts.toml` and unexplained differences (`--test oracle`) with reasons.
6. Adding a version must change existing records only by extending masks. This was checked by
   generating without 8.5 and comparing every record of 8.0 to 8.4 with the full data: identical.

## Known gaps

- Symbols of extensions that the images do not load are vouched for by the stubs alone
  (`coverage.txt`, "Not covered by the Reflection oracle").
- Constants whose value depends on the build or image (`PHP_OS`, `PHP_BINARY`, library versions)
  have no value; the list is `BUILD_SPECIFIC` in `crates/builtins-gen/src/fixture.rs`.
- Methods are never filled from Reflection, and `Closure::__invoke` is absent (the one allowance).
- Enums have implicit `cases`, `from`, `tryFrom`, `$name` and `$value` that the data does not
  declare; consumers add them.
- Default values the stubs leave as `UNKNOWN` have no text.
- The release packaging files (`Dockerfile`, `release/archive.toml`) are placeholders and unbuilt.
