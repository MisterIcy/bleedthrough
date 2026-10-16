# Architecture review checklist

CI proves layering, purity, API stability, size and vocabulary (see [architecture.md](architecture.md)).
It does not prove the following. Reviewers check them on every pull request.

## Single responsibility
- [ ] Each new or changed type, module and crate has one reason to change, stated in its docs.
- [ ] A new crate or module is not a grab-bag; the concern it owns is nameable in one sentence.

## Interface segregation
- [ ] New traits are small and shaped by their callers; no implementor is forced to stub methods.
- [ ] Ports expose only what the owning layer needs.

## Liskov substitution
- [ ] Every implementation of a trait or port honours its documented contract (errors, ordering, panics), not just its signature.
- [ ] No caller needs to know which implementation it holds.

## Open/closed
- [ ] New behaviour is added by new data, packs, adapters or trait implementations rather than by editing a central `match`.
- [ ] Public enums and structs are `#[non_exhaustive]` unless documented as intentionally closed.

## DDD boundaries
- [ ] Domain terms match `docs/glossary.md`; new concepts are added there first.
- [ ] Third-party types do not leak past an adapter (the anti-corruption layer).
- [ ] Pack and rule content stays plain data and never references AST types.
- [ ] Value objects (`Finding`, `Fingerprint`, `Confidence`, `Coverage`) are not mutated after construction.

## Gaps in the automated checks
- [ ] Macro-generated or glob re-exported public items are reflected in the `api/` snapshot by hand if relevant.
- [ ] Any new edge in `architecture.toml` carries a comment justifying it.
- [ ] Any `#[allow(...)]` of a size or complexity lint is local and justified in a comment.

## Tests
- [ ] Every scenario the change adds or alters has a test with a `/// spec:` marker (`docs/testing.md`).
- [ ] Each security decision (taint, sanitiser, trust, reachability, name or type resolution, parsing of untrusted text) has a negative test next to its positive one.
- [ ] New pure logic carries properties: normalisation or ordering (idempotent, order-independent, absorbing as specified), resolution (never panics, stable, within the declared universe), parsers of untrusted text (arbitrary bytes and near-valid mutations never panic and respect limits).
- [ ] Properties complement, and do not replace, an example test of each specified scenario.
- [ ] New tests are hermetic and deterministic: no network, interpreter or clock; seeds fixed; anything else `#[ignore = "reason"]`.
- [ ] Before archiving a change, read the spec traceability report (`cargo xtask spec-trace`): every scenario the change added has a marked test, or is listed as manual with its reason, and no marker is stale.
