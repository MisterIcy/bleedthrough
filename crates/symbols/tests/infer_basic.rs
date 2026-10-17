//! Local inference over parsed snippets: literals, variables, operators.

mod common;
use common::*;

/// spec: type-inference/Assignments and operators are typed
#[test]
fn smoke_assignments_and_literals() {
    let src = "$a = 1; $b = 'x'; $c = $a + 2;";
    let facts = infer(src);
    let main = unit(&facts, "{main}");
    assert_eq!(ty(main, src, "1"), "1");
    assert_eq!(ty(main, src, "'x'"), "'x'");
    assert_eq!(ty(main, src, "$a + 2"), "3");
}

/// spec: type-inference/Flow-sensitivity with joins and bounded loops
#[test]
fn appending_in_a_loop_converges_without_hitting_the_limit() {
    let src = "function f($xs) { $out = []; foreach ($xs as $u) { $out[] = 'a'; } return $out; }";
    let facts = infer(src);
    let f = unit(&facts, "f");
    assert_eq!(f.stats.loop_limit_hits, 0);
    assert_eq!(ty_last(f, src, "$out"), "non-empty-list<string>|array{}");
}

#[test]
fn keyed_writes_in_a_loop_settle_on_one_shape() {
    let src =
        "function f($xs) { $m = []; foreach ($xs as $x) { $m['k'] = 1; $m[5] = 'v'; } return $m; }";
    let facts = infer(src);
    let f = unit(&facts, "f");
    assert_eq!(f.stats.loop_limit_hits, 0);
    assert_eq!(ty_last(f, src, "$m"), "array{}|array{5: 'v', k: 1}");
}

#[test]
fn a_shape_whose_values_keep_changing_generalises_to_an_array() {
    let src = "function f($xs) { $m = ['k' => 0]; $n = 0; foreach ($xs as $x) { $n = $n + 1; $m['k'] = $n; $m[] = 's'; } return $m; }";
    let facts = infer(src);
    let f = unit(&facts, "f");
    assert_eq!(f.stats.loop_limit_hits, 0, "{}", ty_last(f, src, "$m"));
    let m = ty_last(f, src, "$m");
    assert!(m.contains("array<"), "{m}");
}
