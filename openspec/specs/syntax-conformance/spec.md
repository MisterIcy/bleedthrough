## Purpose

Defines the tests every parser backend must pass against the shared fixture corpus, so that adopting, upgrading or adding a backend is decided by results, and so that the parser's behaviour on PHP 7.x and 8.0 to 8.5 syntax and on broken code is known rather than assumed.

## Requirements

### Requirement: A backend-agnostic suite runs through the port
The conformance suite SHALL exercise a backend only through the parser-backend port, SHALL be runnable against any registered backend, and SHALL contain no backend-specific expectation. Its inputs SHALL be the fixtures and ledger of the PHP fixture corpus.

#### Scenario: New backend
- **WHEN** a new backend is registered with the suite
- **THEN** the full suite runs against it without modification

### Requirement: Valid fixtures parse clean
A backend SHALL parse every fixture that the corpus ledger marks valid for any supported PHP version (7.0 to 8.5) with quality clean and no unsupported or error node, and SHALL parse every legacy-removed fixture leniently, as the corpus policy records.

#### Scenario: Valid fixture regression
- **WHEN** a valid fixture yields a diagnostic or an unsupported node
- **THEN** the suite fails and names the fixture and the node

### Requirement: Malformed fixtures satisfy their declared expectations
For each malformed fixture, the suite SHALL verify that the diagnostic family is reported, that an error node covers the declared lines, and that each declared surviving declaration is present in the tree.

#### Scenario: Declaration lost
- **WHEN** a malformed fixture's declared surviving function is missing from the tree
- **THEN** the suite fails and names the fixture and the missing declaration

#### Scenario: Malformed input accepted
- **WHEN** a malformed fixture parses clean
- **THEN** the suite fails

### Requirement: Backend verdicts agree with the ledger
For each fixture, the suite SHALL compare the backend's verdict with the corpus ledger for the supported PHP versions, apart from the lenient legacy cases pinned by the corpus policy. A backend that accepts a fixture which every supported interpreter rejects SHALL fail the suite.

#### Scenario: Backend more lenient than PHP
- **WHEN** a backend accepts a fixture that the ledger marks invalid for every supported version and that is not a pinned legacy case
- **THEN** the suite fails and names the fixture

### Requirement: Golden syntax dumps
For every fixture, the suite SHALL compare a stable, backend-independent textual dump of the owned tree, including spans, against a committed golden file. Updating a golden file SHALL be an explicit, reviewable edit.

#### Scenario: Tree changes
- **WHEN** a change alters the tree for any fixture
- **THEN** the suite fails until the golden file is intentionally updated

### Requirement: Whole-file coverage and nesting invariants hold everywhere
For every fixture and every generated variant, the suite SHALL verify the model's invariants: whole-file coverage, span bounds, parent-child containment, and sibling order.

#### Scenario: Invariant violation
- **WHEN** any tree violates an invariant
- **THEN** the suite fails and reports the file, the variant and the violated invariant

### Requirement: Truncation and mutation resilience
For every valid fixture, the suite SHALL parse every prefix of the fixture in both recovery modes and SHALL parse a reproducible set of seeded mutations (byte flips, deletions, insertions, duplications). No parse SHALL panic, abort or hang, and the invariants SHALL hold. The full-length prefix SHALL parse identically to the unmodified fixture. Mutation seeds SHALL be recorded so any failure is reproducible.

#### Scenario: Prefix parse
- **WHEN** a fixture is cut at every byte position
- **THEN** each prefix returns a result satisfying the invariants

#### Scenario: Reproducing a failure
- **WHEN** a mutation test fails
- **THEN** the failure message includes the seed and the fixture, sufficient to reproduce it

### Requirement: Differential comparison between backends
When more than one backend is registered, the suite SHALL compare the owned-tree dumps of all backends for every fixture. Any difference SHALL fail the suite unless it is listed in a reviewed allowance file with a reason.

#### Scenario: Two backends disagree
- **WHEN** two registered backends produce different trees for a fixture not in the allowance file
- **THEN** the suite fails and shows the difference

### Requirement: The spike results are recorded and reviewed
The change SHALL produce a written report covering: parse results over the corpus by PHP version, behaviour on broken code including the number of dropped regions made explicit, throughput and memory of parse alone versus parse plus conversion, and the adapter's transitive dependency and licence footprint, each with the measurement method. The report SHALL state a go or no-go recommendation against criteria agreed with the project owner, and the project owner SHALL review it before dependent changes are designed on this adapter.

#### Scenario: Conversion cost
- **WHEN** the spike report is written
- **THEN** it states parse-only and parse-plus-conversion throughput on the same corpus and flags whether the provisional conversion-cost limit was exceeded

#### Scenario: Review gate
- **WHEN** the spike report is complete
- **THEN** it is presented to the project owner with a recommendation before dependent changes proceed
