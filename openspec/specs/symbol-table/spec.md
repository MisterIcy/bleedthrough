## Purpose

Defines the project-wide table of declarations and the relationships between them, so analysis can look up any class, function, constant, method or property, walk inheritance, and know exactly what could not be found.

## Requirements

### Requirement: Declarations are collected per file as summaries
For each file, the symbols crate SHALL produce a summary of the declarations it contains: namespaces, classes, interfaces, traits, enums (with backing type and cases), functions, constants (including `define` of a literal name), and for each class-like its constants, properties (including promoted constructor parameters and hooks), methods with parameters and native types, modifiers, attributes, and raw docblock. A summary SHALL be a pure function of the file's syntax tree and SHALL carry no reference to other files.

#### Scenario: Promoted parameter
- **WHEN** a constructor declares `private readonly Db $db`
- **THEN** the summary has a property `db` with the class type and the readonly and private modifiers

#### Scenario: Summary independence
- **WHEN** two files are summarised in either order
- **THEN** each summary is identical to its summary produced alone

### Requirement: The table is built deterministically from summaries
The symbol table SHALL be built from a set of summaries and SHALL be immutable once built. Its contents and every iteration order SHALL be independent of the order in which summaries are supplied and of thread count. Duplicate declarations of the same name SHALL be kept, reported as diagnostics naming both locations, and resolved by a documented rule (the first by path order wins for lookup).

#### Scenario: Supplied in different orders
- **WHEN** the same summaries are supplied in two orders
- **THEN** the tables are equal

#### Scenario: Duplicate class
- **WHEN** two files declare the same class name
- **THEN** a diagnostic names both and lookup returns the first by path

### Requirement: Hierarchy is resolved with cycle protection
Parents, interfaces, trait uses (with `insteadof` and `as` adaptations), and enum interfaces SHALL be linked. Linking SHALL tolerate missing parents (recorded as unresolved) and SHALL detect inheritance cycles, break them, and report a diagnostic, without non-termination.

#### Scenario: Cyclic inheritance
- **WHEN** `A extends B` and `B extends A`
- **THEN** linking terminates, reports the cycle, and both classes remain usable

#### Scenario: Unresolved parent
- **WHEN** a class extends a class that is not loaded
- **THEN** the class is marked as having an unresolved ancestor and its lookups report that

### Requirement: Member lookup follows PHP semantics
Looking up a method, property or constant on a class SHALL search the class, its traits, its parents and its interfaces in PHP's order, honour visibility from the calling context, and return all candidates when an interface or abstract declaration gives several. A lookup that cannot be completed because of an unresolved ancestor or a magic method (`__call`, `__get`, `__callStatic`) SHALL say so in its result.

#### Scenario: Inherited method
- **WHEN** `Child extends Base` and `Base` declares `run()`
- **THEN** `Child::run` resolves to `Base::run`

#### Scenario: Trait method
- **WHEN** a class uses a trait declaring `log()`
- **THEN** `log` resolves to the trait method with the class as its context

#### Scenario: Magic fallback
- **WHEN** a method is not found but the class declares `__call`
- **THEN** the lookup reports a magic-call possibility

### Requirement: Subtype and subclass queries
The table SHALL answer whether one class-like is a subtype of another, list the known subclasses and implementors of a type (within the loaded set), and list the interfaces of a class, with results ordered deterministically.

#### Scenario: Implementors
- **WHEN** three loaded classes implement `LoggerInterface`
- **THEN** the query returns exactly those three in name order

### Requirement: Loading vendor classes is bounded and explicit
A builder SHALL be able to request classes that are referenced but not yet loaded from a class-source port and iterate to a closed set. It SHALL stop at a configurable limit on loaded files or depth, and the result SHALL record each class that was requested but not found and whether the limit was hit, for the coverage report.

#### Scenario: Limit reached
- **WHEN** the closure would load more files than the limit
- **THEN** loading stops, the table is valid, and the result states that the limit was hit

#### Scenario: Class not found
- **WHEN** a referenced class is absent from every source
- **THEN** it is recorded as unresolved and no error is raised

### Requirement: Tables expose a stable fingerprint of summaries
Each summary SHALL expose a content fingerprint that depends only on its declarations (not on comments, whitespace or positions of unrelated code), so that other stages can tell whether a file's declared interface changed.

#### Scenario: Body-only change
- **WHEN** only a method body changes
- **THEN** the file's declaration fingerprint is unchanged

#### Scenario: Signature change
- **WHEN** a method's parameter type changes
- **THEN** the declaration fingerprint changes
