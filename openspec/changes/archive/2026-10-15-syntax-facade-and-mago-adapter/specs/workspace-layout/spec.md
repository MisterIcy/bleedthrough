## MODIFIED Requirements

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
