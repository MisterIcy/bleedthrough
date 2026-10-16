//! File summaries: what one file declares and refers to, a pure function of its syntax tree.

mod dump;
mod extract;
mod model;

pub use dump::{Fingerprint, dump_summary};
pub use extract::{declaration_classes, summarize};
pub(crate) use extract::{int_value, literal_type, string_value};
pub use model::{
    Adaptation, BuiltinMeta, CaseDecl, ClassDecl, ClassKind, ConstDecl, ConstantDecl, ConstantVia,
    FileSummary, FunctionDecl, FunctionRef, MemberOrigin, MethodDecl, ParamDecl, PropertyDecl,
    TraitUse, Visibility,
};
