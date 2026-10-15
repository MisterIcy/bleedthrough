## Purpose

Defines the output formats Bleedthrough can write, the structure and escaping rules of each, and the guarantees common to all of them, so that CI systems, code-scanning platforms and agents can consume results reliably and concisely.

## ADDED Requirements

### Requirement: Six output formats
The system SHALL provide these formats: `json`, `sarif`, `ndjson`, `tsv`, `github-actions`, and `diff`. The name `jsonl` SHALL be accepted as an alias of `ndjson` and SHALL produce identical output. Selecting an unknown format SHALL be an error that lists the valid names.

#### Scenario: Alias
- **WHEN** the format `jsonl` is requested
- **THEN** the output is byte-identical to the output for `ndjson`

#### Scenario: Unknown format
- **WHEN** a format name is requested that is not provided
- **THEN** the request fails and the error lists the six format names

### Requirement: Guarantees common to every format
Output SHALL be UTF-8, SHALL NOT contain colour or terminal control sequences, SHALL be byte-identical for identical reports, and SHALL contain nothing derived from wall-clock time. Every format SHALL be valid and well-formed for a report with zero findings.

#### Scenario: Zero findings
- **WHEN** a report has no findings
- **THEN** every format emits a valid empty result (an empty list, an empty SARIF results array, a header-only table, no annotations, an empty patch)

#### Scenario: Determinism
- **WHEN** the same report is emitted twice in the same format
- **THEN** the outputs are byte-identical

### Requirement: Coverage is never silently dropped
Every format SHALL convey the report's coverage and suppressed counts. Formats with a native place for them (`json`, `ndjson`, `sarif`) SHALL embed them. For `github-actions` they SHALL appear as a notice annotation. For `tsv` and `diff`, which have no place for them without breaking their format, the emission result SHALL return a one-line human-readable coverage summary for the caller to display.

#### Scenario: Incomplete run in a table format
- **WHEN** a report with failed-to-parse files is emitted as `tsv`
- **THEN** the emission result carries a summary stating that files were not analysed

### Requirement: JSON format
The `json` format SHALL be one JSON document containing the schema version, tool information, run options, the ordered findings, the coverage block and the suppressed counts. Each finding SHALL include every field of the finding model. The document SHALL conform to the published schema.

#### Scenario: Schema conformance
- **WHEN** a report is emitted as `json`
- **THEN** the output validates against the published JSON Schema for that schema version

### Requirement: NDJSON format
The `ndjson` format SHALL emit one JSON object per line, each with a `type` field: a first `run` record carrying schema version, tool and run options; one `finding` record per finding in order; and a final `summary` record carrying coverage and suppressed counts. A consumer SHALL be able to detect an interrupted run by the absence of the final summary record. Records SHALL NOT contain embedded newlines.

#### Scenario: Interrupted stream
- **WHEN** emission stops before completion
- **THEN** the output lacks a `summary` record, and no partial line is left in the middle of a record

#### Scenario: One record per line
- **WHEN** a finding message contains a newline character
- **THEN** it is escaped within the JSON string and the record remains on one line

### Requirement: SARIF 2.1.0 format
The `sarif` format SHALL produce a valid SARIF 2.1.0 log with one run. It SHALL declare each rule that produced results; map CWE and OWASP tags to taxonomies referenced by the rules; express each finding as a result with location, message and fingerprint under a versioned partial-fingerprint key; express traces as code flows; express fixes as SARIF fixes; carry confidence, its reasons, the paranoid flag, and the severity score in result properties; and map severity to SARIF levels (critical and high to error, medium to warning, low and info to note). It SHALL express the run's coverage problems (unparsed files, limits hit) as tool execution notifications, and SHALL state whether the invocation was complete.

#### Scenario: Code-scanning ingestion
- **WHEN** a SARIF log is produced for findings with traces
- **THEN** each result has a code flow whose steps match the finding's trace in order

#### Scenario: Fingerprint carried
- **WHEN** a result is produced
- **THEN** it carries the finding's fingerprint under the versioned partial-fingerprint key

#### Scenario: Incomplete analysis visible
- **WHEN** some files failed to parse
- **THEN** the log contains a notification for those files and marks the invocation as incomplete

### Requirement: TSV format
The `tsv` format SHALL emit a header row and one row per finding with fixed columns in this order: path, line, column, end line, end column, rule, severity, confidence, CWE identifiers, OWASP categories, fingerprint, message. Multiple CWE or OWASP values SHALL be comma-separated within the cell. Tab, newline, carriage return and backslash characters in any cell SHALL be escaped as `\t`, `\n`, `\r` and `\\`. The header row MAY be omitted on request.

#### Scenario: Escaping
- **WHEN** a message contains a tab and a newline
- **THEN** the row contains the two-character escapes and remains a single line with the correct number of columns

### Requirement: GitHub Actions format
The `github-actions` format SHALL emit one workflow-command annotation per finding with file, line, column, end line, end column, a title containing the rule identifier, and the message, using the error, warning or notice command according to severity (critical and high as error, medium as warning, low and info as notice). Property values and messages SHALL be escaped as the workflow-command syntax requires. When the number of annotations exceeds a configured cap, the highest severity then highest confidence findings SHALL be emitted first and a final notice SHALL state how many were omitted. A notice SHALL convey coverage and suppressed counts.

#### Scenario: Escaping
- **WHEN** a message contains a percent sign, a newline or a colon in a title
- **THEN** the emitted annotation escapes them so that the workflow command parses to the original text

#### Scenario: Annotation cap
- **WHEN** more findings exist than the cap
- **THEN** only the cap's worth are annotated, ordered by severity then confidence, and a notice states the number omitted

### Requirement: Diff format
The `diff` format SHALL emit a unified diff, in the form accepted by `git apply`, of the fixes of the findings that have one, with file paths relative to the project root. Fixes whose edits overlap an already included fix in the same file SHALL be left out of the patch and reported in the emission result as skipped, with the reason. Findings without a fix SHALL NOT appear. The patch SHALL be produced from the original source text, and if a file's current content no longer matches the content analysed, that file's fixes SHALL be skipped and reported.

#### Scenario: Applying the patch
- **WHEN** the patch is applied to the analysed checkout with `git apply`
- **THEN** it applies cleanly and each included fix's edits are made

#### Scenario: Conflicting fixes
- **WHEN** two findings have fixes that overlap
- **THEN** one is included, the other is reported as skipped with a reason, and the patch still applies cleanly

### Requirement: Output conforms to the published schema and golden files
The JSON Schema for the `json` and `ndjson` records SHALL be published in the repository and versioned with the report schema version. The repository's tests SHALL validate emitter output against it and SHALL compare each format's output for a fixed set of reports with committed expected files.

#### Scenario: Output regression
- **WHEN** a change alters the output of any format for the fixed reports
- **THEN** the tests fail until the expected files are intentionally updated

### Requirement: Emission failures are reported
An emitter that cannot write its output SHALL report an error to the caller and SHALL NOT report success. Output that is incomplete SHALL be distinguishable from complete output wherever the format allows (the `ndjson` summary record, and the SARIF invocation status).

#### Scenario: Write failure
- **WHEN** the destination fails mid-write
- **THEN** the emitter returns an error and the caller can exit with a tool-error status
