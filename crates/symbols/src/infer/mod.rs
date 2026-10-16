//! Local type and call inference: abstract interpretation of one function body at a time over
//! the owned syntax tree, using only declared types from the symbol table.
//!
//! # Model
//!
//! - **Unit.** The unit of analysis is a function, method, closure, arrow function or the
//!   top-level code of a file (`{main}`). Closures are separate units that start from the state
//!   captured where they were created; an arrow function captures the whole state.
//! - **Declared types only.** A call's type is its callee's declared return type (native merged
//!   with docblock); the callee's body is never inferred. No declaration means `unknown`.
//! - **Flow.** The state maps variable names to types. Branches fork and join by union; a
//!   variable assigned on only one path is united with `unknown`. `try` joins conservatively over
//!   every statement boundary. Loops iterate until the loop-head state stops growing; after
//!   [`InferLimits::loop_passes`] passes the variables still changing become `mixed` and the
//!   limit-hit is counted.
//! - **Narrowing.** A fixed subset of conditions narrows variables (see the `narrow` module).
//! - **Calls.** Every call-like expression gets a [`CallResolution`] with explicit reasons.
//!
//! # Limits
//!
//! Property fetches and array elements are not tracked as flow facts, only variables; reads use
//! the declared type. Aliasing through references is not modelled beyond by-reference arguments.
//! `@var` annotations on local variables are not read. Anonymous class bodies are not analysed
//! (their expressions are `unknown`).

mod calls;
mod ctx;
mod env;
mod expr;
mod facts;
mod interp;
mod narrow;
mod ops;

use bleedthrough_syntax::{
    FunctionLike, Member, MemberKind, Modifier, Span, Stmt, StmtKind, SyntaxTree,
};

use crate::name::{Fqn, NameContext, ResolvedName};
use crate::scope::FileScopes;
use crate::special::ClassContext;
use crate::summary::ParamDecl;
use crate::table::SymbolTable;
use crate::types::{Atom, Type};
use ctx::Ctx;
use env::Env;
use interp::{Body, Unit, run_unit};

pub use facts::{
    CallResolution, DynamicKind, ExprKey, ExprTag, FunctionFacts, InferStats, Target, TargetKind,
    UnresolvedReason,
};

/// Term: exempt
///
/// The bounds on the work inference does per function.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record of limits")]
pub struct InferLimits {
    /// Passes over a loop body before its changing variables are widened to `mixed`.
    pub loop_passes: usize,
    /// Deepest statement and expression nesting followed. Callers running on a small stack should
    /// lower it.
    pub max_depth: usize,
    /// Node visits per function before the rest of the body is typed `unknown`.
    pub step_budget: usize,
    /// Most implementors listed on one call target.
    pub max_candidates: usize,
    /// Deepest closure-in-closure nesting analysed.
    pub max_closure_depth: usize,
}

impl Default for InferLimits {
    fn default() -> Self {
        Self {
            loop_passes: 3,
            max_depth: 128,
            step_budget: 200_000,
            max_candidates: 32,
            max_closure_depth: 8,
        }
    }
}

/// Where a function-like was found.
struct Site {
    name: String,
    span: Span,
    class: usize,
    /// A plain function, which has no `$this`.
    free: bool,
}

struct Job<'a> {
    unit: Unit<'a>,
    class: usize,
}

struct Collector<'a> {
    table: &'a SymbolTable,
    scopes: &'a FileScopes,
    classes: Vec<ClassContext>,
    jobs: Vec<Job<'a>>,
}

/// Analyses every function-like body and the top-level code of a file. The facts are sorted by
/// where the unit starts, then ends, then by name.
pub fn infer_file(
    tree: &SyntaxTree,
    table: &SymbolTable,
    limits: &InferLimits,
) -> Vec<FunctionFacts> {
    let scopes = FileScopes::build(tree.statements());
    let mut collector = Collector {
        table,
        scopes: &scopes,
        classes: vec![ClassContext::none()],
        jobs: Vec::new(),
    };
    let end = tree.statements().last().map_or(0, |s| s.span.end);
    collector.jobs.push(Job {
        unit: Unit {
            name: "{main}".to_string(),
            span: Span::new(0, end),
            params: &[],
            param_types: Vec::new(),
            body: Body::Stmts(tree.statements()),
            env: Env::new(),
            depth: 0,
        },
        class: 0,
    });
    collector.statements(tree.statements());
    let Collector { classes, jobs, .. } = collector;
    let mut facts: Vec<FunctionFacts> = Vec::new();
    let mut queue = jobs;
    while let Some(job) = queue.pop() {
        let ctx = Ctx {
            table,
            scopes: &scopes,
            class: &classes[job.class],
        };
        let class = job.class;
        let (done, closures) = run_unit(ctx, limits, job.unit);
        facts.push(done);
        queue.extend(closures.into_iter().map(|unit| Job { unit, class }));
    }
    facts.sort_by(|a, b| {
        (a.span.start, a.span.end, &a.name).cmp(&(b.span.start, b.span.end, &b.name))
    });
    facts
}

impl<'a> Collector<'a> {
    #[allow(
        clippy::too_many_lines,
        reason = "a dispatch over every statement kind"
    )]
    fn statements(&mut self, stmts: &'a [Stmt]) {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::Function(function) => {
                    let scope = self.scopes.at(stmt.span.start);
                    let name = scope.qualify(&function.name);
                    let declared = self.table.function(&name).map(|f| f.params.as_slice());
                    let site = Site {
                        name: name.as_str().to_string(),
                        span: stmt.span,
                        class: 0,
                        free: true,
                    };
                    self.function(function, site, declared);
                    if let Some(body) = &function.body {
                        self.statements(body);
                    }
                }
                StmtKind::ClassLike(class) => {
                    let Some(short) = &class.name else {
                        continue;
                    };
                    let scope = self.scopes.at(stmt.span.start);
                    let fqn = scope.qualify(short);
                    let parent = match class.kind {
                        bleedthrough_syntax::ClassLikeKind::Class => class
                            .extends
                            .first()
                            .and_then(|n| match scope.resolve(n, NameContext::Class, self.table) {
                                ResolvedName::Resolved(f) => Some(f),
                                _ => None,
                            }),
                        _ => None,
                    };
                    self.classes
                        .push(ClassContext::new(fqn.clone(), class.kind, parent));
                    let index = self.classes.len() - 1;
                    for member in &class.members {
                        self.member(member, &fqn, index);
                    }
                }
                StmtKind::Block(body)
                | StmtKind::Namespace { body, .. }
                | StmtKind::While { body, .. }
                | StmtKind::DoWhile { body, .. }
                | StmtKind::For { body, .. }
                | StmtKind::Foreach { body, .. } => self.statements(body),
                StmtKind::Declare {
                    body: Some(body), ..
                } => self.statements(body),
                StmtKind::If {
                    then,
                    else_ifs,
                    otherwise,
                    ..
                } => {
                    self.statements(then);
                    for branch in else_ifs {
                        self.statements(&branch.body);
                    }
                    if let Some(rest) = otherwise {
                        self.statements(rest);
                    }
                }
                StmtKind::Switch { cases, .. } => {
                    for case in cases {
                        self.statements(&case.body);
                    }
                }
                StmtKind::Try {
                    body,
                    catches,
                    finally,
                } => {
                    self.statements(body);
                    for catch in catches {
                        self.statements(&catch.body);
                    }
                    if let Some(rest) = finally {
                        self.statements(rest);
                    }
                }
                _ => {}
            }
        }
    }

    fn member(&mut self, member: &'a Member, class: &Fqn, index: usize) {
        let MemberKind::Method(method) = &member.kind else {
            return;
        };
        let declared = self.table.class(class).and_then(|c| {
            c.methods
                .iter()
                .find(|m| m.name.eq_ignore_ascii_case(&method.name))
                .map(|m| m.params.as_slice())
        });
        let name = format!("{}::{}", class.as_str(), method.name);
        let site = Site {
            name,
            span: member.span,
            class: index,
            free: false,
        };
        self.function(method, site, declared);
        if let Some(body) = &method.body {
            self.statements(body);
        }
    }

    fn function(&mut self, function: &'a FunctionLike, site: Site, declared: Option<&[ParamDecl]>) {
        let Site {
            name,
            span,
            class,
            free,
        } = site;
        let Some(body) = &function.body else {
            return;
        };
        let ctx = Ctx {
            table: self.table,
            scopes: self.scopes,
            class: &self.classes[class],
        };
        let (_, native) = ctx.signature(&function.params, function.return_type.as_ref(), span);
        let usable = declared.filter(|d| {
            d.len() == function.params.len()
                && d.iter()
                    .zip(&function.params)
                    .all(|(d, p)| d.name.trim_start_matches('$') == p.name.trim_start_matches('$'))
        });
        let param_types: Vec<Type> = match usable {
            Some(declared) => declared
                .iter()
                .zip(native)
                .map(|(d, n)| {
                    if d.ty.ty.has_unknown() {
                        n
                    } else {
                        variadic(d, &d.ty.ty)
                    }
                })
                .collect(),
            None => native,
        };
        let mut env = Env::new();
        let is_static = function.modifiers.contains(&Modifier::Static);
        if !free
            && !is_static
            && let Some(current) = self.classes[class].current()
        {
            env.set("this", Type::of(Atom::Static(current.clone())));
        }
        self.jobs.push(Job {
            unit: Unit {
                name,
                span,
                params: &function.params,
                param_types,
                body: Body::Stmts(body),
                env,
                depth: 0,
            },
            class,
        });
    }
}

fn variadic(param: &ParamDecl, ty: &Type) -> Type {
    if param.variadic {
        Type::list_of(ty.clone())
    } else {
        ty.clone()
    }
}
