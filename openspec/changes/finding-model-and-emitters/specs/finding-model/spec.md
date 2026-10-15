## Purpose

Defines what a finding and a whole report contain, which invariants they guarantee, and how they are versioned and ordered, so that every producer (the analysis engine) and every consumer (emitters, baselines, CI gates, agents) shares one trustworthy contract.

## ADDED Requirements

### Requirement: A finding carries identity, evidence and classification
A finding SHALL carry: the identifier of the rule that produced it, a concise message, a primary location, taxonomy tags, a severity, a confidence tier with reasons, a fingerprint, and a flag stating whether it came from a rule that runs only in paranoid mode. It MAY carry a trace and a fix. A finding SHALL NOT carry any wall-clock time or machine-specific data such as absolute paths.

#### Scenario: Complete finding
- **WHEN** the engine reports a tainted flow from a request parameter to a database query
- **THEN** the finding has a rule identifier, message, location, taxonomy tags, severity, confidence tier, reasons, fingerprint and paranoid flag, and the trace if one was available

#### Scenario: No machine-specific data
- **WHEN** the same code is analysed from two different checkout directories
- **THEN** the findings are identical

### Requirement: Taxonomy tags are mandatory and edition-qualified
Every finding SHALL carry at least one CWE identifier and at least one OWASP Top 10 category, each OWASP category qualified by its edition (initially 2021). A finding lacking either SHALL be rejected when it is constructed, never emitted.

#### Scenario: Untagged finding rejected
- **WHEN** a finding is constructed with no CWE identifier or no OWASP category
- **THEN** construction fails with an error naming the missing tag kind

#### Scenario: Second edition added later
- **WHEN** a finding carries OWASP categories from two editions
- **THEN** both are retained and distinguishable by edition

### Requirement: Locations are portable and precise
A location SHALL give a path relative to the analysed project root using forward slashes, a start and end position with 1-based line and 1-based column counted in Unicode scalar values, and byte offsets of the same range. Paths SHALL NOT be absolute and SHALL NOT contain parent-directory segments.

#### Scenario: Windows checkout
- **WHEN** a file is analysed on a platform with backslash separators
- **THEN** the finding's path uses forward slashes

#### Scenario: Multi-byte characters
- **WHEN** a flagged expression follows non-ASCII characters on the same line
- **THEN** the column counts characters, and the byte offsets locate the same text

### Requirement: Traces explain the flow
A trace, when present, SHALL be an ordered list of steps from a source to a sink. Each step SHALL have a location, a kind (source, propagation, call, sanitizer-considered, or sink) and a short label. The first step SHALL be a source and the last a sink. A trace longer than the configured limit SHALL be truncated in the middle, retaining the first and last steps, and SHALL be marked as truncated.

#### Scenario: Trace ends are preserved
- **WHEN** a trace exceeds the length limit
- **THEN** the retained trace still begins with the source and ends with the sink, and is marked truncated

### Requirement: Severity is a level with an optional derived score
Severity SHALL be one of critical, high, medium, low or info, and MAY additionally carry a numeric score and a vector string when the producer derives them. Severity SHALL be independent of confidence.

#### Scenario: Severity without score
- **WHEN** a producer supplies only a severity level
- **THEN** the finding is valid and score and vector are absent

#### Scenario: Independence from confidence
- **WHEN** a critical-severity finding is produced with low confidence
- **THEN** both values are reported as given and neither alters the other

### Requirement: Confidence is a tier with documented reasons
Confidence SHALL be one of high, medium or low. A finding below high SHALL carry at least one reason. Every reason SHALL be a code from a documented vocabulary, optionally with a short detail string; the vocabulary SHALL include at least: source inferred, dynamic dispatch with several candidates, unresolved callee assumed to propagate, parse recovered, type unknown, analysis widened, and sanitizer wrong for sink context. Unknown codes SHALL be rejected at construction.

#### Scenario: Medium without reason
- **WHEN** a finding is constructed with medium confidence and no reason
- **THEN** construction fails

#### Scenario: Reason with detail
- **WHEN** a finding has the reason "dynamic dispatch with several candidates" and detail "3"
- **THEN** both are reported and the code is machine-readable

### Requirement: Fingerprints are stable
A fingerprint SHALL be computed from the rule identifier, the enclosing symbol, and normalized key snippets of the flow, plus an occurrence index to distinguish identical flows within one symbol. It SHALL NOT depend on line numbers, absolute paths, or unrelated parts of the file. It SHALL carry a scheme version, and a change of scheme SHALL change the visible version prefix.

#### Scenario: Code moves
- **WHEN** a flow's code is moved down the file by inserting unrelated lines above it
- **THEN** its fingerprint is unchanged

#### Scenario: Formatting only
- **WHEN** whitespace or comments within the flow change
- **THEN** its fingerprint is unchanged

#### Scenario: Different flow
- **WHEN** the sink call or the tainted source expression changes
- **THEN** the fingerprint changes

#### Scenario: Identical flows in one function
- **WHEN** two identical flows exist in the same symbol
- **THEN** their fingerprints differ through the occurrence index

### Requirement: Fixes are optional, explicit and non-overlapping
A fix, when present, SHALL contain a description, an applicability marking (mechanical, or needs review), and one or more edits, each with a path, a byte range, the original text expected in that range, and replacement text. Edits within one fix SHALL NOT overlap. A finding without a known safe fix SHALL carry none.

#### Scenario: Overlapping edits rejected
- **WHEN** a fix is constructed with two edits that overlap
- **THEN** construction fails

### Requirement: Coverage states what was and was not analysed
Every report SHALL contain a coverage block stating: counts of files by parse quality (clean, recovered, failed) and skipped files with reasons; analysis limits that were hit; counts of unresolved dynamic includes and unresolved dynamic calls; the active packs with versions; the PHP version modelled; and, when a cache was used, hits, misses and invalidations. A report with no findings SHALL still contain coverage.

#### Scenario: Clean result with unparsed files
- **WHEN** a run finds nothing but some files failed to parse
- **THEN** the report contains zero findings and a coverage block that counts the failed files

#### Scenario: Limit hit
- **WHEN** a per-file time limit stops the analysis of a file
- **THEN** coverage lists the limit and the file as skipped with that reason

### Requirement: Suppressed counts are always reported
A report SHALL state how many findings were hidden because they fell below the confidence threshold (per tier) and how many were silenced by the baseline, so that a hidden finding is never invisible.

#### Scenario: Findings below threshold
- **WHEN** twelve medium-confidence findings are hidden by a high-only threshold
- **THEN** the report states twelve hidden at medium confidence

### Requirement: Reports are self-describing and versioned
A report SHALL carry the tool name and version, a schema version, and the run options that affect results (confidence threshold, paranoid mode, modelled PHP version, and an identifier of the rule and pack set). Breaking changes to the report structure SHALL increase the schema's major version; additive changes SHALL NOT.

#### Scenario: Consumer checks the version
- **WHEN** a consumer reads a report
- **THEN** it can determine the schema version before interpreting any other field

### Requirement: Findings are totally ordered and reports are deterministic
Findings in a report SHALL be ordered by path, then start position, then rule identifier, then fingerprint. Reports SHALL contain no timestamps, durations, or process identifiers. The same inputs SHALL always produce an identical report, independent of thread count or filesystem enumeration order.

#### Scenario: Different thread counts
- **WHEN** the same project is analysed with one thread and with many
- **THEN** the reports are identical

#### Scenario: Ties
- **WHEN** two findings share path, position and rule
- **THEN** they are ordered by fingerprint
