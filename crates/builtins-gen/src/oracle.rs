//! The cross-check: compares the generated data with a Reflection fixture.
//!
//! Every function, class and constant the fixture reports must be in the data for that version
//! with the same signature, hierarchy and values. The comparison goes one way (fixture to data):
//! the data has symbols the images cannot show, because they lack the extensions. Expected
//! differences are listed with reasons in `stubs/oracle-allow.toml` ([`Allowances`]).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use bleedthrough_builtins::{
    BuiltinData, ClassDetail, Detail, Kind, Lookup, Param, PhpVersion, Record, Signature,
    VersionMask, Visibility,
};

use crate::fixture::{Fixture, FixtureClass, FixtureParam, FixtureSignature, build_specific};

/// One way the data differs from a fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    pub version: PhpVersion,
    /// The symbol: a function, class or constant name, or `Class::member`.
    pub symbol: String,
    /// What differs, for example `presence`, `return type` or `param 1 ($x) by-reference`.
    pub field: String,
    /// What the fixture reports.
    pub expected: String,
    /// What the data has.
    pub actual: String,
}

impl fmt::Display for Difference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PHP {} {}: {}: Reflection `{}`, data `{}`",
            self.version, self.symbol, self.field, self.expected, self.actual
        )
    }
}

/// The type text in a form where equivalent spellings are equal: lower case, no leading
/// backslashes, `?T` as `T|null`, union members sorted, intersection members sorted.
pub fn normalise_type(text: &str) -> String {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let lower = compact.to_ascii_lowercase();
    let mut members = BTreeSet::new();
    for part in split_top(&lower, '|') {
        let (part, nullable) = match part.strip_prefix('?') {
            Some(rest) => (rest, true),
            None => (part, false),
        };
        if nullable {
            members.insert("null".to_string());
        }
        let member = intersection(part);
        if member == "iterable" {
            // Reflection spells `iterable` as `Traversable|array` from PHP 8.2.
            members.insert("array".to_string());
            members.insert("traversable".to_string());
        } else {
            members.insert(member);
        }
    }
    members.retain(|m| !m.is_empty());
    members.into_iter().collect::<Vec<_>>().join("|")
}

fn intersection(part: &str) -> String {
    let inner = part
        .strip_prefix('(')
        .and_then(|p| p.strip_suffix(')'))
        .unwrap_or(part);
    let mut names: Vec<&str> = split_top(inner, '&')
        .into_iter()
        .map(|n| n.trim_start_matches('\\'))
        .collect();
    names.sort_unstable();
    if names.len() > 1 {
        format!("({})", names.join("&"))
    } else {
        names.join("")
    }
}

/// Splits at `separator` outside parentheses.
fn split_top(text: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0usize, 0usize);
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            c if c == separator && depth == 0 => {
                parts.push(&text[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

fn visibility_text(visibility: Visibility) -> &'static str {
    visibility.as_str()
}

struct Checker<'a> {
    data: &'a BuiltinData,
    version: PhpVersion,
    out: Vec<Difference>,
}

impl Checker<'_> {
    fn differ(
        &mut self,
        symbol: &str,
        field: impl Into<String>,
        expected: impl ToString,
        actual: impl ToString,
    ) {
        self.out.push(Difference {
            version: self.version,
            symbol: symbol.to_string(),
            field: field.into(),
            expected: expected.to_string(),
            actual: actual.to_string(),
        });
    }

    /// Compares a boolean flag.
    fn flag(&mut self, symbol: &str, field: &str, expected: bool, actual: bool) {
        if expected != actual {
            self.differ(symbol, field, expected, actual);
        }
    }

    /// Compares two type spellings after normalisation.
    fn type_of(&mut self, symbol: &str, field: &str, expected: &str, actual: &str) {
        if normalise_type(expected) != normalise_type(actual) {
            self.differ(symbol, field, expected, actual);
        }
    }

    fn signature(&mut self, symbol: &str, expected: &FixtureSignature, actual: &Signature) {
        let params = &actual.params;
        if expected.params.len() != params.len() {
            self.differ(
                symbol,
                "parameter count",
                expected.params.len(),
                params.len(),
            );
        } else {
            for (i, (fx, p)) in expected.params.iter().zip(params).enumerate() {
                self.param(symbol, i + 1, fx, p);
            }
        }
        self.type_of(
            symbol,
            "return type",
            &expected.return_type,
            &actual.return_type,
        );
        self.type_of(
            symbol,
            "tentative return type",
            &expected.tentative_return_type,
            &actual.tentative_return_type,
        );
    }

    fn param(&mut self, symbol: &str, number: usize, fx: &FixtureParam, p: &Param) {
        let label = format!("param {number} (${})", fx.name);
        self.flag(
            symbol,
            &format!("{label} by-reference"),
            fx.by_ref,
            p.by_ref,
        );
        self.flag(
            symbol,
            &format!("{label} optional"),
            fx.optional,
            p.optional,
        );
        self.flag(
            symbol,
            &format!("{label} variadic"),
            fx.variadic,
            p.variadic,
        );
        self.type_of(
            symbol,
            &format!("{label} type"),
            &fx.type_text,
            &p.type_text,
        );
    }

    fn functions(&mut self, fixture: &Fixture) {
        for (name, fx) in &fixture.functions {
            match self.data.lookup(Kind::Function, name, self.version) {
                Lookup::Found(record) => {
                    if let Detail::Function(d) = &record.detail {
                        let s = &d.signature;
                        self.signature(name, &fx.signature, s);
                        self.flag(
                            name,
                            "returns by reference",
                            fx.signature.returns_ref,
                            s.return_by_ref,
                        );
                    }
                }
                _ => self.differ(name, "presence", "declared", "absent"),
            }
        }
    }

    fn constants(&mut self, fixture: &Fixture) {
        for (name, fx) in &fixture.constants {
            match self.data.lookup(Kind::Constant, name, self.version) {
                Lookup::Found(record) => {
                    if let Detail::Constant(d) = &record.detail
                        && build_specific(name).is_none()
                        && !fx.value.is_empty()
                        && d.value.as_deref() != Some(fx.value.as_str())
                    {
                        self.differ(
                            name,
                            "value",
                            &fx.value,
                            d.value.as_deref().unwrap_or("(none)"),
                        );
                    }
                }
                _ => self.differ(name, "presence", "declared", "absent"),
            }
        }
    }

    fn classes(&mut self, fixture: &Fixture) {
        for (name, fx) in &fixture.classes {
            let kind = match fx.kind.as_str() {
                "interface" => Kind::Interface,
                "trait" => Kind::Trait,
                "enum" => Kind::Enum,
                _ => Kind::Class,
            };
            let Lookup::Found(record) = self.data.lookup(kind, name, self.version) else {
                let found = [Kind::Class, Kind::Interface, Kind::Trait, Kind::Enum]
                    .into_iter()
                    .find(|k| matches!(self.data.lookup(*k, name, self.version), Lookup::Found(_)));
                match found {
                    Some(other) => self.differ(name, "kind", kind.as_str(), other.as_str()),
                    None => self.differ(name, "presence", "declared", "absent"),
                }
                continue;
            };
            let Some(detail) = class_detail(record) else {
                continue;
            };
            self.class(name, fx, record, detail);
            self.members(name, fx);
        }
    }

    fn class(&mut self, name: &str, fx: &FixtureClass, record: &Record, detail: &ClassDetail) {
        let kind = record.kind();
        if kind == Kind::Class {
            self.flag(name, "abstract", fx.is_abstract, detail.is_abstract);
        }
        if kind == Kind::Class || kind == Kind::Enum {
            self.flag(name, "final", fx.is_final, detail.is_final);
            self.flag(name, "readonly", fx.is_readonly, detail.is_readonly);
        }
        if kind == Kind::Class {
            let parent = detail.extends.first().map_or("", String::as_str);
            if !fx
                .parent
                .trim_start_matches('\\')
                .eq_ignore_ascii_case(parent.trim_start_matches('\\'))
            {
                self.differ(name, "parent", &fx.parent, parent);
            }
        }
        if kind == Kind::Enum {
            self.type_of(name, "backing type", &fx.backing_type, &detail.backing_type);
        }
        let expected: BTreeSet<String> = fx
            .interfaces
            .iter()
            .map(|i| i.trim_start_matches('\\').to_ascii_lowercase())
            .collect();
        let actual = self.all_interfaces(record);
        if expected != actual {
            let join = |s: &BTreeSet<String>| s.iter().cloned().collect::<Vec<_>>().join(", ");
            let missing: BTreeSet<_> = expected.difference(&actual).cloned().collect();
            let extra: BTreeSet<_> = actual.difference(&expected).cloned().collect();
            self.differ(
                name,
                "interfaces",
                format!("missing from the data: {}", join(&missing)),
                format!("only in the data: {}", join(&extra)),
            );
        }
    }

    /// Every interface of the class, as Reflection lists them: those named by the class, by its
    /// parents and by the interfaces themselves, and the ones PHP adds implicitly.
    fn all_interfaces(&self, record: &Record) -> BTreeSet<String> {
        let mut seen_types = BTreeSet::new();
        let mut interfaces = BTreeSet::new();
        let mut stringable = false;
        self.collect(record, &mut seen_types, &mut interfaces, &mut stringable);
        if stringable {
            interfaces.insert("stringable".to_string());
        }
        interfaces.remove(&record.name.to_ascii_lowercase());
        interfaces
    }

    fn collect(
        &self,
        record: &Record,
        seen: &mut BTreeSet<String>,
        interfaces: &mut BTreeSet<String>,
        stringable: &mut bool,
    ) {
        if !seen.insert(record.name.to_ascii_lowercase()) {
            return;
        }
        let Some(detail) = class_detail(record) else {
            return;
        };
        if self.data.members(&record.name, self.version).any(|m| {
            m.kind() == Kind::Method && m.name.to_ascii_lowercase().ends_with("::__tostring")
        }) {
            *stringable = true;
        }
        if record.kind() == Kind::Enum {
            interfaces.insert("unitenum".to_string());
            if !detail.backing_type.is_empty() {
                interfaces.insert("backedenum".to_string());
            }
        }
        let parents = detail.extends.iter().chain(&detail.implements);
        for parent in parents {
            let key = parent.trim_start_matches('\\');
            for kind in [Kind::Interface, Kind::Class] {
                if let Lookup::Found(found) = self.data.lookup(kind, key, self.version) {
                    if found.kind() == Kind::Interface {
                        interfaces.insert(found.name.to_ascii_lowercase());
                    }
                    self.collect(found, seen, interfaces, stringable);
                    break;
                }
            }
            // An interface the data does not know is still an interface of the class.
            if self.data.lookup(Kind::Interface, key, self.version) == Lookup::Unknown
                && self.data.lookup(Kind::Class, key, self.version) == Lookup::Unknown
            {
                interfaces.insert(key.to_ascii_lowercase());
            }
        }
    }

    fn members(&mut self, class: &str, fx: &FixtureClass) {
        let members: Vec<&Record> = self.data.members(class, self.version).collect();
        self.class_constants(class, fx, &members);
        for member in fx.cases.keys() {
            if find(&members, Kind::EnumCase, member).is_none() {
                self.differ(
                    &format!("{class}::{member}"),
                    "presence",
                    "declared",
                    "absent",
                );
            }
        }
        self.properties(class, fx, &members);
        self.methods(class, fx, &members);
    }

    fn class_constants(&mut self, class: &str, fx: &FixtureClass, members: &[&Record]) {
        for (member, c) in &fx.constants {
            let symbol = format!("{class}::{member}");
            match find(members, Kind::ClassConstant, member).map(|r| &r.detail) {
                Some(Detail::ClassConstant(d)) => {
                    if !c.value.is_empty() && d.value.as_deref() != Some(c.value.as_str()) {
                        let actual = d.value.as_deref().unwrap_or("(none)");
                        self.differ(&symbol, "value", &c.value, actual);
                    }
                    self.flag(&symbol, "final", c.is_final, d.is_final);
                    self.visibility(&symbol, &c.visibility, d.visibility);
                }
                _ => self.differ(&symbol, "presence", "declared", "absent"),
            }
        }
    }

    fn properties(&mut self, class: &str, fx: &FixtureClass, members: &[&Record]) {
        for (member, p) in &fx.properties {
            if fx.kind == "enum" && ["name", "value"].contains(&member.as_str()) {
                // The engine gives every enum these; the stubs do not declare them.
                continue;
            }
            let symbol = format!("{class}::${member}");
            match find(members, Kind::Property, member).map(|r| &r.detail) {
                Some(Detail::Property(d)) => {
                    self.type_of(&symbol, "type", &p.type_text, &d.type_text);
                    self.flag(&symbol, "static", p.is_static, d.is_static);
                    // The stubs mark with `@readonly` the properties that the engine guards with
                    // its own handlers (DOM, for one), which Reflection does not call readonly.
                    // Only a property Reflection calls readonly must be readonly in the data.
                    if p.is_readonly && !d.is_readonly {
                        self.differ(&symbol, "readonly", true, false);
                    }
                    self.visibility(&symbol, &p.visibility, d.visibility);
                }
                _ => self.differ(&symbol, "presence", "declared", "absent"),
            }
        }
    }

    fn methods(&mut self, class: &str, fx: &FixtureClass, members: &[&Record]) {
        for (member, m) in &fx.methods {
            if fx.kind == "enum"
                && ["cases", "from", "tryfrom"].contains(&member.to_ascii_lowercase().as_str())
            {
                // The engine gives every enum these; the stubs do not declare them.
                continue;
            }
            let symbol = format!("{class}::{member}");
            match find(members, Kind::Method, member).map(|r| &r.detail) {
                Some(Detail::Method(d)) => {
                    self.signature(&symbol, &m.signature, &d.signature);
                    self.flag(&symbol, "static", m.is_static, d.is_static);
                    self.flag(&symbol, "final", m.is_final, d.is_final);
                    // Every method of an interface is abstract, declared so or not.
                    let is_abstract = d.is_abstract || fx.kind == "interface";
                    self.flag(&symbol, "abstract", m.is_abstract, is_abstract);
                    self.visibility(&symbol, &m.visibility, d.visibility);
                }
                _ => self.differ(&symbol, "presence", "declared", "absent"),
            }
        }
    }

    fn visibility(&mut self, symbol: &str, expected: &str, actual: Visibility) {
        if expected != visibility_text(actual) {
            self.differ(symbol, "visibility", expected, visibility_text(actual));
        }
    }
}

/// The member record of `kind` named `member` among a class's members.
fn find<'a>(members: &[&'a Record], kind: Kind, member: &str) -> Option<&'a Record> {
    members.iter().copied().find(|r| {
        r.kind() == kind
            && r.name.split_once("::").is_some_and(|(_, m)| {
                if kind == Kind::Method {
                    m.eq_ignore_ascii_case(member)
                } else {
                    m == member
                }
            })
    })
}

fn class_detail(record: &Record) -> Option<&ClassDetail> {
    match &record.detail {
        Detail::Class(d) | Detail::Interface(d) | Detail::Trait(d) | Detail::Enum(d) => Some(d),
        _ => None,
    }
}

/// Every difference between the fixture and the data, for the fixture's version.
pub fn compare(fixture: &Fixture, data: &BuiltinData) -> Vec<Difference> {
    let mut checker = Checker {
        data,
        version: fixture.version,
        out: Vec::new(),
    };
    checker.functions(fixture);
    checker.constants(fixture);
    checker.classes(fixture);
    checker.out
}

/// The symbols the fixture cannot vouch for: those of extensions the image does not load, as a
/// count per extension. Members of classes are not counted apart from their class.
pub fn not_covered(fixture: &Fixture, records: &[Record]) -> BTreeMap<String, usize> {
    let loaded: BTreeSet<String> = fixture
        .extensions
        .iter()
        .filter_map(|e| crate::fixture::extension_name(e))
        .collect();
    let mut counts = BTreeMap::new();
    for record in records {
        if record.mask.contains(fixture.version)
            && !record.kind().is_member()
            && !loaded.contains(&record.extension)
        {
            *counts.entry(record.extension.clone()).or_default() += 1;
        }
    }
    counts
}

/// One reviewed allowance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allowance {
    /// The symbol, compared without case; a trailing `*` makes it a prefix.
    pub symbol: String,
    /// The start of the field name; any field when absent.
    pub field: Option<String>,
    /// The versions it applies to; all when absent.
    pub versions: VersionMask,
    pub reason: String,
}

impl Allowance {
    fn matches(&self, d: &Difference) -> bool {
        let symbol = d.symbol.to_ascii_lowercase();
        let wanted = self.symbol.to_ascii_lowercase();
        let symbol_ok = match wanted.strip_suffix('*') {
            Some(prefix) => symbol.starts_with(prefix),
            None => symbol == wanted,
        };
        symbol_ok
            && self.versions.contains(d.version)
            && self
                .field
                .as_ref()
                .is_none_or(|f| d.field.starts_with(f.as_str()))
    }
}

/// The reviewed file `stubs/oracle-allow.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Allowances {
    pub entries: Vec<Allowance>,
}

impl Allowances {
    /// Reads the file. Each `[[allow]]` needs `symbol` and `reason`; `field` and `versions` are
    /// optional.
    pub fn parse(text: &str) -> Result<Self, String> {
        let table: toml::Table = text
            .parse()
            .map_err(|e| format!("oracle-allow.toml: {e}"))?;
        let list = table
            .get("allow")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let mut entries = Vec::new();
        for (index, item) in list.iter().enumerate() {
            let get = |key: &str| item.get(key).and_then(|v| v.as_str()).map(str::to_string);
            let here = format!("oracle-allow.toml entry {}", index + 1);
            let symbol = get("symbol").ok_or_else(|| format!("{here}: `symbol` is missing"))?;
            let reason = get("reason")
                .filter(|r| !r.trim().is_empty())
                .ok_or_else(|| format!("{here} ({symbol}): a `reason` is required"))?;
            let versions = match get("versions") {
                Some(text) => VersionMask::parse(&text)
                    .map_err(|e| format!("{here} ({symbol}): versions: {e}"))?,
                None => VersionMask::ALL,
            };
            entries.push(Allowance {
                symbol,
                field: get("field"),
                versions,
                reason,
            });
        }
        Ok(Self { entries })
    }
}

/// The result of applying the allowances to the differences.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Differences no allowance covers.
    pub unexplained: Vec<Difference>,
    /// Allowances that matched no difference in any version.
    pub stale: Vec<Allowance>,
    /// How many differences each allowance covered, by entry index.
    pub covered: BTreeMap<usize, usize>,
}

/// Applies the allowances to the differences of all versions.
pub fn apply(differences: &[Difference], allowances: &Allowances) -> Outcome {
    let mut outcome = Outcome::default();
    for d in differences {
        match allowances.entries.iter().position(|a| a.matches(d)) {
            Some(i) => *outcome.covered.entry(i).or_default() += 1,
            None => outcome.unexplained.push(d.clone()),
        }
    }
    // An allowance is stale when no difference matches it at all, even one a earlier entry took.
    outcome.stale = allowances
        .entries
        .iter()
        .filter(|a| !differences.iter().any(|d| a.matches(d)))
        .cloned()
        .collect();
    outcome
}

/// Reads the generated data, the fixtures of every version and the allowances under `root` (the
/// repository root), and compares them: all differences, in version order.
pub fn check_repository(root: &std::path::Path) -> Result<(Vec<Difference>, Allowances), String> {
    let read =
        |path: &str| std::fs::read_to_string(root.join(path)).map_err(|e| format!("{path}: {e}"));
    let data = bleedthrough_builtins::format::read(&read("stubs/builtin.data")?)
        .map_err(|e| format!("stubs/builtin.data: {e}"))?;
    let allowances = Allowances::parse(&read("stubs/oracle-allow.toml")?)?;
    let mut differences = Vec::new();
    for version in PhpVersion::all() {
        let path = format!("stubs/reflection/php-{version}.json");
        let fixture = Fixture::parse(&read(&path)?).map_err(|e| format!("{path}: {e}"))?;
        differences.extend(compare(&fixture, &data));
    }
    Ok((differences, allowances))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bleedthrough_builtins::{FunctionDetail, Signature};

    fn v(text: &str) -> PhpVersion {
        PhpVersion::parse(text).unwrap()
    }

    #[test]
    fn equivalent_type_spellings_are_equal() {
        for (a, b) in [
            ("?int", "int|null"),
            ("null|string|int", "int|string|null"),
            ("\\Foo\\Bar", "foo\\bar"),
            ("A&B", "(B&A)"),
            ("iterable", "Traversable|array"),
            ("(A&B)|null", "?(b&a)"),
            ("", ""),
        ] {
            assert_eq!(normalise_type(a), normalise_type(b), "{a} vs {b}");
        }
        assert_ne!(normalise_type("int"), normalise_type("int|string"));
        assert_ne!(normalise_type(""), normalise_type("mixed"));
    }

    fn data_with(params: Vec<Param>) -> BuiltinData {
        let detail = Detail::Function(FunctionDetail {
            signature: Signature {
                params,
                return_type: "int".to_string(),
                ..Signature::default()
            },
            ..FunctionDetail::default()
        });
        let record = Record::new("f", VersionMask::ALL, "standard", detail);
        BuiltinData::new(Vec::new(), vec![record]).unwrap()
    }

    fn fixture_of(by_ref: bool) -> Fixture {
        Fixture::parse(&format!(
            r#"{{"php_version":"8.4.1","image":"i","extensions":[],"functions":{{"f":{{"params":[{{"name":"x","type":"array","by_ref":{by_ref},"optional":false,"variadic":false,"has_default":false}}],"return_type":"int","tentative_return_type":"","returns_ref":false,"extension":"standard"}}}},"classes":{{}},"constants":{{}}}}"#
        ))
        .unwrap()
    }

    fn param(by_ref: bool) -> Param {
        Param {
            name: "x".to_string(),
            type_text: "array".to_string(),
            by_ref,
            ..Param::default()
        }
    }

    #[test]
    fn a_missing_by_reference_flag_names_the_function_and_parameter() {
        let differences = compare(&fixture_of(true), &data_with(vec![param(false)]));
        assert_eq!(differences.len(), 1, "{differences:?}");
        let text = differences[0].to_string();
        assert!(text.contains(" f: param 1 ($x) by-reference"), "{text}");
        assert!(compare(&fixture_of(true), &data_with(vec![param(true)])).is_empty());
    }

    #[test]
    fn a_missing_symbol_is_a_presence_difference() {
        let mut fixture = fixture_of(false);
        fixture
            .functions
            .insert("g".to_string(), fixture.functions["f"].clone());
        let differences = compare(&fixture, &data_with(vec![param(false)]));
        assert_eq!(differences.len(), 1);
        assert_eq!(
            (
                differences[0].symbol.as_str(),
                differences[0].field.as_str()
            ),
            ("g", "presence")
        );
    }

    #[test]
    fn allowances_cover_differences_and_a_stale_one_fails() {
        let differences = compare(&fixture_of(true), &data_with(vec![param(false)]));
        let covering = Allowances::parse(
            "[[allow]]\nsymbol = \"F\"\nfield = \"param 1\"\nversions = \"8.4\"\nreason = \"test\"\n",
        )
        .unwrap();
        let outcome = apply(&differences, &covering);
        assert!(
            outcome.unexplained.is_empty() && outcome.stale.is_empty(),
            "{outcome:?}"
        );

        let stale = Allowances::parse(
            "[[allow]]\nsymbol = \"F\"\nfield = \"param 1\"\nreason = \"test\"\n\n[[allow]]\nsymbol = \"nothing_like_this*\"\nreason = \"gone\"\n",
        )
        .unwrap();
        let outcome = apply(&differences, &stale);
        assert_eq!(outcome.stale.len(), 1);
        assert_eq!(outcome.stale[0].symbol, "nothing_like_this*");

        let wrong_version = Allowances::parse(
            "[[allow]]\nsymbol = \"f\"\nversions = \"8.0-8.3\"\nreason = \"test\"\n",
        )
        .unwrap();
        let outcome = apply(&differences, &wrong_version);
        assert_eq!(outcome.unexplained.len(), 1);
        assert_eq!(outcome.stale.len(), 1);
    }

    #[test]
    fn symbols_of_unloaded_extensions_are_not_covered() {
        let fixture = fixture_of(false);
        let record = |name: &str, extension: &str| {
            Record::new(
                name,
                VersionMask::ALL,
                extension,
                Detail::Function(FunctionDetail::default()),
            )
        };
        let records = [
            record("a", "standard"),
            record("b", "ldap"),
            record("c", "ldap"),
        ];
        let counts = not_covered(&fixture, &records);
        assert_eq!(counts.get("ldap"), Some(&2));
        assert_eq!(counts.len(), 2); // the fixture lists no extensions, so `standard` counts too
    }

    #[test]
    fn an_allowance_needs_a_reason() {
        assert!(Allowances::parse("[[allow]]\nsymbol = \"f\"\n").is_err());
        assert!(Allowances::parse("[[allow]]\nsymbol = \"f\"\nreason = \" \"\n").is_err());
        assert!(Allowances::parse("[[allow]]\nreason = \"x\"\n").is_err());
    }

    #[test]
    fn prefix_allowances_cover_a_group() {
        let a = Allowances::parse("[[allow]]\nsymbol = \"Foo::*\"\nreason = \"x\"\n").unwrap();
        let d = |symbol: &str| Difference {
            version: v("8.4"),
            symbol: symbol.to_string(),
            field: "presence".to_string(),
            expected: String::new(),
            actual: String::new(),
        };
        let outcome = apply(&[d("foo::bar"), d("Baz::bar")], &a);
        assert_eq!(outcome.unexplained.len(), 1);
    }
}
