## ADDED Requirements

### Requirement: Attributes are kept on constants
An attribute written before a global `const` statement or a class constant SHALL be represented as an attribute of that constant, and the span of the constant SHALL include its attributes, so no attribute is reported as unconverted source.

#### Scenario: Deprecated constant
- **WHEN** `#[\Deprecated(since: '8.4')] const A = UNKNOWN;` is parsed
- **THEN** the tree has one constant statement carrying the attribute, and the file has no error node and no diagnostic

#### Scenario: Deprecated class constant
- **WHEN** a class declares `#[\Deprecated] public const int X = 1;`
- **THEN** the constant member carries the attribute and the file is clean
