//! Workspace model loaded from `cargo metadata`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

/// A declared dependency of a workspace package.
#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub req: String,
    /// True for `[dev-dependencies]` (cargo metadata `kind` = `dev`).
    pub dev: bool,
}

/// A workspace member and its declared dependencies (all kinds).
#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    pub manifest: PathBuf,
    pub dependencies: Vec<Dependency>,
}

impl Package {
    pub fn dir(&self) -> PathBuf {
        self.manifest
            .parent()
            .map(PathBuf::from)
            .unwrap_or_default()
    }
}

/// Runs `cargo metadata --no-deps` and returns the workspace members.
pub fn load() -> Result<Vec<Package>, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|e| format!("cannot run cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let json: Value = serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    let packages = json["packages"]
        .as_array()
        .ok_or("metadata has no packages")?;
    Ok(packages.iter().map(parse_package).collect())
}

/// Runs `cargo metadata` with dependency resolution and returns, for every package in the
/// resolved graph, the names of the packages it depends on (all kinds).
pub fn load_graph() -> Result<BTreeMap<String, Vec<String>>, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--locked"])
        .output()
        .map_err(|e| format!("cannot run cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let json: Value = serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    Ok(resolved_graph(&json))
}

fn resolved_graph(json: &Value) -> BTreeMap<String, Vec<String>> {
    let names: BTreeMap<&str, &str> = json["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| Some((p["id"].as_str()?, p["name"].as_str()?)))
        .collect();
    let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for node in json["resolve"]["nodes"].as_array().into_iter().flatten() {
        let Some(name) = node["id"].as_str().and_then(|id| names.get(id)) else {
            continue;
        };
        let deps = node["deps"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| d["pkg"].as_str().and_then(|id| names.get(id)))
            .map(|n| (*n).to_string());
        graph.entry((*name).to_string()).or_default().extend(deps);
    }
    graph
}

fn parse_package(value: &Value) -> Package {
    let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
    let dependencies = value["dependencies"]
        .as_array()
        .map(|deps| {
            deps.iter()
                .map(|d| Dependency {
                    name: text(&d["name"]),
                    req: text(&d["req"]),
                    dev: d["kind"].as_str() == Some("dev"),
                })
                .collect()
        })
        .unwrap_or_default();
    Package {
        name: text(&value["name"]),
        manifest: PathBuf::from(text(&value["manifest_path"])),
        dependencies,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_resolved_graph_maps_ids_to_names() {
        let json: Value = serde_json::json!({
            "packages": [
                {"id": "a 1.0 (path+file:///a)", "name": "a"},
                {"id": "b 1.0 (registry+x)", "name": "b"}
            ],
            "resolve": {"nodes": [
                {"id": "a 1.0 (path+file:///a)", "deps": [{"pkg": "b 1.0 (registry+x)"}]},
                {"id": "b 1.0 (registry+x)", "deps": []}
            ]}
        });
        let graph = resolved_graph(&json);
        assert_eq!(graph["a"], vec!["b".to_string()]);
        assert!(graph["b"].is_empty());
    }
}
