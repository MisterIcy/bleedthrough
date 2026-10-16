//! The reviewed file `stubs/conflicts.toml`: how to resolve a symbol that one PHP version declares
//! more than once with details that differ.
//!
//! Declarations that are equal (apart from SAPI, guard and alias target) merge without an entry.
//! Any other conflict needs an entry that says which variant is kept and why, and an entry that
//! no longer matches anything is an error, so the file cannot go stale.

use std::collections::BTreeSet;

use bleedthrough_builtins::{Kind, normalise_name};

/// One reviewed resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub kind: Kind,
    /// The symbol's name as written in the file.
    pub name: String,
    pub reason: String,
    /// Keep only the declarations under exactly this guard.
    pub keep_guard: Option<String>,
    /// Drop the declarations that come from this SAPI.
    pub drop_sapi: Option<String>,
    /// Drop the value (constants) or default (properties): it differs between the variants.
    pub clear_value: bool,
    /// The extension to record, when the variants disagree.
    pub extension: Option<String>,
}

/// The parsed file, remembering which entries were used.
#[derive(Debug, Clone, Default)]
pub struct Conflicts {
    entries: Vec<Conflict>,
    used: BTreeSet<usize>,
}

impl Conflicts {
    /// No entries.
    pub fn none() -> Self {
        Self::default()
    }

    /// Reads `stubs/conflicts.toml`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let table: toml::Table = text.parse().map_err(|e| format!("conflicts.toml: {e}"))?;
        let mut entries = Vec::new();
        let list = table
            .get("conflict")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for (index, item) in list.iter().enumerate() {
            let get = |key: &str| item.get(key).and_then(|v| v.as_str()).map(str::to_string);
            let here = format!("conflicts.toml entry {}", index + 1);
            let kind = get("kind")
                .and_then(|k| Kind::parse(&k))
                .ok_or_else(|| format!("{here}: `kind` is missing or unknown"))?;
            let name = get("name").ok_or_else(|| format!("{here}: `name` is missing"))?;
            let reason = get("reason")
                .filter(|r| !r.trim().is_empty())
                .ok_or_else(|| format!("{here} ({name}): a `reason` is required"))?;
            entries.push(Conflict {
                kind,
                name,
                reason,
                keep_guard: get("keep_guard"),
                drop_sapi: get("drop_sapi"),
                clear_value: item
                    .get("clear_value")
                    .and_then(toml::Value::as_bool)
                    .unwrap_or(false),
                extension: get("extension"),
            });
        }
        Ok(Self {
            entries,
            used: BTreeSet::new(),
        })
    }

    /// The entry for a symbol key, marking it used.
    pub fn take(&mut self, kind: Kind, key_name: &str) -> Option<Conflict> {
        let index = self
            .entries
            .iter()
            .position(|e| e.kind == kind && normalise_name(kind, &e.name) == key_name)?;
        self.used.insert(index);
        self.entries.get(index).cloned()
    }

    /// Entries that resolved no conflict in any version.
    pub fn unused(&self) -> Vec<&Conflict> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(i, _)| !self.used.contains(i))
            .map(|(_, e)| e)
            .collect()
    }

    /// The entries that were used, in file order.
    pub fn used(&self) -> Vec<&Conflict> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(i, _)| self.used.contains(i))
            .map(|(_, e)| e)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"
[[conflict]]
kind = "function"
name = "ldap_connect"
keep_guard = "!defined(HAVE_ORALDAP)"
reason = "default builds use OpenLDAP"

[[conflict]]
kind = "const"
name = "PASSWORD_ARGON2_PROVIDER"
clear_value = true
extension = "standard"
reason = "which provider is a build choice"
"#;

    #[test]
    fn entries_are_read_and_tracked() {
        let mut c = Conflicts::parse(FILE).unwrap();
        assert_eq!(c.unused().len(), 2);
        let e = c.take(Kind::Function, "ldap_connect").unwrap();
        assert_eq!(e.keep_guard.as_deref(), Some("!defined(HAVE_ORALDAP)"));
        assert!(c.take(Kind::Function, "other").is_none());
        assert_eq!(c.unused().len(), 1);
        assert_eq!(c.used().len(), 1);
    }

    #[test]
    fn an_entry_without_a_reason_is_rejected() {
        let text = "[[conflict]]\nkind = \"function\"\nname = \"f\"\n";
        assert!(Conflicts::parse(text).unwrap_err().contains("reason"));
        assert!(Conflicts::parse("[[conflict]]\nname = \"f\"\nreason = \"r\"\n").is_err());
    }
}
