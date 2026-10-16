//! Built-in symbols in the table, from the real built-in data and from tiny in-memory data.

use bleedthrough_symbols_suite::builtins::{InMemorySource, PhpVersion};
use bleedthrough_symbols_suite::symbols::{
    Availability, Builtins, Fqn, SymbolTable, TableDiagnosticCode, TypeSource,
};
use bleedthrough_symbols_suite::{summarize_source, workspace_root};
use bleedthrough_syntax_mago::MagoBackend;

fn real(version: &str) -> Builtins {
    let text = std::fs::read_to_string(workspace_root().join("stubs/builtin.data")).unwrap();
    let source = InMemorySource::from_text(&text).unwrap();
    Builtins::load(&source, PhpVersion::parse(version).unwrap()).unwrap()
}

fn fqn(name: &str) -> Fqn {
    Fqn::new(name)
}

fn with_project(
    builtins: &Builtins,
    source: &str,
) -> (SymbolTable, Vec<bleedthrough_symbols_suite::FileSummary>) {
    let project = vec![summarize_source(&MagoBackend::new(), "app.php", source)];
    let mut all = vec![builtins.summary()];
    all.extend(project.clone());
    (SymbolTable::build(all), project)
}

#[test]
fn the_whole_data_converts_without_problems() {
    let summary = real("8.2").summary();
    assert!(summary.classes.len() > 100 && summary.functions.len() > 1000);
    let bad: Vec<_> = summary
        .diagnostics
        .iter()
        .take(10)
        .map(|d| &d.message)
        .collect();
    assert!(
        summary.diagnostics.is_empty(),
        "{} problems, e.g. {bad:?}",
        summary.diagnostics.len()
    );
}

#[test]
fn a_tentative_return_type_is_the_return_type() {
    let t = SymbolTable::build(vec![real("8.2").summary()]);
    let count = t.method(&fqn("ArrayIterator"), "count", None);
    let ret = &count.accessible().unwrap().member.ret;
    assert_eq!(ret.ty.to_string(), "int");
    assert_eq!(
        t.class(&fqn("arrayiterator"))
            .unwrap()
            .builtin
            .as_ref()
            .unwrap()
            .extension
            .as_ref(),
        "spl"
    );
}

#[test]
fn a_docblock_refines_a_builtin_function() {
    let t = SymbolTable::build(vec![real("8.2").summary()]);
    let ret = &t.function(&fqn("STR_REPLACE")).unwrap().ret;
    assert_eq!(ret.source, TypeSource::Docblock);
    assert_eq!(ret.ty.to_string(), "string|array<array-key, string>");
    let strlen = &t.function(&fqn("strlen")).unwrap();
    assert_eq!(strlen.ret.ty.to_string(), "int");
    assert_eq!(strlen.params[0].ty.source, TypeSource::Native);
}

#[test]
fn availability_depends_on_the_modelled_version() {
    let old = real("8.2");
    assert_eq!(old.function(&fqn("array_find")), Availability::NotInVersion);
    assert!(matches!(
        old.function(&fqn("mb_strlen")),
        Availability::Available { .. }
    ));
    assert_eq!(
        old.function(&fqn("no_such_function")),
        Availability::Unknown
    );
    let new = real("8.4");
    assert!(matches!(
        new.function(&fqn("array_find")),
        Availability::Available { .. }
    ));
}

#[test]
fn guards_and_unbundled_extensions_are_surfaced() {
    let new = real("8.4");
    assert_eq!(
        new.function(&fqn("imap_open")),
        Availability::Unbundled {
            extension: "imap".into()
        }
    );
    let Availability::Available { guards, .. } = real("8.3").constant(&fqn("ABDAY_1")) else {
        panic!("ABDAY_1 exists in 8.3");
    };
    assert!(
        guards.iter().any(|g| g.contains("HAVE_NL_LANGINFO")),
        "{guards:?}"
    );
}

#[test]
fn a_project_class_cannot_shadow_a_builtin() {
    let builtins = real("8.2");
    let (t, _) = with_project(
        &builtins,
        "<?php\nclass Countable { public function mine() {} }\n",
    );
    let d = t
        .diagnostics()
        .iter()
        .find(|d| d.code == TableDiagnosticCode::ShadowsBuiltin)
        .expect("a shadowing diagnostic");
    let paths: Vec<_> = d.locations.iter().map(|l| &*l.path).collect();
    assert_eq!(paths, ["<builtin>", "app.php"]);
    let class = t.class(&fqn("Countable")).unwrap();
    assert!(class.builtin.is_some() && class.methods.iter().all(|m| &*m.name != "mine"));
}

#[test]
fn unknown_and_unavailable_references_are_reported() {
    let builtins = real("8.2");
    let (t, project) = with_project(
        &builtins,
        "<?php\nnamespace App;\nstrlen('x'); nonexistent_fn(); array_find([], fn() => true); new \\Missing\\Thing(); new \\ArrayIterator([]);\n",
    );
    let report = t.unknown_references(&builtins, &project);
    assert_eq!(report.functions, [fqn("nonexistent_fn")]);
    assert_eq!(report.classes, [fqn("Missing\\Thing")]);
    assert_eq!(report.unavailable.len(), 1);
    assert_eq!(report.unavailable[0].name, fqn("array_find"));
}

#[test]
fn in_memory_data_needs_no_files() {
    let text = "bleedthrough-builtins 1
function	foo_bar	8.0-8.5		demo							int		
interface	Demo	8.0-8.5		core							
";
    let source = InMemorySource::from_text(text).unwrap();
    let builtins = Builtins::load(&source, PhpVersion::parse("8.3").unwrap()).unwrap();
    let t = SymbolTable::build(vec![builtins.summary()]);
    assert!(t.function(&fqn("foo_bar")).is_some() && t.class(&fqn("demo")).is_some());
}
