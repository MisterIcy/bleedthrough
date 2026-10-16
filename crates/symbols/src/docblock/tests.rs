use super::*;
use crate::{Atom, Fqn, SymbolDiagnosticCode, Type};

fn parse(text: &str) -> Type {
    let result = parse_type(text, &TypeContext::new(), 0);
    assert!(
        result.diagnostics.is_empty(),
        "{text}: {:?}",
        result.diagnostics
    );
    result.ty.unwrap_or_else(|| panic!("{text} did not parse"))
}

fn shown(text: &str) -> String {
    parse(text).to_string()
}

#[test]
fn generic_arrays() {
    let t = parse("array<string, int>");
    assert_eq!(
        t.atoms(),
        [Atom::Array {
            key: Box::new(Type::string()),
            value: Box::new(Type::int()),
            non_empty: false
        }]
    );
    assert_eq!(shown("array<int>"), "array<array-key, int>");
    assert_eq!(shown("string[]"), "array<array-key, string>");
    assert_eq!(shown("int[][]"), "array<array-key, array<array-key, int>>");
    assert_eq!(shown("list<string>|null"), "list<string>|null");
    assert_eq!(shown("array"), "array<array-key, mixed>");
}

#[test]
fn shapes_record_keys_and_optionality() {
    assert_eq!(
        shown("array{id: int, name?: string}"),
        "array{id: int, name?: string}"
    );
    assert_eq!(shown("array{int, string}"), "array{0: int, 1: string}");
    assert_eq!(shown("array{'a b': int, ...}"), "array{'a b': int, ...}");
    assert_eq!(shown("list{int}"), "array{0: int}");
    assert_eq!(
        shown("array{\n  id: int,\n  tags: list<string>\n}"),
        "array{id: int, tags: list<string>}"
    );
}

#[test]
fn class_strings_literals_and_callables() {
    assert_eq!(shown("class-string<App\\Foo>"), "class-string<App\\Foo>");
    assert_eq!(shown("class-string"), "class-string");
    assert_eq!(shown("'a'|'b'"), "'a'|'b'");
    assert_eq!(shown("5|-3"), "-3|5");
    assert_eq!(
        shown("callable(int, string=): void"),
        "callable(int, string=): void"
    );
    assert_eq!(
        shown("Closure(int ...$xs): string"),
        "callable(...int): string"
    );
    assert_eq!(shown("callable"), "callable");
    assert_eq!(shown("int<0, 10>"), "int<0, 10>");
    assert_eq!(shown("int<min, 0>"), "non-positive-int");
}

#[test]
fn nullable_unions_intersections_and_aliases() {
    assert_eq!(shown("?Foo"), "Foo|null");
    assert_eq!(shown("(A&B)|null"), "(A&B)|null");
    assert_eq!(shown("integer|boolean|double"), "bool|int|float");
    assert_eq!(shown("scalar"), "bool|int|float|string");
    assert_eq!(shown("numeric"), "int|float|numeric-string");
    assert_eq!(shown("array-key"), "int|string");
    assert_eq!(shown("no-return"), "never");
    assert_eq!(shown("Suit::Hearts"), "Suit::Hearts");
    assert_eq!(shown("iterable<string, int>"), "iterable<string, int>");
    assert_eq!(shown("iterable<int>"), "iterable<mixed, int>");
}

#[test]
fn names_resolve_through_the_scope_and_class_context() {
    use crate::{ClassContext, Scope};
    use bleedthrough_syntax::{ClassLikeKind, Name, Span, UseItem, UseKind};
    let mut scope = Scope::in_namespace("App");
    scope.add_import(&UseItem {
        span: Span::new(0, 1),
        kind: UseKind::Class,
        name: Name::from_written(Span::new(0, 1), "Lib\\Thing"),
        alias: None,
    });
    let class = ClassContext::new(Fqn::new("App\\Me"), ClassLikeKind::Class, None);
    let ctx = TypeContext::new().with_scope(&scope).with_class(&class);
    let ty = |text: &str| parse_type(text, &ctx, 0).ty.unwrap().to_string();
    assert_eq!(ty("Thing|Other|self"), "App\\Me|App\\Other|Lib\\Thing");
    assert_eq!(ty("static"), "static(App\\Me)");
    assert_eq!(ty("$this"), "static(App\\Me)");
    assert_eq!(ty("\\Abs\\Olute"), "Abs\\Olute");
}

#[test]
fn templates_are_recorded_and_substituted() {
    let templates: Vec<Box<str>> = vec!["T".into()];
    let ctx = TypeContext::new().with_templates(&templates);
    let ty = parse_type("list<T>", &ctx, 0).ty.unwrap();
    assert_eq!(ty.to_string(), "list<T>");
    let bound = ty.substitute(&[("T", Type::string())]);
    assert_eq!(bound.to_string(), "list<string>");
}

#[test]
fn a_broken_type_reports_where_and_yields_no_type() {
    let result = parse_type("array<string, $x", &TypeContext::new(), 100);
    assert!(result.ty.is_none());
    let d = &result.diagnostics[0];
    assert_eq!(d.code, SymbolDiagnosticCode::DocblockSyntax);
    assert_eq!(d.span.start, 100 + "array<string, ".len() as u32);
}

#[test]
fn trailing_text_is_an_error_for_a_complete_type_but_not_for_a_prefix() {
    assert!(parse_type("int foo", &TypeContext::new(), 0).ty.is_none());
    let prefix = super::typeparse::parse_prefix(
        "int $x the value",
        &TypeContext::new(),
        super::typeparse::Locator::contiguous(0),
    );
    assert_eq!(prefix.ty.unwrap().to_string(), "int");
    assert_eq!(&"int $x the value"[prefix.consumed..], "$x the value");
}

#[test]
fn unsupported_constructs_degrade_with_a_note() {
    let result = parse_type("key-of<Foo>|int", &TypeContext::new(), 0);
    let ty = result.ty.unwrap();
    assert!(ty.has_unknown());
    assert_eq!(
        result.diagnostics[0].code,
        SymbolDiagnosticCode::DocblockUnsupported
    );
    let wild = parse_type("Foo::*", &TypeContext::new(), 0);
    assert!(wild.ty.unwrap().has_unknown());
}

#[test]
fn deep_nesting_is_refused() {
    let deep = format!("{}int{}", "list<".repeat(200), ">".repeat(200));
    let result = parse_type(&deep, &TypeContext::new(), 0);
    assert!(result.ty.is_none());
    assert!(result.diagnostics[0].message.contains("nested"));
}

#[test]
fn whitespace_before_a_parenthesis_is_not_a_signature() {
    let prefix = super::typeparse::parse_prefix(
        "callable (see below)",
        &TypeContext::new(),
        super::typeparse::Locator::contiguous(0),
    );
    assert_eq!(prefix.ty.unwrap().to_string(), "callable");
}

const ROUND_TRIP: &[&str] = &[
    "null",
    "void",
    "mixed",
    "unknown",
    "true",
    "false",
    "bool",
    "int",
    "float",
    "string",
    "non-empty-string",
    "numeric-string",
    "class-string",
    "class-string<Foo>",
    "positive-int",
    "negative-int",
    "non-negative-int",
    "non-positive-int",
    "int<2, 9>",
    "int<min, 9>",
    "int<2, max>",
    "5",
    "-3",
    "'abc'",
    "'it\\'s'",
    "array<array-key, mixed>",
    "non-empty-array<string, int>",
    "list<string>",
    "non-empty-list<string>",
    "array{}",
    "array{id: int, name?: string}",
    "array{id: int, ...}",
    "array{0: int, 1: string}",
    "array{...}",
    "object",
    "App\\Foo",
    "Box<int, string>",
    "Suit::Hearts",
    "static(Foo)",
    "callable",
    "callable(int, string=, ...bool): void",
    "callable(): mixed",
    "iterable",
    "iterable<int, string>",
    "resource",
    "(A&B)|null",
    "int|null",
    "never",
    "Foo|null",
    "string|Box<list<int>>",
    "array<string, array{a: int}>",
];

#[test]
fn rendering_and_parsing_round_trip_over_every_atom_kind() {
    let templates: Vec<Box<str>> = vec!["T".into()];
    let ctx = TypeContext::new().with_templates(&templates);
    for text in ROUND_TRIP.iter().chain(&["T", "list<T>|T"]) {
        let first = parse_type(text, &ctx, 0)
            .ty
            .unwrap_or_else(|| panic!("{text}"));
        assert_eq!(&first.to_string(), text, "render of {text}");
        let again = parse_type(&first.to_string(), &ctx, 0).ty.unwrap();
        assert_eq!(first, again, "parse of rendered {text}");
    }
}

#[test]
fn the_round_trip_corpus_covers_every_atom_kind() {
    let mut kinds = std::collections::BTreeSet::new();
    let templates: Vec<Box<str>> = vec!["T".into()];
    let ctx = TypeContext::new().with_templates(&templates);
    for text in ROUND_TRIP.iter().chain(&["T"]) {
        let ty = parse_type(text, &ctx, 0).ty.unwrap();
        collect(&ty, &mut kinds);
    }
    let expected = [
        "Null",
        "Void",
        "Mixed",
        "Unknown",
        "True",
        "False",
        "Bool",
        "Int",
        "IntLiteral",
        "IntRange",
        "Float",
        "String",
        "NonEmptyString",
        "NumericString",
        "StringLiteral",
        "ClassString",
        "Array",
        "List",
        "Shape",
        "Object",
        "Named",
        "EnumCase",
        "Static",
        "Intersection",
        "Template",
        "Callable",
        "Iterable",
        "Resource",
    ];
    for kind in expected {
        assert!(kinds.contains(kind), "no corpus entry for {kind}");
    }
}

fn collect(ty: &Type, kinds: &mut std::collections::BTreeSet<String>) {
    for atom in ty.atoms() {
        let name = format!("{atom:?}");
        let end = name.find(['(', ' ', '{']).unwrap_or(name.len());
        kinds.insert(name[..end].to_string());
        if let Atom::Intersection(parts) = atom {
            for part in parts {
                kinds.insert(
                    format!("{part:?}")
                        .split(['(', ' '])
                        .next()
                        .unwrap()
                        .to_string(),
                );
            }
        }
    }
}

fn doc(text: &str) -> DocBlock {
    parse_docblock(text, 0, &TypeContext::new())
}

#[test]
fn param_and_return_tags() {
    let block = doc(
        "/**\n * @param array<string, int> $x the map\n * @param int|null $y\n * @return list<string>\n */",
    );
    assert_eq!(block.params.len(), 2);
    assert_eq!(&*block.params[0].name, "x");
    assert_eq!(
        block.params[0].ty.as_ref().unwrap().to_string(),
        "array<string, int>"
    );
    assert_eq!(block.params[1].ty.as_ref().unwrap().to_string(), "int|null");
    assert_eq!(block.returns.unwrap().ty.to_string(), "list<string>");
    assert!(block.diagnostics.is_empty());
}

#[test]
fn multi_line_tags_and_variadics() {
    let block = doc(
        "/**\n * @param array{\n *     id: int,\n *     name: string\n * } $row the row\n * @param string ...$parts\n * @param int &$count\n */",
    );
    assert_eq!(block.params.len(), 3);
    assert_eq!(
        block.params[0].ty.as_ref().unwrap().to_string(),
        "array{id: int, name: string}"
    );
    assert!(block.params[1].variadic);
    assert_eq!(&*block.params[1].name, "parts");
    assert!(block.params[2].by_ref);
    assert!(block.diagnostics.is_empty(), "{:?}", block.diagnostics);
}

#[test]
fn var_and_property_tags() {
    let block = doc(
        "/**\n * @var Foo $x\n * @var int\n * @property string $name\n * @property-read int $id\n * @property-write bool $flag\n */",
    );
    assert_eq!(block.vars.len(), 2);
    assert_eq!(block.vars[0].name.as_deref(), Some("x"));
    assert_eq!(block.vars[1].name, None);
    let access: Vec<_> = block.properties.iter().map(|p| p.access).collect();
    assert_eq!(
        access,
        [
            PropertyAccess::ReadWrite,
            PropertyAccess::Read,
            PropertyAccess::Write
        ]
    );
    assert_eq!(&*block.properties[1].name, "id");
}

#[test]
fn method_tags() {
    let block = doc(
        "/**\n * @method static Foo create(array<string> $data, int ...$more)\n * @method string getName()\n * @method doIt($a = [1, 2], $b = null)\n */",
    );
    assert_eq!(block.methods.len(), 3);
    let create = &block.methods[0];
    assert!(create.is_static);
    assert_eq!(&*create.name, "create");
    assert_eq!(create.ret.to_string(), "Foo");
    assert_eq!(create.params.len(), 2);
    assert!(create.params[1].variadic);
    assert_eq!(block.methods[1].ret.to_string(), "string");
    let do_it = &block.methods[2];
    assert!(do_it.ret.has_unknown());
    assert_eq!(do_it.params.len(), 2);
    assert!(do_it.params[0].optional);
    assert!(block.diagnostics.is_empty(), "{:?}", block.diagnostics);
}

#[test]
fn templates_are_recorded_and_used_by_later_tags() {
    let block = doc(
        "/**\n * @template T of Foo\n * @template-covariant U\n * @param T $item\n * @return list<U>\n * @extends Base<T>\n */",
    );
    let names: Vec<_> = block.templates.iter().map(|t| &*t.name).collect();
    assert_eq!(names, ["T", "U"]);
    assert_eq!(
        block.templates[0].bound.as_ref().unwrap().to_string(),
        "Foo"
    );
    assert_eq!(block.params[0].ty.as_ref().unwrap().to_string(), "T");
    assert_eq!(block.returns.unwrap().ty.to_string(), "list<U>");
    assert_eq!(block.inherits[0].to_string(), "Base<T>");
}

#[test]
fn prefixed_tags_replace_plain_ones() {
    let block = doc(
        "/**\n * @param array $a\n * @psalm-param list<string> $a\n * @return int\n * @phpstan-return positive-int\n */",
    );
    assert_eq!(block.params.len(), 1);
    assert_eq!(
        block.params[0].ty.as_ref().unwrap().to_string(),
        "list<string>"
    );
    assert_eq!(block.returns.unwrap().ty.to_string(), "positive-int");
}

#[test]
fn broken_tags_become_diagnostics_with_file_offsets() {
    let text = "/**\n * @param array<string, $x\n * @return int\n */";
    let block = parse_docblock(text, 500, &TypeContext::new());
    assert!(block.params.is_empty());
    assert_eq!(block.returns.unwrap().ty.to_string(), "int");
    let d = &block.diagnostics[0];
    assert_eq!(d.code, SymbolDiagnosticCode::DocblockSyntax);
    let at = d.span.start as usize - 500;
    assert_eq!(&text[at..at + 2], "$x");
}

#[test]
fn unknown_tags_and_inline_tags_are_ignored() {
    let block = doc(
        "/**\n * Summary with {@inheritdoc} inline.\n * @author Someone\n * @deprecated since 1.0\n */",
    );
    assert!(block.params.is_empty() && block.diagnostics.is_empty());
}

#[test]
fn a_param_without_a_name_is_reported() {
    let block = doc("/** @param int */");
    assert_eq!(
        block.diagnostics[0].code,
        SymbolDiagnosticCode::DocblockSyntax
    );
}
