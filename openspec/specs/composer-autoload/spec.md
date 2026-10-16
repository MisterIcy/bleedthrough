## Purpose

Defines how Composer's autoload configuration is read and used to locate class files, so vendor and project classes can be loaded on demand and the dependency set is known from the lock file.

## Requirements

### Requirement: Autoload rules are read from composer.json and composer.lock
The system SHALL read the `autoload` and `autoload-dev` sections of the root `composer.json` and of each package in `composer.lock`: PSR-4 and PSR-0 prefixes (with multiple directories per prefix), classmap entries, and files entries. Excluded paths (`exclude-from-classmap`) SHALL be honoured. Malformed or missing files SHALL yield diagnostics and an empty rule set for that source, not a failure of the run.

#### Scenario: PSR-4 prefix
- **WHEN** `composer.json` maps `App\` to `src/`
- **THEN** `App\Http\Kernel` locates `src/Http/Kernel.php`

#### Scenario: Malformed lock file
- **WHEN** `composer.lock` is not valid JSON
- **THEN** a diagnostic is produced and project-only loading continues

### Requirement: Locating is a port owned by the symbols crate
The symbols crate SHALL own a class-locator port mapping a fully qualified class name to a project-relative or vendor-relative file path or to "not found", and a class-source port yielding the summary for a located file. The Composer reader SHALL implement the locator in infrastructure. The symbols crate SHALL NOT read files or JSON.

#### Scenario: Replacing the locator
- **WHEN** a different locator is supplied (for example a fixed map in a test)
- **THEN** the symbol table builder works unchanged

### Requirement: Location is deterministic and follows Composer's order
When several rules could match a name, the locator SHALL apply Composer's precedence (classmap, then PSR-4 longest prefix, then PSR-0) and, within a rule, the directories in declared order, returning the first existing file. The result SHALL not depend on directory enumeration order.

#### Scenario: Longest prefix
- **WHEN** both `App\` and `App\Models\` are mapped
- **THEN** `App\Models\User` uses the `App\Models\` mapping

### Requirement: Paths stay inside the project
Located paths SHALL be normalised, SHALL be relative to the project root or the vendor directory, and SHALL be rejected if they escape both (through `..` or symlinks to outside locations), with a diagnostic.

#### Scenario: Escape attempt
- **WHEN** a mapping points outside the project
- **THEN** the rule is ignored with a diagnostic

### Requirement: The package set is available to other stages
The reader SHALL expose the list of locked packages with name, version and source reference, ordered by name, so pack activation can use it without re-reading the file. The data SHALL be identical for identical lock files.

#### Scenario: Package list
- **WHEN** the lock file lists two packages
- **THEN** they are exposed in name order with versions

### Requirement: Files autoload entries are reported
Files listed under `files` autoload (which execute on load) SHALL be included in the project's analysed file set and SHALL be reported in coverage.

#### Scenario: Bootstrap file
- **WHEN** a package lists `src/helpers.php` under `files`
- **THEN** it is included as an analysed file
