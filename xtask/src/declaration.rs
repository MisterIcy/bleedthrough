//! The machine-readable layering declaration (`architecture.toml`).

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Domain,
    Application,
    Infrastructure,
    Delivery,
    Tooling,
}

impl Layer {
    fn parse(text: &str) -> Result<Self, String> {
        match text {
            "domain" => Ok(Self::Domain),
            "application" => Ok(Self::Application),
            "infrastructure" => Ok(Self::Infrastructure),
            "delivery" => Ok(Self::Delivery),
            "tooling" => Ok(Self::Tooling),
            other => Err(format!("unknown layer `{other}`")),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Domain => "domain",
            Self::Application => "application",
            Self::Infrastructure => "infrastructure",
            Self::Delivery => "delivery",
            Self::Tooling => "tooling",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Declared {
    pub layer: Layer,
    pub allow: BTreeSet<String>,
    /// Parser adapters are the only crates allowed to depend on a parser backend.
    pub adapter: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Declaration {
    pub crates: BTreeMap<String, Declared>,
    /// Crate-name patterns (`prefix*` or exact) banned from domain manifests.
    pub domain_denied: Vec<String>,
    /// Crate-name patterns identifying third-party PHP parser backends.
    pub parser_backends: Vec<String>,
}

impl Declaration {
    pub fn parse(text: &str) -> Result<Self, String> {
        let root: toml::Table = text
            .parse()
            .map_err(|e| format!("architecture.toml: {e}"))?;
        let mut declaration = Self::default();
        let crates = root
            .get("crate")
            .and_then(|v| v.as_table())
            .ok_or("missing [crate.*]")?;
        for (name, entry) in crates {
            declaration
                .crates
                .insert(name.clone(), parse_entry(name, entry)?);
        }
        let policy = root.get("policy").and_then(|v| v.as_table());
        declaration.domain_denied = string_list(policy, "domain_denied");
        declaration.parser_backends = string_list(policy, "parser_backends");
        Ok(declaration)
    }

    pub fn layer_of(&self, name: &str) -> Option<Layer> {
        self.crates.get(name).map(|c| c.layer)
    }
}

fn parse_entry(name: &str, entry: &toml::Value) -> Result<Declared, String> {
    let layer = entry
        .get("layer")
        .and_then(|v| v.as_str())
        .ok_or(format!("{name}: no layer"))?;
    let layer = Layer::parse(layer).map_err(|e| format!("{name}: {e}"))?;
    let allow = string_list(entry.as_table(), "allow").into_iter().collect();
    let adapter = entry
        .get("adapter")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    Ok(Declared {
        layer,
        allow,
        adapter,
    })
}

fn string_list(table: Option<&toml::Table>, key: &str) -> Vec<String> {
    table
        .and_then(|t| t.get(key))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Matches a crate name against an exact name or a `prefix*` pattern.
pub fn matches(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => pattern == name,
    }
}
