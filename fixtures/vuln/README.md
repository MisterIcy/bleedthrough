# Vulnerability fixture corpus

Small PHP cases labelled by weakness class (CWE), used to measure what Bleedthrough finds and what it
wrongly reports. Nothing here names a rule, an engine or a pack: a case says *which weakness is where*,
so rules can be renamed, split or replaced without touching the corpus. The syntax corpus lives
apart, in `fixtures/php/`.

| Path | Holds |
|---|---|
| `cwe-<n>/<case-id>/` | The PHP files of one case |
| `manifest.toml` | Every case: label, CWE, shapes, twin, provenance, licence, expectations |
| `expectations.toml` | The accuracy ratchet (per class minimum true positives, maximum false positives, known gaps, the scan switch) |

The manifest's structure is described by `schemas/vuln-manifest.schema.json`; the evaluation result by
`schemas/eval-result.schema.json`. How scoring works and how to add a case: `docs/evaluation.md`.

## Manifest format

A header with the closed vocabulary of flow shapes, optional class notes, then one `[[case]]` per case.

```toml
shapes = ["direct", "local-flow", "array-element", "user-function", "method",
          "cross-file-include", "autoload-class", "framework-request",
          "correct-sanitizer", "wrong-sanitizer"]

# Optional: shapes a class does not admit (the coverage report shows them as n/a).
[[class]]
cwe = "CWE-502"
not_applicable = ["framework-request"]
reason = "why"

[[case]]
id = "cwe89-direct"                  # unique, lower-case
cwe = "CWE-89"                       # must be in the taxonomy registry
label = "vulnerable"                 # or "safe"
description = "the data flow in a sentence"
php = "8.0+"                         # or "7.4-8.3"
shapes = ["direct"]                  # from the vocabulary above
twin = "cwe89-direct-safe"           # the counterpart; it must point back
fix = "what the safe twin changes"   # vulnerable cases only
provenance = "original"              # or where the material comes from
licence = "MIT OR Apache-2.0"        # from the allow-list (same as deny.toml)
files = ["cwe-89/cwe89-direct/index.php"]

[[case.expect]]                      # vulnerable cases: one or more
file = "cwe-89/cwe89-direct/index.php"
lines = [4, 4]                       # 1-based, inclusive, inside the file
min_confidence = "medium"            # low | medium | high
# cwe = "CWE-89"                     # optional; defaults to the case's CWE
```

A **safe** case instead declares what must stay unreported. By default that is every file as a whole;
narrow it with `[[case.protect]]` entries (`file`, `lines`). A safe case with no twin is a *standalone
negative* and says why with `standalone = "reason"` (a construct that merely looks dangerous). A case
the tool is known to get wrong carries `known_gap = { reason = "...", tracking = "..." }`; it is
counted and reported separately, never removed or relabelled.

## Rules

- **Twins:** every vulnerable case has a safe twin that differs only by the fix (a sanitizer, a
  parameterised query, a validation, a constant in place of input), and says what the fix is.
- **Original or allowed:** content is written for this project or comes from material under an
  allowed licence; provenance and licence are recorded for every case.
- **Self-contained:** no Composer packages. A framework shape uses a small stub written into the case
  (`Request`, a PDO use, ...). A file that mentions `vendor/autoload` or `composer` is rejected.
- **Small and focused:** one flow per case, at most 100 lines per file.
- **Valid PHP:** every file parses with the Mago backend and passes `php -l`
  (`cargo test -p bleedthrough-syntax-mago --test vuln_corpus`).
- **A disputed label** is settled by citing a CWE definition or a documented exploit path in review.

## Checks

`cargo xtask check-corpus` (offline, needs no scan) fails on: a PHP file the manifest does not list, a
listed file that is missing, duplicate ids, a missing or unknown CWE, a vulnerable case without an
expected finding or without a twin, a twin that does not point back, a line range outside its file, a
known gap without reason or tracking, a missing provenance, a licence off the allow-list, an oversized
file, and a case that depends on an installed package.

`cargo run -p bleedthrough-eval -- coverage` prints, per class, which flow shapes have no case.

## Add a case

1. Write the vulnerable file and its twin under `cwe-<n>/<id>/` and `cwe-<n>/<id>-safe/`, starting with
   `<?php`. Keep the twin identical except for the fix.
2. Add both `[[case]]` blocks (the twin points back), with the expected finding on the sink's lines.
3. Run `cargo xtask check-corpus`, then
   `cargo test -p bleedthrough-syntax-mago --test vuln_corpus`.
4. Once a scan exists, run `cargo xtask eval` and, if the result moved on purpose,
   `cargo xtask eval --update`; review the printed diff.
