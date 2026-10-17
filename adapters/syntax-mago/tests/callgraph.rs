//! The call graph and the constant folder over PHP source, through the Mago backend.

use bleedthrough_engine_suite::Project;
use bleedthrough_engine_suite::engine::{
    CallGraph, CallGraphConfig, CallOutcome, IncludeOutcome, UnitId, Unresolved,
};
use bleedthrough_engine_suite::symbols::{DynamicKind, UnresolvedReason};
use bleedthrough_syntax_mago::MagoBackend;

fn project(files: &[(&str, &str)]) -> Project {
    Project::load(&MagoBackend::new(), files)
}

fn unit(graph: &CallGraph, name: &str) -> UnitId {
    let found: Vec<UnitId> = (0..graph.units().len())
        .map(UnitId::from_index)
        .filter(|u| &*graph.unit(*u).key.name == name)
        .collect();
    assert_eq!(found.len(), 1, "unit {name}: {found:?}");
    found[0]
}

fn main_of(graph: &CallGraph, file: &str) -> UnitId {
    (0..graph.units().len())
        .map(UnitId::from_index)
        .find(|u| &*graph.unit(*u).file == file && &*graph.unit(*u).key.name == "{main}")
        .expect("main unit")
}

fn names(graph: &CallGraph, units: &[UnitId]) -> Vec<String> {
    units
        .iter()
        .map(|u| graph.unit(*u).key.name.to_string())
        .collect()
}

fn outcomes(graph: &CallGraph, caller: &str) -> Vec<CallOutcome> {
    graph
        .sites_of(unit(graph, caller))
        .iter()
        .map(|s| s.outcome.clone())
        .collect()
}

fn only_outcome<'g>(graph: &'g CallGraph, caller: &str) -> &'g CallOutcome {
    let sites = graph.sites_of(unit(graph, caller));
    assert_eq!(sites.len(), 1, "{sites:?}");
    &sites[0].outcome
}

// ---- edges -----------------------------------------------------------------------------------

/// spec: call-graph/Edges come from call resolutions
#[test]
fn single_target_calls_have_one_edge_each() {
    let p = project(&[(
        "a.php",
        "<?php\nclass A { function m($x) { return $x; } static function s($x) { return $x; } function __construct() {} }\nfunction helper($v) { return $v; }\nfunction go(A $a) { helper(1); $a->m(2); A::s(3); new A(); }\n",
    )]);
    let g = p.call_graph();
    let go = unit(&g, "go");
    let callees = names(&g, g.callees(go));
    assert_eq!(callees, vec!["A::m", "A::s", "A::__construct", "helper"]);
    for outcome in outcomes(&g, "go") {
        match outcome {
            CallOutcome::Units {
                targets,
                multi_target,
                truncated,
                ..
            } => {
                assert_eq!(targets.len(), 1);
                assert!(!multi_target && !truncated);
            }
            other => panic!("{other:?}"),
        }
    }
}

/// spec: call-graph/Edges come from call resolutions
#[test]
fn an_interface_call_has_an_edge_to_every_implementor_and_is_multi_target() {
    let p = project(&[(
        "a.php",
        "<?php\ninterface Sink { function put($x); }\nclass One implements Sink { function put($x) { return 1; } }\nclass Two implements Sink { function put($x) { return 2; } }\nfunction go(Sink $s) { $s->put(1); }\n",
    )]);
    let g = p.call_graph();
    let CallOutcome::Units {
        targets,
        multi_target,
        truncated,
        ..
    } = only_outcome(&g, "go")
    else {
        panic!("{:?}", outcomes(&g, "go"));
    };
    assert_eq!(names(&g, targets), vec!["One::put", "Two::put"]);
    assert!(*multi_target);
    assert!(!*truncated);
}

/// spec: call-graph/Edges come from call resolutions
#[test]
fn an_inherited_method_resolves_to_the_parent_body() {
    let p = project(&[(
        "a.php",
        "<?php\nclass P { function run() { return 1; } }\nclass C extends P {}\nfunction go(C $c) { $c->run(); }\n",
    )]);
    let g = p.call_graph();
    assert_eq!(names(&g, g.callees(unit(&g, "go"))), vec!["P::run"]);
}

/// spec: call-graph/Edges come from call resolutions
#[test]
fn a_builtin_call_carries_its_identity_and_has_no_edge() {
    let p = project(&[("a.php", "<?php\nfunction go($x) { return strlen($x); }\n")]);
    let g = p.call_graph();
    let go = unit(&g, "go");
    assert!(g.callees(go).is_empty());
    let CallOutcome::Builtin(name) = only_outcome(&g, "go") else {
        panic!("{:?}", outcomes(&g, "go"));
    };
    assert_eq!(name.as_str(), "strlen");
}

// ---- unresolved ------------------------------------------------------------------------------

/// spec: call-graph/Unresolved calls are recorded with reasons
#[test]
fn unresolved_calls_are_recorded_with_their_reason_and_counted() {
    let p = project(&[(
        "a.php",
        "<?php\nfunction go($f, $o) { $f(1); $o->m(2); nothing_here(3); }\n",
    )]);
    let g = p.call_graph();
    let found = outcomes(&g, "go");
    assert_eq!(
        found,
        vec![
            CallOutcome::Unresolved(Unresolved::Dynamic(DynamicKind::VariableFunction)),
            CallOutcome::Unresolved(Unresolved::Inference(UnresolvedReason::UnknownReceiverType)),
            CallOutcome::Unresolved(Unresolved::Inference(UnresolvedReason::FunctionNotFound)),
        ]
    );
    let c = g.coverage();
    assert_eq!(c.total_unresolved_calls(), 3);
    assert_eq!(
        c.unresolved_calls.get("dynamic_variable_function"),
        Some(&1)
    );
    assert_eq!(c.unresolved_calls.get("unknown_receiver_type"), Some(&1));
    assert_eq!(c.unresolved_calls.get("function_not_found"), Some(&1));
}

// ---- caps ------------------------------------------------------------------------------------

/// spec: call-graph/Candidate sets are capped
#[test]
fn candidates_past_the_cap_are_cut_in_unit_order_and_flagged() {
    let mut src = String::from("<?php\ninterface Sink { function put($x); }\n");
    for n in 0..5 {
        src.push_str(&format!(
            "class Impl{n} implements Sink {{ function put($x) {{ return {n}; }} }}\n"
        ));
    }
    src.push_str("function go(Sink $s) { $s->put(1); }\n");
    let p = project(&[("a.php", &src)]);
    let g = p.call_graph_with(&CallGraphConfig { max_candidates: 3 });
    let CallOutcome::Units {
        targets,
        multi_target,
        truncated,
        ..
    } = only_outcome(&g, "go")
    else {
        panic!();
    };
    assert_eq!(
        names(&g, targets),
        vec!["Impl0::put", "Impl1::put", "Impl2::put"]
    );
    assert!(*multi_target && *truncated);
    assert_eq!(g.coverage().truncated_calls, 1);
}

// ---- includes --------------------------------------------------------------------------------

/// spec: call-graph/Includes are resolved through a port
#[test]
fn literal_includes_link_to_the_included_files_top_level_code() {
    let p = project(&[
        (
            "src/a.php",
            "<?php\nrequire __DIR__ . '/helpers.php';\nrequire_once dirname(__FILE__) . '/../lib/b.php';\ninclude 'helpers.php';\n",
        ),
        ("src/helpers.php", "<?php\nfunction h() {}\n"),
        ("lib/b.php", "<?php\n$x = 1;\n"),
    ]);
    let g = p.call_graph();
    let main = main_of(&g, "src/a.php");
    let resolved: Vec<Vec<String>> = g
        .includes()
        .iter()
        .map(|i| match &i.outcome {
            IncludeOutcome::Resolved(t) => t.iter().map(|u| g.unit(*u).file.to_string()).collect(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        resolved,
        vec![
            vec!["src/helpers.php"],
            vec!["lib/b.php"],
            vec!["src/helpers.php"]
        ]
    );
    let callees = g.callees(main);
    assert_eq!(callees.len(), 2, "distinct targets");
    assert_eq!(g.coverage().total_unresolved_includes(), 0);
}

/// spec: call-graph/Includes are resolved through a port
#[test]
fn dynamic_and_missing_includes_are_recorded_as_unresolved() {
    let p = project(&[(
        "a.php",
        "<?php\nfunction f($page) { include $page . '.php'; }\ninclude __DIR__ . '/missing.php';\ninclude $_GET['p'];\nrequire \"{$x}/y.php\";\n",
    )]);
    let g = p.call_graph();
    let outcomes: Vec<&IncludeOutcome> = g.includes().iter().map(|i| &i.outcome).collect();
    assert_eq!(outcomes.len(), 4);
    let dynamic = outcomes
        .iter()
        .filter(|o| matches!(o, IncludeOutcome::Dynamic))
        .count();
    assert_eq!(dynamic, 3);
    assert!(
        outcomes
            .iter()
            .any(|o| matches!(o, IncludeOutcome::NotFound(p) if &**p == "missing.php"))
    );
    let c = g.coverage();
    assert_eq!(c.unresolved_includes.get("dynamic"), Some(&3));
    assert_eq!(c.unresolved_includes.get("not_found"), Some(&1));
    assert_eq!(c.total_unresolved_includes(), 4);
}

/// spec: call-graph/Includes are resolved through a port
#[test]
fn constants_and_interpolation_fold_into_include_paths() {
    let p = project(&[
        (
            "a.php",
            "<?php\nconst BASE = 'inc';\ndefine('EXT', '.php');\nclass K { const DIR = 'cls'; }\ninclude BASE . '/one' . '.php';\ninclude K::DIR . '/two.php';\ninclude \"inc/three.php\";\ninclude __DIR__ . DIRECTORY_SEPARATOR . 'inc/one.php';\n",
        ),
        ("inc/one.php", "<?php\n"),
        ("cls/two.php", "<?php\n"),
        ("inc/three.php", "<?php\n"),
    ]);
    let g = p.call_graph();
    let kinds: Vec<bool> = g
        .includes()
        .iter()
        .map(|i| matches!(i.outcome, IncludeOutcome::Resolved(_)))
        .collect();
    assert_eq!(kinds, vec![true, true, true, true], "{:?}", g.includes());
}

/// spec: call-graph/Includes are resolved through a port
#[test]
fn values_read_from_variables_are_never_folded() {
    let p = project(&[
        (
            "a.php",
            "<?php\n$dir = 'inc';\ninclude $dir . '/one.php';\nconst C = 'x' . 'y';\ninclude C . '.php';\ninclude strtolower('INC/ONE.PHP');\n",
        ),
        ("inc/one.php", "<?php\n"),
    ]);
    let g = p.call_graph();
    let dynamic = g
        .includes()
        .iter()
        .filter(|i| matches!(i.outcome, IncludeOutcome::Dynamic))
        .count();
    assert_eq!(dynamic, 3, "{:?}", g.includes());
}

// ---- recursion -------------------------------------------------------------------------------

/// spec: call-graph/Recursion is grouped
#[test]
fn mutually_recursive_functions_share_a_component_placed_after_their_callees() {
    let p = project(&[(
        "a.php",
        "<?php\nfunction leaf() {}\nfunction ping($n) { leaf(); pong($n); }\nfunction pong($n) { ping($n); }\nfunction solo($n) { solo($n); }\nfunction top() { ping(1); solo(1); }\n",
    )]);
    let g = p.call_graph();
    let comp = |n: &str| g.component_of(unit(&g, n));
    assert_eq!(comp("ping"), comp("pong"));
    assert!(g.components()[comp("ping")].recursive);
    assert!(g.components()[comp("solo")].recursive, "self-recursion");
    assert!(!g.components()[comp("leaf")].recursive);
    assert!(comp("leaf") < comp("ping"), "callees first");
    assert!(comp("ping") < comp("top"));
    assert!(comp("solo") < comp("top"));
}

/// spec: call-graph/Entry points of analysis are defined
#[test]
fn a_function_nobody_calls_is_still_a_unit_in_a_component() {
    let p = project(&[("a.php", "<?php\nfunction orphan() { return $_GET['x']; }\n")]);
    let g = p.call_graph();
    let orphan = unit(&g, "orphan");
    assert!(
        g.components()[g.component_of(orphan)]
            .units
            .contains(&orphan)
    );
}

// ---- determinism -----------------------------------------------------------------------------

/// spec: call-graph/Graph construction is deterministic
#[test]
fn the_graph_does_not_depend_on_file_order() {
    let files = [
        (
            "b.php",
            "<?php\nfunction fb() { fa(); }\nclass B { function m() { fb(); } }\n",
        ),
        (
            "a.php",
            "<?php\nfunction fa() { }\ninclude 'b.php';\nfunction fc(B $b) { $b->m(); fa(); }\n",
        ),
        ("c.php", "<?php\nfunction fz() { fc(new B()); fa(); }\n"),
    ];
    let render = |order: &[usize]| {
        let ordered: Vec<(&str, &str)> = order.iter().map(|i| files[*i]).collect();
        let g = project(&ordered).call_graph();
        let units: Vec<String> = g
            .units()
            .iter()
            .map(|u| format!("{} {}", u.file, u.key.name))
            .collect();
        let edges: Vec<Vec<usize>> = (0..units.len())
            .map(|i| {
                g.callees(UnitId::from_index(i))
                    .iter()
                    .map(|u| u.index())
                    .collect()
            })
            .collect();
        let comps: Vec<Vec<usize>> = g
            .components()
            .iter()
            .map(|c| c.units.iter().map(|u| u.index()).collect())
            .collect();
        format!(
            "{units:?}\n{edges:?}\n{comps:?}\n{:?}\n{:?}",
            g.sites(),
            g.includes()
        )
    };
    let base = render(&[0, 1, 2]);
    for order in [[2, 1, 0], [1, 2, 0], [1, 0, 2], [2, 0, 1], [0, 2, 1]] {
        assert_eq!(render(&order), base, "{order:?}");
    }
}

// ---- the folder ------------------------------------------------------------------------------

/// spec: call-graph/Includes are resolved through a port
#[test]
fn the_folder_evaluates_only_constant_expressions() {
    use bleedthrough_engine_suite::engine::{Folded, Folder};
    let src = "<?php\nconst KNOWN = 'k';\nfunction f($v) {\n $a = 'lit' . '-' . 7;\n $b = __FILE__;\n $c = dirname(__FILE__, 2);\n $d = KNOWN . '!';\n $e = $v . 'x';\n $f = \"a{$v}\";\n $g = \"tab\\t\";\n $h = UNKNOWN_CONST;\n $i = 1.5;\n sink('lit' . '-' . 7, __FILE__, dirname(__FILE__, 2), KNOWN . '!', $v . 'x', \"a{$v}\", \"tab\\t\", UNKNOWN_CONST, 1.5, null);\n}\n";
    let p = project(&[("d/e/a.php", src)]);
    let (file, graphs) = &p.units[0];
    let f = graphs.iter().find(|g| &*g.key.name == "f").unwrap();
    let folder = Folder::new(f, &p.table, file);
    let call = f
        .blocks
        .iter()
        .flat_map(|b| &b.instrs)
        .find_map(|i| match &i.kind {
            bleedthrough_engine_suite::ir::InstrKind::Call {
                callee: bleedthrough_engine_suite::ir::Callee::Function(n),
                args,
                ..
            } if &*n.text == "sink" => Some(args.clone()),
            _ => None,
        })
        .expect("the sink call");
    let folded: Vec<Option<String>> = call
        .iter()
        .map(|a| folder.fold(a.value).value().map(str::to_string))
        .collect();
    assert_eq!(
        folded,
        vec![
            Some("lit-7".into()),
            Some("d/e/a.php".into()),
            Some("d".into()),
            Some("k!".into()),
            None,
            None,
            Some("tab\t".into()),
            None,
            None,
            Some(String::new()),
        ]
    );
    assert_eq!(folder.fold(call[4].value), Folded::NotFolded);
}
