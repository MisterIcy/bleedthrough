//! **Layer:** Domain.
//!
//! **Responsibility:** names, the symbol table, types and local inference derived from the syntax
//! tree: fully qualified names, per-file summaries, the project-wide table with its hierarchy,
//! the type model with its docblock parser, and intra-procedural call resolution.
//!
//! **Permitted workspace dependencies:** `bleedthrough-syntax`, `bleedthrough-builtins`.
//!
//! The crate does no I/O: the file locator and file source it needs are ports implemented in
//! infrastructure, and the built-in data arrives through the model the builtins crate owns.

#![warn(clippy::exhaustive_enums, clippy::exhaustive_structs)]

mod builtin;
mod diag;
mod docblock;
mod infer;
mod name;
mod native;
mod scope;
mod special;
mod summary;
mod table;
mod types;

pub use builtin::{Availability, BUILTIN_PATH, Builtins};
pub use diag::{SymbolDiagnostic, SymbolDiagnosticCode};
pub use docblock::{
    DocBlock, DocMethod, DocMethodParam, DocParam, DocProperty, DocReturn, DocTemplate, DocVar,
    PropertyAccess, TypeContext, TypeParse, parse_docblock, parse_type,
};
pub use infer::{
    CallResolution, DynamicKind, ExprKey, ExprTag, FunctionFacts, InferLimits, InferStats, Target,
    TargetKind, UnresolvedReason, infer_file,
};
pub use name::{Fqn, NameContext, NameFailure, ResolvedName};
pub use native::{EffectiveType, TypeSource, merge_types, native_type};
pub use scope::{DeclaredNames, Existence, FileScopes, NoDeclarations, Scope};
pub use special::ClassContext;
pub use summary::{
    Adaptation, BuiltinMeta, CaseDecl, ClassDecl, ClassKind, ConstDecl, ConstantDecl, ConstantVia,
    FileSummary, Fingerprint, FunctionDecl, FunctionRef, MemberOrigin, MethodDecl, ParamDecl,
    PropertyDecl, TraitUse, Visibility, dump_summary, summarize,
};
pub use table::{
    Candidate, ClassConstant, ClassLocator, ClassSource, DeclLocation, LoadLimits, LoadReport,
    Lookup, Magic, Subtyping, SymbolTable, TableBuilder, TableDiagnostic, TableDiagnosticCode,
    Unavailable, UnknownReport,
};
pub use types::{
    Atom, CallableParam, CallableSig, DEFAULT_LITERAL_CAP, DEFAULT_UNION_CAP, NamedObject,
    Relation, ShapeEntry, ShapeKey, Type,
};
