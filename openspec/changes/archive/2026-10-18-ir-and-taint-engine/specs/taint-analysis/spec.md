## Purpose

Defines the taint analysis semantics: what makes a value tainted, how taint moves, what removes it and for which kinds of sink, how calls are summarised across functions, and what is assumed where the analysis cannot see, so that results are explainable and the false-positive and false-negative trade-offs are stated, not accidental.

## ADDED Requirements

### Requirement: Taint originates at rule-declared sources
A value SHALL become tainted when it is produced by a source named in a taint rule: a superglobal read, a call to a source function or method, or a source constructor. The taint SHALL remember its origin (rule, location, symbol) so that a trace can start there. A superglobal element (`$_GET['id']`) and the superglobal itself SHALL both be tainted.

#### Scenario: Superglobal element
- **WHEN** `$_GET['id']` is assigned to a variable
- **THEN** the variable is tainted with that superglobal as origin

#### Scenario: Source call
- **WHEN** a rule names `Request->get` as a source and the code calls it on a resolved `Request`
- **THEN** the call result is tainted

### Requirement: Taint propagates through ordinary data flow
Taint SHALL pass through assignment, concatenation, interpolation, array construction, array element reads and writes, casts to string or array, ternary and null-coalescing results, `match` results, `list` destructuring, `foreach` over a tainted array, `yield` into a generator's consumer, by-reference arguments, `return` values, and closure captures. Arrays SHALL be treated as one value whose taint is the union of their elements' taint (element-collapsing); property taint SHALL be tracked per declaring class and property name across the project (field-insensitive on objects, flow-insensitive across methods); static properties and globals likewise.

#### Scenario: String building
- **WHEN** a tainted value is concatenated into a larger string
- **THEN** the result is tainted

#### Scenario: Taint through a property
- **WHEN** one method stores a tainted value in `$this->query` and another method passes `$this->query` to a sink
- **THEN** the flow is reported

#### Scenario: Array element
- **WHEN** a tainted value is stored in one element of an array and another element is passed to a sink
- **THEN** the sink is reported, because arrays are element-collapsing

### Requirement: Flow-sensitivity within a function
Within a function, assignment SHALL replace the previous taint of a variable on that path (strong update), and branches SHALL join by union. A variable overwritten by an untainted value before the sink SHALL NOT be reported.

#### Scenario: Overwritten variable
- **WHEN** a variable is assigned tainted data, then assigned a constant, then used in a sink
- **THEN** no finding is reported for that sink

#### Scenario: One branch tainted
- **WHEN** a variable is tainted in one branch and clean in the other and used after the join
- **THEN** it is tainted at the sink

### Requirement: Sanitisation is by sink class
A sanitizer SHALL record on the taint it processes the classes it clears and SHALL clear only those, as named by its rule. Taint that reaches a sink whose class has been cleared SHALL NOT produce a finding for that sink; taint that reaches a sink whose class has not been cleared SHALL, and if a sanitizer for another class was applied on the way, the flow SHALL record it as a considered sanitizer. A sanitizer applied to only some of the values joined into a result SHALL clear only those values.

#### Scenario: Right sanitizer
- **WHEN** a value passes through a sanitizer that clears `sql` and reaches an `sql` sink
- **THEN** no finding is reported

#### Scenario: Wrong sanitizer
- **WHEN** a value passes through a sanitizer that clears `file-path` and reaches an `sql` sink
- **THEN** a finding is reported and the trace records the considered sanitizer

#### Scenario: Partial sanitisation
- **WHEN** a sanitised value and an unsanitised tainted value are concatenated and reach the sink
- **THEN** the finding is reported with the unsanitised value as origin

### Requirement: Sinks check only vulnerable arguments
A sink SHALL report only when taint reaches an argument the rule marks vulnerable (by position or name, or the receiver), for a sink class not cleared on that taint. Taint in other arguments SHALL NOT be reported.

#### Scenario: Non-vulnerable argument
- **WHEN** taint reaches an argument that the rule does not list as vulnerable
- **THEN** no finding is reported

### Requirement: Propagators and the default call policy are defined
For a call to a symbol named as a propagator in a rule, taint SHALL flow from the listed inputs to the result. For a call to a built-in or project function with no rule entry, taint SHALL flow by the default policy: a call whose declared or inferred result type cannot carry string, array or object data (for example `int`, `float`, `bool`, `void`) yields an untainted result; otherwise taint flows from every argument and the receiver to the result. A rule's sanitizer entry SHALL take precedence over the default.

#### Scenario: Numeric result
- **WHEN** `strlen($tainted)` is used in a sink
- **THEN** the result is untainted, because its declared return type is `int`

#### Scenario: Built-in string function
- **WHEN** `strtoupper($tainted)` is used in a sink and no rule lists it
- **THEN** the result is tainted by the default policy

### Requirement: Type facts prune taint
A value whose inferred type is certainly numeric, boolean, null or an enum case, and not a string, array or arbitrary object, SHALL carry no taint. A value of unknown type SHALL keep its taint. A cast to `int`, `float` or `bool` SHALL clear taint for all classes.

#### Scenario: Integer cast
- **WHEN** a tainted value is cast to `int` and used in an `sql` sink
- **THEN** no finding is reported

#### Scenario: Unknown type keeps taint
- **WHEN** a tainted value's type is unknown
- **THEN** it remains tainted

### Requirement: Calls are summarised and composed across functions
For each unit, the analysis SHALL compute a summary of how taint through its parameters, receiver, captured and global state reaches its return value, its by-reference arguments, other state, and sinks inside it (directly or through its callees), and which sources it introduces. A call SHALL apply the summary of each of its candidate callees, joining the results. Summaries SHALL be computed callees-first over the component order; within a recursive component they SHALL be iterated until they stop changing or the iteration limit is reached.

#### Scenario: Flow through a helper
- **WHEN** a function passes `$_GET['q']` to a helper that passes its parameter to an `sql` sink
- **THEN** a finding is reported at the sink with a trace that includes the call into the helper

#### Scenario: Helper that sanitizes
- **WHEN** the helper passes its parameter through a sanitizer for the sink's class before the sink
- **THEN** no finding is reported

#### Scenario: Recursive function
- **WHEN** a recursive function returns its tainted parameter
- **THEN** the analysis terminates and the call result is tainted

### Requirement: Unresolved callees are assumed to propagate
For a call whose targets are unresolved, the analysis SHALL assume taint flows from every argument to the result and SHALL mark the resulting flow so that the finding's confidence is lowered and carries the reason that an unresolved callee was assumed to propagate. The assumption SHALL NOT be applied to calls resolved to a built-in or project function.

#### Scenario: Unknown receiver
- **WHEN** a tainted value is passed to a method on a receiver of unknown type and the result reaches a sink
- **THEN** a finding is reported with reduced confidence and the unresolved-callee reason

### Requirement: Sources inferred rather than declared are marked
Where taint arises only from an inferred origin, such as a property assigned from a source in another method, or a value returned from a function that the analysis summarised as returning a source, the flow SHALL be marked as having an inferred source for the confidence reason.

#### Scenario: Source in another method
- **WHEN** a flow's source is reached only through a property set elsewhere
- **THEN** the finding carries the inferred-source reason

### Requirement: Dynamic dispatch with several candidates lowers confidence
A flow that passes through a call with several candidate callees SHALL be marked, and the finding SHALL carry the reason that dynamic dispatch had several candidates, with the candidate count.

#### Scenario: Interface call
- **WHEN** a flow goes through an interface call with three implementors
- **THEN** the finding carries the several-candidates reason with detail `3`

### Requirement: Rules gate analysis by version and mode
A taint rule SHALL be applied only if the configured PHP version lies within its `php_min` and `php_max`, and a paranoid rule only if paranoid mode is on. A rule excluded for either reason SHALL produce no findings and SHALL NOT affect other rules.

#### Scenario: Version outside the range
- **WHEN** the configured PHP version is below a rule's `php_min`
- **THEN** that rule yields no findings

#### Scenario: Paranoid rule in normal mode
- **WHEN** paranoid mode is off
- **THEN** paranoid rules yield no findings
