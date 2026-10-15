//! Reads one rule file into [`LocatedRule`]s.
//!
//! The file is parsed into a spanned TOML tree and walked by hand against the known keys, so that
//! every unknown key, wrong type and bad value in the file is reported, each with its line and
//! key, rather than only the first.

use std::collections::BTreeMap;

use bleedthrough_rules::{
    ArgConditionDraft, BodyDraft, ExtraCategoryDraft, LocatedError, LocatedRule, MetricsDraft,
    Origin, PatternDraft, PropagatorDraft, RuleDraft, SCHEMA_VERSION, SanitizerDraft, SinkDraft,
    TaintDraft,
};
use toml::Spanned;
use toml::de::{DeTable, DeValue};

type Table<'a> = DeTable<'a>;
type Value<'a> = Spanned<DeValue<'a>>;

const RULE_KEYS: [&str; 15] = [
    "id",
    "title",
    "description",
    "remediation",
    "area",
    "references",
    "cwe",
    "owasp",
    "owasp_extra",
    "paranoid",
    "php_min",
    "php_max",
    "pack",
    "metrics",
    "taint",
];
const METRIC_KEYS: [&str; 8] = [
    "attack_vector",
    "attack_complexity",
    "privileges_required",
    "user_interaction",
    "scope",
    "confidentiality",
    "integrity",
    "availability",
];

/// What was read from one file.
pub(crate) struct FileRules {
    pub rules: Vec<LocatedRule>,
    pub errors: Vec<LocatedError>,
}

struct Reader<'a> {
    file: &'a str,
    starts: Vec<usize>,
    errors: Vec<LocatedError>,
    lines: BTreeMap<String, u32>,
}

impl<'a> Reader<'a> {
    fn line(&self, offset: usize) -> u32 {
        let index = self.starts.partition_point(|s| *s <= offset);
        u32::try_from(index).unwrap_or(u32::MAX).max(1)
    }

    fn error(&mut self, offset: usize, key: Option<&str>, message: impl Into<String>) {
        let line = self.line(offset);
        self.errors.push(LocatedError::new(
            self.file,
            Some(line),
            key.map(str::to_string),
            message,
        ));
    }

    /// Records the line of a field path for later semantic errors.
    fn mark(&mut self, path: &str, offset: usize) {
        let line = self.line(offset);
        self.lines.entry(path.to_string()).or_insert(line);
    }

    fn unknown_keys(&mut self, table: &Table<'_>, allowed: &[&str], within: &str) {
        for (key, _) in table.iter() {
            if !allowed.contains(&key.get_ref().as_ref()) {
                let message = format!(
                    "unknown key `{}`{within}; allowed keys: {}",
                    key.get_ref(),
                    allowed.join(", ")
                );
                self.error(key.span().start, Some(key.get_ref()), message);
            }
        }
    }

    fn str_at(&mut self, table: &Table<'_>, key: &str, path: &str) -> Option<String> {
        let value = table.get(key)?;
        self.mark(path, value.span().start);
        match value.get_ref() {
            DeValue::String(s) => Some(s.to_string()),
            other => {
                self.wrong_type(value, key, "a string", other);
                None
            }
        }
    }

    fn wrong_type(&mut self, value: &Value<'_>, key: &str, expected: &str, found: &DeValue<'_>) {
        let message = format!("`{key}` must be {expected}, found {}", found.type_str());
        self.error(value.span().start, Some(key), message);
    }

    fn bool_at(&mut self, table: &Table<'_>, key: &str, path: &str) -> Option<bool> {
        let value = table.get(key)?;
        self.mark(path, value.span().start);
        match value.get_ref() {
            DeValue::Boolean(b) => Some(*b),
            other => {
                self.wrong_type(value, key, "a boolean", other);
                None
            }
        }
    }

    /// A list of strings, or, when `numbers` is set, of strings and integers (argument selectors).
    fn list_at(&mut self, table: &Table<'_>, key: &str, path: &str, numbers: bool) -> Vec<String> {
        let Some(value) = table.get(key) else {
            return vec![];
        };
        self.mark(path, value.span().start);
        let DeValue::Array(items) = value.get_ref() else {
            self.wrong_type(value, key, "an array", value.get_ref());
            return vec![];
        };
        let mut out = Vec::new();
        for (i, item) in items.iter().enumerate() {
            self.mark(&format!("{path}[{i}]"), item.span().start);
            match item.get_ref() {
                DeValue::String(s) => out.push(s.to_string()),
                DeValue::Integer(n) if numbers => out.push(n.as_str().to_string()),
                other => {
                    let expected = if numbers {
                        "a string or an integer"
                    } else {
                        "a string"
                    };
                    let message = format!(
                        "`{key}[{i}]` must be {expected}, found {}",
                        other.type_str()
                    );
                    self.error(item.span().start, Some(key), message);
                }
            }
        }
        out
    }

    fn table_at<'t, 'i>(
        &mut self,
        table: &'t Table<'i>,
        key: &str,
        path: &str,
    ) -> Option<&'t Table<'i>> {
        let value = table.get(key)?;
        self.mark(path, value.span().start);
        match value.get_ref() {
            DeValue::Table(t) => Some(t),
            other => {
                self.wrong_type(value, key, "a table", other);
                None
            }
        }
    }

    /// An array of tables (`[[...]]` or an array of inline tables), each with its start offset.
    fn tables_at<'t, 'i>(
        &mut self,
        table: &'t Table<'i>,
        key: &str,
    ) -> Vec<(&'t Table<'i>, usize)> {
        let Some(value) = table.get(key) else {
            return vec![];
        };
        let DeValue::Array(items) = value.get_ref() else {
            self.wrong_type(value, key, "an array of tables", value.get_ref());
            return vec![];
        };
        let mut out = Vec::new();
        for item in items.iter() {
            match item.get_ref() {
                DeValue::Table(t) => out.push((t, item.span().start)),
                other => {
                    let message =
                        format!("`{key}` entries must be tables, found {}", other.type_str());
                    self.error(item.span().start, Some(key), message);
                }
            }
        }
        out
    }

    fn metrics(&mut self, rule: &Table<'_>) -> MetricsDraft {
        let Some(t) = self.table_at(rule, "metrics", "metrics") else {
            return MetricsDraft::default();
        };
        self.unknown_keys(t, &METRIC_KEYS, " in `metrics`");
        let mut get = |key: &str| self.str_at(t, key, &format!("metrics.{key}"));
        MetricsDraft {
            attack_vector: get("attack_vector"),
            attack_complexity: get("attack_complexity"),
            privileges_required: get("privileges_required"),
            user_interaction: get("user_interaction"),
            scope: get("scope"),
            confidentiality: get("confidentiality"),
            integrity: get("integrity"),
            availability: get("availability"),
        }
    }

    fn extras(&mut self, rule: &Table<'_>) -> Vec<ExtraCategoryDraft> {
        let mut out = Vec::new();
        for (i, (t, at)) in self.tables_at(rule, "owasp_extra").into_iter().enumerate() {
            self.mark(&format!("owasp_extra[{i}]"), at);
            self.unknown_keys(t, &["category", "reason"], " in `owasp_extra`");
            let category = self.str_at(t, "category", &format!("owasp_extra[{i}].category"));
            let reason = self.str_at(t, "reason", &format!("owasp_extra[{i}].reason"));
            out.push(ExtraCategoryDraft {
                category: category.unwrap_or_default(),
                reason: reason.unwrap_or_default(),
            });
        }
        out
    }

    fn taint(&mut self, t: &Table<'_>) -> TaintDraft {
        self.unknown_keys(
            t,
            &["sources", "sinks", "sanitizers", "propagators"],
            " in `taint`",
        );
        let sources = self.list_at(t, "sources", "body.sources", false);
        let mut taint = TaintDraft {
            sources,
            ..TaintDraft::default()
        };
        for (i, (s, at)) in self.tables_at(t, "sinks").into_iter().enumerate() {
            let p = format!("body.sinks[{i}]");
            self.mark(&p, at);
            self.unknown_keys(s, &["symbol", "args", "class"], " in a sink");
            taint.sinks.push(SinkDraft {
                symbol: self
                    .str_at(s, "symbol", &format!("{p}.symbol"))
                    .unwrap_or_default(),
                args: self.list_at(s, "args", &format!("{p}.args"), true),
                class: self
                    .str_at(s, "class", &format!("{p}.class"))
                    .unwrap_or_default(),
            });
        }
        for (i, (s, at)) in self.tables_at(t, "sanitizers").into_iter().enumerate() {
            let p = format!("body.sanitizers[{i}]");
            self.mark(&p, at);
            self.unknown_keys(s, &["symbol", "clears"], " in a sanitizer");
            taint.sanitizers.push(SanitizerDraft {
                symbol: self
                    .str_at(s, "symbol", &format!("{p}.symbol"))
                    .unwrap_or_default(),
                clears: self.list_at(s, "clears", &format!("{p}.clears"), false),
            });
        }
        for (i, (s, at)) in self.tables_at(t, "propagators").into_iter().enumerate() {
            let p = format!("body.propagators[{i}]");
            self.mark(&p, at);
            self.unknown_keys(s, &["symbol", "from"], " in a propagator");
            taint.propagators.push(PropagatorDraft {
                symbol: self
                    .str_at(s, "symbol", &format!("{p}.symbol"))
                    .unwrap_or_default(),
                from: self.list_at(s, "from", &format!("{p}.from"), true),
            });
        }
        taint
    }

    fn pattern(&mut self, t: &Table<'_>) -> PatternDraft {
        self.unknown_keys(t, &["shape", "symbol", "args"], " in `pattern`");
        let mut pattern = PatternDraft {
            shape: self.str_at(t, "shape", "body.shape").unwrap_or_default(),
            symbol: self.str_at(t, "symbol", "body.symbol"),
            args: vec![],
        };
        for (i, (a, at)) in self.tables_at(t, "args").into_iter().enumerate() {
            let p = format!("body.args[{i}]");
            self.mark(&p, at);
            self.unknown_keys(a, &["arg", "test", "value"], " in an argument condition");
            let arg = match a.get("arg").map(|v| v.get_ref()) {
                Some(DeValue::Integer(n)) => Some(n.as_str().to_string()),
                _ => self.str_at(a, "arg", &format!("{p}.arg")),
            };
            self.mark(
                &format!("{p}.arg"),
                a.get("arg").map_or(at, |v| v.span().start),
            );
            pattern.args.push(ArgConditionDraft {
                arg: arg.unwrap_or_default(),
                test: self
                    .str_at(a, "test", &format!("{p}.test"))
                    .unwrap_or_default(),
                value: self.str_at(a, "value", &format!("{p}.value")),
            });
        }
        pattern
    }

    /// The body: exactly one of `[rule.taint]` and `[rule.pattern]`.
    fn body(&mut self, rule: &Table<'_>, at: usize) -> Option<BodyDraft> {
        let taint = self
            .table_at(rule, "taint", "body")
            .map(|t| BodyDraft::Taint(self.taint(t)));
        let pattern = self
            .table_at(rule, "pattern", "body")
            .map(|t| BodyDraft::Pattern(self.pattern(t)));
        match (taint, pattern) {
            (Some(_), Some(_)) => {
                self.error(
                    at,
                    Some("pattern"),
                    "a rule has a `taint` or a `pattern` body, not both",
                );
                None
            }
            (body, None) | (None, body) => body,
        }
    }

    fn rule(&mut self, t: &Table<'_>, at: usize) -> RuleDraft {
        let mut allowed = RULE_KEYS.to_vec();
        allowed.push("pattern");
        self.unknown_keys(t, &allowed, " in a rule");
        let (cwe, owasp) = (
            self.list_at(t, "cwe", "cwe", false),
            self.list_at(t, "owasp", "owasp", false),
        );
        RuleDraft {
            id: self.str_at(t, "id", "id"),
            title: self.str_at(t, "title", "title"),
            description: self.str_at(t, "description", "description"),
            remediation: self.str_at(t, "remediation", "remediation"),
            area: self.str_at(t, "area", "area"),
            references: self.list_at(t, "references", "references", false),
            cwe,
            owasp,
            owasp_extra: self.extras(t),
            metrics: self.metrics(t),
            paranoid: self.bool_at(t, "paranoid", "paranoid").unwrap_or(false),
            php_min: self.str_at(t, "php_min", "php_min"),
            php_max: self.str_at(t, "php_max", "php_max"),
            pack: self.str_at(t, "pack", "pack"),
            body: self.body(t, at),
        }
    }

    /// The `schema` key: an integer major version no higher than the supported one.
    fn schema(&mut self, root: &Table<'_>) -> bool {
        let Some(value) = root.get("schema") else {
            self.error(
                0,
                Some("schema"),
                "the file must declare `schema = 1` at the top",
            );
            return false;
        };
        let DeValue::Integer(n) = value.get_ref() else {
            self.wrong_type(value, "schema", "an integer", value.get_ref());
            return false;
        };
        match n.as_str().parse::<u32>() {
            Ok(v) if v == SCHEMA_VERSION => true,
            Ok(v) if v > SCHEMA_VERSION => {
                let message = format!(
                    "the file declares schema {v} but this tool supports schema {SCHEMA_VERSION}"
                );
                self.error(value.span().start, Some("schema"), message);
                false
            }
            _ => {
                let message = format!(
                    "schema {:?} is not a supported version; this tool supports schema {SCHEMA_VERSION}",
                    n.as_str()
                );
                self.error(value.span().start, Some("schema"), message);
                false
            }
        }
    }
}

/// Reads one rule file. `file` is the name used in messages.
pub(crate) fn read_file(file: &str, text: &str) -> FileRules {
    let mut starts = vec![0];
    starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    let mut reader = Reader {
        file,
        starts,
        errors: vec![],
        lines: BTreeMap::new(),
    };
    let (doc, syntax) = DeTable::parse_recoverable(text);
    if !syntax.is_empty() {
        for e in &syntax {
            let at = e.span().map_or(0, |s| s.start);
            reader.error(at, None, e.message().to_string());
        }
        return FileRules {
            rules: vec![],
            errors: reader.errors,
        };
    }
    let root = doc.get_ref();
    reader.unknown_keys(root, &["schema", "rule"], " at the top of the file");
    if !reader.schema(root) {
        return FileRules {
            rules: vec![],
            errors: reader.errors,
        };
    }
    let mut rules = Vec::new();
    for (table, at) in reader.tables_at(root, "rule") {
        reader.lines.clear();
        let draft = reader.rule(table, at);
        let origin = Origin {
            file: file.to_string(),
            line: reader.line(at),
        };
        rules.push(LocatedRule {
            draft,
            origin,
            lines: std::mem::take(&mut reader.lines),
        });
    }
    reader
        .errors
        .sort_by_key(|e| (e.line(), e.key().map(str::to_string)));
    FileRules {
        rules,
        errors: reader.errors,
    }
}
