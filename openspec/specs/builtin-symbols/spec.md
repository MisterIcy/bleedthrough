## Purpose

Defines how PHP's built-in functions, classes, interfaces, enums and constants enter the symbol table from the built-in data, so resolution and type inference know their signatures and availability for the modelled PHP version.

## Requirements

### Requirement: Built-in symbols come from the built-in data port
The symbol table SHALL obtain built-in symbols only through the built-in data source port, with origin marked as built-in. The symbols crate SHALL NOT read files, parse PHP stub files, or contain hand-written built-in declarations.

#### Scenario: In-memory data
- **WHEN** a test supplies built-in data through an in-memory source
- **THEN** the table resolves those symbols without any file access

### Requirement: Built-in types are converted by the project's type model
Declared, tentative and docblock-refined type texts SHALL be converted by the type model's parser. The effective return type SHALL be the declared type, else the tentative type; a docblock refinement SHALL be applied only by the same refinement rule as for project code. Conversion failures SHALL yield a diagnostic and the unknown type for that field.

#### Scenario: Tentative return type
- **WHEN** a built-in method has only a tentative return type
- **THEN** the table uses it as the method's return type

#### Scenario: Unparseable type text
- **WHEN** a type text cannot be converted
- **THEN** a diagnostic is produced and the field is unknown

### Requirement: Lookup is filtered by the modelled PHP version
Lookup of a built-in symbol SHALL take the modelled PHP version (8.0 to 8.5) and SHALL return found, not available in that version, or unknown. Results for symbols that exist only on some builds (guarded) or whose extension is unbundled SHALL carry that information.

#### Scenario: Version-specific function
- **WHEN** a function exists only from PHP 8.4 and the modelled version is 8.2
- **THEN** lookup reports not available in 8.2

#### Scenario: Unbundled extension
- **WHEN** a symbol belongs to an extension no longer bundled in the modelled version
- **THEN** the result is marked unbundled

### Requirement: Project code cannot shadow built-ins
A project declaration whose name collides with a built-in SHALL be reported as a diagnostic and SHALL NOT replace the built-in in lookups for that name.

#### Scenario: Collision
- **WHEN** a project declares a class with a built-in class's name
- **THEN** a diagnostic names both and lookup returns the built-in

### Requirement: Unknown built-ins are reported
Functions and classes referenced by project code that neither the project nor the built-in data declares SHALL be listed in an unknown-built-in report, and the corresponding call resolutions SHALL be unresolved with the reason function or class not found.

#### Scenario: Missing function
- **WHEN** project code calls a function that no source declares
- **THEN** it appears in the report and the call is unresolved with that reason
