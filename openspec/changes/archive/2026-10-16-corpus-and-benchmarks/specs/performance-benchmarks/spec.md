## Purpose

Defines how Bleedthrough's speed is measured and protected: the stages measured, the metrics and result format, the comparison of two builds on one machine, and which measurements gate which workflows, so that "fast" is a tracked property and noise is not mistaken for regression.

## ADDED Requirements

### Requirement: Benchmarks are organised by stage
The harness SHALL measure named stages independently: parsing, and each later stage (symbol table construction, analysis) when it exists, plus an end-to-end stage that runs the `bleedthrough` binary as a user would. Adding a stage SHALL not change the result format or the harness. A stage that cannot run yet SHALL be reported as unavailable with a reason, never omitted or reported as zero.

#### Scenario: Stage not yet available
- **WHEN** the end-to-end stage is requested and the binary offers no scan command
- **THEN** the result lists the stage as unavailable with the reason

#### Scenario: New stage added later
- **WHEN** a later change registers a stage
- **THEN** it appears in results and comparisons without changing existing stages' output

### Requirement: Inputs are named and reproducible
Every measurement SHALL state its input: a synthetic tree identified by seed and parameters, or a pinned real-world project identified by its pin. Results SHALL record the input's file count, byte count and, for the parser stage, node count, so throughput can be computed and input drift is visible.

#### Scenario: Input drift
- **WHEN** two results for the same named input disagree on file count or byte count
- **THEN** their comparison is refused and explains the mismatch

### Requirement: Metrics reported per run
Each measurement SHALL report wall-clock time with the individual samples, throughput in files and bytes per second, peak resident memory where the platform exposes it, and, in a separate allocation mode, the total number of allocations, total bytes allocated and peak live bytes. Wall-clock results SHALL come from at least a configurable number of timed iterations after warm-up iterations and SHALL report median and spread, not a single sample. Where a metric is not available on the platform, it SHALL be reported as unavailable.

#### Scenario: Single sample refused
- **WHEN** a run is configured with fewer than the minimum iterations
- **THEN** the harness refuses to run and states the minimum

#### Scenario: Memory not exposed
- **WHEN** the platform does not expose peak resident memory
- **THEN** the result reports it as unavailable

### Requirement: Results are versioned machine-readable documents
Each run SHALL write one result document with a schema version, the tool version and commit, the inputs, the stages, the metrics, and a description of the environment (operating system, CPU model, logical core count, build profile). The document SHALL contain no secrets or absolute paths. Equal measurements SHALL serialise identically.

#### Scenario: Reading old results
- **WHEN** a result with an older schema version is read
- **THEN** it is accepted if the version is supported, or rejected with a message naming the version

### Requirement: Builds are compared on one machine
A comparison command SHALL take a base build and a head build, run both on the same machine alternately on the same inputs, and report per stage the difference in the median with its uncertainty. A change SHALL be reported as a regression only when it exceeds both a configured threshold and the measured noise of the base runs. A comparison between results from different environments SHALL be refused unless explicitly forced, and a forced comparison SHALL be labelled as such.

#### Scenario: Within noise
- **WHEN** head is 2% slower than base and the base runs vary by 5%
- **THEN** the comparison reports no regression

#### Scenario: Clear regression
- **WHEN** head is 20% slower than base and the base runs vary by 3%
- **THEN** the comparison reports a regression naming the stage and the input

#### Scenario: Different machines
- **WHEN** two results record different CPU models
- **THEN** the comparison is refused unless forced

### Requirement: Deterministic counters are committed and gated
Counters that do not depend on the machine (input file count, input byte count, node count, allocation count, total allocated bytes) SHALL be recorded for the synthetic inputs in a committed baseline. A change that raises an allocation counter beyond a stated tolerance, or alters an input counter, SHALL fail the offline check, and lowering a counter SHALL require the baseline to be updated by an explicit command with a reviewable diff.

#### Scenario: Allocation regression
- **WHEN** a change doubles the parser stage's allocation count on the reference input
- **THEN** the offline check fails and names the stage, the counter and both values

#### Scenario: Baseline update
- **WHEN** the baseline update command runs
- **THEN** only the baseline file changes and the diff shows each counter

### Requirement: Wall-time is never a required gate
Wall-clock and memory measurements SHALL run in a separate workflow, on a schedule and on demand, SHALL publish their result documents and a summary, and SHALL NOT be part of the required pull-request status. The workflow SHALL fail its own run on a clear regression as defined by the comparison, so regressions are visible without blocking merges on noisy hardware.

#### Scenario: Pull request
- **WHEN** a pull request runs the required checks
- **THEN** no wall-clock benchmark runs and none can fail the required status

#### Scenario: Scheduled run regresses
- **WHEN** the scheduled workflow detects a clear regression
- **THEN** that workflow run fails and its summary names the stage, the input and the difference

### Requirement: Benchmark code does not change what is measured
The harness SHALL measure the same code paths the product uses, through the same public interfaces, SHALL NOT enable special modes that the product does not, and SHALL document what each stage includes and excludes (for example whether file reading is inside the timed region).

#### Scenario: Stage description
- **WHEN** a contributor reads the benchmark documentation
- **THEN** each stage states what is inside and outside its timed region
