## Purpose

Defines the rule model and the TOML file format in which rules and library knowledge are written, with strict validation, so that rules are reviewable data, errors are precise, and every consumer (engine, packs, cache, CI) sees the same deterministic rule set.

## ADDED Requirements

### Requirement: The rule model is pure and loader-independent
The rule model and its validation SHALL be defined in the rules crate without file, parser or serialization dependencies. Files SHALL be read and parsed by an infrastructure loader that hands the model to the rules crate through a port the rules crate owns, so a different format or source can be added without changing the model.

#### Scenario: Different source
- **WHEN** rules are supplied by a source other than TOML files
- **THEN** the model and validation are unchanged

### Requirement: Every rule has complete metadata
A rule SHALL have a stable identifier, a title, a description, a remediation text, taxonomy tags, severity metrics, a paranoid flag, and the PHP versions it applies to. It MAY have references and an owning pack. Identifiers SHALL match `BT` followed by four digits, SHALL be unique across the rule set, and SHALL never be reused for a different rule once released.

#### Scenario: Duplicate identifier
- **WHEN** two rules in the loaded set share an identifier
- **THEN** loading fails naming both locations

#### Scenario: Missing remediation
- **WHEN** a rule has no remediation text
- **THEN** validation fails naming the rule and the field

### Requirement: Rules are tagged or rejected
A rule without at least one registered CWE and one registered OWASP category SHALL be rejected at load time and SHALL never reach the engine.

#### Scenario: Untagged rule
- **WHEN** a rule file has no taxonomy tags
- **THEN** loading fails naming the rule

### Requirement: Taint rules describe flows as data
A taint rule SHALL declare sources, sinks, sanitizers and propagators. Each SHALL reference a PHP symbol by name (function, static or instance method, constructor, class, or superglobal) as written with its namespace, and SHALL NOT embed engine logic. A sink SHALL declare which arguments are vulnerable, by position or by parameter name, and the sink class it belongs to. A sanitizer SHALL declare the sink classes for which it neutralises taint, so a sanitizer for one class does not clear another. A propagator SHALL declare which inputs flow to the output.

#### Scenario: Sink with argument position
- **WHEN** a sink is declared for `mysqli_query` with argument 2 vulnerable
- **THEN** only taint reaching that argument is considered at that sink

#### Scenario: Wrong-context sanitizer
- **WHEN** a sanitizer declares only the shell-command class
- **THEN** it is not treated as clearing taint for the SQL class

### Requirement: Sink classes come from a closed vocabulary
Sink classes SHALL be drawn from a documented vocabulary (for example SQL, OS command, code evaluation, file path, file inclusion, deserialization, XML parsing, outgoing request, redirect target, header, LDAP query, XPath query, log). An unknown class SHALL be rejected. The vocabulary SHALL be extensible only by editing the rules crate.

#### Scenario: Unknown class
- **WHEN** a sink declares a class not in the vocabulary
- **THEN** validation fails naming the class

### Requirement: Pattern rules are simple and bounded
A pattern rule SHALL match one of a closed set of shapes: a call to a named symbol, optionally constrained by argument conditions (literal or non-literal, equal to or matching a literal value, absent); the use of a named function, class or construct; or an `include` or `require` with a non-literal path. It SHALL NOT contain arbitrary expressions or code. Regular expressions, where allowed, SHALL be restricted to a linear-time subset.

#### Scenario: Banned function
- **WHEN** a pattern rule declares use of `eval`
- **THEN** it is valid and needs no argument condition

#### Scenario: Unsupported shape
- **WHEN** a pattern rule declares a shape outside the closed set
- **THEN** validation fails naming the shape

### Requirement: Unknown keys and bad values are errors
The loader SHALL reject unknown keys, wrong types, and invalid values, and SHALL report every error found in a file, not only the first, each with the file path, the line and the key.

#### Scenario: Misspelt key
- **WHEN** a rule file contains `remedation`
- **THEN** loading fails reporting the file, line and the unknown key

### Requirement: Schema version is explicit
Every rule file SHALL declare its schema version. A file with a higher major version than the tool supports SHALL be rejected with a message naming both versions. Additive changes SHALL NOT increase the major version.

#### Scenario: Newer schema
- **WHEN** a file declares a major version the tool does not support
- **THEN** loading fails naming the file's version and the supported one

### Requirement: Loading is deterministic and hashable
The loaded rule set SHALL be ordered by rule identifier, independent of file enumeration order. It SHALL have a content hash that depends only on rule content, and that is reported in the run options and used in cache keys. Paths and comments in files SHALL NOT affect the hash.

#### Scenario: Reordered files
- **WHEN** the same rules are loaded from files enumerated in a different order
- **THEN** the rule set and its hash are identical

#### Scenario: Comment edit
- **WHEN** only a comment in a rule file changes
- **THEN** the hash is unchanged

### Requirement: Rule sets can be validated without a scan
The tool SHALL specify a validation entry point that loads and validates rule files and reports errors without analysing any code, so that rule authors and CI can check rules quickly. Its exit status SHALL follow the project's exit-code scheme.

#### Scenario: Invalid rules
- **WHEN** validation runs over a directory containing an invalid rule
- **THEN** it reports the errors and exits with the tool-error status
