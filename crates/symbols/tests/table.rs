//! The symbol table: hierarchy linking, cycle cutting, subtype queries and member lookup, on
//! tables built from parsed snippets.

mod common;
use common::*;

fn fqn(name: &str) -> Fqn {
    Fqn::new(name)
}

fn names(classes: Vec<&ClassDecl>) -> Vec<String> {
    classes.iter().map(|c| c.name.to_string()).collect()
}

// ---- names ----

/// spec: name-resolution/Class names resolve through namespace and imports
#[test]
fn equal_names_hash_alike_and_different_names_do_not() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let hash = |f: &Fqn| {
        let mut h = DefaultHasher::new();
        f.hash(&mut h);
        h.finish()
    };
    assert_eq!(hash(&fqn("App\\Foo")), hash(&fqn("\\app\\FOO")));
    assert_ne!(hash(&fqn("App\\Foo")), hash(&fqn("App\\Bar")));
}

#[test]
fn only_certain_resolutions_have_a_single_name() {
    let name = fqn("A");
    assert_eq!(ResolvedName::Resolved(name.clone()).fqn(), Some(&name));
    assert_eq!(ResolvedName::LateStatic(name.clone()).fqn(), Some(&name));
    let both = ResolvedName::FunctionOrConst {
        namespaced: name.clone(),
        global: name,
    };
    assert_eq!(both.fqn(), None);
}

// ---- hierarchy ----

const HIERARCHY: &str = "
interface I {}
interface J extends I {}
trait T {}
class P implements J { use T; }
class C extends P {}
";

/// spec: symbol-table/Hierarchy is resolved with cycle protection
#[test]
fn ancestors_list_parents_then_interfaces_then_traits() {
    let t = table(HIERARCHY);
    assert_eq!(names(t.ancestors(&fqn("C"))), ["P", "J", "I", "T"]);
    assert_eq!(names(t.ancestors(&fqn("I"))), Vec::<String>::new());
    assert_eq!(names(t.interfaces_of(&fqn("C"))), ["I", "J"]);
    assert_eq!(
        t.parent_of(&fqn("C"))
            .map(|c| c.name.to_string())
            .as_deref(),
        Some("P")
    );
    assert!(t.parent_of(&fqn("P")).is_none());
}

/// spec: symbol-table/Subtype and subclass queries
#[test]
fn subtyping_is_yes_no_or_unknown() {
    let t = table(HIERARCHY);
    assert_eq!(t.is_subtype(&fqn("C"), &fqn("C")), Subtyping::Yes);
    assert_eq!(t.is_subtype(&fqn("C"), &fqn("I")), Subtyping::Yes);
    assert_eq!(t.is_subtype(&fqn("C"), &fqn("T")), Subtyping::Yes);
    assert_eq!(t.is_subtype(&fqn("I"), &fqn("C")), Subtyping::No);
    assert_eq!(t.is_subtype(&fqn("Nope"), &fqn("I")), Subtyping::Unknown);
    let partial = table("class A extends Missing {} class B {}");
    assert_eq!(partial.is_subtype(&fqn("A"), &fqn("B")), Subtyping::Unknown);
    assert_eq!(partial.is_subtype(&fqn("B"), &fqn("A")), Subtyping::No);
    assert_eq!(
        partial.is_subtype(&fqn("A"), &fqn("Missing")),
        Subtyping::Unknown
    );
}

#[test]
fn subtypes_and_implementors_are_transitive_and_sorted() {
    let t = table(HIERARCHY);
    assert_eq!(names(t.subtypes(&fqn("I"))), ["C", "J", "P"]);
    assert_eq!(names(t.implementors(&fqn("I"))), ["C", "P"]);
    assert_eq!(names(t.subtypes(&fqn("T"))), ["C", "P"]);
    assert_eq!(names(t.subtypes(&fqn("C"))), Vec::<String>::new());
    let e = table("interface I {} enum E implements I {} class K implements I {}");
    assert_eq!(names(e.implementors(&fqn("I"))), ["E", "K"]);
}

#[test]
fn missing_ancestors_make_descendants_incomplete() {
    let t = table(
        "class A extends Gone {} class B extends A {} class C {} interface I extends Away {}",
    );
    assert!(t.has_unresolved_ancestor(&fqn("A")));
    assert!(t.has_unresolved_ancestor(&fqn("B")));
    assert!(!t.has_unresolved_ancestor(&fqn("C")));
    assert!(t.has_unresolved_ancestor(&fqn("I")));
    assert!(!t.has_unresolved_ancestor(&fqn("Unknown")));
    assert_eq!(t.unresolved_ancestors(&fqn("A")), [fqn("Gone")]);
    assert_eq!(t.unresolved_ancestors(&fqn("B")), Vec::<Fqn>::new());
    assert_eq!(t.unresolved_ancestors(&fqn("C")), Vec::<Fqn>::new());
}

#[test]
fn a_missing_trait_is_an_unresolved_ancestor() {
    let t = table("class A { use Gone; use Gone; } class B implements I, I {}");
    assert_eq!(t.unresolved_ancestors(&fqn("A")), [fqn("Gone")]);
    assert_eq!(t.unresolved_ancestors(&fqn("B")), [fqn("I")]);
}

#[test]
fn enums_implement_unit_and_backed_enum_when_the_table_has_them() {
    let with_builtins = "interface UnitEnum {} interface BackedEnum extends UnitEnum {}
        enum Plain {} enum Backed: int {} class NotEnum {}";
    let t = table(with_builtins);
    assert_eq!(
        t.is_subtype(&fqn("Plain"), &fqn("UnitEnum")),
        Subtyping::Yes
    );
    assert_eq!(
        t.is_subtype(&fqn("Plain"), &fqn("BackedEnum")),
        Subtyping::No
    );
    assert_eq!(
        t.is_subtype(&fqn("Backed"), &fqn("BackedEnum")),
        Subtyping::Yes
    );
    assert_eq!(
        t.is_subtype(&fqn("NotEnum"), &fqn("UnitEnum")),
        Subtyping::No
    );
    assert!(!t.has_unresolved_ancestor(&fqn("Plain")));
    // without them the enum is not marked incomplete
    let bare = table("enum Plain {} enum Backed: string {}");
    assert!(!bare.has_unresolved_ancestor(&fqn("Plain")));
    assert!(!bare.has_unresolved_ancestor(&fqn("Backed")));
    assert!(bare.ancestors(&fqn("Backed")).is_empty());
    // a declared interface is not listed twice
    let twice = table("interface UnitEnum {} enum E implements UnitEnum {}");
    assert_eq!(names(twice.ancestors(&fqn("E"))), ["UnitEnum"]);
}

/// spec: symbol-table/Hierarchy is resolved with cycle protection
#[test]
fn inheritance_cycles_are_cut_and_reported() {
    let t = table("class A extends B {} class B extends A {} class C extends A {}");
    let cycles: Vec<_> = t
        .diagnostics()
        .iter()
        .filter(|d| d.code == TableDiagnosticCode::InheritanceCycle)
        .collect();
    assert_eq!(cycles.len(), 1);
    assert_eq!(cycles[0].message, "inheritance cycle: A -> B");
    assert_eq!(cycles[0].locations.len(), 2);
    assert!(cycles[0].locations.iter().all(|l| &*l.path == "a.php"));
    // the cut edge is B -> A, so the walk terminates
    assert_eq!(names(t.ancestors(&fqn("A"))), ["B"]);
    assert_eq!(names(t.ancestors(&fqn("B"))), Vec::<String>::new());
    assert_eq!(names(t.ancestors(&fqn("C"))), ["A", "B"]);
    assert!(t.parent_of(&fqn("B")).is_none());
}

#[test]
fn a_cycle_through_an_interface_and_a_self_loop_are_cut() {
    let t = table("interface I extends J {} interface J extends I {} class S extends S {}");
    let cycles = t
        .diagnostics()
        .iter()
        .filter(|d| d.code == TableDiagnosticCode::InheritanceCycle)
        .count();
    assert_eq!(cycles, 2);
    assert_eq!(names(t.ancestors(&fqn("S"))), Vec::<String>::new());
    assert_eq!(names(t.ancestors(&fqn("I"))), ["J"]);
    assert_eq!(t.is_subtype(&fqn("S"), &fqn("I")), Subtyping::No);
}

#[test]
fn a_diamond_is_not_a_cycle_and_lists_each_ancestor_once() {
    let t = table(
        "interface Base {} interface L extends Base {} interface R extends Base {}
         class D implements L, R {} class E extends D implements Base {}",
    );
    assert!(t.diagnostics().is_empty());
    assert_eq!(names(t.ancestors(&fqn("D"))), ["L", "Base", "R"]);
    assert_eq!(names(t.ancestors(&fqn("E"))), ["D", "L", "Base", "R"]);
}

#[test]
fn long_chains_settle_incompleteness_in_post_order() {
    let t = table(
        "class A extends Gone {} class B extends A {} class C extends B {} class D extends C {}
         class Y {} class Z extends Y {}",
    );
    for incomplete in ["A", "B", "C", "D"] {
        assert!(t.has_unresolved_ancestor(&fqn(incomplete)), "{incomplete}");
    }
    for complete in ["Y", "Z"] {
        assert!(!t.has_unresolved_ancestor(&fqn(complete)), "{complete}");
    }
    // an incomplete interface taints implementors through interfaces and traits
    let u = table(
        "interface I extends Gone {} trait T { use Gone; } class A implements I {} class B { use T; }",
    );
    assert!(u.has_unresolved_ancestor(&fqn("A")));
    assert!(u.has_unresolved_ancestor(&fqn("B")));
}

/// spec: symbol-table/The table is built deterministically from summaries
#[test]
fn duplicate_declarations_are_reported_and_the_first_one_wins() {
    let t = table_of(&[
        ("a.php", "class A { function one() {} }"),
        ("b.php", "class A { function two() {} }"),
    ]);
    let duplicates: Vec<_> = t
        .diagnostics()
        .iter()
        .filter(|d| d.code == TableDiagnosticCode::DuplicateClass)
        .collect();
    assert_eq!(duplicates.len(), 1);
    assert_eq!(duplicates[0].locations.len(), 2);
    assert_eq!(t.class_path(&fqn("A")), Some("a.php"));
    assert_eq!(t.class_path(&fqn("Nope")), None);
}

// ---- member lookup ----

const MEMBERS: &str = "
interface Shape { const SIDES = 0; function area(); }
trait Loud { function shout() {} protected function hush() {} public $volume; }
class Base implements Shape {
    const KIND = 'base';
    private const SECRET = 1;
    protected $name;
    private $hidden;
    function area() {}
    protected function guard() {}
    private function inner() {}
}
class Child extends Base { use Loud; function own() {} }
class Magic { function __call($n, $a) {} function __callStatic($n, $a) {} function __get($n) {} function __set($n, $v) {} }
enum Suit { case Hearts; case Spades; const Wild = 1; }
";

/// spec: symbol-table/Member lookup follows PHP semantics
#[test]
fn methods_are_found_on_the_class_its_parents_and_its_traits() {
    let t = table(MEMBERS);
    let own = t.method(&fqn("Child"), "OWN", None);
    assert_eq!(own.candidates.len(), 1);
    assert_eq!(own.candidates[0].owner.name, fqn("Child"));
    assert!(own.accessible().is_some());
    assert!(!own.incomplete);
    let inherited = t.method(&fqn("Child"), "area", None);
    assert_eq!(inherited.candidates[0].owner.name, fqn("Base"));
    // the interface declaration is a second candidate only when it is not the owner already
    assert_eq!(inherited.candidates.len(), 2);
    assert_eq!(inherited.candidates[1].owner.name, fqn("Shape"));
    let from_trait = t.method(&fqn("Child"), "shout", None);
    assert_eq!(from_trait.candidates[0].owner.name, fqn("Loud"));
    assert_eq!(from_trait.candidates[0].context.name, fqn("Child"));
    assert!(
        t.method(&fqn("Child"), "nothing", None)
            .candidates
            .is_empty()
    );
}

#[test]
fn an_unloaded_class_is_incomplete_and_a_class_with_a_missing_ancestor_too() {
    let t = table("class A extends Gone { function m() {} }");
    let unloaded = t.method(&fqn("Nope"), "m", None);
    assert!(unloaded.incomplete);
    assert!(unloaded.candidates.is_empty());
    assert!(!unloaded.magic.any());
    let partial = t.method(&fqn("A"), "other", None);
    assert!(partial.incomplete);
    assert!(!t.method(&fqn("A"), "m", None).candidates.is_empty());
}

#[test]
fn visibility_decides_access() {
    let t = table(MEMBERS);
    let base = fqn("Base");
    let child = fqn("Child");
    let other = fqn("Magic");
    let check = |name: &str, caller: Option<&Fqn>| {
        t.method(&child, name, caller)
            .candidates
            .first()
            .map(|c| c.accessible)
    };
    assert_eq!(check("area", None), Some(true));
    assert_eq!(check("guard", None), Some(false));
    assert_eq!(check("guard", Some(&child)), Some(true));
    assert_eq!(check("guard", Some(&base)), Some(true));
    assert_eq!(check("guard", Some(&other)), Some(false));
    assert_eq!(check("inner", Some(&base)), Some(true));
    assert_eq!(check("inner", Some(&child)), Some(false));
    let private = t.method(&base, "inner", Some(&base));
    assert!(private.candidates[0].accessible);
    let denied = t.method(&base, "inner", Some(&child));
    assert!(!denied.candidates[0].accessible);
    assert!(denied.accessible().is_none());
    // a protected member is also reachable from a parent of the owner's context
    assert!(t.method(&child, "hush", Some(&child)).candidates[0].accessible);
    assert!(!t.method(&child, "hush", Some(&other)).candidates[0].accessible);
    assert!(t.method(&child, "hush", Some(&base)).candidates[0].accessible);
}

#[test]
fn properties_and_constants_are_found_by_exact_name() {
    let t = table(MEMBERS);
    let base = fqn("Base");
    assert_eq!(
        &*t.property(&base, "name", None).candidates[0].member.name,
        "name"
    );
    assert!(t.property(&base, "NAME", None).candidates.is_empty());
    assert!(!t.property(&base, "hidden", None).candidates[0].accessible);
    assert!(t.property(&base, "hidden", Some(&base)).candidates[0].accessible);
    assert_eq!(
        t.property(&fqn("Child"), "volume", None).candidates[0]
            .owner
            .name,
        fqn("Loud")
    );
    let kind = t.class_constant(&fqn("Child"), "KIND", None);
    assert_eq!(kind.candidates[0].owner.name, fqn("Base"));
    assert!(
        t.class_constant(&fqn("Child"), "kind", None)
            .candidates
            .is_empty()
    );
    assert!(!t.class_constant(&base, "SECRET", None).candidates[0].accessible);
    let sides = t.class_constant(&fqn("Child"), "SIDES", None);
    assert_eq!(sides.candidates[0].owner.name, fqn("Shape"));
    assert!(sides.candidates[0].accessible);
}

#[test]
fn magic_methods_are_reported_only_when_nothing_was_found() {
    let t = table(MEMBERS);
    let magic = fqn("Magic");
    let m = t.method(&magic, "anything", None);
    assert!(m.candidates.is_empty());
    assert!(m.magic.call && m.magic.call_static && !m.magic.get && !m.magic.set);
    assert!(m.magic.any());
    let p = t.property(&magic, "anything", None);
    assert!(p.magic.get && p.magic.set && !p.magic.call && !p.magic.call_static);
    // a found member reports no magic
    let found = t.method(&magic, "__call", None);
    assert!(!found.candidates.is_empty() && !found.magic.any());
    // a class without magic methods
    let none = t.method(&fqn("Base"), "zzz", None);
    assert!(!none.magic.any());
    assert_eq!(Magic::default(), none.magic);
    let only_get = table("class G { function __get($n) {} }");
    let m = only_get.method(&fqn("G"), "x", None);
    assert!(!m.magic.any());
    let p = only_get.property(&fqn("G"), "x", None);
    assert!(p.magic.get && !p.magic.set && p.magic.any());
    let only_set = table("class G { function __set($n, $v) {} }");
    assert!(only_set.property(&fqn("G"), "x", None).magic.any());
    let only_static = table("class G { static function __callStatic($n, $a) {} }");
    assert!(only_static.method(&fqn("G"), "x", None).magic.any());
}

#[test]
fn enum_cases_are_found_by_exact_name_on_enums() {
    let t = table(MEMBERS);
    assert!(matches!(
        t.enum_case(&fqn("Suit"), "Hearts"),
        Some(ClassConstant::Case(c)) if &*c.name == "Hearts"
    ));
    assert!(t.enum_case(&fqn("Suit"), "hearts").is_none());
    assert!(t.enum_case(&fqn("Suit"), "Wild").is_none());
    assert!(t.enum_case(&fqn("Nope"), "Hearts").is_none());
    assert!(t.is_interface(&fqn("Shape")));
    assert!(!t.is_interface(&fqn("Base")));
    assert!(!t.is_interface(&fqn("Nope")));
}

/// spec: symbol-table/Member lookup follows PHP semantics
#[test]
fn trait_adaptations_rename_exclude_and_restyle_methods() {
    let src = "
trait A { function hello() {} function shared() {} }
trait B { function shared() {} private function secret() {} }
class U {
    use A, B { A::shared insteadof B; B::shared as bShared; secret as public; hello as protected greet; }
}
class V { use A { hello as private; } }
class W { use A, B { B::shared insteadof A; } }
";
    let t = table(src);
    let u = fqn("U");
    let owner = |name: &str| {
        t.method(&u, name, None)
            .candidates
            .first()
            .map(|c| (c.owner.name.to_string(), c.visibility))
    };
    assert_eq!(owner("shared"), Some(("A".into(), Visibility::Public)));
    assert_eq!(owner("bShared"), Some(("B".into(), Visibility::Public)));
    assert_eq!(owner("BSHARED"), Some(("B".into(), Visibility::Public)));
    assert_eq!(owner("secret"), Some(("B".into(), Visibility::Public)));
    assert_eq!(owner("greet"), Some(("A".into(), Visibility::Protected)));
    assert_eq!(owner("hello"), Some(("A".into(), Visibility::Public)));
    let v = t.method(&fqn("V"), "hello", None);
    assert_eq!(v.candidates[0].visibility, Visibility::Private);
    assert!(!v.candidates[0].accessible);
    assert_eq!(
        t.method(&fqn("W"), "shared", None).candidates[0].owner.name,
        fqn("B")
    );
}

#[test]
fn an_alias_naming_a_different_trait_does_not_apply() {
    let src = "
trait A { function m() {} }
trait B { function m() {} }
class U { use A, B { A::m as fromA; B::m as private fromB; m as protected; } }
";
    let t = table(src);
    let u = fqn("U");
    let a = t.method(&u, "fromA", None);
    assert_eq!(a.candidates[0].owner.name, fqn("A"));
    let b = t.method(&u, "fromB", None);
    assert_eq!(b.candidates[0].owner.name, fqn("B"));
    assert_eq!(b.candidates[0].visibility, Visibility::Private);
    // `m as protected` restyles every trait's m, the first found wins
    let m = t.method(&u, "m", None);
    assert_eq!(m.candidates[0].visibility, Visibility::Protected);
    let scoped = table(
        "trait A { function m() {} } trait B { function m() {} } class U { use A, B { B::m as protected; A::m insteadof B; } }",
    );
    assert_eq!(
        scoped.method(&fqn("U"), "m", None).candidates[0].visibility,
        Visibility::Public
    );
}

#[test]
fn traits_used_by_traits_are_followed_to_a_bounded_depth() {
    let mut src = String::from("trait T0 { function deep() {} }\n");
    for i in 1..=18 {
        src.push_str(&format!("trait T{i} {{ use T{}; }}\n", i - 1));
    }
    src.push_str("class Shallow { use T5; } class AtLimit { use T16; } class Over { use T17; }");
    let t = table(&src);
    let owner = |class: &str| {
        t.method(&fqn(class), "deep", None)
            .candidates
            .first()
            .map(|c| c.owner.name.to_string())
    };
    assert_eq!(owner("Shallow").as_deref(), Some("T0"));
    assert_eq!(owner("AtLimit").as_deref(), Some("T0"));
    assert_eq!(owner("Over"), None);
}

#[test]
fn a_parent_chain_cycle_does_not_loop_in_lookup() {
    let t = table("class A extends B { function a() {} } class B extends A { function b() {} }");
    assert!(!t.method(&fqn("A"), "b", None).candidates.is_empty());
    assert!(t.method(&fqn("B"), "a", None).candidates.is_empty());
}

// ---- declarations, duplicates and accessors ----

#[test]
fn diagnostic_codes_have_stable_spellings() {
    let all = [
        (TableDiagnosticCode::DuplicateClass, "duplicate_class"),
        (TableDiagnosticCode::DuplicateFunction, "duplicate_function"),
        (TableDiagnosticCode::DuplicateConstant, "duplicate_constant"),
        (TableDiagnosticCode::InheritanceCycle, "inheritance_cycle"),
        (TableDiagnosticCode::ShadowsBuiltin, "shadows_builtin"),
    ];
    for (code, text) in all {
        assert_eq!(code.as_str(), text);
    }
}

const DECLS: &str = "
function zeta() {}
function Alpha() {}
const LIMIT = 1;
const AA = 2;
class Beta {}
class alpha {}
";

/// spec: symbol-table/The table is built deterministically from summaries
#[test]
fn accessors_list_declarations_in_name_order() {
    let t = table_of(&[("b.php", DECLS), ("a.php", "function other() {}")]);
    assert_eq!(t.files().len(), 2);
    assert_eq!(&*t.files()[0].path, "a.php");
    let fns: Vec<_> = t.functions().map(|f| f.name.to_string()).collect();
    assert_eq!(fns, ["Alpha", "other", "zeta"]);
    let consts: Vec<_> = t.constants().map(|c| c.name.to_string()).collect();
    assert_eq!(consts, ["AA", "LIMIT"]);
    let classes: Vec<_> = t.classes().map(|c| c.name.to_string()).collect();
    assert_eq!(classes, ["alpha", "Beta"]);
    assert!(t.function(&fqn("ZETA")).is_some());
    assert!(t.function(&fqn("nope")).is_none());
    assert_eq!(
        t.constant(&fqn("LIMIT"))
            .map(|c| c.name.to_string())
            .as_deref(),
        Some("LIMIT")
    );
    assert!(t.constant(&fqn("limit")).is_none());
    assert!(t.class(&fqn("BETA")).is_some());
}

#[test]
fn the_table_answers_for_declared_names() {
    let t = table(DECLS);
    assert_eq!(
        DeclaredNames::function(&t, &fqn("zeta")),
        Existence::Declared
    );
    assert_eq!(
        DeclaredNames::function(&t, &fqn("nope")),
        Existence::Unknown
    );
    assert_eq!(
        DeclaredNames::constant(&t, &fqn("LIMIT")),
        Existence::Declared
    );
    assert_eq!(
        DeclaredNames::constant(&t, &fqn("NOPE")),
        Existence::Unknown
    );
}

#[test]
fn duplicate_functions_and_constants_are_reported() {
    let t = table_of(&[
        ("a.php", "function f() {} const K = 1;"),
        ("b.php", "function F() {} const K = 2;"),
    ]);
    let codes: Vec<_> = t.diagnostics().iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        [
            TableDiagnosticCode::DuplicateFunction,
            TableDiagnosticCode::DuplicateConstant
        ]
    );
    let function = &t.diagnostics()[0];
    assert_eq!(
        function.message,
        "function `f` is declared more than once: a.php:0, b.php:0"
    );
    assert!(
        t.diagnostics()[1]
            .message
            .starts_with("constant `K` is declared more than once")
    );
    assert_eq!(t.function(&fqn("f")).map(|f| f.span.start), Some(0));
}

#[test]
fn a_declaration_shadowing_a_builtin_loses_to_it() {
    let mut builtin = summary(
        "<builtin>",
        "class Exception { function m() {} } function strlen() {} const PHP_EOL = 1;",
    );
    builtin.classes[0].builtin = Some(BuiltinMeta::default());
    builtin.functions[0].builtin = Some(BuiltinMeta::default());
    builtin.constants[0].builtin = Some(BuiltinMeta::default());
    let user = summary(
        "user.php",
        "class Exception { function other() {} } function strlen() {} const PHP_EOL = 2;",
    );
    let t = SymbolTable::build(vec![user, builtin]);
    let codes: Vec<_> = t.diagnostics().iter().map(|d| d.code).collect();
    assert_eq!(codes.len(), 3);
    assert!(
        codes
            .iter()
            .all(|c| *c == TableDiagnosticCode::ShadowsBuiltin)
    );
    assert_eq!(t.class_path(&fqn("Exception")), Some("<builtin>"));
    assert!(t.diagnostics().iter().any(|d| d.message.starts_with(
        "class `Exception` is also built in; the built-in declaration is used: <builtin>:0, user.php:0"
    )));
    assert!(!t.method(&fqn("Exception"), "m", None).candidates.is_empty());
    // two built-ins of one name are a plain duplicate, not a shadow
    let mut other = summary("<other>", "function strlen() {}");
    other.functions[0].builtin = Some(BuiltinMeta::default());
    let mut first = summary("<first>", "function strlen() {}");
    first.functions[0].builtin = Some(BuiltinMeta::default());
    let t = SymbolTable::build(vec![first, other]);
    assert_eq!(
        t.diagnostics()[0].code,
        TableDiagnosticCode::DuplicateFunction
    );
}

#[test]
fn a_visibility_only_adaptation_leaves_a_property_alone() {
    let t = table("trait T { public $m; function m() {} } class U { use T { m as private; } }");
    let property = t.property(&fqn("U"), "m", None);
    assert_eq!(property.candidates[0].visibility, Visibility::Public);
    assert!(property.candidates[0].accessible);
    let method = t.method(&fqn("U"), "m", None);
    assert_eq!(method.candidates[0].visibility, Visibility::Private);
}

#[test]
fn cutting_a_cycle_keeps_the_other_edges_of_the_node() {
    let t = table("interface A extends B, C {} interface B extends A, C {} interface C {}");
    assert_eq!(names(t.ancestors(&fqn("B"))), ["C"]);
    assert_eq!(names(t.ancestors(&fqn("A"))), ["B", "C"]);
    let u =
        table("class P {} class A extends B implements I {} class B extends A {} interface I {}");
    assert_eq!(names(u.ancestors(&fqn("A"))), ["B", "I"]);
    assert_eq!(names(u.ancestors(&fqn("B"))), Vec::<String>::new());
}

// ---- loading through ports ----

use std::collections::BTreeMap;

struct Disk(BTreeMap<&'static str, (&'static str, &'static str)>);

impl ClassLocator for Disk {
    fn locate(&self, class: &Fqn) -> Option<Box<str>> {
        self.0.get(class.key()).map(|(path, _)| (*path).into())
    }
}

impl ClassSource for Disk {
    fn load(&self, path: &str) -> Option<FileSummary> {
        self.0
            .values()
            .find(|(p, _)| *p == path)
            .map(|(p, src)| summary(p, src))
    }
}

fn disk(files: &[(&'static str, &'static str, &'static str)]) -> Disk {
    Disk(files.iter().map(|(k, p, s)| (*k, (*p, *s))).collect())
}

fn loaded(builder: TableBuilder<'_>, src: &str) -> (SymbolTable, LoadReport) {
    builder.build(vec![summary("main.php", src)])
}

/// spec: symbol-table/Loading vendor classes is bounded and explicit
#[test]
fn referenced_classes_are_loaded_transitively_and_missing_ones_reported() {
    let d = disk(&[
        ("a", "a.php", "class A extends B {}"),
        ("b", "b.php", "class B implements Gone {}"),
        ("c", "c.php", "class C {}"),
        ("lie", "lie.php", "class Different {}"),
    ]);
    let (t, report) = loaded(
        TableBuilder::new(&d, &d),
        "class Main extends A { function f(C $c, Lie $l, Unlocated $u) {} }",
    );
    assert_eq!(
        report.loaded,
        ["a.php", "c.php", "lie.php", "b.php"].map(Box::<str>::from)
    );
    assert_eq!(
        report.not_found,
        [fqn("Gone"), fqn("Lie"), fqn("Unlocated")]
    );
    assert!(!report.limit_hit);
    assert!(t.class(&fqn("B")).is_some() && t.class(&fqn("C")).is_some());
    assert_eq!(t.is_subtype(&fqn("Main"), &fqn("B")), Subtyping::Yes);
}

#[test]
fn classes_already_given_are_not_loaded_again() {
    let d = disk(&[
        ("a", "a.php", "class A {}"),
        ("self", "main.php", "class Self_ {}"),
    ]);
    let (_, report) = TableBuilder::new(&d, &d).build(vec![
        summary("main.php", "class Main extends A {}"),
        summary("a.php", "class A {}"),
    ]);
    assert!(report.loaded.is_empty() && report.not_found.is_empty());
    // a class located in a file that is already loaded, but not declared there
    let d = disk(&[("a", "main.php", "")]);
    let (_, report) =
        TableBuilder::new(&d, &d).build(vec![summary("main.php", "class Main extends A {}")]);
    assert_eq!(report.not_found, [fqn("A")]);
    assert!(report.loaded.is_empty());
}

#[test]
fn a_file_that_fails_to_load_leaves_its_class_missing() {
    struct Refusing;
    impl ClassLocator for Refusing {
        fn locate(&self, _: &Fqn) -> Option<Box<str>> {
            Some("x.php".into())
        }
    }
    impl ClassSource for Refusing {
        fn load(&self, _: &str) -> Option<FileSummary> {
            None
        }
    }
    let (_, report) = loaded(
        TableBuilder::new(&Refusing, &Refusing),
        "class M extends A {}",
    );
    assert_eq!(report.not_found, [fqn("A")]);
    assert!(report.loaded.is_empty());
}

#[test]
fn loading_stops_at_the_file_limit() {
    let d = disk(&[
        ("a", "a.php", "class A {}"),
        ("b", "b.php", "class B {}"),
        ("c", "c.php", "class C {}"),
    ]);
    let limits = LoadLimits {
        max_files: 2,
        max_depth: 16,
    };
    let (t, report) = loaded(
        TableBuilder::new(&d, &d).with_limits(limits),
        "class M { function f(A $a, B $b, C $c) {} }",
    );
    assert!(report.limit_hit);
    assert_eq!(report.loaded.len(), 2);
    assert_eq!(t.files().len(), 3);
    // exactly at the limit is fine
    let at = LoadLimits {
        max_files: 3,
        max_depth: 16,
    };
    let (_, report) = loaded(
        TableBuilder::new(&d, &d).with_limits(at),
        "class M { function f(A $a, B $b, C $c) {} }",
    );
    assert!(!report.limit_hit);
    assert_eq!(report.loaded.len(), 3);
}

#[test]
fn loading_stops_at_the_depth_limit() {
    let d = disk(&[
        ("a", "a.php", "class A extends B {}"),
        ("b", "b.php", "class B extends C {}"),
        ("c", "c.php", "class C {}"),
    ]);
    let run = |max_depth| {
        let limits = LoadLimits {
            max_files: 100,
            max_depth,
        };
        loaded(
            TableBuilder::new(&d, &d).with_limits(limits),
            "class M extends A {}",
        )
        .1
    };
    let two = run(2);
    assert!(two.limit_hit);
    assert_eq!(two.loaded.len(), 2);
    let three = run(3);
    assert!(!three.limit_hit);
    assert_eq!(three.loaded.len(), 3);
    assert_eq!(LoadLimits::default().max_files, 5_000);
    assert_eq!(LoadLimits::default().max_depth, 16);
}

// ---- unknown references ----

fn builtins() -> Builtins {
    use bleedthrough_builtins::{
        BuiltinData, ClassDetail, Detail, ExtensionRecord, FunctionDetail, PhpVersion, Record,
        VersionMask,
    };
    let v = |m| PhpVersion::new(m).unwrap();
    let ext = |n: &str| ExtensionRecord {
        name: n.into(),
        unbundled: VersionMask::EMPTY,
    };
    let data = BuiltinData::new(
        vec![ext("standard"), ext("future")],
        vec![
            Record::new(
                "strlen",
                VersionMask::ALL,
                "standard",
                Detail::Function(FunctionDetail::default()),
            ),
            Record::new(
                "newfn",
                VersionMask::range(v(4), v(5)),
                "future",
                Detail::Function(FunctionDetail::default()),
            ),
            Record::new(
                "Exception",
                VersionMask::ALL,
                "standard",
                Detail::Class(ClassDetail::default()),
            ),
            Record::new(
                "NewClass",
                VersionMask::range(v(4), v(5)),
                "future",
                Detail::Class(ClassDetail::default()),
            ),
        ],
    )
    .unwrap();
    Builtins::new(data, v(0))
}

/// spec: builtin-symbols/Unknown built-ins are reported
#[test]
fn unknown_references_are_checked_against_the_table_and_builtins() {
    let main = summary(
        "main.php",
        "namespace App;
        class Local {}
        function helper() {}
        class U { function f(\\Exception $e, \\Missing $m, \\App\\Local $l, \\NewClass $n, \\Zed $z, \\Missing $again) {
            strlen('x'); helper(); newfn(); \\nothing(); strlen('y'); nothing2();
        } }",
    );
    let t = SymbolTable::build(vec![main.clone()]);
    let report = t.unknown_references(&builtins(), [&main]);
    assert_eq!(report.classes, [fqn("Missing"), fqn("Zed")]);
    assert_eq!(report.functions, [fqn("nothing"), fqn("nothing2")]);
    let unavailable: Vec<_> = report
        .unavailable
        .iter()
        .map(|u| u.name.to_string())
        .collect();
    assert_eq!(unavailable, ["NewClass", "newfn"]);
    assert!(
        report
            .unavailable
            .iter()
            .all(|u| u.availability == Availability::NotInVersion)
    );
    assert_ne!(report, UnknownReport::default());
    let empty = t.unknown_references(&builtins(), std::iter::empty());
    assert_eq!(empty, UnknownReport::default());
}

#[test]
fn a_namespaced_function_wins_over_the_global_fallback() {
    let main = summary(
        "main.php",
        "namespace App; function mine() {} function g() { mine(); strlen('x'); other(); }",
    );
    let t = SymbolTable::build(vec![main.clone()]);
    let report = t.unknown_references(&builtins(), [&main]);
    // `other` could be App\other or \other: the global name is reported
    assert_eq!(report.functions, [fqn("other")]);
    assert!(report.classes.is_empty() && report.unavailable.is_empty());
    // a fallback that the table declares is also known
    let lib = summary("lib.php", "function other() {}");
    let t = SymbolTable::build(vec![main.clone(), lib]);
    assert_eq!(
        t.unknown_references(&builtins(), [&main]),
        UnknownReport::default()
    );
}

#[test]
fn cutting_a_trait_cycle_keeps_the_other_traits() {
    let t = table("trait T1 { use T2, T3; } trait T2 { use T1, T3; } trait T3 {}");
    assert_eq!(names(t.ancestors(&fqn("T2"))), ["T3"]);
    assert_eq!(names(t.ancestors(&fqn("T1"))), ["T2", "T3"]);
}

#[test]
fn a_class_is_located_once_and_not_after_an_earlier_file_declared_it() {
    use std::cell::RefCell;
    struct Counting {
        asked: RefCell<Vec<String>>,
    }
    impl ClassLocator for Counting {
        fn locate(&self, class: &Fqn) -> Option<Box<str>> {
            self.asked.borrow_mut().push(class.to_string());
            match class.key() {
                "a" => Some("ab.php".into()),
                "b" => Some("b.php".into()),
                _ => None,
            }
        }
    }
    impl ClassSource for Counting {
        fn load(&self, path: &str) -> Option<FileSummary> {
            match path {
                "ab.php" => Some(summary("ab.php", "class A extends Gone {} class B {}")),
                "b.php" => Some(summary("b.php", "class B {}")),
                _ => None,
            }
        }
    }
    let c = Counting {
        asked: RefCell::new(Vec::new()),
    };
    let (_, report) = loaded(
        TableBuilder::new(&c, &c),
        "class M extends A { function f(B $b, Gone $g) {} }",
    );
    // B was declared by the file that held A, so only A and Gone were looked up, Gone once
    assert_eq!(report.loaded, ["ab.php".into()] as [Box<str>; 1]);
    assert_eq!(*c.asked.borrow(), ["A", "Gone"]);
    assert_eq!(report.not_found, [fqn("Gone")]);
}
