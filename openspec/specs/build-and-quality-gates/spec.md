## Purpose

Defines the toolchain, licensing, lint, dependency-policy and CI requirements that keep Bleedthrough buildable, legally clean and reviewable, in line with the project priority of being safe, sound and trustworthy over fast to ship.

## Requirements

### Requirement: Exactly pinned toolchain and single workspace MSRV
The repository SHALL pin the Rust toolchain to the exact version 1.99.0 so that a contributor with a standard rustup installation uses the same compiler as CI. The workspace SHALL declare a single minimum supported Rust version of 1.99 and edition 2024, inherited by every crate. Changing the pin SHALL be an explicit, reviewable edit.

#### Scenario: Contributor with another default toolchain
- **WHEN** a contributor with a different default toolchain runs a build in the repository
- **THEN** rustup selects 1.99.0, or the build fails with a message naming the required version

#### Scenario: Crate overriding the MSRV
- **WHEN** a workspace crate declares its own `rust-version` differing from the workspace value
- **THEN** the policy check fails

### Requirement: Project licensing
The project SHALL be licensed under `MIT OR Apache-2.0`. The repository SHALL contain both license texts, and every workspace crate SHALL declare that license expression through workspace-inherited metadata.

#### Scenario: Crate metadata
- **WHEN** package metadata is listed for any workspace crate
- **THEN** its license is `MIT OR Apache-2.0`

### Requirement: Crates are not published
Every workspace crate SHALL be marked non-publishable, so that no crate can be released to a package registry by accident. The project SHALL be distributed as built binaries and container images, defined by a separate change.

#### Scenario: Publish attempt
- **WHEN** a contributor attempts to publish any workspace crate to a registry
- **THEN** the publish is refused because the crate is marked non-publishable

### Requirement: Dependency license and advisory policy
Dependencies SHALL be limited to an explicit allow-list of licenses compatible with distributing under `MIT OR Apache-2.0`. CI SHALL fail on a dependency with a disallowed or missing license, a known security advisory, or an unapproved registry or source.

#### Scenario: Disallowed license introduced
- **WHEN** a pull request adds a dependency whose license is not on the allow-list
- **THEN** the dependency check fails and names the crate and license

#### Scenario: Advisory present
- **WHEN** a dependency has a published security advisory
- **THEN** the dependency check fails unless the advisory is explicitly ignored with a recorded justification

### Requirement: Parser backends are pinned
Any third-party PHP parser dependency SHALL be pinned to an exact version, and any version change SHALL be an explicit, reviewable edit.

#### Scenario: Floating parser version
- **WHEN** a parser-backend dependency is declared with a version range rather than an exact version
- **THEN** the policy check fails

### Requirement: Unsafe code is forbidden by default
Workspace crates SHALL forbid `unsafe` code. A crate that needs it SHALL opt out explicitly in its own manifest with a justification comment, visible in review.

#### Scenario: Unsafe block without opt-out
- **WHEN** a crate that has not opted out contains an `unsafe` block
- **THEN** the build fails

### Requirement: Formatting and lint gates
CI SHALL fail on unformatted code and on any lint warning from the standard linter at the project's configured lint levels.

#### Scenario: Warning in a pull request
- **WHEN** a change introduces a linter warning
- **THEN** the CI lint job fails

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
