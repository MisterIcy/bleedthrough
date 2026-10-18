//! Results do not depend on the order files or units are processed in, or on how the summaries
//! were computed.

use std::collections::BTreeMap;

use bleedthrough_engine_suite::engine::{
    self, FindingsInput, Gate, Limits, ProjectInput, ProjectView, RuleIndex, SourceUnit,
};
use bleedthrough_engine_suite::rules::RuleSet;
use bleedthrough_engine_suite::{FileList, Project, workspace_root};
use bleedthrough_syntax_mago::MagoBackend;

/// The engine's test rule sets, loaded from `crates/engine/tests/rules`.
fn test_rules() -> RuleSet {
    let root = workspace_root().join("crates/engine/tests/rules");
    match bleedthrough_fs::load_rules(&root) {
        Ok(loaded) => loaded.set,
        Err(errors) => {
            let lines: Vec<String> = errors.iter().map(ToString::to_string).collect();
            panic!("the test rules are invalid:\n{}", lines.join("\n"));
        }
    }
}

const FILES: [(&str, &str); 4] = [
    (
        "lib/db.php",
        "<?php\nfunction run($q) { mysqli_query($l, $q); }\nfunction wrap($q) { return run($q . ';'); }\nclass Repo { public $raw; function find($x) { run($x); return $this->raw; } }\n",
    ),
    (
        "lib/cycle.php",
        "<?php\nfunction a($x) { return b($x); }\nfunction b($x) { if ($x) { return a($x); } return $x; }\n",
    ),
    (
        "web/index.php",
        "<?php\nrequire __DIR__ . '/../lib/db.php';\nwrap($_GET['a']);\n$r = new Repo();\n$r->raw = $_POST['b'];\n$r->find($_COOKIE['c']);\nmysqli_query($l, a($_GET['d']));\nhash('md5', $x);\n$f = $_GET['f']; $f();\n",
    ),
    (
        "web/broken.php",
        "<?php\nsystem($_REQUEST['cmd']);\nfunction oops( {\n",
    ),
];

/// How the units are handed to the engine.
#[derive(Clone, Copy)]
enum Order {
    AsGiven,
    Reversed,
    Rotated,
}

fn arrange<'a>(mut units: Vec<SourceUnit<'a>>, order: Order) -> Vec<SourceUnit<'a>> {
    match order {
        Order::AsGiven => {}
        Order::Reversed => units.reverse(),
        Order::Rotated => {
            let by = units.len() / 3;
            units.rotate_left(by);
        }
    }
    units
}

/// Everything observable about a run, as text.
fn observe(files: &[(&str, &str)], order: Order) -> String {
    let rules = test_rules();
    let index = RuleIndex::build(&rules, &Gate::default());
    let project = Project::load(&MagoBackend::new(), files);
    let units = arrange(project.source_units(), order);
    let resolver = FileList(
        project
            .units
            .iter()
            .map(|(f, _)| f.as_str().into())
            .collect(),
    );
    let graph = engine::CallGraph::build(
        &units,
        &project.table,
        &resolver,
        &engine::CallGraphConfig::default(),
    );
    let limits = Limits::default();
    let flow = engine::analyse_project(&ProjectInput {
        table: &project.table,
        index: &index,
        limits,
        graph: &graph,
        units: &units,
        fail_units: None,
    });
    let texts: BTreeMap<&str, &str> = project
        .sources
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let input = FindingsInput {
        rules: &rules,
        index: &index,
        graph: &graph,
        units: &units,
        texts: &texts,
        limits,
    };
    let findings = engine::build_findings(&input, &flow);
    let coverage = engine::build_coverage(&input, &flow, "8.3");
    format!("{findings:#?}\n{coverage:#?}")
}

fn permutations(items: &[(&'static str, &'static str)]) -> Vec<Vec<(&'static str, &'static str)>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut out = Vec::new();
    for at in 0..items.len() {
        let mut rest = items.to_vec();
        let first = rest.remove(at);
        for mut tail in permutations(&rest) {
            tail.insert(0, first);
            out.push(tail);
        }
    }
    out
}

/// spec: finding-production/Different processing order
#[test]
fn any_order_of_files_and_units_gives_identical_results_byte_for_byte() {
    let baseline = observe(&FILES, Order::AsGiven);
    assert!(
        baseline.contains("BT9001"),
        "the project has findings to compare"
    );
    for files in permutations(&FILES) {
        assert_eq!(observe(&files, Order::AsGiven), baseline);
    }
    let mut reversed = FILES.to_vec();
    reversed.reverse();
    for order in [Order::Reversed, Order::Rotated] {
        assert_eq!(observe(&FILES, order), baseline);
        assert_eq!(observe(&reversed, order), baseline);
    }
}

/// spec: finding-production/Two independent components
#[test]
fn components_can_be_summarised_in_any_order() {
    let rules = test_rules();
    let index = RuleIndex::build(&rules, &Gate::default());
    let project = Project::load(&MagoBackend::new(), &FILES);
    let units = project.source_units();
    let graph = project.call_graph();
    let input = ProjectInput {
        table: &project.table,
        index: &index,
        limits: Limits::default(),
        graph: &graph,
        units: &units,
        fail_units: None,
    };
    let view = ProjectView::new(&input).unwrap();
    let heap = engine::Heap::new();
    let count = graph.units().len();

    // Callees first, in the graph's order, and again with independent components swapped: a
    // component only needs the summaries of the components it calls.
    let compute = |order: Vec<usize>| {
        let mut done: Vec<Option<engine::UnitFlow>> = vec![None; count];
        for at in order {
            for (id, flow) in view.summarise_component(at, &done, &heap).flows {
                done[id.index()] = Some(flow);
            }
        }
        done
    };
    let forward: Vec<usize> = (0..graph.components().len()).collect();
    let baseline = compute(forward.clone());
    let depends = |a: usize, b: usize| {
        graph.components()[a].units.iter().any(|u| {
            graph
                .callees(*u)
                .iter()
                .any(|t| graph.components()[b].units.contains(t))
        })
    };
    // Move every component as early as its dependencies allow, in reverse index order.
    let mut placed: Vec<usize> = Vec::new();
    let mut pending: Vec<usize> = forward.iter().rev().copied().collect();
    while !pending.is_empty() {
        let at = pending
            .iter()
            .position(|c| {
                pending
                    .iter()
                    .all(|other| other == c || !depends(*c, *other))
            })
            .expect("the component order is acyclic");
        placed.push(pending.remove(at));
    }
    assert_ne!(placed, forward, "the project has independent components");
    assert_eq!(compute(placed), baseline);
}
