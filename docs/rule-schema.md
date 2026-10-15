# Rule schema

Rules are TOML files under `rules/`, one or several rules per file, read in path order. A rule is
reviewable data: it names PHP symbols and tags, and contains no engine logic. The loader
(`bleedthrough-fs`) rejects unknown keys, wrong types and invalid values, and reports **every**
problem in a file as `file:line: key: message`. `rules check` (library entry point
`bleedthrough_fs::check_rules`, wired into the CLI by `scan-pipeline`) validates a directory
without analysing any code and reports the tool-error status when the rules cannot be used.

Every file starts with `schema = 1`. A file with a higher major version than the tool supports is
rejected with a message naming both versions. Additive keys do not raise the major version.

## Rule metadata

| Key | Required | Meaning |
|---|---|---|
| `id` | yes | `BT` and four digits. Unique, immutable once released, never reused; removed identifiers go into `rules/RETIRED.md` |
| `title`, `description`, `remediation` | yes | Text for people. A rule without remediation is rejected |
| `area` | no | Informational grouping, such as `injection` |
| `references` | no | URLs or identifiers |
| `cwe`, `owasp` | yes | Registered tags, `CWE-89` and `OWASP:2021:A03` (see [taxonomy.md](taxonomy.md)). Each category must be mapped from one of the weaknesses |
| `[[rule.owasp_extra]]` | no | A deliberate cross-mapping: `category` and a non-empty `reason` |
| `paranoid` | no | `true` for rules that run only in paranoid mode (default `false`) |
| `php_min`, `php_max` | no | `major.minor` bounds, inclusive |
| `pack` | no | The owning pack |
| `[rule.metrics]` | yes | The eight CVSS 3.1 base metrics, below |
| `[rule.taint]` or `[rule.pattern]` | yes, exactly one | The body |

### Severity

Severity is derived, never declared. A rule gives the eight CVSS 3.1 base metrics as words; a fixed
formula (`severity_scheme` 1) produces the score, the vector string and the level. A `score` or
`level` key is an unknown key.

| Metric key | Values |
|---|---|
| `attack_vector` | `network`, `adjacent`, `local`, `physical` |
| `attack_complexity` | `low`, `high` |
| `privileges_required` | `none`, `low`, `high` |
| `user_interaction` | `none`, `required` |
| `scope` | `unchanged`, `changed` |
| `confidentiality`, `integrity`, `availability` | `none`, `low`, `high` |

Levels: 0.0 info, 0.1 to 3.9 low, 4.0 to 6.9 medium, 7.0 to 8.9 high, 9.0 to 10.0 critical. The
result is the rule author's base-metric estimate for the class of weakness, not an official CVSS
assessment of a specific vulnerability. Severity does not change with confidence or with the flow.

## Symbols

Symbols are written as in PHP source, with their namespace, and are not resolved by the schema.

| Form | Meaning |
|---|---|
| `mysqli_query`, `\App\Db\run` | function |
| `PDO::quote`, `\App\Db::run` | static method |
| `PDO->query`, `PDO->query+` | instance method; `+` also matches subclasses |
| `new PDO` | constructor |
| `class Phar` | class (use patterns) |
| `$_GET`, `$_POST`, `$_COOKIE`, `$_FILES`, `$_REQUEST`, `$_SERVER`, `$_ENV`, `$_SESSION`, `$GLOBALS` | superglobal |

Arguments are selected by position counting from 1 (`2`) or by parameter name (`"query"`). In the
inputs of an instance-method propagator, `"this"` is the receiver.

## Sink classes

`sql`, `os-command`, `code-eval`, `file-path`, `file-inclusion`, `deserialization`, `xml-parsing`,
`outgoing-request`, `redirect-target`, `header`, `ldap-query`, `xpath-query`, `log`. The vocabulary
is closed; extending it means editing `crates/rules/src/symbol.rs`.

## Taint rules

`sources` lists symbols. Each `[[rule.taint.sinks]]` has a `symbol`, the vulnerable `args` and a
`class`; only taint reaching those arguments counts. Each `[[rule.taint.sanitizers]]` lists the
`clears` classes it neutralises taint for and no others. Each `[[rule.taint.propagators]]` lists the
`from` inputs that flow to the result.

```toml
# example: taint-rule
schema = 1

[[rule]]
id = "BT0001"
title = "SQL injection through mysqli"
area = "injection"
description = "Request data reaches an SQL query built by string concatenation."
remediation = "Use prepared statements with bound parameters."
references = ["https://owasp.org/Top10/2021/A03_2021-Injection/"]
cwe = ["CWE-89"]
owasp = ["OWASP:2021:A03"]
php_min = "7.4"

[rule.metrics]
attack_vector = "network"
attack_complexity = "low"
privileges_required = "none"
user_interaction = "none"
scope = "unchanged"
confidentiality = "high"
integrity = "high"
availability = "high"

[rule.taint]
sources = ["$_GET", "$_POST", "$_COOKIE"]

[[rule.taint.sinks]]
symbol = "mysqli_query"
args = [2]
class = "sql"

[[rule.taint.sinks]]
symbol = "mysqli->query+"
args = [1]
class = "sql"

[[rule.taint.sanitizers]]
symbol = "mysqli_real_escape_string"
clears = ["sql"]

[[rule.taint.propagators]]
symbol = "trim"
from = [1]
```

## Pattern rules

A pattern rule matches one closed shape and never contains code:

- `call`: a call to `symbol`, optionally with `[[rule.pattern.args]]` conditions (`arg`, `test`,
  `value`). Tests: `literal`, `non-literal`, `absent`, `equals`, `matches`.
- `use`: any use of the named function, class or construct.
- `include-non-literal`: an `include` or `require` whose path is not a literal.

Regular expressions in `matches` are limited to a linear-time subset: literals, `.`, character
classes, `^` and `$`, escapes, and a quantifier (`*`, `+`, `?`, `{m}`, `{m,n}` up to 1000) on one
atom. Groups, alternation, back-references and look-around are rejected.

```toml
# example: pattern-rule
schema = 1

[[rule]]
id = "BT0002"
title = "Use of eval"
description = "eval executes a string as PHP code."
remediation = "Remove eval; use a data format and a parser instead."
cwe = ["CWE-95"]
owasp = ["OWASP:2021:A03"]
paranoid = true

[rule.metrics]
attack_vector = "network"
attack_complexity = "high"
privileges_required = "none"
user_interaction = "none"
scope = "unchanged"
confidentiality = "high"
integrity = "high"
availability = "high"

[rule.pattern]
shape = "use"
symbol = "eval"

[[rule]]
id = "BT0003"
title = "Cookie without a restricted name"
description = "A cookie is set with a name that is not a plain literal."
remediation = "Use a fixed cookie name."
cwe = ["CWE-1004"]
owasp = ["OWASP:2021:A05"]

[rule.metrics]
attack_vector = "network"
attack_complexity = "low"
privileges_required = "none"
user_interaction = "required"
scope = "unchanged"
confidentiality = "low"
integrity = "none"
availability = "none"

[rule.pattern]
shape = "call"
symbol = "setcookie"

[[rule.pattern.args]]
arg = 1
test = "matches"
value = "^[A-Za-z_]{1,32}$"
```

## Determinism and the content hash

The loaded set is ordered by identifier whatever order the files are read in. Its content hash is
the SHA-256 of a canonical form of the validated rules: file paths, comments, key order and the
order of items inside a rule do not affect it. The hash is reported in the run options and enters
cache keys.
