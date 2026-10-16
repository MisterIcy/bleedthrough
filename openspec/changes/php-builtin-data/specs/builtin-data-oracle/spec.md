## Purpose

Defines the Reflection fixtures taken from real PHP interpreters, which complete the data where the stubs are silent and serve as an independent check that the generated data is correct.

## ADDED Requirements

### Requirement: Fixtures come from a reviewed script on pinned interpreters
Reflection fixtures SHALL be produced by a PHP script kept in the repository and reviewed, run in the official PHP command-line image of each version from 8.0 to 8.5, with each image pinned by digest, and the output SHALL be checked in as JSON together with the PHP version, the digest, and the list of loaded extensions. Running the script SHALL be a manual, opt-in step that required CI never performs.

#### Scenario: Reproducibility record
- **WHEN** a fixture file is opened
- **THEN** it states the PHP version, the image digest and the extensions loaded

### Requirement: The script reports facts only
The dump SHALL contain, per version: functions with parameters (name, type text, by-reference, optional, variadic, default presence), return types including tentative return types, classes, interfaces, traits and enums with parents, interfaces, constants with values, properties with types and modifiers, methods, and global constants with values. It SHALL contain no documentation text.

#### Scenario: Constants with values
- **WHEN** a global constant is dumped
- **THEN** its value and type are included

### Requirement: Fixtures fill what the stubs lack
For PHP 8.0 and 8.1, whose stubs contain no constants and few properties, the generated data SHALL take constants and properties from the fixtures. For all versions, constant values SHALL come from the fixtures where available. Every record or field derived this way SHALL be marked as Reflection-derived.

#### Scenario: 8.0 constant
- **WHEN** an 8.0 global constant is absent from the stubs
- **THEN** it appears in the data from the fixture, marked as Reflection-derived

### Requirement: The generated data is cross-checked against the fixtures
A test SHALL compare the generated data with the fixtures for each version: every function, class and constant present in a fixture SHALL be present in the data with equal parameter count, by-reference flags, optional and variadic flags, return and tentative types, parents, interfaces and constant values, apart from reviewed allowances. Differences SHALL fail the test and name the symbol and the field.

#### Scenario: Wrong by-reference flag
- **WHEN** the generated data lacks a by-reference flag that the fixture reports
- **THEN** the cross-check fails naming the function and parameter

### Requirement: Allowances are reviewed and reasoned
Expected differences (extensions not loaded in the image, symbols that exist only on some builds, type spellings that differ but are equivalent) SHALL be listed in an allowance file with a reason per entry. The check SHALL fail on an allowance that no longer matches any difference.

#### Scenario: Stale allowance
- **WHEN** an allowance entry matches no difference
- **THEN** the check fails

### Requirement: Fixtures are never required to run the product
No runtime path of the tool SHALL read Reflection fixtures or run PHP. They SHALL be consumed only by the generator and the cross-check test.

#### Scenario: Product build
- **WHEN** the release binary is built
- **THEN** it contains the generated data and none of the fixtures
