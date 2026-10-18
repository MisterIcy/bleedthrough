# Engine: IR, taint analysis and findings

The engine turns parsed PHP into findings. It lives in two domain crates: `bleedthrough-ir` (lowering)
and `bleedthrough-engine` (call graph, taint analysis, pattern matching, finding production). Neither
touches a file system, a parser or a clock: lowering reads the owned syntax tree and the symbol facts,
and the engine reads lowered graphs, the symbol table and a rule index. Nothing in the product calls
the engine until the `scan-pipeline` change; tests drive it through `bleedthrough-engine-suite`.

## Intermediate representation

One control-flow graph per function-like unit: each function, method, closure, arrow function, and the
top level of a file. A graph has blocks of instructions in a fixed order, terminators that carry the
edges (branch, switch, loop, exception, return), and per-value type classes. Instructions carry what the
analysis needs and no more: assignments, reads and writes of locals and properties, calls with their
resolution (the candidate declarations from the symbol table, or the reason none was found), concat and
interpolation as explicit instructions, casts, tests, includes, and an *unknown* instruction for
constructs the model does not interpret (it lists the variables inside, so nothing is dropped
silently).

Lowering is total over the syntax model, deterministic, independent of other files, and uses explicit
work stacks. Dump a graph with `bleedthrough_ir::dump` (the `ir-lowering` goldens are dumps).

## Taint model

- **Sources and sinks** come from the rule index: a source names a symbol (a superglobal, a function, a
  method); a sink names a symbol, the arguments that are vulnerable, and a *class* (`sql`,
  `os-command`, `file-path`, ...).
- **Labels.** A tainted value carries one label per origin (up to `max_origins`). A label holds the
  origin (a source occurrence, or a symbolic input while a summary is computed), the classes cleared by
  sanitisers, the sanitisers applied, the marks that lower confidence, and the steps the flow took
  (propagation, call, sanitiser; at most 64, the middle dropped when longer).
- **Flow-sensitive within a unit.** A forward dataflow over the graph joins labels at merges; a block is
  visited at most `block_visits` times.
- **Sanitisation is by class.** A sanitiser clears the classes the rule lists. A sanitiser for another
  class does not clear the sink's class; the flow is kept, and the finding notes the wrong sanitiser.
- **Type facts prune taint.** A value whose type is certainly numeric, boolean, null or an enum case
  carries no taint; a cast to `int`, `float` or `bool` clears all classes. A value of unknown type
  keeps its taint and is marked.
- **Summaries.** Each unit gets a summary: which parameters and sources reach the result, which
  parameters reach which sinks, and what is written to properties and globals. Call sites apply the
  summary of every candidate callee. Recursive groups (components of the call graph) are iterated up to
  `iterations` passes.
- **Properties, static properties and globals** are abstract locations, one per class and name, updated
  in rounds (up to `rounds`). A flow that depends on a location written elsewhere is marked as having an
  inferred source.
- **Includes** are followed only when the path folds to a literal that the include resolver maps to
  project files; other includes are counted as unresolved.

### Default call policy

A call to a symbol with no rule entry flows by the default policy:

| Call | Result |
| --- | --- |
| Rule sanitiser | clears the listed classes (takes precedence over everything below) |
| Rule propagator | taint flows from the listed inputs |
| Resolved, declared or inferred result type cannot carry strings, arrays or objects (`int`, `float`, `bool`, `void`) | untainted |
| Resolved otherwise | taint flows from every argument and the receiver |
| Unresolved | taint flows from every argument; the flow is marked *assumed unresolved* |

Known weaknesses, all pinned by tests or recorded in the corpus expectations:

- **Neutralising built-ins are not modelled.** A hash, encoding or numeric-producing built-in whose
  declared return type is `string` keeps the taint. Rules must list them as sanitisers; a shared model is
  a follow-up (`v1-rule-catalog`).
- **Arrays are element-collapsing and objects are field-insensitive**: tainting one key or field taints
  all of them. This over-taints; the corpus safe twins measure it.
- **Guards and sink options are not recognised.** A check such as `in_array($x, $allowed)` or
  `str_starts_with(...)`, or an option such as `allowed_classes => false`, does not clear a flow. The
  safe twins of CWE-502, 601 and 918 are reported (`adapters/syntax-mago/tests/engine.rs` pins this).
- **Include modelling approximates PHP scoping**: only literal-resolvable includes are followed, and
  their effects are limited to locals by name.
- **Dynamic features** (variable variables, `eval`, `extract`, dynamic class names) are not interpreted;
  taint through them is assumed or lost as the *unknown* instruction dictates, and calls on them are
  unresolved.

## Confidence

Every finding carries the reasons it is less than certain, from a closed vocabulary; the tier is the
lowest any reason implies.

| Reason | Tier |
| --- | --- |
| an unresolved callee was assumed to propagate | low |
| a file on the path was parsed with recovery | low |
| a limit widened the analysis before the flow was found | low |
| the source was inferred (a property or global written elsewhere) | medium |
| dynamic dispatch had several candidates (with the count) | medium |
| a value of unknown type kept its taint | medium |
| a sanitiser for a different class was applied | medium |
| none | high |

A by-name pattern match also carries the unresolved-callee reason; a pattern match in a recovered file
carries the recovery reason. Pattern findings have no trace.

Findings are de-duplicated per rule, source origin, sink unit and sink span, ordered by path, offset,
rule and fingerprint. The fingerprint hashes the rule, the enclosing symbol, the normalised sink and
source text and an occurrence index, so it survives unrelated edits.

## Limits

Every unbounded dimension has a limit. Reaching one widens conservatively (keeps taint, drops
precision), marks what depends on it with the *widened* reason, and appears in the report's coverage as
a limit hit with the kind below and the file. Defaults are **provisional**: they were chosen before
measurement and will be tuned from the first corpus and benchmark runs, in an edit to this table, not
silently. All are fields of `Limits`, `LowerConfig` or `CallGraphConfig`.

| Limit (field) | Default | Effect when reached | Reported kind |
| --- | --- | --- | --- |
| Expression and statement nesting (`LowerConfig::max_depth`) | 256 | the deeper node becomes one *unknown* instruction over the variables inside it | `expression_depth` |
| Blocks per unit (`LowerConfig::max_blocks`) | 20 000 | no further blocks are created: later code lowers into the last block | `block_limit` |
| Instructions per unit (`LowerConfig::max_instructions`) | 200 000 | later instructions are dropped | `instruction_limit` |
| Instruction transfers per unit (`Limits::step_budget`) | 2 000 000 | the unit's analysis stops; the hits found so far are kept and marked widened | `step_budget` |
| Visits per block (`Limits::block_visits`) | 32 | the block is not processed again; hits of the unit are marked widened | `block_visits` |
| Origins per value (`Limits::max_origins`) | 16 | the first origins in sorted order are kept, the rest dropped, and the value is marked widened | `origin_cap` |
| Passes over a recursive component (`Limits::iterations`) | 8 | the latest summaries are used and marked widened; a flow that needed more passes can still be missing | `iterations` |
| Units in a component iterated fully (`Limits::component_size`) | 512 | a larger component iterates twice, then is widened | `component_size` |
| Depth of closures analysed inside closures (`Limits::closure_depth`) | 4 | a deeper closure is not analysed; its value carries only what it captured | `summary_depth` |
| Dispatch candidates per call (`CallGraphConfig::max_candidates`) | 32 | the call keeps the first candidates in a fixed order and the call graph counts the truncation | `candidates` |
| Rounds of abstract locations (`Limits::rounds`) | 4 | the project result is marked not settled and the locations keep the taint of the last round | `iterations` |
| Steps in a trace (`Limits::trace_steps`) | 24 | the middle is cut and the trace is marked truncated | none (a trace mark) |

Each kind is reached by a test in `adapters/syntax-mago/tests/limits.rs`, together with the adversarial
cases: a thousand levels of nesting, a very long concatenation and assignment chain, a thousand
functions in a cycle, many blocks and a wide array. The mago parser itself returns an error node past
about a thousand nesting levels; those files are reported as recovered and the engine analyses what the
parser gave it.

## Work-unit API

Analysis is exposed as order-independent units so a scheduler can run them in parallel without
changing the result. `analyse_project` is the single-threaded driver; a parallel driver calls the same
four functions:

1. `lower_file` (in `bleedthrough-ir`): one file's units, a pure function of its tree and facts.
2. `ProjectView::new` and `summarise_component(component, done, heap)`: the summaries of one
   component of the call graph given the finished components below it. Components in the same layer of
   the condensation are independent.
3. `scan_unit(unit, flow)`: the sink hits and pattern matches of one unit.
4. `merge(results)`: combines unit results; it sorts everything canonically, so any order of inputs
   gives the same bytes.

An internal fault (a panic) in one unit is caught at the unit boundary and recorded as a skipped unit
with the reason `internal_fault`; the other units are unaffected. Workspace builds use
`panic = "unwind"` and must not use `abort`.

`adapters/syntax-mago/tests/determinism.rs` shuffles files and unit order and compares the findings
and coverage byte for byte.
