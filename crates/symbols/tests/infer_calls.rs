//! Calls, member access and constants: what each resolves to, and the type that results.

mod common;
use common::*;

const SRC: &str = r####"interface Shape { function area(): float; }
interface Named { function name(): string; }
abstract class Base implements Shape { function area(): float { return 1.0; } abstract function kind(): string; static function make(): static { return new static(); } function chain(): static { return $this; } public int $count = 0; public ?Base $next = null; const TAG = 'b'; }
class Kid extends Base implements Named { function kind(): string { return 'k'; } function name(): string { return 'n'; } function area(): float { return 2.0; } private function secret(): int { return 1; } public string $label = 'l'; public static int $made = 0; const TAG = 'k'; }
class Other extends Base { function kind(): string { return 'o'; } function __construct(public int $x = 0) {} }
class Magic { function __call($n, $a) { return 1; } static function __callStatic($n, $a) { return 's'; } function __get($n) { return 2; } }
trait Tr { function hello(): string { return 'h'; } }
class UsesTr { use Tr; }
function plain(int $a): string { return ''; }
function swap(array &$arr, int &$n) { $arr = []; }
class Inv { function __invoke(int $a): bool { return true; } }
function t(Shape $s, Kid $k, Base $b, Kid|Other $u, Named&Shape $both, Magic $mg, UsesTr $ut, ?Kid $nk, Inv $inv, $unk, string $str, string $cls, Kid ...$rest) {
$r = [];
$r[] = $s->area();
$r[] = $k->area();
$r[] = $b->area();
$r[] = $u->kind();
$r[] = $both->name();
$r[] = $both->area();
$r[] = $mg->anything();
$r[] = Magic::anything();
$r[] = $ut->hello();
$r[] = $nk?->name();
$r[] = $nk->name();
$r[] = $k->nope();
$r[] = $k->secret();
$r[] = $unk->go();
$r[] = $k->$str();
$r[] = $k::make();
$r[] = Kid::make();
$r[] = Base::make();
$r[] = $k->chain()->kind();
$r[] = plain(1);
$r[] = \plain(1);
$r[] = nofunc(1);
$r[] = $unk(1);
$r[] = $inv(1);
$r[] = 'plain'(1);
$r[] = [$k, 'kind']();
$r[] = 'Kid::make'();
$r[] = [new Kid, 'name']();
$r[] = ['Kid', 'make']();
$r[] = new Kid();
$r[] = new Other(3);
$r[] = new Shape();
$r[] = new Gone();
$r[] = new $cls();
$r[] = new $k();
$r[] = new static();
$r[] = new (Kid::class)();
$f = plain(...);
$r[] = $f(1);
$g = $k->area(...);
$r[] = $g();
$h = Kid::make(...);
$r[] = $h();
$cl = fn(int $q): string => 'x';
$r[] = $cl(1);
$arr = [1]; $n = 0;
swap($arr, $n);
$r[] = $arr;
$r[] = $k->count;
$r[] = $k->label;
$r[] = $k->next;
$r[] = $k->next->next;
$r[] = $k->nothing;
$r[] = $mg->dyn;
$r[] = $k->$str;
$r[] = Kid::$made;
$r[] = Kid::$nope;
$r[] = Kid::TAG;
$r[] = Base::TAG;
$r[] = $k::TAG;
$r[] = Kid::NOPE;
$r[] = Kid::$str;
$r[] = $rest[0]->kind();
$r[] = $nk?->label;
$r[] = $u->label;
$r[] = $both->count;
$r[] = Kid::class;
$r[] = $k::class;
$r[] = $unk::class;
}
"####;

/// spec: type-inference/Calls are resolved with explicit outcomes
#[test]
fn calls_resolve_to_declarations_or_say_why_not() {
    let facts = infer(SRC);
    let t = unit(&facts, "t");
    let cases: &[(&str, &str)] = &[
        (
            "$s->area()",
            "resolved [method Shape::area implementors(Base, Kid, Other)]",
        ),
        ("$k->area()", "resolved [method Kid::area]"),
        ("$b->area()", "resolved [method Base::area]"),
        (
            "$u->kind()",
            "resolved [method Kid::kind; method Other::kind]",
        ),
        (
            "$both->name()",
            "resolved [method Named::name implementors(Kid)]",
        ),
        (
            "$both->area()",
            "resolved [method Shape::area implementors(Base, Kid, Other)]",
        ),
        ("$mg->anything()", "unresolved(magic_method)"),
        ("Magic::anything()", "unresolved(magic_method)"),
        ("$ut->hello()", "resolved [method Tr::hello]"),
        ("$nk?->name()", "resolved [method Kid::name]"),
        ("$nk->name()", "resolved [method Kid::name]"),
        ("$k->nope()", "unresolved(method_not_found)"),
        ("$k->secret()", "unresolved(not_accessible)"),
        ("$unk->go()", "unresolved(unknown_receiver_type)"),
        ("$k->$str()", "dynamic(variable_method)"),
        ("$k::make()", "resolved [method Base::make]"),
        ("Kid::make()", "resolved [method Base::make]"),
        ("Base::make()", "resolved [method Base::make]"),
        ("$k->chain()", "resolved [method Base::chain]"),
        ("$k->chain()->kind()", "resolved [method Kid::kind]"),
        ("plain(1)", "resolved [function plain]"),
        ("\\plain(1)", "resolved [function plain]"),
        ("nofunc(1)", "unresolved(function_not_found)"),
        ("$unk(1)", "dynamic(variable_function)"),
        ("$inv(1)", "resolved [method Inv::__invoke]"),
        ("'plain'(1)", "resolved [function plain]"),
        ("'Kid::make'()", "dynamic(callable_string)"),
        ("new Kid", "resolved []"),
        ("new Kid()", "resolved []"),
        ("new Other(3)", "resolved [constructor Other::__construct]"),
        ("new Shape()", "resolved []"),
        ("new Gone()", "unresolved(class_not_loaded)"),
        ("new $cls()", "dynamic(variable_class)"),
        ("new $k()", "resolved []"),
        ("new (Kid::class)()", "dynamic(variable_class)"),
        ("(Kid::class)()", "dynamic(variable_function)"),
        ("plain(...)", "resolved [function plain]"),
        ("$f(1)", "resolved [closure {closure}]"),
        ("$k->area(...)", "resolved [method Kid::area]"),
        ("$g()", "resolved [closure {closure}]"),
        ("Kid::make(...)", "resolved [method Base::make]"),
        ("$h()", "resolved [closure {closure}]"),
        ("$cl(1)", "resolved [closure {closure}]"),
        ("swap($arr, $n)", "resolved [function swap]"),
        ("$rest[0]->kind()", "resolved [method Kid::kind]"),
    ];
    for (needle, expected) in cases {
        assert_eq!(call_last(t, SRC, needle), *expected, "{needle}");
    }
}

/// spec: type-inference/Every expression receives a type
#[test]
fn call_results_members_and_constants_are_typed() {
    let facts = infer(SRC);
    let t = unit(&facts, "t");
    let cases: &[(&str, &str)] = &[
        ("$s->area()", "float"),
        ("$k->area()", "float"),
        ("$b->area()", "float"),
        ("$u->kind()", "string"),
        ("$both->name()", "string"),
        ("$both->area()", "float"),
        ("$mg->anything()", "unknown"),
        ("Magic::anything()", "unknown"),
        ("$ut->hello()", "string"),
        ("$nk?->name()", "string|null"),
        ("$nk->name()", "string"),
        ("$k->nope()", "unknown"),
        ("$k->secret()", "unknown"),
        ("$unk->go()", "unknown"),
        ("$k->$str()", "unknown"),
        ("$k::make()", "Kid"),
        ("Kid::make()", "Kid"),
        ("Base::make()", "Base"),
        ("$k->chain()", "Kid"),
        ("$k->chain()->kind()", "string"),
        ("$unk(1)", "unknown"),
        ("$inv(1)", "bool"),
        ("'Kid::make'", "'Kid::make'"),
        ("'Kid::make'()", "unknown"),
        ("new Kid", "Kid"),
        ("new Kid()", "Kid"),
        ("new Other(3)", "Other"),
        ("new Shape()", "Shape"),
        ("new Gone()", "Gone"),
        ("new $cls()", "object"),
        ("new $k()", "Kid"),
        ("new (Kid::class)()", "object"),
        ("(Kid::class)", "class-string<Kid>"),
        ("(Kid::class)()", "unknown"),
        ("Kid::class", "class-string<Kid>"),
        ("$f = plain(...)", "callable(int): string"),
        ("$f(1)", "string"),
        ("$g = $k->area(...)", "callable(): float"),
        ("$k->area(...)", "callable(): float"),
        ("$g()", "float"),
        ("$h = Kid::make(...)", "callable(): Kid"),
        ("Kid::make(...)", "callable(): Kid"),
        ("$h()", "Kid"),
        ("$cl = fn(int $q): string => 'x'", "callable(int): string"),
        ("$cl(1)", "string"),
        ("$k->count", "int"),
        ("$k->label", "string"),
        ("$k->next", "Base|null"),
        ("$k->next->next", "Base|null"),
        ("$k->nothing", "unknown"),
        ("$mg->dyn", "unknown"),
        ("$k->$str", "unknown"),
        ("Kid::$made", "int"),
        ("Kid::$nope", "unknown"),
        ("Kid::TAG", "'k'"),
        ("Base::TAG", "'b'"),
        ("Kid::NOPE", "unknown"),
        ("Kid::$str", "unknown"),
        ("$rest[0]->kind()", "string"),
        ("$nk?->label", "string|null"),
    ];
    for (needle, expected) in cases {
        assert_eq!(ty_last(t, SRC, needle), *expected, "{needle}");
    }
}

const SRC2: &str = r####"interface Shape { function area(): float; }
interface Named { function name(): string; }
abstract class Base implements Shape { function area(): float { return 1.0; } abstract function kind(): string; function chain(): static { return $this; } function withdef(int $a, int $b = 1, ...$c): int { return 1; } const TAG = 'b'; }
class Kid extends Base implements Named { function kind(): string { return 'k'; } function name(): string { return 'n'; } public string $label = 'l';
  /** @param static&Named $o */ function inter($o) { return $o->chain(); }
  function inside() { return [$this->area(), $this->chain(), $this->kind(), static::TAG, $this->label, $this->nope]; } }
class Other extends Base { function kind(): string { return 'o'; } public string $label = 'o'; }
class Closure { function bindTo(?object $o): ?Closure { return null; } }
enum Suit: string { case H = 'h'; case S = 's'; function label(): string { return 'x'; } const D = self::H; }
/** @template T */
class Box { /** @var T */ public $v; /** @return T */ function get() { return $this->v; } /** @return static */ function me() { return $this; } }
function plain(int $a, int $b = 0): string { return ''; }
function swap(array &$arr, int &$n) { $arr = []; }
function vr(string &...$xs) {}
/**
 * @param Box<int> $bi
 * @param class-string<Kid> $ck
 * @param Kid|Other $u
 * @param Named&Shape $both
 * @param list<int> $list
 * @param ?Kid $nk
 */
function t(Shape $s, Kid $k, Base $b, $u, $both, Box $bi, $ck, $list, $nk, Closure $cl, Suit $su, string $str, $c, string $dync, $fnv) {
  $m = 'area';
  $r[] = $k->$m();
  $mm = $c ? 'area' : 'kind';
  $r[] = $k->$mm();
  $r[] = $k->$str();
  $p = 'label';
  $r[] = $k->$p;
  $pp = $c ? 'label' : 'nope';
  $r[] = $k->$pp;
  $r[] = $b->kind();
  $r[] = $s->area();
  $r[] = $u->kind(...);
  $r[] = $k->kind(...);
  $r[] = $k->withdef(...);
  $r[] = $u->label;
  $r[] = $nk->label;
  $r[] = $nk?->label;
  $r[] = $k?->label;
  $r[] = $k?->area();
  $z = null;
  $r[] = $z?->name();
  $r[] = $z->name();
  $r[] = $z?->label;
  $r[] = $z->label;
  $r[] = $bi->v;
  $r[] = $bi->get();
  $r[] = new $ck();
  $r[] = new $ck;
  $r[] = new $k;
  $r[] = $cl->bindTo($k);
  $r[] = $su->label();
  $r[] = Suit::H->label();
  $e = Suit::H;
  $r[] = $e->label();
  $r[] = Suit::H;
  $r[] = Suit::X;
  $r[] = Kid::{$dync};
  $r[] = plain(a: 1 + 2);
  $r[] = plain(b: 2, a: 1);
  $r[] = plain(...$list);
  $arr = [1]; $n = 0;
  swap($arr, $n);
  $r[] = $arr; $r[] = $n;
  $arr2 = [1]; $n2 = 'a';
  swap(n: $n2, arr: $arr2);
  $r[] = $arr2; $r[] = $n2;
  $v1 = 1; $v2 = 2;
  vr($v1, $v2);
  $r[] = $v1; $r[] = $v2;
  $fv = fn() => 1;
  $r[] = $fv->bindTo($k);
  $oo = new Kid;
  $arr4 = [1]; $n4 = 0;
  $fname = 'swap';
  $fname($arr4, $n4);
  $r[] = $arr4;
  $arr5 = [1]; $n5 = 0;
  $fname2 = $c ? 'swap' : 'plain';
  $fname2($arr5, $n5);
  $r[] = $arr5;
  $r[] = $oo->kind() . ($oo = new Other)->kind();
  $gv = 1;
  global $gv;
  $r[] = $gv;
  $arr3 = [1]; $n3 = 0;
  swap(...[$arr3, $n3]);
  $r[] = $arr3;
}
"####;

fn t2() -> (Vec<FunctionFacts>, &'static str) {
    (infer(SRC2), SRC2)
}

/// spec: type-inference/Calls are resolved with explicit outcomes
#[test]
fn dynamic_names_that_are_literals_resolve() {
    let (facts, src) = t2();
    let t = unit(&facts, "t");
    assert_eq!(
        call_last(t, src, "$k->$m()"),
        "resolved [method Base::area]"
    );
    assert_eq!(ty_last(t, src, "$k->$m()"), "float");
    assert_eq!(
        call_last(t, src, "$k->$mm()"),
        "resolved [method Base::area; method Kid::kind]"
    );
    assert_eq!(ty_last(t, src, "$k->$mm()"), "float|string");
    assert_eq!(call_last(t, src, "$k->$str()"), "dynamic(variable_method)");
    assert_eq!(ty_last(t, src, "$k->$p"), "string");
    assert_eq!(ty_last(t, src, "$k->$pp"), "unknown|string");
}

/// spec: type-inference/Calls are resolved with explicit outcomes
#[test]
fn abstract_and_interface_methods_list_their_implementors() {
    let (facts, src) = t2();
    let t = unit(&facts, "t");
    assert_eq!(
        call_last(t, src, "$b->kind()"),
        "resolved [method Base::kind implementors(Kid, Other)]"
    );
    assert_eq!(
        call_last(t, src, "$s->area()"),
        "resolved [method Shape::area implementors(Base, Kid, Other)]"
    );
    assert_eq!(ty_last(t, src, "$u->kind(...)"), "callable");
    assert_eq!(ty_last(t, src, "$k->kind(...)"), "callable(): string");
    assert_eq!(
        ty_last(t, src, "$k->withdef(...)"),
        "callable(int, int=, ...unknown=): int"
    );
}

/// spec: type-inference/Inference terminates and is bounded
#[test]
fn implementor_lists_are_capped_by_the_candidate_limit() {
    let table = table(SRC2);
    for (cap, listed, limited) in [
        (3, "Base, Kid, Other", false),
        (2, "Base, Kid", true),
        (1, "Base", true),
    ] {
        let limits = InferLimits {
            max_candidates: cap,
            ..InferLimits::default()
        };
        let facts = infer_with(SRC2, &table, &limits);
        let t = unit(&facts, "t");
        let shown = call_last(t, SRC2, "$s->area()");
        let expected = if limited {
            format!("resolved [method Shape::area implementors({listed})] residual(limit_hit)")
        } else {
            format!("resolved [method Shape::area implementors({listed})]")
        };
        assert_eq!(shown, expected, "cap {cap}");
    }
}

/// spec: type-inference/Calls are resolved with explicit outcomes
#[test]
fn nullsafe_calls_and_properties_add_null_only_for_null_receivers() {
    let (facts, src) = t2();
    let t = unit(&facts, "t");
    assert_eq!(ty_last(t, src, "$nk->label"), "string");
    assert_eq!(ty_last(t, src, "$nk?->label"), "string|null");
    assert_eq!(ty_last(t, src, "$k?->label"), "string");
    assert_eq!(ty_last(t, src, "$k?->area()"), "float");
    assert_eq!(ty_last(t, src, "$z?->name()"), "null");
    assert_eq!(ty_last(t, src, "$z->name()"), "unknown");
    assert_eq!(
        call_last(t, src, "$z?->name()"),
        "unresolved(method_not_found)"
    );
    assert_eq!(ty_last(t, src, "$z?->label"), "null");
    assert_eq!(ty_last(t, src, "$z->label"), "unknown");
    assert_eq!(ty_last(t, src, "$u->label"), "string");
}

/// spec: type-inference/Calls are resolved with explicit outcomes
#[test]
fn generics_enums_closures_and_class_strings_are_receivers() {
    let (facts, src) = t2();
    let t = unit(&facts, "t");
    assert_eq!(ty_last(t, src, "$bi->v"), "int");
    assert_eq!(ty_last(t, src, "$bi->get()"), "int");
    assert_eq!(ty_last(t, src, "new $ck()"), "Kid");
    assert_eq!(ty_last(t, src, "new $ck"), "Kid");
    assert_eq!(ty_last(t, src, "new $k"), "Kid");
    assert_eq!(ty_last(t, src, "$cl->bindTo($k)"), "Closure|null");
    assert_eq!(
        call_last(t, src, "$cl->bindTo($k)"),
        "resolved [method Closure::bindTo]"
    );
    assert_eq!(
        call_last(t, src, "$su->label()"),
        "resolved [method Suit::label]"
    );
    assert_eq!(
        call_last(t, src, "Suit::H->label()"),
        "resolved [method Suit::label]"
    );
    assert_eq!(
        call_last(t, src, "$e->label()"),
        "resolved [method Suit::label]"
    );
    assert_eq!(ty_last(t, src, "Suit::H"), "Suit::H");
    assert_eq!(ty_last(t, src, "Suit::X"), "unknown");
    let cl = "function f(Closure $c) { return $c->bindTo(null); } class Closure { function bindTo(?object $o): ?Closure { return null; } }";
    let _ = cl;
}

/// spec: type-inference/Calls are resolved with explicit outcomes
#[test]
fn late_static_receivers_inside_classes() {
    let (facts, src) = t2();
    let m = unit(&facts, "Kid::inside");
    assert_eq!(
        call_last(m, src, "$this->area()"),
        "resolved [method Base::area]"
    );
    assert_eq!(
        call_last(m, src, "$this->kind()"),
        "resolved [method Kid::kind]"
    );
    assert_eq!(ty_last(m, src, "$this->chain()"), "static(Kid)");
    assert_eq!(ty_last(m, src, "$this->label"), "string");
    assert_eq!(ty_last(m, src, "static::TAG"), "'b'");
    assert_eq!(ty_last(m, src, "$this->nope"), "unknown");
}

/// spec: type-inference/Assignments and operators are typed
#[test]
fn arguments_are_typed_and_by_reference_ones_are_updated() {
    let (facts, src) = t2();
    let t = unit(&facts, "t");
    assert_eq!(ty_last(t, src, "plain(a: 1 + 2)"), "string");
    assert_eq!(ty_last(t, src, "1 + 2"), "3");
    assert_eq!(ty_last(t, src, "plain(...$list)"), "string");
    assert_eq!(ty_last(t, src, "$list"), "list<int>");
    let after = |needle: &str, nth: usize| ty_nth(t, src, needle, nth);
    // positional by-reference parameters take their declared type
    assert_eq!(ty_nth(t, src, "$arr", 2), "array<array-key, mixed>");
    assert_eq!(ty_nth(t, src, "$n", 2), "int");
    // named ones are matched by name, in any order
    assert_eq!(ty_nth(t, src, "$arr2", 2), "array<array-key, mixed>");
    assert_eq!(ty_nth(t, src, "$n2", 2), "int");
    // a by-reference variadic covers every remaining positional argument
    assert_eq!(ty_nth(t, src, "$v1", 2), "string");
    assert_eq!(ty_nth(t, src, "$v2", 2), "string");
    // a spread cannot be matched to parameters
    assert_eq!(ty_last(t, src, "$arr3"), "non-empty-list<1>");
    let _ = after;
}

/// spec: type-inference/Every expression receives a type
#[test]
fn dynamic_class_constant_names_type_their_expression() {
    let (facts, src) = t2();
    let t = unit(&facts, "t");
    assert_eq!(ty_last(t, src, "Kid::{$dync}"), "unknown");
    assert_eq!(ty_last(t, src, "$dync"), "string");
}

/// spec: type-inference/Calls are resolved with explicit outcomes
#[test]
fn closure_values_intersections_and_name_state_resolve() {
    let (facts, src) = t2();
    let t = unit(&facts, "t");
    assert_eq!(
        call_last(t, src, "$fv->bindTo($k)"),
        "resolved [method Closure::bindTo]"
    );
    // a literal function name applies that function's by-reference parameters
    assert_eq!(ty_nth(t, src, "$arr4", 2), "array<array-key, mixed>");
    // two possible functions: none of them is assumed
    assert_eq!(ty_nth(t, src, "$arr5", 2), "non-empty-list<1>");
    // each call is resolved in the state in which it runs
    assert_eq!(
        call_last(t, src, "$oo->kind()"),
        "resolved [method Kid::kind]"
    );
    assert_eq!(
        call_last(t, src, "($oo = new Other)->kind()"),
        "resolved [method Other::kind]"
    );
    // `global` forgets what was known
    assert_eq!(ty_nth(t, src, "$gv", 1), "unknown");
    assert_eq!(ty_nth(t, src, "$gv", 2), "unknown");
    let inter = unit(&facts, "Kid::inter");
    assert_eq!(
        call_last(inter, src, "$o->chain()"),
        "resolved [method Base::chain]"
    );
    assert_eq!(ty_last(inter, src, "$o->chain()"), "static(Kid)");
}

/// spec: type-inference/Inference terminates and is bounded
#[test]
fn unknown_values_in_loops_converge() {
    let src = "function f($c, $u) { $x = $u; while ($c) { $y = $x; } }";
    let facts = infer(src);
    let f = unit(&facts, "f");
    assert_eq!(f.stats.loop_limit_hits, 0);
    assert_eq!(ty_last(f, src, "$y = $x"), "unknown");
}

/// spec: type-inference/Inference terminates and is bounded
#[test]
fn closures_beyond_the_depth_limit_are_typed_unknown() {
    let src = "function f() { $a = fn($y) => $y + 1; }";
    let limits = InferLimits {
        max_closure_depth: 0,
        ..InferLimits::default()
    };
    let facts = infer_with(src, &table(src), &limits);
    let f = unit(&facts, "f");
    assert_eq!(ty_last(f, src, "$y + 1"), "unknown");
    assert_eq!(ty_last(f, src, "1"), "unknown");
    assert_eq!(facts.len(), 2);
}

/// spec: type-inference/Calls are resolved with explicit outcomes
#[test]
fn a_call_in_an_assigned_element_key_is_recorded_once_as_it_ran() {
    // The key is evaluated for the assignment and again, silently, to update the array's type;
    // by then `$oo` is a `B`, but the recorded call is the one that ran with an `A`.
    let src = "class A { function kind() { return 1; } } class B { function kind() { return 2; } }
      function f() { $oo = new A; $arr = []; $arr[$oo->kind() . ($oo = new B)->kind()] = 1; }";
    let facts = infer(src);
    let f = unit(&facts, "f");
    assert_eq!(
        call_last(f, src, "$oo->kind()"),
        "resolved [method A::kind]"
    );
    assert_eq!(
        call_last(f, src, "($oo = new B)->kind()"),
        "resolved [method B::kind]"
    );
}
