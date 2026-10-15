//! **Layer:** Domain.
//!
//! **Responsibility:** the owned PHP syntax tree, source spans, and the parser-backend port that adapters implement.
//!
//! **Permitted workspace dependencies:** none.
//!
//! No type of any parser backend appears here. Every byte of a file is accounted for in the tree:
//! whitespace, a comment, a node, or an explicit `Error` or `Unsupported` node.

#![warn(clippy::exhaustive_enums, clippy::exhaustive_structs)]

mod decl;
mod describe;
mod diagnostic;
mod expr;
mod invariants;
mod node;
mod port;
mod span;
mod stmt;
mod tree;
mod trivia;
mod types;
mod walk;

pub use decl::{
    Attribute, ClassLike, ClassLikeKind, ConstItem, FunctionLike, HookBody, Member, MemberKind,
    Modifier, Param, PropertyHook, PropertyItem, TraitAdaptation, TraitAdaptationKind,
};
pub use diagnostic::{Diagnostic, DiagnosticCode, ParseQuality};
pub use expr::{
    Arg, ArgKind, ArrayItem, ArrayItemKind, ArrayStyle, AssignOp, BinaryOp, CallTarget, CastKind,
    ClassRefKind, ClosureUse, Expr, ExprKind, IncludeKind, MagicConstantKind, MatchArm, MemberName,
    UnaryOp,
};
pub use invariants::{InvariantKind, Violation, check_invariants};
pub use node::{Literal, Name, NameKind, Node, QuoteKind, StringKind, StringPart, StringPartKind};
pub use port::{BackendInfo, ParseOptions, ParsedFile, ParserBackend, RecoveryMode};
pub use span::{LineColumn, LineIndex, Span};
pub use stmt::{Catch, Directive, ElseIf, StaticVar, Stmt, StmtKind, SwitchCase, UseItem, UseKind};
pub use tree::SyntaxTree;
pub use trivia::{Comment, CommentKind, docblock_before};
pub use types::{BuiltinType, TypeHint, TypeKind};
pub use walk::{NodeRef, Visitor, visit_tree, walk};
