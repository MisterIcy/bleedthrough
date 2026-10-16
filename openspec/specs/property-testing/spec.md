## Purpose

Defines property-based testing in the project: where generators live, how runs are made reproducible, and which classes of code must have properties, so that invariants are checked across inputs no one thought to write.

## Requirements

### Requirement: Properties run reproducibly
Property tests SHALL use a seed that is fixed by default, SHALL print the seed and the minimised failing input on failure, and SHALL accept an environment setting to repeat a seed or raise the case count. The default case count SHALL keep the whole property suite within a documented time budget in required CI. A failing input found by a longer run SHALL be committed as a regression test.

#### Scenario: Failure is reproducible
- **WHEN** a property fails
- **THEN** the output includes the seed and a minimised input, and rerunning with that seed fails identically

#### Scenario: Regression kept
- **WHEN** a longer run finds a failing input
- **THEN** a regression test containing that input is added with the fix

### Requirement: Generators are shared and live outside product code
Generic generators (PHP names, bounded source text, relative paths, versions, arbitrary bytes) SHALL live in a single test-support location available to the test code of any crate. Generators of a crate's own types SHALL live in that crate's test code, or in test support only when used by crates that depend on it. All generators SHALL be dev-only and SHALL NOT be reachable from any product build, and test support SHALL NOT introduce a dependency cycle among workspace crates.

#### Scenario: Product build
- **WHEN** a release binary is built
- **THEN** no generator or property-testing crate is compiled into it

#### Scenario: Crate tests its own types
- **WHEN** a crate needs generators of its own types in its tests
- **THEN** it defines them in its test code without depending on test support that depends on it

### Requirement: Existing round-trip and canonical forms have properties
The following SHALL have properties: every canonical text format (built-in data, taxonomy, rule files where a canonical writer exists) parses what it writes to an equal value and writes identical bytes the second time; fingerprints are stable under irrelevant input changes and differ for relevant ones; emitters produce byte-identical output for equal reports and valid output for generated reports; severity scoring stays within its range and is monotonic in its stated inputs.

#### Scenario: Round trip
- **WHEN** a generated built-in data set is written and read
- **THEN** the value read equals the value written and a second write is byte-identical

#### Scenario: Emitter determinism
- **WHEN** a generated report is emitted twice in every format
- **THEN** the outputs are byte-identical and validate against the published schema where one exists

### Requirement: New pure logic must carry properties
A change that adds pure logic of these kinds SHALL include properties for it: normalisation or ordering of values (for example type unions: order-independent, idempotent, absorbing as specified), name or path resolution (never panics, stable for equal inputs, results within the declared universe), and parsers of untrusted text (never panic, never exceed stated limits, report errors rather than abort, on arbitrary bytes and on near-valid mutations). The review checklist SHALL include this obligation.

#### Scenario: Parser of untrusted text without a no-panic property
- **WHEN** a change adds a parser of untrusted text with no arbitrary-input property
- **THEN** the review checklist item for properties is not satisfied

### Requirement: Properties do not replace examples
Properties SHALL complement and not replace example tests of specified scenarios; each specified scenario SHALL still have an example test with a readable expected value.

#### Scenario: Scenario covered only by a property
- **WHEN** the traceability report finds a scenario covered only by a property
- **THEN** it is reported as lacking an example test
