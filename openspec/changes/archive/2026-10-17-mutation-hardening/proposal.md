## Why

The `testing-strategy` change introduced mutation testing and recorded its first baseline: of 2730 viable mutants
in the domain crates, 1482 survive (symbols 32% caught, syntax 41%, findings 61%, rules 74%, builtins 86%,
taxonomy 96%). 202 of the survivors are in security-critical code. They are recorded in
`testing/mutants-survivors.toml` as `baseline = true` debt. This change pays it down in a deep testing and mutation
pass, before further systems are built on these crates.

## What Changes

- Kill every critical-code survivor with a test (symbols `types/` and `table/`, findings `coverage.rs` and
  `confidence.rs`, taxonomy `scoring.rs`, `severity.rs`, `tags.rs`), or prove it equivalent in a reasoned entry.
- Kill the remaining viable survivors crate by crate (symbols `infer/` and `summary/`, syntax `describe.rs` and
  `walk.rs`, rules, builtins), replacing each leftover with a ledger entry that gives a specific reason.
- Remove every `baseline = true` entry; the ledger then holds only reasoned survivors, and the loader refuses
  `baseline` entries.
- Raise coverage floors to the new measured values.
- Add the missing example-test markers (`/// spec:`) while touching tests, and review the traceability report.
- Run `mutants.yml` and `determinism.yml` by manual dispatch (left open by `testing-strategy` tasks 4.5 and 5.3).

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `mutation-testing`: the ledger no longer admits unanalysed (`baseline`) entries. The rest of the change is test and
  ledger work that the existing requirements already demand.

## Impact

Tests in `crates/symbols`, `crates/syntax`, `crates/findings`, `crates/rules`, `crates/builtins`,
`crates/taxonomy`; `testing/mutants-survivors.toml`, `testing/coverage-floors.toml`. Possible small product fixes
where a surviving mutant reveals a real defect. No public API change expected.
