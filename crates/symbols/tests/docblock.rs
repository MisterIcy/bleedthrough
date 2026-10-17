//! Docblock type expressions and tags, through the public parsing API.
#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::type_complexity,
    reason = "tests are long tables of cases and the test parser is a dispatch over PHP syntax"
)]

use bleedthrough_symbols::{DocBlock, PropertyAccess, TypeContext, parse_docblock, parse_type};

/// The display of the type `text` parses to; panics unless it parses without a diagnostic.
fn shown(text: &str) -> String {
    let result = parse_type(text, &TypeContext::new(), 0);
    assert!(
        result.diagnostics.is_empty(),
        "{text}: {:?}",
        result.diagnostics
    );
    result
        .ty
        .unwrap_or_else(|| panic!("{text} did not parse"))
        .to_string()
}

/// The diagnostics of `text`, parsed at file offset 10, as (message, start, end).
fn errors(text: &str) -> Vec<(String, u32, u32)> {
    let result = parse_type(text, &TypeContext::new(), 10);
    assert!(result.ty.is_none(), "{text} parsed as {:?}", result.ty);
    result
        .diagnostics
        .iter()
        .map(|d| (d.message.clone(), d.span.start, d.span.end))
        .collect()
}

fn one_error(text: &str, message: &str, start: u32, end: u32) {
    assert_eq!(errors(text), [(message.to_string(), start, end)], "{text}");
}

fn doc(comment: &str) -> DocBlock {
    parse_docblock(comment, 100, &TypeContext::new())
}

/// spec: type-model/Docblock types are parsed by an owned parser
#[test]
fn class_string_spellings_and_the_array_callable() {
    assert_eq!(shown("trait-string"), "class-string");
    assert_eq!(shown("interface-string"), "class-string");
    assert_eq!(shown("enum-string"), "class-string");
    assert_eq!(shown("callable-array"), "array<array-key, mixed>");
    assert_eq!(shown("class-string<A>"), "class-string<A>");
}

/// spec: type-model/Docblock types are parsed by an owned parser
#[test]
fn string_literals_keep_their_escapes() {
    assert_eq!(shown(r"'a\'b'"), r"'a\'b'");
    // a backslash before anything but the quote or a backslash stays a backslash
    assert_eq!(shown(r"'a\nb'"), r"'a\\nb'");
    assert_eq!(shown(r"'a\\b'"), r"'a\\b'");
    // an escaped backslash does not escape the closing quote
    assert_eq!(shown(r"'a\\'"), r"'a\\'");
    assert_eq!(shown(r#""q\"x""#), r#"'q"x'"#);
    one_error(r"'a\'", "expected a type", 10, 11);
}

/// spec: type-model/Docblock types are parsed by an owned parser
#[test]
fn a_minus_needs_a_digit_to_start_a_number() {
    assert_eq!(shown("-5"), "-5");
    assert_eq!(shown("-5_0"), "-50");
    one_error("-x", "expected a type", 10, 11);
    // `_5` is a name, so a lone minus before it is not a number
    one_error("-_5", "expected a type", 10, 11);
    one_error("int<-_5, 1>", "expected an integer bound", 14, 15);
}

/// spec: type-model/Docblock types are parsed by an owned parser
#[test]
fn integer_ranges_take_integers_or_their_own_keywords() {
    assert_eq!(shown("int<0, 10>"), "int<0, 10>");
    assert_eq!(shown("INT<MIN, 3>"), "int<min, 3>");
    assert_eq!(shown("int<0, max>"), "non-negative-int");
    one_error("int<mix, 3>", "expected an integer bound", 14, 15);
    one_error("int<0, mix>", "expected an integer bound", 17, 18);
    // `max` is not a lower bound and `min` is not an upper one
    one_error("int<max, 3>", "expected an integer bound", 14, 15);
    one_error("int<0, min>", "expected an integer bound", 17, 18);
}

/// spec: type-model/Malformed docblocks degrade gracefully
#[test]
fn generic_argument_counts_are_checked() {
    one_error(
        "array<int,string,x>",
        "an array takes one or two type arguments",
        29,
        30,
    );
    one_error("list<int,string>", "a list takes one type argument", 26, 27);
    one_error(
        "iterable<a,b,c>",
        "iterable takes one or two type arguments",
        25,
        26,
    );
    assert_eq!(shown("iterable<int>"), "iterable<mixed, int>");
    assert_eq!(shown("list<int>"), "list<int>");
    assert_eq!(shown("array<string, int>"), "array<string, int>");
}

/// spec: type-model/Docblock types are parsed by an owned parser
#[test]
fn a_class_string_bound_is_one_plain_class_name() {
    for text in ["class-string<A|B>", "class-string<A<int>>"] {
        let result = parse_type(text, &TypeContext::new(), 10);
        assert_eq!(result.ty.unwrap().to_string(), "class-string", "{text}");
        assert_eq!(
            result.diagnostics[0].message,
            "only a single class name is kept as a class-string bound"
        );
    }
}

/// spec: type-model/Docblock types are parsed by an owned parser
#[test]
fn brackets_make_arrays_only_when_they_close() {
    assert_eq!(shown("int[]"), "array<array-key, int>");
    assert_eq!(shown("int[ ]"), "array<array-key, int>");
    assert_eq!(shown("int[][]"), "array<array-key, array<array-key, int>>");
    one_error("int[", "unexpected text after the type", 13, 14);
    one_error("int ]]", "unexpected text after the type", 14, 16);
}

/// spec: type-model/Docblock types are parsed by an owned parser
#[test]
fn closure_is_a_callable_only_when_called_like_one() {
    assert_eq!(shown("Closure"), "Closure");
    assert_eq!(shown("Closure|null"), "Closure|null");
    assert_eq!(shown("Closure(int): void"), "callable(int): void");
    assert_eq!(shown("closure(int): void"), "callable(int): void");
    assert_eq!(shown(r"\Closure(int): void"), "callable(int): void");
    // after a space the parenthesis is prose, not a signature
    let block = doc("/** @return Closure (the closure) */");
    assert_eq!(block.returns.unwrap().ty.to_string(), "Closure");
    let block = doc("/** @return callable (the callable) */");
    assert_eq!(block.returns.unwrap().ty.to_string(), "callable");
}

/// spec: type-model/Docblock types are parsed by an owned parser
#[test]
fn a_brace_after_a_class_name_is_prose() {
    let block = doc("/** @return Foo {an object} */");
    assert_eq!(block.returns.unwrap().ty.to_string(), "Foo");
    assert_eq!(shown("object"), "object");
}

/// The text with `n` levels of nesting, from a pair of wrappers around `int`.
fn nested(open: &str, close: &str, n: usize) -> String {
    format!("{}int{}", open.repeat(n), close.repeat(n))
}

/// spec: type-model/Malformed docblocks degrade gracefully
#[test]
fn nesting_is_limited_to_twenty_four_levels_in_every_construct() {
    let cases: [(&str, String, String, u32); 7] = [
        ("second arguments", "array<int, ".into(), ">".into(), 270),
        ("parentheses", "(".into(), ")".into(), 25),
        ("array arguments", "array<".into(), ">".into(), 150),
        ("callable returns", String::new(), String::new(), 296),
        (
            "callable parameters",
            "callable(".into(),
            "): void".into(),
            224,
        ),
        ("shape values", "array{a: ".into(), "}".into(), 222),
        ("shape entries", "array{".into(), "}".into(), 150),
    ];
    for (name, open, close, at) in cases {
        let build = |n: usize| {
            if name == "callable returns" {
                format!("{}int", "callable(): ".repeat(n))
            } else {
                nested(&open, &close, n)
            }
        };
        let fine = parse_type(&build(24), &TypeContext::new(), 0);
        assert!(fine.ty.is_some() && fine.diagnostics.is_empty(), "{name}");
        let deep = parse_type(&build(25), &TypeContext::new(), 0);
        assert!(deep.ty.is_none(), "{name}");
        let [d] = deep.diagnostics.as_slice() else {
            panic!("{name}: {:?}", deep.diagnostics)
        };
        assert_eq!(d.message, "type is nested too deeply", "{name}");
        assert_eq!((d.span.start, d.span.end), (at, at + 1), "{name}");
    }
}

/// spec: type-model/Docblock types are parsed by an owned parser
#[test]
fn flat_repetition_is_not_nesting() {
    let long_union = format!("{}int", "int|".repeat(60));
    assert_eq!(shown(&long_union), "int");
    assert_eq!(
        shown(&format!("int{}", "[]".repeat(60)))
            .matches("array<")
            .count(),
        60
    );
    assert_eq!(shown(&format!("{}int", "?".repeat(60))), "int|null");
}

/// spec: type-model/Malformed docblocks degrade gracefully
#[test]
fn a_param_tag_reports_a_bad_type_once() {
    let block = doc("/** @param Foo< $x */");
    let spans: Vec<_> = block
        .diagnostics
        .iter()
        .map(|d| (d.message.as_str(), d.span.start, d.span.end))
        .collect();
    assert_eq!(spans, [("expected a type", 116, 117)]);
    let block = doc("/** @param $x */");
    assert_eq!(block.params.len(), 1);
    let block = doc("/** @param int */");
    assert_eq!(
        block.diagnostics[0].message,
        "`@param` needs a parameter name"
    );
}

/// spec: type-model/Docblock types are parsed by an owned parser
#[test]
fn plain_tags_accumulate_and_prefixed_tags_replace_by_name() {
    let block = doc("/** @param int $a\n * @param string $a */");
    let shown: Vec<_> = block
        .params
        .iter()
        .map(|p| {
            (
                p.name.to_string(),
                p.ty.as_ref().unwrap().to_string(),
                p.span.start,
                p.span.end,
            )
        })
        .collect();
    assert_eq!(
        shown,
        [
            ("a".into(), "int".into(), 104, 117),
            ("a".into(), "string".into(), 121, 137)
        ]
    );

    let block = doc("/** @param int $a\n * @param int $b\n * @psalm-param string $b */");
    let shown: Vec<_> = block
        .params
        .iter()
        .map(|p| {
            (
                p.name.to_string(),
                p.ty.as_ref().unwrap().to_string(),
                p.span.start,
            )
        })
        .collect();
    assert_eq!(
        shown,
        [
            ("a".into(), "int".into(), 104),
            ("b".into(), "string".into(), 138)
        ]
    );

    let block = doc("/** @var int $a\n * @var int $b\n * @psalm-var string $b */");
    let shown: Vec<_> = block
        .vars
        .iter()
        .map(|v| {
            (
                v.name.as_deref().unwrap().to_string(),
                v.ty.to_string(),
                v.span.start,
            )
        })
        .collect();
    assert_eq!(
        shown,
        [
            ("a".into(), "int".into(), 104),
            ("b".into(), "string".into(), 134)
        ]
    );

    let block = doc("/** @var int $a\n * @var string $a */");
    assert_eq!(block.vars.len(), 2);
}

/// spec: type-model/Docblock types are parsed by an owned parser
#[test]
fn template_bounds_follow_of_or_as() {
    let block =
        doc("/** @template T of int|string\n * @template-covariant U as Foo\n * @template V */");
    let shown: Vec<_> = block
        .templates
        .iter()
        .map(|t| {
            (
                t.name.to_string(),
                t.bound.as_ref().map(ToString::to_string),
            )
        })
        .collect();
    assert_eq!(
        shown,
        [
            ("T".into(), Some("int|string".into())),
            ("U".into(), Some("Foo".into())),
            ("V".into(), None),
        ]
    );
    // the bound is read from the right place whatever the name's length
    let block = doc("/** @template TKey of int */");
    assert_eq!(
        block.templates[0].bound.as_ref().unwrap().to_string(),
        "int"
    );
    let block = doc("/** @template */");
    assert_eq!(block.diagnostics[0].message, "a template tag needs a name");
}

/// spec: type-model/Docblock types are parsed by an owned parser
#[test]
fn method_tags_read_static_return_name_and_parameters() {
    let block = doc(
        "/** @method static getFoo()\n * @method static Foo make()\n * @method int  spaced (int   $a, string &$b, float ...$c, $d = 1)\n * @method void bare($x, $y)\n * @method callable(int): void cb() */",
    );
    let shown: Vec<_> = block
        .methods
        .iter()
        .map(|m| {
            (
                m.name.to_string(),
                m.is_static,
                m.ret.to_string(),
                m.params
                    .iter()
                    .map(|p| {
                        (
                            p.name.to_string(),
                            p.ty.as_ref().map(ToString::to_string),
                            p.variadic,
                            p.by_ref,
                            p.optional,
                        )
                    })
                    .collect::<Vec<_>>(),
                m.span.start,
                m.span.end,
            )
        })
        .collect();
    let param = |n: &str, t: Option<&str>, v: bool, r: bool, o: bool| {
        (n.to_string(), t.map(String::from), v, r, o)
    };
    assert_eq!(
        shown,
        [
            ("getFoo".into(), false, "unknown".into(), vec![], 104, 127),
            ("make".into(), true, "Foo".into(), vec![], 131, 156),
            (
                "spaced".into(),
                false,
                "int".into(),
                vec![
                    param("a", Some("int"), false, false, false),
                    param("b", Some("string"), false, true, false),
                    param("c", Some("float"), true, false, false),
                    param("d", None, false, false, true),
                ],
                160,
                223
            ),
            (
                "bare".into(),
                false,
                "void".into(),
                vec![
                    param("x", None, false, false, false),
                    param("y", None, false, false, false)
                ],
                227,
                252
            ),
            (
                "cb".into(),
                false,
                "callable(int): void".into(),
                vec![],
                256,
                288
            ),
        ]
    );
    // `static` before a bare name is the return type, which has no class to refer to here
    assert_eq!(block.diagnostics.len(), 1);
    assert_eq!(
        block.diagnostics[0].message,
        "`self`, `static` or `parent` outside a class-like"
    );
    assert_eq!(
        (
            block.diagnostics[0].span.start,
            block.diagnostics[0].span.end
        ),
        (112, 119)
    );
    let block = doc("/** @method void m(int $a */");
    assert_eq!(
        block.diagnostics[0].message,
        "cannot read the `@method` signature"
    );
}

/// spec: type-model/Docblock types are parsed by an owned parser
#[test]
fn property_tags_carry_access_and_report_a_missing_name() {
    let block = doc("/** @property-read Foo<  $x\n * @property int\n */");
    let shown: Vec<_> = block
        .diagnostics
        .iter()
        .map(|d| (d.message.as_str(), d.span.start, d.span.end))
        .collect();
    assert_eq!(
        shown,
        [
            ("expected a type", 125, 126),
            ("a property tag needs a `$name`", 131, 144)
        ]
    );
    let block =
        doc("/** @property-write int $a\n * @property-read string $b\n * @property bool $c */");
    let access: Vec<_> = block
        .properties
        .iter()
        .map(|p| (p.name.to_string(), p.access))
        .collect();
    assert_eq!(
        access,
        [
            ("a".into(), PropertyAccess::Write),
            ("b".into(), PropertyAccess::Read),
            ("c".into(), PropertyAccess::ReadWrite),
        ]
    );
}

/// spec: type-model/Malformed docblocks degrade gracefully
#[test]
fn offsets_follow_a_tag_across_lines() {
    // a tag whose text starts on the next line
    let block = doc("/** @return\n * Foo< */");
    let d = &block.diagnostics[0];
    assert_eq!(
        (d.message.as_str(), d.span.start, d.span.end),
        ("expected a type", 118, 118)
    );
    let block = doc("/** @return\n * int */");
    let ret = block.returns.unwrap();
    assert_eq!(
        (ret.ty.to_string(), ret.span.start, ret.span.end),
        ("int".into(), 104, 118)
    );
    // a continuation line moves the error to its own offset
    let block = doc("/**\n * @param Foo<\n *   bar $x\n * @return int|\n */");
    let shown: Vec<_> = block
        .diagnostics
        .iter()
        .map(|d| (d.message.as_str(), d.span.start, d.span.end))
        .collect();
    assert_eq!(
        shown,
        [("expected `>`", 128, 129), ("expected a type", 145, 145)]
    );
    let block = doc("/**\n   * @return int<\n   */");
    let d = &block.diagnostics[0];
    assert_eq!(
        (d.message.as_str(), d.span.start, d.span.end),
        ("expected an integer bound", 120, 120)
    );
}

/// spec: type-model/Docblock types are parsed by an owned parser
#[test]
fn callable_parameters_may_be_named() {
    assert_eq!(
        shown("callable(int $a_b, string &$b2, int ...$_c): void"),
        "callable(int, string, ...int): void"
    );
    assert_eq!(shown("callable(int $été): void"), "callable(int): void");
    assert!(!errors_of("callable(int $): void").is_empty());
}

fn errors_of(text: &str) -> Vec<String> {
    parse_type(text, &TypeContext::new(), 0)
        .diagnostics
        .into_iter()
        .map(|d| d.message)
        .collect()
}

/// spec: type-model/Docblock types are parsed by an owned parser
#[test]
fn a_tag_whose_text_starts_on_the_next_line_has_no_leading_break() {
    let block = doc("/** @param\n * $a */");
    assert!(block.diagnostics.is_empty(), "{:?}", block.diagnostics);
    assert_eq!(block.params.len(), 1);
    assert!(block.params[0].ty.is_none());
}
