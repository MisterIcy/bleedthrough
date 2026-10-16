# Accuracy evaluation

How Bleedthrough measures what it finds: a corpus of small labelled PHP cases, a scorer that reads the
`json` report of a scan, and committed expectations that can only move up on purpose. Speed is measured
separately, see [benchmarks.md](benchmarks.md).

```mermaid
flowchart LR
  FX[(fixtures/vuln<br/>manifest.toml)] --> EV
  XT[cargo xtask eval] -- scan fixtures/vuln --format json --> B[[bleedthrough]]
  B -- json report --> EV[bleedthrough-eval<br/>validate, match, score]
  EX[(expectations.toml)] --> EV
  EV --> R[verdict: pass / fail / not evaluated<br/>table + JSON]
```

The evaluator is black-box: it reads only the published report (validated against
`schemas/report.schema.json`), so an engine rewrite needs no change here and the agent-facing contract
is exercised on every run. A report that fails validation, or a scan that exits abnormally, fails the
evaluation and is shown; no metrics are reported for it.

## Commands

| Command | Does |
|---|---|
| `cargo xtask check-corpus` | Offline completeness and label checks; needs no scan |
| `cargo xtask eval` | Builds `bleedthrough`, scans `fixtures/vuln`, scores, compares with the expectations |
| `cargo xtask eval --json FILE` | Also writes the machine-readable result (`schemas/eval-result.schema.json`) |
| `cargo xtask eval --update` | Rewrites `expectations.toml` from this result and prints the diff |
| `cargo xtask eval --ci` | As `eval`, but `not evaluated` exits 0 while the scan switch is false |
| `cargo run -p bleedthrough-eval -- coverage` | The shape-coverage report below |
| `cargo run -p bleedthrough-eval -- eval --report FILE` | Score a stand-in `json` report instead of running the binary |

Exit status of `eval`: **0** pass, **1** fail, **3** not evaluated.

## How matching works

A finding **matches** an expected finding when all of these hold:

- it is in the same file (paths are relative to the scanned root, `fixtures/vuln`);
- **any** CWE in the finding's `taxonomy.cwe` equals the expectation's CWE;
- its line range overlaps the expected line range;
- its confidence tier is at least the expectation's `min_confidence`.

Expectations are assigned, in sorted order (case, file, lines, CWE), to the first unused finding in
sorted order (path, lines, rule, fingerprint, higher tier first). A finding satisfies at most one
expectation and an expectation is satisfied once, so the result never depends on the order of the
report. What is left over is classified:

| Finding | Counts as |
|---|---|
| Matches an expectation | **true positive** |
| Has the case's CWE inside a safe case's protected range | **false positive**, naming the case |
| Anything else (extra finding in a vulnerable case, other CWE, unlisted file) | **unlabelled**: listed, never scored |
| An expectation no finding satisfies | **false negative**, naming the case |

Unlabelled findings are information, not errors: the corpus labels what is known, and a correct extra
finding must not be punished, while labelled safe twins still catch over-reporting.

Metrics are reported per CWE, per rule and in total. **Precision** is tp/(tp+fp), **recall** is
tp/(tp+fn). A metric whose denominator is zero is `undefined` (JSON `null`), never 0 or 1. Per-rule
numbers attribute only what a finding says (its `rule`); a missed vulnerability belongs to no rule, so a
rule's false negatives and recall are always undefined. Output is byte-identical for identical input.

## Expectations and the ratchet

`fixtures/vuln/expectations.toml` records, per CWE, `min_tp`, `max_fp` and `known_gaps`. The
evaluation **fails** when:

- true positives drop below `min_tp` or false positives exceed `max_fp`;
- a case outside the known gaps fails (the message names the class and the case);
- a **known gap now passes** (the improvement must be recorded: remove the gap from the manifest, then
  `--update`), so stale exceptions cannot accumulate;
- the report has results for a class the expectations do not list;
- the known gaps in the manifest and in the expectations differ.

`--update` changes only `expectations.toml` and prints each changed count and each added or removed
gap, so the diff is reviewable. A known gap is declared in the manifest with a reason and a tracking
reference (`known_gap = { reason, tracking }`); the expectations record which cases are gaps.
The committed counts are all zero today because no scan exists; the first change that produces a report
runs `--update` and commits the result.

## The scan switch

While `bleedthrough` has no scan command (it prints no report, exits with usage status 2, or says
"not implemented"), the evaluation prints `not evaluated` with the reason, reports no metric and exits 3.
In CI (`--ci`) that counts as a pass only while `[eval] scan-switch = false` in
`expectations.toml`. **The change that adds the scan command MUST set `scan-switch = true`** (and then
run `cargo xtask eval --update`); from then on `not evaluated` fails CI, so absence can never silently
become permanent.

## Add a case and its twin

See [fixtures/vuln/README.md](../fixtures/vuln/README.md): write the vulnerable file and its safe twin
(identical except for the fix), add both `[[case]]` blocks with the expected finding on the sink's
lines, then run `cargo xtask check-corpus` and
`cargo test -p bleedthrough-syntax-mago --test vuln_corpus`. Labels are settled in review by citing a
CWE definition or a documented exploit path.

## Coverage of flow shapes

Generated by `cargo run -p bleedthrough-eval -- coverage`: a class and a shape with no case is
`missing`. `correct-sanitizer` is covered by the safe twin of the `wrong-sanitizer` case.

| CWE | Cases | direct | local-flow | array-element | user-function | method | cross-file-include | autoload-class | framework-request | correct-sanitizer | wrong-sanitizer |
|---|---|---|---|---|---|---|---|---|---|---|---|
| CWE-22 | 18 | yes | yes | yes | yes | yes | yes | yes | yes | yes | yes |
| CWE-78 | 18 | yes | yes | yes | yes | yes | yes | yes | yes | yes | yes |
| CWE-89 | 18 | yes | yes | yes | yes | yes | yes | yes | yes | yes | yes |
| CWE-502 | 18 | yes | yes | yes | yes | yes | yes | yes | yes | yes | yes |
| CWE-601 | 18 | yes | yes | yes | yes | yes | yes | yes | yes | yes | yes |
| CWE-918 | 18 | yes | yes | yes | yes | yes | yes | yes | yes | yes | yes |

Cross-site scripting (CWE-79) has no cases: the taxonomy registry deliberately leaves it out of v1 and the
corpus check rejects a CWE the registry does not know. Cases for it arrive with the taxonomy decision.

The first cases are generated from templates (one sink per class, nine flow shapes, each with a twin)
and then maintained by hand. They are small on purpose; precision on real code is a different question
(see the real-world corpus in [benchmarks.md](benchmarks.md)).
