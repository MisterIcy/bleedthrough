## 1. Critical code

- [x] 1.0 Re-run `cargo xtask mutants` per crate and record the real survivor count against the ledger (1370 entries whose `count`s sum to the 1482 survivors; the 1371 in the design counted one extra)

- [x] 1.1 Kill the survivors in taxonomy and findings critical files; re-run those crates and confirm they no longer appear
- [x] 1.2 Kill the survivors in `symbols` `types/`, `table/` and `name.rs`; re-run and confirm
- [x] 1.3 Any critical survivor that is truly equivalent gets an `equivalent = true` entry with the proof

## 2. Remaining survivors

- [x] 2.1 builtins, rules, findings: kill or give a specific reasoned entry
- [x] 2.2 syntax: kill or give a specific reasoned entry
- [x] 2.3 symbols `infer/`, `summary/`, `builtin.rs`: kill or give a specific reasoned entry

## 3. Close out

- [x] 3.1 No `baseline = true` entry remains; make the ledger loader refuse them
- [x] 3.2 Raise the coverage floors; read the spec-trace report and add missing markers
- [x] 3.3 Run `mutants.yml` and `determinism.yml` by manual dispatch and record the results
