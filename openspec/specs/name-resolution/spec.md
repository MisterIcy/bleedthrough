## Purpose

Defines how names written in source become fully qualified names, following PHP's rules, so every later stage refers to classes, functions and constants unambiguously and knows when it could not.

## Requirements

### Requirement: Class names resolve through namespace and imports
A class name SHALL resolve to a fully qualified name by PHP's rules: a fully qualified name is taken as written; a qualified or unqualified name is first matched against the file's `use` imports (by the first segment, or the whole name for unqualified), and otherwise prefixed with the current namespace; a `namespace\` relative name is prefixed with the current namespace. Matching of class names SHALL be case-insensitive while preserving the declared spelling for display.

#### Scenario: Import alias
- **WHEN** a file imports `App\Db\Connection as Conn` and uses `new Conn()`
- **THEN** the name resolves to `App\Db\Connection`

#### Scenario: Current namespace prefix
- **WHEN** a file in namespace `App\Http` refers to `Request` with no import
- **THEN** the name resolves to `App\Http\Request`

#### Scenario: Qualified name uses first segment
- **WHEN** a file imports `App\Db as D` and refers to `D\Connection`
- **THEN** the name resolves to `App\Db\Connection`

### Requirement: Functions and constants fall back to the global namespace
An unqualified function or constant name SHALL resolve to the current namespace's declaration if one exists in the symbol table, otherwise to the global one. Imports of kind `function` and `const` SHALL be honoured separately from class imports. Where the choice depends on a declaration that cannot be found, the resolution SHALL record both candidates.

#### Scenario: Namespaced shadowing
- **WHEN** namespace `App` declares `strlen` and a file in `App` calls `strlen()`
- **THEN** the call resolves to `App\strlen`

#### Scenario: Global fallback
- **WHEN** nothing named `strlen` exists in the current namespace
- **THEN** the call resolves to `\strlen`

#### Scenario: Undecidable fallback
- **WHEN** the namespaced declaration might exist in code that is not loaded
- **THEN** the resolution lists the namespaced and the global candidates

### Requirement: Special class names are resolved in context
`self`, `static` and `parent` SHALL resolve against the enclosing class-like: `self` to it, `parent` to its declared parent, and `static` to a late-static-binding marker bound to it. Outside a class-like, or `parent` without a parent, the resolution SHALL fail explicitly. Inside a closure the binding SHALL be that of the defining scope.

#### Scenario: parent without a parent
- **WHEN** `parent::foo()` appears in a class with no parent
- **THEN** resolution reports an error naming the construct

### Requirement: Group, multiple and conditional imports are handled
Group uses (`use A\{B, C as D}`), multiple clauses, and function and const imports SHALL be expanded. Namespace blocks and the global namespace SHALL scope imports correctly, and imports SHALL NOT leak between namespace blocks of one file.

#### Scenario: Group use
- **WHEN** `use App\{Models\User, Util\Str as S}` is declared
- **THEN** `User` resolves to `App\Models\User` and `S` to `App\Util\Str`

### Requirement: Resolution never guesses
A name that cannot be resolved (for example a dynamic name, or an import conflict) SHALL yield a resolution failure carrying the reason and the span, never a best guess. Duplicate imports of one alias SHALL be reported as diagnostics.

#### Scenario: Dynamic class name
- **WHEN** code uses `new $class`
- **THEN** no name is produced and the construct is marked dynamic for the caller to handle

### Requirement: Resolution is deterministic and per file
Resolving a file's names SHALL depend only on that file's syntax tree and, for the function and constant fallback, on a symbol table passed in explicitly. The same inputs SHALL always give the same results.

#### Scenario: Repeated resolution
- **WHEN** the same file is resolved twice with the same table
- **THEN** the results are identical
