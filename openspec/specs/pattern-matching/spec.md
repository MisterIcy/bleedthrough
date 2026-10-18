## Purpose

Defines how pattern rules, the closed set of non-flow rule shapes, are executed against the code, so that simple dangerous constructs are found without a taint flow and with the same output model as flow findings.

## Requirements

### Requirement: Call patterns match resolved calls
A `call` pattern SHALL match each call whose resolved target is the named symbol (function, static method, instance method with optional subclass matching, constructor), and SHALL evaluate its argument conditions against the call's arguments. A call whose target is unresolved SHALL match only when the written name matches the rule's symbol exactly and no other candidate was resolved, and the finding SHALL carry the unresolved-callee reason.

#### Scenario: Function call matches
- **WHEN** code calls the named function with arguments meeting the conditions
- **THEN** a finding is reported at the call

#### Scenario: Different function of the same short name
- **WHEN** a call resolves to a differently namespaced function with the same short name
- **THEN** no finding is reported

### Requirement: Argument conditions are evaluated exactly
The tests `literal`, `non-literal`, `absent`, `equals` and `matches` SHALL be evaluated on the argument expression after constant folding of literal string concatenation and class or global constants with known literal values. `matches` SHALL use the schema's linear-time regular expression subset. An argument that cannot be folded SHALL be non-literal.

#### Scenario: Folded concatenation
- **WHEN** an argument is `'ab' . 'cd'` and the condition is `equals "abcd"`
- **THEN** the condition holds

#### Scenario: Absent argument
- **WHEN** the call omits the argument and the condition is `absent`
- **THEN** the condition holds

### Requirement: Use patterns match any use of a symbol
A `use` pattern SHALL match any reference to the named function, class or construct: a call, `new`, a static access, a type-hint position excluded, a callable string or array naming it when the string is a literal, and a first-class callable form.

#### Scenario: Callable string
- **WHEN** code passes the literal string naming a flagged function as a callback
- **THEN** a finding is reported at the string

### Requirement: Include-non-literal patterns
An `include-non-literal` pattern SHALL match each `include`, `include_once`, `require` and `require_once` whose path cannot be folded to a literal.

#### Scenario: Dynamic path
- **WHEN** a `require` uses a variable path
- **THEN** a finding is reported at the include

#### Scenario: Folded path
- **WHEN** the path is `__DIR__ . '/a.php'`
- **THEN** no finding is reported

### Requirement: Pattern findings use the common output model
Pattern findings SHALL have the rule's identifier, taxonomy, severity and fingerprint, a location at the matched construct, and no trace. Their confidence SHALL be high except where the call was matched by name alone or the file was parsed with recovery, which SHALL lower it with the matching reason. Version and paranoid gating SHALL apply as for taint rules.

#### Scenario: Finding shape
- **WHEN** a pattern rule matches
- **THEN** the finding has the rule's severity and taxonomy, a location, and no trace
