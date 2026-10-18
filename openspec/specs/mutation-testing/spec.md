## Purpose

Defines mutation testing of the domain crates: what is mutated, when it runs, and how surviving mutants are recorded and ratcheted, so that the suite is measured by whether it detects broken logic.

## Requirements

### Requirement: Domain logic is mutation tested
A documented command SHALL apply code mutations to each domain crate and run that crate's tests, reporting per crate and per file the numbers of mutants caught, missed, timed out and unviable, and the resulting score. Mutants SHALL be generated only from product code, not from test code, generated code or vendored data. The command SHALL be runnable for a single crate and for the changed files of a pull request.

#### Scenario: Missed mutant
- **WHEN** a mutation that changes a comparison in a domain function is not detected by any test
- **THEN** the report lists the mutant with file, line and the change made

#### Scenario: Single crate
- **WHEN** the command is run for one crate
- **THEN** only that crate is mutated and its tests run

### Requirement: Mutation testing never blocks merges
Mutation testing SHALL run on demand, SHALL publish its report, and SHALL NOT be part of the required status. A full run is too slow for routine CI, so it SHALL be dispatched once on the release commit before a release, and the release checklist SHALL require that run to pass. The on-demand workflow SHALL fail when the ratchet below is violated, and a contributor MAY run it locally on the changed files.

#### Scenario: Pull request
- **WHEN** a pull request runs the required checks
- **THEN** no mutation run is part of them

#### Scenario: Release
- **WHEN** a release is prepared
- **THEN** the release checklist requires the mutation workflow to have been dispatched on the release commit and to have passed

### Requirement: Surviving mutants are a ledger with reasons
A committed ledger SHALL list every tolerated surviving mutant by crate, file and a stable description, each with a reason (for example an equivalent mutant, or code reachable only through a limit that is tested elsewhere). A surviving mutant absent from the ledger SHALL fail the on-demand run. A ledger entry whose mutant is now caught or no longer exists SHALL fail the run until removed, so the ledger cannot accumulate stale entries.

#### Scenario: New survivor
- **WHEN** a change introduces logic whose mutation no test detects and the ledger has no entry
- **THEN** the on-demand run fails and names the mutant

#### Scenario: Stale entry
- **WHEN** a ledger entry's mutant is now detected
- **THEN** the run fails until the entry is removed

### Requirement: Every ledger entry is analysed
Each entry in the survivor ledger SHALL state a specific reason that explains why its mutant is tolerated, and the ledger SHALL NOT admit entries marked as unanalysed or as recorded debt. The ledger loader SHALL refuse such an entry, in any file, and the refusal SHALL name the entry. A reason that only restates that the mutant survived, or that it has not been analysed, SHALL be refused.

#### Scenario: Unanalysed entry
- **WHEN** the ledger contains an entry marked as recorded baseline debt
- **THEN** loading the ledger fails and names the entry

#### Scenario: Reason restates the survival
- **WHEN** an entry's reason says only that the mutant survives or has not been analysed
- **THEN** loading the ledger fails and names the entry

#### Scenario: Reasoned entry
- **WHEN** an entry in non-critical code gives a specific reason, such as a limit tested elsewhere or an equivalent mutant
- **THEN** the ledger accepts it

### Requirement: Security-critical code is held to a stricter standard
For code that decides taint, sanitisation, severity or confidence, name and type resolution, and the handling of limits, the ledger SHALL admit only equivalent mutants. Any survivor in these areas SHALL be fixed with a test, not tolerated, unless the mutant is proven equivalent in the entry's reason.

#### Scenario: Survivor in a sanitiser decision
- **WHEN** a mutant in sanitiser matching survives and is not equivalent
- **THEN** the ledger refuses the entry and the run fails

### Requirement: Runs are bounded and reproducible
Mutation runs SHALL use the pinned toolchain, a documented per-mutant timeout, and a fixed mutant order, so that two runs on the same commit give the same report. Reports SHALL contain no absolute paths or timestamps in the compared fields.

#### Scenario: Repeat run
- **WHEN** the run is repeated on the same commit
- **THEN** the sets of caught and missed mutants are identical
