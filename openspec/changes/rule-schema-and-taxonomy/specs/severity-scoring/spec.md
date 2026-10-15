## Purpose

Defines how a rule's severity is declared and turned into a score, a vector string and a level by one fixed, versioned formula, so severity is reviewable in the rule file, reproducible, and independent of the analysis engine and of confidence.

## ADDED Requirements

### Requirement: Rules declare base metrics
A rule SHALL declare its severity by the eight CVSS 3.1 base metrics: attack vector, attack complexity, privileges required, user interaction, scope, and confidentiality, integrity and availability impact. All eight SHALL be required, and each SHALL take only the values the CVSS 3.1 specification defines.

#### Scenario: Missing metric
- **WHEN** a rule omits the scope metric
- **THEN** validation fails naming the rule and the metric

#### Scenario: Invalid value
- **WHEN** a rule gives an attack vector value outside the specification
- **THEN** validation fails naming the value

### Requirement: A fixed formula derives the score
The score SHALL be computed from the metrics by the CVSS 3.1 base-score equations, using the specification's round-up rule, in integer or exact arithmetic so the result is identical on every platform. The score SHALL be reported with one decimal place. Declared scores SHALL NOT be accepted; only metrics are.

#### Scenario: Reference vector
- **WHEN** the metrics are network, low complexity, no privileges, no interaction, unchanged scope, high/high/high impact
- **THEN** the score is 9.8

#### Scenario: Declared score rejected
- **WHEN** a rule file supplies a numeric score
- **THEN** validation fails as an unknown field

### Requirement: The vector string is standard and versioned
The vector string SHALL use the CVSS 3.1 vector syntax and SHALL be present on every finding. The model SHALL document that the value is the rule author's base-metric estimate and not an official CVSS assessment of a specific vulnerability. The formula SHALL carry a scheme version recorded in reports, and a change to it SHALL change that version.

#### Scenario: Vector shape
- **WHEN** the reference metrics are rendered
- **THEN** the vector is `CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H`

### Requirement: Level is derived from the score
The severity level SHALL be derived from the score: 0.0 is info, 0.1 to 3.9 low, 4.0 to 6.9 medium, 7.0 to 8.9 high, and 9.0 to 10.0 critical. A rule SHALL NOT override the level.

#### Scenario: Boundary
- **WHEN** a score is exactly 7.0
- **THEN** the level is high

### Requirement: Severity is independent of confidence and of the flow
The severity of a finding SHALL equal the severity of its rule and SHALL NOT change with confidence, the shape of the flow, or the project analysed. Confidence SHALL remain a separate field.

#### Scenario: Low confidence
- **WHEN** a finding from a critical rule has low confidence
- **THEN** its severity is still critical
