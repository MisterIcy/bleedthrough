## Purpose

Defines the intermediate representation the analysis runs on: a control-flow graph per function-like unit, lowered from the owned syntax tree, total over the syntax model and deterministic, so that every later analysis sees all PHP the parser accepts in one simple, uniform form.

## Requirements

### Requirement: One control-flow graph per unit
Lowering SHALL produce one graph for each function, method, closure, arrow function and property hook body, and one for the top-level code of each file. A graph SHALL consist of basic blocks of simple instructions with explicit edges, one entry block and one exit, and SHALL record its owning unit, file and span.

#### Scenario: File with a class and top-level code
- **WHEN** a file declares a class with two methods and contains top-level statements
- **THEN** lowering yields three graphs for the methods and the top-level code, plus one for each closure or arrow function in them

#### Scenario: Nested closure
- **WHEN** a method contains a closure
- **THEN** the closure is a separate graph linked to its creation point, with its captured variables recorded

### Requirement: Control flow is modelled for every PHP construct
The graph SHALL model sequencing, conditionals, `switch`, `match`, all loop forms with `break` and `continue` levels, short-circuit operators and the null-coalescing and nullsafe forms, `try`/`catch`/`finally` including `return` through `finally`, `goto` and labels, `yield` and `yield from` as suspension points, `throw`, `exit`, and early `return`. A path that cannot be modelled precisely SHALL be over-approximated by additional edges, never by dropping edges.

#### Scenario: Return inside try with finally
- **WHEN** a function returns inside a `try` block that has a `finally`
- **THEN** the graph routes the return through the `finally` block before the exit

#### Scenario: Short-circuit operand
- **WHEN** the right operand of `&&` contains a call
- **THEN** the call lies in a block reached only on the path where the left operand allowed evaluation

### Requirement: Lowering is total over the syntax model
Lowering SHALL accept every tree the syntax model can represent, including trees with error nodes and unsupported kinds, and SHALL NOT fail or panic. A node it cannot model SHALL become an explicit unknown instruction that takes the node's operands and yields an unknown value, and SHALL be counted per kind. An error node SHALL mark the graph as recovered.

#### Scenario: Unsupported expression kind
- **WHEN** an expression kind that lowering does not model appears
- **THEN** an unknown instruction is emitted with the operands it could see, and the count for that kind increases

#### Scenario: File with parse errors
- **WHEN** a tree contains an error node
- **THEN** lowering completes, the affected graph is marked recovered, and the remaining graphs are unaffected

### Requirement: Instructions carry what analysis needs
Instructions SHALL cover value construction, assignment, reads and writes of locals, properties, static properties, array elements and globals, string building and interpolation, casts, binary and unary operations, calls of every form (function, method, static, constructor, callable value), `include` forms, `eval`, `print` and `echo`, and returns. Every instruction SHALL carry the span of the source it came from. Every call instruction SHALL carry the call resolution computed by type inference for that expression, or an explicit marker that none exists.

#### Scenario: Call carries its resolution
- **WHEN** a method call was resolved to one target by type inference
- **THEN** the lowered call instruction records that target and the reasons attached to the resolution

#### Scenario: Spans point to source
- **WHEN** any instruction is reported in a trace
- **THEN** its span refers to the source text that produced it

### Requirement: Interpolation and concatenation are explicit
Concatenation and interpolated strings (including heredocs) SHALL be lowered to an operation whose result depends on all of its operands; function-style formatting such as `sprintf` is an ordinary call. Each part of an interpolated string SHALL be a distinct operand with its own span.

#### Scenario: Interpolated string
- **WHEN** a double-quoted string interpolates two variables
- **THEN** the lowered operation has both variables as operands with separate spans

### Requirement: Lowering is deterministic and independent of other files
Lowering a file SHALL depend only on that file's tree and the inference facts for it, SHALL give identical output for identical input, and SHALL number blocks and values in source order so that a graph's textual dump is stable. A textual dump of a graph SHALL exist for golden tests and diagnostics.

#### Scenario: Repeat lowering
- **WHEN** a file is lowered twice
- **THEN** the dumps are byte-identical

#### Scenario: Other file changes
- **WHEN** another file in the project changes and this file's tree and facts are the same
- **THEN** this file's graphs are identical to the previous run
