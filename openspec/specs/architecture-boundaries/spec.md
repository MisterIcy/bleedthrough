## Purpose

Defines the architectural qualities the project enforces in CI (domain purity, encapsulation, stable public surfaces, bounded size and complexity) and states honestly which design principles are verified by tools and which by human review, so that passing CI is never mistaken for proof of good design.

## Requirements

### Requirement: Domain crates are pure
Domain crates SHALL NOT perform or link to input/output: no file-system, network, process, environment or wall-clock access, and no dependency on crates whose purpose is such access or on serialization-format or async-runtime crates. Data-contract derive support is permitted. Anything impure SHALL sit behind a port implemented in the infrastructure layer.

#### Scenario: Domain code reads a file
- **WHEN** a domain crate uses a disallowed file-system, network, process, environment or clock API
- **THEN** the lint stage fails and names the item and crate

#### Scenario: Domain manifest adds an infrastructure dependency
- **WHEN** a domain crate manifest adds a crate on the infrastructure deny-list
- **THEN** the dependency check fails

### Requirement: Public surface of inner layers is explicit and stable
The public API of every domain and application crate SHALL be recorded in a committed snapshot, and CI SHALL fail when the actual API differs from the snapshot. Updating a snapshot SHALL be an explicit, reviewable edit. Items not intended for other crates SHALL NOT be public.

The check SHALL run on the pinned stable toolchain. Because it derives the surface from source, items it cannot resolve (such as macro-generated items or complex glob re-exports) SHALL be documented as a known limit of the check, and the review checklist SHALL cover them.

#### Scenario: Accidental public item
- **WHEN** a change makes a previously private item public in a domain crate without updating the snapshot
- **THEN** the API check fails and shows the difference

#### Scenario: Intentional API change
- **WHEN** a change updates the code and the snapshot together
- **THEN** the API check passes and the snapshot change is visible in review

### Requirement: Extensible domain types are non-exhaustive
Public enumerations and structures in domain and application crates SHALL be marked non-exhaustive unless the crate documents them as intentionally closed. CI SHALL flag unmarked ones.

#### Scenario: Closed public enum without justification
- **WHEN** a public enum in a domain crate is neither non-exhaustive nor documented as closed
- **THEN** the lint stage fails

### Requirement: Size and complexity limits
CI SHALL enforce configured limits on function length, cognitive complexity and argument count, failing on violations. Exceptions SHALL be local, explicit, and justified in a comment.

#### Scenario: Oversized function
- **WHEN** a function exceeds a configured limit without a justified exception
- **THEN** the lint stage fails

### Requirement: Verified and review-only principles are stated
The repository SHALL document, for each SOLID principle and each DDD boundary rule, whether it is machine-verified (and by which CI check) or review-verified. Principles that cannot be machine-verified, including single responsibility, interface segregation, Liskov substitution and open/closed design quality, SHALL be covered by an architecture review checklist that the pull-request template references.

#### Scenario: Reading the guarantees
- **WHEN** a contributor reads the architecture documentation
- **THEN** it lists each principle with its verification method, and states that passing CI does not demonstrate review-only principles

#### Scenario: Pull request review
- **WHEN** a pull request is opened
- **THEN** its template presents the architecture review checklist

### Requirement: Architecture checks are blocking
The layering, purity, public-API, extensibility, size and glossary checks SHALL be required CI checks. A change SHALL NOT be mergeable while any of them fails.

#### Scenario: Architecture check fails
- **WHEN** any architecture check fails on a pull request
- **THEN** the overall CI status is failure and identifies the failing check
