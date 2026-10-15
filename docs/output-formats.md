# Output formats

Bleedthrough writes a [`Report`](glossary.md#report) in one of six formats. Every format is
UTF-8, free of colour and terminal control sequences, and byte-identical for identical reports.
There are no timestamps, durations or absolute paths. Findings are ordered by path, start
position, rule and fingerprint.

| Format | Use | Coverage and suppressed counts |
| --- | --- | --- |
| `json` | One document for tools and agents | embedded |
| `ndjson` (alias `jsonl`) | Streaming, one record per line | final `summary` record |
| `sarif` | Code-scanning platforms (SARIF 2.1.0) | invocation notifications and properties |
| `tsv` | Tables for shell tools | one-line summary returned to the caller |
| `github-actions` | Workflow-command annotations | a notice annotation |
| `diff` | `git apply`-able patch of suggested fixes | one-line summary returned to the caller |

An unknown name is an error that lists the six valid names.

## Report schema

The `json` document and the `ndjson` records are described by
[`schemas/report.schema.json`](../schemas/report.schema.json) (JSON Schema 2020-12). The schema
version is currently **1.0** and is carried in the document and in the `run` record, so a consumer
can read it before anything else. Additive changes keep the major version; a breaking change
bumps it.

### Confidence reasons

A finding below high confidence carries at least one reason from this closed vocabulary. Adding a
code is an additive change; the schema enumerates the codes and unknown codes are rejected.

| Code | Meaning |
| --- | --- |
| `source_inferred` | The source was inferred rather than declared |
| `dynamic_dispatch_several_candidates` | Dynamic dispatch with several candidate targets (detail: the count) |
| `unresolved_callee_assumed_propagate` | An unresolved callee was assumed to propagate taint |
| `parse_recovered` | The file was parsed with error recovery |
| `type_unknown` | A type needed by the analysis was unknown |
| `analysis_widened` | The analysis hit a limit and widened its result |
| `sanitizer_wrong_for_sink_context` | A sanitizer was applied that is wrong for the sink context |

### Fingerprints

`bt1:` followed by 32 lowercase hex digits: the first 128 bits of a SHA-256 over the rule, the
enclosing symbol, the normalized key snippets and an occurrence index. Line numbers and paths are
not part of it. A change of scheme changes the prefix.

### Severity score

When present, the score is a number from 0.0 to 10.0 with one decimal place.

## ndjson

A `run` record, one `finding` record per finding, and a final `summary` record. A stream without
the `summary` record was interrupted. Example (`mixed` report, first and last records):

```json
{"type":"run","schema_version":"1.0","tool":{"name":"bleedthrough","version":"0.0.0"},"options":{"confidence_threshold":"medium","paranoid":false,"php_version":"8.3","ruleset_id":"core-1"}}
{"type":"summary","coverage":{"files_clean":12,"files_recovered":0,"files_failed":0,"skipped":[],"limits_hit":[],"unresolved_includes":0,"unresolved_calls":0,"packs":[{"name":"core","version":"1.0.0"},{"name":"symfony","version":"1.0.0"}],"php_version":"8.3","cache":null},"suppressed":{"below_threshold_low":12,"below_threshold_medium":3,"baselined":7}}
```

## tsv

Columns: `path`, `line`, `column`, `end_line`, `end_column`, `rule`, `severity`, `confidence`,
`cwe`, `owasp`, `fingerprint`, `message`. Several CWE or OWASP values are comma-separated in
their cell. Tab, newline, carriage return and backslash are escaped as `\t`, `\n`, `\r`, `\\`.
The header can be omitted. Example (`incomplete` report):

```tsv
path	line	column	end_line	end_column	rule	severity	confidence	cwe	owasp	fingerprint	message
lib/Shell.php	2	6	2	30	php.cmdi	high	high	CWE-89	A03:2021	bt1:fc3a94b29d42e89efcd39b735c85941f	Tainted value reaches a sink in Shell::run
```

## github-actions

One `error`, `warning` or `notice` command per finding (critical and high are errors, medium is a
warning, low and info are notices), escaped as workflow commands require. When more findings
exist than the cap, the highest severity and then highest confidence come first and a notice
states how many were omitted. The default cap is 10, taken from GitHub's documented limit of ten
annotations of each kind per step (and fifty per job). Example (`mixed` report):

```text
::notice file=lib/Shell.php,line=2,col=1,endLine=2,endColumn=5,title=php.info::Tainted value reaches a sink in Shell::note
::error file=lib/Shell.php,line=2,col=6,endLine=2,endColumn=30,title=php.cmdi::Tainted value reaches a sink in Shell::run
::warning file=src/Db.php,line=2,col=1,endLine=2,endColumn=7,title=php.xss::Tainted value reaches a sink in Db::show
::notice file=src/Db.php,line=3,col=7,endLine=3,endColumn=13,title=php.xss::Tainted value reaches a sink in Db::list
::error file=src/Db.php,line=4,col=1,endLine=4,endColumn=22,title=php.sqli::Tainted value reaches a sink in Db::find
::error file=src/Db.php,line=4,col=14,endLine=4,endColumn=21,title=php.sqli2::Tainted value reaches a sink in Db::find2
::notice title=bleedthrough%3A coverage::coverage: all 12 file(s) analysed; suppressed: 12 low and 3 medium confidence, 7 by baseline
```

## sarif

One run with the rules that produced results, CWE and OWASP taxa as taxonomies referenced from
the rules, the fingerprint under `partialFingerprints["bleedthrough/v1"]`, traces as code flows,
fixes as SARIF fixes, and confidence, reasons, the paranoid flag and the score in result
properties. Unparsed files and limits hit are tool execution notifications, and
`executionSuccessful` is false when the analysis was incomplete.

## diff

A unified diff of the fixes, applicable with `git apply` from the project root. Fixes that overlap
an included fix, or whose source text no longer matches, are left out and reported to the
caller with a reason. Example (`mixed` report; the overlapping fix of `php.sqli2` was skipped):

```diff
diff --git a/lib/Shell.php b/lib/Shell.php
--- a/lib/Shell.php
+++ b/lib/Shell.php
@@ -1,2 +1,2 @@
 <?php
-echo shell_exec($_GET['cmd']);
\ No newline at end of file
+echo shell_exec(escapeshellarg($_GET['cmd']));
\ No newline at end of file
diff --git a/src/Db.php b/src/Db.php
--- a/src/Db.php
+++ b/src/Db.php
@@ -1,4 +1,4 @@
 <?php
 $id = $_GET['id'];
 $q = "SELECT * FROM t WHERE id = $id";
-mysqli_query($db, $q);
+mysqli_execute($db, $q);
```

## Testing

Golden files for every format and a fixed set of reports live in
`crates/emit/tests/golden/`; run the tests with `BLESS=1` to rewrite them and review the diff.
The examples above are checked against those files by a test.

The end-to-end tests that run the real binary belong to the `scan-pipeline` change. Each format
needs one: scan fixture projects with `--format <name>` and compare the output with the golden
files in `crates/emit/tests/golden/`, so the two layers cannot drift apart.
