## Purpose

Defines what testing the project requires of every change: the layers of tests and their homes, the link between specified scenarios and tests, and the rules that keep the suite hermetic, deterministic and honest, so that a passing suite means something for a security tool.

## Requirements

### Requirement: Test layers and their homes are defined
The project SHALL define four test layers and where each lives: unit tests beside the code they test; integration tests in a crate's `tests/` directory exercising only its public interface; corpus and conformance suites over the shared fixtures; and end-to-end tests that run the built binary. Documentation SHALL state, for each layer, what it is for and what it must not do.

#### Scenario: Contributor looks for the rule
- **WHEN** a contributor reads the testing documentation
- **THEN** each layer's purpose, location and prohibitions are stated

### Requirement: Every specified scenario is tested
Every scenario in a capability's specification SHALL be covered by at least one automated test, and a pull request that adds or changes a scenario SHALL add or change its test. A test that covers a specified scenario SHALL carry a marker naming the capability and requirement it covers. A scenario that cannot be tested automatically SHALL be listed with its reason in the change's tasks and verified by a recorded manual procedure.

#### Scenario: Scenario without a test
- **WHEN** the traceability report is produced
- **THEN** it lists every scenario of the main specifications that no marked test covers, grouped by capability

#### Scenario: Untestable scenario
- **WHEN** a scenario is verified manually
- **THEN** the change records the reason and the procedure, and the report lists the scenario as manual

### Requirement: Traceability is reported, not blocking
The traceability report SHALL be produced on demand and by the scheduled workflow, SHALL be machine-readable and human-readable, and SHALL NOT fail the required status. A marker naming a requirement that does not exist in the specifications SHALL be reported as stale.

#### Scenario: Stale marker
- **WHEN** a test is marked with a requirement that was removed or renamed
- **THEN** the report lists the marker as stale with the test's name

### Requirement: Tests are hermetic
Tests in the required status SHALL NOT use the network, SHALL NOT require a PHP interpreter, a container runtime or any tool outside the pinned toolchain, SHALL NOT read or write outside a temporary directory or the repository, and SHALL NOT depend on the current time, the machine's locale, the working directory, or environment variables not set by the test. A test that needs such a resource SHALL be ignored by default with a stated reason and run only in a labelled job.

#### Scenario: Test needing an interpreter
- **WHEN** a test needs the PHP interpreter
- **THEN** it is ignored by default with a reason, and a labelled job runs it

#### Scenario: Offline run
- **WHEN** the required test suite runs with no network
- **THEN** it passes

### Requirement: Tests are deterministic
A test SHALL give the same result on every run and regardless of test ordering and thread count. Randomised tests SHALL use a seed that is fixed by default, printed on failure, and settable to reproduce the failure. The scheduled workflow SHALL run the suite with one thread and with the default thread count and fail on any difference.

#### Scenario: Order dependence
- **WHEN** the suite passes with the default thread count and fails with one thread
- **THEN** the scheduled workflow fails and names the differing tests

### Requirement: Ignored tests are accounted for
Every ignored test SHALL carry a reason string. A check SHALL fail on an ignored test without a reason, and CI output SHALL state the number of ignored tests and their reasons.

#### Scenario: Silent ignore
- **WHEN** a test is marked ignored without a reason
- **THEN** the check fails and names the test

### Requirement: Security-relevant behaviour has negative tests
Code that decides whether data is tainted, sanitised, trusted or reachable, that resolves names or types, or that parses untrusted text SHALL have tests for the case where the decision is "not". A change adding such a decision SHALL include a test for the opposite outcome next to each positive test.

#### Scenario: Positive-only tests
- **WHEN** a review finds a sanitizer recognised with no test showing an unsuitable sanitizer is not accepted
- **THEN** the review checklist item for negative tests is not satisfied
