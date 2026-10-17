//! Expression forms: every kind gets a type, and its children are typed along the way.

mod common;
use common::*;

const CLASSES: &str = "
class P { public int $n = 0; public ?P $next = null; public static int $count = 0; const K = 5; function m(): string { return 's'; } }
const LIMIT = 7;
function nv(): never { exit; }
";

fn run(body: &str) -> (String, Vec<FunctionFacts>) {
    let src = format!("{CLASSES} function f(P $p, $name) {{ {body} }}");
    let facts = infer(&src);
    (src, facts)
}

fn tys(body: &str, needles: &[(&str, &str)]) {
    let (src, facts) = run(body);
    let f = unit(&facts, "f");
    for (needle, expected) in needles {
        assert_eq!(ty_last(f, &src, needle), *expected, "{needle}");
    }
}

/// spec: type-inference/Every expression receives a type
#[test]
fn names_constants_and_class_references_are_typed() {
    tys(
        "$b = P::K; $lim = LIMIT; $t = TRUE; $v = FALSE; $w = NULL; $unk = NOPE;",
        &[
            ("P", "class-string<P>"),
            ("P::K", "5"),
            ("LIMIT", "7"),
            ("TRUE", "true"),
            ("FALSE", "false"),
            ("NULL", "null"),
            ("NOPE", "unknown"),
        ],
    );
    let src = "class Q { function m() { return [__CLASS__, static::class, self::class]; } }
        function g() { return __CLASS__; }";
    let facts = infer(src);
    let m = unit(&facts, "Q::m");
    assert_eq!(ty_last(m, src, "__CLASS__"), "class-string<Q>");
    assert_eq!(ty_last(m, src, "static"), "class-string<Q>");
    assert_eq!(ty_last(m, src, "self"), "class-string<Q>");
    assert_eq!(ty_last(unit(&facts, "g"), src, "__CLASS__"), "class-string");
}

/// spec: type-inference/Every expression receives a type
#[test]
fn members_and_variable_variables_type_their_parts() {
    tys(
        "$name = 'x'; $a = $$name; $c = $p->n; $d = P::$count; $e = $p->next?->n; $f = $p->next;",
        &[
            ("$p->n", "int"),
            ("P::$count", "int"),
            ("$count", "'count'"),
            ("$p->next?->n", "int|null"),
            ("$p->next", "P|null"),
            ("$$name", "unknown"),
        ],
    );
    let (src, facts) = run("$name = 'x'; $a = $$name;");
    assert_eq!(ty_nth(unit(&facts, "f"), &src, "$name", 1), "'x'");
}

/// spec: type-inference/Every expression receives a type
#[test]
fn wrappers_calls_and_strings_type_their_children() {
    tys(
        "$name = 'x'; $j = clone $p; $k = (1 + 2); $l = include 'x.php'; $m = eval('1');
         $o = `ls $name`; $q = \"a$name\"; $r = \"$name\"; $s = \"$name$name\"; $t = \"$name!\";",
        &[
            ("clone $p", "P"),
            ("(1 + 2)", "3"),
            ("include 'x.php'", "mixed"),
            ("'x.php'", "'x.php'"),
            ("eval('1')", "mixed"),
            ("'1'", "'1'"),
            ("`ls $name`", "false|string|null"),
            ("\"a$name\"", "non-empty-string"),
            ("\"$name\"", "string"),
            ("\"$name$name\"", "string"),
            ("\"$name!\"", "non-empty-string"),
        ],
    );
    let (src, facts) = run("$name = 'x'; $q = \"a$name\";");
    assert_eq!(ty_nth(unit(&facts, "f"), &src, "$name", 1), "'x'");
}

/// spec: type-inference/Every expression receives a type
#[test]
fn anonymous_classes_yield_pipes_and_closures_are_typed() {
    let src = "function gen() { yield 1; yield 'k' => 2; $z = yield; yield from [1]; }
        function f() { $g = new class(1) { public $z = 1; }; $pp = 5 |> strlen(...); $h = function ($x) use ($g) { return $x; }; $i = fn($y) => $y + 1; }";
    let facts = infer(src);
    let g = unit(&facts, "gen");
    assert_eq!(ty_last(g, src, "yield 1"), "mixed");
    assert_eq!(ty_last(g, src, "'k'"), "'k'");
    assert_eq!(ty_last(g, src, "2"), "2");
    assert_eq!(ty_last(g, src, "yield"), "mixed");
    assert_eq!(ty_last(g, src, "yield from [1]"), "mixed");
    assert_eq!(ty_last(g, src, "[1]"), "non-empty-list<1>");
    let f = unit(&facts, "f");
    assert_eq!(ty_last(f, src, "new class(1) { public $z = 1; }"), "object");
    assert_eq!(ty_last(f, src, "5 |> strlen(...)"), "unknown");
    assert_eq!(ty_last(f, src, "5"), "5");
    assert_eq!(ty_last(f, src, "strlen(...)"), "callable");
    assert_eq!(
        ty_last(f, src, "fn($y) => $y + 1"),
        "callable(unknown): unknown"
    );
    assert!(ty_last(f, src, "function ($x) use ($g) { return $x; }").starts_with("callable("),);
    let names: Vec<_> = facts.iter().map(|f| f.name.to_string()).collect();
    assert_eq!(
        names.iter().filter(|n| n.starts_with("{closure}@")).count(),
        2,
        "{names:?}"
    );
}

/// spec: type-inference/Flow-sensitivity with joins and bounded loops
#[test]
fn throw_and_exit_make_the_state_unreachable() {
    let (src, facts) = run(
        "$x = 1; if ($name) { $x = 's'; throw new Exception(); } $r = $x;
         $y = 1; if ($name) { $y = 's'; exit(1); } $q = $y;",
    );
    let f = unit(&facts, "f");
    assert_eq!(ty_last(f, &src, "$r = $x"), "1");
    assert_eq!(ty_last(f, &src, "$q = $y"), "1");
    assert_eq!(ty_last(f, &src, "throw new Exception()"), "never");
    assert_eq!(ty_last(f, &src, "exit(1)"), "never");
    assert_eq!(ty_last(f, &src, "1"), "1");
}

/// spec: type-inference/Assignments and operators are typed
#[test]
fn assignment_targets_are_typed_and_destructured() {
    tys(
        "$p->n = 5; P::$count = 6; [$aa, $bb] = [1, 's']; ['k' => $cc] = ['k' => 5]; ($dd) = 7;
         [$gg, [$hh]] = [1, [2]]; $ee = &$ff;",
        &[
            ("$p->n = 5", "5"),
            ("P::$count = 6", "6"),
            ("$aa", "1|'s'"),
            ("$cc", "5"),
            ("$dd", "7"),
            ("($dd)", "7"),
            ("$hh", "2|null"),
            ("$ee", "unknown"),
        ],
    );
    let (src, facts) =
        run("['k' => $cc, 'j' => $dd] = ['k' => 5, 'j' => 's']; $r = [$cc]; $q = [$dd];");
    let f = unit(&facts, "f");
    assert_eq!(ty_last(f, &src, "$cc"), "5");
    assert_eq!(ty_last(f, &src, "$dd"), "'s'");
}

/// spec: type-inference/Assignments and operators are typed
#[test]
fn by_reference_destructuring_and_foreach_targets() {
    let (src, facts) = run("foreach ([1, 2] as &$v) { $a = $v; } [&$w, $z] = [1, 2]; $b = $w;");
    let f = unit(&facts, "f");
    assert_eq!(ty_last(f, &src, "$a = $v"), "1|2");
    assert_eq!(ty_last(f, &src, "$b = $w"), "1|2");
}

/// spec: type-inference/Assignments and operators are typed
#[test]
fn element_writes_start_from_the_current_value() {
    let (src, facts) =
        run("$xs[] = 1; $r = $xs; ($ys) = 5; ($ys) += 2; $q = $ys; $zs = 1; ($zs)++; $t = $zs;");
    let f = unit(&facts, "f");
    assert_eq!(ty_last(f, &src, "$r = $xs"), "array{0: 1}");
    assert_eq!(ty_last(f, &src, "$q = $ys"), "7");
    assert_eq!(ty_last(f, &src, "$t = $zs"), "int");
}

/// spec: type-inference/Assignments and operators are typed
#[test]
fn bitwise_not_of_strings_and_nothing() {
    tys(
        "$s = 'x'; $a = ~$s; $b = ~5; $c = ~nv();",
        &[("~$s", "string"), ("~5", "int"), ("~nv()", "int")],
    );
}

/// spec: type-inference/Inference terminates and is bounded
#[test]
fn closure_nesting_is_bounded_by_the_limit() {
    let src = "function f() { $a = function () { $b = function () { $c = function () { return 1; }; }; }; }";
    let table = build_table(src);
    let count = |max: usize| {
        let limits = InferLimits {
            max_closure_depth: max,
            ..InferLimits::default()
        };
        infer_with(src, &table, &limits).len()
    };
    // `{main}` and `f`, plus one unit per closure level that fits
    assert_eq!(count(0), 2);
    assert_eq!(count(1), 3);
    assert_eq!(count(2), 4);
    assert_eq!(count(3), 5);
    assert_eq!(count(8), 5);
}

fn build_table(src: &str) -> SymbolTable {
    table(src)
}

const ITERATION: &str = "
interface Traversable {} interface Iterator extends Traversable {}
class Plain { function key(): int { return 1; } function current(): string { return 's'; } }
class Half { function key(): int { return 1; } }
class Agg { function getIterator(): Plain { return new Plain(); } }
class Agg2 { function getIterator(): Agg { return new Agg(); } }
class Agg3 { function getIterator(): Agg2 { return new Agg2(); } }
class Agg4 { function getIterator(): Agg3 { return new Agg3(); } }
class Agg5 { function getIterator(): Agg4 { return new Agg4(); } }
class Both { function getIterator(): Plain|int { return new Plain(); } }
class Unk { function key() {} function current() {} }
/**
 * @param Iterator<string> $one
 * @param Iterator<int, bool> $two
 * @param Iterator $none
 * @param Iterator<int, bool, float> $three
 */
function g($one, $two, $none, $three, Plain $p, Half $h, Agg $a, Agg2 $a2, Agg3 $a3, Agg4 $a4, Agg5 $a5, Both $b, Unk $u, Gone $gone, int $i) {
";

/// spec: type-inference/Every expression receives a type
#[test]
fn iterating_objects_uses_generics_accessors_and_iterators() {
    let cases: &[(&str, &str, &str)] = &[
        ("$one", "mixed", "string"),
        ("$two", "int", "bool"),
        ("$three", "int", "bool"),
        ("$none", "unknown", "unknown"),
        ("$p", "int", "string"),
        ("$h", "unknown", "unknown"),
        ("$a", "int", "string"),
        ("$a2", "int", "string"),
        ("$a3", "int", "string"),
        ("$a4", "unknown", "unknown"),
        ("$a5", "unknown", "unknown"),
        ("$b", "unknown|int", "unknown|string"),
        ("$u", "unknown", "unknown"),
        ("$gone", "unknown", "unknown"),
        ("$i", "unknown", "unknown"),
    ];
    let body: String = cases
        .iter()
        .enumerate()
        .map(|(n, (var, _, _))| {
            format!("foreach ({var} as $k{n} => $v{n}) {{ $ka{n} = $k{n}; $va{n} = $v{n}; }}\n")
        })
        .collect();
    let src = format!("{ITERATION} {body} }}");
    let facts = infer(&src);
    let g = unit(&facts, "g");
    for (n, (var, key, value)) in cases.iter().enumerate() {
        assert_eq!(
            ty_last(g, &src, &format!("$ka{n} = $k{n}")),
            *key,
            "key of {var}"
        );
        assert_eq!(
            ty_last(g, &src, &format!("$va{n} = $v{n}")),
            *value,
            "value of {var}"
        );
    }
}

/// spec: type-inference/Inference terminates and is bounded
#[test]
fn nesting_depth_is_bounded_but_sequences_are_not() {
    let sequence: String = (0..40).map(|i| format!("$v{i} = {i};")).collect();
    let src = format!("function f() {{ {sequence} }}");
    let tight = InferLimits {
        max_depth: 6,
        ..InferLimits::default()
    };
    let facts = infer_with(&src, &table(&src), &tight);
    let f = unit(&facts, "f");
    assert!(!f.stats.depth_limit_hit);
    assert_eq!(ty_last(f, &src, "$v39 = 39"), "39");

    let deep = format!(
        "function g() {{ $x = {}1{}; }}",
        "(".repeat(10),
        ")".repeat(10)
    );
    let facts = infer_with(&deep, &table(&deep), &tight);
    let g = unit(&facts, "g");
    assert!(g.stats.depth_limit_hit);
    assert_eq!(ty_last(g, &deep, "1"), "unknown");
    let roomy = InferLimits {
        max_depth: 40,
        ..InferLimits::default()
    };
    let facts = infer_with(&deep, &table(&deep), &roomy);
    assert!(!unit(&facts, "g").stats.depth_limit_hit);
    assert_eq!(ty_last(unit(&facts, "g"), &deep, "1"), "1");
}

/// spec: type-inference/Assignments and operators are typed
#[test]
fn property_and_static_property_targets_record_the_assigned_type() {
    let (src, facts) = run("$p->n = 5; P::$count = 6;");
    let f = unit(&facts, "f");
    assert_eq!(ty_last(f, &src, "$p->n"), "5");
    assert_eq!(ty_last(f, &src, "P::$count"), "6");
    assert_eq!(ty_last(f, &src, "$count"), "'count'");
    let (src, facts) = run("$p->$name = 5;");
    assert_eq!(ty_last(unit(&facts, "f"), &src, "$name"), "unknown");
}

/// spec: type-inference/Assignments and operators are typed
#[test]
fn a_reference_marked_operand_is_updated_like_the_variable() {
    let (src, facts) = run("$x = 1; ++&$x; $r = $x;");
    assert_eq!(ty_last(unit(&facts, "f"), &src, "$r = $x"), "int");
}
