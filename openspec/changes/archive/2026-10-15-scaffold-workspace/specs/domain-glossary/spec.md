## Purpose

Establishes a single ubiquitous language for the Bleedthrough domain so that code, rules, output and documentation use the same terms with the same meaning, and so that this vocabulary is checked rather than assumed.

## ADDED Requirements

### Requirement: A single glossary defines the domain language
The repository SHALL contain one glossary document defining each domain term with a definition. It SHALL define at least: Source, Sink, Sanitizer, Propagator, Taint, Entry point, Rule, Pack, Finding, Fingerprint, Severity, Confidence, Coverage, Summary, Baseline, Parse quality, Fix (a suggested patch for a finding), and Suppression (a baseline entry that silences a finding). Each term SHALL appear once.

#### Scenario: Required terms present
- **WHEN** the glossary is read
- **THEN** every required term is defined exactly once

### Requirement: Public domain types name their term
Every public type in a domain crate that represents a glossary concept SHALL state, in its documentation, the glossary term it represents. CI SHALL fail when a public domain type has no term statement, or names a term absent from the glossary. Types that represent no domain concept (such as error or helper types) SHALL be explicitly marked as exempt.

#### Scenario: Undocumented domain type
- **WHEN** a public domain type has neither a glossary term statement nor an exemption marker
- **THEN** the glossary check fails and names the type

#### Scenario: Unknown term
- **WHEN** a type documents a term that is not in the glossary
- **THEN** the glossary check fails and names the term

### Requirement: Glossary changes are deliberate
Adding, renaming or removing a glossary term SHALL be an explicit edit to the glossary, and renaming or removing a term SHALL fail the check until every reference is updated.

#### Scenario: Term removed while in use
- **WHEN** a glossary term is removed while a public type still names it
- **THEN** the glossary check fails
