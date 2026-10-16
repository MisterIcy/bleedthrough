## Purpose

Defines how the built-in data is generated mechanically from the vendored php-src stub files of each supported release, so that every declaration is accounted for and the result is reproducible.

## ADDED Requirements

### Requirement: The generator reads only pinned, vendored inputs
The generator SHALL read only the vendored stub trees listed in the manifest (one per supported PHP version from 8.0 to 8.5) and the Reflection fixtures, SHALL perform no network access, and SHALL execute no PHP code. Running it twice on the same inputs SHALL produce byte-identical output.

#### Scenario: Offline run
- **WHEN** the generator runs with no network
- **THEN** it completes and the output is identical to a run with network

### Requirement: Stubs are parsed with the project's own pipeline
Stub files SHALL be parsed through the parser-backend port, not by regular expressions or by an external PHP interpreter. The generator SHALL be generic over the backend. Parse results with errors or unsupported nodes in a stub file SHALL be reported with file and span, and SHALL fail the run unless the construct is listed in a reviewed known-gap file with a reason.

#### Scenario: Unparseable construct
- **WHEN** a stub file produces an error node not listed as a known gap
- **THEN** generation fails naming the file and span

### Requirement: Every stub declaration is accounted for
The generator SHALL produce a coverage report comparing, per file and version, the declarations the parser found (functions, classes, interfaces, traits, enums, constants, properties, methods) with the records produced. Any declaration found but not produced SHALL fail the run unless it falls under an explicit exclusion rule with a reason.

#### Scenario: Dropped declaration
- **WHEN** a declaration is found in a stub but no record results
- **THEN** generation fails and names the declaration

### Requirement: Build guards are captured, not ignored
Lines of the form `#if`, `#ifdef`, `#ifndef`, `#elif`, `#else` and `#endif` SHALL be interpreted as a guard stack. A declaration inside guards SHALL carry the guard text. The same symbol declared in several branches of a guard SHALL yield one record with the alternatives recorded, not duplicates or an error.

#### Scenario: Guarded function
- **WHEN** a function is declared inside `#ifdef HAVE_GETHOSTNAME`
- **THEN** its record carries that guard text

#### Scenario: Both branches declare a function
- **WHEN** a function appears in both branches of an `#if/#else`
- **THEN** one record results, with both guard alternatives

### Requirement: Annotations are interpreted
The generator SHALL interpret the annotation vocabulary present in the stubs: `@param` and `@return` refinements, `@tentative-return-type`, `@alias`, `@implementation-alias`, `@deprecated` and `#[\Deprecated]`, `#[\SensitiveParameter]`, `#[\NoDiscard]`, `@readonly`, `@virtual`, `@cvalue`, `@var` on constants, `@genstubs-expose-comment-block`, and `@refcount`. Annotations present in the stubs that the generator does not know SHALL be listed in the report and SHALL fail the run unless allow-listed, so new annotations in a future release are noticed. Documentation prose in docblocks SHALL NOT be stored.

#### Scenario: Unknown annotation
- **WHEN** a stub uses an annotation the generator does not know
- **THEN** generation fails naming the annotation and file

#### Scenario: No prose
- **WHEN** a docblock contains explanatory text
- **THEN** the data contains none of it

### Requirement: Versions are extracted separately, then merged
The generator SHALL extract each pinned release independently and merge by fully qualified name and normalised signature into version masks, per the data model. Normalisation SHALL not alter meaning (for example whitespace and attribute order). A symbol present in some versions only SHALL receive the mask of those versions.

#### Scenario: Function added in 8.3
- **WHEN** a function appears in the 8.3, 8.4 and 8.5 stubs but not earlier
- **THEN** its record has the mask `8.3-8.5`

### Requirement: Test-only and template extensions are excluded explicitly
Stubs of test-only and template extensions (`zend_test`, `dl_test`, `skeleton`) SHALL be excluded by a named, reasoned rule. All other stub files, including SAPI-specific ones, SHALL be included and labelled with their extension or SAPI.

#### Scenario: Exclusion is visible
- **WHEN** the coverage report is produced
- **THEN** it lists the excluded files and the rule that excluded them

### Requirement: Duplicate symbol names across files are merged
The same function declared in several stub files (for example provided by several SAPIs) SHALL be merged into one record that lists each SAPI, with parameter differences kept as separate records only if the signatures differ.

#### Scenario: SAPI-provided function
- **WHEN** a function is declared in two SAPI stub files with the same signature
- **THEN** one record results listing both SAPIs

### Requirement: Extension bundling is declared in a reviewed file
Which extensions are bundled by default in which version SHALL be taken from a reviewed, version-controlled file citing its sources, not inferred from the presence of stubs.

#### Scenario: Unbundled extension
- **WHEN** the file marks `imap` as unbundled from 8.4
- **THEN** the data records `imap` symbols as unbundled for 8.4 onwards
