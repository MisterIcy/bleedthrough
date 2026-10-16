## MODIFIED Requirements

### Requirement: Continuous integration runs every gate
For every pull request and every push to the main branch, CI SHALL run build, tests, format check, lint, dependency policy, and every architecture check defined in the `workspace-layout`, `architecture-boundaries` and `domain-glossary` capabilities, and SHALL also run the offline checks of the vulnerability corpus, the accuracy evaluation and the deterministic benchmark counters, and SHALL report a single pass or fail status. A change SHALL NOT be mergeable while any of them fails. Every job SHALL use the pinned toolchain; no job SHALL require a nightly or otherwise different toolchain. Jobs that need the network to fetch third-party projects, or that depend on wall-clock timing, SHALL run in separate workflows outside the required status.

#### Scenario: All checks pass
- **WHEN** a change satisfies every gate
- **THEN** CI reports success

#### Scenario: One gate fails
- **WHEN** any single gate fails
- **THEN** the overall CI status is failure and identifies the failing gate

#### Scenario: Accuracy regression in a pull request
- **WHEN** a change makes the accuracy evaluation fail
- **THEN** the required status is failure and names the evaluation as the failing gate

#### Scenario: Networked or timing jobs
- **WHEN** the real-world corpus or wall-time benchmarks fail in their own workflow
- **THEN** the required pull-request status is unaffected
