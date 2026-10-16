## Purpose

Defines the corpus of real third-party PHP projects and the generated synthetic trees that Bleedthrough is exercised against for robustness, determinism and performance, and how they are pinned, fetched and verified without being vendored into the repository.

## ADDED Requirements

### Requirement: Projects are pinned and recorded
A committed list SHALL name each real-world project with: an identifier, its repository location, an exact commit, the SHA-256 of the source archive for that commit, its licence, the PHP versions it targets, a size tier (`small`, `medium`, `large`) and the reason it was chosen. The list SHALL contain at least one project in each tier and SHALL include projects that use a framework, projects that do not, and a project with legacy procedural code. A project with an unrecorded licence SHALL NOT be listed.

#### Scenario: Missing pin
- **WHEN** a listed project has no commit or no archive hash
- **THEN** the corpus list check fails and names the project

#### Scenario: Tier coverage
- **WHEN** the list has no project in the `large` tier
- **THEN** the corpus list check fails

### Requirement: Fetching is on demand, verified and never vendored
Real-world projects SHALL be fetched only by an explicit command, into a cache directory outside version control, and SHALL be verified against the recorded archive hash before use. A hash mismatch SHALL delete nothing already verified, SHALL fail the command, and SHALL name the project and both hashes. Required CI jobs SHALL NOT fetch real-world projects. Fetched content SHALL NOT be committed, packaged or redistributed.

#### Scenario: Archive altered upstream
- **WHEN** a fetched archive does not match its recorded hash
- **THEN** the command fails naming the project, the expected hash and the actual hash, and the archive is not used

#### Scenario: Offline run
- **WHEN** the cache holds no fetched projects and the network is unavailable
- **THEN** every offline check still passes, and checks that need real-world projects report them as unavailable rather than passed

### Requirement: Parse-failure budget on real code
For each pinned project, the parser SHALL be run over every PHP file and the number of files with parse errors, and the number that fail outright, SHALL be recorded in a committed budget. A run SHALL fail when either number exceeds its budget, SHALL fail when a number drops below its budget until the budget is lowered deliberately, and SHALL list the files behind any change. Files that real PHP rejects SHALL be excluded from the budget with the interpreter's verdict recorded as the reason.

#### Scenario: Parser regression
- **WHEN** a parser change makes three more files in a project fail to parse
- **THEN** the run fails and lists those files

#### Scenario: Parser improvement
- **WHEN** a parser change makes fewer files fail than the budget records
- **THEN** the run fails until the budget is lowered

### Requirement: Robustness and determinism on real code
When a scan command exists, a scan of each fetched project SHALL complete without a crash or a panic, SHALL terminate within a documented time limit, SHALL report its coverage (files clean, recovered, failed, skipped), and SHALL produce byte-identical machine-readable reports when run twice and when run with different worker counts. A violation SHALL fail the run and name the project.

#### Scenario: Non-deterministic output
- **WHEN** two scans of the same project differ in their reports
- **THEN** the run fails and shows the first differing record

#### Scenario: Crash on real code
- **WHEN** a scan of a project exits through a panic
- **THEN** the run fails, names the project, and preserves the offending file name

### Requirement: A synthetic corpus can be generated deterministically
A generator SHALL produce a PHP source tree from a seed and size parameters (file count, average file size, class-to-function ratio, nesting depth, include density, and vulnerable-pattern density), SHALL produce byte-identical trees for identical parameters, and SHALL use only constructs the parser accepts. Generated trees SHALL NOT be committed. The generator SHALL be able to produce trees of at least 100,000 files.

#### Scenario: Reproducible tree
- **WHEN** the generator runs twice with the same seed and parameters
- **THEN** both trees are byte-identical

#### Scenario: Generated code parses
- **WHEN** a generated tree is parsed
- **THEN** no file reports a parse error

### Requirement: Synthetic trees are labelled by construction
Where the generator plants vulnerable patterns, it SHALL emit a label file listing each planted pattern's file, line range and CWE in the same form as the vulnerability corpus expectations, so the accuracy evaluator can score a scan of a large tree.

#### Scenario: Planted pattern
- **WHEN** the generator is asked for a non-zero vulnerable-pattern density
- **THEN** the label file lists every planted pattern with its location
