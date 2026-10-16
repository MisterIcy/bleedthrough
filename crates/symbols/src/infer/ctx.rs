//! What expression typing and narrowing read but never change: the table, the file's scopes and
//! the enclosing class-like.

use bleedthrough_syntax::{Expr, ExprKind, Literal, Name, Param, Span, TypeHint};

use crate::name::{Fqn, NameContext, ResolvedName};
use crate::native::native_type;
use crate::scope::{FileScopes, Scope};
use crate::special::ClassContext;
use crate::table::SymbolTable;
use crate::types::{CallableParam, CallableSig, Type};

/// A class named in an expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ClassRes {
    /// A class known by name.
    Class(Fqn),
    /// `static`: the class the call was made on, bound at run time, here bounded by this class.
    Late(Fqn),
    /// Computed at run time.
    Dynamic,
    /// The name cannot be resolved (`self` outside a class, an error node).
    Failed,
}

impl ClassRes {
    pub fn fqn(&self) -> Option<&Fqn> {
        match self {
            Self::Class(f) | Self::Late(f) => Some(f),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Ctx<'a> {
    pub table: &'a SymbolTable,
    pub scopes: &'a FileScopes,
    pub class: &'a ClassContext,
}

impl Ctx<'_> {
    pub fn scope_at(&self, span: Span) -> &Scope {
        self.scopes.at(span.start)
    }

    /// The class a name denotes (including `self`, `static` and `parent`).
    pub fn class_of_name(&self, name: &Name) -> ClassRes {
        match self
            .class
            .resolve_name(self.scope_at(name.span), name, self.table)
        {
            ResolvedName::Resolved(f) => ClassRes::Class(f),
            ResolvedName::LateStatic(f) => ClassRes::Late(f),
            ResolvedName::Dynamic(_) => ClassRes::Dynamic,
            _ => ClassRes::Failed,
        }
    }

    /// The class an expression in class position denotes: a name or `self`/`static`/`parent`.
    /// Anything else is handled by the caller through the expression's type.
    pub fn class_of_expr(&self, expr: &Expr) -> Option<ClassRes> {
        match &expr.kind {
            ExprKind::Name(name) => Some(self.class_of_name(name)),
            ExprKind::ClassRef(kind) => Some(match self.class.resolve(*kind, expr.span) {
                ResolvedName::Resolved(f) => ClassRes::Class(f),
                ResolvedName::LateStatic(f) => ClassRes::Late(f),
                _ => ClassRes::Failed,
            }),
            ExprKind::Parenthesized(inner) => self.class_of_expr(inner),
            _ => None,
        }
    }

    /// The function a written name denotes: the namespaced one if declared, else the global one.
    /// `None` when neither is declared.
    pub fn resolve_function(&self, name: &Name) -> Option<Fqn> {
        match self
            .scope_at(name.span)
            .resolve(name, NameContext::Function, self.table)
        {
            ResolvedName::Resolved(f) => Some(f),
            ResolvedName::FunctionOrConst { namespaced, global } => {
                if self.table.function(&namespaced).is_some() {
                    Some(namespaced)
                } else {
                    Some(global)
                }
            }
            _ => None,
        }
    }

    /// The constant a written name denotes, as for [`Ctx::resolve_function`].
    pub fn resolve_constant(&self, name: &Name) -> Option<Fqn> {
        match self
            .scope_at(name.span)
            .resolve(name, NameContext::Const, self.table)
        {
            ResolvedName::Resolved(f) => Some(f),
            ResolvedName::FunctionOrConst { namespaced, global } => {
                if self.table.constant(&namespaced).is_some() {
                    Some(namespaced)
                } else {
                    Some(global)
                }
            }
            _ => None,
        }
    }

    /// The callable signature and the parameter types of a function-like.
    pub fn signature(
        &self,
        params: &[Param],
        return_type: Option<&TypeHint>,
        span: Span,
    ) -> (CallableSig, Vec<Type>) {
        let scope = self.scope_at(span);
        let mut sig_params = Vec::new();
        let mut types = Vec::new();
        for param in params {
            let declared = param
                .type_hint
                .as_ref()
                .map_or_else(Type::unknown, |h| native_type(h, scope, self.class));
            let nullable_default = matches!(
                param.default.as_ref().map(|d| &d.kind),
                Some(ExprKind::Literal(Literal::Null))
            );
            let declared = if nullable_default && param.type_hint.is_some() {
                declared.union(&Type::null())
            } else {
                declared
            };
            sig_params.push(CallableParam {
                ty: declared.clone(),
                optional: param.default.is_some() || param.variadic,
                variadic: param.variadic,
            });
            types.push(if param.variadic {
                Type::list_of(declared)
            } else {
                declared
            });
        }
        let ret = return_type.map_or_else(Type::unknown, |h| native_type(h, scope, self.class));
        (
            CallableSig {
                params: sig_params,
                ret,
            },
            types,
        )
    }
}
