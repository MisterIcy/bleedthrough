## Purpose

Defines the bounds on the work the engine does, what it does when a bound is reached, and the guarantee that adversarial or enormous input yields a bounded, reported and conservative result rather than a hang, a crash or silent incompleteness.

## ADDED Requirements

### Requirement: Every unbounded dimension has a limit
The engine SHALL bound: the number of blocks and instructions per unit; fixpoint iterations per unit and per recursive component; the number of distinct taint origins held by one value; the length of a trace; dispatch candidates per call; the depth of summary application; the number of units in a component analysed together; and total work per unit as a step budget. Defaults SHALL be documented, and every limit SHALL be configurable.

#### Scenario: Limits documented
- **WHEN** a contributor reads the engine documentation
- **THEN** every limit is listed with its default and the effect of reaching it

### Requirement: Reaching a limit widens conservatively and is reported
When a limit is reached, the analysis SHALL widen the affected result towards the conservative side defined for it (the origin set of a value is truncated in a deterministic order and the loss flagged; a fixpoint stops and treats unstable values as tainted by all their known origins), SHALL count the limit hit by kind with the unit's location, and SHALL mark any finding produced after the widening with the widening reason.

#### Scenario: Origin set exceeds the cap
- **WHEN** a value accumulates more distinct origins than the cap
- **THEN** origins beyond the cap are dropped in a deterministic order, the limit hit is counted, and findings from the value carry the widening reason

#### Scenario: Fixpoint does not converge
- **WHEN** a recursive component does not stabilise within the iteration limit
- **THEN** the analysis stops, uses the latest summaries, counts the limit hit and marks the dependent findings

### Requirement: Widening never hides a reachable sink silently
After any widening, the analysis SHALL NOT drop taint from a value in a way that removes a finding that the unwidened analysis would have reported without also reporting the limit hit for that unit. An analysis that skips a unit entirely SHALL report it as skipped.

#### Scenario: Unit over its step budget
- **WHEN** a unit exceeds its step budget
- **THEN** its remaining analysis is skipped or widened, the unit is reported in the coverage with the limit kind, and the run continues

### Requirement: Adversarial input terminates within bounded resources
The engine SHALL terminate and SHALL NOT overflow the stack or exhaust memory on: deeply nested expressions and statements, very long concatenation chains, very large arrays and match expressions, functions with thousands of blocks, long chains of assignments, many mutually recursive functions, and deep or wide class hierarchies. Algorithms over graphs and trees SHALL use explicit work lists or bounded recursion.

#### Scenario: Deeply nested expression
- **WHEN** a unit contains an expression nested one thousand levels deep
- **THEN** lowering and analysis complete without a stack overflow, widening or reporting a limit if the depth bound is reached

#### Scenario: Large mutually recursive set
- **WHEN** one thousand functions call each other in a cycle
- **THEN** analysis terminates within the iteration and component limits

### Requirement: Limit behaviour is tested
Each limit SHALL have a test that reaches it and asserts the widening behaviour, the counter, and the finding reason.

#### Scenario: Each limit
- **WHEN** the limit tests run
- **THEN** every limit kind listed in the engine documentation is reached by at least one test
