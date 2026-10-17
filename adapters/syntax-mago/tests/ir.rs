//! IR lowering scenarios, run through the Mago backend.

use std::collections::BTreeSet;

use bleedthrough_engine_suite::ir::{
    BlockId, Callee, ConstValue, Graph, Instr, InstrKind, Place, Resolution, Terminator, dump,
};
use bleedthrough_engine_suite::{dump_graphs, lower_alone};
use bleedthrough_syntax_mago::MagoBackend;

fn lower(src: &str) -> Vec<Graph> {
    lower_alone(&MagoBackend::new(), src)
}

fn unit<'a>(graphs: &'a [Graph], name: &str) -> &'a Graph {
    graphs
        .iter()
        .find(|g| &*g.key.name == name)
        .unwrap_or_else(|| {
            panic!(
                "no unit {name}; have {:?}",
                graphs.iter().map(|g| &g.key.name).collect::<Vec<_>>()
            )
        })
}

fn instrs(g: &Graph) -> Vec<(BlockId, &Instr)> {
    g.blocks
        .iter()
        .enumerate()
        .flat_map(|(i, b)| {
            let id = BlockId::new(u32::try_from(i).unwrap());
            b.instrs.iter().map(move |ins| (id, ins))
        })
        .collect()
}

/// The blocks holding a call to the function `name`.
fn call_blocks(g: &Graph, name: &str) -> Vec<BlockId> {
    instrs(g)
        .into_iter()
        .filter(|(_, i)| match &i.kind {
            InstrKind::Call {
                callee: Callee::Function(n),
                ..
            } => &*n.text == name,
            _ => false,
        })
        .map(|(b, _)| b)
        .collect()
}

fn preds(g: &Graph, target: BlockId) -> BTreeSet<BlockId> {
    (0..g.blocks.len())
        .map(|i| BlockId::new(u32::try_from(i).unwrap()))
        .filter(|b| g.all_successors(*b).contains(&target))
        .collect()
}

/// The blocks reachable from `from` along any edge.
fn reachable(g: &Graph, from: BlockId) -> BTreeSet<BlockId> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![from];
    while let Some(b) = stack.pop() {
        if seen.insert(b) {
            stack.extend(g.all_successors(b));
        }
    }
    seen
}

// ---- units ---------------------------------------------------------------------------------

/// spec: ir-lowering/One control-flow graph per unit
#[test]
fn a_class_with_two_methods_and_top_level_code_yields_a_graph_each() {
    let src = "<?php\nclass A { function one() {} function two() { return 1; } }\n$x = 1;\n";
    let graphs = lower(src);
    let names: Vec<&str> = graphs.iter().map(|g| &*g.key.name).collect();
    assert_eq!(names, vec!["{main}", "A::one", "A::two"], "{names:?}");
    let two = unit(&graphs, "A::two");
    assert_eq!(&**two.owner.as_ref().unwrap(), "A");
    assert!(!two.is_static);
    assert_eq!(two.entry, BlockId::new(0));
    assert!(matches!(two.block(two.exit).term, Terminator::Exit));
}

/// spec: ir-lowering/One control-flow graph per unit
#[test]
fn closures_and_arrow_functions_are_separate_graphs_with_their_captures() {
    let src = "<?php\nclass A { function m($p) { $y = 2; $f = function ($z) use ($p, &$y) { return $p . $z; }; $g = fn($q) => $q . $y; return $f; } }\n";
    let graphs = lower(src);
    assert_eq!(
        graphs.len(),
        4,
        "{:?}",
        graphs.iter().map(|g| &g.key.name).collect::<Vec<_>>()
    );
    let closure = graphs
        .iter()
        .find(|g| g.kind == bleedthrough_engine_suite::ir::UnitKind::Closure)
        .unwrap();
    let caps: Vec<(&str, bool)> = closure
        .captures
        .iter()
        .map(|c| (&*c.name, c.by_ref))
        .collect();
    assert_eq!(caps, vec![("p", false), ("y", true)]);
    assert_eq!(closure.params.len(), 1);
    let arrow = graphs
        .iter()
        .find(|g| g.kind == bleedthrough_engine_suite::ir::UnitKind::ArrowFn)
        .unwrap();
    let caps: Vec<&str> = arrow.captures.iter().map(|c| &*c.name).collect();
    assert_eq!(
        caps,
        vec!["y"],
        "an arrow function captures what its body reads, by value"
    );

    let method = unit(&graphs, "A::m");
    let created: Vec<&InstrKind> = instrs(method)
        .into_iter()
        .map(|(_, i)| &i.kind)
        .filter(|k| matches!(k, InstrKind::Closure { .. }))
        .collect();
    assert_eq!(created.len(), 2, "each creation point links its graph");
    let InstrKind::Closure {
        unit: key,
        captures,
    } = created[0]
    else {
        unreachable!()
    };
    assert_eq!(key, &closure.key);
    assert_eq!(captures.len(), 2);
    assert_eq!(captures[0].inner, closure.captures[0].local);
}

#[test]
fn functions_declared_inside_bodies_and_property_hooks_are_units() {
    let src = "<?php\nfunction outer() { if (true) { function inner($a) { return $a; } } }\nclass H { public string $name { get => strtoupper($this->name); set { $this->name = $value; } } }\n";
    let graphs = lower(src);
    let names: Vec<String> = graphs.iter().map(|g| g.key.name.to_string()).collect();
    assert!(names.iter().any(|n| n == "inner"), "{names:?}");
    let hooks: Vec<&Graph> = graphs
        .iter()
        .filter(|g| g.kind == bleedthrough_engine_suite::ir::UnitKind::Hook)
        .collect();
    assert_eq!(hooks.len(), 2, "{names:?}");
    let set = hooks
        .iter()
        .find(|g| g.key.name.ends_with("::set"))
        .unwrap();
    assert_eq!(
        set.params.iter().map(|p| &*p.name).collect::<Vec<_>>(),
        vec!["value"]
    );
}

#[test]
fn promoted_constructor_parameters_store_into_properties() {
    let src = "<?php\nclass P { function __construct(private string $q, int $n) {} }\n";
    let graphs = lower(src);
    let ctor = unit(&graphs, "P::__construct");
    let stores: Vec<&Instr> = instrs(ctor)
        .into_iter()
        .map(|(_, i)| i)
        .filter(|i| {
            matches!(
                &i.kind,
                InstrKind::Store {
                    place: Place::Prop { .. },
                    ..
                }
            )
        })
        .collect();
    assert_eq!(
        stores.len(),
        1,
        "only the promoted parameter: {}",
        dump(ctor)
    );
}

// ---- control flow ----------------------------------------------------------------------------

/// spec: ir-lowering/Control flow is modelled for every PHP construct
#[test]
fn if_else_branches_join() {
    let src =
        "<?php\nfunction f($c) { if ($c) { a(); } elseif ($c > 1) { b(); } else { c(); } d(); }\n";
    let graphs = lower(src);
    let g = unit(&graphs, "f");
    let d = call_blocks(g, "d")[0];
    for branch in ["a", "b", "c"] {
        let block = call_blocks(g, branch)[0];
        assert!(reachable(g, block).contains(&d), "{branch} reaches d");
        assert_ne!(block, d);
    }
    assert!(!reachable(g, call_blocks(g, "a")[0]).contains(&call_blocks(g, "b")[0]));
    assert_eq!(preds(g, d).len(), 3, "three arms join: {}", dump(g));
}

/// spec: ir-lowering/Control flow is modelled for every PHP construct
#[test]
fn loops_have_back_edges_and_break_continue_levels() {
    let src = "<?php\nfunction f($n) {\n while ($n) {\n  while ($n > 1) { if ($n) { break 2; } if ($n < 0) { continue 2; } inner(); }\n  outer_tail();\n }\n after();\n}\n";
    let graphs = lower(src);
    let g = unit(&graphs, "f");
    let after = call_blocks(g, "after")[0];
    let tail = call_blocks(g, "outer_tail")[0];
    let inner = call_blocks(g, "inner")[0];
    // `break 2` leaves both loops: some block jumps straight to the block holding after().
    let brk = preds(g, after);
    assert!(
        brk.iter()
            .any(|b| matches!(g.block(*b).term, Terminator::Goto(t) if t == after)),
        "{}",
        dump(g)
    );
    // `continue 2` goes to the outer loop head, skipping outer_tail().
    assert!(reachable(g, inner).contains(&tail));
    assert!(!preds(g, tail).iter().any(|p| {
        matches!(g.block(*p).term, Terminator::Goto(_))
            && g.block(*p)
                .instrs
                .iter()
                .any(|i| matches!(&i.kind, InstrKind::Unary { .. }))
    }));
    // loop bodies come back: inner() reaches itself.
    assert!(
        reachable(g, inner)
            .iter()
            .any(|b| g.all_successors(*b).contains(&inner))
    );
}

/// spec: ir-lowering/Control flow is modelled for every PHP construct
#[test]
fn do_while_for_and_foreach_loop_back() {
    let src = "<?php\nfunction f($a) { do { x(); } while ($a); for ($i = 0; $i < 3; $i++) { y(); } foreach ($a as $k => $v) { z($k, $v); } end(); }\n";
    let graphs = lower(src);
    let g = unit(&graphs, "f");
    for name in ["x", "y", "z"] {
        let b = call_blocks(g, name)[0];
        assert!(
            g.blocks.iter().enumerate().any(|(i, _)| {
                let id = BlockId::new(u32::try_from(i).unwrap());
                g.all_successors(id).contains(&b) && reachable(g, b).contains(&id)
            }),
            "{name} is in a loop: {}",
            dump(g)
        );
    }
    let text = dump(g);
    assert!(
        text.contains("iter v") && text.contains("Key") && text.contains("Value"),
        "{text}"
    );
}

/// spec: ir-lowering/Control flow is modelled for every PHP construct
#[test]
fn switch_falls_through_and_break_leaves() {
    let src = "<?php\nfunction f($x) { switch ($x) { case 1: one(); case 2: two(); break; default: dflt(); } end(); }\n";
    let graphs = lower(src);
    let g = unit(&graphs, "f");
    let one = call_blocks(g, "one")[0];
    let two = call_blocks(g, "two")[0];
    let dflt = call_blocks(g, "dflt")[0];
    let end = call_blocks(g, "end")[0];
    assert!(
        matches!(g.block(one).term, Terminator::Goto(t) if t == two),
        "case 1 falls into case 2"
    );
    assert!(
        matches!(g.block(two).term, Terminator::Goto(t) if t == end),
        "break leaves the switch"
    );
    assert!(!reachable(g, two).contains(&dflt) || reachable(g, two).contains(&end));
    assert!(!preds(g, dflt).is_empty());
}

/// spec: ir-lowering/Control flow is modelled for every PHP construct
#[test]
fn match_evaluates_arms_in_order_and_throws_without_a_default() {
    let src = "<?php\nfunction f($x) { return match ($x) { 1, 2 => a(), 3 => b(), }; }\nfunction g($x) { return match ($x) { 1 => a(), default => d() }; }\n";
    let graphs = lower(src);
    let f = unit(&graphs, "f");
    assert!(
        f.blocks
            .iter()
            .any(|b| matches!(b.term, Terminator::Throw(_))),
        "unhandled match throws: {}",
        dump(f)
    );
    let g = unit(&graphs, "g");
    assert!(
        !g.blocks
            .iter()
            .any(|b| matches!(b.term, Terminator::Throw(_)))
    );
    let a = call_blocks(g, "a")[0];
    let d = call_blocks(g, "d")[0];
    assert!(!reachable(g, a).contains(&d), "arms do not fall through");
}

/// spec: ir-lowering/Control flow is modelled for every PHP construct
#[test]
fn the_right_operand_of_a_short_circuit_runs_only_on_its_path() {
    let src = "<?php\nfunction f($a, $b) { if ($a && check($b)) { return 1; } return $a ?: other($b); }\n";
    let graphs = lower(src);
    let g = unit(&graphs, "f");
    let check = call_blocks(g, "check")[0];
    let ps = preds(g, check);
    assert_eq!(ps.len(), 1, "{}", dump(g));
    let p = *ps.iter().next().unwrap();
    assert!(
        matches!(g.block(p).term, Terminator::Branch { then, .. } if then == check),
        "reached only when the left operand is true"
    );
    let other = call_blocks(g, "other")[0];
    let ps = preds(g, other);
    assert_eq!(ps.len(), 1);
    let p = *ps.iter().next().unwrap();
    assert!(
        matches!(g.block(p).term, Terminator::Branch { otherwise, .. } if otherwise == other),
        "?: runs the right side when the left is falsy"
    );
}

/// spec: ir-lowering/Control flow is modelled for every PHP construct
#[test]
fn nullsafe_and_coalesce_guard_their_operands() {
    let src = "<?php\nfunction f($o, $a) { $x = $o?->go(arg()); $y = $a ?? fallback(); return $x . $y; }\n";
    let graphs = lower(src);
    let g = unit(&graphs, "f");
    let arg = call_blocks(g, "arg")[0];
    assert_eq!(
        preds(g, arg).len(),
        1,
        "arguments of a null-safe call run only when the receiver is not null"
    );
    let p = *preds(g, arg).iter().next().unwrap();
    assert!(
        matches!(g.block(p).term, Terminator::Branch { then, .. } if then == arg),
        "{}",
        dump(g)
    );
    let fb = call_blocks(g, "fallback")[0];
    let p = *preds(g, fb).iter().next().unwrap();
    assert!(matches!(g.block(p).term, Terminator::Branch { otherwise, .. } if otherwise == fb));
}

/// spec: ir-lowering/Control flow is modelled for every PHP construct
#[test]
fn a_return_inside_try_goes_through_finally() {
    let src = "<?php\nfunction f($x) {\n try { if ($x) { return risky($x); } work(); }\n catch (Exception $e) { handle($e); }\n finally { cleanup(); }\n after();\n}\n";
    let graphs = lower(src);
    let g = unit(&graphs, "f");
    let cleanup = call_blocks(g, "cleanup")[0];
    let risky = call_blocks(g, "risky")[0];
    // No block reachable from the try body returns a value before passing through cleanup().
    let direct_return = g.blocks.iter().enumerate().any(|(i, b)| {
        matches!(b.term, Terminator::Return(Some(_)))
            && !reachable(g, cleanup).contains(&BlockId::new(u32::try_from(i).unwrap()))
    });
    assert!(
        !direct_return,
        "every value return is reached via finally: {}",
        dump(g)
    );
    assert!(reachable(g, risky).contains(&cleanup));
    // The finally block continues to the code after the try and to the routed return.
    let Terminator::Choice(targets) = &g.block(cleanup).term else {
        panic!("finally ends in a choice: {}", dump(g));
    };
    let after = call_blocks(g, "after")[0];
    assert!(targets.contains(&after));
    assert!(
        targets
            .iter()
            .any(|t| matches!(g.block(*t).term, Terminator::Return(Some(_)))),
        "{}",
        dump(g)
    );
    // The catch is a handler of the try body; exceptions may also reach finally.
    let work = call_blocks(g, "work")[0];
    let handle = call_blocks(g, "handle")[0];
    let handlers = &g.block(work).handlers;
    assert!(handlers.iter().any(|h| reachable(g, *h).contains(&handle)));
    assert!(handlers.contains(&cleanup));
}

/// spec: ir-lowering/Control flow is modelled for every PHP construct
#[test]
fn break_through_finally_is_routed_and_finally_without_catch_works() {
    let src = "<?php\nfunction f($a) { foreach ($a as $x) { try { if ($x) { break; } step(); } finally { done(); } } end(); }\n";
    let graphs = lower(src);
    let g = unit(&graphs, "f");
    let done = call_blocks(g, "done")[0];
    let end = call_blocks(g, "end")[0];
    let Terminator::Choice(targets) = &g.block(done).term else {
        panic!("{}", dump(g));
    };
    assert!(targets.iter().any(|t| reachable(g, *t).contains(&end)));
    assert!(reachable(g, call_blocks(g, "step")[0]).contains(&done));
}

/// spec: ir-lowering/Control flow is modelled for every PHP construct
#[test]
fn goto_and_labels_link_blocks() {
    let src =
        "<?php\nfunction f($a) { goto skip; first(); skip: second(); if ($a) { goto skip; } }\n";
    let graphs = lower(src);
    let g = unit(&graphs, "f");
    let first = call_blocks(g, "first")[0];
    let second = call_blocks(g, "second")[0];
    assert!(
        preds(g, first).is_empty() || !reachable(g, g.entry).contains(&first),
        "first() is dead code"
    );
    assert!(reachable(g, g.entry).contains(&second));
    assert!(
        preds(g, second).len() >= 2,
        "forward and backward gotos both reach the label: {}",
        dump(g)
    );
}

/// spec: ir-lowering/Control flow is modelled for every PHP construct
#[test]
fn throw_exit_and_yield_are_modelled() {
    let src = "<?php\nfunction f($a) { if ($a) { throw new E(); } yield $a; yield from g(); exit('bye'); after(); }\n";
    let graphs = lower(src);
    let g = unit(&graphs, "f");
    assert!(
        g.blocks
            .iter()
            .any(|b| matches!(b.term, Terminator::Throw(_)))
    );
    let text = dump(g);
    assert!(text.contains("yield -"), "{text}");
    assert!(text.contains("yield-from"), "{text}");
    assert!(text.contains("exit v"), "{text}");
    let after = call_blocks(g, "after")[0];
    assert!(
        !reachable(g, g.entry).contains(&after),
        "code after exit is unreachable"
    );
}

#[test]
fn dumps_of_a_sample_are_stable_across_runs() {
    let src = "<?php\nfunction f($a) { try { return $a; } finally { log($a); } }\n";
    let a = dump_graphs(&lower(src), src);
    let b = dump_graphs(&lower(src), src);
    assert_eq!(a, b);
}

#[test]
fn constants_decode_strings_and_booleans() {
    let src = "<?php\nfunction f() { return ['a' . \"b\\n\", true, NULL, PHP_EOL, 7]; }\n";
    let graphs = lower(src);
    let g = unit(&graphs, "f");
    let consts: Vec<&ConstValue> = instrs(g)
        .into_iter()
        .filter_map(|(_, i)| match &i.kind {
            InstrKind::Const(c) => Some(c),
            _ => None,
        })
        .collect();
    assert!(consts.contains(&&ConstValue::Str(Some("a".into()))));
    assert!(consts.contains(&&ConstValue::Str(Some("b\n".into()))));
    assert!(consts.contains(&&ConstValue::Bool(true)));
    assert!(consts.contains(&&ConstValue::Null));
    let _ = Resolution::Missing;
}

// ---- calls ---------------------------------------------------------------------------------

/// spec: ir-lowering/Instructions carry what analysis needs
#[test]
fn call_resolutions_come_from_the_inference_facts() {
    let src = "<?php\nclass A { function m($x) { return $x; } static function s($x) { return $x; } }\nfunction local_fn($v) { return $v; }\nfunction f(A $a, $dyn) {\n local_fn(1); strlen('x'); $a->m(2); A::s(3); $dyn(4); $a->$dyn(5); nothing_here(6); new A(); include 'x.php'; eval('1;'); echo 1; print 2;\n}\n";
    let graphs = lower(src);
    let g = unit(&graphs, "f");
    let text = dump(g);
    for needle in [
        "call fn local_fn(",
        "call fn strlen(",
        "res=resolved [function strlen]",
        "call method",
        "call static",
        "call value",
        "res=unresolved(function_not_found)",
        "new ",
        "include ",
        "eval v",
        "echo v",
        "print v",
    ] {
        assert!(text.contains(needle), "missing {needle:?} in\n{text}");
    }
    assert!(
        !text.contains("res=none"),
        "every call carries a resolution from the facts: {text}"
    );
}

/// spec: ir-lowering/Instructions carry what analysis needs
#[test]
fn a_call_without_facts_is_marked_missing() {
    use bleedthrough_engine_suite::{ir, parse};
    let backend = MagoBackend::new();
    let src = "<?php\nfoo(1);\n";
    let parsed = parse(&backend, "a.php", src);
    let graphs = ir::lower_file(parsed.tree(), &[]);
    assert!(dump(&graphs[0]).contains("res=none"));
}

// ---- totality over the corpora ---------------------------------------------------------------

/// spec: ir-lowering/Lowering is total over the syntax model
#[test]
fn every_conformance_fixture_lowers_without_panicking() {
    use bleedthrough_engine_suite::{lower_source, php_files, table_with_builtins, workspace_root};
    use bleedthrough_syntax_conformance::Corpus;
    let backend = MagoBackend::new();
    let root = workspace_root().join("fixtures/php");
    let corpus = Corpus::load(&root).expect("the corpus loads");
    assert!(!corpus.fixtures.is_empty());
    let mut lowered = 0;
    for fixture in &corpus.fixtures {
        let bytes = corpus.read(fixture).expect("fixture readable");
        let source = String::from_utf8_lossy(&bytes).into_owned();
        let table = table_with_builtins(&backend, &[(fixture.path.as_str(), source.as_str())]);
        let graphs = lower_source(&backend, &table, &fixture.path, &source);
        assert!(!graphs.is_empty(), "{}", fixture.path);
        for g in &graphs {
            assert!(g.blocks.len() > 1, "{} {}", fixture.path, g.key.name);
        }
        lowered += 1;
    }
    assert!(lowered > 50, "{lowered}");
    let _ = php_files;
}

/// spec: ir-lowering/Lowering is total over the syntax model
#[test]
fn malformed_fixtures_mark_the_graphs_they_damage() {
    use bleedthrough_engine_suite::{lower_alone, workspace_root};
    let backend = MagoBackend::new();
    let source = std::fs::read_to_string(
        workspace_root().join("fixtures/php/malformed/unbalanced-brace.php"),
    )
    .unwrap();
    let graphs = lower_alone(&backend, &source);
    assert!(graphs.iter().any(|g| g.recovered));
    assert!(
        graphs.iter().any(|g| !g.recovered),
        "undamaged units stay unmarked"
    );
}

// ---- goldens ---------------------------------------------------------------------------------

fn golden_for(dir: &str, php: &std::path::Path) {
    use bleedthrough_engine_suite::{check_golden, dump_source, workspace_root};
    let source = std::fs::read_to_string(php).unwrap();
    let stem = php.file_stem().unwrap().to_string_lossy();
    let actual = dump_source(&MagoBackend::new(), &source);
    let golden = workspace_root().join(format!("crates/engine-suite/golden/ir/{dir}/{stem}.txt"));
    if let Err(message) = check_golden(&golden, &actual) {
        panic!("{message}");
    }
}

/// spec: ir-lowering/Lowering is deterministic and independent of other files
#[test]
fn inference_fixtures_have_golden_ir_dumps() {
    use bleedthrough_engine_suite::{php_files, workspace_root};
    let files = php_files(&workspace_root().join("fixtures/inference"));
    assert!(!files.is_empty());
    for f in files {
        golden_for("inference", &f);
    }
}

/// spec: ir-lowering/Lowering is deterministic and independent of other files
#[test]
fn realistic_fixtures_have_golden_ir_dumps() {
    use bleedthrough_engine_suite::{php_files, workspace_root};
    let files = php_files(&workspace_root().join("fixtures/php/realistic"));
    assert!(!files.is_empty());
    for f in files {
        golden_for("realistic", &f);
    }
}

// ---- properties ------------------------------------------------------------------------------

/// spec: ir-lowering/Lowering is total over the syntax model
/// spec: ir-lowering/Lowering is deterministic and independent of other files
#[test]
fn lowering_mutated_fixtures_never_panics_and_numbers_values_uniquely() {
    use bleedthrough_engine_suite::{php_files, workspace_root};
    use bleedthrough_syntax_conformance::mutate::{derive_seed, mutate};
    let backend = MagoBackend::new();
    let mut files = php_files(&workspace_root().join("fixtures/php/realistic"));
    files.extend(php_files(&workspace_root().join("fixtures/inference")));
    for file in files {
        let original = std::fs::read(&file).unwrap();
        for index in 0..12 {
            let seed = derive_seed(7, &file.to_string_lossy(), index);
            let mutated = mutate(&original, seed);
            let source = String::from_utf8_lossy(&mutated.bytes).into_owned();
            let graphs = lower_alone(&backend, &source);
            let again = lower_alone(&backend, &source);
            assert_eq!(dump_graphs(&graphs, &source), dump_graphs(&again, &source));
            for g in &graphs {
                let mut seen = BTreeSet::new();
                for b in &g.blocks {
                    let mut last = None;
                    for i in &b.instrs {
                        if let Some(d) = i.dest {
                            assert!(seen.insert(d), "{} defines {d} twice", g.key.name);
                            assert!(d.raw() < g.value_count);
                            // numbering follows emission order within a block
                            assert!(
                                last.is_none_or(|l| d > l),
                                "{} numbers out of order",
                                g.key.name
                            );
                            last = Some(d);
                        }
                    }
                }
            }
        }
    }
}
