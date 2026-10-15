//! Manifest policy: workspace inheritance, domain purity, parser isolation and pinning.

use crate::declaration::{Declaration, Layer, matches};
use crate::meta::Package;

const INHERITED: [&str; 5] = [
    "edition",
    "rust-version",
    "license",
    "repository",
    "publish",
];

/// Every crate must inherit package metadata and lints from the workspace.
pub fn check_inheritance(manifests: &[(String, String)]) -> Vec<String> {
    let mut errors = Vec::new();
    for (name, text) in manifests {
        let Ok(table) = text.parse::<toml::Table>() else {
            errors.push(format!("{name}: manifest does not parse"));
            continue;
        };
        for key in INHERITED {
            if !is_inherited(table.get("package").and_then(|p| p.get(key))) {
                errors.push(format!(
                    "{name}: `{key}` must be inherited (`{key}.workspace = true`)"
                ));
            }
        }
        if !is_inherited(table.get("lints")) {
            errors.push(format!("{name}: `[lints]` must be `workspace = true`"));
        }
    }
    errors
}

fn is_inherited(value: Option<&toml::Value>) -> bool {
    value
        .and_then(|v| v.get("workspace"))
        .and_then(|v| v.as_bool())
        == Some(true)
}

/// The workspace root must define the values every crate inherits.
pub fn check_workspace_root(text: &str) -> Vec<String> {
    let Ok(table) = text.parse::<toml::Table>() else {
        return vec!["root Cargo.toml does not parse".into()];
    };
    let package = table.get("workspace").and_then(|w| w.get("package"));
    let expect = [
        ("edition", "2024"),
        ("rust-version", "1.99"),
        ("license", "MIT OR Apache-2.0"),
    ];
    let mut errors: Vec<String> = expect
        .iter()
        .filter(|(key, value)| {
            package.and_then(|p| p.get(*key)).and_then(|v| v.as_str()) != Some(*value)
        })
        .map(|(key, value)| format!("[workspace.package] `{key}` must be \"{value}\""))
        .collect();
    if package
        .and_then(|p| p.get("publish"))
        .and_then(|v| v.as_bool())
        != Some(false)
    {
        errors.push("[workspace.package] `publish` must be false".into());
    }
    errors
}

/// Domain manifests must not depend on infrastructure crates.
pub fn check_purity(declaration: &Declaration, packages: &[Package]) -> Vec<String> {
    let mut errors = Vec::new();
    for package in domain_packages(declaration, packages) {
        for dep in &package.dependencies {
            if declaration
                .domain_denied
                .iter()
                .any(|p| matches(p, &dep.name))
            {
                errors.push(format!(
                    "`{}` (domain) depends on infrastructure crate `{}`; move the impure work behind a port",
                    package.name, dep.name
                ));
            }
        }
    }
    errors
}

fn domain_packages<'a>(
    declaration: &'a Declaration,
    packages: &'a [Package],
) -> impl Iterator<Item = &'a Package> {
    packages
        .iter()
        .filter(|p| declaration.layer_of(&p.name) == Some(Layer::Domain))
}

/// Parser backends are reachable only from adapter crates, and only at an exact version.
pub fn check_parser_backends(declaration: &Declaration, packages: &[Package]) -> Vec<String> {
    let mut errors = Vec::new();
    for package in packages {
        let adapter = declaration
            .crates
            .get(&package.name)
            .is_some_and(|c| c.adapter);
        for dep in &package.dependencies {
            if !declaration
                .parser_backends
                .iter()
                .any(|p| matches(p, &dep.name))
            {
                continue;
            }
            if !adapter {
                errors.push(format!(
                    "`{}` depends on parser backend `{}` but is not an adapter crate",
                    package.name, dep.name
                ));
            } else if !dep.req.starts_with('=') {
                errors.push(format!(
                    "`{}` must pin parser backend `{}` exactly (`=x.y.z`), found `{}`",
                    package.name, dep.name, dep.req
                ));
            }
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::Dependency;
    use std::path::PathBuf;

    const DECL: &str = r#"
        [policy]
        domain_denied = ["walkdir", "tokio*"]
        parser_backends = ["mago-*"]
        [crate.dom]
        layer = "domain"
        [crate.adapter]
        layer = "infrastructure"
        adapter = true
    "#;

    fn pkg(name: &str, deps: &[(&str, &str)]) -> Package {
        Package {
            name: name.into(),
            manifest: PathBuf::new(),
            dependencies: deps
                .iter()
                .map(|(n, r)| Dependency {
                    name: (*n).into(),
                    req: (*r).into(),
                    dev: false,
                })
                .collect(),
        }
    }

    #[test]
    fn crate_overriding_inherited_metadata_fails() {
        let good = "[package]\nname=\"a\"\nedition.workspace=true\nrust-version.workspace=true\nlicense.workspace=true\nrepository.workspace=true\npublish.workspace=true\n[lints]\nworkspace=true\n";
        let bad = good.replace("rust-version.workspace=true", "rust-version=\"1.80\"");
        assert!(check_inheritance(&[("a".into(), good.into())]).is_empty());
        let errors = check_inheritance(&[("b".into(), bad)]);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("rust-version"));
    }

    #[test]
    fn missing_lints_inheritance_fails() {
        let text = "[package]\nedition.workspace=true\nrust-version.workspace=true\nlicense.workspace=true\nrepository.workspace=true\npublish.workspace=true\n";
        assert!(
            check_inheritance(&[("a".into(), text.into())])
                .iter()
                .any(|e| e.contains("lints"))
        );
    }

    #[test]
    fn workspace_root_values_are_checked() {
        let ok = "[workspace.package]\nedition=\"2024\"\nrust-version=\"1.99\"\nlicense=\"MIT OR Apache-2.0\"\npublish=false\n";
        assert!(check_workspace_root(ok).is_empty());
        assert_eq!(check_workspace_root(&ok.replace("1.99", "1.70")).len(), 1);
    }

    #[test]
    fn banned_crate_in_domain_manifest_fails() {
        let declaration = Declaration::parse(DECL).unwrap();
        let errors = check_purity(&declaration, &[pkg("dom", &[("tokio-util", "1")])]);
        assert!(errors[0].contains("`dom`") && errors[0].contains("`tokio-util`"));
        assert!(check_purity(&declaration, &[pkg("dom", &[("serde", "1")])]).is_empty());
    }

    #[test]
    fn parser_backend_outside_adapter_fails() {
        let declaration = Declaration::parse(DECL).unwrap();
        let errors =
            check_parser_backends(&declaration, &[pkg("dom", &[("mago-syntax", "=1.57.0")])]);
        assert!(errors[0].contains("`dom`") && errors[0].contains("`mago-syntax`"));
    }

    #[test]
    fn floating_parser_version_fails_and_exact_passes() {
        let declaration = Declaration::parse(DECL).unwrap();
        let floating =
            check_parser_backends(&declaration, &[pkg("adapter", &[("mago-syntax", "^1.57")])]);
        assert!(floating[0].contains("exactly"));
        assert!(
            check_parser_backends(
                &declaration,
                &[pkg("adapter", &[("mago-syntax", "=1.57.0")])]
            )
            .is_empty()
        );
    }
}
