## 1. Taxonomy crate

- [x] 1.1 Create `crates/taxonomy` (`bleedthrough-taxonomy`, domain, no dependencies) and declare it in `architecture.toml` with `findings` and `rules` allowed to depend on it; verify `cargo xtask check-layering` passes and a dependency from taxonomy to any other crate fails
- [x] 1.2 Move `Cwe`, `OwaspRef`, `Taxonomy`, `SeverityLevel`, `Severity` and their tests from `bleedthrough-findings` to taxonomy and update findings to use them without re-exporting; verify `cargo test --workspace --locked` passes
- [x] 1.3 Add `Term:` doc lines and glossary entries for the taxonomy concepts (weakness, category, registry, severity metrics); verify `cargo xtask check-glossary` passes
- [x] 1.4 Regenerate API snapshots (`bleedthrough-taxonomy.txt`, findings, rules) and review the diff; verify `cargo xtask check-api` passes

## 2. Registry

- [x] 2.1 Collect names and OWASP 2021 mappings for the v1-scope weaknesses from the MITRE CWE catalog and the OWASP Top 10 2021 pages, recording sources and catalog version in `docs/taxonomy.md`; verify every entry cites its source and CWE-79 is absent
- [x] 2.2 Implement the static registry (weaknesses, OWASP 2021 A01 to A10 with names) and lookup; verify lookups for known and unknown numbers and editions
- [x] 2.3 Make `Cwe` and `OwaspRef` constructors validate against the registry and add stable text forms (`CWE-89`, `OWASP:2021:A03`) with parsing; verify round-trip, unregistered-number and unregistered-edition tests
- [x] 2.4 Implement the weakness-to-category consistency check with the explicit-reason exception; verify consistent, unrelated and reasoned cases
- [x] 2.5 Write the procedure for adding a weakness in `docs/taxonomy.md`; verify by following it for a throwaway entry

## 3. Severity model

- [x] 3.1 Implement the metric enums with the CVSS 3.1 values and the vector rendering; verify the rendered vector for the reference metrics
- [x] 3.2 Implement the base-score equations with integer arithmetic and the specification's round-up; verify against published reference vectors (including scope-changed and zero-impact cases) listed in the test with their sources
- [x] 3.3 Implement the level mapping and the `severity_scheme` version constant; verify boundaries 0.0, 0.1, 3.9, 4.0, 6.9, 7.0, 8.9, 9.0 and 10.0
- [x] 3.4 Make findings take score, vector and level from a rule's metrics through one constructor; verify the independence-from-confidence scenario and that a level-only finding is still valid

## 4. Rule model

- [x] 4.1 Add `RuleId` (`BT` plus four digits) and `Rule` metadata with validating builders and private fields; verify missing remediation, bad identifier and untagged rule fail with named fields
- [x] 4.2 Add `SymbolRef` and the closed `SinkClass` vocabulary; verify parsing of function, namespaced function, static method, instance method with subclass flag, constructor and superglobal forms
- [x] 4.3 Add the taint body (sources, sinks with vulnerable arguments by position or name, sanitizers by sink class, propagators); verify the wrong-context sanitizer and argument-position scenarios in unit tests
- [x] 4.4 Add the pattern body with the closed shapes and argument conditions, and the restricted regular-expression subset; verify banned-function, non-literal include, unsupported shape and rejected regex cases
- [x] 4.5 Add the `RuleSet` (sorted by identifier, duplicate detection) and its canonical form for hashing; verify duplicate identifiers name both locations and the canonical form ignores insertion order
- [x] 4.6 Define the `RuleSource` port and the located-error type; verify a stub source drives validation end to end in tests
- [x] 4.7 Check `rules/RETIRED.md` handling: retired identifiers cannot be reissued; verify a rule using a retired identifier fails

## 5. TOML loader

- [x] 5.1 Add `toml` and `serde` to `bleedthrough-fs` only, with licences allowed by `deny.toml` and exact versions in the lock; verify `cargo deny check` and `cargo xtask check-layering` pass
- [x] 5.2 Implement TOML definition structs with unknown-field rejection and schema-version handling; verify misspelt key, wrong type and newer major version failures
- [x] 5.3 Implement error collection with file, line and key for every error in a file; verify a file with three errors reports three
- [x] 5.4 Implement directory loading with deterministic ordering and the content hash over the canonical form; verify reordered files give the same set and hash and a comment edit leaves the hash unchanged
- [x] 5.5 Implement the `rules check` library entry point (load, validate, report, exit status) without CLI wiring; verify an invalid rule directory yields the tool-error status

## 6. Documentation and gates

- [x] 6.1 Write `docs/rule-schema.md` with a worked taint rule and pattern rule, each parsed by a test so the examples cannot rot; verify the tests fail when an example is broken
- [x] 6.2 Update `docs/architecture.md` and its diagram for the taxonomy crate and the rule loader; verify the diagram matches `architecture.toml`
- [x] 6.3 Run `cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo deny check`, `cargo xtask check-all` and `cargo test --workspace --locked`; verify all pass on the pinned toolchain
