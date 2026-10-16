//! Reflection fixtures (`stubs/reflection/php-8.N.json`): reading them, and filling with them what
//! the stubs do not say.
//!
//! The fixtures are what the interpreters of the pinned images report about their built-in
//! symbols. They complete the data in two ways: constant values (the stubs never have them), and
//! the constants and properties of PHP 8.0 and 8.1 (whose stubs declare almost none). Everything
//! taken from a fixture is marked in the record's `reflected` mask.

use std::collections::{BTreeMap, BTreeSet};

use bleedthrough_builtins::{
    ConstantDetail, Detail, PhpVersion, PropertyDetail, Record, VersionMask, Visibility,
};
use serde_json::Value;

use crate::extract::Extracted;

/// One parameter as Reflection reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureParam {
    pub name: String,
    pub type_text: String,
    pub by_ref: bool,
    pub optional: bool,
    pub variadic: bool,
    pub has_default: bool,
}

/// The parameters and return types of a function or method.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FixtureSignature {
    pub params: Vec<FixtureParam>,
    pub return_type: String,
    pub tentative_return_type: String,
    pub returns_ref: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureFunction {
    pub signature: FixtureSignature,
    pub extension: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureMethod {
    pub signature: FixtureSignature,
    pub visibility: String,
    pub is_static: bool,
    pub is_final: bool,
    pub is_abstract: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureProperty {
    pub type_text: String,
    pub visibility: String,
    pub is_static: bool,
    pub is_readonly: bool,
    pub has_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureClassConstant {
    pub value: String,
    pub type_text: String,
    pub visibility: String,
    pub is_final: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureClass {
    /// `class`, `interface`, `trait` or `enum`.
    pub kind: String,
    pub extension: String,
    pub is_abstract: bool,
    pub is_final: bool,
    pub is_readonly: bool,
    pub parent: String,
    pub interfaces: Vec<String>,
    pub backing_type: String,
    pub constants: BTreeMap<String, FixtureClassConstant>,
    /// Case name to backing value text (empty for a pure enum).
    pub cases: BTreeMap<String, String>,
    pub properties: BTreeMap<String, FixtureProperty>,
    pub methods: BTreeMap<String, FixtureMethod>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureConstant {
    /// The Reflection extension (group) name.
    pub extension: String,
    /// `int`, `string`, `float`, `bool`, `array`, `null` or another debug type name.
    pub type_text: String,
    /// The value as PHP literal text; empty when it has no literal form.
    pub value: String,
}

/// One parsed fixture file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fixture {
    pub version: PhpVersion,
    /// The interpreter's full version (`8.4.26`).
    pub php_version: String,
    pub image: String,
    pub extensions: Vec<String>,
    pub functions: BTreeMap<String, FixtureFunction>,
    pub classes: BTreeMap<String, FixtureClass>,
    pub constants: BTreeMap<String, FixtureConstant>,
}

fn text(value: &Value, key: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("`{key}` is missing or not text"))
}

fn flag(value: &Value, key: &str) -> Result<bool, String> {
    value
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("`{key}` is missing or not a boolean"))
}

/// An object member. PHP writes an empty map as `[]`, which counts as an empty object.
fn map(value: &Value, key: &str) -> Result<serde_json::Map<String, Value>, String> {
    match value.get(key) {
        Some(Value::Object(object)) => Ok(object.clone()),
        Some(Value::Array(items)) if items.is_empty() => Ok(serde_json::Map::new()),
        _ => Err(format!("`{key}` is missing or not an object")),
    }
}

fn signature(value: &Value) -> Result<FixtureSignature, String> {
    let params = value
        .get("params")
        .and_then(Value::as_array)
        .ok_or("`params` is missing")?
        .iter()
        .map(|p| {
            Ok(FixtureParam {
                name: text(p, "name")?,
                type_text: text(p, "type")?,
                by_ref: flag(p, "by_ref")?,
                optional: flag(p, "optional")?,
                variadic: flag(p, "variadic")?,
                has_default: flag(p, "has_default")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(FixtureSignature {
        params,
        return_type: text(value, "return_type")?,
        tentative_return_type: text(value, "tentative_return_type")?,
        returns_ref: flag(value, "returns_ref")?,
    })
}

fn class(value: &Value) -> Result<FixtureClass, String> {
    let mut out = FixtureClass {
        kind: text(value, "kind")?,
        extension: text(value, "extension")?,
        is_abstract: flag(value, "abstract")?,
        is_final: flag(value, "final")?,
        is_readonly: flag(value, "readonly")?,
        parent: text(value, "parent")?,
        interfaces: value
            .get("interfaces")
            .and_then(Value::as_array)
            .ok_or("`interfaces` is missing")?
            .iter()
            .map(|i| {
                i.as_str()
                    .map(str::to_string)
                    .ok_or("an interface is not text")
            })
            .collect::<Result<_, _>>()?,
        backing_type: text(value, "backing_type")?,
        constants: BTreeMap::new(),
        cases: BTreeMap::new(),
        properties: BTreeMap::new(),
        methods: BTreeMap::new(),
    };
    for (name, c) in &map(value, "constants")? {
        out.constants.insert(
            name.clone(),
            FixtureClassConstant {
                value: text(c, "value")?,
                type_text: text(c, "type")?,
                visibility: text(c, "visibility")?,
                is_final: flag(c, "final")?,
            },
        );
    }
    for (name, v) in &map(value, "cases")? {
        let v = v.as_str().ok_or("a case value is not text")?;
        out.cases.insert(name.clone(), v.to_string());
    }
    for (name, p) in &map(value, "properties")? {
        out.properties.insert(
            name.clone(),
            FixtureProperty {
                type_text: text(p, "type")?,
                visibility: text(p, "visibility")?,
                is_static: flag(p, "static")?,
                is_readonly: flag(p, "readonly")?,
                has_default: flag(p, "has_default")?,
            },
        );
    }
    for (name, m) in &map(value, "methods")? {
        out.methods.insert(
            name.clone(),
            FixtureMethod {
                signature: signature(m)?,
                visibility: text(m, "visibility")?,
                is_static: flag(m, "static")?,
                is_final: flag(m, "final")?,
                is_abstract: flag(m, "abstract")?,
            },
        );
    }
    Ok(out)
}

impl Fixture {
    /// Parses the text of a fixture file.
    pub fn parse(source: &str) -> Result<Self, String> {
        let root: Value = serde_json::from_str(source).map_err(|e| format!("not JSON: {e}"))?;
        let php_version = text(&root, "php_version")?;
        let minor = php_version.split('.').take(2).collect::<Vec<_>>().join(".");
        let version = PhpVersion::parse(&minor)
            .ok_or_else(|| format!("`{php_version}` is not a PHP 8.0 to 8.5 version"))?;
        let mut fixture = Fixture {
            version,
            php_version,
            image: text(&root, "image")?,
            extensions: root
                .get("extensions")
                .and_then(Value::as_array)
                .ok_or("`extensions` is missing")?
                .iter()
                .map(|e| {
                    e.as_str()
                        .map(str::to_string)
                        .ok_or("an extension is not text")
                })
                .collect::<Result<_, _>>()?,
            functions: BTreeMap::new(),
            classes: BTreeMap::new(),
            constants: BTreeMap::new(),
        };
        for (name, f) in &map(&root, "functions")? {
            fixture.functions.insert(
                name.clone(),
                FixtureFunction {
                    signature: signature(f).map_err(|e| format!("function {name}: {e}"))?,
                    extension: text(f, "extension")?,
                },
            );
        }
        for (name, c) in &map(&root, "classes")? {
            fixture.classes.insert(
                name.clone(),
                class(c).map_err(|e| format!("class {name}: {e}"))?,
            );
        }
        for (name, c) in &map(&root, "constants")? {
            fixture.constants.insert(
                name.clone(),
                FixtureConstant {
                    extension: text(c, "extension")?,
                    type_text: text(c, "type")?,
                    value: text(c, "value")?,
                },
            );
        }
        Ok(fixture)
    }
}

/// The data's extension name for a Reflection extension name: lower case, without the `Zend `
/// prefix (`Zend OPcache` is `opcache`). `None` when the result is not a plain word.
pub fn extension_name(reflection: &str) -> Option<String> {
    let name = reflection
        .strip_prefix("Zend ")
        .unwrap_or(reflection)
        .to_ascii_lowercase();
    (!name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-'))
    .then_some(name)
}

/// Constants whose value depends on the build, the host or the patch release of the image that was
/// dumped, so a value taken from the fixture would be wrong for other PHP builds. They keep no
/// value in the data. Reviewed list; each group says why.
const BUILD_SPECIFIC: &[(&str, &[&str])] = &[
    (
        "patch release of the image",
        &[
            "PHP_VERSION",
            "PHP_RELEASE_VERSION",
            "PHP_EXTRA_VERSION",
            "PHP_VERSION_ID",
        ],
    ),
    (
        "install paths and host of the image",
        &[
            "PHP_BINARY",
            "PHP_BINDIR",
            "PHP_CONFIG_FILE_PATH",
            "PHP_CONFIG_FILE_SCAN_DIR",
            "PHP_DATADIR",
            "PHP_EXTENSION_DIR",
            "PHP_LIBDIR",
            "PHP_LOCALSTATEDIR",
            "PHP_MANDIR",
            "PHP_PREFIX",
            "PHP_SYSCONFDIR",
            "PEAR_EXTENSION_DIR",
            "PEAR_INSTALL_DIR",
            "DEFAULT_INCLUDE_PATH",
            "PHP_OS",
            "PHP_OS_FAMILY",
            "PHP_SAPI",
            "PHP_SHLIB_SUFFIX",
            "PHP_MAXPATHLEN",
            "PHP_FD_SETSIZE",
        ],
    ),
    (
        "which of ext/standard, ext/openssl and ext/sodium provides Argon2 is a build choice",
        &["PASSWORD_ARGON2_PROVIDER"],
    ),
    (
        "build options of the image",
        &[
            "PHP_DEBUG",
            "PHP_ZTS",
            "ZEND_DEBUG_BUILD",
            "ZEND_THREAD_SAFE",
            "ZEND_MULTIBYTE",
        ],
    ),
    (
        "versions of the libraries the image was built with",
        &[
            "CURLVERSION_NOW",
            "ICONV_IMPL",
            "ICONV_VERSION",
            "LIBXML_DOTTED_VERSION",
            "LIBXML_LOADED_VERSION",
            "LIBXML_VERSION",
            "MB_ONIGURUMA_VERSION",
            "OPENSSL_VERSION_NUMBER",
            "OPENSSL_VERSION_TEXT",
            "PCRE_JIT_SUPPORT",
            "PCRE_VERSION",
            "PCRE_VERSION_MAJOR",
            "PCRE_VERSION_MINOR",
            "SODIUM_LIBRARY_MAJOR_VERSION",
            "SODIUM_LIBRARY_MINOR_VERSION",
            "SODIUM_LIBRARY_VERSION",
            "XML_SAX_IMPL",
            "ZLIB_VERNUM",
            "ZLIB_VERSION",
        ],
    ),
];

/// Whether the value of this global constant must not be taken from a fixture, and why.
pub fn build_specific(name: &str) -> Option<&'static str> {
    BUILD_SPECIFIC
        .iter()
        .find(|(_, names)| names.contains(&name))
        .map(|(reason, _)| *reason)
}

/// What a fill did, for the coverage report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FillReport {
    /// Constant values (global or class) written from the fixture.
    pub values: usize,
    /// Global constants added because the stubs have none for the version.
    pub constants_added: usize,
    /// Class constants added likewise.
    pub class_constants_added: usize,
    /// Properties added likewise.
    pub properties_added: usize,
    /// Functions whose extension was taken from the fixture instead of the SAPI name.
    pub extensions: usize,
    /// Global constants left without a value because it depends on the build.
    pub build_specific: usize,
}

/// The versions whose stubs lack constants and most properties.
fn stubs_lack_members(version: PhpVersion) -> bool {
    version.minor() <= 1
}

fn visibility(text: &str) -> Visibility {
    match text {
        "private" => Visibility::Private,
        "protected" => Visibility::Protected,
        _ => Visibility::Public,
    }
}

/// A fixture value or `None` for an empty one.
fn value_of(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_string())
}

/// The model's type text for a debug type name.
fn constant_type(debug_type: &str) -> String {
    match debug_type {
        "int" | "string" | "float" | "bool" | "array" => debug_type.to_string(),
        _ => String::new(),
    }
}

/// What the stubs already declare, for deciding what is missing.
#[derive(Default)]
struct Declared {
    constants: BTreeSet<String>,
    /// `('c' | 'p', lower-cased Class::member)`.
    members: BTreeSet<(char, String)>,
    classes: BTreeSet<String>,
}

/// Fills one version's extraction from its fixture.
pub fn fill(fixture: &Fixture, items: &mut Vec<Extracted>) -> FillReport {
    let mut report = FillReport::default();
    let declared = fill_known(fixture, items, &mut report);
    if stubs_lack_members(fixture.version) {
        let added = add_missing(fixture, &declared, &mut report);
        items.extend(added);
    }
    report
}

/// Gives the records the stubs declare their values (and SAPI functions their extension) from the
/// fixture, and notes what is declared.
fn fill_known(fixture: &Fixture, items: &mut [Extracted], report: &mut FillReport) -> Declared {
    let mark = VersionMask::of(fixture.version);
    let classes: BTreeMap<String, &FixtureClass> = fixture
        .classes
        .iter()
        .map(|(name, c)| (name.to_ascii_lowercase(), c))
        .collect();
    let mut declared = Declared::default();
    for item in items.iter_mut() {
        let name = item.record.name.clone();
        match &mut item.record.detail {
            Detail::Constant(detail) => {
                declared.constants.insert(name.clone());
                if let Some(fx) = fixture.constants.get(&name) {
                    if let Some(extension) = extension_name(&fx.extension) {
                        item.record.extension = extension;
                    }
                    if build_specific(&name).is_some() {
                        detail.value = None;
                        report.build_specific += 1;
                    } else if let Some(value) = value_of(&fx.value) {
                        detail.value = Some(value);
                        item.record.reflected = item.record.reflected.union(mark);
                        report.values += 1;
                    }
                }
            }
            Detail::ClassConstant(detail) => {
                declared.members.insert(('c', name.to_ascii_lowercase()));
                if let Some((owner, member)) = name.split_once("::")
                    && let Some(fx) = classes
                        .get(&owner.to_ascii_lowercase())
                        .and_then(|c| c.constants.get(member))
                    && let Some(value) = value_of(&fx.value)
                {
                    detail.value = Some(value);
                    item.record.reflected = item.record.reflected.union(mark);
                    report.values += 1;
                }
            }
            Detail::Property(_) => {
                declared.members.insert(('p', name.to_ascii_lowercase()));
            }
            Detail::Function(_) => {
                if item.record.sapi.contains(&item.record.extension)
                    && let Some(fx) = fixture.functions.get(&name.to_ascii_lowercase())
                    && let Some(extension) = extension_name(&fx.extension)
                {
                    item.record.extension = extension;
                    report.extensions += 1;
                }
            }
            Detail::Class(_) | Detail::Interface(_) | Detail::Trait(_) | Detail::Enum(_) => {
                declared.classes.insert(name.to_ascii_lowercase());
            }
            _ => {}
        }
    }
    declared
}

/// The global constants the fixture has and the stubs lack.
fn missing_constants(
    fixture: &Fixture,
    declared: &Declared,
    report: &mut FillReport,
) -> Vec<Record> {
    let mark = VersionMask::of(fixture.version);
    let mut records = Vec::new();
    for (name, fx) in &fixture.constants {
        let Some(extension) = extension_name(&fx.extension) else {
            continue;
        };
        if declared.constants.contains(name) {
            continue;
        }
        let value = if build_specific(name).is_some() {
            report.build_specific += 1;
            None
        } else {
            value_of(&fx.value)
        };
        report.constants_added += 1;
        records.push(Record::new(
            name.clone(),
            mark,
            extension,
            Detail::Constant(ConstantDetail {
                type_text: constant_type(&fx.type_text),
                value,
                visibility: Visibility::Public,
                is_final: false,
            }),
        ));
    }
    records
}

/// The records the fixture has and the stubs lack: global constants, and the constants and
/// properties of classes the stubs do declare.
fn add_missing(fixture: &Fixture, declared: &Declared, report: &mut FillReport) -> Vec<Extracted> {
    let mark = VersionMask::of(fixture.version);
    let file = format!("stubs/reflection/php-{}.json", fixture.version);
    let mut added = Vec::new();
    let mut push = |mut record: Record| {
        record.reflected = mark;
        added.push(Extracted {
            record,
            file: file.clone(),
            guard: None,
        });
    };
    for record in missing_constants(fixture, declared, report) {
        push(record);
    }
    for (class_name, class) in &fixture.classes {
        let Some(extension) = extension_name(&class.extension) else {
            continue;
        };
        if !declared.classes.contains(&class_name.to_ascii_lowercase()) {
            continue;
        }
        for (member, fx) in &class.constants {
            let name = format!("{class_name}::{member}");
            if declared.members.contains(&('c', name.to_ascii_lowercase())) {
                continue;
            }
            report.class_constants_added += 1;
            push(Record::new(
                name,
                mark,
                extension.clone(),
                Detail::ClassConstant(ConstantDetail {
                    type_text: fx.type_text.clone(),
                    value: value_of(&fx.value),
                    visibility: visibility(&fx.visibility),
                    is_final: fx.is_final,
                }),
            ));
        }
        for (member, fx) in &class.properties {
            let name = format!("{class_name}::{member}");
            if declared.members.contains(&('p', name.to_ascii_lowercase())) {
                continue;
            }
            report.properties_added += 1;
            push(Record::new(
                name,
                mark,
                extension.clone(),
                Detail::Property(PropertyDetail {
                    type_text: fx.type_text.clone(),
                    visibility: visibility(&fx.visibility),
                    is_static: fx.is_static,
                    is_readonly: fx.is_readonly,
                    ..PropertyDetail::default()
                }),
            ));
        }
    }
    added
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(version: &str, constants: &str, classes: &str) -> Fixture {
        Fixture::parse(&format!(
            r#"{{"php_version":"{version}.1","image":"php@sha256:aa","extensions":["Core"],"functions":{{}},"classes":{{{classes}}},"constants":{{{constants}}}}}"#
        ))
        .unwrap()
    }

    fn v(text: &str) -> PhpVersion {
        PhpVersion::parse(text).unwrap()
    }

    const CLASS: &str = r#""Foo":{"kind":"class","extension":"Core","abstract":false,"final":false,"readonly":false,"parent":"","interfaces":[],"backing_type":"","constants":{"BAR":{"value":"2","type":"","visibility":"public","final":false}},"cases":{},"properties":{"p":{"type":"int","visibility":"public","static":false,"readonly":false,"has_default":false}},"methods":{}}"#;

    fn class_item(name: &str, version: &str) -> Extracted {
        Extracted {
            record: Record::new(
                name,
                VersionMask::of(v(version)),
                "core",
                Detail::Class(bleedthrough_builtins::ClassDetail::default()),
            ),
            file: "x.stub.php".into(),
            guard: None,
        }
    }

    #[test]
    fn an_8_0_constant_absent_from_the_stubs_is_added_and_marked() {
        let fx = fixture(
            "8.0",
            r#""E_ALL":{"extension":"Core","type":"int","value":"32767"}"#,
            "",
        );
        let mut items = Vec::new();
        let report = fill(&fx, &mut items);
        assert_eq!(report.constants_added, 1);
        let record = &items[0].record;
        assert_eq!(record.name, "E_ALL");
        assert_eq!(record.reflected, VersionMask::of(v("8.0")));
        let Detail::Constant(d) = &record.detail else {
            panic!("not a constant");
        };
        assert_eq!(d.value.as_deref(), Some("32767"));
        assert_eq!(d.type_text, "int");
    }

    #[test]
    fn members_of_known_classes_are_added_in_8_0_and_8_1_only() {
        let fx = fixture("8.1", "", CLASS);
        let mut items = vec![class_item("Foo", "8.1")];
        let report = fill(&fx, &mut items);
        assert_eq!(
            (report.class_constants_added, report.properties_added),
            (1, 1)
        );
        assert!(items.iter().any(|i| i.record.name == "Foo::BAR"));
        let fx = fixture("8.2", "", CLASS);
        let mut items = vec![class_item("Foo", "8.2")];
        assert_eq!(fill(&fx, &mut items), FillReport::default());
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn a_class_the_stubs_lack_gets_no_members() {
        let fx = fixture("8.0", "", CLASS);
        let mut items = Vec::new();
        assert_eq!(fill(&fx, &mut items).class_constants_added, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn values_are_filled_for_every_version_and_build_specific_ones_stay_empty() {
        let fx = fixture(
            "8.4",
            r#""M_PI":{"extension":"standard","type":"float","value":"3.14"},"PHP_OS":{"extension":"Core","type":"string","value":"'Linux'"}"#,
            "",
        );
        let constant = |name: &str| Extracted {
            record: Record::new(
                name,
                VersionMask::of(v("8.4")),
                "standard",
                Detail::Constant(ConstantDetail::default()),
            ),
            file: "x.stub.php".into(),
            guard: None,
        };
        let mut items = vec![constant("M_PI"), constant("PHP_OS")];
        let report = fill(&fx, &mut items);
        assert_eq!((report.values, report.build_specific), (1, 1));
        let value = |i: &Extracted| match &i.record.detail {
            Detail::Constant(d) => d.value.clone(),
            _ => None,
        };
        assert_eq!(value(&items[0]).as_deref(), Some("3.14"));
        assert_eq!(items[0].record.reflected, VersionMask::of(v("8.4")));
        assert_eq!(value(&items[1]), None);
        assert!(items[1].record.reflected.is_empty());
    }

    #[test]
    fn extension_names_follow_the_data_spelling() {
        assert_eq!(extension_name("Zend OPcache").as_deref(), Some("opcache"));
        assert_eq!(extension_name("SPL").as_deref(), Some("spl"));
        assert_eq!(extension_name("").as_deref(), None);
    }

    #[test]
    fn a_fixture_of_the_wrong_shape_is_rejected() {
        assert!(Fixture::parse("{}").is_err());
        assert!(Fixture::parse(r#"{"php_version":"7.4.1"}"#).is_err());
    }
}
