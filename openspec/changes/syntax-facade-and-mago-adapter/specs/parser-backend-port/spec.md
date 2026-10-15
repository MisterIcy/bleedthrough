## Purpose

Defines the contract that any parser backend satisfies, so the rest of the system can parse PHP without knowing which parser is used and can rely on the same behaviour from every backend, including on hostile or broken input.

## ADDED Requirements

### Requirement: Neutral input and options
A backend SHALL accept a file path (relative to the project root, used only for labelling), the file's bytes (not required to be valid UTF-8), and backend-neutral options: whether short open tags are enabled, a recovery mode (strict or editor), and a maximum input size. Options SHALL NOT expose any backend-specific setting.

#### Scenario: Non-UTF-8 input
- **WHEN** a file with invalid UTF-8 bytes is submitted
- **THEN** the backend returns a result, not an error about encoding

#### Scenario: Editor recovery
- **WHEN** a file is submitted in editor recovery mode and it ends mid-statement
- **THEN** the backend returns the unfinished constructs in the tree and diagnostics for the missing delimiters

### Requirement: A result always comes back
For every possible input, a backend SHALL return a result consisting of a syntax tree, diagnostics, a parse quality, and backend identity. It SHALL NOT panic, abort, exhaust memory without bound, or run without bound, for any input including empty files, files with NUL bytes, truncated files, and deeply nested code.

#### Scenario: Deep nesting
- **WHEN** a file nests expressions deeper than the backend's limit
- **THEN** the result has a recursion-limit diagnostic and a quality of recovered or failed, and the process continues

#### Scenario: Empty file
- **WHEN** an empty file is submitted
- **THEN** the result is a valid clean tree with no statements

#### Scenario: Oversized input
- **WHEN** the input exceeds the maximum size option
- **THEN** the result has quality failed, an input-too-large diagnostic, and no parsing is attempted

### Requirement: Backends are pure and thread-safe
A backend SHALL NOT access the file system, environment or clock while parsing. It SHALL be safe to call from many threads at once, and the same input SHALL always produce an equal result.

#### Scenario: Concurrent parsing
- **WHEN** many files are parsed concurrently through one backend
- **THEN** each result equals the result of parsing that file alone

### Requirement: Backends identify themselves
A backend SHALL report a name and a version identifying the parser build it wraps. The identity SHALL change when the wrapped parser's version changes, and SHALL be usable as part of a cache key and in the run's coverage information.

#### Scenario: Parser upgrade
- **WHEN** the pinned parser version is changed
- **THEN** the reported backend version changes with it

### Requirement: The port lives in the syntax crate
The port definition and everything a caller needs to use it SHALL be in the syntax crate. Callers SHALL obtain a backend only as an injected value and SHALL NOT name a concrete backend.

#### Scenario: Caller depends on the port only
- **WHEN** a domain or application crate parses a file
- **THEN** it does so through the port and does not depend on any adapter crate
