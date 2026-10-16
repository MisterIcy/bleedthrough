//! Generic traversal: a borrowed reference to any node, its children in source order, and a visitor.

use crate::decl::{
    Attribute, ClassLike, ConstItem, FunctionLike, HookBody, Member, MemberKind, Param,
    PropertyHook, PropertyItem, TraitAdaptation, TraitAdaptationKind,
};
use crate::expr::{
    Arg, ArgKind, ArrayItem, ArrayItemKind, CallTarget, ClosureUse, Expr, ExprKind, MatchArm,
    MemberName,
};
use crate::node::{Name, StringPart, StringPartKind};
use crate::span::Span;
use crate::stmt::{Catch, Directive, ElseIf, StaticVar, Stmt, StmtKind, SwitchCase, UseItem};
use crate::tree::SyntaxTree;
use crate::types::{TypeHint, TypeKind};

/// Term: exempt
///
/// A borrowed reference to any node of the tree.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
#[allow(missing_docs, reason = "one variant per node type")]
pub enum NodeRef<'a> {
    Stmt(&'a Stmt),
    Expr(&'a Expr),
    Type(&'a TypeHint),
    Member(&'a Member),
    Arg(&'a Arg),
    ArrayItem(&'a ArrayItem),
    StringPart(&'a StringPart),
    MatchArm(&'a MatchArm),
    Param(&'a Param),
    Attribute(&'a Attribute),
    Name(&'a Name),
    UseItem(&'a UseItem),
    ElseIf(&'a ElseIf),
    SwitchCase(&'a SwitchCase),
    Catch(&'a Catch),
    StaticVar(&'a StaticVar),
    Directive(&'a Directive),
    ConstItem(&'a ConstItem),
    PropertyItem(&'a PropertyItem),
    Hook(&'a PropertyHook),
    ClosureUse(&'a ClosureUse),
    TraitAdaptation(&'a TraitAdaptation),
}

/// Term: exempt
///
/// Receives nodes during [`walk`].
pub trait Visitor {
    /// Called for each node before its children; return `false` to skip the children.
    fn enter(&mut self, node: NodeRef<'_>) -> bool;
}

struct FnVisitor<'f>(&'f mut dyn FnMut(NodeRef<'_>));

impl Visitor for FnVisitor<'_> {
    fn enter(&mut self, node: NodeRef<'_>) -> bool {
        (self.0)(node);
        true
    }
}

/// Visits `node` and its descendants in source order.
pub fn walk(node: NodeRef<'_>, visitor: &mut dyn Visitor) {
    if visitor.enter(node) {
        for child in node.children() {
            walk(child, visitor);
        }
    }
}

/// Calls `f` for every node of the tree in source order.
pub fn visit_tree(tree: &SyntaxTree, f: &mut dyn FnMut(NodeRef<'_>)) {
    let mut visitor = FnVisitor(f);
    for statement in tree.statements() {
        walk(NodeRef::Stmt(statement), &mut visitor);
    }
}

/// Children under construction, remembering which stretches are whole statement or member lists.
#[derive(Default)]
struct Out<'a> {
    nodes: Vec<NodeRef<'a>>,
    runs: Vec<(usize, usize)>,
}

impl<'a> Out<'a> {
    fn push(&mut self, node: NodeRef<'a>) {
        self.nodes.push(node);
    }

    fn extend(&mut self, items: impl IntoIterator<Item = NodeRef<'a>>) {
        self.nodes.extend(items);
    }

    /// Appends a whole list and records it as one run.
    fn run(&mut self, items: impl Iterator<Item = NodeRef<'a>>) {
        let start = self.nodes.len();
        self.nodes.extend(items);
        self.runs.push((start, self.nodes.len()));
    }
}

fn exprs<'a>(out: &mut Out<'a>, items: &'a [Expr]) {
    out.extend(items.iter().map(NodeRef::Expr));
}

fn stmts<'a>(out: &mut Out<'a>, items: &'a [Stmt]) {
    out.run(items.iter().map(NodeRef::Stmt));
}

fn opt_expr<'a>(out: &mut Out<'a>, item: Option<&'a Expr>) {
    out.extend(item.map(NodeRef::Expr));
}

fn args<'a>(out: &mut Out<'a>, items: &'a [Arg]) {
    out.extend(items.iter().map(NodeRef::Arg));
}

fn attributes<'a>(out: &mut Out<'a>, items: &'a [Attribute]) {
    out.extend(items.iter().map(NodeRef::Attribute));
}

fn params<'a>(out: &mut Out<'a>, items: &'a [Param]) {
    out.extend(items.iter().map(NodeRef::Param));
}

fn opt_type<'a>(out: &mut Out<'a>, item: Option<&'a TypeHint>) {
    out.extend(item.map(NodeRef::Type));
}

fn names<'a>(out: &mut Out<'a>, items: &'a [Name]) {
    out.extend(items.iter().map(NodeRef::Name));
}

fn member<'a>(out: &mut Out<'a>, name: &'a MemberName) {
    if let MemberName::Dynamic(expr) = name {
        out.push(NodeRef::Expr(expr));
    }
}

fn target<'a>(out: &mut Out<'a>, target: &'a CallTarget) {
    match target {
        CallTarget::Function(callee) => out.push(NodeRef::Expr(callee)),
        CallTarget::Method {
            object, member: m, ..
        } => {
            out.push(NodeRef::Expr(object));
            member(out, m);
        }
        CallTarget::Static { class, member: m } => {
            out.push(NodeRef::Expr(class));
            member(out, m);
        }
    }
}

fn function_like<'a>(out: &mut Out<'a>, f: &'a FunctionLike) {
    attributes(out, &f.attributes);
    params(out, &f.params);
    opt_type(out, f.return_type.as_ref());
    if let Some(body) = &f.body {
        stmts(out, body);
    }
}

fn class_like<'a>(out: &mut Out<'a>, c: &'a ClassLike) {
    attributes(out, &c.attributes);
    opt_type(out, c.backing_type.as_ref());
    names(out, &c.extends);
    names(out, &c.implements);
    out.run(c.members.iter().map(NodeRef::Member));
}

fn expr_children<'a>(out: &mut Out<'a>, kind: &'a ExprKind) {
    use ExprKind as E;
    match kind {
        E::Literal(_)
        | E::Variable(_)
        | E::ClassRef(_)
        | E::MagicConstant(_)
        | E::Error { .. }
        | E::Unsupported { .. } => {}
        E::Name(n) | E::ConstFetch(n) => out.push(NodeRef::Name(n)),
        E::Interpolated { parts, .. } => out.extend(parts.iter().map(NodeRef::StringPart)),
        E::Array { items, .. } => out.extend(items.iter().map(NodeRef::ArrayItem)),
        E::Index { array, index } => {
            out.push(NodeRef::Expr(array));
            opt_expr(out, index.as_deref());
        }
        E::Property {
            object, member: m, ..
        } => {
            out.push(NodeRef::Expr(object));
            member(out, m);
        }
        E::StaticProperty { class, name } => {
            out.push(NodeRef::Expr(class));
            out.push(NodeRef::Expr(name));
        }
        E::ClassConst { class, member: m } => {
            out.push(NodeRef::Expr(class));
            member(out, m);
        }
        E::Call { target: t, args: a } => {
            target(out, t);
            args(out, a);
        }
        E::CallableRef { target: t } => target(out, t),
        E::New { class, args: a } => {
            out.push(NodeRef::Expr(class));
            args(out, a);
        }
        E::AnonymousClass { args: a, class } => {
            args(out, a);
            class_like(out, class);
        }
        other => expr_children_more(out, other),
    }
}

fn expr_children_more<'a>(out: &mut Out<'a>, kind: &'a ExprKind) {
    use ExprKind as E;
    match kind {
        E::Unary { operand, .. } | E::Cast { operand, .. } => out.push(NodeRef::Expr(operand)),
        E::Binary { lhs, rhs, .. } => {
            out.push(NodeRef::Expr(lhs));
            out.push(NodeRef::Expr(rhs));
        }
        E::Assign {
            target: t, value, ..
        } => {
            out.push(NodeRef::Expr(t));
            out.push(NodeRef::Expr(value));
        }
        E::Conditional {
            condition,
            then,
            otherwise,
        } => {
            out.push(NodeRef::Expr(condition));
            opt_expr(out, then.as_deref());
            out.push(NodeRef::Expr(otherwise));
        }
        E::Match { subject, arms } => {
            out.push(NodeRef::Expr(subject));
            out.extend(arms.iter().map(NodeRef::MatchArm));
        }
        E::Closure {
            attributes: a,
            params: p,
            uses,
            return_type,
            body,
            ..
        } => {
            attributes(out, a);
            params(out, p);
            out.extend(uses.iter().map(NodeRef::ClosureUse));
            opt_type(out, return_type.as_deref());
            stmts(out, body);
        }
        E::ArrowFn {
            attributes: a,
            params: p,
            return_type,
            body,
            ..
        } => {
            attributes(out, a);
            params(out, p);
            opt_type(out, return_type.as_deref());
            out.push(NodeRef::Expr(body));
        }
        E::Yield { key, value } => {
            opt_expr(out, key.as_deref());
            opt_expr(out, value.as_deref());
        }
        E::YieldFrom(e)
        | E::Throw(e)
        | E::Clone(e)
        | E::Print(e)
        | E::Eval(e)
        | E::Empty(e)
        | E::Parenthesized(e)
        | E::VariableVariable(e) => out.push(NodeRef::Expr(e)),
        E::Include { path, .. } => out.push(NodeRef::Expr(path)),
        E::Exit { args: a, .. } => args(out, a),
        E::Isset(items) => exprs(out, items),
        E::Pipe { input, callable } => {
            out.push(NodeRef::Expr(input));
            out.push(NodeRef::Expr(callable));
        }
        _ => {}
    }
}

fn bodies<'a>(out: &mut Out<'a>, kind: &'a StmtKind) {
    use StmtKind as S;
    match kind {
        S::Namespace { name, body, .. } => {
            out.extend(name.iter().map(NodeRef::Name));
            stmts(out, body);
        }
        S::If {
            condition,
            then,
            else_ifs,
            otherwise,
        } => {
            out.push(NodeRef::Expr(condition));
            stmts(out, then);
            out.extend(else_ifs.iter().map(NodeRef::ElseIf));
            if let Some(body) = otherwise {
                stmts(out, body);
            }
        }
        S::While { condition, body } => {
            out.push(NodeRef::Expr(condition));
            stmts(out, body);
        }
        S::DoWhile { body, condition } => {
            stmts(out, body);
            out.push(NodeRef::Expr(condition));
        }
        S::For {
            init,
            condition,
            step,
            body,
        } => {
            exprs(out, init);
            exprs(out, condition);
            exprs(out, step);
            stmts(out, body);
        }
        S::Foreach {
            subject,
            key,
            value,
            body,
        } => {
            out.push(NodeRef::Expr(subject));
            opt_expr(out, key.as_ref());
            out.push(NodeRef::Expr(value));
            stmts(out, body);
        }
        S::Switch { subject, cases } => {
            out.push(NodeRef::Expr(subject));
            out.extend(cases.iter().map(NodeRef::SwitchCase));
        }
        S::Try {
            body,
            catches,
            finally,
        } => {
            stmts(out, body);
            out.extend(catches.iter().map(NodeRef::Catch));
            if let Some(f) = finally {
                stmts(out, f);
            }
        }
        S::Declare { directives, body } => {
            out.extend(directives.iter().map(NodeRef::Directive));
            if let Some(b) = body {
                stmts(out, b);
            }
        }
        _ => {}
    }
}

fn stmt_children<'a>(out: &mut Out<'a>, kind: &'a StmtKind) {
    use StmtKind as S;
    match kind {
        S::OpenTag { .. }
        | S::CloseTag
        | S::InlineHtml { .. }
        | S::Goto(_)
        | S::Label(_)
        | S::HaltCompiler
        | S::Noop
        | S::Error { .. }
        | S::Unsupported { .. } => {}
        S::Use { items } => out.extend(items.iter().map(NodeRef::UseItem)),
        S::ClassLike(c) => class_like(out, c),
        S::Function(f) => function_like(out, f),
        S::Const {
            attributes: a,
            items,
        } => {
            attributes(out, a);
            out.extend(items.iter().map(NodeRef::ConstItem));
        }
        S::Block(b) => stmts(out, b),
        S::Expr(e) => out.push(NodeRef::Expr(e)),
        S::Echo { values, .. } | S::Global(values) | S::Unset(values) => exprs(out, values),
        S::Return(e) | S::Break(e) | S::Continue(e) => opt_expr(out, e.as_ref()),
        S::StaticVars(vars) => out.extend(vars.iter().map(NodeRef::StaticVar)),
        other => bodies(out, other),
    }
}

fn member_children<'a>(out: &mut Out<'a>, kind: &'a MemberKind) {
    match kind {
        MemberKind::TraitUse {
            traits,
            adaptations,
        } => {
            names(out, traits);
            out.extend(adaptations.iter().map(NodeRef::TraitAdaptation));
        }
        MemberKind::Const {
            attributes: a,
            type_hint,
            items,
            ..
        } => {
            attributes(out, a);
            opt_type(out, type_hint.as_ref());
            out.extend(items.iter().map(NodeRef::ConstItem));
        }
        MemberKind::Property {
            attributes: a,
            type_hint,
            items,
            hooks,
            ..
        } => {
            attributes(out, a);
            opt_type(out, type_hint.as_ref());
            out.extend(items.iter().map(NodeRef::PropertyItem));
            out.extend(hooks.iter().map(NodeRef::Hook));
        }
        MemberKind::Method(f) => function_like(out, f),
        MemberKind::EnumCase {
            attributes: a,
            value,
            ..
        } => {
            attributes(out, a);
            opt_expr(out, value.as_ref());
        }
        MemberKind::Error { .. } | MemberKind::Unsupported { .. } => {}
    }
}

fn type_children<'a>(out: &mut Out<'a>, kind: &'a TypeKind) {
    match kind {
        TypeKind::Named(n) => out.push(NodeRef::Name(n)),
        TypeKind::Nullable(t) | TypeKind::Parenthesized(t) => out.push(NodeRef::Type(t)),
        TypeKind::Union(ts) | TypeKind::Intersection(ts) => {
            out.extend(ts.iter().map(NodeRef::Type));
        }
        TypeKind::Builtin(_) | TypeKind::Error { .. } | TypeKind::Unsupported { .. } => {}
    }
}

fn small_children<'a>(out: &mut Out<'a>, node: NodeRef<'a>) {
    match node {
        NodeRef::Arg(a) => match &a.kind {
            ArgKind::Positional(e) | ArgKind::Spread(e) | ArgKind::Named { value: e, .. } => {
                out.push(NodeRef::Expr(e));
            }
        },
        NodeRef::ArrayItem(i) => match &i.kind {
            ArrayItemKind::Value(e) | ArrayItemKind::Spread(e) => out.push(NodeRef::Expr(e)),
            ArrayItemKind::KeyValue { key, value } => {
                out.push(NodeRef::Expr(key));
                out.push(NodeRef::Expr(value));
            }
            ArrayItemKind::Missing => {}
        },
        NodeRef::StringPart(p) => {
            if let StringPartKind::Interpolation(e) = &p.kind {
                out.push(NodeRef::Expr(e));
            }
        }
        NodeRef::MatchArm(m) => {
            exprs(out, &m.conditions);
            out.push(NodeRef::Expr(&m.body));
        }
        NodeRef::Param(p) => {
            attributes(out, &p.attributes);
            opt_type(out, p.type_hint.as_ref());
            opt_expr(out, p.default.as_ref());
            out.extend(p.hooks.iter().map(NodeRef::Hook));
        }
        NodeRef::Attribute(a) => {
            out.push(NodeRef::Name(&a.name));
            args(out, &a.args);
        }
        NodeRef::UseItem(u) => out.push(NodeRef::Name(&u.name)),
        NodeRef::ElseIf(e) => {
            out.push(NodeRef::Expr(&e.condition));
            stmts(out, &e.body);
        }
        NodeRef::SwitchCase(c) => {
            opt_expr(out, c.test.as_ref());
            stmts(out, &c.body);
        }
        NodeRef::Catch(c) => {
            out.push(NodeRef::Type(&c.types));
            stmts(out, &c.body);
        }
        NodeRef::StaticVar(v) => opt_expr(out, v.init.as_ref()),
        NodeRef::Directive(d) => out.push(NodeRef::Expr(&d.value)),
        NodeRef::ConstItem(c) => out.push(NodeRef::Expr(&c.value)),
        NodeRef::PropertyItem(p) => opt_expr(out, p.default.as_ref()),
        NodeRef::Hook(h) => {
            attributes(out, &h.attributes);
            params(out, &h.params);
            match &h.body {
                HookBody::Abstract => {}
                HookBody::Block(b) => stmts(out, b),
                HookBody::Expr(e) => out.push(NodeRef::Expr(e)),
            }
        }
        NodeRef::TraitAdaptation(t) => match &t.kind {
            TraitAdaptationKind::Precedence {
                trait_name,
                instead_of,
                ..
            } => {
                out.push(NodeRef::Name(trait_name));
                names(out, instead_of);
            }
            TraitAdaptationKind::Alias { trait_name, .. } => {
                out.extend(trait_name.iter().map(NodeRef::Name));
            }
        },
        _ => {}
    }
}

impl<'a> NodeRef<'a> {
    /// The node's span.
    pub fn span(&self) -> Span {
        match self {
            Self::Stmt(n) => n.span,
            Self::Expr(n) => n.span,
            Self::Type(n) => n.span,
            Self::Member(n) => n.span,
            Self::Arg(n) => n.span,
            Self::ArrayItem(n) => n.span,
            Self::StringPart(n) => n.span,
            Self::MatchArm(n) => n.span,
            Self::Param(n) => n.span,
            Self::Attribute(n) => n.span,
            Self::Name(n) => n.span,
            Self::UseItem(n) => n.span,
            Self::ElseIf(n) => n.span,
            Self::SwitchCase(n) => n.span,
            Self::Catch(n) => n.span,
            Self::StaticVar(n) => n.span,
            Self::Directive(n) => n.span,
            Self::ConstItem(n) => n.span,
            Self::PropertyItem(n) => n.span,
            Self::Hook(n) => n.span,
            Self::ClosureUse(n) => n.span,
            Self::TraitAdaptation(n) => n.span,
        }
    }

    fn build(&self) -> Out<'a> {
        let mut out = Out::default();
        match *self {
            Self::Stmt(n) => stmt_children(&mut out, &n.kind),
            Self::Expr(n) => expr_children(&mut out, &n.kind),
            Self::Type(n) => type_children(&mut out, &n.kind),
            Self::Member(n) => member_children(&mut out, &n.kind),
            other => small_children(&mut out, other),
        }
        out
    }

    /// The direct children in source order.
    pub fn children(&self) -> Vec<NodeRef<'a>> {
        self.build().nodes
    }

    /// The direct children that form whole statement or member lists, one entry per list.
    /// Between two neighbours of one list nothing but whitespace and comments may appear.
    pub fn child_lists(&self) -> Vec<Vec<NodeRef<'a>>> {
        let out = self.build();
        out.runs
            .iter()
            .map(|&(a, b)| out.nodes[a..b].to_vec())
            .collect()
    }

    /// Whether this is an explicit `Unsupported` node.
    pub fn is_unsupported(&self) -> bool {
        matches!(
            self,
            Self::Stmt(Stmt {
                kind: StmtKind::Unsupported { .. },
                ..
            }) | Self::Expr(Expr {
                kind: ExprKind::Unsupported { .. },
                ..
            }) | Self::Type(TypeHint {
                kind: TypeKind::Unsupported { .. },
                ..
            }) | Self::Member(Member {
                kind: MemberKind::Unsupported { .. },
                ..
            })
        )
    }

    /// Whether this is an explicit `Error` node.
    pub fn is_error(&self) -> bool {
        matches!(
            self,
            Self::Stmt(Stmt {
                kind: StmtKind::Error { .. },
                ..
            }) | Self::Expr(Expr {
                kind: ExprKind::Error { .. },
                ..
            }) | Self::Type(TypeHint {
                kind: TypeKind::Error { .. },
                ..
            }) | Self::Member(Member {
                kind: MemberKind::Error { .. },
                ..
            })
        )
    }
}
