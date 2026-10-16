## Purpose

Defines the local, per-function inference of expression types and call targets that the flow analysis builds on, including exactly how uncertainty is reported.

## ADDED Requirements

### Requirement: Inference is intra-procedural and uses declared callee types
Inference SHALL analyse one function, method, closure or top-level script body at a time. The types of callee return values, properties and constants SHALL come from declarations (native and docblock) in the symbol table, never from inferring the callee's body.

#### Scenario: Callee body ignored
- **WHEN** a called function has no declared return type
- **THEN** the call expression has type unknown, even if the body obviously returns a string

### Requirement: Every expression receives a type
For every expression in an analysed body, inference SHALL produce a type, using `unknown` where it cannot decide. Results SHALL be addressable by the expression's span and kind, and SHALL be deterministic.

#### Scenario: Coverage of expressions
- **WHEN** a function body is analysed
- **THEN** every expression node in it has a recorded type

### Requirement: Assignments and operators are typed
Inference SHALL type literals, variables from their most recent assignment on the path, `new` expressions, array literals (including shapes and lists), casts, string interpolation, arithmetic and concatenation, comparisons, logical operators, the null coalescing and ternary operators, `match`, and closures and arrow functions (with their captured variables), following PHP's semantics for the result types.

#### Scenario: New expression
- **WHEN** `$db = new Connection();` is analysed
- **THEN** `$db` has the object type `Connection` after the assignment

#### Scenario: Concatenation
- **WHEN** two strings are concatenated
- **THEN** the result is a string

### Requirement: Flow-sensitivity with joins and bounded loops
Variable types SHALL follow control flow: branches (`if`, `else`, `switch`, `match`, ternary, short-circuit operators) are analysed separately and joined by union; `try`/`catch`/`finally` joins SHALL be conservative; loops SHALL be analysed to a fixed point bounded by a configured iteration limit, after which the types are widened and the limit-hit is recorded.

#### Scenario: Branch join
- **WHEN** a variable is a string in one branch and null in the other
- **THEN** after the branch its type is `string|null`

#### Scenario: Loop limit
- **WHEN** a loop does not converge within the iteration limit
- **THEN** affected variables are widened and the limit-hit is recorded

### Requirement: A defined narrowing subset
Within the guarded branch, inference SHALL narrow types for: `instanceof`; comparison with `null` using `===`, `!==`, `==` and `!=`; `is_string`, `is_int`, `is_float`, `is_bool`, `is_array`, `is_object`, `is_null`, `is_callable`, `is_numeric`; `isset` and `empty` on simple variables; `===` against literals; and negation and conjunction of these. All other conditions SHALL leave types unchanged. The subset SHALL be documented.

#### Scenario: Instanceof
- **WHEN** `if ($x instanceof Foo) { ... }` guards a block
- **THEN** inside the block `$x` has type `Foo`

#### Scenario: Unrecognised condition
- **WHEN** a condition calls a user function that might validate the value
- **THEN** no narrowing is applied

### Requirement: Calls are resolved with explicit outcomes
Each call, method call, static call, `new` and first-class-callable expression SHALL receive a resolution: the list of resolved candidate declarations (more than one when the receiver type has several classes or an interface), or a dynamic outcome (variable function, variable method name, callable string), or an unresolved outcome with a reason code drawn from a documented vocabulary (unknown receiver type, class not loaded, method not found, magic method, dynamic name, limit hit). A resolution SHALL never be a silent guess.

#### Scenario: Interface receiver
- **WHEN** the receiver type is an interface with three loaded implementors
- **THEN** the resolution lists the interface method and notes the implementors as candidates

#### Scenario: Unknown receiver
- **WHEN** the receiver type is unknown
- **THEN** the resolution is unresolved with reason unknown receiver type

#### Scenario: Variable function
- **WHEN** code calls `$fn($arg)` where `$fn` is not a known closure or literal
- **THEN** the resolution is dynamic

### Requirement: Uncertainty is reported for confidence
Inference results SHALL report, per function, the counts of unknown expression types, unresolved calls and dynamic calls, and whether any limit was hit, so coverage and finding confidence can use them. Reason codes SHALL map onto the confidence-reason vocabulary of the finding model.

#### Scenario: Counts available
- **WHEN** a function contains one dynamic call and two unresolved calls
- **THEN** its result reports those counts

### Requirement: Inference terminates and is bounded
Inference SHALL terminate on any syntax tree, including trees containing error and unsupported nodes, which SHALL type as unknown. Work per function SHALL be bounded by configured limits.

#### Scenario: Error node
- **WHEN** a body contains an error node
- **THEN** inference completes and the node's type is unknown
