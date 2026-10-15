# Taxonomy registry

The vocabulary rules and findings are tagged with lives in `bleedthrough-taxonomy`
(`crates/taxonomy/src/registry.rs`). It is compiled-in data, not a loaded file: a tag for a
weakness or category that is not in the registry is rejected when it is constructed, so a tag cannot
be misspelt, invented or ambiguous between editions.

## Sources

Names and mappings are taken from the publications while authoring, never from memory.

| What | Source | Version |
|---|---|---|
| Weakness names | MITRE CWE catalog definition pages, `https://cwe.mitre.org/data/definitions/<number>.html` | CWE 4.20 (the version shown on every page when fetched, 2026-10-15) |
| Category ids and names | OWASP Top 10 2021, `https://top10.owasp.org/2021/` | 2021 |
| Weakness to category mapping | the "List of Mapped CWEs" section of each OWASP 2021 category page, e.g. `https://top10.owasp.org/2021/A03_2021-Injection/` | 2021 |

Each registry entry records the catalog version its name was checked against (`catalog`). A weakness
is mapped to every 2021 category whose "List of Mapped CWEs" contains it (CWE-73 and CWE-259 appear
in two).

## Scope of v1

Injection (SQL CWE-89, OS command CWE-78, generic command CWE-77, argument CWE-88, code CWE-94 and
CWE-95, LDAP CWE-90, XPath CWE-643), path handling (CWE-22, CWE-73), PHP file inclusion (CWE-98),
unrestricted upload (CWE-434), unsafe deserialization (CWE-502), XML external entities (CWE-611),
server-side request forgery (CWE-918), open redirect (CWE-601), weak cryptography and randomness
(CWE-327, CWE-328, CWE-330, CWE-338), hard-coded credentials and keys (CWE-259, CWE-321, CWE-798),
and insecure cookie configuration (CWE-614, CWE-1004).

Cross-site scripting (CWE-79) is **not** in the registry in v1; a rule tagged CWE-79 is rejected as
an unregistered weakness. CWE-16 ("Configuration") is a CWE *category*, not a weakness, and is not
registered; configuration rules use the specific weaknesses above.

`v1-rule-catalog` extends the list as rules are written.

## Text forms

`CWE-<number>` and `OWASP:<edition>:<id>`, for example `CWE-89` and `OWASP:2021:A03`. Both parse
back to the same values.

## Consistency check

A rule's categories must each be in the mapping of at least one of its weaknesses
(`Taxonomy::checked`). `CWE-89` with `OWASP:2021:A02` is rejected. A deliberate cross-mapping is
added as an `owasp_extra` entry with a non-empty `reason`.

## Adding a weakness

1. Fetch `https://cwe.mitre.org/data/definitions/<number>.html`. Confirm it is a weakness (not a
   category or view) and copy its name and the catalog version shown on the page.
2. Find its OWASP 2021 category by looking for `CWE-<number>` in the "List of Mapped CWEs" of the
   ten category pages under `https://top10.owasp.org/2021/`. An entry with no mapping needs a
   discussion before it is added.
3. Add one `WeaknessEntry { number, name, catalog, categories }` to `WEAKNESSES` in
   `crates/taxonomy/src/registry.rs`, in ascending order of number. `catalog` is `CATALOG`; if the
   page shows a newer version, update `CATALOG` only after re-checking every existing name.
4. Run `cargo test -p bleedthrough-taxonomy` (the registry tests check ordering, the catalog version
   and that every cited category exists) and `cargo xtask check-all`.
5. Update the scope list above and cite the page in the pull request.

## Adding an OWASP edition

Add the `CategoryEntry` rows of the new edition to `CATEGORIES` and extend the `categories` of the
weaknesses that map to it. Existing entries do not change; tags carry their edition, so 2021 tags
stay valid.

## Custom registries

`Registry::new`, `Cwe::new_in` and `OwaspRef::new_in` build tags against a registry other than the
built-in one. They exist for editions not yet curated and for tests (for example the emitter
fixtures that exercise two editions). Production code validates against `Registry::builtin()`
through `Cwe::new`, `OwaspRef::new` and `Taxonomy::checked`; adding a weakness to the shipped
registry remains an explicit edit to `registry.rs`.
