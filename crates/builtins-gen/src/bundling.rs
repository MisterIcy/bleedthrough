//! The reviewed file `stubs/bundling.toml`: which extensions left the PHP distribution, and when.
//!
//! Whether a stub exists says only that php-src had the extension in that release; being
//! unbundled (moved to PECL) is a documented decision, so it is taken from this file, which cites
//! its sources, and checked against the stubs.

use bleedthrough_builtins::{ExtensionRecord, VersionMask};

/// One extension that is not bundled in some versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unbundled {
    pub name: String,
    pub unbundled: VersionMask,
    pub source: String,
}

/// The parsed file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Bundling {
    pub extensions: Vec<Unbundled>,
}

impl Bundling {
    /// Reads `stubs/bundling.toml`. Every entry must cite a source.
    pub fn parse(text: &str) -> Result<Self, String> {
        let table: toml::Table = text.parse().map_err(|e| format!("bundling.toml: {e}"))?;
        let mut extensions = Vec::new();
        let list = table
            .get("extension")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for item in &list {
            let get = |key: &str| item.get(key).and_then(|v| v.as_str()).map(str::to_string);
            let name = get("name").ok_or("bundling.toml: an entry has no `name`")?;
            let unbundled = get("unbundled")
                .ok_or_else(|| format!("bundling.toml: `{name}` has no `unbundled` versions"))?;
            let unbundled = VersionMask::parse(&unbundled)
                .map_err(|e| format!("bundling.toml: `{name}`: {e}"))?;
            let source = get("source")
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| format!("bundling.toml: `{name}` must cite a `source`"))?;
            extensions.push(Unbundled {
                name,
                unbundled,
                source,
            });
        }
        extensions.sort_by(|a, b| a.name.cmp(&b.name));
        if let Some(pair) = extensions.windows(2).find(|w| w[0].name == w[1].name) {
            return Err(format!("bundling.toml: `{}` is listed twice", pair[0].name));
        }
        Ok(Self { extensions })
    }

    /// The extension records for the data file.
    pub fn records(&self) -> Vec<ExtensionRecord> {
        self.extensions
            .iter()
            .map(|e| ExtensionRecord {
                name: e.name.clone(),
                unbundled: e.unbundled,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_need_a_source_and_valid_versions() {
        let ok = "[[extension]]\nname = \"imap\"\nunbundled = \"8.4-8.5\"\nsource = \"https://example.org\"\n";
        let b = Bundling::parse(ok).unwrap();
        assert_eq!(b.records()[0].unbundled.to_string(), "8.4-8.5");
        assert!(
            Bundling::parse(&ok.replace("source", "note"))
                .unwrap_err()
                .contains("source")
        );
        assert!(Bundling::parse(&ok.replace("8.4-8.5", "9.0")).is_err());
        assert!(
            Bundling::parse(&format!("{ok}{ok}"))
                .unwrap_err()
                .contains("twice")
        );
    }
}
