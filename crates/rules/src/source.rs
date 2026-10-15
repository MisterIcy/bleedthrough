use std::collections::BTreeMap;
use std::fmt;

use crate::draft::RuleDraft;
use crate::id::RuleId;
use crate::rule::Origin;
use crate::set::RuleSet;

/// Term: exempt
///
/// A problem with a rule file, located: the file, the line and the key when known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedError {
    file: String,
    line: Option<u32>,
    key: Option<String>,
    message: String,
}

impl LocatedError {
    /// An error in `file`, optionally at a line and a key.
    pub fn new(
        file: impl Into<String>,
        line: Option<u32>,
        key: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            file: file.into(),
            line,
            key,
            message: message.into(),
        }
    }

    /// The file.
    pub fn file(&self) -> &str {
        &self.file
    }

    /// The line, from 1, when known.
    pub fn line(&self) -> Option<u32> {
        self.line
    }

    /// The key, when known.
    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    /// What is wrong.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for LocatedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.file)?;
        if let Some(line) = self.line {
            write!(f, ":{line}")?;
        }
        f.write_str(": ")?;
        if let Some(key) = &self.key {
            write!(f, "{key}: ")?;
        }
        f.write_str(&self.message)
    }
}

impl std::error::Error for LocatedError {}

/// Term: Rule
///
/// One rule as a source read it: the draft, where it was defined and the line of each key.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    clippy::exhaustive_structs,
    reason = "a plain record filled in by rule sources"
)]
pub struct LocatedRule {
    /// The rule as written.
    pub draft: RuleDraft,
    /// Where the rule starts.
    pub origin: Origin,
    /// The line of each field path (`remediation`, `sinks[0].class`), for error locations.
    pub lines: BTreeMap<String, u32>,
}

/// Term: Rule
///
/// What a source yields: the rules it read, the retired identifiers, and the errors it found
/// while reading (unreadable or malformed files, unknown keys, wrong types).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(
    clippy::exhaustive_structs,
    reason = "a plain record filled in by rule sources"
)]
pub struct SourceOutput {
    /// The rules read, in any order.
    pub rules: Vec<LocatedRule>,
    /// The retired identifiers, as written.
    pub retired: Vec<String>,
    /// Problems found while reading.
    pub errors: Vec<LocatedError>,
}

/// Term: Rule
///
/// The port through which rules enter the system. A TOML directory is one implementation; any
/// other source is validated identically.
pub trait RuleSource {
    /// Reads every rule it can and reports every problem it finds.
    fn load(&self) -> SourceOutput;
}

/// The line for a field path: the exact key, else the closest enclosing key, else the rule's own.
fn line_of(rule: &LocatedRule, field: &str) -> u32 {
    let mut path = field;
    loop {
        if let Some(line) = rule.lines.get(path) {
            return *line;
        }
        match path.rfind(['.', '[']) {
            Some(i) => path = &path[..i],
            None => return rule.origin.line,
        }
    }
}

/// Loads, validates and assembles a rule set from a source. Every error of every rule is
/// reported, with its file, line and key.
pub fn load_rule_set(source: &dyn RuleSource) -> Result<RuleSet, Vec<LocatedError>> {
    let output = source.load();
    let mut errors = output.errors.clone();
    let mut retired = Vec::new();
    for text in &output.retired {
        match RuleId::new(text) {
            Ok(id) => retired.push(id),
            Err(e) => errors.push(LocatedError::new("rules/RETIRED.md", None, None, e)),
        }
    }
    let mut rules = Vec::new();
    for located in &output.rules {
        match located.draft.validate() {
            Ok(rule) => rules.push(rule.with_origin(located.origin.clone())),
            Err(list) => errors.extend(list.into_iter().filter_map(|e| {
                let line = line_of(located, e.field());
                // A value the source already rejected would only be reported again as missing.
                let known = output
                    .errors
                    .iter()
                    .any(|r| r.file() == located.origin.file && r.line() == Some(line));
                if known {
                    return None;
                }
                let message = match e.rule() {
                    Some(rule) => format!("rule {rule}: {}", e.message()),
                    None => e.message().to_string(),
                };
                Some(LocatedError::new(
                    &located.origin.file,
                    Some(line),
                    Some(e.field().to_string()),
                    message,
                ))
            })),
        }
    }
    match RuleSet::new(rules, &retired) {
        Ok(set) if errors.is_empty() => Ok(set),
        Ok(_) => Err(errors),
        Err(list) => {
            errors.extend(list.into_iter().map(|e| {
                let (file, line) = e
                    .origin()
                    .map_or((String::new(), None), |o| (o.file.clone(), Some(o.line)));
                LocatedError::new(
                    file,
                    line,
                    Some(e.field().to_string()),
                    e.message().to_string(),
                )
            }));
            Err(errors)
        }
    }
}
