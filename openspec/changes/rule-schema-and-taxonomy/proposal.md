## Why

Every rule and every finding must be tagged with CWE and OWASP categories and a severity, and the rest of the system (pack loading, the rule catalog, the engine, baselines, CI gates) reads rules through one schema. Today the taxonomy and severity types live in `bleedthrough-findings`, which the rule model cannot depend on, the `bleedthrough-rules` crate is an empty skeleton, and the severity "derived from CVSS-like scoring" decision has no model. This change fixes the vocabulary and the rule file contract first, so the catalog, packs and engine are all written against a stable, validated schema rather than each inventing their own.

## What Changes

- Add a pure domain crate **`bleedthrough-taxonomy`** that owns the shared vocabulary: CWE identifiers, OWASP Top 10 categories qualified by edition, severity level, and the severity model. `Cwe`, `OwaspRef`, `Taxonomy`, `SeverityLevel` and `Severity` move there from `bleedthrough-findings` (**BREAKING** for the crate-internal API only; no release exists). `bleedthrough-findings` and `bleedthrough-rules` both depend on it.
- Add a curated, versioned **registry** of supported weaknesses (CWE number, name, the CWE catalog version it was checked against, and the OWASP 2021 categories it is conventionally mapped to) and the OWASP Top 10 2021 categories A01 to A10. A rule or finding that cites a CWE outside the registry is rejected. Open redirect (CWE-601) is in the v1 registry; XSS (CWE-79) is not.
- Add the **severity model**: a rule declares CVSS 3.1 base metrics; a fixed, versioned formula derives the score, the vector string and the severity level. Findings take their severity from their rule.
- Define the **rule schema** in `bleedthrough-rules` (model and validation, no I/O): rule metadata (identifier, title, description, remediation, references, taxonomy, severity metrics, paranoid flag, minimum and maximum PHP version, optional pack association), **taint rules** (sources, sinks with vulnerable argument positions, sanitizers by sink class, propagators, all referencing PHP symbols by name), and **simple AST pattern rules** (calls with argument constraints, use of banned functions or constructs). The general pattern language is out of scope.
- Add a **TOML rule loader** in `bleedthrough-fs`: parses rule files into the model through a port owned by `bleedthrough-rules`, rejects unknown keys, reports every error with file, line and key, and produces a deterministic rule set with a content hash for cache keys and report identity.
- Add a **rule-schema reference** document, and specify a `rules check` validation entry point (wired into the CLI in `scan-pipeline`).
- Update `architecture.toml`, `deny.toml` (new third-party crates for TOML parsing in infrastructure only), API snapshots, glossary and docs.

Out of scope: the actual v1 rule catalog (`v1-rule-catalog`), pack activation and runtime data packs (`pack-system`), how the engine evaluates rules (`ir-and-taint-engine`), the general pattern DSL, suppression syntax in rule files, and CLI wiring.

## Capabilities

### New Capabilities
- `security-taxonomy`: the shared CWE and OWASP vocabulary, the curated weakness registry, edition handling, and the rule that unknown tags are rejected.
- `severity-scoring`: the rule-declared metrics, the fixed formula, vector string, level mapping, and independence from confidence.
- `rule-schema`: the rule model and the TOML file contract: metadata, taint rules, pattern rules, validation, errors, determinism and versioning.

### Modified Capabilities
- `finding-model`: taxonomy tags must come from the registry, and severity is derived from rule metrics by the severity model instead of being supplied freely.
- `workspace-layout`: the domain layer gains the taxonomy crate.

## Impact

- New crate `crates/taxonomy` (domain, no workspace or third-party dependency); `crates/findings` and `crates/rules` depend on it; `architecture.toml` lists the new edges; `api/` snapshots gain `bleedthrough-taxonomy.txt` and change for findings and rules.
- `crates/rules` gains the model and validation, still dependency-free beyond taxonomy.
- `crates/fs` gains the TOML loader and the third-party `toml` and `serde` crates (infrastructure only; licences checked by `cargo deny`).
- Downstream: `v1-rule-catalog` and `pack-system` author rules against this schema; `ir-and-taint-engine` consumes the rule model; `scan-pipeline` exposes `rules check`.
