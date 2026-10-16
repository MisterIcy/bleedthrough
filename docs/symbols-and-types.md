# Symbols and types

How the analysis knows what a name means and what type a value has. The code is in
`bleedthrough-symbols` (domain layer); the built-in data comes from `bleedthrough-builtins`, and
reading Composer metadata lives in `bleedthrough-fs`.

Pipeline: syntax tree → **file summary** (per file, cacheable) → **symbol table** (all files plus
built-ins, immutable) → **inference** (per function body, against the table).

## Name rules

A written name becomes a **fully qualified name** (`Fqn`) in the scope where it appears.

- Class and function names are compared case-insensitively; the declared spelling is kept for
  display. Constants are case-sensitive in their last segment.
- A name with a leading `\` is already qualified. A qualified name (`A\B`) has its first segment
  replaced by a matching `use` alias, else the current namespace is prefixed. An unqualified class
  name uses a `use` alias, else the current namespace.
- An unqualified function or constant name tries the namespaced candidate first, then the global
  one (`FunctionOrConst`); the table decides which exists.
- `self` and `parent` resolve to the enclosing class and its declared parent; `static` is a
  late-static marker bound to the enclosing class. In a trait, `self` is also late-bound.
  Outside a class they fail with a named reason.
- `namespace\X` is relative to the current namespace. Dynamic names (`new $x`) are `Dynamic`.
- A duplicate `use` alias is reported as a diagnostic and the first import wins.

## Summaries and the table

A **file summary** holds a file's declarations (classes, interfaces, traits, enums, functions,
constants) with effective types, plus the classes and functions its bodies reference. A body-only
edit keeps the summary **fingerprint**; a signature or type-bearing docblock change alters it.

The **symbol table** is built from summaries and is independent of supply order. It answers
class, function, constant, method, property and class-constant lookups with inheritance (parents,
interfaces, traits), visibility against a calling class, and magic-method fallbacks (`__call`,
`__callStatic`, `__get`, `__set`). Lookups report `incomplete` when an ancestor is not loaded.
Duplicate declarations and inheritance cycles are diagnostics; cycles terminate. Built-in classes
cannot be shadowed by project classes. Unloaded classes are discovered through a locator
(Composer classmap, PSR-4 longest prefix, PSR-0); loading stops at the **vendor limits**.

## Type syntax

A type is a normalised union. Native hints and docblocks (`@param`, `@return`, `@var`,
`@property`, `@method`, `@template`, with `psalm-` and `phpstan-` prefixes winning over plain
tags) are converted into it. When both exist, a docblock that is a subtype of the native type
wins; otherwise the native type stays and a diagnostic says the docblock contradicts it or is
unverified.

Supported: scalars and literals (`'a'`, `5`), `int` ranges (`positive-int`, `int<1, 9>`),
`non-empty-string`, `numeric-string`, `class-string<T>`, `array<K, V>`, `list<V>`,
`non-empty-` variants, shapes `array{id: int, name?: string, ...}`, `iterable<K, V>`,
`callable(int, string=): void`, `Closure`, generics `Box<int>`, `static`, `$this`, template
parameters, intersections and unions, `self`/`parent`. Constructs outside this subset are noted
and become `unknown`, never silently `mixed`.

`unknown` (not analysed) is distinct from `mixed` (anything). A union that exceeds the union cap
degrades literals first, then widens to `mixed`, and the type is flagged **widened**.

## Inference

One function-like at a time (functions, methods, closures, arrow functions and `{main}`), by
abstract interpretation over the owned syntax tree.

- A call's type is the callee's **declared** return type. Callee bodies are never inferred.
- Branches join by union; a variable assigned on one path only is united with `unknown`.
  `try` joins conservatively. Loops iterate until the loop-head state stops growing.
- Closures are separate units starting from the captured state; arrow functions capture all.
- By-reference arguments take the parameter's declared type; `$a[k] = v` and `$a[] = v` update the
  array type of a simple variable.

```php expect
<?php
function f(?string $s, bool $c) {
    if ($s === null) { return; }
    $s;
    // expect $s => string
    if ($c) { $x = 'a'; } else { $x = null; }
    $x;
    // expect $x => 'a'|null
    $n = 1 + 2;
    // expect $n => 3
}
```

### Narrowing subset

Only these conditions narrow, and only for simple variables (`$x`):

| Condition | True branch | False branch |
| --- | --- | --- |
| `$x instanceof C` | `C` objects | the type without `C` |
| `$x === null`, `== null`, `is_null($x)` | `null` | without `null` |
| `$x !== null`, `!= null` | without `null` | `null` |
| `is_string`, `is_int`, `is_float`, `is_bool`, `is_array`, `is_object`, `is_callable`, `is_numeric` | that kind | without it |
| `isset($x)` | without `null` | unchanged |
| `empty($x)` | falsy part | truthy part |
| `$x === <literal>` (string, int, bool, null) | the literal | without it when the type is that literal |
| `!c`, `a && b`, `a \|\| b`, `and`, `or` | combination of the above | |

Everything else (property fetches, array elements, user functions, bare truthiness) leaves both
branches unchanged.

```php expect
<?php
class Foo {} class Bar {}
function g(Foo|Bar|null $x) {
    if ($x instanceof Foo) { $x; }
    // expect $x => Foo
}
```

## Call resolution

Every call, `new` and first-class callable gets a resolution:

- **resolved**: one or more targets (function, method, constructor, closure). A method on an
  interface or abstract method lists the **implementors** (up to the candidate limit; the list may
  include abstract classes). Parts that could not be resolved are kept as residual reasons.
- **dynamic**: the callee is computed at run time (`variable_function`, `variable_method`,
  `variable_class`, `callable_string`).
- **unresolved**, with a reason: `unknown_receiver_type`, `class_not_loaded`, `method_not_found`,
  `magic_method`, `not_accessible`, `function_not_found`, `dynamic_name`, `limit_hit`.

Per function, `InferStats` counts expressions, those typed `unknown`, widened types, resolved /
unresolved / dynamic calls, loop-limit hits, and whether the depth or step limit was hit.

### Mapping to confidence reasons

The analysis does not depend on the findings crate; consumers map facts to `ReasonCode`:

| Fact | Reason |
| --- | --- |
| a type needed is `unknown`; a call is `unresolved` for any reason except `limit_hit` | `type_unknown` |
| a resolved call with several targets (implementors, union receivers) | `dynamic_dispatch_several_candidates` |
| a callee is `unresolved` or `dynamic` and taint is assumed to pass through | `unresolved_callee_assumed_propagate` |
| `limit_hit`, a widened type, a loop/depth/step limit, an incomplete table | `analysis_widened` |
| the file's tree had error nodes | `parse_recovered` |

## Limits and provisional defaults

These are defaults chosen without measurement and are listed for review.

| Limit | Default | Where |
| --- | --- | --- |
| Union cap (atoms before widening to `mixed`) | 64 | `DEFAULT_UNION_CAP` |
| String literal cap (longer degrades to `non-empty-string`) | 256 bytes | `DEFAULT_LITERAL_CAP` |
| Loop passes before widening | 3 | `InferLimits::loop_passes` |
| Expression/statement depth | 128 | `InferLimits::max_depth` |
| Node visits per function | 200 000 | `InferLimits::step_budget` |
| Implementors listed per target | 32 | `InferLimits::max_candidates` |
| Closure nesting | 8 | `InferLimits::max_closure_depth` |
| Vendor files loaded | 5 000 | `LoadLimits::max_files` |
| Vendor load depth | 16 | `LoadLimits::max_depth` |

Debug builds use a lot of stack per level; callers on small stacks should lower `max_depth`.

## Built-in data

`Builtins::load(source, version)` reads `builtin.data` (see [builtin-data.md](builtin-data.md))
for one PHP version and yields a summary that is added to the table under the path `<builtin>`.
Declared types of built-ins are native; docblock types in the data refine them by the same
merge rule. Symbols absent in the chosen version are unavailable, with availability recorded.

## Limitations

- Only variables are flow-tracked: property fetches and array elements read declared types.
- No aliasing beyond by-reference arguments; `@var` on locals is not read.
- Anonymous class bodies are not analysed.
- Template parameters of callees are not inferred from arguments; unbound templates are `unknown`.
- Class-string and dynamic calls resolve only when the value is a known literal or `class-string<T>`.
