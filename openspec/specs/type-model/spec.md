## Purpose

Defines the types the analysis reasons about, how native declarations and docblocks are turned into them, and how they are normalised, so that equal types compare equal and type uncertainty is explicit.

## Requirements

### Requirement: Types are canonical unions of atoms
A type SHALL be a set of atomic types in canonical form: duplicates removed, subsumed atoms absorbed (for example a literal string absorbed by `string`), ordering fixed, `mixed` absorbing everything it contains, and `never` the empty union. Equal types SHALL compare equal and hash equal.

#### Scenario: Normalisation
- **WHEN** the union `string|'a'|string` is constructed
- **THEN** the result is `string`

#### Scenario: Order independence
- **WHEN** `int|null` and `null|int` are constructed
- **THEN** they are equal

### Requirement: The atoms cover PHP's type system
Atoms SHALL include: null; booleans (true, false, bool); integers (with literal values and ranges where known); floats; strings (literal, non-empty, numeric, class-string with optional bound, general); arrays (keyed with key and value types, lists, non-empty, and shapes with required and optional keys); objects (a class-like name with optional type arguments, the generic `object`, and enum cases); callables (with optional signature); iterables; resources; void; never; mixed; and `unknown`. A static or self-referential object type SHALL be representable.

#### Scenario: Array shape
- **WHEN** a docblock says `array{id: int, name?: string}`
- **THEN** the type records two keys, the second optional

### Requirement: Unknown is distinct from mixed
`unknown` SHALL mean the analysis could not determine the type; `mixed` SHALL mean the type was determined to be arbitrary. Combining either with other atoms SHALL follow documented rules, and a result containing `unknown` SHALL be identifiable so that consumers can lower confidence.

#### Scenario: Unknown propagates
- **WHEN** `unknown` is united with `string`
- **THEN** the result is marked as containing unknown

### Requirement: Unions are bounded
A union exceeding a configured size SHALL be widened to a documented supertype, and the result SHALL record that it was widened. The same input SHALL always widen the same way.

#### Scenario: Large union
- **WHEN** a union would exceed the limit
- **THEN** it is widened and carries the widened flag

### Requirement: Native types are converted exactly
Native parameter, return and property types, including nullable, union, intersection and disjunctive-normal-form types, `self`, `static`, `parent`, `iterable`, `callable`, `never`, `void`, and enum backing types, SHALL be converted to the type model with names resolved. Intersections that are not representable SHALL be kept as intersections, not simplified away.

#### Scenario: Nullable
- **WHEN** a parameter is declared `?Foo`
- **THEN** its type is `Foo|null` with `Foo` fully qualified

### Requirement: Docblock types are parsed by an owned parser
The system SHALL parse the type expressions of `@param`, `@return`, `@var`, `@property`, `@property-read`, `@property-write` and `@method` tags, in the core PHPDoc and Psalm subset: class names, scalar names, nullable and union and intersection forms, arrays (`T[]`, `array<K,V>`, `list<T>`, `non-empty-array`, shapes), `class-string<T>`, literal types, `callable(...)`, `iterable<...>`, `self`/`static`, and `@template` declarations (recorded, with simple substitution when type arguments are given). The parser SHALL NOT depend on any third-party docblock library.

#### Scenario: Generic array
- **WHEN** a docblock says `@param array<string, int> $x`
- **THEN** the parameter has a keyed array type with string keys and int values

### Requirement: Malformed docblocks degrade gracefully
A docblock or tag that cannot be parsed SHALL produce a diagnostic with its span and SHALL leave the native type (or unknown) in force; it SHALL NOT fail the file or panic. Unsupported tags and type constructs outside the subset SHALL be ignored with an informational diagnostic.

#### Scenario: Broken type
- **WHEN** a docblock says `@param array<string, $x`
- **THEN** a diagnostic locates the problem and the native type of the parameter is used

### Requirement: Native and docblock types are merged by explicit rules
Where both exist, the effective type SHALL be the docblock type if it is a refinement of the native type (a subtype), and otherwise the native type with a diagnostic about the contradiction. A docblock for a parameter without a native type SHALL be used as the effective type. Which source produced the effective type SHALL be recorded.

#### Scenario: Refinement
- **WHEN** the native type is `array` and the docblock says `list<string>`
- **THEN** the effective type is `list<string>` and the source is docblock

#### Scenario: Contradiction
- **WHEN** the native type is `int` and the docblock says `string`
- **THEN** the effective type is `int` and a diagnostic is reported

### Requirement: Types render stably
Types SHALL render to a stable text form that parses back to an equal type for the representable subset, and that is used in tests and in finding trace labels.

#### Scenario: Round trip
- **WHEN** `array{id: int}|null` is rendered and parsed
- **THEN** the result equals the original
