//! Local inference scenarios, run through the Mago backend.

use bleedthrough_symbols_suite::symbols::{FunctionFacts, InferLimits};
use bleedthrough_symbols_suite::{infer_source, table_from_sources};
use bleedthrough_syntax_mago::MagoBackend;

/// Infers `main` against a table made of `files` and `main` itself.
fn analyse_with(files: &[(&str, &str)], main: &str, limits: &InferLimits) -> Vec<FunctionFacts> {
    let backend = MagoBackend::new();
    let mut all: Vec<(&str, &str)> = files.to_vec();
    all.push(("main.php", main));
    let table = table_from_sources(&backend, &all);
    infer_source(&backend, &table, "main.php", main, limits)
}

fn analyse(main: &str) -> Vec<FunctionFacts> {
    analyse_with(&[], main, &InferLimits::default())
}

fn unit<'a>(facts: &'a [FunctionFacts], name: &str) -> &'a FunctionFacts {
    facts.iter().find(|f| &*f.name == name).unwrap_or_else(|| {
        panic!(
            "no unit {name}; have {:?}",
            facts.iter().map(|f| &f.name).collect::<Vec<_>>()
        )
    })
}

/// The types recorded for every expression whose source is exactly `text`, in source order.
fn types_of(f: &FunctionFacts, src: &str, text: &str) -> Vec<String> {
    f.expr_types
        .iter()
        .filter(|(k, _)| src.get(k.start as usize..k.end as usize) == Some(text))
        .map(|(_, t)| t.to_string())
        .collect()
}

/// The type of the last occurrence of `text`.
fn last(f: &FunctionFacts, src: &str, text: &str) -> String {
    types_of(f, src, text)
        .pop()
        .unwrap_or_else(|| panic!("no expression `{text}`"))
}

fn call(f: &FunctionFacts, src: &str, text: &str) -> String {
    f.calls
        .iter()
        .filter(|(k, _)| src.get(k.start as usize..k.end as usize) == Some(text))
        .map(|(_, r)| r.to_string())
        .next_back()
        .unwrap_or_else(|| panic!("no call `{text}`"))
}

#[test]
fn a_callee_body_is_never_inferred() {
    let src = "<?php\nfunction g() { return 'abc'; }\nfunction f() { $r = g(); $r; }\n";
    let facts = analyse(src);
    assert_eq!(last(unit(&facts, "f"), src, "$r"), "unknown");
}

#[test]
fn declared_return_types_flow_to_the_caller() {
    let src = "<?php\nfunction g(): int { return 1; }\nfunction f() { $r = g(); $r; }\n";
    let facts = analyse(src);
    assert_eq!(last(unit(&facts, "f"), src, "$r"), "int");
    assert_eq!(call(unit(&facts, "f"), src, "g()"), "resolved [function g]");
}

#[test]
fn new_gives_the_object_type() {
    let src = "<?php\nclass Connection {}\nfunction f() { $db = new Connection(); $db; }\n";
    let facts = analyse(src);
    let f = unit(&facts, "f");
    assert_eq!(last(f, src, "$db"), "Connection");
    assert_eq!(call(f, src, "new Connection()"), "resolved []");
}

#[test]
fn new_with_a_constructor_resolves_it() {
    let src = "<?php\nclass Conn { function __construct(string $dsn) {} }\nfunction f() { new Conn('x'); }\n";
    let facts = analyse(src);
    assert_eq!(
        call(unit(&facts, "f"), src, "new Conn('x')"),
        "resolved [constructor Conn::__construct]"
    );
}

#[test]
fn concatenation_is_a_string() {
    let src = "<?php\nfunction f(string $a, string $b) { $c = $a . $b; $c; $d = 'x' . 'y'; $d; }\n";
    let facts = analyse(src);
    let f = unit(&facts, "f");
    assert_eq!(last(f, src, "$c"), "string");
    assert_eq!(last(f, src, "$d"), "'xy'");
}

#[test]
fn literals_arrays_casts_and_arithmetic() {
    let src = "<?php\nfunction f(int $n, float $x) {\n $a = [1, 2]; $a;\n $b = ['id' => 1, 'name' => 'x']; $b;\n $c = (string) $n; $c;\n $d = $n + 1; $d;\n $e = $x * 2; $e;\n $g = 1 + 2; $g;\n $h = \"v=$n\"; $h;\n $i = (array) 5; $i;\n $j = $n <=> 1; $j;\n $k = !$n; $k;\n}\n";
    let facts = analyse(src);
    let f = unit(&facts, "f");
    assert_eq!(last(f, src, "$a"), "non-empty-list<1|2>");
    assert_eq!(last(f, src, "$b"), "array{id: 1, name: 'x'}");
    assert_eq!(last(f, src, "$c"), "string");
    assert_eq!(last(f, src, "$d"), "int");
    assert_eq!(last(f, src, "$e"), "float");
    assert_eq!(last(f, src, "$g"), "3");
    assert_eq!(last(f, src, "$h"), "non-empty-string");
    assert_eq!(last(f, src, "$i"), "non-empty-list<5>");
    assert_eq!(last(f, src, "$k"), "bool");
}

#[test]
fn null_coalescing_ternary_and_match() {
    let src = "<?php\nfunction f(?string $s, int $n) {\n $a = $s ?? 'd'; $a;\n $b = $n ? 'x' : 5; $b;\n $c = match ($n) { 1 => 'a', 2 => 'b', default => null }; $c;\n}\n";
    let facts = analyse(src);
    let f = unit(&facts, "f");
    assert_eq!(last(f, src, "$a"), "string");
    assert_eq!(last(f, src, "$b"), "5|'x'");
    assert_eq!(last(f, src, "$c"), "'a'|'b'|null");
}

#[test]
fn closures_capture_and_are_units_of_their_own() {
    let src = "<?php\nfunction f(int $n) {\n $name = 'x';\n $c = function (string $a) use ($name) { return $a . $name; };\n $c;\n $d = fn($z) => $z + $n;\n $d;\n}\n";
    let facts = analyse(src);
    assert!(facts.iter().any(|u| u.name.starts_with("{closure}@")));
    let closure = facts
        .iter()
        .find(|u| u.name.starts_with("{closure}@"))
        .unwrap();
    assert_eq!(last(closure, src, "$name"), "'x'");
    assert!(last(unit(&facts, "f"), src, "$c").starts_with("callable(string): unknown"));
}

#[test]
fn branches_join_by_union() {
    let src = "<?php\nfunction f(bool $c) {\n if ($c) { $x = 'a'; } else { $x = null; }\n $x;\n}\n";
    let facts = analyse(src);
    assert_eq!(last(unit(&facts, "f"), src, "$x"), "'a'|null");
}

#[test]
fn a_variable_set_on_one_path_may_be_undefined() {
    let src = "<?php\nfunction f(bool $c) {\n if ($c) { $x = 'a'; }\n $x;\n}\n";
    let facts = analyse(src);
    let t = last(unit(&facts, "f"), src, "$x");
    assert!(t.contains("unknown") && t.contains("'a'"), "{t}");
}

#[test]
fn a_guard_clause_narrows_the_rest() {
    let src = "<?php\nfunction f(?string $s) {\n if ($s === null) { return; }\n $s;\n}\n";
    let facts = analyse(src);
    assert_eq!(last(unit(&facts, "f"), src, "$s"), "string");
}

#[test]
fn loops_converge_for_counters_and_growing_strings() {
    let src = "<?php\nfunction f() {\n $i = 0; $s = 'a';\n while (cond()) { $i++; $s .= 'x'; }\n $i; $s;\n}\nfunction cond(): bool { return true; }\n";
    let facts = analyse(src);
    let f = unit(&facts, "f");
    assert_eq!(last(f, src, "$i"), "int");
    assert_eq!(last(f, src, "$s"), "non-empty-string");
    assert!(!f.stats.limit_hit());
}

#[test]
fn a_loop_that_does_not_converge_is_widened_and_recorded() {
    let src = "<?php\nfunction f() {\n $a = 1; $b = 'x'; $c = null;\n while (cond()) { $t = $a; $a = $b; $b = $c; $c = $t; }\n $a;\n}\nfunction cond(): bool { return true; }\n";
    let limits = InferLimits {
        loop_passes: 1,
        ..InferLimits::default()
    };
    let facts = analyse_with(&[], src, &limits);
    let f = unit(&facts, "f");
    assert_eq!(f.stats.loop_limit_hits, 1);
    assert!(f.stats.limit_hit());
    assert!(last(f, src, "$a").contains("mixed"));
}

#[test]
fn foreach_types_elements_from_arrays() {
    let src = "<?php\n/** @param list<string> $names */\nfunction f(array $names, array $m) {\n foreach ($names as $i => $n) { $n; $i; }\n foreach ($m as $k => $v) { $v; }\n}\n";
    let facts = analyse(src);
    let f = unit(&facts, "f");
    assert_eq!(last(f, src, "$n"), "string");
    assert_eq!(last(f, src, "$i"), "int");
}

#[test]
fn switch_and_try_join_conservatively() {
    let src = "<?php\nfunction f(int $n) {\n switch ($n) { case 1: $x = 'a'; break; case 2: $x = 5; break; default: $x = null; }\n $x;\n try { $y = 1; mayThrow(); $y = 'two'; } catch (\\Exception $e) { $e; $y; }\n}\nfunction mayThrow() {}\n";
    let facts = analyse(src);
    let f = unit(&facts, "f");
    assert_eq!(last(f, src, "$x"), "5|'a'|null");
    let y = types_of(f, src, "$y");
    assert!(
        y.last()
            .is_some_and(|t| t.contains("'two'") && t.contains('1')),
        "{y:?}"
    );
    assert_eq!(last(f, src, "$e"), "Exception");
}

mod narrowing {
    use super::*;

    fn inside(cond: &str, param: &str) -> (String, String) {
        let src = format!(
            "<?php\nclass Foo {{}} class Bar {{}}\nfunction f({param}) {{\n if ({cond}) {{ $x; }} else {{ $x; }}\n}}\n"
        );
        let facts = analyse(&src);
        let f = unit(&facts, "f");
        let seen = types_of(f, &src, "$x");
        (seen[seen.len() - 2].clone(), seen[seen.len() - 1].clone())
    }

    #[test]
    fn instanceof() {
        assert_eq!(
            inside("$x instanceof Foo", "$x"),
            ("Foo".into(), "unknown".into())
        );
        assert_eq!(
            inside("$x instanceof Foo", "Foo|Bar|null $x"),
            ("Foo".into(), "Bar|null".into())
        );
        assert_eq!(
            inside("!($x instanceof Foo)", "Foo|int $x"),
            ("int".into(), "Foo".into())
        );
    }

    #[test]
    fn null_comparisons() {
        assert_eq!(
            inside("$x === null", "?string $x"),
            ("null".into(), "string".into())
        );
        assert_eq!(
            inside("$x !== null", "?string $x"),
            ("string".into(), "null".into())
        );
        assert_eq!(
            inside("null != $x", "?string $x"),
            ("string".into(), "string|null".into()).clone()
        );
    }

    #[test]
    fn type_tests() {
        assert_eq!(
            inside("is_string($x)", "string|int $x"),
            ("string".into(), "int".into())
        );
        assert_eq!(
            inside("is_int($x)", "string|int|null $x"),
            ("int".into(), "string|null".into())
        );
        assert_eq!(
            inside("is_array($x)", "$x"),
            ("array<array-key, mixed>".into(), "unknown".into())
        );
        assert_eq!(
            inside("is_null($x)", "?int $x"),
            ("null".into(), "int".into())
        );
        assert_eq!(
            inside("is_numeric($x)", "string|int $x"),
            ("int|numeric-string".into(), "string".into())
        );
    }

    #[test]
    fn isset_and_empty() {
        assert_eq!(
            inside("isset($x)", "?string $x"),
            ("string".into(), "null".into())
        );
        assert_eq!(
            inside("empty($x)", "?string $x"),
            ("string|null".into(), "non-empty-string".into())
        );
    }

    #[test]
    fn literal_comparison() {
        assert_eq!(
            inside("$x === 'a'", "string $x"),
            ("'a'".into(), "string".into())
        );
        assert_eq!(
            inside("$x === true", "bool $x"),
            ("true".into(), "false".into())
        );
    }

    #[test]
    fn conjunction_and_disjunction() {
        assert_eq!(
            inside("$x !== null && is_string($x)", "string|int|null $x"),
            ("string".into(), "int|null".into()).clone()
        );
        assert_eq!(
            inside("$x === null || $x === ''", "?string $x"),
            ("''|null".into(), "string".into()).clone()
        );
    }

    #[test]
    fn an_unrecognised_condition_changes_nothing() {
        assert_eq!(
            inside("validate($x)", "string|int $x"),
            ("int|string".into(), "int|string".into())
        );
        assert_eq!(
            inside("$x", "?string $x"),
            ("string|null".into(), "string|null".into())
        );
    }
}

#[test]
fn an_interface_receiver_lists_the_implementors() {
    let src = "<?php\ninterface Shape { function area(): float; }\nclass Sq implements Shape { function area(): float { return 1.0; } }\nclass Ci implements Shape { function area(): float { return 2.0; } }\nclass Tr implements Shape { function area(): float { return 3.0; } }\nfunction f(Shape $s) { $s->area(); }\n";
    let facts = analyse(src);
    assert_eq!(
        call(unit(&facts, "f"), src, "$s->area()"),
        "resolved [method Shape::area implementors(Ci, Sq, Tr)]"
    );
}

#[test]
fn an_unknown_receiver_is_unresolved_with_a_reason() {
    let src = "<?php\nfunction f($x) { $x->run(); }\n";
    let facts = analyse(src);
    assert_eq!(
        call(unit(&facts, "f"), src, "$x->run()"),
        "unresolved(unknown_receiver_type)"
    );
}

#[test]
fn unloaded_classes_missing_methods_and_magic_are_told_apart() {
    let src = "<?php\nclass Known { function ok() {} }\nclass Magic { function __call($n, $a) {} }\nfunction f(Known $k, Magic $m, Gone $g) { $k->nope(); $m->anything(); $g->x(); }\n";
    let facts = analyse(src);
    let f = unit(&facts, "f");
    assert_eq!(call(f, src, "$k->nope()"), "unresolved(method_not_found)");
    assert_eq!(call(f, src, "$m->anything()"), "unresolved(magic_method)");
    assert_eq!(call(f, src, "$g->x()"), "unresolved(class_not_loaded)");
}

#[test]
fn variable_functions_methods_and_classes_are_dynamic() {
    let src = "<?php\nfunction f($fn, $m, $o, $cls) { $fn(1); $o->$m(); new $cls(); }\n";
    let facts = analyse(src);
    let f = unit(&facts, "f");
    assert_eq!(call(f, src, "$fn(1)"), "dynamic(variable_function)");
    assert_eq!(call(f, src, "$o->$m()"), "dynamic(variable_method)");
    assert_eq!(call(f, src, "new $cls()"), "dynamic(variable_class)");
}

#[test]
fn known_closures_and_literal_names_are_not_dynamic() {
    let src = "<?php\nfunction target(): int { return 1; }\nfunction f() { $c = fn(): string => 'x'; $c(); $n = 'target'; $n(); }\n";
    let facts = analyse(src);
    let f = unit(&facts, "f");
    assert_eq!(call(f, src, "$c()"), "resolved [closure {closure}]");
    assert_eq!(call(f, src, "$n()"), "resolved [function target]");
    assert_eq!(last(f, src, "$c()"), "string");
}

#[test]
fn static_calls_and_late_static_binding() {
    let src = "<?php\nclass A { static function make(): static { return new static(); } function chain(): static { return $this; } }\nclass B extends A {}\nfunction f() { $b = B::make(); $b; $b->chain(); (new A)->chain(); }\n";
    let facts = analyse(src);
    let f = unit(&facts, "f");
    assert_eq!(last(f, src, "$b"), "B");
    assert_eq!(call(f, src, "B::make()"), "resolved [method A::make]");
}

#[test]
fn nullsafe_calls_add_null() {
    let src = "<?php\nclass U { function name(): string { return ''; } }\nfunction f(?U $u) { $n = $u?->name(); $n; $m = $u->name(); $m; }\n";
    let facts = analyse(src);
    let f = unit(&facts, "f");
    let ns = types_of(f, src, "$n");
    assert_eq!(ns.last().unwrap(), "string|null");
    assert_eq!(last(f, src, "$m"), "string");
}

#[test]
fn properties_and_constants_use_declarations() {
    let src = "<?php\nenum Suit: string { case Hearts = 'h'; const Wild = 1; }\nclass C { public int $n = 0; const MAX = 5; public ?string $s = null; static function s(): int { return self::MAX; } }\nfunction f(C $c) { $c->n; $c->s; C::MAX; Suit::Hearts; Suit::class; }\n";
    let facts = analyse(src);
    let f = unit(&facts, "f");
    assert_eq!(last(f, src, "$c->n"), "int");
    assert_eq!(last(f, src, "$c->s"), "string|null");
    assert_eq!(last(f, src, "C::MAX"), "5");
    assert_eq!(last(f, src, "Suit::Hearts"), "Suit::Hearts");
    assert_eq!(last(f, src, "Suit::class"), "class-string<Suit>");
}

#[test]
fn this_is_the_enclosing_class() {
    let src =
        "<?php\nclass A { function a(): int { return 1; } function b() { $this->a(); $this; } }\n";
    let facts = analyse(src);
    let f = unit(&facts, "A::b");
    assert_eq!(last(f, src, "$this"), "static(A)");
    assert_eq!(call(f, src, "$this->a()"), "resolved [method A::a]");
}

#[test]
fn by_reference_arguments_take_the_declared_type() {
    let src = "<?php\nfunction fill(array &$out): void {}\nfunction f() { fill($r); $r; }\n";
    let facts = analyse(src);
    assert_eq!(
        last(unit(&facts, "f"), src, "$r"),
        "array<array-key, mixed>"
    );
}

#[test]
fn array_element_writes_update_the_array_type() {
    let src = "<?php\nfunction f() { $a = []; $a['k'] = 1; $a['j'] = 'x'; $a; $l = []; $l[] = 1; $l[] = 'a'; $l; }\n";
    let facts = analyse(src);
    let f = unit(&facts, "f");
    assert_eq!(last(f, src, "$a"), "array{j: 'x', k: 1}");
    assert_eq!(last(f, src, "$l"), "array{0: 1, 1: 'a'}");
}

#[test]
fn counts_are_reported_per_function() {
    let src = "<?php\nfunction f($x, $fn, Gone $g) { $fn(); $x->a(); $g->b(); known(); }\nfunction known(): int { return 1; }\n";
    let facts = analyse(src);
    let s = unit(&facts, "f").stats;
    assert_eq!(s.dynamic_calls, 1);
    assert_eq!(s.unresolved_calls, 2);
    assert_eq!(s.resolved_calls, 1);
    assert!(s.unknown_expressions > 0);
}

#[test]
fn keys_are_sorted_and_results_are_deterministic() {
    let src = "<?php\nfunction f($a) { if ($a) { g($a); } foreach ([1,2] as $v) { g($v); } }\nfunction g($x) {}\n";
    let a = analyse(src);
    let b = analyse(src);
    assert_eq!(a, b);
    for unit in &a {
        assert!(unit.expr_types.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(unit.calls.windows(2).all(|w| w[0].0 < w[1].0));
    }
}

mod goldens {
    use std::path::Path;

    use bleedthrough_symbols_suite::symbols::InferLimits;
    use bleedthrough_symbols_suite::{
        calls_text, infer_source, run_golden_dir, table_with_builtins, workspace_root,
    };
    use bleedthrough_syntax_mago::MagoBackend;

    #[test]
    fn resolved_call_goldens() {
        let backend = MagoBackend::new();
        let dir = workspace_root().join("fixtures/inference");
        let failures = run_golden_dir(Path::new(&dir), &|name, source| {
            let table = table_with_builtins(&backend, &[(name, source)]);
            let facts = infer_source(&backend, &table, name, source, &InferLimits::default());
            calls_text(&facts, source)
        });
        assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    }
}

mod robustness {
    use super::*;

    /// Runs `f` on a thread with a large stack so debug-build recursion has room.
    fn big_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(f)
            .unwrap()
            .join()
            .unwrap()
    }

    #[test]
    fn broken_source_still_completes() {
        for src in [
            "<?php\nfunction f( { $x = ; if ( { } foo(",
            "<?php\nclass { function ( ) {} }\nfunction g() { $a->; new ; }",
            "<?php\nfunction f() { match( { }; fn( => ; $x = [1, ; }",
            "<?php\nfunction f() { echo <<<EOT\nunterminated",
            "",
            "<?php",
        ] {
            let facts = analyse(src);
            assert!(!facts.is_empty(), "{src}");
        }
    }

    #[test]
    fn deep_expression_nesting_hits_the_depth_limit() {
        let src = big_stack(|| {
            let depth = 5_000;
            let src = format!(
                "<?php\nfunction f($a) {{ $r = {}$a{}; }}\n",
                "(".repeat(depth),
                ")".repeat(depth)
            );
            let limits = InferLimits {
                max_depth: 64,
                ..InferLimits::default()
            };
            let facts = analyse_with(&[], &src, &limits);
            assert!(!facts.is_empty());
            src
        });
        assert!(src.len() > 10_000);
    }

    #[test]
    fn deep_binary_chains_and_statement_nesting_terminate() {
        big_stack(|| {
            let sum = (0..3_000)
                .map(|i| format!("{i}"))
                .collect::<Vec<_>>()
                .join(" + ");
            let ifs = format!("{}$x = 1;{}", "if ($a) { ".repeat(400), " }".repeat(400));
            let src = format!("<?php\nfunction f($a) {{ $s = {sum}; {ifs} }}\n");
            let limits = InferLimits {
                max_depth: 96,
                ..InferLimits::default()
            };
            let facts = analyse_with(&[], &src, &limits);
            let f = unit(&facts, "f");
            assert!(f.stats.depth_limit_hit);
        });
    }

    #[test]
    fn a_tiny_step_budget_stops_the_body_and_says_so() {
        let body = "$a = 1; ".repeat(500);
        let src = format!("<?php\nfunction f() {{ {body} $z; }}\n");
        let limits = InferLimits {
            step_budget: 50,
            ..InferLimits::default()
        };
        let facts = analyse_with(&[], &src, &limits);
        let f = unit(&facts, "f");
        assert!(f.stats.budget_exhausted);
        assert!(f.stats.limit_hit());
    }

    #[test]
    fn huge_unions_widen_and_are_flagged() {
        let mut body = String::from("$x = null; ");
        for i in 0..200 {
            body.push_str(&format!("if ($c{i}) {{ $x = new C{i}(); }} "));
        }
        let src = format!("<?php\nfunction f($c0) {{ {body} $x; }}\n");
        let facts = analyse(&src);
        let f = unit(&facts, "f");
        assert!(f.stats.widened_types > 0);
        assert!(f.stats.limit_hit());
        assert_eq!(last(f, &src, "$x"), "mixed");
    }

    #[test]
    fn nested_loops_and_closures_terminate() {
        let src = "<?php\nfunction f($n) {\n $a = 0; $b = '';\n while ($n) { for ($i = 0; $i < 3; $i++) { foreach ([1, 2] as $v) { $a = $a + $v; $b = $b . $a; $f = function () use (&$a, $b) { return fn() => $a . $b; }; } } }\n do { $a++; } while ($a < 10);\n}\n";
        let facts = analyse(src);
        assert!(facts.len() >= 3);
    }

    #[test]
    fn unsupported_constructs_are_unknown_not_fatal() {
        let src = "<?php\nfunction f() {\n $a = new class { function m() { return 1; } };\n $a->m();\n goto end; end: $x = eval('1'); list($p, $q) = [1, 2]; [$r, [$s]] = [1, [2]]; $p; yield 1; $t = include 'x.php'; static $u = 3; global $g; unset($p); print $x;\n}\n";
        let facts = analyse(src);
        assert!(facts.iter().any(|u| &*u.name == "f"));
    }

    #[test]
    fn results_do_not_depend_on_how_many_times_it_runs() {
        let src = "<?php\nfunction f($a, $b) { while ($a) { $x = $b; $b = [$x]; $a = $a - 1; } return $b; }\n";
        let first = analyse(src);
        for _ in 0..5 {
            assert_eq!(first, analyse(src));
        }
    }
}

mod docs {
    use bleedthrough_symbols_suite::{table_with_builtins, workspace_root};
    use bleedthrough_syntax_mago::MagoBackend;

    use super::*;

    /// Every ```php expect block of the document is analysed and each `// expect <expr> => <type>`
    /// comment is checked against the last type recorded for that expression.
    #[test]
    fn the_examples_in_the_document_hold() {
        let doc =
            std::fs::read_to_string(workspace_root().join("docs/symbols-and-types.md")).unwrap();
        let mut checked = 0;
        for block in doc.split("```php expect\n").skip(1) {
            let src = block.split("```").next().unwrap();
            let backend = MagoBackend::new();
            let table = table_with_builtins(&backend, &[("main.php", src)]);
            let facts = infer_source(&backend, &table, "main.php", src, &InferLimits::default());
            for line in src.lines() {
                let Some(rest) = line.trim().strip_prefix("// expect ") else {
                    continue;
                };
                let (expr, expected) = rest.split_once(" => ").unwrap();
                let got = facts
                    .iter()
                    .rev()
                    .find_map(|f| types_of(f, src, expr).pop());
                assert_eq!(got.as_deref(), Some(expected), "`{expr}` in\n{src}");
                checked += 1;
            }
        }
        assert!(checked >= 4);
    }
}
