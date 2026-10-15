## Purpose

Defines the corpus of PHP source files, valid and malformed, organised by language version, together with the per-version verdict ledger that is checked against real PHP interpreters, so that any parser (the Mago adapter, a future backend, or a parser we write ourselves) can be judged against the same independent evidence and regressions are caught regardless of which parser is in use.

## Requirements

### Requirement: The corpus is parser-independent and has one home
The fixtures SHALL live in a single directory tree at the repository root, SHALL contain no expectation specific to any parser backend, and SHALL be consumable by any test through plain file reading. Fixtures for syntax checking SHALL be kept apart from any later fixtures for vulnerability analysis.

#### Scenario: New backend
- **WHEN** a new parser backend is added
- **THEN** it is tested against the existing corpus without changing any fixture

### Requirement: Valid fixtures cover each PHP version from 7.0 to 8.5
For each PHP version from 7.0 through 8.5, the corpus SHALL contain valid fixtures for the syntax that version introduced. A manifest SHALL list each feature with the PHP documentation page that describes it, and SHALL map each feature to at least one fixture. A feature listed without a fixture, or a fixture mapped to no feature, SHALL fail a completeness check.

#### Scenario: Missing fixture
- **WHEN** the manifest lists a feature for which no fixture exists
- **THEN** the completeness check fails and names the feature

#### Scenario: Orphan fixture
- **WHEN** a fixture file exists that the manifest does not list
- **THEN** the completeness check fails and names the file

### Requirement: Fixture groups
The corpus SHALL contain four groups: valid fixtures for syntax introduced per version; legacy-removed fixtures for syntax valid in some 7.x version and rejected by PHP 8.x (for example the `(unset)` cast and brace-style string or array offsets); malformed fixtures; and realistic fixtures that resemble real application files (a controller, a repository, an inline-HTML template, a legacy procedural script).

#### Scenario: Group membership
- **WHEN** the manifest is read
- **THEN** every fixture is assigned to exactly one of the four groups

### Requirement: Malformed fixtures cover classes of breakage
The malformed group SHALL include at least: truncated files; unbalanced braces, brackets and parentheses; unterminated strings, comments and heredocs; invalid tokens; misplaced keywords; broken mixing of inline HTML and PHP; invalid hook, attribute or type syntax; extreme nesting depth; NUL bytes; and invalid UTF-8.

#### Scenario: Each class represented
- **WHEN** the manifest is read
- **THEN** each listed class of breakage has at least one malformed fixture

### Requirement: Malformed expectations are backend-neutral
Each malformed fixture SHALL declare, without reference to any parser's messages: the diagnostic code family that must be reported, the line range that the error region must cover, and the names of valid declarations before and after the breakage that must still be present in the resulting tree. It SHALL NOT declare exact error messages or exact recovery shapes.

#### Scenario: Declarations around the break survive
- **WHEN** a malformed fixture declares that functions `ok_before` and `ok_after` must be present
- **THEN** a parser passes only if both appear in the tree and an error node covers the declared lines

### Requirement: A per-version verdict ledger is committed
The corpus SHALL include a machine-readable ledger recording, for each fixture and each PHP version from 7.0 to 8.5, whether the real interpreter accepts the fixture's syntax (valid or invalid), together with the interpreter's version and the date the verdict was recorded.

#### Scenario: Legacy fixture
- **WHEN** a legacy-removed fixture is recorded
- **THEN** the ledger marks it valid for the 7.x versions that accept it and invalid for 8.x versions that reject it

### Requirement: The ledger is verified against real interpreters
A CI job SHALL run the interpreter's syntax check for each PHP version in the ledger, using the official container image for that version, and SHALL fail if any verdict disagrees with the ledger. A version for which no interpreter can be obtained SHALL be listed as unverified with a reason, SHALL be visible in the job output, and SHALL NOT be reported as verified. Local runs without the interpreter for a version SHALL use the ledger and report that version's verdicts as unverified.

#### Scenario: Ledger drift
- **WHEN** an interpreter's verdict for a fixture differs from the ledger
- **THEN** the job fails and names the fixture, the version, and both verdicts

#### Scenario: Interpreter unavailable
- **WHEN** no interpreter image exists for a listed version
- **THEN** the job output lists that version as unverified with the reason, and the ledger entries for it are not described as verified

### Requirement: Backend policy for legacy syntax is pinned
For each legacy-removed fixture, the corpus SHALL record the policy that parser backends follow: accept leniently (the project's policy), with the tree they are expected to produce pinned by the golden dump. A backend that rejects a fixture marked as accepted, or accepts one marked as rejected, SHALL fail the conformance suite. The ledger's 8.x verdict SHALL NOT, by itself, make a lenient acceptance a failure.

#### Scenario: Lenient acceptance
- **WHEN** a backend accepts the `(unset)` cast fixture, as the policy requires
- **THEN** the suite passes despite the 8.x interpreter rejecting it

### Requirement: Fixtures are original and their provenance is recorded
Every fixture SHALL be written for this project or derived from material whose licence is on the project's licence allow-list, and the manifest SHALL record each fixture's provenance and licence. Content copied from a source with an unrecorded or disallowed licence SHALL NOT be added.

#### Scenario: Third-party fixture
- **WHEN** a fixture is derived from an external source
- **THEN** the manifest records the source and an allowed licence, or the fixture is rejected by the manifest check

### Requirement: Fixtures are small and focused
Each valid or malformed fixture SHALL exercise one feature or one kind of breakage and SHALL be at most 200 lines. Realistic fixtures MAY be larger, but each SHALL have a stated purpose in the manifest.

#### Scenario: Oversized fixture
- **WHEN** a non-realistic fixture exceeds the line limit
- **THEN** the manifest check fails

### Requirement: Corpus changes are deliberate and documented
Adding, changing or removing a fixture SHALL be accompanied in the same change by the manifest and ledger updates. The repository SHALL document how to add a fixture and how to record its verdicts.

#### Scenario: Fixture without ledger entry
- **WHEN** a fixture is added with no ledger entry
- **THEN** the completeness check fails
