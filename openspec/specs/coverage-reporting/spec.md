## Purpose

Defines how code coverage is measured and used: an advisory report that shows what the tests never reach, and per-crate floors for domain code that prevent silent erosion without turning a percentage into the goal.

## Requirements

### Requirement: Coverage is measured per crate with line and branch data
A documented command SHALL run the workspace tests under instrumentation using the pinned toolchain and SHALL report, per crate and per file, line coverage and, where the toolchain provides it, branch coverage, in a machine-readable form and as a human-readable summary. Generated code, vendored data, test code, tooling crates' examples and the `xtask` crate SHALL be excluded, with the exclusions listed in one place.

#### Scenario: Report produced
- **WHEN** the coverage command runs
- **THEN** per-crate and per-file line figures are written in both forms and the exclusion list is shown

#### Scenario: Branch data unavailable
- **WHEN** the toolchain provides no branch data
- **THEN** the report states that branch coverage is unavailable and does not report it as zero

### Requirement: Coverage is published, not gated per change
CI SHALL publish the coverage report and a per-crate comparison with the target branch on each pull request as an informational summary, and SHALL list the lines changed by the pull request that no test executes. These SHALL NOT fail the required status.

#### Scenario: New untested lines
- **WHEN** a pull request adds lines no test executes
- **THEN** the summary lists them by file and line

### Requirement: Domain crates have a floor that only rises deliberately
A committed file SHALL record a minimum line coverage for each domain crate. The check SHALL fail when a crate's coverage falls below its floor, and SHALL fail when coverage is more than a stated margin above the floor until the floor is raised, so the floor tracks reality. Changing a floor SHALL be an explicit edit with a stated reason. The floor check SHALL run in a job that reuses the instrumented run and SHALL be part of the required status only if its documented time budget is met; otherwise it SHALL run in the scheduled workflow and on demand.

#### Scenario: Erosion
- **WHEN** a change lowers a domain crate's coverage below its floor
- **THEN** the check fails and names the crate, the floor and the actual figure

#### Scenario: Stale floor
- **WHEN** a crate's coverage is more than the margin above its floor
- **THEN** the check fails until the floor is raised

### Requirement: Coverage is not a quality claim
Documentation SHALL state that coverage shows only what tests execute, not what they verify, and that mutation testing is the measure of whether tests detect faults. The project SHALL NOT use a coverage figure as a release criterion or in public claims.

#### Scenario: Documentation
- **WHEN** a contributor reads the coverage section of the testing documentation
- **THEN** it states what coverage does and does not show
