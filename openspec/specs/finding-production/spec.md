## Purpose

Defines how engine results become findings in the common model: traces, confidence and its reasons, severity, fingerprints and coverage, and the guarantees about determinism and independence from scheduling that let the application layer parallelise and cache safely.

## Requirements

### Requirement: Each flow becomes one finding with a trace
For each distinct combination of rule, source location and sink location, the engine SHALL produce exactly one finding at the sink location, with a trace listing the source, the propagation steps that carry the taint (assignments and operations that change the representation), the calls the flow passes through, any considered sanitizer, and the sink, each with a location and a short label. Traces SHALL be cut in the middle when over the configured length, keeping both ends and marking the cut.

#### Scenario: Two sources reach one sink
- **WHEN** two different sources reach the same sink argument
- **THEN** two findings are produced, one per source

#### Scenario: Same flow reached by two paths
- **WHEN** the same source and sink are connected by two paths
- **THEN** one finding is produced

#### Scenario: Long trace
- **WHEN** a flow has more steps than the trace limit
- **THEN** the trace keeps the first and last steps and is marked truncated

### Requirement: Confidence is derived from stated reasons
A finding SHALL have high confidence only when its flow involves no inferred source, no multi-candidate dispatch, no assumed-propagating unresolved callee, no widening, no recovery-parsed file on the path, no unknown type needed for the decision and no wrong-sanitizer note. Each present condition SHALL add its reason from the closed vocabulary, and the tier SHALL be the lowest tier any reason implies: `low` for an assumed-propagating unresolved callee, a recovery-parsed file and widening; `medium` for an inferred source, multi-candidate dispatch, an unknown type and a wrong sanitizer. A finding below high confidence SHALL carry at least one reason.

#### Scenario: Clean flow
- **WHEN** a flow is direct, resolved and complete
- **THEN** the finding has high confidence and no reasons

#### Scenario: Multiple conditions
- **WHEN** a flow both passes through multi-candidate dispatch and an unresolved callee
- **THEN** the finding carries both reasons and the tier `low`

#### Scenario: Medium reason only
- **WHEN** the only reason is an inferred source
- **THEN** the tier is `medium`

### Requirement: Severity and classification come from the rule
A finding's severity SHALL be the severity the rule's base metrics derive, and its taxonomy SHALL be the rule's CWE and OWASP tags. Neither SHALL vary with confidence or with the flow. The paranoid flag SHALL be copied from the rule.

#### Scenario: Same rule, different flows
- **WHEN** the same rule yields two flows of different length and confidence
- **THEN** both findings have the identical severity and taxonomy

### Requirement: Fingerprints are stable and meaningful
A finding's fingerprint SHALL be computed by the finding model's rule from the rule identifier, the file, the normalised sink text and the source, and SHALL NOT change when unrelated lines above the sink are inserted or removed, and SHALL change when the sink or source changes.

#### Scenario: Lines inserted above
- **WHEN** unrelated lines are inserted above a flow
- **THEN** the finding's location shifts and its fingerprint is unchanged

### Requirement: Coverage is reported honestly
The engine's result SHALL report the number of calls left unresolved by reason, includes left unresolved, graphs marked recovered, units skipped, and every limit that was hit by kind and location, so that a clean report can be told apart from an analysis that could not see. These SHALL be in the form the report's coverage section consumes.

#### Scenario: Unresolved calls counted
- **WHEN** the analysed code contains unresolved calls
- **THEN** the result reports their count by reason

### Requirement: Results are deterministic and independent of scheduling
The set and order of findings, their traces and the coverage counts SHALL be identical for identical inputs regardless of the order in which files or units are processed, the number of workers, and whether summaries were computed in one pass or incrementally. Findings SHALL be returned in a canonical order.

#### Scenario: Different processing order
- **WHEN** the same project is analysed with units processed in a different order
- **THEN** the findings and coverage are identical

### Requirement: Work is exposed as order-independent units
The engine SHALL provide separate operations to lower a file, to compute the summaries of one call-graph component given the summaries of its callees, and to scan one unit given the summaries, and each SHALL be a pure function of its explicit inputs, so that an application can run independent operations concurrently. The engine SHALL NOT create threads, read files or the clock, or use global mutable state.

#### Scenario: Two independent components
- **WHEN** two components have no dependency on each other
- **THEN** their summaries can be computed in either order or concurrently with identical results

### Requirement: Rule problems and internal faults do not abort analysis
A unit whose analysis fails an internal invariant SHALL be reported as skipped with a reason and counted in coverage, and analysis of other units SHALL continue. The engine SHALL NOT panic on any syntax tree.

#### Scenario: Internal fault in one unit
- **WHEN** one unit hits an internal fault
- **THEN** it is reported as skipped with the reason and other units' findings are unaffected
