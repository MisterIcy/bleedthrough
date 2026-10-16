## Purpose

Defines where the built-in data comes from, how its sources are pinned and verified, how the licences of those sources are honoured, and how it is regenerated when PHP releases a new version.

## ADDED Requirements

### Requirement: Sources are vendored and pinned by content
The `*.stub.php` files of the pinned release tag for each supported version SHALL be vendored in the repository under their original paths, together with the licence files that govern them. A manifest SHALL record, for each version, the tag, the full commit SHA, and a content hash for every vendored file. A check SHALL fail if any vendored file differs from its recorded hash, or if a file is present that the manifest does not list.

#### Scenario: Altered vendored file
- **WHEN** a vendored stub file is edited
- **THEN** the check fails naming the file

#### Scenario: Unlisted file
- **WHEN** a file is added to the vendored tree without a manifest entry
- **THEN** the check fails

### Requirement: Vendored files are unmodified and excluded from project checks that do not apply
Vendored files SHALL be byte-identical to the upstream files at the pinned commit. They SHALL be excluded from formatting and lint checks that do not apply to them, by explicit configuration.

#### Scenario: Verification against upstream
- **WHEN** a vendored file is compared with the upstream file at the recorded commit
- **THEN** the bytes are equal

### Requirement: The licence texts accompany every distribution
A `NOTICE` file SHALL contain the full PHP License 3.01 text and the Zend Engine License 2.00 text with their copyright lines, identify the data as derived from the PHP source files, and be included in release archives and the Docker image. A check SHALL fail if a release or image build omits it.

#### Scenario: Release archive
- **WHEN** a release archive is built
- **THEN** it contains the NOTICE file

### Requirement: Only facts are stored
The generated data, the fixtures and the manifest SHALL contain names, types, flags, version information, default expression text and constant values, and SHALL NOT contain documentation prose, comments or examples copied from the PHP source or the PHP manual. A check SHALL verify that data fields contain only the expected shapes.

#### Scenario: Prose in a field
- **WHEN** a record field contains free-form sentences
- **THEN** the format check fails

### Requirement: Regeneration is checked in CI
`cargo xtask check-builtins` SHALL regenerate the data from the vendored sources and fixtures, compare it byte for byte with the checked-in data file, verify the manifest hashes, the NOTICE presence and the format validity, and fail on any difference. It SHALL be part of the full gate run.

#### Scenario: Stale data
- **WHEN** a vendored source or the generator changes without regenerating
- **THEN** the check fails and says to run `cargo xtask gen-builtins`

### Requirement: Updating for a new PHP release is a documented procedure
The repository SHALL document how to add a new PHP version: vendor the tag and update the manifest, extend the version mask, add the Reflection fixture, regenerate, review the diff and the coverage report, and update the bundling file. Adding a version SHALL NOT change existing records except by extending masks.

#### Scenario: New version added
- **WHEN** a new PHP version is added
- **THEN** earlier versions' lookups return the same results as before

### Requirement: The licence position is confirmed before release
Before the first public release, the project owner SHALL record in the repository a decision on the licensing of the derived data, after reviewing the PHP License 3.01, the Zend Engine License 2.00, and the research notes. Until then the repository SHALL state that the position is unconfirmed.

#### Scenario: Release gate
- **WHEN** a release is prepared and no decision is recorded
- **THEN** the release checklist fails
