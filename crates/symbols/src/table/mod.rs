//! The project-wide table of declarations: indexes, hierarchy, member lookup and subtype queries.
//! A table is immutable once built and a pure function of the summaries it was built from.

mod builder;
mod hierarchy;
mod lookup;
mod query;
mod report;

use std::collections::BTreeMap;

use bleedthrough_syntax::Span;

use crate::name::Fqn;
use crate::scope::{DeclaredNames, Existence};
use crate::summary::{ClassDecl, ConstantDecl, FileSummary, FunctionDecl};
use hierarchy::Links;

pub use builder::{ClassLocator, ClassSource, LoadLimits, LoadReport, TableBuilder};
pub use lookup::{Candidate, ClassConstant, Lookup, Magic};
pub use query::Subtyping;
pub use report::{Unavailable, UnknownReport};

/// Term: Symbol table
///
/// What a table diagnostic is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum TableDiagnosticCode {
    /// Two files declare the same class-like.
    DuplicateClass,
    /// Two files declare the same function.
    DuplicateFunction,
    /// Two files declare the same constant.
    DuplicateConstant,
    /// Classes inherit from each other in a loop; one edge was cut.
    InheritanceCycle,
    /// A declaration has the name of a built-in symbol; the built-in one is used.
    ShadowsBuiltin,
}

impl TableDiagnosticCode {
    /// The stable snake-case spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DuplicateClass => "duplicate_class",
            Self::DuplicateFunction => "duplicate_function",
            Self::DuplicateConstant => "duplicate_constant",
            Self::InheritanceCycle => "inheritance_cycle",
            Self::ShadowsBuiltin => "shadows_builtin",
        }
    }
}

/// Term: Symbol table
///
/// Where a declaration is.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct DeclLocation {
    /// The file's path.
    pub path: Box<str>,
    /// The declaration's span.
    pub span: Span,
}

/// Term: Symbol table
///
/// A problem found while building the table, naming every declaration involved.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct TableDiagnostic {
    /// What kind of problem.
    pub code: TableDiagnosticCode,
    /// A readable description.
    pub message: String,
    /// The declarations involved, the one lookup returns first.
    pub locations: Vec<DeclLocation>,
}

/// One declaration under an index key: slot, whether built in, location and display name.
type Entry = (Slot, bool, DeclLocation, String);

/// Where a declaration sits: file index and index within the file's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Slot {
    file: usize,
    index: usize,
}

/// Term: Symbol table
///
/// Every declaration of a project, its dependencies and PHP itself, with the inheritance links
/// between them. Duplicate names are all kept; lookup returns the built-in declaration if there is
/// one and otherwise the first by file path.
#[derive(Debug, Clone, PartialEq)]
pub struct SymbolTable {
    files: Vec<FileSummary>,
    classes: BTreeMap<Box<str>, Slot>,
    functions: BTreeMap<Box<str>, Slot>,
    constants: BTreeMap<Box<str>, Slot>,
    links: BTreeMap<Box<str>, Links>,
    children: BTreeMap<Box<str>, Vec<Box<str>>>,
    diagnostics: Vec<TableDiagnostic>,
}

impl SymbolTable {
    /// Builds a table. The result does not depend on the order of `summaries`.
    pub fn build(mut summaries: Vec<FileSummary>) -> Self {
        summaries.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then_with(|| a.fingerprint().cmp(&b.fingerprint()))
        });
        summaries.dedup();
        let mut diagnostics = Vec::new();
        let classes = index_classes(&summaries, &mut diagnostics);
        let functions = index_functions(&summaries, &mut diagnostics);
        let constants = index_constants(&summaries, &mut diagnostics);
        let mut table = Self {
            files: summaries,
            classes,
            functions,
            constants,
            links: BTreeMap::new(),
            children: BTreeMap::new(),
            diagnostics,
        };
        hierarchy::link(&mut table);
        table.diagnostics.sort();
        table
    }

    /// The summaries the table was built from, sorted by path.
    pub fn files(&self) -> &[FileSummary] {
        &self.files
    }

    /// Problems found while building, sorted.
    pub fn diagnostics(&self) -> &[TableDiagnostic] {
        &self.diagnostics
    }

    /// A class, interface, trait or enum by name (case-insensitive).
    pub fn class(&self, name: &Fqn) -> Option<&ClassDecl> {
        self.classes.get(name.key()).map(|s| self.class_at(*s))
    }

    /// A function by name (case-insensitive).
    pub fn function(&self, name: &Fqn) -> Option<&FunctionDecl> {
        let slot = self.functions.get(name.key())?;
        self.files.get(slot.file)?.functions.get(slot.index)
    }

    /// A global constant by name (namespace case-insensitive, the constant itself case-sensitive).
    pub fn constant(&self, name: &Fqn) -> Option<&ConstantDecl> {
        let slot = self.constants.get(name.constant_key().as_str())?;
        self.files.get(slot.file)?.constants.get(slot.index)
    }

    /// Every class-like in name order (the winning declaration of each name).
    pub fn classes(&self) -> impl Iterator<Item = &ClassDecl> {
        self.classes.values().map(|s| self.class_at(*s))
    }

    /// Every function in name order.
    pub fn functions(&self) -> impl Iterator<Item = &FunctionDecl> {
        self.functions
            .values()
            .filter_map(|s| self.files.get(s.file)?.functions.get(s.index))
    }

    /// Every constant in name order.
    pub fn constants(&self) -> impl Iterator<Item = &ConstantDecl> {
        self.constants
            .values()
            .filter_map(|s| self.files.get(s.file)?.constants.get(s.index))
    }

    fn class_at(&self, slot: Slot) -> &ClassDecl {
        &self.files[slot.file].classes[slot.index]
    }

    /// The path of the file that declares the winning declaration of `name`.
    pub fn class_path(&self, name: &Fqn) -> Option<&str> {
        self.classes
            .get(name.key())
            .map(|s| &*self.files[s.file].path)
    }
}

impl DeclaredNames for SymbolTable {
    fn function(&self, name: &Fqn) -> Existence {
        if SymbolTable::function(self, name).is_some() {
            Existence::Declared
        } else {
            Existence::Unknown
        }
    }

    fn constant(&self, name: &Fqn) -> Existence {
        if SymbolTable::constant(self, name).is_some() {
            Existence::Declared
        } else {
            Existence::Unknown
        }
    }
}

fn location(file: &FileSummary, span: Span) -> DeclLocation {
    DeclLocation {
        path: file.path.clone(),
        span,
    }
}

/// Indexes declarations by key. `rank` orders candidates of one key: built-in first, then file order.
fn index<'a, I>(
    items: I,
    what: TableDiagnosticCode,
    noun: &str,
    diagnostics: &mut Vec<TableDiagnostic>,
) -> BTreeMap<Box<str>, Slot>
where
    I: Iterator<Item = (Box<str>, Slot, bool, &'a FileSummary, Span, String)>,
{
    let mut groups: BTreeMap<Box<str>, Vec<Entry>> = BTreeMap::new();
    for (key, slot, builtin, file, span, display) in items {
        groups
            .entry(key)
            .or_default()
            .push((slot, builtin, location(file, span), display));
    }
    let mut out = BTreeMap::new();
    for (key, mut group) in groups {
        // Stable: file order is already the tie-break.
        group.sort_by_key(|(_, builtin, _, _)| !*builtin);
        if group.len() > 1 {
            let places: Vec<String> = group
                .iter()
                .map(|(_, _, l, _)| format!("{}:{}", l.path, l.span.start))
                .collect();
            let shadows = group[0].1 && group.iter().any(|g| !g.1);
            let (code, message) = if shadows {
                (
                    TableDiagnosticCode::ShadowsBuiltin,
                    format!(
                        "{noun} `{}` is also built in; the built-in declaration is used: {}",
                        group[0].3,
                        places.join(", ")
                    ),
                )
            } else {
                (
                    what,
                    format!(
                        "{noun} `{}` is declared more than once: {}",
                        group[0].3,
                        places.join(", ")
                    ),
                )
            };
            diagnostics.push(TableDiagnostic {
                code,
                message,
                locations: group.iter().map(|(_, _, l, _)| l.clone()).collect(),
            });
        }
        out.insert(key, group[0].0);
    }
    out
}

fn index_classes(
    files: &[FileSummary],
    diagnostics: &mut Vec<TableDiagnostic>,
) -> BTreeMap<Box<str>, Slot> {
    let items = files.iter().enumerate().flat_map(|(file, f)| {
        f.classes.iter().enumerate().map(move |(index, c)| {
            (
                Box::<str>::from(c.name.key()),
                Slot { file, index },
                c.builtin.is_some(),
                f,
                c.span,
                c.name.to_string(),
            )
        })
    });
    index(
        items,
        TableDiagnosticCode::DuplicateClass,
        "class",
        diagnostics,
    )
}

fn index_functions(
    files: &[FileSummary],
    diagnostics: &mut Vec<TableDiagnostic>,
) -> BTreeMap<Box<str>, Slot> {
    let items = files.iter().enumerate().flat_map(|(file, f)| {
        f.functions.iter().enumerate().map(move |(index, c)| {
            (
                Box::<str>::from(c.name.key()),
                Slot { file, index },
                c.builtin.is_some(),
                f,
                c.span,
                c.name.to_string(),
            )
        })
    });
    index(
        items,
        TableDiagnosticCode::DuplicateFunction,
        "function",
        diagnostics,
    )
}

fn index_constants(
    files: &[FileSummary],
    diagnostics: &mut Vec<TableDiagnostic>,
) -> BTreeMap<Box<str>, Slot> {
    let items = files.iter().enumerate().flat_map(|(file, f)| {
        f.constants.iter().enumerate().map(move |(index, c)| {
            (
                Box::<str>::from(c.name.constant_key()),
                Slot { file, index },
                c.builtin.is_some(),
                f,
                c.span,
                c.name.to_string(),
            )
        })
    });
    index(
        items,
        TableDiagnosticCode::DuplicateConstant,
        "constant",
        diagnostics,
    )
}
