//! Narrowing by conditions: the type of a variable in the branches of an `if`.
#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::type_complexity,
    reason = "tests are long tables of cases and the test parser is a dispatch over PHP syntax"
)]

mod common;
use common::*;

/// spec: type-inference/A defined narrowing subset
#[test]
fn conditions_narrow_the_variable_in_both_branches() {
    let table: &[(&str, &str, &str, &str, &str)] = &[
        (
            "",
            "int|string|null $v",
            "$v === null",
            "null",
            "int|string",
        ),
        (
            "",
            "int|string|null $v",
            "$v !== null",
            "int|string",
            "null",
        ),
        (
            "",
            "int|string|null $v",
            "$v == null",
            "int|string|null",
            "int|string",
        ),
        (
            "",
            "int|string|null $v",
            "$v != null",
            "int|string",
            "int|string|null",
        ),
        ("", "?A $v", "$v == null", "null", "A"),
        ("", "?A $v", "$v === null", "null", "A"),
        ("", "mixed $v", "$v === null", "null", "mixed"),
        ("", "$v", "$v === null", "null", "unknown"),
        ("", "$v", "$v !== null", "unknown", "null"),
        (
            "",
            "int|string|null $v",
            "is_null($v)",
            "null",
            "int|string",
        ),
        (
            "",
            "int|string|null $v",
            "!is_null($v)",
            "int|string",
            "null",
        ),
        ("", "mixed $v", "is_string($v)", "string", "mixed"),
        (
            "",
            "int|string|null $v",
            "is_string($v)",
            "string",
            "int|null",
        ),
        ("", "$v", "is_int($v)", "int", "unknown"),
        (
            "",
            "int|string|float|bool|null $v",
            "is_int($v)",
            "int",
            "bool|float|string|null",
        ),
        (
            "",
            "int|string|float|bool|null $v",
            "is_integer($v)",
            "int",
            "bool|float|string|null",
        ),
        (
            "",
            "int|string|float|bool|null $v",
            "is_long($v)",
            "int",
            "bool|float|string|null",
        ),
        (
            "",
            "int|string|float|bool|null $v",
            "is_float($v)",
            "float",
            "bool|int|string|null",
        ),
        (
            "",
            "int|string|float|bool|null $v",
            "is_double($v)",
            "float",
            "bool|int|string|null",
        ),
        (
            "",
            "int|string|float|bool|null $v",
            "is_bool($v)",
            "bool",
            "int|float|string|null",
        ),
        ("", "mixed $v", "is_bool($v)", "bool", "mixed"),
        (
            "",
            "mixed $v",
            "is_array($v)",
            "array<array-key, mixed>",
            "mixed",
        ),
        (
            "",
            "array|string $v",
            "is_array($v)",
            "array<array-key, mixed>",
            "string",
        ),
        ("", "A|string $v", "is_object($v)", "A", "string"),
        ("", "mixed $v", "is_object($v)", "object", "mixed"),
        ("", "mixed $v", "is_callable($v)", "callable", "mixed"),
        (
            "",
            "callable|string|int|A|array|null $v",
            "is_callable($v)",
            "string|array<array-key, mixed>|A|callable",
            "int|string|array<array-key, mixed>|A|null",
        ),
        (
            "",
            "mixed $v",
            "is_numeric($v)",
            "int|float|numeric-string",
            "mixed",
        ),
        (
            "",
            "int|float|string|bool|null $v",
            "is_numeric($v)",
            "int|float|numeric-string",
            "bool|string|null",
        ),
        (
            "@param numeric-string|non-empty-string|'12'|'ab'|' 3 '|'' $v",
            "string $v",
            "is_numeric($v)",
            "numeric-string",
            "non-empty-string|''",
        ),
        ("", "int|string $v", "! is_int($v)", "string", "int"),
        (
            "",
            "int|string|null $v",
            "is_int($v) || is_null($v)",
            "int|null",
            "string",
        ),
        (
            "",
            "int|string|null $v",
            "is_int($v) && $v !== 5",
            "int",
            "5|string|null",
        ),
        (
            "",
            "int|string|null $v",
            "is_int($v) or is_null($v)",
            "int|null",
            "string",
        ),
        (
            "",
            "int|string|null $v",
            "is_int($v) and $v !== null",
            "int",
            "string|null",
        ),
        ("", "int|string|null $v", "isset($v)", "int|string", "null"),
        ("", "$v", "isset($v)", "unknown", "null"),
        (
            "",
            "int|string|null $v",
            "isset($v, $w)",
            "int|string",
            "int|string|null",
        ),
        (
            "",
            "int|string|null $v",
            "isset($v->x)",
            "int|string|null",
            "int|string|null",
        ),
        (
            "",
            "int|string|null $v",
            "isset($v[0])",
            "int|string|null",
            "int|string|null",
        ),
        (
            "",
            "int|string|null $v",
            "($v === null)",
            "null",
            "int|string",
        ),
        ("", "A|B|C|null $v", "$v instanceof A", "A|B", "C|null"),
        ("", "A|B|C|null $v", "$v instanceof B", "B", "A|C|null"),
        (
            "",
            "A|B|C|null $v",
            "$v instanceof I",
            "B|(A&I)|(C&I)",
            "A|C|null",
        ),
        ("", "A|B|C|null $v", "$v instanceof C", "C", "A|B|null"),
        ("", "A|I $v", "$v instanceof B", "B", "A|I"),
        ("", "I $v", "$v instanceof C", "C&I", "I"),
        ("", "I $v", "$v instanceof A", "A&I", "I"),
        ("", "F $v", "$v instanceof C", "never", "F"),
        ("", "mixed $v", "$v instanceof A", "A", "mixed"),
        ("", "object $v", "$v instanceof A", "A", "object"),
        ("", "$v", "$v instanceof A", "A", "unknown"),
        ("", "A $v", "$v instanceof Unknown", "A&Unknown", "A"),
        ("", "Unknown $v", "$v instanceof A", "A", "Unknown"),
        ("", "E $v", "$v instanceof A", "never", "E"),
        ("", "E $v", "$v instanceof E", "E", "never"),
        ("", "int $v", "$v instanceof A", "never", "int"),
        ("", "A $v", "$v instanceof static", "A", "A"),
        ("", "A $v", "$v instanceof $w", "A", "A"),
        ("", "A $v", "!($v instanceof B)", "A", "B"),
        ("", "I&A $v", "$v instanceof B", "A&B&I", "A&I"),
        (
            "",
            "int|string|bool|null $v",
            "$v === 5",
            "5",
            "bool|int|string|null",
        ),
        (
            "",
            "int|string|bool|null $v",
            "$v === 'x'",
            "'x'",
            "bool|int|string|null",
        ),
        (
            "",
            "int|string|bool|null $v",
            "$v === true",
            "true",
            "false|int|string|null",
        ),
        (
            "",
            "int|string|bool|null $v",
            "$v === false",
            "false",
            "true|int|string|null",
        ),
        (
            "",
            "int|string|bool|null $v",
            "$v !== true",
            "false|int|string|null",
            "true",
        ),
        (
            "",
            "int|string|bool|null $v",
            "5 === $v",
            "5",
            "bool|int|string|null",
        ),
        (
            "",
            "int|string|bool|null $v",
            "$v === -3",
            "-3",
            "bool|int|string|null",
        ),
        (
            "",
            "int|string|bool|null $v",
            "$v == 5",
            "bool|int|string|null",
            "bool|int|string|null",
        ),
        ("@param 1|2|3 $v", "$v", "$v === 2", "2", "1|3"),
        ("", "mixed $v", "$v === 2", "2", "mixed"),
        (
            "@param non-empty-string|int<1, 9>|numeric-string|bool $v",
            "$v",
            "$v === 'a'",
            "'a'",
            "bool|int<1, 9>|non-empty-string",
        ),
        (
            "@param non-empty-string|int<1, 9>|numeric-string|bool $v",
            "$v",
            "$v === 5",
            "5",
            "bool|int<1, 9>|non-empty-string",
        ),
        (
            "@param non-empty-string|int<1, 9>|numeric-string|bool $v",
            "$v",
            "$v === 0",
            "never",
            "bool|int<1, 9>|non-empty-string",
        ),
        (
            "@param non-empty-string|int<1, 9>|numeric-string|bool $v",
            "$v",
            "$v === 12",
            "never",
            "bool|int<1, 9>|non-empty-string",
        ),
        (
            "@param non-empty-string|int<1, 9>|numeric-string|bool $v",
            "$v",
            "$v === ''",
            "never",
            "bool|int<1, 9>|non-empty-string",
        ),
        (
            "@param non-empty-string|int<1, 9>|numeric-string|bool $v",
            "$v",
            "$v === '12'",
            "'12'",
            "bool|int<1, 9>|non-empty-string",
        ),
        (
            "@param non-empty-string|int<1, 9>|numeric-string|bool $v",
            "$v",
            "$v === ' 3 '",
            "' 3 '",
            "bool|int<1, 9>|non-empty-string",
        ),
        (
            "@param int<min, 3>|int<5, max> $v",
            "$v",
            "$v === 4",
            "never",
            "int<min, 3>|int<5, max>",
        ),
        (
            "@param int<min, 3>|int<5, max> $v",
            "$v",
            "$v === 3",
            "3",
            "int<min, 3>|int<5, max>",
        ),
        (
            "@param int<min, 3>|int<5, max> $v",
            "$v",
            "$v === 5",
            "5",
            "int<min, 3>|int<5, max>",
        ),
        (
            "@param int<min, 3>|int<5, max> $v",
            "$v",
            "$v === 9",
            "9",
            "int<min, 3>|int<5, max>",
        ),
        (
            "",
            "int|string $v",
            "$v === 5.5",
            "int|string",
            "int|string",
        ),
        ("", "int|string $v", "$v === $w", "int|string", "int|string"),
    ];
    for (doc, params, cond, yes, no) in table {
        let (y, n) = nr(doc, params, cond);
        assert_eq!((y.as_str(), n.as_str()), (*yes, *no), "{params} / {cond}");
    }
}

/// spec: type-inference/A defined narrowing subset
#[test]
fn empty_splits_a_type_into_falsy_and_truthy_parts() {
    let table: &[(&str, &str, &str, &str, &str)] = &[
        (
            "@param true|false|null|0|1|''|'0'|'a'|'00'|-1 $v",
            "$v",
            "empty($v)",
            "false|0|''|'0'|null",
            "true|-1|1|'00'|'a'",
        ),
        ("", "mixed $v", "empty($v)", "mixed", "mixed"),
        ("", "bool $v", "empty($v)", "false", "true"),
        ("", "string $v", "empty($v)", "string", "non-empty-string"),
        ("", "int $v", "empty($v)", "int", "int"),
        ("", "?A $v", "empty($v)", "null", "A"),
        (
            "",
            "A|I|F|E|object|callable|iterable $v",
            "empty($v)",
            "iterable",
            "object|callable|iterable",
        ),
        (
            "@param non-empty-string|numeric-string|int<1, 5>|int<-5, -1>|int<-2, 2>|int<min, 0>|int<0, max> $v",
            "$v",
            "empty($v)",
            "non-positive-int|int<-2, 2>|non-negative-int",
            "non-positive-int|int<-2, 2>|non-negative-int|non-empty-string",
        ),
        (
            "@param non-empty-array<int, int>|array<int, int>|list<int>|non-empty-list<int>|array{a: int}|array{a?: int}|array{} $v",
            "$v",
            "empty($v)",
            "array<int, int>|array{a?: int}",
            "array<int, int>|array{a?: int}",
        ),
        (
            "",
            "float|void|null $v",
            "empty($v)",
            "void|float|null",
            "float",
        ),
        (
            "",
            "array $v",
            "empty($v)",
            "array<array-key, mixed>",
            "array<array-key, mixed>",
        ),
        ("", "string $v", "!empty($v)", "non-empty-string", "string"),
        ("", "?int $v", "empty($v) || $v === 3", "int|null", "int"),
        ("", "int $v", "empty($w)", "int", "int"),
        ("", "int $v", "empty($v->x)", "int", "int"),
    ];
    for (doc, params, cond, yes, no) in table {
        let (y, n) = nr(doc, params, cond);
        assert_eq!((y.as_str(), n.as_str()), (*yes, *no), "{params} / {cond}");
    }
}

/// spec: type-inference/A defined narrowing subset
#[test]
fn literal_spellings_and_singleton_types_narrow_precisely() {
    let table: &[(&str, &str, &str, &str, &str)] = &[
        (
            "",
            "int|string|null $v",
            "($v) === null",
            "null",
            "int|string",
        ),
        (
            "",
            "int|string|null $v",
            "isset(($v))",
            "int|string",
            "null",
        ),
        (
            "",
            "int|string|null $v",
            "$v === NULL",
            "null",
            "int|string",
        ),
        (
            "",
            "int|string|bool $v",
            "$v === TRUE",
            "true",
            "false|int|string",
        ),
        (
            "",
            "int|string|bool $v",
            "$v === False",
            "false",
            "true|int|string",
        ),
        (
            "",
            "int|string|bool $v",
            "$v === (5)",
            "5",
            "bool|int|string",
        ),
        (
            "",
            "int|string|bool $v",
            "$v === -(5)",
            "bool|int|string",
            "bool|int|string",
        ),
        (
            "",
            "int|string|null $v",
            "$v === Foo",
            "int|string|null",
            "int|string|null",
        ),
        (
            "@param numeric-string $v",
            "$v",
            "$v === '12'",
            "'12'",
            "numeric-string",
        ),
        (
            "@param numeric-string $v",
            "$v",
            "$v === 'ab'",
            "never",
            "numeric-string",
        ),
        (
            "@param numeric-string $v",
            "$v",
            "$v === ' 3 '",
            "' 3 '",
            "numeric-string",
        ),
        (
            "@param '12'|'ab'|1|1.5 $v",
            "$v",
            "is_numeric($v)",
            "1|float|'12'",
            "'ab'",
        ),
        (
            "@param 'ab'|'1e3'|'' $v",
            "$v",
            "is_numeric($v)",
            "'1e3'",
            "''|'ab'",
        ),
        (
            "",
            "int|string|null $v",
            "isset($v, $w->x)",
            "int|string|null",
            "int|string|null",
        ),
        (
            "",
            "int|string|null $v",
            "isset($v, $z)",
            "int|string",
            "int|string|null",
        ),
        (
            "@param int<1, 5> $v",
            "$v",
            "empty($v)",
            "never",
            "int<1, 5>",
        ),
        (
            "@param int<-5, -1> $v",
            "$v",
            "empty($v)",
            "never",
            "int<-5, -1>",
        ),
        (
            "@param int<1, max> $v",
            "$v",
            "empty($v)",
            "never",
            "positive-int",
        ),
        (
            "@param int<min, -1> $v",
            "$v",
            "empty($v)",
            "never",
            "negative-int",
        ),
        (
            "@param int<-2, 2> $v",
            "$v",
            "empty($v)",
            "int<-2, 2>",
            "int<-2, 2>",
        ),
        (
            "@param int<0, 3> $v",
            "$v",
            "empty($v)",
            "int<0, 3>",
            "int<0, 3>",
        ),
        (
            "@param int<-3, 0> $v",
            "$v",
            "empty($v)",
            "int<-3, 0>",
            "int<-3, 0>",
        ),
        (
            "@param array{a: int} $v",
            "$v",
            "empty($v)",
            "never",
            "array{a: int}",
        ),
        (
            "@param array{a?: int} $v",
            "$v",
            "empty($v)",
            "array{a?: int}",
            "array{a?: int}",
        ),
        ("@param array{} $v", "$v", "empty($v)", "array{}", "never"),
        (
            "@param array{a: int, b?: int} $v",
            "$v",
            "empty($v)",
            "never",
            "array{a: int, b?: int}",
        ),
        (
            "@param non-empty-array<int, int> $v",
            "$v",
            "empty($v)",
            "never",
            "non-empty-array<int, int>",
        ),
        (
            "@param non-empty-list<int> $v",
            "$v",
            "empty($v)",
            "never",
            "non-empty-list<int>",
        ),
        (
            "@param list<int> $v",
            "$v",
            "empty($v)",
            "list<int>",
            "list<int>",
        ),
        ("@param '0'|'1' $v", "$v", "empty($v)", "'0'", "'1'"),
        ("@param ''|'a' $v", "$v", "empty($v)", "''", "'a'"),
        ("@param '0' $v", "$v", "empty($v)", "'0'", "never"),
        ("@param 0|1 $v", "$v", "empty($v)", "0", "1"),
        ("@param 5 $v", "$v", "empty($v)", "never", "5"),
        ("@param -5 $v", "$v", "empty($v)", "never", "-5"),
        ("@param 0 $v", "$v", "empty($v)", "0", "never"),
        ("@param ''|'0' $v", "$v", "!empty($v)", "never", "''|'0'"),
        ("@param ''|'0'|'x' $v", "$v", "!empty($v)", "'x'", "''|'0'"),
        (
            "@param ''|'0'|'x'|null|false|0|1 $v",
            "$v",
            "!empty($v)",
            "1|'x'",
            "false|0|''|'0'|null",
        ),
        ("", "?int $v", "!empty($v)", "int", "int|null"),
    ];
    for (doc, params, cond, yes, no) in table {
        let (y, n) = nr(doc, params, cond);
        assert_eq!((y.as_str(), n.as_str()), (*yes, *no), "{params} / {cond}");
    }
}

/// spec: type-inference/A defined narrowing subset
#[test]
fn instanceof_narrows_static_and_enum_case_receivers() {
    let src = format!(
        "{PRELUDE} class K extends A {{ function m() {{ $v = $this; if ($v instanceof K) {{ $yes = $v; }} else {{ $no = $v; }} }} }}"
    );
    let f = infer(&src);
    let m = unit(&f, "K::m");
    assert_eq!(ty_last(m, &src, "$yes = $v"), "static(K)");
    assert_eq!(ty_last(m, &src, "$no = $v"), "never");

    let src = format!(
        "{PRELUDE} class K extends A {{ function m() {{ $v = $this; if ($v instanceof C) {{ $yes = $v; }} else {{ $no = $v; }} }} }}"
    );
    let f = infer(&src);
    let m = unit(&f, "K::m");
    assert_eq!(ty_last(m, &src, "$yes = $v"), "never");
    assert_eq!(ty_last(m, &src, "$no = $v"), "static(K)");

    let src = format!(
        "{PRELUDE} function f() {{ $v = E::X; if ($v instanceof E) {{ $yes = $v; }} else {{ $no = $v; }} }}
        function g() {{ $v = E::X; if ($v instanceof A) {{ $yes = $v; }} else {{ $no = $v; }} }}"
    );
    assert_eq!(
        branch_types_in(&src, "f"),
        ("E::X".to_string(), "never".to_string())
    );
    assert_eq!(
        branch_types_in(&src, "g"),
        ("never".to_string(), "E::X".to_string())
    );
}

fn branch_types_in(src: &str, unit_name: &str) -> (String, String) {
    let facts = infer(src);
    let f = unit(&facts, unit_name);
    (ty_last(f, src, "$yes = $v"), ty_last(f, src, "$no = $v"))
}

/// spec: type-inference/A defined narrowing subset
#[test]
fn match_arms_with_literal_conditions_narrow_the_subject() {
    let src = "/** @param 1|2|3|'a' $v */ function f($v) {
        match ($v) { 1, 2 => $one = $v, 'a' => $two = $v, default => $other = $v };
    }";
    let facts = infer(src);
    let f = unit(&facts, "f");
    assert_eq!(ty_last(f, src, "$one = $v"), "1|2");
    assert_eq!(ty_last(f, src, "$two = $v"), "'a'");
    assert_eq!(ty_last(f, src, "$other = $v"), "3");
    // a non-literal condition leaves the subject alone
    let src = "/** @param 1|2|3 $v */ function f($v, $w) {
        match ($v) { 1, $w => $one = $v, default => $other = $v };
    }";
    let facts = infer(src);
    let f = unit(&facts, "f");
    assert_eq!(ty_last(f, src, "$one = $v"), "1|2|3");
    assert_eq!(ty_last(f, src, "$other = $v"), "1|2|3");
    // a subject that is not a variable
    let src = "/** @param 1|2|3 $v */ function f($v) {
        match ($v + 0) { 1 => $one = $v, default => $other = $v };
    }";
    let facts = infer(src);
    assert_eq!(ty_last(unit(&facts, "f"), src, "$one = $v"), "1|2|3");
}
