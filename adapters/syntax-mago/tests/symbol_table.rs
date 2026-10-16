//! Symbol-table scenarios, run through the Mago backend.

use bleedthrough_symbols_suite::symbols::{
    Fqn, Subtyping, SymbolTable, TableDiagnosticCode, Visibility,
};
use bleedthrough_symbols_suite::table_from_sources;
use bleedthrough_syntax_mago::MagoBackend;

fn table(files: &[(&str, &str)]) -> SymbolTable {
    table_from_sources(&MagoBackend::new(), files)
}

fn fqn(name: &str) -> Fqn {
    Fqn::new(name)
}

const BASE: &str = "<?php\nclass Base { public function run(): int { return 1; } protected function guarded() {} private function secret() {} public $open; private $hidden; const LIMIT = 3; }\n";
const CHILD: &str = "<?php\nclass Child extends Base { public function own() {} }\n";

#[test]
fn tables_do_not_depend_on_supply_order() {
    let a = table(&[("a.php", BASE), ("b.php", CHILD)]);
    let b = table(&[("b.php", CHILD), ("a.php", BASE)]);
    assert_eq!(a, b);
}

#[test]
fn duplicate_classes_are_reported_and_the_first_path_wins() {
    let t = table(&[
        ("z.php", "<?php\nclass Dup { function z() {} }\n"),
        ("a.php", "<?php\nclass Dup { function a() {} }\n"),
    ]);
    let d = &t.diagnostics()[0];
    assert_eq!(d.code, TableDiagnosticCode::DuplicateClass);
    let paths: Vec<_> = d.locations.iter().map(|l| &*l.path).collect();
    assert_eq!(paths, ["a.php", "z.php"]);
    assert_eq!(t.class_path(&fqn("dup")), Some("a.php"));
    assert_eq!(&*t.class(&fqn("Dup")).unwrap().methods[0].name, "a");
}

#[test]
fn cyclic_inheritance_terminates_and_stays_usable() {
    let t = table(&[(
        "c.php",
        "<?php\nclass A extends B { function fa() {} }\nclass B extends A { function fb() {} }\nclass S extends S {}\ninterface I extends J {}\ninterface J extends I {}\n",
    )]);
    let cycles = t
        .diagnostics()
        .iter()
        .filter(|d| d.code == TableDiagnosticCode::InheritanceCycle)
        .count();
    assert_eq!(cycles, 3);
    assert!(t.method(&fqn("A"), "fa", None).accessible().is_some());
    assert!(
        t.method(&fqn("B"), "fa", None).accessible().is_some()
            || t.method(&fqn("A"), "fb", None).accessible().is_some()
    );
    assert_eq!(t.is_subtype(&fqn("S"), &fqn("Other")), Subtyping::No);
}

#[test]
fn an_unresolved_parent_is_marked() {
    let t = table(&[(
        "c.php",
        "<?php\nclass Orphan extends Missing { }\nclass Grand extends Orphan {}\nclass Fine {}\n",
    )]);
    assert!(t.has_unresolved_ancestor(&fqn("Orphan")));
    assert!(t.has_unresolved_ancestor(&fqn("Grand")));
    assert!(!t.has_unresolved_ancestor(&fqn("Fine")));
    assert_eq!(t.unresolved_ancestors(&fqn("Orphan")), [fqn("Missing")]);
    let lookup = t.method(&fqn("Grand"), "nope", None);
    assert!(lookup.candidates.is_empty() && lookup.incomplete);
    assert_eq!(
        t.is_subtype(&fqn("Grand"), &fqn("Other")),
        Subtyping::Unknown
    );
    assert_eq!(t.is_subtype(&fqn("Fine"), &fqn("Other")), Subtyping::No);
}

#[test]
fn inherited_members_resolve_to_the_declaring_class() {
    let t = table(&[("a.php", BASE), ("b.php", CHILD)]);
    let child = fqn("Child");
    let run = t.method(&child, "RUN", None);
    let found = run.accessible().unwrap();
    assert_eq!(found.owner.name, fqn("Base"));
    assert!(t.method(&child, "own", None).accessible().is_some());
    assert!(t.property(&child, "open", None).accessible().is_some());
    assert!(
        t.class_constant(&child, "LIMIT", None)
            .accessible()
            .is_some()
    );
    assert!(
        t.class_constant(&child, "limit", None)
            .candidates
            .is_empty()
    );
}

#[test]
fn visibility_depends_on_the_calling_context() {
    let t = table(&[
        ("a.php", BASE),
        ("b.php", CHILD),
        ("c.php", "<?php\nclass Other {}\n"),
    ]);
    let child = fqn("Child");
    let guarded =
        |caller: Option<&Fqn>| t.method(&child, "guarded", caller).candidates[0].accessible;
    assert!(!guarded(None));
    assert!(guarded(Some(&fqn("Child"))));
    assert!(guarded(Some(&fqn("Base"))));
    assert!(!guarded(Some(&fqn("Other"))));
    let secret = t.method(&child, "secret", Some(&fqn("Child")));
    assert!(secret.candidates.is_empty() || !secret.candidates[0].accessible);
    let own_secret = t.method(&fqn("Base"), "secret", Some(&fqn("Base")));
    assert!(own_secret.candidates[0].accessible);
    assert_eq!(own_secret.candidates[0].visibility, Visibility::Private);
}

#[test]
fn trait_methods_resolve_with_the_using_class_as_context() {
    let t = table(&[(
        "t.php",
        "<?php\ntrait Logs { public function log() {} protected function quiet() {} }\nclass Service { use Logs; }\n",
    )]);
    let found = t.method(&fqn("Service"), "log", None);
    let c = found.accessible().unwrap();
    assert_eq!(c.owner.name, fqn("Logs"));
    assert_eq!(c.context.name, fqn("Service"));
    assert!(!t.method(&fqn("Service"), "quiet", None).candidates[0].accessible);
}

#[test]
fn trait_adaptations_are_honoured() {
    let t = table(&[(
        "t.php",
        "<?php\ntrait A { function hello() {} function only() {} }\ntrait B { function hello() {} }\nclass C { use A, B { A::hello insteadof B; B::hello as protected bHello; only as private; } }\n",
    )]);
    let c = fqn("C");
    assert_eq!(
        t.method(&c, "hello", None).candidates[0].owner.name,
        fqn("A")
    );
    let aliased = &t.method(&c, "bHello", None).candidates[0];
    assert_eq!(aliased.owner.name, fqn("B"));
    assert_eq!(aliased.visibility, Visibility::Protected);
    assert_eq!(
        t.method(&c, "only", None).candidates[0].visibility,
        Visibility::Private
    );
}

#[test]
fn magic_methods_are_reported_when_nothing_is_found() {
    let t = table(&[(
        "m.php",
        "<?php\nclass Magic { function __call($n, $a) {} public static function __callStatic($n, $a) {} function __get($n) {} }\nclass Plain {}\n",
    )]);
    let miss = t.method(&fqn("Magic"), "anything", None);
    assert!(miss.candidates.is_empty());
    assert!(miss.magic.call && miss.magic.call_static);
    assert!(t.property(&fqn("Magic"), "anything", None).magic.get);
    assert!(!t.method(&fqn("Plain"), "x", None).magic.any());
}

#[test]
fn interfaces_contribute_candidates_and_enums_expose_cases() {
    let t = table(&[(
        "i.php",
        "<?php\ninterface Shape { const SIDES = 0; function area(): float; }\nabstract class Poly implements Shape { function area(): float { return 1.0; } }\nenum Level: int { case Low = 1; }\n",
    )]);
    let area = t.method(&fqn("Poly"), "area", None);
    let owners: Vec<_> = area
        .candidates
        .iter()
        .map(|c| c.owner.name.to_string())
        .collect();
    assert_eq!(owners, ["Poly", "Shape"]);
    assert!(
        t.class_constant(&fqn("Poly"), "SIDES", None)
            .accessible()
            .is_some()
    );
    assert!(t.enum_case(&fqn("Level"), "Low").is_some());
    assert!(t.enum_case(&fqn("Level"), "High").is_none());
}

#[test]
fn implementors_are_listed_in_name_order() {
    let t = table(&[(
        "l.php",
        "<?php\ninterface Logger {}\nclass Zed implements Logger {}\nclass Alpha implements Logger {}\nclass Mid extends Alpha {}\ninterface Sub extends Logger {}\nclass ViaSub implements Sub {}\nclass Unrelated {}\n",
    )]);
    let names: Vec<_> = t
        .implementors(&fqn("Logger"))
        .iter()
        .map(|c| c.name.to_string())
        .collect();
    assert_eq!(names, ["Alpha", "Mid", "ViaSub", "Zed"]);
    let subs: Vec<_> = t
        .subtypes(&fqn("Alpha"))
        .iter()
        .map(|c| c.name.to_string())
        .collect();
    assert_eq!(subs, ["Mid"]);
    let interfaces: Vec<_> = t
        .interfaces_of(&fqn("ViaSub"))
        .iter()
        .map(|c| c.name.to_string())
        .collect();
    assert_eq!(interfaces, ["Logger", "Sub"]);
    assert_eq!(t.is_subtype(&fqn("Mid"), &fqn("logger")), Subtyping::Yes);
}

mod loading {
    use std::collections::BTreeMap;

    use bleedthrough_symbols_suite::summarize_source;
    use bleedthrough_symbols_suite::symbols::{
        ClassLocator, ClassSource, FileSummary, LoadLimits, TableBuilder,
    };

    use super::*;

    /// An in-memory "vendor": class key -> path, path -> source.
    struct Vendor {
        classes: BTreeMap<&'static str, &'static str>,
        files: BTreeMap<&'static str, &'static str>,
    }

    impl ClassLocator for Vendor {
        fn locate(&self, class: &Fqn) -> Option<Box<str>> {
            self.classes.get(class.key()).map(|p| (*p).into())
        }
    }

    impl ClassSource for Vendor {
        fn load(&self, path: &str) -> Option<FileSummary> {
            let source = self.files.get(path)?;
            Some(summarize_source(&MagoBackend::new(), path, source))
        }
    }

    fn vendor() -> Vendor {
        Vendor {
            classes: BTreeMap::from([
                ("lib\\base", "vendor/base.php"),
                ("lib\\root", "vendor/root.php"),
                ("lib\\used", "vendor/used.php"),
                ("lib\\ghost", "vendor/ghost.php"),
            ]),
            files: BTreeMap::from([
                (
                    "vendor/base.php",
                    "<?php\nnamespace Lib;\nclass Base extends Root { function run() {} }\n",
                ),
                (
                    "vendor/root.php",
                    "<?php\nnamespace Lib;\nclass Root { function top() {} }\n",
                ),
                ("vendor/used.php", "<?php\nnamespace Lib;\nclass Used {}\n"),
            ]),
        }
    }

    fn project() -> Vec<FileSummary> {
        vec![summarize_source(
            &MagoBackend::new(),
            "src/App.php",
            "<?php\nuse Lib\\Base;\nuse Lib\\Used;\nuse Lib\\Missing;\nclass App extends Base { function f() { return new Used(); } function g() { new Missing(); } }\n",
        )]
    }

    #[test]
    fn the_closure_loads_the_vendor_parent_and_reports_the_absent_class() {
        let v = vendor();
        let (table, report) = TableBuilder::new(&v, &v).build(project());
        assert!(
            table
                .method(&fqn("App"), "top", None)
                .accessible()
                .is_some()
        );
        assert!(table.class(&fqn("Lib\\Used")).is_some());
        assert_eq!(report.not_found, [fqn("Lib\\Missing")]);
        assert!(!report.limit_hit);
        assert_eq!(&*report.loaded[0], "vendor/base.php");
        assert!(!table.has_unresolved_ancestor(&fqn("App")));
    }

    #[test]
    fn the_file_limit_stops_loading_with_a_valid_table() {
        let v = vendor();
        let (table, report) = TableBuilder::new(&v, &v)
            .with_limits(LoadLimits {
                max_files: 1,
                max_depth: 16,
            })
            .build(project());
        assert!(report.limit_hit);
        assert_eq!(report.loaded.len(), 1);
        assert!(table.class(&fqn("App")).is_some());
    }

    #[test]
    fn the_depth_limit_stops_loading_too() {
        let v = vendor();
        let (table, report) = TableBuilder::new(&v, &v)
            .with_limits(LoadLimits {
                max_files: 100,
                max_depth: 1,
            })
            .build(project());
        assert!(report.limit_hit);
        assert!(table.class(&fqn("Lib\\Base")).is_some());
        assert!(table.class(&fqn("Lib\\Root")).is_none());
        assert!(table.has_unresolved_ancestor(&fqn("App")));
    }

    #[test]
    fn a_locator_that_points_at_the_wrong_file_is_reported_not_fatal() {
        let mut v = vendor();
        v.classes.insert("lib\\missing", "vendor/used.php");
        let (_, report) = TableBuilder::new(&v, &v).build(project());
        assert_eq!(report.not_found, [fqn("Lib\\Missing")]);
    }

    #[test]
    fn loading_is_deterministic() {
        let v = vendor();
        let (a, ra) = TableBuilder::new(&v, &v).build(project());
        let (b, rb) = TableBuilder::new(&v, &v).build(project());
        assert_eq!((a, ra), (b, rb));
    }
}
