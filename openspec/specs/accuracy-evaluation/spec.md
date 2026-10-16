## Purpose

Defines how detection quality is measured: scoring a scan's machine-readable report against the labelled corpus, the metrics reported, how results are compared with committed expectations so quality can only ratchet upward deliberately, and how the evaluation behaves while no scan exists.

## Requirements

### Requirement: Evaluation consumes the public report
The evaluator SHALL obtain findings only from the `json` report that the `bleedthrough` binary writes for a corpus tree, validated against the published report schema, and SHALL NOT use any engine internals. A report that fails schema validation, or a scan that exits abnormally, SHALL fail the evaluation and be shown in the output.

#### Scenario: Invalid report
- **WHEN** the scan's report does not validate against the schema
- **THEN** the evaluation fails with the validation error and no metrics are reported

#### Scenario: Engine swapped
- **WHEN** the engine is rewritten and the report remains valid
- **THEN** the evaluator needs no change

### Requirement: Findings are matched to labels deterministically
A reported finding SHALL match an expected finding when it is in the same file, its CWE equals the expectation's CWE, its range overlaps the expected line range, and its confidence tier is at least the expected minimum. A finding with a matching CWE in a `safe` case's protected range SHALL count as a false positive. A finding that matches no expectation and lies outside every protected range SHALL be counted as unlabelled and reported separately, never as a false positive or true positive. Matching SHALL not depend on the order of findings.

#### Scenario: Correct finding
- **WHEN** a finding of the expected CWE overlaps the expected range at or above the minimum confidence
- **THEN** it counts as a true positive

#### Scenario: Finding in a safe twin
- **WHEN** a finding of the case's CWE is reported inside a safe case's protected range
- **THEN** it counts as a false positive and names the case

#### Scenario: Missed vulnerability
- **WHEN** a vulnerable case has an expected finding and the report has none that match
- **THEN** it counts as a false negative and names the case

#### Scenario: Unlabelled finding
- **WHEN** a finding matches no expectation and lies outside every protected range
- **THEN** it is listed as unlabelled and affects neither precision nor recall

### Requirement: Metrics are reported per weakness class and per rule
The evaluation SHALL report, for each CWE and for each rule that produced a matched or false-positive finding, the counts of true positives, false positives and false negatives, precision and recall, and the number of known gaps. It SHALL also report overall totals, and SHALL report a metric as undefined, not as zero or one, when its denominator is zero. Output SHALL be available as a human-readable table and as machine-readable JSON with a versioned schema, and SHALL be byte-identical for identical inputs.

#### Scenario: No cases for a class
- **WHEN** a class has no vulnerable cases
- **THEN** its recall is reported as undefined

#### Scenario: Reproducible output
- **WHEN** the evaluation runs twice on the same report and corpus
- **THEN** the machine-readable output is byte-identical

### Requirement: Results are compared with committed expectations
A committed expectations file SHALL record, per CWE, the minimum true-positive count and the maximum false-positive count the project accepts, and the set of known gaps. The evaluation SHALL fail when a count is worse than the expectation, when a case outside the known gaps fails, or when the report contains a case result absent from the expectations file. It SHALL also fail when a known gap now passes, naming the case, so that the improvement is recorded by deliberately updating the expectations and removing the gap. The expectations file SHALL be updated only by an explicit command whose diff is reviewable.

#### Scenario: Regression
- **WHEN** a change causes a previously found vulnerability to be missed
- **THEN** the evaluation fails and names the case and class

#### Scenario: Improvement must be recorded
- **WHEN** a known-gap case is now found
- **THEN** the evaluation fails until the gap is removed from the expectations

#### Scenario: Update is explicit
- **WHEN** the update command is run
- **THEN** only the expectations file changes, and the diff lists each changed count and each added or removed gap

### Requirement: Absence of a scan is explicit
While the `bleedthrough` binary offers no scan command, or the scan reports that it is not implemented, the evaluation SHALL report `not evaluated` with the reason, SHALL exit with a status distinct from success and failure, and SHALL NOT report any metric. The required CI status SHALL treat `not evaluated` as a pass only until a documented switch records that the scan exists, after which `not evaluated` SHALL fail.

#### Scenario: No scan yet
- **WHEN** the evaluation runs before a scan command exists
- **THEN** it prints `not evaluated` with the reason and exits with the distinct status

#### Scenario: Switch recorded
- **WHEN** the scan switch is recorded and the evaluation reports `not evaluated`
- **THEN** CI fails

### Requirement: The scorer is itself tested
The evaluator SHALL be tested against hand-written reports whose correct metrics are known, covering at least: every matching rule, every metric, undefined metrics, known-gap handling, unlabelled findings, malformed reports, and order independence. These tests SHALL run offline in required CI.

#### Scenario: Known report
- **WHEN** the scorer runs on a hand-written report with a known outcome
- **THEN** the computed counts equal the documented ones
