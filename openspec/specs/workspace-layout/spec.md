## Purpose

Defines the layered crate set of the Bleedthrough workspace, the responsibility of each crate, who owns the ports between layers, and the permitted dependency directions, so that architectural boundaries (pure domain, swappable infrastructure, thin front-ends) are enforced mechanically rather than by convention.

## Requirements

### Requirement: One crate per concern, in four layers
The workspace SHALL organise its crates into four layers and SHALL contain one crate for each concern, each with a documented single responsibility. The domain layer SHALL contain syntax (owned AST and parser-backend port), symbols, intermediate representation, analysis engine, rules, packs, and findings. The application layer SHALL contain the scan use cases. The infrastructure layer SHALL contain emitters, file-system access, and the cache store, plus parser adapters as they are added. The delivery layer SHALL contain the command-line front-end, which produces a binary named `bleedthrough`.

#### Scenario: Workspace builds with all concern crates present
- **WHEN** a contributor runs a workspace build on a clean checkout
- **THEN** every crate compiles and the `bleedthrough` binary is produced

#### Scenario: Each crate declares its layer and responsibility
- **WHEN** a contributor opens any workspace crate
- **THEN** its top-level documentation states its layer, its responsibility, and which workspace crates it may depend on

### Requirement: Dependencies point inward
Domain crates SHALL NOT depend on application, infrastructure, delivery or tooling crates. Application crates SHALL depend only on domain crates. Infrastructure crates SHALL depend only on application and domain crates, and SHALL NOT depend on each other except where declared. Delivery crates MAY depend on any layer. Tooling crates (conformance suites and build tasks) MAY depend on domain crates and on nothing else among the workspace crates, and no non-tooling crate SHALL depend on a tooling crate except through a dev-dependency, which exists only to run that tooling's checks against the crate. No dependency cycle SHALL exist.

#### Scenario: Domain depends outward
- **WHEN** a domain crate declares a dependency on an application, infrastructure, delivery or tooling crate
- **THEN** the layering check fails and names both crates

#### Scenario: Infrastructure crate depends on another infrastructure crate without declaration
- **WHEN** an infrastructure crate depends on a different infrastructure crate not permitted by the layering declaration
- **THEN** the layering check fails

#### Scenario: Adapter runs the conformance suite as a dev-dependency
- **WHEN** an infrastructure adapter declares a tooling crate as a dev-dependency only
- **THEN** the layering check succeeds

#### Scenario: Normal dependency on a tooling crate
- **WHEN** a non-tooling crate declares a tooling crate as a normal dependency
- **THEN** the layering check fails and names both crates

#### Scenario: Tooling depends on a non-domain crate
- **WHEN** a tooling crate declares a dependency on an application, infrastructure or delivery crate
- **THEN** the layering check fails and names both crates

### Requirement: Ports are owned by the inner layer
Interfaces through which the system reaches the outside world SHALL be defined in the domain or application layer and implemented in the infrastructure layer. The parser-backend port SHALL be owned by the syntax crate; ports for sources, caching, lock-file reading and report emission SHALL be owned by the application crate. Only the delivery layer SHALL choose and wire concrete implementations.

#### Scenario: Swapping an implementation
- **WHEN** a different implementation of a port (for example a different parser adapter or emitter) is wired in the command-line front-end
- **THEN** no source change is required in any domain or application crate

#### Scenario: Application selects a concrete adapter
- **WHEN** an application or domain crate depends on a concrete infrastructure implementation
- **THEN** the layering check fails

### Requirement: Engine is a library; front-ends are thin
The analysis engine and every crate below it SHALL be usable as a library without any delivery crate. Front-ends (the CLI now, an LSP later) SHALL contain wiring and presentation only, and SHALL NOT be depended on by any other crate.

#### Scenario: Engine builds without front-ends
- **WHEN** only the engine crate and its dependencies are built
- **THEN** the build succeeds without compiling the command-line crate

### Requirement: Parser backends are isolated in adapter crates
No crate other than a dedicated parser-adapter crate SHALL depend on a third-party PHP parser. Adapter crates SHALL depend on the syntax crate and implement its backend port; the syntax crate and all other crates SHALL NOT depend on any adapter except the delivery layer that selects one.

#### Scenario: Non-adapter crate depends on a parser backend
- **WHEN** a crate that is not an adapter lists a third-party PHP parser as a dependency
- **THEN** the dependency check fails

#### Scenario: Swapping the backend
- **WHEN** a different adapter crate is selected in the command-line front-end
- **THEN** no source change is required in the syntax, symbols, IR, engine, rules, packs, findings, application or other infrastructure crates

### Requirement: Layering is declared in one place and verified in CI
The layer of each crate and the permitted dependency edges SHALL be declared in one machine-readable file and verified by an automated check that runs in CI. A dependency not permitted by the declaration, a crate missing from the declaration, and any cycle SHALL fail the check.

#### Scenario: Permitted graph passes
- **WHEN** the workspace dependency graph matches the declaration
- **THEN** the check succeeds

#### Scenario: Unlisted crate
- **WHEN** a new workspace crate is added without a declaration entry
- **THEN** the check fails and names the crate

### Requirement: Reserved locations for extension content
The repository SHALL reserve documented directories for first-party packs, built-in rule files, and additional parser adapters, without functional content in this change. An LSP front-end crate SHALL be documented as reserved and not created.

#### Scenario: Reserved directories are documented
- **WHEN** a contributor reads the repository layout documentation
- **THEN** it identifies where packs, rule files and parser adapters are added and states that the LSP crate is not yet present
