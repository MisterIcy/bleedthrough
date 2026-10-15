## Purpose

Defines the owned syntax model that all analysis reads PHP through: what it represents, how locations and comments are expressed, and the guarantee that no source code is silently lost, so that a parser backend can be replaced without any change downstream.

## ADDED Requirements

### Requirement: The syntax model contains no third-party parser types
The syntax model, including its spans, diagnostics and identifiers, SHALL be defined entirely by the project. No type, trait or lifetime from any parser backend SHALL appear in the public interface of the syntax crate or of any crate that depends on it.

#### Scenario: Backend swapped
- **WHEN** a different parser backend is used to produce the syntax model
- **THEN** no crate other than the adapter and the composition root changes

### Requirement: The model represents the constructs of PHP 8.0 to 8.5
The syntax model SHALL represent every construct that symbol collection or flow analysis needs, for PHP 8.0 through 8.5, including: namespaces and imports; classes, interfaces, traits and enums with their members; constants, properties (including readonly, asymmetric visibility and property hooks) and promoted constructor parameters; functions, methods, closures and arrow functions; attributes; union, intersection, disjunctive-normal-form and nullable types; all control-flow statements including match, try, catch and finally, switch, goto and declare; every operator and assignment form including destructuring and references; function, method, static, nullsafe and first-class-callable calls; object creation; array and list forms; strings with interpolation, heredoc and nowdoc; include, require, eval and exit; the pipe operator; clone; throw as an expression; yield and yield from; magic constants; variable variables and dynamic member and constant access; and inline HTML with its open and close tags.

#### Scenario: Valid feature fixture
- **WHEN** a valid fixture for any listed construct is parsed
- **THEN** the resulting tree contains no unsupported or error node

#### Scenario: Syntax newer than the modelled version
- **WHEN** code uses a construct introduced after the configured PHP version
- **THEN** the model still represents it, and the parser does not reject it on version grounds

### Requirement: Legacy syntax is accepted leniently
Syntax that was valid in PHP 7.x and remains valid in PHP 8.x SHALL be represented like any other syntax. Syntax that PHP 7.x accepted and PHP 8.x rejects (such as the `(unset)` cast or brace-style string and array offsets) SHALL be accepted and represented where the tree is unambiguous, so that legacy code can still be analysed; the project's analysis scope remains PHP 8.0 to 8.5. Which legacy constructs are accepted SHALL be pinned by the fixture corpus, so any change in this behaviour is visible.

#### Scenario: Legacy cast
- **WHEN** a file uses the `(unset)` cast
- **THEN** the file parses without an error node, and the corpus records this as a pinned lenient case

### Requirement: Nothing is silently dropped
Every byte of a source file SHALL be accounted for: it is whitespace, part of a comment, inside a node that represents it, or inside an explicit error node or unsupported node. A construct the model cannot represent SHALL become an unsupported node carrying the construct's kind name and span. A region the parser could not parse SHALL become an error node covering that region. The number of unsupported and error nodes SHALL be available to the caller.

#### Scenario: Unmappable construct
- **WHEN** a backend yields a construct that the model has no representation for
- **THEN** the tree contains an unsupported node with that construct's kind and span, and the file's unsupported count is non-zero

#### Scenario: Whole-file coverage
- **WHEN** any file is parsed, valid or not
- **THEN** the union of trivia, nodes, error nodes and unsupported nodes covers every non-whitespace byte of the file

### Requirement: Spans are byte offsets with consistent nesting
Every node SHALL have a span given as start and end byte offsets into the original file, with start not after end and both within the file. A child's span SHALL lie within its parent's, and sibling spans SHALL not overlap and SHALL be in source order. Offsets SHALL be convertible to 1-based line and 1-based column counted in Unicode scalar values, where each byte of invalid UTF-8 counts as one column.

#### Scenario: Multi-byte text
- **WHEN** a node follows non-ASCII text on the same line
- **THEN** its column counts scalar values while its byte offsets locate the same text

#### Scenario: Invalid UTF-8
- **WHEN** a file contains bytes that are not valid UTF-8
- **THEN** the file is still parsed, the bytes are retained, and each invalid byte counts as one column

### Requirement: Comments and docblocks are retained raw
All comments SHALL be retained with their kind (line, block or docblock) and span. For any declaration, the model SHALL return the docblock that immediately precedes it, separated by whitespace only, as raw unparsed text. Interpreting docblock content is outside the syntax model.

#### Scenario: Docblock lookup
- **WHEN** a docblock immediately precedes a method declaration
- **THEN** the lookup for that method returns the docblock's text and span

#### Scenario: Detached comment
- **WHEN** another statement lies between a docblock and a declaration
- **THEN** the lookup for the declaration returns nothing

### Requirement: Names are kept as written
Identifiers that denote classes, functions and constants SHALL be retained as written in the source with their qualification kind (unqualified, qualified, fully qualified or namespace-relative). The model SHALL NOT resolve them.

#### Scenario: Qualified name
- **WHEN** code refers to `\Foo\Bar::baz()`
- **THEN** the model records the name `Foo\Bar` as fully qualified and the member `baz`, without resolving imports

### Requirement: Diagnostics and parse quality describe each file
Each parsed file SHALL carry diagnostics, each with a stable code, a message and a span, and a parse quality: clean (no diagnostics and no unsupported node), recovered (diagnostics or unsupported nodes, but a usable tree), or failed (no usable tree). The codes SHALL include at least unexpected token, unexpected end of file, unclosed string, recursion limit exceeded, backend failure, input too large, and unsupported construct.

#### Scenario: Syntax error in one method
- **WHEN** a file contains one syntax error inside a method
- **THEN** the file's quality is recovered, a diagnostic locates the error, and an error node covers the affected region

#### Scenario: Unusable input
- **WHEN** no usable tree can be produced
- **THEN** the quality is failed and a diagnostic states why

### Requirement: Literals keep their source form
String and numeric literals SHALL retain their raw source text. Interpolated strings, heredocs and nowdocs SHALL expose their literal parts and embedded expressions separately, in order.

#### Scenario: Interpolation
- **WHEN** a double-quoted string embeds `$name` and `{$obj->method()}`
- **THEN** the model exposes the literal fragments and the two embedded expressions in source order

### Requirement: The model is immutable and deterministic
A syntax tree SHALL be immutable once built, SHALL be safe to share across threads, and SHALL be identical for identical input and backend version.

#### Scenario: Repeated parse
- **WHEN** the same bytes are parsed twice
- **THEN** the two trees and their diagnostics are equal
