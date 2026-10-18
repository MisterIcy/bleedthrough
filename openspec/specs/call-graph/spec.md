## Purpose

Defines the project call graph built from lowered units and resolved calls: how callees are determined, how dynamic dispatch and includes are bounded, how recursion is grouped, and how everything that could not be resolved is recorded rather than ignored.

## Requirements

### Requirement: Edges come from call resolutions
For every call instruction, the graph SHALL contain an edge to each candidate target given by the call's resolution. A call resolved to one target SHALL have one edge; a call resolved to several candidates (dynamic dispatch through an interface or parent) SHALL have an edge to each candidate up to the candidate cap and SHALL record that the call is multi-target. A call resolved to a built-in SHALL be marked as a built-in call and have no project edge.

#### Scenario: Interface call with two implementors
- **WHEN** a call through an interface type has two project implementors
- **THEN** the call has two edges and is marked multi-target

#### Scenario: Built-in call
- **WHEN** a call resolves to a built-in function
- **THEN** it carries the built-in's identity and no project edge

### Requirement: Unresolved calls are recorded with reasons
A call with no usable resolution SHALL be recorded as unresolved with the reason given by type inference (unknown receiver, dynamic name, missing class, depth or candidate cap), and SHALL be counted in the coverage. An unresolved call SHALL NOT silently disappear from the graph.

#### Scenario: Dynamic function name
- **WHEN** a call uses a variable as the function name and no literal value is known
- **THEN** it is recorded as unresolved with the dynamic reason and counted

### Requirement: Candidate sets are capped
When the number of dispatch candidates exceeds the configured cap, the graph SHALL keep the candidates in a deterministic order up to the cap, SHALL mark the call as truncated, and SHALL count the truncation as a limit hit.

#### Scenario: Interface with many implementors
- **WHEN** an interface has more implementors than the cap
- **THEN** only the first cap-many in deterministic order are edges, and the call is marked truncated

### Requirement: Includes are resolved through a port
For each `include`, `include_once`, `require` and `require_once`, the engine SHALL ask an include resolver, a port it owns, for the target files when the path is a literal or a constant expression the engine can evaluate (string literals, concatenation of literals, `__DIR__`, `__FILE__`, `dirname(__FILE__)`, and class or global constants with known literal values). A resolved include SHALL give the included file's top-level graph a call edge from the include site. An unresolved or dynamic include SHALL be recorded and counted as an unresolved include. A resolver that finds no file SHALL cause the include to be reported as unresolved, not as an error.

#### Scenario: Literal include
- **WHEN** a file includes `__DIR__ . '/helpers.php'` and that file is in the project
- **THEN** the graph has an edge from the include site to the top-level code of `helpers.php`

#### Scenario: Dynamic include
- **WHEN** the include path depends on a variable
- **THEN** it is recorded as an unresolved include and counted

### Requirement: Recursion is grouped
The graph SHALL compute strongly connected components over its edges and SHALL provide them in a deterministic order such that every component appears after the components it calls. Self-recursive and mutually recursive functions SHALL belong to one component.

#### Scenario: Mutual recursion
- **WHEN** two functions call each other
- **THEN** they are in one component, placed after their other callees

### Requirement: Entry points of analysis are defined
Every unit SHALL be analysable on its own; no unit SHALL require a caller to be analysed. Units with no callers SHALL still be scanned for flows that begin and end inside them or in the callees.

#### Scenario: Function never called
- **WHEN** a function reads `$_GET` and passes it to a sink and nothing calls it
- **THEN** the flow is reported

### Requirement: Graph construction is deterministic
Identical inputs SHALL give identical graphs, edge order and component order, independent of the order in which files were supplied.

#### Scenario: File order
- **WHEN** the same files are supplied in a different order
- **THEN** the graph, including component order, is identical
