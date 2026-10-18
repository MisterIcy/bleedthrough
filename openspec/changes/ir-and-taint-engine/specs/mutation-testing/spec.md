## MODIFIED Requirements

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
