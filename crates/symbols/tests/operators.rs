//! Operator, cast, array literal and element access typing, observed through inference.
#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::type_complexity,
    reason = "tests are long tables of cases and the test parser is a dispatch over PHP syntax"
)]

mod common;
use common::*;

/// spec: type-inference/Assignments and operators are typed
#[test]
fn operators_cast_and_array_expressions_have_the_documented_types() {
    let table: &[(&str, &str, &str)] = &[
        ("", "1 + 2", "3"),
        ("", "5 - 7", "-2"),
        ("", "3 * 4", "12"),
        ("", "$i + 1", "int"),
        ("", "$i + $f", "float"),
        ("", "$f * 2", "float"),
        ("", "$i / 2", "int|float"),
        ("", "$f / 2", "float"),
        ("", "4 / 2", "int|float"),
        ("", "$i ** 2", "int|float"),
        ("", "$f ** 2", "float"),
        ("", "$s + 1", "int|float"),
        ("", "$s - $i", "int|float"),
        ("", "$i % 3", "int"),
        ("", "$i << 1", "int"),
        ("", "$i >> 1", "int"),
        ("", "$i & 1", "int"),
        ("", "$s & $s", "string"),
        ("", "$s | 'a'", "string"),
        ("", "$i ^ $i", "int"),
        ("", "$a + $a", "array<array-key, mixed>"),
        ("", "[1] + [2]", "non-empty-list<1|2>"),
        ("", "['a' => 1] + ['b' => 2]", "array<string, 1|2>"),
        ("", "$a + 1", "unknown"),
        ("", "$u + 1", "unknown"),
        ("", "$n + 1", "int"),
        ("", "$b + 1", "int"),
        ("", "null + 1", "int"),
        ("", "-5", "-5"),
        ("", "-$i", "int"),
        ("", "-$f", "float"),
        ("", "+$s", "int|float"),
        ("", "!$i", "bool"),
        ("", "~$i", "int"),
        ("", "'a' . 'b'", "'ab'"),
        ("", "'a' . 1", "'a1'"),
        ("", "'a' . true", "'a1'"),
        ("", "'a' . null", "'a'"),
        ("", "$s . 'x'", "non-empty-string"),
        ("", "$s . $s", "string"),
        ("", "$i . $i", "non-empty-string"),
        ("", "$f . $s", "non-empty-string"),
        ("", "1 . 2", "'12'"),
        ("", "$i++", "int"),
        ("", "$i--", "int"),
        ("", "$f++", "float"),
        ("", "$n++", "int|null"),
        ("", "$s++", "string"),
        ("", "$b++", "bool"),
        ("", "$m++", "mixed"),
        ("", "$u++", "unknown"),
        ("", "++$i", "int"),
        ("", "--$n", "int|null"),
        ("", "(int) $s", "int"),
        ("", "(float) $s", "float"),
        ("", "(bool) $s", "bool"),
        ("", "(string) $s", "string"),
        ("", "(string) 5", "string"),
        ("", "(string) $i", "string"),
        ("", "(array) $a", "array<array-key, mixed>"),
        ("", "(array) null", "array{}"),
        ("", "(array) $m", "array<array-key, mixed>"),
        ("", "(array) 5", "non-empty-list<5>"),
        ("", "(array) 'x'", "non-empty-list<'x'>"),
        ("", "(object) $a", "stdClass"),
        ("", "(object) $m", "stdClass"),
        ("", "(object) new stdClass", "stdClass"),
        ("", "(array) new stdClass", "array<string, mixed>"),
        ("", "[1, 2]", "non-empty-list<1|2>"),
        ("", "[1, 'a' => 2]", "array{0: 1, a: 2}"),
        ("", "['a' => 1, 'b' => 2]", "array{a: 1, b: 2}"),
        ("", "[3 => 'x', 'y']", "array{3: 'x', 4: 'y'}"),
        ("", "[...$a]", "array<array-key, mixed>"),
        ("", "[1, ...$a]", "non-empty-array<array-key, mixed>"),
        ("", "[$s => 1]", "non-empty-array<string, 1>"),
        ("", "[$i => 1, 2]", "non-empty-array<int, 1|2>"),
        ("", "[0 => 'a', 0 => 'b']", "array{0: 'b'}"),
        ("", "[]", "array{}"),
        ("", "$a[0]", "mixed"),
        ("", "$a['k']", "mixed"),
        ("", "[1, 2][0]", "1|2"),
        ("", "[1, 2][5]", "1|2"),
        ("", "['a' => 1]['a']", "1"),
        ("", "['a' => 1]['b']", "null"),
        ("", "['a' => 1][$s]", "1"),
        ("", "$s[0]", "string"),
        ("", "$m[0]", "mixed"),
        ("", "$u[0]", "unknown"),
        ("", "$i[0]", "null"),
        ("", "null[0]", "null"),
        ("", "$f[0]", "null"),
        ("", "$b[0]", "null"),
        ("", "$n[0]", "null"),
        ("", "$a[]", "mixed"),
    ];
    for (setup, expr, expected) in table {
        assert_eq!(ex(setup, expr), *expected, "{setup} |> {expr}");
    }
}

/// spec: type-inference/Assignments and operators are typed
#[test]
fn writes_compound_assignments_and_misc_expressions_have_the_documented_types() {
    let table: &[(&str, &str, &str)] = &[
        ("$x = []; $x[] = 1;", "$x", "array{0: 1}"),
        ("$x = []; $x['k'] = 'v';", "$x", "array{k: 'v'}"),
        ("$x = [1, 2]; $x[] = 3;", "$x", "non-empty-list<1|2|3>"),
        (
            "$x = [1, 2]; $x['k'] = 3;",
            "$x",
            "non-empty-array<array-key, 1|2|3>",
        ),
        (
            "$x = [1, 2]; $x[0] = 'z';",
            "$x",
            "non-empty-array<int, 1|2|'z'>",
        ),
        (
            "$x = [1, 2]; $x[$i] = 'z';",
            "$x",
            "non-empty-array<int, 1|2|'z'>",
        ),
        (
            "$x = ['a' => 1]; $x[$s] = 2;",
            "$x",
            "non-empty-array<string, 1|2>",
        ),
        ("$x = ['a' => 1]; $x[] = 2;", "$x", "array{0: 2, a: 1}"),
        ("$x = [5 => 1]; $x[] = 2;", "$x", "array{5: 1, 6: 2}"),
        ("$x = ['a' => 1]; $x['a'] = 'b';", "$x", "array{a: 'b'}"),
        ("$x = null; $x[] = 1;", "$x", "array{0: 1}"),
        ("$x = null; $x['k'] = 1;", "$x", "array{k: 1}"),
        (
            "$x = $a; $x[] = 1;",
            "$x",
            "non-empty-array<array-key, mixed>",
        ),
        (
            "$x = $a; $x['k'] = 1;",
            "$x",
            "non-empty-array<array-key, mixed>",
        ),
        (
            "$x = $a; $x[$i] = 's';",
            "$x",
            "non-empty-array<array-key, mixed>",
        ),
        ("$x = 'str'; $x[0] = 'a';", "$x", "'str'"),
        ("$x = $m; $x[] = 1;", "$x", "mixed"),
        (
            "$x = [1]; $x[] = 's'; $x[] = 2.5;",
            "$x",
            "non-empty-list<1|float|'s'>",
        ),
        ("$x = 1; $x += 2;", "$x", "3"),
        ("$x = 1; $x -= 2;", "$x", "-1"),
        ("$x = 2; $x *= 3;", "$x", "6"),
        ("$x = $i; $x /= 2;", "$x", "int|float"),
        ("$x = $i; $x %= 2;", "$x", "int"),
        ("$x = $i; $x **= 2;", "$x", "int|float"),
        ("$x = 'a'; $x .= 'b';", "$x", "'ab'"),
        ("$x = $s; $x .= $s;", "$x", "string"),
        ("$x = $n; $x ??= 5;", "$x", "int"),
        ("$x = $i; $x <<= 1;", "$x", "int"),
        ("$x = $i; $x >>= 1;", "$x", "int"),
        ("$x = $i; $x &= 1;", "$x", "int"),
        ("$x = $i; $x |= 1;", "$x", "int"),
        ("$x = $i; $x ^= 1;", "$x", "int"),
        ("$x = 3; $x++;", "$x", "int"),
        ("$x = 3; $x--;", "$x", "int"),
        ("$x = null; $x++;", "$x", "1"),
        ("$x = null; $x--;", "$x", "null"),
        ("$x = 1.5; $x++;", "$x", "float"),
        ("$x = true; $x++;", "$x", "true"),
        ("$x = 's'; $x--;", "$x", "int|float|string"),
        ("$x = $m; $x--;", "$x", "mixed"),
        ("$x = $u; $x--;", "$x", "unknown"),
        ("$x = $n; $x++;", "$x", "int"),
        ("$x = 4; $x += 1.5;", "$x", "float"),
        ("$x = [1]; $x += [2];", "$x", "non-empty-list<1|2>"),
        ("", "$n ?? 5", "int"),
        ("", "$n ?? 'a'", "int|'a'"),
        ("", "$u ?? 1", "unknown|1"),
        ("", "$i ?? 1", "int"),
        ("", "null ?? 'a'", "'a'"),
        ("", "$b ? 1 : 2", "1|2"),
        ("", "$b ?: 3", "true|3"),
        ("", "$s ?: 'z'", "non-empty-string"),
        ("", "$n ?: 9", "int"),
        ("", "$n ? $n : 'no'", "int|'no'|null"),
        ("", "1 < 2", "bool"),
        ("", "$i == $f", "bool"),
        ("", "$i === $i", "bool"),
        ("", "$i <=> $f", "int<-1, 1>"),
        ("", "$i && $b", "bool"),
        ("", "$b || $i", "bool"),
        ("", "$i instanceof stdClass", "bool"),
        ("", "isset($n)", "bool"),
        ("", "empty($s)", "bool"),
        ("", "-$n", "int"),
        ("", "true", "true"),
        ("", "false", "false"),
        ("", "null", "null"),
        ("", "1.5", "float"),
        ("", "'lit'", "'lit'"),
        ("", "\"dq\"", "'dq'"),
        ("", "\"dq $s\"", "non-empty-string"),
        ("", "__LINE__", "int"),
        ("", "__FILE__", "string"),
        ("", "clone $u", "unknown"),
        ("", "print 'x'", "1"),
        ("", "@$i", "int"),
        ("", "$b ? $i : $s", "int|string"),
    ];
    for (setup, expr, expected) in table {
        assert_eq!(ex(setup, expr), *expected, "{setup} |> {expr}");
    }
}

fn with_prelude(body: &str, expr: &str) -> String {
    let src = format!(
        "{PRELUDE} function nv(): never {{ exit; }} function f(int $i, float $f, string $s, mixed $m, $u, A $o) {{ {body} $r = {expr}; }}"
    );
    let facts = infer(&src);
    ty_last(unit(&facts, "f"), &src, expr)
}

/// spec: type-inference/Assignments and operators are typed
#[test]
fn never_operands_and_object_casts() {
    assert_eq!(with_prelude("", "nv() . 's'"), "never");
    assert_eq!(with_prelude("", "'s' . nv()"), "never");
    assert_eq!(with_prelude("", "nv() + 1"), "never");
    assert_eq!(with_prelude("", "(string) nv()"), "string");
    assert_eq!(with_prelude("", "(object) $o"), "A");
    assert_eq!(with_prelude("", "(object) 5"), "stdClass");
    assert_eq!(with_prelude("", "(object) E::X"), "E::X");
    assert_eq!(with_prelude("", "(object) $o->x"), "stdClass");
    assert_eq!(with_prelude("", "(array) $o"), "array<string, mixed>");
}

/// spec: type-inference/Assignments and operators are typed
#[test]
fn integer_like_operators_ignore_float_and_array_operands() {
    let table = [
        ("$f % 2", "int"),
        ("$f << 1", "int"),
        ("1 >> $f", "int"),
        ("[1] % 2", "int"),
        ("$s & 1", "int"),
        ("1 | $s", "int"),
        ("$s ^ $s", "string"),
        ("$s & $s", "string"),
        ("$s | $i", "int"),
        ("$o + 1", "unknown"),
        ("$f . 1", "non-empty-string"),
    ];
    for (expr, expected) in table {
        assert_eq!(with_prelude("", expr), expected, "{expr}");
    }
}

/// spec: type-inference/Assignments and operators are typed
#[test]
fn array_keys_follow_php_normalisation() {
    let table = [
        ("['1' => 'a']", "array{1: 'a'}"),
        ("['01' => 'a']", "array{'01': 'a'}"),
        ("['-5' => 'a']", "array{-5: 'a'}"),
        ("['+5' => 'a']", "array{'+5': 'a'}"),
        (
            "['9223372036854775808' => 'a']",
            "array{'9223372036854775808': 'a'}",
        ),
        ("[$f => 1]", "non-empty-array<int, 1>"),
        ("[true => 1]", "non-empty-array<int, 1>"),
        ("[$i < 1 => 1]", "non-empty-array<int, 1>"),
        ("[null => 1]", "non-empty-array<string, 1>"),
        ("[$m => 1]", "non-empty-array<array-key, 1>"),
        ("[$u => 1]", "non-empty-array<array-key, 1>"),
        ("[$s => 1]", "non-empty-array<string, 1>"),
        ("[$i => 1]", "non-empty-array<int, 1>"),
        ("[$o => 1]", "non-empty-array<array-key, 1>"),
    ];
    for (expr, expected) in table {
        assert_eq!(with_prelude("", expr), expected, "{expr}");
    }
    assert_eq!(with_prelude("$x = []; $x['5'] = 1;", "$x"), "array{5: 1}");
    assert_eq!(
        with_prelude("$x = []; $x['05'] = 1;", "$x"),
        "array{'05': 1}"
    );
    assert_eq!(with_prelude("$x = ['5' => 'a'];", "$x[5]"), "'a'");
    assert_eq!(with_prelude("$x = [5 => 'a'];", "$x['5']"), "'a'");
}

/// spec: type-inference/Assignments and operators are typed
#[test]
fn mixed_collections_iterate_and_spread_as_mixed() {
    let src = "function f(mixed $m, $u) { foreach ($m as $k => $v) { $a = $k; $b = $v; } foreach ($u as $k2 => $v2) { $c = $k2; $d = $v2; } $e = [...$m]; }";
    let facts = infer(src);
    let f = unit(&facts, "f");
    assert_eq!(ty_last(f, src, "$a = $k"), "mixed");
    assert_eq!(ty_last(f, src, "$b = $v"), "mixed");
    assert_eq!(ty_last(f, src, "$c = $k2"), "unknown");
    assert_eq!(ty_last(f, src, "$d = $v2"), "unknown");
    assert_eq!(ty_last(f, src, "[...$m]"), "array<mixed, mixed>");
}

/// spec: type-inference/Inference terminates and is bounded
#[test]
fn shapes_degrade_to_arrays_past_thirty_two_entries() {
    let entries = |n: usize| {
        (0..n)
            .map(|i| format!("'k{i}' => 1"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let at_limit = with_prelude(&format!("$x = [{}]; $x['new'] = 2;", entries(31)), "$x");
    assert!(at_limit.starts_with("array{"), "{at_limit}");
    let over = with_prelude(&format!("$x = [{}]; $x['new'] = 2;", entries(32)), "$x");
    assert_eq!(over, "non-empty-array<string, 1|2>");
    let literal_over = with_prelude(&format!("$x = [{}];", entries(33)), "$x");
    assert_eq!(literal_over, "non-empty-array<string, 1>");
    let literal_at = with_prelude(&format!("$x = [{}];", entries(32)), "$x");
    assert!(literal_at.starts_with("array{"), "{literal_at}");
}
