//! Per-namespace-block import scopes and name resolution.

use std::collections::BTreeMap;

use bleedthrough_syntax::{Name, NameKind, Span, Stmt, StmtKind, UseItem, UseKind};

use crate::diag::{SymbolDiagnostic, SymbolDiagnosticCode};
use crate::name::{Fqn, NameContext, NameFailure, ResolvedName};

/// Term: exempt
///
/// Whether a function or constant is declared in the code the caller knows about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Existence {
    /// A declaration exists.
    Declared,
    /// None exists and none can exist in code not yet loaded.
    Absent,
    /// None is known, but unloaded code might declare it.
    Unknown,
}

/// Term: Symbol table
///
/// The question resolution asks of the symbol table for the function and constant fallback. It is a
/// parameter, so resolving a file depends only on its tree and the answers given.
pub trait DeclaredNames {
    /// Whether `name` is a declared function.
    fn function(&self, name: &Fqn) -> Existence;
    /// Whether `name` is a declared constant.
    fn constant(&self, name: &Fqn) -> Existence;
}

/// Term: exempt
///
/// The answer used when no table is available: every namespaced candidate is undecidable.
#[derive(Debug, Clone, Copy, Default)]
#[allow(clippy::exhaustive_structs, reason = "a unit type with no state")]
pub struct NoDeclarations;

impl DeclaredNames for NoDeclarations {
    fn function(&self, _name: &Fqn) -> Existence {
        Existence::Unknown
    }

    fn constant(&self, _name: &Fqn) -> Existence {
        Existence::Unknown
    }
}

/// Term: Fully qualified name
///
/// The namespace and imports in force for the code of one namespace block. Imports never leak
/// between blocks: each block gets its own scope.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Scope {
    namespace: String,
    classes: BTreeMap<String, Fqn>,
    functions: BTreeMap<String, Fqn>,
    constants: BTreeMap<String, Fqn>,
}

impl Scope {
    /// The scope of code outside any namespace, with no imports.
    pub fn global() -> Self {
        Self::default()
    }

    /// The scope of a namespace with no imports yet.
    pub fn in_namespace(namespace: &str) -> Self {
        Self {
            namespace: namespace.to_string(),
            ..Self::default()
        }
    }

    /// Builds the scope of one block: its namespace and the `use` statements among `stmts`.
    pub fn from_statements(
        namespace: Option<&Name>,
        stmts: &[Stmt],
    ) -> (Self, Vec<SymbolDiagnostic>) {
        let mut scope = Self::in_namespace(namespace.map_or("", |n| &n.text));
        let mut diagnostics = Vec::new();
        for stmt in stmts {
            if let StmtKind::Use { items } = &stmt.kind {
                diagnostics.extend(items.iter().filter_map(|item| scope.add_import(item)));
            }
        }
        (scope, diagnostics)
    }

    /// The namespace, empty for the global one.
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// The fully qualified name a declaration of `short` in this scope has.
    pub fn qualify(&self, short: &str) -> Fqn {
        Fqn::in_namespace(&self.namespace, short)
    }

    /// Adds one import; a second import of the same alias is reported and ignored.
    pub fn add_import(&mut self, item: &UseItem) -> Option<SymbolDiagnostic> {
        let target = Fqn::new(&item.name.text);
        let alias = item
            .alias
            .as_deref()
            .unwrap_or_else(|| target.short_name())
            .to_string();
        let (map, key) = match item.kind {
            UseKind::Class => (&mut self.classes, alias.to_ascii_lowercase()),
            UseKind::Function => (&mut self.functions, alias.to_ascii_lowercase()),
            _ => (&mut self.constants, alias.clone()),
        };
        if map.contains_key(&key) {
            return Some(SymbolDiagnostic::new(
                SymbolDiagnosticCode::DuplicateImport,
                format!("the alias `{alias}` is imported twice"),
                item.span,
            ));
        }
        map.insert(key, target);
        None
    }

    /// Resolves a written name. Functions and constants without a qualifier or import consult
    /// `declared` and keep both candidates when it cannot decide.
    pub fn resolve(
        &self,
        name: &Name,
        context: NameContext,
        declared: &dyn DeclaredNames,
    ) -> ResolvedName {
        if name.text.is_empty() {
            return ResolvedName::Failed {
                reason: NameFailure::Empty,
                span: name.span,
            };
        }
        match (name.kind, context) {
            (NameKind::Unqualified, NameContext::Class) => {
                ResolvedName::Resolved(self.unqualified_class(&name.text))
            }
            (NameKind::Unqualified, _) => self.unqualified_callable(name, context, declared),
            _ => ResolvedName::Resolved(self.prefixed(name)),
        }
    }

    fn unqualified_class(&self, text: &str) -> Fqn {
        self.classes
            .get(&text.to_ascii_lowercase())
            .cloned()
            .unwrap_or_else(|| self.qualify(text))
    }

    fn unqualified_callable(
        &self,
        name: &Name,
        context: NameContext,
        declared: &dyn DeclaredNames,
    ) -> ResolvedName {
        let text: &str = &name.text;
        let imported = match context {
            NameContext::Function => self.functions.get(&text.to_ascii_lowercase()),
            _ => self.constants.get(text),
        };
        if let Some(target) = imported {
            return ResolvedName::Resolved(target.clone());
        }
        let global = Fqn::new(text);
        let reserved = ["true", "false", "null"]
            .iter()
            .any(|c| text.eq_ignore_ascii_case(c));
        if self.namespace.is_empty() || (context == NameContext::Const && reserved) {
            return ResolvedName::Resolved(global);
        }
        let namespaced = self.qualify(text);
        let existence = if context == NameContext::Function {
            declared.function(&namespaced)
        } else {
            declared.constant(&namespaced)
        };
        match existence {
            Existence::Declared => ResolvedName::Resolved(namespaced),
            Existence::Absent => ResolvedName::Resolved(global),
            Existence::Unknown => ResolvedName::FunctionOrConst { namespaced, global },
        }
    }

    /// Qualified, fully qualified and relative names; the first segment of a qualified name is
    /// matched against the class imports whatever the context.
    fn prefixed(&self, name: &Name) -> Fqn {
        let text: &str = &name.text;
        match name.kind {
            NameKind::FullyQualified => Fqn::new(text),
            NameKind::Qualified => {
                let (first, rest) = text.split_once('\\').unwrap_or((text, ""));
                match self.classes.get(&first.to_ascii_lowercase()) {
                    Some(target) => Fqn::new(&format!("{target}\\{rest}")),
                    None => self.qualify(text),
                }
            }
            _ => self.qualify(text),
        }
    }
}

/// Term: Fully qualified name
///
/// The scopes of one file: the global scope for code outside namespace blocks and one scope per
/// namespace block, found by source offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileScopes {
    global: Scope,
    blocks: Vec<(Span, Scope)>,
    diagnostics: Vec<SymbolDiagnostic>,
}

impl FileScopes {
    /// Builds the scopes from the top-level statements of a file.
    pub fn build(stmts: &[Stmt]) -> Self {
        let (global, mut diagnostics) = Scope::from_statements(None, stmts);
        let mut blocks = Vec::new();
        for stmt in stmts {
            if let StmtKind::Namespace { name, body, .. } = &stmt.kind {
                let (scope, found) = Scope::from_statements(name.as_ref(), body);
                diagnostics.extend(found);
                blocks.push((stmt.span, scope));
            }
        }
        diagnostics.sort();
        Self {
            global,
            blocks,
            diagnostics,
        }
    }

    /// The scope in force at a byte offset.
    pub fn at(&self, offset: u32) -> &Scope {
        self.blocks
            .iter()
            .find(|(span, _)| span.start <= offset && offset < span.end.max(span.start + 1))
            .map_or(&self.global, |(_, scope)| scope)
    }

    /// Duplicate-import diagnostics, sorted.
    pub fn diagnostics(&self) -> &[SymbolDiagnostic] {
        &self.diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bleedthrough_syntax::Node;

    fn name(written: &str) -> Name {
        Name::from_written(Span::new(0, 1), written)
    }

    fn import(kind: UseKind, written: &str, alias: Option<&str>) -> UseItem {
        UseItem {
            span: Span::new(0, 1),
            kind,
            name: name(written),
            alias: alias.map(Into::into),
        }
    }

    fn scope(namespace: &str, items: &[UseItem]) -> Scope {
        let mut scope = Scope::in_namespace(namespace);
        for item in items {
            scope.add_import(item);
        }
        scope
    }

    fn class(scope: &Scope, written: &str) -> String {
        match scope.resolve(&name(written), NameContext::Class, &NoDeclarations) {
            ResolvedName::Resolved(fqn) => fqn.to_string(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn import_alias() {
        let s = scope(
            "App",
            &[import(UseKind::Class, "App\\Db\\Connection", Some("Conn"))],
        );
        assert_eq!(class(&s, "Conn"), "App\\Db\\Connection");
        assert_eq!(class(&s, "conn"), "App\\Db\\Connection");
    }

    #[test]
    fn current_namespace_prefix() {
        let s = scope("App\\Http", &[]);
        assert_eq!(class(&s, "Request"), "App\\Http\\Request");
        assert_eq!(class(&s, "namespace\\Sub\\X"), "App\\Http\\Sub\\X");
        assert_eq!(class(&s, "\\Other\\Y"), "Other\\Y");
        assert_eq!(class(&scope("", &[]), "Foo"), "Foo");
    }

    #[test]
    fn qualified_name_uses_the_first_segment() {
        let s = scope("", &[import(UseKind::Class, "App\\Db", Some("D"))]);
        assert_eq!(class(&s, "D\\Connection"), "App\\Db\\Connection");
        assert_eq!(class(&scope("N", &[]), "D\\Connection"), "N\\D\\Connection");
    }

    #[test]
    fn default_alias_is_the_last_segment() {
        let s = scope("N", &[import(UseKind::Class, "Vendor\\Pkg\\Thing", None)]);
        assert_eq!(class(&s, "Thing"), "Vendor\\Pkg\\Thing");
    }

    #[test]
    fn a_repeated_alias_is_reported_and_the_first_wins() {
        let mut s = Scope::global();
        assert!(
            s.add_import(&import(UseKind::Class, "A\\X", None))
                .is_none()
        );
        let d = s.add_import(&import(UseKind::Class, "B\\X", None)).unwrap();
        assert_eq!(d.code, SymbolDiagnosticCode::DuplicateImport);
        assert_eq!(class(&s, "X"), "A\\X");
    }

    struct Declares(&'static str, Existence);

    impl DeclaredNames for Declares {
        fn function(&self, n: &Fqn) -> Existence {
            if n.as_str() == self.0 {
                Existence::Declared
            } else {
                self.1
            }
        }

        fn constant(&self, n: &Fqn) -> Existence {
            self.function(n)
        }
    }

    #[test]
    fn functions_fall_back_to_the_global_namespace() {
        let s = scope("App", &[]);
        let n = name("strlen");
        let shadow = Declares("App\\strlen", Existence::Absent);
        let r = s.resolve(&n, NameContext::Function, &shadow);
        assert_eq!(r, ResolvedName::Resolved(Fqn::new("App\\strlen")));
        let none = Declares("", Existence::Absent);
        let r = s.resolve(&n, NameContext::Function, &none);
        assert_eq!(r, ResolvedName::Resolved(Fqn::new("strlen")));
        let maybe = Declares("", Existence::Unknown);
        let r = s.resolve(&n, NameContext::Function, &maybe);
        assert_eq!(
            r,
            ResolvedName::FunctionOrConst {
                namespaced: Fqn::new("App\\strlen"),
                global: Fqn::new("strlen")
            }
        );
    }

    #[test]
    fn function_and_const_imports_are_separate_from_classes() {
        let s = scope(
            "App",
            &[
                import(UseKind::Function, "Util\\helper", None),
                import(UseKind::Const, "Util\\LIMIT", None),
                import(UseKind::Class, "Util\\Helper", None),
            ],
        );
        let f = s.resolve(&name("helper"), NameContext::Function, &NoDeclarations);
        assert_eq!(f, ResolvedName::Resolved(Fqn::new("Util\\helper")));
        let c = s.resolve(&name("LIMIT"), NameContext::Const, &NoDeclarations);
        assert_eq!(c, ResolvedName::Resolved(Fqn::new("Util\\LIMIT")));
        let lower = s.resolve(&name("limit"), NameContext::Const, &NoDeclarations);
        assert!(matches!(lower, ResolvedName::FunctionOrConst { .. }));
        assert_eq!(class(&s, "Helper"), "Util\\Helper");
    }

    #[test]
    fn true_false_null_are_global() {
        let s = scope("App", &[]);
        let r = s.resolve(&name("TRUE"), NameContext::Const, &NoDeclarations);
        assert_eq!(r, ResolvedName::Resolved(Fqn::new("TRUE")));
    }

    #[test]
    fn resolving_twice_gives_identical_results() {
        let s = scope("App", &[import(UseKind::Class, "Lib\\Thing", None)]);
        let n = name("strlen");
        let first = s.resolve(&n, NameContext::Function, &NoDeclarations);
        assert_eq!(first, s.resolve(&n, NameContext::Function, &NoDeclarations));
        assert_eq!(class(&s, "Thing"), class(&s, "Thing"));
    }

    #[test]
    fn an_empty_name_fails() {
        let r = Scope::global().resolve(&name(""), NameContext::Class, &NoDeclarations);
        assert!(matches!(r, ResolvedName::Failed { .. }));
    }

    fn stmt(start: u32, end: u32, kind: StmtKind) -> Stmt {
        Node::new(Span::new(start, end), kind)
    }

    #[test]
    fn imports_do_not_leak_between_namespace_blocks() {
        let use_a = stmt(
            10,
            20,
            StmtKind::Use {
                items: vec![import(UseKind::Class, "Lib\\Thing", None)],
            },
        );
        let blocks = vec![
            stmt(
                0,
                50,
                StmtKind::Namespace {
                    name: Some(name("A")),
                    braced: true,
                    body: vec![use_a],
                },
            ),
            stmt(
                50,
                100,
                StmtKind::Namespace {
                    name: Some(name("B")),
                    braced: true,
                    body: vec![],
                },
            ),
        ];
        let scopes = FileScopes::build(&blocks);
        assert_eq!(class(scopes.at(15), "Thing"), "Lib\\Thing");
        assert_eq!(class(scopes.at(60), "Thing"), "B\\Thing");
        assert_eq!(scopes.at(15).namespace(), "A");
        assert_eq!(scopes.at(60).namespace(), "B");
    }

    #[test]
    fn group_use_items_arrive_joined() {
        let s = scope(
            "",
            &[
                import(UseKind::Class, "App\\Models\\User", None),
                import(UseKind::Class, "App\\Util\\Str", Some("S")),
            ],
        );
        assert_eq!(class(&s, "User"), "App\\Models\\User");
        assert_eq!(class(&s, "S"), "App\\Util\\Str");
    }
}
