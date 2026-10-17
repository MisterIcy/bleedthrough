//! The fact records: expression keys, call-resolution predicates, counts and lookups.

mod common;
use common::*;

use bleedthrough_symbols::{
    CallResolution, DynamicKind, ExprKey, ExprTag, InferStats, Target, TargetKind, UnresolvedReason,
};

const TAGGED: &str = "
class P { const K = 1; public $p; public static $s; }
const LIMIT = 7;
function g() {}
function f($a, $o) {
    $r = [
        1, \"a$a\", $a, $$a, P::K, LIMIT, static::class, __LINE__, [1], $a[0], $o->p, P::$s, g(), g(...),
        new P, new class {}, -$a, (int)$a, $a + 1, $a = 2, $a ? 1 : 2, match ($a) { default => 1 },
        function () {}, fn() => 1, clone $o, print 1, include 'x', eval('1'), isset($a), empty($a),
        (1), 1 |> g(...),
    ];
    throw new Exception();
    exit(1);
}
function gen() { yield 1; yield from [1]; }";

/// spec: type-inference/Every expression receives a type
#[test]
fn every_expression_kind_has_its_own_tag() {
    let facts = infer(TAGGED);
    let f = unit(&facts, "f");
    let tag_of = |needle: &str| -> &'static str {
        let key = f
            .expr_types
            .iter()
            .map(|(k, _)| *k)
            .find(|k| key_text(TAGGED, k) == needle)
            .unwrap_or_else(|| panic!("no {needle}"));
        key.tag.as_str()
    };
    for (needle, tag) in [
        ("1", "literal"),
        ("\"a$a\"", "interpolated"),
        ("$a", "variable"),
        ("$$a", "variable-variable"),
        ("P", "name"),
        ("P::K", "class-const"),
        ("LIMIT", "const-fetch"),
        ("static", "class-ref"),
        ("__LINE__", "magic-constant"),
        ("[1]", "array"),
        ("$a[0]", "index"),
        ("$o->p", "property"),
        ("P::$s", "static-property"),
        ("g()", "call"),
        ("g(...)", "callable-ref"),
        ("new P", "new"),
        ("new class {}", "anonymous-class"),
        ("-$a", "unary"),
        ("(int)$a", "cast"),
        ("$a + 1", "binary"),
        ("$a = 2", "assign"),
        ("$a ? 1 : 2", "conditional"),
        ("match ($a) { default => 1 }", "match"),
        ("function () {}", "closure"),
        ("fn() => 1", "arrow-fn"),
        ("clone $o", "clone"),
        ("print 1", "print"),
        ("include 'x'", "include"),
        ("eval('1')", "eval"),
        ("isset($a)", "isset"),
        ("empty($a)", "empty"),
        ("(1)", "parenthesized"),
        ("1 |> g(...)", "pipe"),
        ("throw new Exception()", "throw"),
        ("exit(1)", "exit"),
    ] {
        assert_eq!(tag_of(needle), tag, "{needle}");
    }
    let generator = unit(&facts, "gen");
    let gen_tag = |needle: &str| {
        generator
            .expr_types
            .iter()
            .map(|(k, _)| *k)
            .find(|k| key_text(TAGGED, k) == needle)
            .map(|k| k.tag.as_str())
    };
    assert_eq!(gen_tag("yield 1"), Some("yield"));
    assert_eq!(gen_tag("yield from [1]"), Some("yield-from"));
}

/// spec: type-inference/Every expression receives a type
#[test]
fn tags_have_stable_names_and_keys_print_them() {
    let key = ExprKey {
        start: 3,
        end: 9,
        tag: ExprTag::Call,
    };
    assert_eq!(key.to_string(), "3..9:call");
    for (tag, name) in [
        (ExprTag::Error, "error"),
        (ExprTag::Unsupported, "unsupported"),
    ] {
        assert_eq!(tag.as_str(), name);
    }
    use bleedthrough_syntax::ExprKind;
    assert_eq!(
        ExprTag::of(&ExprKind::Error { diagnostic: None }),
        ExprTag::Error
    );
    assert_eq!(
        ExprTag::of(&ExprKind::Unsupported { kind: "x".into() }),
        ExprTag::Unsupported
    );
}

fn target(name: &str, implementors: &[&str]) -> Target {
    Target {
        kind: TargetKind::Method,
        owner: Some(Fqn::new("O")),
        name: name.into(),
        implementors: implementors.iter().map(|n| Fqn::new(n)).collect(),
    }
}

/// spec: type-inference/Calls are resolved with explicit outcomes
#[test]
fn resolution_predicates_classify_the_outcome() {
    let none = CallResolution::resolved(vec![]);
    let one = CallResolution::resolved(vec![target("a", &[])]);
    let two = CallResolution::resolved(vec![target("a", &[]), target("b", &[])]);
    let abstract_one = CallResolution::resolved(vec![target("a", &["X"])]);
    let partial = CallResolution::Resolved {
        targets: vec![target("a", &[])],
        residual: vec![UnresolvedReason::LimitHit],
    };
    let dynamic = CallResolution::Dynamic(DynamicKind::VariableMethod);
    let missing = CallResolution::Unresolved(UnresolvedReason::MethodNotFound);
    for (r, unresolved, is_dynamic, polymorphic) in [
        (&none, false, false, false),
        (&one, false, false, false),
        (&two, false, false, true),
        (&abstract_one, false, false, true),
        (&partial, true, false, false),
        (&dynamic, false, true, false),
        (&missing, true, false, false),
    ] {
        assert_eq!(r.is_unresolved(), unresolved, "{r}");
        assert_eq!(r.is_dynamic(), is_dynamic, "{r}");
        assert_eq!(r.is_polymorphic(), polymorphic, "{r}");
    }
}

/// spec: type-inference/Uncertainty is reported for confidence
#[test]
fn a_limit_is_hit_by_any_one_of_its_counters() {
    let base = InferStats::default();
    assert!(!base.limit_hit());
    assert!(
        InferStats {
            loop_limit_hits: 1,
            ..base
        }
        .limit_hit()
    );
    assert!(
        InferStats {
            loop_limit_hits: 2,
            ..base
        }
        .limit_hit()
    );
    assert!(
        InferStats {
            depth_limit_hit: true,
            ..base
        }
        .limit_hit()
    );
    assert!(
        InferStats {
            budget_exhausted: true,
            ..base
        }
        .limit_hit()
    );
    assert!(
        InferStats {
            widened_types: 1,
            ..base
        }
        .limit_hit()
    );
    assert!(
        InferStats {
            widened_types: 3,
            ..base
        }
        .limit_hit()
    );
    // other counters are not limits
    let busy = InferStats {
        expressions: 4,
        unknown_expressions: 2,
        resolved_calls: 1,
        unresolved_calls: 1,
        dynamic_calls: 1,
        ..base
    };
    assert!(!busy.limit_hit());
}

/// spec: type-inference/Uncertainty is reported for confidence
#[test]
fn the_counts_follow_the_recorded_facts() {
    let src = "function plain(): int { return 1; }
        function f($u) { $a = plain(); $b = plain(); $c = $u->go(); $d = $u->stop(); $e = $u(); $g = $u(); }";
    let facts = infer(src);
    let f = unit(&facts, "f");
    let s = f.stats;
    assert_eq!(s.resolved_calls, 2);
    assert_eq!(s.unresolved_calls, 2);
    assert_eq!(s.dynamic_calls, 2);
    assert_eq!(s.expressions as usize, f.expr_types.len());
    assert_eq!(
        s.unknown_expressions as usize,
        f.expr_types.iter().filter(|(_, t)| t.has_unknown()).count()
    );
    assert!(s.unknown_expressions > 2 && s.unknown_expressions < s.expressions);
    assert_eq!(s.widened_types, 0);
}

/// spec: type-inference/Inference terminates and is bounded
#[test]
fn facts_are_found_by_key_and_missing_keys_are_absent() {
    let src = "function plain(): int { return 1; } function f($u) { $a = plain(); $c = $u->go(); }";
    let facts = infer(src);
    let f = unit(&facts, "f");
    for (key, ty) in &f.expr_types {
        assert_eq!(f.type_of(key), Some(ty));
    }
    for (key, call) in &f.calls {
        assert_eq!(f.call_at(key), Some(call));
    }
    assert!(!f.calls.is_empty());
    let absent = ExprKey {
        start: 0,
        end: 0,
        tag: ExprTag::Eval,
    };
    assert_eq!(f.type_of(&absent), None);
    assert_eq!(f.call_at(&absent), None);
    let literal_key = f
        .expr_types
        .iter()
        .map(|(k, _)| *k)
        .find(|k| k.tag == ExprTag::Literal);
    assert_eq!(literal_key, None);
    let var_key = f
        .expr_types
        .iter()
        .map(|(k, _)| *k)
        .find(|k| k.tag == ExprTag::Variable)
        .unwrap();
    assert_eq!(f.call_at(&var_key), None);
}
