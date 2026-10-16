//! The built-in data generator's extraction, run with the Mago backend on stub excerpts.

#![allow(
    missing_docs,
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    reason = "test fixtures are long and need no documentation"
)]

use std::path::Path;

use bleedthrough_builtins_gen::PhpVersion;
use bleedthrough_builtins_gen::count::compare;
use bleedthrough_builtins_gen::extract::Extracted;
use bleedthrough_builtins_gen::model;
use bleedthrough_builtins_gen::release::extract_source;
use bleedthrough_syntax::{ParseOptions, ParserBackend};
use bleedthrough_syntax_mago::MagoBackend;

fn v(text: &str) -> PhpVersion {
    PhpVersion::parse(text).unwrap()
}

/// Items and error messages of one stub source placed at `path` in a release tree.
fn run(path: &str, source: &str) -> (Vec<Extracted>, Vec<String>) {
    let (items, _, errors) = extract_source(&MagoBackend::new(), path, source.as_bytes(), v("8.4"));
    (items, errors.iter().map(ToString::to_string).collect())
}

fn find<'a>(items: &'a [Extracted], name: &str) -> &'a Extracted {
    items
        .iter()
        .find(|i| i.record.name == name)
        .unwrap_or_else(|| {
            panic!(
                "no record `{name}` in {:?}",
                items.iter().map(|i| &i.record.name).collect::<Vec<_>>()
            )
        })
}

fn clean(path: &str, source: &str) -> Vec<Extracted> {
    let (items, errors) = run(path, source);
    assert!(errors.is_empty(), "{errors:?}");
    items
}

#[test]
fn guards_nest_and_branch_and_apply_to_members() {
    let source = r#"<?php
function plain(): void {}
#ifdef HAVE_A
function in_a(): void {}
#if FOO >= 2
function in_a_and_foo(): void {}
#elif BAR
function in_a_and_bar(): void {}
#else
function in_a_and_neither(): void {}
#endif
class C {
    public function member(): void {}
}
#endif
function after(): void {}
"#;
    let items = clean("ext/standard/a.stub.php", source);
    let guard = |name: &str| find(&items, name).guard.clone();
    assert_eq!(guard("plain"), None);
    assert_eq!(guard("in_a").as_deref(), Some("defined(HAVE_A)"));
    assert_eq!(
        guard("in_a_and_foo").as_deref(),
        Some("defined(HAVE_A) && FOO >= 2")
    );
    assert_eq!(
        guard("in_a_and_bar").as_deref(),
        Some("defined(HAVE_A) && !(FOO >= 2) && BAR")
    );
    assert_eq!(
        guard("in_a_and_neither").as_deref(),
        Some("defined(HAVE_A) && !(FOO >= 2) && !BAR")
    );
    assert_eq!(guard("C::member").as_deref(), Some("defined(HAVE_A)"));
    assert_eq!(guard("after"), None);
}

#[test]
fn a_guard_inside_a_declaration_is_an_error() {
    let source = "<?php\nfunction f(\n#ifdef X\n int $a\n#endif\n): void {}\n";
    let (_, errors) = run("ext/standard/a.stub.php", source);
    assert!(
        errors.iter().any(|e| e.contains("inside a declaration")),
        "{errors:?}"
    );
}

#[test]
fn functions_carry_signatures_annotations_and_attributes() {
    let source = r#"<?php
/**
 * Explains in prose what this does. SECRET-PROSE
 * @param array<int, string> $items the items
 * @param resource $stream
 * @return string|false
 * @alias other_name
 * @refcount 1
 */
#[\Deprecated(since: '8.3', message: "use g() instead")]
#[\NoDiscard]
function f(array $items, &$stream = null, int|string ...$rest): string|false {}

function g(#[\SensitiveParameter] string $secret, int $n = UNKNOWN, array $o = [1,
    2]): void {}

/** @tentative-return-type */
function h(): ?int {}
"#;
    let items = clean("ext/standard/a.stub.php", source);
    let f = find(&items, "f");
    let model::Detail::Function(detail) = &f.record.detail else {
        panic!()
    };
    let sig = &detail.signature;
    assert_eq!(sig.return_type, "string|false");
    assert_eq!(sig.doc_return_type, "string|false");
    assert_eq!(sig.params[0].doc_type, "array<int, string>");
    assert_eq!(sig.params[0].type_text, "array");
    assert!(sig.params[1].by_ref && sig.params[1].optional);
    assert_eq!(sig.params[1].doc_type, "resource");
    assert_eq!(sig.params[1].default.as_deref(), Some("null"));
    assert!(sig.params[2].variadic && sig.params[2].optional);
    assert_eq!(sig.params[2].type_text, "int|string");
    assert_eq!(detail.alias_of.as_deref(), Some("other_name"));
    assert!(detail.no_discard);
    let dep = f.record.deprecation.as_ref().unwrap();
    assert_eq!(
        (dep.since.as_str(), dep.message.as_str()),
        ("8.3", "use g() instead")
    );
    assert_eq!(f.record.extension, "standard");

    let model::Detail::Function(g) = &find(&items, "g").record.detail else {
        panic!()
    };
    assert!(g.signature.params[0].sensitive);
    assert!(g.signature.params[1].optional && g.signature.params[1].default.is_none());
    assert_eq!(g.signature.params[2].default.as_deref(), Some("[1, 2]"));

    let model::Detail::Function(h) = &find(&items, "h").record.detail else {
        panic!()
    };
    assert_eq!(h.signature.return_type, "");
    assert_eq!(h.signature.tentative_return_type, "?int");

    let dump = format!("{items:?}");
    assert!(
        !dump.contains("SECRET-PROSE") && !dump.contains("Explains") && !dump.contains("the items")
    );
}

#[test]
fn classes_members_constants_and_cases_are_extracted() {
    let source = r#"<?php
namespace Random\Engine;

use Random\Engine;
use Countable as Cnt;

/** @strict-properties */
final class Mt19937 extends Engine implements Cnt, \Stringable
{
    /**
     * @var int
     * @cvalue MT_N
     */
    public const MT_N = UNKNOWN;
    /** @var string */
    final protected const string NAME = "mt";
    public readonly int $seed;
    /** @readonly */
    public ?Engine $parent = null;
    /** @virtual */
    public int $size;
    public static function make(Engine $e, self $s): static {}
    abstract protected function hook(): void;
}

enum Suit: string {
    case Hearts = 'H';
    #[\Deprecated(since: '8.5')]
    case Spades = 'S';
}
"#;
    let items = clean("ext/random/random.stub.php", source);
    let class = find(&items, "Random\\Engine\\Mt19937");
    let model::Detail::Class(c) = &class.record.detail else {
        panic!()
    };
    assert!(c.is_final);
    assert_eq!(c.extends, ["Random\\Engine"]);
    assert_eq!(c.implements, ["Countable", "Stringable"]);

    let n = find(&items, "Random\\Engine\\Mt19937::MT_N");
    let model::Detail::ClassConstant(n) = &n.record.detail else {
        panic!()
    };
    assert_eq!((n.type_text.as_str(), n.value.as_deref()), ("int", None));
    let name = find(&items, "Random\\Engine\\Mt19937::NAME");
    let model::Detail::ClassConstant(name) = &name.record.detail else {
        panic!()
    };
    assert_eq!(name.type_text, "string");
    assert_eq!(name.value.as_deref(), Some("\"mt\""));
    assert!(name.is_final && name.visibility == model::Visibility::Protected);

    let seed = find(&items, "Random\\Engine\\Mt19937::seed");
    let model::Detail::Property(seed) = &seed.record.detail else {
        panic!()
    };
    assert!(seed.is_readonly && !seed.is_virtual);
    let parent = find(&items, "Random\\Engine\\Mt19937::parent");
    let model::Detail::Property(parent) = &parent.record.detail else {
        panic!()
    };
    assert!(parent.is_readonly);
    assert_eq!(
        (parent.type_text.as_str(), parent.default.as_deref()),
        ("?Random\\Engine", Some("null"))
    );
    let size = find(&items, "Random\\Engine\\Mt19937::size");
    let model::Detail::Property(size) = &size.record.detail else {
        panic!()
    };
    assert!(size.is_virtual);

    let make = find(&items, "Random\\Engine\\Mt19937::make");
    let model::Detail::Method(m) = &make.record.detail else {
        panic!()
    };
    assert!(m.is_static);
    assert_eq!(m.signature.params[0].type_text, "Random\\Engine");
    assert_eq!(m.signature.params[1].type_text, "self");
    assert_eq!(m.signature.return_type, "static");
    let hook = find(&items, "Random\\Engine\\Mt19937::hook");
    let model::Detail::Method(hook) = &hook.record.detail else {
        panic!()
    };
    assert!(hook.is_abstract && hook.visibility == model::Visibility::Protected);

    let suit = find(&items, "Random\\Engine\\Suit");
    let model::Detail::Enum(e) = &suit.record.detail else {
        panic!()
    };
    assert_eq!(e.backing_type, "string");
    let hearts = find(&items, "Random\\Engine\\Suit::Hearts");
    let model::Detail::EnumCase(case) = &hearts.record.detail else {
        panic!()
    };
    assert_eq!(case.value.as_deref(), Some("'H'"));
    assert_eq!(
        find(&items, "Random\\Engine\\Suit::Spades")
            .record
            .deprecation
            .as_ref()
            .unwrap()
            .since,
        "8.5"
    );
}

#[test]
fn global_constants_keep_their_attributes_and_skip_c_values() {
    let source = r#"<?php
/**
 * @var int
 * @cvalue PHP_EXTR_SKIP
 */
const EXTR_SKIP = UNKNOWN;
/** @var string */
#[\Deprecated(since: '8.3', message: 'use X')]
const OLD = "old";
const PLAIN = 1 + 2;
"#;
    let items = clean("ext/standard/a.stub.php", source);
    let model::Detail::Constant(skip) = &find(&items, "EXTR_SKIP").record.detail else {
        panic!()
    };
    assert_eq!(
        (skip.type_text.as_str(), skip.value.as_deref()),
        ("int", None)
    );
    let old = find(&items, "OLD");
    assert_eq!(old.record.deprecation.as_ref().unwrap().message, "use X");
    let model::Detail::Constant(plain) = &find(&items, "PLAIN").record.detail else {
        panic!()
    };
    assert_eq!(plain.value.as_deref(), Some("1 + 2"));
}

#[test]
fn an_unknown_annotation_fails_naming_the_tag_and_file() {
    let (_, errors) = run(
        "ext/standard/a.stub.php",
        "<?php\n/** @brand-new-tag x */\nfunction f(): void {}\n",
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("a.stub.php") && e.contains("@brand-new-tag")),
        "{errors:?}"
    );
    let (_, errors) = run(
        "ext/standard/a.stub.php",
        "<?php\n#[\\SomethingNew]\nfunction f(): void {}\n",
    );
    assert!(
        errors.iter().any(|e| e.contains("unknown attribute")),
        "{errors:?}"
    );
}

#[test]
fn skipped_and_file_level_annotations_are_accepted() {
    let source = "<?php\n/** @generate-class-entries */\n\n/** @refcount 1 @compile-time-eval */\nfunction f(): void {}\n/** @see https://example.org\n * @since 8.1 */\nfunction g(): void {}\n";
    clean("ext/standard/a.stub.php", source);
}

#[test]
fn facts_in_a_docblock_that_precedes_nothing_are_not_lost_silently() {
    let (_, errors) = run(
        "ext/standard/a.stub.php",
        "<?php\n/** @return int */\n\n$x = 1;\n",
    );
    assert!(
        errors.iter().any(|e| e.contains("precedes no declaration")),
        "{errors:?}"
    );
}

#[test]
fn whitespace_comments_and_attribute_order_do_not_change_the_records() {
    let a = "<?php\n/** @param int $a */\n#[\\Deprecated(since: '8.1', message: 'm')]\n#[\\NoDiscard]\nfunction f(int $a,\n  ?string  $b  =  null ): int|false {}\n";
    let b = "<?php\n/**\n * @param int $a\n */\n#[\\NoDiscard, \\Deprecated(message: 'm', since: '8.1')]\nfunction   f( int $a, ?string $b = null ) : int | false {}\n";
    let ra = clean("ext/standard/a.stub.php", a);
    let rb = clean("ext/standard/a.stub.php", b);
    assert_eq!(ra[0].record, rb[0].record);
}

#[test]
fn removing_an_extraction_case_is_detected_by_the_counter() {
    let source = "<?php\nfunction a(): void {}\nclass K { public int $p; const C = 1; public function m(): void {} }\n";
    let backend = MagoBackend::new();
    let parsed = backend.parse("f.stub.php", source.as_bytes(), &ParseOptions::default());
    let (items, coverage, errors) = extract_source(
        &backend,
        "ext/standard/a.stub.php",
        source.as_bytes(),
        v("8.4"),
    );
    assert!(errors.is_empty() && coverage.is_complete());
    for skip in 0..items.len() {
        let mut fewer = items.clone();
        let dropped = fewer.remove(skip);
        let coverage = compare(parsed.tree(), &fewer);
        assert!(
            !coverage.is_complete(),
            "dropping {} went unnoticed",
            dropped.record.name
        );
    }
}

#[test]
fn stub_files_that_do_not_parse_cleanly_fail_with_the_file() {
    let (_, errors) = run("ext/standard/a.stub.php", "<?php\nfunction f( {\n");
    assert!(
        errors
            .iter()
            .any(|e| e.starts_with("ext/standard/a.stub.php")
                && e.contains("does not parse cleanly")),
        "{errors:?}"
    );
}

type Check<'a> = &'a dyn Fn(&[Extracted]);

fn vendored(release: &str, path: &str) -> Option<String> {
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../third_party/php-src")
        .join(release)
        .join(path);
    std::fs::read_to_string(file).ok()
}

#[test]
fn real_stubs_of_each_extension_family_match_hand_checked_facts() {
    let release = "php-8.4.26";
    let check = |path: &str, f: Check<'_>| {
        let source = vendored(release, path).unwrap_or_else(|| panic!("{path} is not vendored"));
        let (items, _, errors) =
            extract_source(&MagoBackend::new(), path, source.as_bytes(), v("8.4"));
        assert!(errors.is_empty(), "{errors:?}");
        f(&items);
    };
    // Core, plain functions.
    check("Zend/zend_builtin_functions.stub.php", &|items| {
        let model::Detail::Function(f) = &find(items, "strlen").record.detail else {
            panic!()
        };
        assert_eq!(f.signature.params.len(), 1);
        assert_eq!(f.signature.params[0].name, "string");
        assert_eq!(f.signature.params[0].type_text, "string");
        assert_eq!(f.signature.return_type, "int");
        assert_eq!(find(items, "strlen").record.extension, "core");
    });
    // A class extension with methods, constants and tentative return types.
    check("ext/date/php_date.stub.php", &|items| {
        let m = find(items, "DateTime::format");
        let model::Detail::Method(m) = &m.record.detail else {
            panic!()
        };
        assert_eq!(m.signature.tentative_return_type, "string");
        assert_eq!(m.signature.params[0].type_text, "string");
        assert_eq!(
            find(items, "DateTimeInterface::ATOM").record.extension,
            "date"
        );
    });
    // A namespaced extension.
    check("ext/pdo/pdo_dbh.stub.php", &|items| {
        assert!(items.iter().any(|i| i.record.name.starts_with("PDO::")));
    });
    // Guarded declarations.
    check("ext/standard/basic_functions.stub.php", &|items| {
        assert_eq!(
            find(items, "nl_langinfo").guard.as_deref(),
            Some("defined(HAVE_NL_LANGINFO)")
        );
        assert_eq!(find(items, "strtolower").guard, None);
    });
}

/// A small fixture of `version` with the given global constants (JSON object members).
fn fixture(version: &str, constants: &str) -> bleedthrough_builtins_gen::fixture::Fixture {
    bleedthrough_builtins_gen::fixture::Fixture::parse(&format!(
        r#"{{"php_version":"{version}.1","image":"php@sha256:aa","extensions":["Core"],"functions":{{}},"classes":{{}},"constants":{{{constants}}}}}"#
    ))
    .unwrap()
}

#[test]
fn an_8_0_constant_comes_from_the_fixture_and_is_marked() {
    use bleedthrough_builtins_gen::generate::generate;
    use bleedthrough_builtins_gen::release::ReleaseInput;

    let root = std::env::temp_dir().join(format!("reflected-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("php-8.0.0");
    std::fs::create_dir_all(dir.join("Zend")).unwrap();
    std::fs::write(
        dir.join("Zend/zend.stub.php"),
        "<?php\nfunction f(): void {}\n",
    )
    .unwrap();
    let releases = vec![(
        "php-8.0.0".to_string(),
        ReleaseInput {
            version: v("8.0"),
            root: dir,
        },
    )];
    let constant = r#""E_ALL":{"extension":"Core","type":"int","value":"32767"}"#;
    let generated = generate(
        &MagoBackend::new(),
        &releases,
        "",
        "",
        &[fixture("8.0", constant)],
    )
    .unwrap();
    let data = model::format::read(&generated.data).unwrap();
    let model::Lookup::Found(record) = data.lookup(model::Kind::Constant, "E_ALL", v("8.0")) else {
        panic!("E_ALL not found");
    };
    assert_eq!(record.reflected, model::VersionMask::of(v("8.0")));
    // A release without a fixture is an error, not a silent gap.
    let errors = generate(&MagoBackend::new(), &releases, "", "", &[]).unwrap_err();
    assert!(
        errors.iter().any(|e| e.contains("no Reflection fixture")),
        "{errors:?}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn bundling_must_agree_with_the_stubs() {
    use bleedthrough_builtins_gen::generate::generate;
    use bleedthrough_builtins_gen::release::ReleaseInput;

    let root = std::env::temp_dir().join(format!("bundling-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut releases = Vec::new();
    for minor in ["3", "4"] {
        let dir = root.join(format!("php-8.{minor}.0"));
        std::fs::create_dir_all(dir.join("ext/imap")).unwrap();
        std::fs::write(
            dir.join("ext/imap/imap.stub.php"),
            "<?php\nfunction imap_x(): void {}\n",
        )
        .unwrap();
        releases.push((
            format!("php-8.{minor}.0"),
            ReleaseInput {
                version: v(&format!("8.{minor}")),
                root: dir,
            },
        ));
    }
    let fixtures = [fixture("8.3", ""), fixture("8.4", "")];
    let bundling = "[[extension]]\nname = \"imap\"\nunbundled = \"8.4-8.5\"\nsource = \"s\"\n";
    // The 8.4 tree still has imap stubs: the file and the stubs disagree.
    let errors = generate(&MagoBackend::new(), &releases, "", bundling, &fixtures).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.contains("imap") && e.contains("unbundled")),
        "{errors:?}"
    );
    // With the stubs gone from 8.4 the data is generated and lookups say "unbundled".
    std::fs::remove_dir_all(root.join("php-8.4.0/ext/imap")).unwrap();
    let generated = generate(&MagoBackend::new(), &releases, "", bundling, &fixtures).unwrap();
    let data = model::format::read(&generated.data).unwrap();
    assert_eq!(
        data.lookup(model::Kind::Function, "imap_x", v("8.4")),
        model::Lookup::Unbundled
    );
    assert!(matches!(
        data.lookup(model::Kind::Function, "imap_x", v("8.3")),
        model::Lookup::Found(_)
    ));
    let _ = std::fs::remove_dir_all(root);
}
