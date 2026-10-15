## Purpose

Defines what the Mago adapter must guarantee: that the third-party parser is contained, exactly pinned and audited, and that its output is converted into the owned syntax model faithfully, with every source region it dropped or failed to parse made explicit.

## ADDED Requirements

### Requirement: The adapter is the only consumer of Mago
Only the adapter crate SHALL depend on any Mago crate, directly or by declaration. Every direct Mago dependency of the adapter SHALL be pinned to an exact version, the lock file SHALL be committed, and builds and CI SHALL run with the lock file enforced. The adapter SHALL be declared as an adapter in the architecture declaration.

#### Scenario: Another crate depends on Mago
- **WHEN** a crate other than the adapter declares a Mago dependency
- **THEN** the policy check fails

#### Scenario: Floating version
- **WHEN** a direct Mago dependency of the adapter uses a version range
- **THEN** the policy check fails

#### Scenario: Transitive Mago crate escapes the adapter
- **WHEN** a Mago crate that is only a transitive dependency becomes reachable from a crate other than the adapter
- **THEN** the dependency check fails

### Requirement: The dependency footprint is audited
The full transitive dependency closure of the adapter SHALL pass the project's licence allow-list and advisory checks. The set of crates and licences it introduces SHALL be recorded in the spike report, and any crate with an advisory or an unapproved licence SHALL be reported rather than ignored.

#### Scenario: New transitive licence
- **WHEN** a transitive dependency of the adapter has a licence outside the allow-list
- **THEN** the dependency check fails and names the crate

### Requirement: Conversion is faithful and exhaustive
For the pinned Mago version, every statement and expression kind SHALL be converted to an owned node or to an explicit unsupported node. The conversion SHALL be written so that a pinned-version upgrade that introduces a new kind fails the build or the tests rather than being ignored.

#### Scenario: Mago adds a construct
- **WHEN** the pinned Mago version adds a new statement or expression kind
- **THEN** the adapter fails to build or its tests fail until the kind is handled

#### Scenario: Equivalent meaning
- **WHEN** a valid construct is converted
- **THEN** the owned node has the same meaning, operands and order as the source, and the same span

### Requirement: Regions dropped by the parser become error nodes
Where Mago records a parse error and omits the failing statement, the adapter SHALL insert an error node covering every non-trivia source region not covered by a converted statement, linked to the diagnostic that explains it. A class, function or other declaration containing a syntax error SHALL therefore appear in the tree as an error node, never as nothing.

#### Scenario: One bad method in a class
- **WHEN** a class contains a method with a syntax error and Mago drops the whole class
- **THEN** the tree contains an error node spanning the class and a diagnostic locating the error

#### Scenario: Valid statements around an error
- **WHEN** valid statements precede and follow a broken one
- **THEN** both valid statements are present in the tree, and the broken region is an error node between them

### Requirement: Mago errors map to project diagnostics
Each Mago parse error SHALL be mapped to a diagnostic with a stable project code, a message and a span. No Mago error type or message format SHALL be exposed.

#### Scenario: Unexpected token
- **WHEN** Mago reports an unexpected token
- **THEN** the diagnostic has the unexpected-token code and the token's span

### Requirement: Recovery modes map to Mago settings
Strict mode SHALL return only what Mago produces for complete statements, with gaps filled as error nodes. Editor mode SHALL additionally keep unfinished bodies and lists at the end of the file, as Mago's incomplete-input recovery allows, while still reporting the missing delimiters.

#### Scenario: Truncated file in editor mode
- **WHEN** a file is cut off inside a method body and editor mode is used
- **THEN** the class and method appear in the tree and diagnostics report the missing delimiters

### Requirement: The adapter contains backend failures
The adapter SHALL convert a panic or a recursion-limit error from Mago into a failed or recovered result with the backend-failure or recursion-limit diagnostic. It SHALL NOT let a panic from the backend cross into the caller.

#### Scenario: Backend panics
- **WHEN** Mago panics while parsing a file
- **THEN** the adapter returns a failed result with a backend-failure diagnostic and the process continues

### Requirement: The adapter reports its identity
The adapter SHALL report its backend name and the exact pinned Mago version, matching the lock file.

#### Scenario: Identity matches the pin
- **WHEN** the adapter reports its identity
- **THEN** the version equals the exact version in the manifest and lock file
