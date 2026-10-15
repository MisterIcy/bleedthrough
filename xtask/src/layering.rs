//! Layer dependency rule: declared edges only, pointing inward, without cycles.

use std::collections::{BTreeMap, BTreeSet};

use crate::declaration::{Declaration, Layer};
use crate::meta::{Dependency, Package};

/// Returns every violation of the layering declaration.
pub fn check(declaration: &Declaration, packages: &[Package]) -> Vec<String> {
    let mut errors = Vec::new();
    let members: BTreeSet<&str> = packages.iter().map(|p| p.name.as_str()).collect();
    for package in packages {
        match declaration.crates.get(&package.name) {
            None => errors.push(format!(
                "crate `{}` is not declared in architecture.toml",
                package.name
            )),
            Some(declared) => {
                for dep in package
                    .dependencies
                    .iter()
                    .filter(|d| members.contains(d.name.as_str()))
                    .filter(|d| !is_test_support_edge(declaration, d))
                {
                    let allowed = declared.allow.contains(&dep.name);
                    errors.extend(edge_errors(declaration, &package.name, &dep.name, allowed));
                }
            }
        }
    }
    for name in declaration
        .crates
        .keys()
        .filter(|n| !members.contains(n.as_str()))
    {
        errors.push(format!(
            "crate `{name}` is declared in architecture.toml but is not a workspace member"
        ));
    }
    errors.extend(cycle_errors(packages, &members));
    errors
}

/// A dev-dependency on a tooling crate (for example the shared conformance suite) is exempt from the
/// layer and `allow` rules: it never reaches a shipped artefact. A normal dependency on a tooling
/// crate is still rejected by `layer_violation`.
fn is_test_support_edge(declaration: &Declaration, dep: &Dependency) -> bool {
    dep.dev && declaration.layer_of(&dep.name) == Some(Layer::Tooling)
}

fn edge_errors(declaration: &Declaration, from: &str, to: &str, allowed: bool) -> Vec<String> {
    let mut errors = Vec::new();
    let (Some(lf), Some(lt)) = (declaration.layer_of(from), declaration.layer_of(to)) else {
        errors.push(format!(
            "`{from}` -> `{to}`: `{to}` is not declared in architecture.toml"
        ));
        return errors;
    };
    if let Some(reason) = layer_violation(lf, lt) {
        errors.push(format!(
            "`{from}` ({}) -> `{to}` ({}): {reason}",
            lf.name(),
            lt.name()
        ));
    }
    if !allowed {
        errors.push(format!(
            "`{from}` -> `{to}` is not permitted by architecture.toml"
        ));
    }
    errors
}

fn layer_violation(from: Layer, to: Layer) -> Option<&'static str> {
    use Layer::{Application, Delivery, Domain, Infrastructure, Tooling};
    match (from, to) {
        (_, Delivery | Tooling) => Some("nothing may depend on a delivery or tooling crate"),
        (Tooling, Domain) => None,
        (Tooling, _) => Some("tooling crates may depend on domain crates only"),
        (Domain, Domain) | (Application, Domain) | (Infrastructure, Domain | Application) => None,
        (Delivery, _) => None,
        (Infrastructure, Infrastructure) => None, // permitted only if declared; checked by caller
        (Domain, _) => Some("domain crates may depend on domain crates only"),
        (Application, _) => Some("application crates may depend on domain crates only"),
    }
}

fn cycle_errors(packages: &[Package], members: &BTreeSet<&str>) -> Vec<String> {
    let graph: BTreeMap<&str, Vec<&str>> = packages
        .iter()
        .map(|p| {
            let deps = p
                .dependencies
                .iter()
                .map(|d| d.name.as_str())
                .filter(|n| members.contains(n));
            (p.name.as_str(), deps.collect())
        })
        .collect();
    let mut done = BTreeSet::new();
    let mut errors = Vec::new();
    for start in graph.keys() {
        let mut stack = Vec::new();
        visit(start, &graph, &mut stack, &mut done, &mut errors);
    }
    errors
}

fn visit<'a>(
    node: &'a str,
    graph: &BTreeMap<&'a str, Vec<&'a str>>,
    stack: &mut Vec<&'a str>,
    done: &mut BTreeSet<&'a str>,
    errors: &mut Vec<String>,
) {
    if let Some(pos) = stack.iter().position(|n| *n == node) {
        let mut cycle: Vec<&str> = stack[pos..].to_vec();
        cycle.push(node);
        errors.push(format!("dependency cycle: {}", cycle.join(" -> ")));
        return;
    }
    if !done.insert(node) {
        return;
    }
    stack.push(node);
    for next in graph.get(node).into_iter().flatten() {
        visit(next, graph, stack, done, errors);
    }
    stack.pop();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::Dependency;
    use std::path::PathBuf;

    const DECL: &str = r#"
        [crate.d1]
        layer = "domain"
        allow = []
        [crate.d2]
        layer = "domain"
        allow = ["d1"]
        [crate.app]
        layer = "application"
        allow = ["d1"]
        [crate.i1]
        layer = "infrastructure"
        allow = ["app"]
        [crate.i2]
        layer = "infrastructure"
        allow = ["app"]
        [crate.cli]
        layer = "delivery"
        allow = ["app", "i1", "i2"]
    "#;

    fn pkg(name: &str, deps: &[&str]) -> Package {
        Package {
            name: name.into(),
            manifest: PathBuf::from(format!("{name}/Cargo.toml")),
            dependencies: deps
                .iter()
                .map(|d| Dependency {
                    name: (*d).into(),
                    req: "*".into(),
                    dev: false,
                })
                .collect(),
        }
    }

    fn base() -> Vec<Package> {
        vec![
            pkg("d1", &[]),
            pkg("d2", &["d1"]),
            pkg("app", &["d1"]),
            pkg("i1", &["app"]),
            pkg("i2", &["app"]),
            pkg("cli", &["app", "i1", "i2"]),
        ]
    }

    fn run(packages: &[Package]) -> Vec<String> {
        check(&Declaration::parse(DECL).unwrap(), packages)
    }

    fn replace(packages: &mut [Package], name: &str, deps: &[&str]) {
        let slot = packages.iter_mut().find(|p| p.name == name).unwrap();
        *slot = pkg(name, deps);
    }

    fn assert_names(errors: &[String], a: &str, b: &str) {
        assert!(
            errors
                .iter()
                .any(|e| e.contains(&format!("`{a}`")) && e.contains(&format!("`{b}`"))),
            "expected an error naming {a} and {b}, got {errors:?}"
        );
    }

    #[test]
    fn permitted_graph_passes() {
        assert!(run(&base()).is_empty());
    }

    #[test]
    fn domain_depending_outward_fails() {
        let mut packages = base();
        replace(&mut packages, "d1", &["i1"]);
        assert_names(&run(&packages), "d1", "i1");
    }

    #[test]
    fn application_on_concrete_infrastructure_fails() {
        let mut packages = base();
        replace(&mut packages, "app", &["d1", "i1"]);
        let errors = run(&packages);
        assert_names(&errors, "app", "i1");
        assert!(
            errors
                .iter()
                .any(|e| e.contains("application crates may depend on domain"))
        );
    }

    #[test]
    fn undeclared_infrastructure_edge_fails() {
        let mut packages = base();
        replace(&mut packages, "i1", &["app", "i2"]);
        assert_names(&run(&packages), "i1", "i2");
    }

    #[test]
    fn unlisted_crate_fails() {
        let mut packages = base();
        packages.push(pkg("stray", &[]));
        assert!(run(&packages).iter().any(|e| e.contains("`stray`")));
    }

    #[test]
    fn cycle_fails() {
        let mut packages = base();
        replace(&mut packages, "d1", &["d2"]);
        let errors = run(&packages);
        assert!(
            errors
                .iter()
                .any(|e| e.contains("cycle") && e.contains("d1") && e.contains("d2"))
        );
    }

    const TOOLING_DECL: &str = r#"
        [crate.d1]
        layer = "domain"
        allow = []
        [crate.app]
        layer = "application"
        allow = ["d1"]
        [crate.i1]
        layer = "infrastructure"
        allow = ["d1"]
        [crate.suite]
        layer = "tooling"
        allow = ["d1"]
    "#;

    fn dev(mut package: Package, name: &str) -> Package {
        package.dependencies.push(Dependency {
            name: name.into(),
            req: "*".into(),
            dev: true,
        });
        package
    }

    fn run_tooling(packages: &[Package]) -> Vec<String> {
        check(&Declaration::parse(TOOLING_DECL).unwrap(), packages)
    }

    #[test]
    fn tooling_may_depend_on_domain_and_be_a_dev_dependency() {
        let packages = [
            pkg("d1", &[]),
            pkg("app", &["d1"]),
            dev(pkg("i1", &["d1"]), "suite"),
            pkg("suite", &["d1"]),
        ];
        assert!(run_tooling(&packages).is_empty());
    }

    #[test]
    fn normal_dependency_on_tooling_fails() {
        let packages = [
            pkg("d1", &[]),
            pkg("app", &["d1", "suite"]),
            pkg("i1", &["d1"]),
            pkg("suite", &["d1"]),
        ];
        assert_names(&run_tooling(&packages), "app", "suite");
    }

    #[test]
    fn tooling_depending_on_application_fails() {
        let packages = [
            pkg("d1", &[]),
            pkg("app", &["d1"]),
            pkg("i1", &["d1"]),
            pkg("suite", &["d1", "app"]),
        ];
        assert_names(&run_tooling(&packages), "suite", "app");
    }

    #[test]
    fn depending_on_delivery_fails() {
        let mut packages = base();
        replace(&mut packages, "i1", &["app", "cli"]);
        assert_names(&run(&packages), "i1", "cli");
    }
}
