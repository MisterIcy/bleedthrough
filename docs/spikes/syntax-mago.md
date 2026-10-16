# Spike: Mago as the first parser backend

Change `syntax-facade-and-mago-adapter`. Backend `mago-syntax` 1.57.0, measured with
`adapters/syntax-mago/examples/spike.rs` in a release build on Linux (PHP 8.4 locally, ledger from
official images 7.0 to 8.5). Criteria are the provisional ones of design D11; **the owner confirms them**.

## Recommendation

**Accept, with three owner decisions** (below). The port and owned tree stand either way; only the
adapter would change if the decisions go against Mago.

## Criteria

| Criterion | Rule | Result | Verdict |
|---|---|---|---|
| Coverage | every valid 8.0 to 8.5 fixture parses clean; gaps listed | 76 valid + 4 realistic fixtures conform except the listed gaps | **Pass with findings** |
| Recovery | zero uncovered non-trivia bytes on the mutation corpus; count regions made explicit | 2100 mutated files (25 seeds x 84 valid/realistic fixtures), 50 736 nodes, **0 invariant violations**; **2 331** regions made explicit (error plus unsupported nodes) | **Pass** |
| Conversion cost | parse + convert <= 2x parse-only | 1.73x on fixtures, **1.67x on 10 731 real files (55.6 MB, a `vendor/` tree)**, but **1.93x / 2.45x / 2.73x / 2.79x on generated single files of 1 / 2 / 4 / 8 MB** | **Pass on real code, fails on very large single files** |
| Memory | record peak, no limit | see below | Recorded |
| Footprint | licence and advisory checks pass; crates listed | **`cargo deny check` fails on two items** | **Fail, needs a decision** |

Both timings include the 64 MiB-stack thread spawn, so the ratio isolates conversion. Medians of
5 rounds (3 for the size sweep). The generated file is the realistic fixtures wrapped in namespaces
and repeated; a 16 MB input exceeds the default 8 MiB `max_bytes` and is rejected, so it is not a
measurement.

### Memory (VmHWM, separate processes per phase)

| Phase | Peak resident |
|---|---|
| Fixtures only | about 22 MB |
| Parse only, 8 MB file + 55 MB project | 464 MB |
| Parse + convert, same inputs | 626 MB |

The peak is set by the 8 MB generated file (about 58x the source for parse-only, 78x with the owned
tree), not by the project run, which is file-at-a-time. Mago's arena is the larger share. At a
typical file size this is negligible; at the 8 MiB limit, parallel scans multiply it by the thread count.

### Throughput

Parse only: 46 to 51 MB/s. Parse + convert: 31 MB/s on the real project, 17 MB/s on the 8 MB file.
The real project parsed 10 727 of 10 731 files clean; the 4 others carry 11 error nodes (invalid or
non-PHP files in `vendor/`). The project run is local and uncommitted; no project source is stored.

## Findings

### Mago coverage gaps (listed in `adapters/syntax-mago/tests/known-gaps.toml`)
1. **PHP 8.5 attributes on non-class `const` were dropped** (`v8.5/attributes-on-constants.php`). Mago's span for a constant excludes its attributes, so the adapter saw them as unconverted source and the attribute became an error node. Fixed in the adapter (spans now include attribute lists and the `Const` statement carries `attributes`); the known-gap entry is gone.
2. **Removed brace string offset `$s{0}` is rejected** (`legacy-removed/brace-string-offset.php`). The lenient-acceptance policy for removed 7.x syntax is therefore not met for this construct, so task 6.4 is not ticked.

### Recovery visibility
Mago repairs some syntax errors by inserting a synthetic terminator or closer, so there is a
diagnostic but **no `Error` node** (`$x = [1,2,3;`, `foo(1,(2+3);`). Such errors are visible only
through diagnostics. Mago also drops unparseable statements inside nested lists, which the adapter
fills per sequence. Mago's spans for a missing closer take the range of the following token, so
nodes overshoot and overlap successors; the adapter bounds every span (`fit`, `bounded`,
`each_bounded`), which removed 244 containment/order violations the prefix sweep found.

### Leniency
`echo ;` and similar parse clean. Mago accepts some code PHP rejects, so a clean parse is not proof of validity; the ledger and `php -l` are the arbiter in the suite.

### Robustness and design consequences
- **Forced wildcard arms:** Mago's `Statement` and `Expression` are `#[non_exhaustive]`, so the compiler demands one wildcard arm in each match (the `Unsupported` conversion, lint allowed on that function only). A variant added by an upgrade is caught by the suite, not the compiler. All other Mago enums are matched exhaustively.
- **One thread per parse:** Mago recurses per nesting level and a stack overflow aborts the process, so each parse runs on a scoped 64 MiB-stack thread (reserved, not committed). Cost is in both timing columns above.
- **Depth cap of 512** (owner decision, raised from 300): beyond it the converter emits `Error` nodes. A valid expression with more than about 500 chained operators is therefore reported as recovered. Raising it, or flattening associative chains in the model, is the decision.
- **`preserve_order` feature unification:** Mago's closure enables `serde_json/preserve_order` workspace-wide. The SARIF emitter (earlier change) relied on sorted keys; `crates/emit/src/sarif.rs` now sorts all objects itself, byte-identical in both configurations.
- **Nested coverage limit:** `check_invariants` proves whole-file coverage at the top level and neighbour-to-neighbour coverage in every nested list. Bytes before the first or after the last item of a nested list rely on the adapter's delimiter-bounded gap filler and structural tests.
- **Ledger:** `malformed/{invalid-attribute, nul-bytes, unterminated-comment}` are accepted by PHP 7.x (attributes parse as comments there) and `extreme-nesting` is valid everywhere; the ledger records this truthfully.

### Footprint (`cargo deny check` fails; nothing ignored)
Direct closure of the adapter: 85 crates, including `mago-database`, `notify`, `inotify`, `walkdir`,
`rayon`, `regex`, `schemars`, `blake3` and `rand`, all pulled by Mago for features this project does not use.
1. **Licence:** `notify` 8.2.0 (via `mago-database`) is **CC0-1.0**, not on the allow-list.
2. **Advisory:** RUSTSEC-2024-0436, `paste` 1.0.15 is **unmaintained** (via `mago-syntax`).

Bans are clean (all eight `mago-*` crates are restricted to the adapter and their Mago parents).
Options: allow CC0-1.0 for `notify` explicitly (public-domain dedication, low risk); ignore the
advisory with a recorded reason (unmaintained, not a vulnerability), or wait for upstream.

## Owner decisions (recorded 2026-10-15)
1. **Criteria confirmed** as written, including the 2x limit (met on real code; the generated 2 to 8 MB single files exceed it and are accepted as outside realistic source sizes) and the 8 MiB default input limit.
2. **Licence:** CC0-1.0 is allowed in `deny.toml`, for `notify` (and the other CC0 crates already in the tree).
3. **Advisory:** RUSTSEC-2024-0436 (`paste`, unmaintained) is ignored in `deny.toml` with the recorded reason; revisit when Mago drops it. `cargo deny check` now passes.
4. **Depth cap raised from 300 to 512.** The converter margin is thinner at 512: the 2 MiB-stack clone/compare/drop test of a capped tree still passes in debug and release.
5. **Mago gaps accepted as known gaps** (`tests/known-gaps.toml`): the rejected `$s{0}` (the attribute-on-`const` gap was later fixed in the adapter). Task 6.4 is closed on that basis.

**Decision: accept Mago as the first backend.**
