//! Name resolution inside bodies, nested declarations, budgets and loop limits.

mod common;
use common::*;

/// spec: type-inference/Every expression receives a type
#[test]
fn declarations_nested_in_statements_are_analysed() {
    let src = "
        function outer($c) {
            if ($c) { function in_if() { return 1; } } elseif ($c) { function in_elseif() { return 1; } } else { function in_else() { return 1; } }
            while ($c) { function in_while() { return 1; } }
            do { function in_do() { return 1; } } while ($c);
            for (;;) { function in_for() { return 1; } }
            foreach ([1] as $x) { function in_foreach() { return 1; } }
            switch ($c) { case 1: function in_case() { return 1; } }
            try { function in_try() { return 1; } } catch (Exception $e) { function in_catch() { return 1; } } finally { function in_finally() { return 1; } }
            { function in_block() { return 1; } }
            declare(ticks=1) { function in_declare() { return 1; } }
            class InFn { function m() { return 1; } }
        }
        namespace N { function in_ns() { return 1; } }";
    let facts = infer(src);
    let names: Vec<String> = facts.iter().map(|f| f.name.to_string()).collect();
    for expected in [
        "outer",
        "in_if",
        "in_elseif",
        "in_else",
        "in_while",
        "in_do",
        "in_for",
        "in_foreach",
        "in_case",
        "in_try",
        "in_catch",
        "in_finally",
        "in_block",
        "in_declare",
        "InFn::m",
        "N\\in_ns",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "{expected} missing from {names:?}"
        );
    }
}

/// spec: type-inference/Every expression receives a type
#[test]
fn only_classes_have_a_parent_for_parent_calls() {
    let src = "
        class Base { function who(): string { return 'b'; } }
        class Kid extends Base { function t() { return parent::who(); } }
        interface Iface extends Base { function t(); }
        trait Tr { function t() { return parent::who(); } }";
    let facts = infer(src);
    assert_eq!(
        ty_last(unit(&facts, "Kid::t"), src, "parent::who()"),
        "string"
    );
    assert_eq!(
        ty_last(unit(&facts, "Tr::t"), src, "parent::who()"),
        "unknown"
    );
}

/// spec: type-inference/Every expression receives a type
#[test]
fn late_static_and_self_references_resolve_inside_classes() {
    let src = "
        class P { const K = 5; public static function make(): static { return new static(); }
          function t() { return [static::K, self::K, static::make(), new static(), new self()]; } }
        function free() { return static::K; }";
    let facts = infer(src);
    let t = unit(&facts, "P::t");
    assert_eq!(ty_last(t, src, "static::K"), "5");
    assert_eq!(ty_last(t, src, "self::K"), "5");
    assert_eq!(ty_last(t, src, "static::make()"), "static(P)");
    assert_eq!(ty_last(t, src, "new static()"), "static(P)");
    assert_eq!(ty_last(t, src, "new self()"), "P");
    assert_eq!(ty_last(unit(&facts, "free"), src, "static::K"), "unknown");
}

/// spec: type-inference/Every expression receives a type
#[test]
fn unqualified_functions_and_constants_fall_back_to_the_global_ones() {
    let src = "
        namespace { function glob(): string { return ''; } const GK = 5; function both(): int { return 1; } const BK = 1; }
        namespace N {
            function both(): string { return ''; }
            const BK = 's';
            function local(): float { return 1.5; }
            const LK = true;
            function t() { return [glob(), both(), local(), \\both(), GK, BK, LK, \\BK]; }
        }";
    let facts = infer(src);
    let t = unit(&facts, "N\\t");
    assert_eq!(ty_nth(t, src, "glob()", 0), "string");
    assert_eq!(ty_nth(t, src, "both()", 0), "string");
    assert_eq!(ty_nth(t, src, "local()", 0), "float");
    assert_eq!(ty_nth(t, src, "\\both()", 0), "int");
    assert_eq!(ty_nth(t, src, "GK", 0), "5");
    assert_eq!(ty_nth(t, src, "BK", 0), "'s'");
    assert_eq!(ty_nth(t, src, "LK", 0), "true");
    assert_eq!(ty_nth(t, src, "\\BK", 0), "1");
}

/// spec: type-inference/Every expression receives a type
#[test]
fn echo_const_and_global_statements_type_their_expressions() {
    let src =
        "const TOP = 5; function f() { echo 1 + 2, 'a' . 'b'; global $g; static $n = 3; $v = $g; }";
    let facts = infer(src);
    let m = unit(&facts, "{main}");
    assert_eq!(ty_last(m, src, "5"), "5");
    let f = unit(&facts, "f");
    assert_eq!(ty_last(f, src, "1 + 2"), "3");
    assert_eq!(ty_last(f, src, "'a' . 'b'"), "'ab'");
    assert_eq!(ty_last(f, src, "$g"), "unknown");
    assert_eq!(ty_last(f, src, "$v = $g"), "unknown");
}

/// spec: type-inference/Inference terminates and is bounded
#[test]
fn the_step_budget_stops_work_and_is_reported() {
    let src = "function f($c) { $a = 1; $b = 2; $d = 3; if ($c) { $a = 4; } return $a; }";
    let table = table(src);
    let run = |budget: usize| {
        let limits = InferLimits {
            step_budget: budget,
            ..InferLimits::default()
        };
        let facts = infer_with(src, &table, &limits);
        let f = unit(&facts, "f");
        (f.stats.budget_exhausted, ty_last(f, src, "$a = 1"))
    };
    // find the smallest budget that is enough, then pin the boundary
    let enough = (1..200)
        .find(|b| !run(*b).0)
        .expect("some budget is enough");
    assert_eq!(enough, ENOUGH, "steps needed");
    assert_eq!(run(enough), (false, "1".to_string()));
    assert!(run(enough - 1).0);
    assert_eq!(run(0).1, "unknown");
    assert_eq!(run(1_000_000), (false, "1".to_string()));
}
const ENOUGH: usize = 16;

/// spec: type-inference/Inference terminates and is bounded
#[test]
fn loops_that_do_not_converge_are_widened_and_counted() {
    let src =
        "function f($c) { $x = 0; $y = 'a'; while ($c) { $x = [$x]; $y = $y . 'b'; } $r = $x; }";
    let table = table(src);
    let few = InferLimits {
        loop_passes: 2,
        ..InferLimits::default()
    };
    let facts = infer_with(src, &table, &few);
    let f = unit(&facts, "f");
    assert_eq!(f.stats.loop_limit_hits, 1);
    assert_eq!(ty_last(f, src, "$r = $x"), "mixed");
    // growth without end never converges, however many passes are allowed
    let roomy = InferLimits {
        loop_passes: 50,
        ..InferLimits::default()
    };
    let facts = infer_with(src, &table, &roomy);
    assert_eq!(unit(&facts, "f").stats.loop_limit_hits, 1);
    let calm = "function g($c) { $x = 1; while ($c) { $x = 's'; } }";
    let calm_table = self::table(calm);
    for (passes, hits) in [(0, 1), (1, 1), (2, 0), (3, 0), (50, 0)] {
        let limits = InferLimits {
            loop_passes: passes,
            ..InferLimits::default()
        };
        let facts = infer_with(calm, &calm_table, &limits);
        let stats = &unit(&facts, "g").stats;
        assert_eq!(stats.loop_limit_hits, hits, "passes {passes}");
    }
}

/// spec: type-inference/Every expression receives a type
#[test]
fn a_redeclared_function_keeps_its_own_parameters() {
    let src = "
        /** @param int $a */
        function dup($a) { return $a; }
        function dup(int $a, string $b) { return [$a, $b]; }";
    let facts = infer(src);
    let second = facts.iter().filter(|f| &*f.name == "dup").nth(1).unwrap();
    assert_eq!(ty_last(second, src, "$b"), "string");
    assert_eq!(ty_last(second, src, "$a"), "int");
}

/// spec: type-inference/Every expression receives a type
#[test]
fn closure_signatures_mark_optional_variadic_and_nullable_defaults() {
    let src = "function k() {
        $c = function ($a, $b = 1, ...$r) {};
        $d = function (int $y = null, $x = null, ?string $z = 'q'): int { return 1; };
        $e = function ($q) use (&$c) {};
    }";
    let facts = infer(src);
    let k = unit(&facts, "k");
    assert_eq!(
        ty_last(k, src, "$c"),
        "callable(unknown, unknown=, ...unknown=): unknown"
    );
    assert_eq!(
        ty_last(k, src, "$d"),
        "callable(int|null=, unknown=, string|null=): int"
    );
    assert_eq!(ty_last(k, src, "$e"), "callable(unknown): unknown");
}

/// spec: type-inference/Assignments and operators are typed
#[test]
fn element_write_bases_are_evaluated_without_recording_twice() {
    let src = "function f() { $i = 0; $a = []; $a[$i++] = 1; $b = [[]]; $b[0][$i++] = 2; }";
    let facts = infer(src);
    let f = unit(&facts, "f");
    assert_eq!(ty_nth(f, src, "$i++", 0), "0");
    assert_eq!(ty_nth(f, src, "$i++", 1), "int");
    let closures = "function g() { $a = []; $a[(function () { return 1; })()] = 5; $o = new stdClass; $o->p[(fn() => 2)()] = 6; }";
    let facts = infer(closures);
    let n = facts
        .iter()
        .filter(|f| f.name.starts_with("{closure}"))
        .count();
    assert_eq!(n, 2);
}

/// spec: type-inference/Every expression receives a type
#[test]
fn a_parenthesised_class_name_denotes_the_same_class() {
    let src = "
        class Q { const K = 5; function t() { return [new (Q), new (self), new (Missing), (Q)::K, (self)::K, (Missing)::K]; } }";
    let facts = infer(src);
    let t = unit(&facts, "Q::t");
    assert_eq!(ty_last(t, src, "new (Q)"), "Q");
    assert_eq!(ty_last(t, src, "new (self)"), "Q");
    assert_eq!(ty_last(t, src, "new (Missing)"), "Missing");
    assert_eq!(ty_last(t, src, "(Q)::K"), "5");
    assert_eq!(ty_last(t, src, "(self)::K"), "5");
    assert_eq!(ty_last(t, src, "(Missing)::K"), "unknown");
}
