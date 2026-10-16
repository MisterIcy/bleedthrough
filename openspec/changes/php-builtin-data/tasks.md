## 1. Feasibility gate

- [x] 1.1 Download the six pinned tags' stub files into a scratch directory, record tag, commit SHA and per-file SHA-256, and count stub files and declarations with the project's parser instead of regular expressions; verify the counts replace the approximations in the design
- [x] 1.2 Parse every stub of every tag with the Mago adapter and list each error or unsupported node with file and span; verify the list is complete and decide for each entry: known gap with reason, or Mago upgrade needed
- [x] 1.3 If upgrades are needed, follow `docs/mago-upgrade.md` in a separate commit and rerun the parse; verify the remaining gap list is short and each item is explained

## 2. Data crate

- [x] 2.1 Create `crates/builtins` (`bleedthrough-builtins`, domain, no dependencies) and declare it in `architecture.toml`; verify `cargo xtask check-layering` passes and a dependency from it fails
- [x] 2.2 Implement the version mask (`u16` bitset over 8.0 to 8.5, range-list text form) and lookup distinguishing "not in that version" from "unknown"; verify mask arithmetic and non-contiguous masks in tests
- [x] 2.3 Implement the record types (function, class-like kinds, enum cases, constants, properties, methods) with parameter and return details, guards, deprecation, alias, extension, SAPI and provenance fields; verify construction rules (overlapping masks rejected)
- [x] 2.4 Implement the canonical writer and strict reader with line-numbered errors and the field validators that reject prose; verify round-trip identity, malformed-line, duplicate, overlap, unknown-version and prose-in-field cases
- [x] 2.5 Implement the `BuiltinDataSource` port and an in-memory source; verify a consumer test using only the port
- [x] 2.6 Add `Term:` doc lines and glossary entries (built-in data, version mask, oracle); verify `cargo xtask check-glossary` and API snapshot generation pass

## 3. Vendoring and provenance

- [x] 3.1 Implement `cargo xtask vendor-php-src` (explicit, networked) that writes `*.stub.php` files and licence files for a tag and updates `stubs/MANIFEST.toml`; verify it on one tag in a scratch location without touching the repo
- [x] 3.2 Vendor the six pinned tags and commit the manifest; verify each vendored file matches upstream bytes at the recorded commit
- [x] 3.3 Implement the manifest check (hash mismatch, unlisted file) in `check-builtins`; verify an edited and an added file each fail
- [x] 3.4 Exclude `third_party/` and `stubs/` generated content from formatting and lint steps by configuration; verify `cargo fmt --check` and clippy ignore them while layering and purity checks still run
- [x] 3.5 Write `NOTICE` with the PHP License 3.01 and Zend Engine License 2.00 texts and copyright lines taken from the vendored licence files; verify the text equals the vendored files
- [x] 3.6 Include `NOTICE` in the release archive and Docker image definitions and add the presence check; verify a build without it fails the check
- [x] 3.7 Write `docs/licensing-builtins.md` (research findings, what other projects do, unconfirmed status) and add the pre-release owner-decision item to the release checklist; verify the checklist fails while no decision is recorded

## 4. Generator crate

- [x] 4.1 Create `crates/builtins-gen` (tooling, may depend on `syntax`, `builtins`, `serde_json`) and the example `gen_builtins` in `adapters/syntax-mago` with a dev-dependency; verify layering (including the dev-dependency rule) passes
- [x] 4.2 Implement the guard scanner (guard stack by line) and its association with declarations; verify nested guards, `#else`, `#elif` and declarations spanning guard lines on excerpts of real stubs
- [x] 4.3 Implement the declaration extractor over the owned tree (functions, classes, interfaces, traits, enums, methods, properties, class and global constants) with attributes; verify against hand-checked stubs for one file per extension family
- [x] 4.4 Implement annotation interpretation with the known-tag table, unknown-tag failure, allow-list and prose discarding; verify every annotation listed in the spec, an unknown tag and a docblock containing prose
- [x] 4.5 Implement the declaration counter (via the syntax visitor) and the coverage report with exclusion rules (`zend_test`, `dl_test`, `skeleton`); verify that removing an extraction case makes the run fail
- [x] 4.6 Implement per-version extraction and normalisation; verify whitespace and attribute-order differences do not change normalised signatures

## 5. Merge and bundling

- [x] 5.1 Implement the merge into records with masks, one record per distinct signature, SAPI merging and alias linking; verify added-parameter, added-in-8.3, same-signature-two-SAPIs and alias scenarios
- [x] 5.2 Implement `stubs/bundling.toml` loading and the bundled/unbundled states, citing sources; verify the imap/pspell/oci8 unbundling case
- [x] 5.3 Implement guard-alternative merging for the same symbol in several branches; verify one record with both alternatives
- [x] 5.4 Wire `cargo xtask gen-builtins`; generate `stubs/builtin.data` and `stubs/coverage.txt` from stubs only; verify reproducibility (two runs, byte-identical) and measure the file size
- [x] 5.5 Add a lookup test over the generated data for well-known symbols across versions (for example `strlen`, `PDO::query`, `Exception::__construct`, a function added in 8.3 and one deprecated in 8.x); verify results against the PHP manual pages cited in the test

## 6. Reflection oracle

- [ ] 6.1 Write `stubs/reflection/dump.php` (facts only, canonical JSON) and review it line by line; verify by running it with the local PHP 8.4 on a scratch copy and inspecting the output shape
- [ ] 6.2 Implement `cargo xtask refresh-reflection` that prints the images and digests it will run, requires explicit invocation, and writes `stubs/reflection/php-<v>.json` with version, digest and loaded extensions; verify the dry-run output without executing images
- [ ] 6.3 Run the refresh for PHP 8.0 to 8.5 with the owner's go-ahead and commit the fixtures; verify each records version, digest and extensions
- [ ] 6.4 Implement fixture reading and gap filling (8.0/8.1 constants and properties, all constant values) with Reflection provenance marks; verify an 8.0 constant appears marked
- [ ] 6.5 Implement the type-spelling normaliser and the cross-check test with `stubs/oracle-allow.toml` and stale-allowance failure; verify the wrong-by-reference-flag and stale-allowance scenarios with injected differences
- [ ] 6.6 Triage the real differences from the first full cross-check: fix generator bugs, add reasoned allowances; verify the cross-check passes with every allowance explained
- [ ] 6.7 Report symbols not covered by the oracle (extensions not in the images) in `stubs/coverage.txt`; verify the section appears

## 7. Gates and docs

- [ ] 7.1 Implement `cargo xtask check-builtins` (regenerate to temp and diff, manifest, NOTICE, format validity) and add it to `check-all`; verify each failure mode and that CI runs it offline
- [ ] 7.2 Write `docs/builtin-data.md` (format, how to read it, regeneration, adding a PHP version, refreshing fixtures, known gaps); verify the add-a-version procedure with a dry run that leaves earlier lookups unchanged
- [ ] 7.3 Update `docs/architecture.md` and its diagram for the new crates; regenerate API snapshots; verify the diagram matches `architecture.toml`
- [ ] 7.4 Run `cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo deny check`, `cargo xtask check-all` and `cargo test --workspace --locked`; verify all pass on the pinned toolchain
