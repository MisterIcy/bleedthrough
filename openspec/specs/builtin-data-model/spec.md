## Purpose

Defines the built-in data record model, its version semantics and its canonical text format, so the symbol table can resolve PHP's built-in symbols for any supported version from one deterministic, dependency-free source.

## Requirements

### Requirement: The model is a pure domain crate
The built-in data model, its parser and its writer SHALL live in a domain crate with no workspace or third-party dependencies and no I/O. It SHALL NOT depend on the syntax model or on any PHP parser.

#### Scenario: Purity
- **WHEN** the crate's manifest is inspected
- **THEN** it has no dependencies and the layering check passes

### Requirement: Records cover every kind of built-in symbol
There SHALL be record kinds for functions, classes, interfaces, traits, enums (with cases and backing type), class constants, global constants, properties and methods. A record SHALL carry its fully qualified name, its owning extension, and where it applies the SAPI, the build-guard text, and a deprecation (with since-version and message when known).

#### Scenario: Method record
- **WHEN** a method of a built-in class is recorded
- **THEN** it belongs to its class record and carries its own signature, modifiers and version mask

### Requirement: Signatures keep what the stubs and Reflection express
A function or method signature SHALL record, per parameter: name, declared type text, docblock-refined type text when it differs, by-reference flag, optional flag with the default expression as source text, variadic flag, and whether the parameter is sensitive; and, for the return: declared type text, tentative type text, docblock-refined type text, and by-reference return. Type texts SHALL be stored as PHP type syntax and SHALL NOT be interpreted by the data crate.

#### Scenario: Tentative return type
- **WHEN** a method has a tentative return type
- **THEN** it is stored in the tentative field and the declared field is empty

#### Scenario: By-reference parameter
- **WHEN** a function takes a parameter by reference
- **THEN** the flag is set on that parameter

### Requirement: Version availability is a mask, and signatures are per distinct form
Every record SHALL carry the set of PHP versions from 8.0 to 8.5 for which it applies, as a mask. When a symbol's signature differs between versions, there SHALL be one record per distinct signature, each with the mask of versions it applies to, and the masks of one symbol's records SHALL NOT overlap. Looking up a symbol for a version SHALL return the single record whose mask contains it, or report that the symbol is not available in that version, distinguishing "not in that version" from "unknown symbol".

#### Scenario: Signature changed in 8.1
- **WHEN** a function gains a parameter in 8.1
- **THEN** the data has two records with masks `8.0` and `8.1-8.5`

#### Scenario: Not available
- **WHEN** a function exists only from 8.4 and the version is 8.2
- **THEN** lookup reports "not available in 8.2", not "unknown"

#### Scenario: Overlapping masks rejected
- **WHEN** data contains two records for one symbol with overlapping masks
- **THEN** reading the data fails naming the symbol

### Requirement: Removal, unbundling and re-addition are distinguishable
The model SHALL represent an extension as bundled or unbundled per version (for example extensions moved to PECL), separately from a symbol being removed by the language, and masks SHALL be allowed to be non-contiguous.

#### Scenario: Unbundled extension
- **WHEN** an extension is no longer bundled from 8.4
- **THEN** its symbols are reported as unbundled, not removed, for 8.4 onwards

### Requirement: Aliases are explicit
An alias function SHALL be a record with its own signature and a reference to its target.

#### Scenario: Alias lookup
- **WHEN** an alias is looked up
- **THEN** the record names its target and has its own parameters

### Requirement: Provenance of each record is recorded
Each record SHALL state whether it came from the stubs or from the Reflection fixtures, and the Reflection-derived constant values SHALL be marked as such.

#### Scenario: Reflection-filled constant
- **WHEN** an 8.0 constant comes from the Reflection fixtures
- **THEN** its record says so

### Requirement: The text format is canonical and round-trips
The data SHALL be stored in a documented, line-oriented text format with a format version, records sorted by kind and fully qualified name and then by mask, fields in fixed order, and a single canonical escaping. Reading and then writing the data SHALL reproduce the input byte for byte. A reader SHALL reject an unsupported format version, unknown record kinds, duplicate records and malformed lines, reporting the line number.

#### Scenario: Round trip
- **WHEN** a data file is read and written
- **THEN** the output equals the input

#### Scenario: Malformed line
- **WHEN** a line has the wrong number of fields
- **THEN** reading fails naming the line

### Requirement: The data crate owns the source port
The crate SHALL define a `BuiltinDataSource` port yielding the parsed data, so any source (embedded file, test data) can be supplied without changing consumers. Consumers SHALL depend only on the port and the model.

#### Scenario: Test source
- **WHEN** a test supplies an in-memory source
- **THEN** consumers work without any file

### Requirement: Lookups are deterministic
Iteration orders and lookup results SHALL depend only on the data, never on hashing or insertion order.

#### Scenario: Same data, two loads
- **WHEN** the same data is loaded twice
- **THEN** all iterations are identical
