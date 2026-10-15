use std::collections::BTreeMap;

use crate::error::RuleError;
use crate::id::RuleId;
use crate::rule::Rule;

/// The rule-file schema major version this build understands, written into the canonical form.
pub const SCHEMA_VERSION: u32 = 1;

/// Term: Rule
///
/// A validated set of rules, ordered by identifier regardless of the order they were supplied in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSet {
    rules: BTreeMap<RuleId, Rule>,
}

fn place(rule: &Rule) -> String {
    rule.origin()
        .map_or_else(|| "an unknown place".to_string(), ToString::to_string)
}

impl RuleSet {
    /// Builds the set. Fails on a duplicate identifier (naming both locations) and on an
    /// identifier in `retired`, which can never be reissued; every problem is reported.
    pub fn new(rules: Vec<Rule>, retired: &[RuleId]) -> Result<Self, Vec<RuleError>> {
        let mut set = BTreeMap::new();
        let mut errors = Vec::new();
        for rule in rules {
            if retired.contains(rule.id()) {
                let message = format!(
                    "identifier {} is retired and cannot be reissued (see rules/RETIRED.md)",
                    rule.id()
                );
                errors.push(Self::error(&rule, message));
                continue;
            }
            if let Some(first) = set.get(rule.id()) {
                let message = format!(
                    "identifier {} is defined twice: at {} and at {}",
                    rule.id(),
                    place(first),
                    place(&rule)
                );
                errors.push(Self::error(&rule, message));
                continue;
            }
            set.insert(rule.id().clone(), rule);
        }
        if errors.is_empty() {
            Ok(Self { rules: set })
        } else {
            Err(errors)
        }
    }

    fn error(rule: &Rule, message: String) -> RuleError {
        let e = RuleError::new("id", message).in_rule(rule.id().as_str());
        match rule.origin() {
            Some(o) => e.at(o.clone()),
            None => e,
        }
    }

    /// The rules in identifier order.
    pub fn iter(&self) -> impl Iterator<Item = &Rule> {
        self.rules.values()
    }

    /// The rule with this identifier.
    pub fn get(&self, id: &RuleId) -> Option<&Rule> {
        self.rules.get(id)
    }

    /// How many rules.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Whether the set has no rules.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The canonical text of the whole set, in identifier order, for hashing. It depends only on
    /// rule content, never on file paths, comments, key order or enumeration order.
    pub fn canonical(&self) -> String {
        let mut text = format!("bleedthrough-rules schema={SCHEMA_VERSION}\n");
        for rule in self.rules.values() {
            text.push_str("---\n");
            text.push_str(&rule.canonical());
        }
        text
    }
}
