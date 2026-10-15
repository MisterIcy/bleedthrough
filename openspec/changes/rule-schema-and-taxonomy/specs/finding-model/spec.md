## MODIFIED Requirements

### Requirement: Taxonomy tags are mandatory and edition-qualified
Every finding SHALL carry at least one CWE identifier and at least one OWASP Top 10 category, each OWASP category qualified by its edition (initially 2021), and each drawn from the taxonomy registry. A finding lacking either, or carrying a tag absent from the registry, SHALL be rejected when it is constructed, never emitted. The tag types SHALL be those defined by the taxonomy crate.

#### Scenario: Untagged finding rejected
- **WHEN** a finding is constructed with no CWE identifier or no OWASP category
- **THEN** construction fails with an error naming the missing tag kind

#### Scenario: Unregistered tag rejected
- **WHEN** a finding is constructed with a CWE absent from the registry
- **THEN** construction fails naming the number

#### Scenario: Second edition added later
- **WHEN** a finding carries OWASP categories from two editions
- **THEN** both are retained and distinguishable by edition

### Requirement: Severity is a level with an optional derived score
Severity SHALL be one of critical, high, medium, low or info. A finding produced from a rule SHALL carry the score, vector string and level derived from that rule's declared metrics by the severity model. A finding from a producer without metrics MAY carry only a level, in which case score and vector are absent. Severity SHALL be independent of confidence.

#### Scenario: Rule-derived severity
- **WHEN** a finding is produced from a rule whose metrics derive 9.8
- **THEN** the finding carries level critical, score 9.8 and the rule's vector string

#### Scenario: Severity without score
- **WHEN** a producer supplies only a severity level
- **THEN** the finding is valid and score and vector are absent

#### Scenario: Independence from confidence
- **WHEN** a critical-severity finding is produced with low confidence
- **THEN** both values are reported as given and neither alters the other
