## Purpose

Defines the shared vocabulary of weakness and risk categories that rules and findings are tagged with, and the curated registry that keeps those tags correct, so a tag can never be misspelt, invented or silently ambiguous between editions.

## ADDED Requirements

### Requirement: The vocabulary lives in one pure crate
CWE identifiers, OWASP categories with their edition, and the registry SHALL be defined in a single domain crate that has no workspace or third-party dependency and that both the rule model and the finding model depend on. Neither the rule model nor the finding model SHALL define its own copy.

#### Scenario: One definition
- **WHEN** a rule and a finding refer to the same CWE
- **THEN** both use the same type from the taxonomy crate

### Requirement: CWE identifiers are validated against a curated registry
The registry SHALL list every weakness the tool supports, each with its CWE number, its official name, and the version of the CWE catalog against which the name was checked. Constructing a tag for a CWE not in the registry SHALL fail with an error naming the number. Adding a weakness SHALL be an explicit edit to the registry.

#### Scenario: Known weakness
- **WHEN** a tag is constructed for CWE-89
- **THEN** it succeeds and the registry yields the official name

#### Scenario: Unknown weakness
- **WHEN** a tag is constructed for a number absent from the registry
- **THEN** construction fails naming the number

### Requirement: OWASP categories are edition-qualified and registered
The registry SHALL contain the OWASP Top 10 2021 categories A01 to A10 with their official names, and SHALL be able to hold further editions without changing existing entries. A category reference SHALL always carry its edition and SHALL fail if the edition or id is not registered.

#### Scenario: Registered category
- **WHEN** a reference is constructed for edition 2021, A03
- **THEN** it succeeds and the registry yields "Injection"

#### Scenario: Unregistered edition
- **WHEN** a reference is constructed for an edition the registry lacks
- **THEN** construction fails naming the edition

### Requirement: Weakness to category mapping is checked, not assumed
Each registry weakness SHALL list the OWASP 2021 categories it is mapped to. A rule or finding SHALL carry at least one weakness and at least one category. For a rule, a category SHALL be accepted only if it is in the mapping of at least one of the rule's weaknesses or is added with an explicit stated reason; findings inherit their tags from the rule that produced them and are not re-checked.

#### Scenario: Consistent tags
- **WHEN** a rule is tagged CWE-89 and OWASP 2021 A03
- **THEN** the tags are accepted

#### Scenario: Unrelated category
- **WHEN** a rule is tagged CWE-89 and OWASP 2021 A02 with no stated reason
- **THEN** validation fails naming the pair

### Requirement: v1 scope of the registry
The initial registry SHALL include the weaknesses in v1 scope, among them injection classes (SQL, OS command, code, LDAP, XPath), path traversal, unsafe file inclusion and upload, unsafe deserialization, XML external entities, server-side request forgery, open redirect (CWE-601), weak cryptography and randomness, hard-coded credentials, and insecure configuration. It SHALL NOT include cross-site scripting (CWE-79) in v1. Names and mappings SHALL be taken from the MITRE catalog and the OWASP publication at authoring time, with the sources recorded.

#### Scenario: Open redirect supported
- **WHEN** a rule is tagged CWE-601 with OWASP 2021 A01
- **THEN** the tags are accepted

#### Scenario: XSS not yet supported
- **WHEN** a rule is tagged CWE-79
- **THEN** validation fails as an unregistered weakness

### Requirement: Tags have stable text forms
A CWE SHALL be rendered as `CWE-<number>` and an OWASP category as `OWASP:<edition>:<id>`, and both SHALL parse back to the same values.

#### Scenario: Round trip
- **WHEN** `OWASP:2021:A03` is parsed and rendered
- **THEN** the result is `OWASP:2021:A03`
