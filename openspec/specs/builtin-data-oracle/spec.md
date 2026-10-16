## Purpose

Defines the Reflection fixtures taken from real PHP interpreters, which complete the data where the stubs are silent and serve as an independent check that the generated data is correct.

## Requirements

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
For every version, the generated data SHALL take from the fixture the classes, interfaces, traits, enums, global constants, class constants and properties that the stubs of that version do not declare (the stubs of PHP 8.0 and 8.1 declare almost no constants, and every release registers some symbols in C only), and constant values SHALL come from the fixtures where available, except for constants whose value depends on the build or the image (listed and reasoned in the generator). Every record or field derived this way SHALL be marked as Reflection-derived.

#### Scenario: 8.0 constant
- **WHEN** an 8.0 global constant is absent from the stubs
- **THEN** it appears in the data from the fixture, marked as Reflection-derived

#### Scenario: Class the stubs lack
- **WHEN** a class the fixture reports (such as `stdClass` in 8.0) has no declaration in the stubs of that version
- **THEN** it appears in the data from the fixture, marked as Reflection-derived

#### Scenario: Build-specific value
- **WHEN** a constant's value depends on the build (such as `PHP_OS` or `PHP_BINARY`)
- **THEN** the data records the constant without a value

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
