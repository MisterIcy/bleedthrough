## MODIFIED Requirements

### Requirement: One crate per concern, in four layers
The workspace SHALL organise its crates into four layers and SHALL contain one crate for each concern, each with a documented single responsibility. The domain layer SHALL contain syntax (owned AST and parser-backend port), taxonomy (shared CWE, OWASP and severity vocabulary), builtins (PHP built-in symbol data model and its source port), symbols, intermediate representation, analysis engine, rules, packs, and findings. The application layer SHALL contain the scan use cases. The infrastructure layer SHALL contain emitters, file-system access (including rule loading), and the cache store, plus parser adapters as they are added. The delivery layer SHALL contain the command-line front-end, which produces a binary named `bleedthrough`. The tooling layer SHALL contain the conformance suites, the built-in data generator and the build tasks.

#### Scenario: Workspace builds with all concern crates present
- **WHEN** a contributor runs a workspace build on a clean checkout
- **THEN** every crate compiles and the `bleedthrough` binary is produced

#### Scenario: Each crate declares its layer and responsibility
- **WHEN** a contributor opens any workspace crate
- **THEN** its top-level documentation states its layer, its responsibility, and which workspace crates it may depend on
