# Symbol fixtures

PHP files that exercise name resolution, file summaries, the symbol table and inference, each
with a sibling `*.txt` **golden dump** of what the code under test produces for it.

```
fixtures/symbols/summary/   one file -> its FileSummary (see below)
fixtures/types/             docblock and native type scenarios
```

## Golden format

Dumps are plain text, one fact per line, with no positions of unrelated code, so they diff well:

```
file <name>
incomplete                       only when the tree had error or unsupported nodes
class <Fqn> [abstract final readonly]
  extends <Fqn> / implements <Fqn>, ... / backed by <type>
  use <Trait>, ...               trait uses, adaptations indented below
  const NAME [visibility final]: <type>
  case Name = <type>
  property $name [visibility static readonly default] <origin>: <type> (<source>)
  method name(<params>) -> <type> (<source>) [flags] <origin>
function <Fqn>(<params>) -> <type> (<source>)
constant <Fqn> [const|define]: <type>
references classes: ...          sorted, distinct
references functions: a | b      namespaced candidate | global fallback
diagnostic <code> <start>..<end>: <message>
```

`<source>` is where an effective type came from: `native`, `docblock` or `absent`.

## Updating goldens

`BLEEDTHROUGH_BLESS=1 cargo test -p bleedthrough-syntax-mago --test symbols` rewrites the `*.txt`
files from the current output. Review the diff before committing: a golden change is a behaviour
change.
