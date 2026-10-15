//! Parser isolation over the resolved dependency graph: a third-party parser backend must not be
//! reachable, directly or through any other crate, from a workspace crate that is not an adapter.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::declaration::{Declaration, matches};

/// Package name -> names of the packages it depends on (all kinds), from the resolved graph.
pub type Graph = BTreeMap<String, Vec<String>>;

/// Returns one error per non-adapter workspace crate that reaches a parser backend, naming the
/// shortest path.
pub fn check(declaration: &Declaration, graph: &Graph, members: &[String]) -> Vec<String> {
    let mut errors = Vec::new();
    for member in members {
        let adapter = declaration.crates.get(member).is_some_and(|c| c.adapter);
        if adapter {
            continue;
        }
        if let Some(path) = path_to_backend(declaration, graph, member) {
            errors.push(format!(
                "`{member}` reaches parser backend `{}` through {}; only adapter crates may",
                path.last().map_or("", String::as_str),
                path.join(" -> ")
            ));
        }
    }
    errors
}

fn path_to_backend(declaration: &Declaration, graph: &Graph, start: &str) -> Option<Vec<String>> {
    let mut parents: BTreeMap<&str, &str> = BTreeMap::new();
    let mut seen: BTreeSet<&str> = BTreeSet::from([start]);
    let mut queue: VecDeque<&str> = VecDeque::from([start]);
    while let Some(node) = queue.pop_front() {
        for next in graph.get(node).into_iter().flatten() {
            if !seen.insert(next.as_str()) {
                continue;
            }
            parents.insert(next.as_str(), node);
            if declaration.parser_backends.iter().any(|p| matches(p, next)) {
                let mut path = vec![next.clone()];
                let mut at = next.as_str();
                while let Some(parent) = parents.get(at) {
                    path.push((*parent).to_string());
                    at = parent;
                }
                path.reverse();
                return Some(path);
            }
            queue.push_back(next.as_str());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const DECL: &str = r#"
        [policy]
        parser_backends = ["mago-*"]
        [crate.dom]
        layer = "domain"
        [crate.adapter]
        layer = "infrastructure"
        adapter = true
        [crate.other]
        layer = "infrastructure"
    "#;

    fn graph(edges: &[(&str, &[&str])]) -> Graph {
        edges
            .iter()
            .map(|(k, v)| {
                (
                    (*k).to_string(),
                    v.iter().map(|s| (*s).to_string()).collect(),
                )
            })
            .collect()
    }

    fn members() -> Vec<String> {
        vec!["dom".into(), "adapter".into(), "other".into()]
    }

    fn run(graph: &Graph) -> Vec<String> {
        check(&Declaration::parse(DECL).unwrap(), graph, &members())
    }

    #[test]
    fn only_the_adapter_reaches_the_backend() {
        let g = graph(&[
            ("adapter", &["mago-syntax", "dom"]),
            ("mago-syntax", &["mago-span"]),
            ("dom", &[]),
            ("other", &["dom"]),
        ]);
        assert!(run(&g).is_empty());
    }

    #[test]
    fn a_direct_dependency_of_another_crate_fails_naming_it() {
        let g = graph(&[("other", &["mago-span"]), ("adapter", &["mago-syntax"])]);
        let errors = run(&g);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("other -> mago-span"), "{errors:?}");
    }

    #[test]
    fn a_transitive_path_is_named_in_full() {
        let g = graph(&[
            ("dom", &["helper"]),
            ("helper", &["wrapper"]),
            ("wrapper", &["mago-database"]),
        ]);
        let errors = run(&g);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            errors[0].contains("dom -> helper -> wrapper -> mago-database"),
            "{errors:?}"
        );
    }

    #[test]
    fn cycles_in_the_graph_do_not_hang() {
        let g = graph(&[("dom", &["a"]), ("a", &["b"]), ("b", &["a"])]);
        assert!(run(&g).is_empty());
    }
}
