//! Statements.

use bleedthrough_syntax::{
    Catch, Directive, ElseIf, Expr, Node, Span, StaticVar, Stmt, StmtKind, SwitchCase, UseItem,
    UseKind,
};
use mago_span::HasSpan;
use mago_syntax::cst::{
    Block, Declare, DeclareBody, Echo, Foreach, ForeachBody, ForeachTarget, If, IfBody, Statement,
    Switch, SwitchBody, SwitchCase as MagoCase, Terminator, Try, Use, UseItems, UseType, While,
    WhileBody,
};

use super::Cx;
use super::common::{name, text, variable_name, with_attributes};
use crate::errors::span;

fn use_kind(kind: Option<&UseType<'_>>) -> UseKind {
    match kind {
        None => UseKind::Class,
        Some(UseType::Function(_)) => UseKind::Function,
        Some(UseType::Const(_)) => UseKind::Const,
    }
}

/// A missing terminator has no reliable span; it is not part of the statement.
fn terminator_end(terminator: &Terminator<'_>) -> Option<u32> {
    match terminator {
        Terminator::Missing(_) => None,
        Terminator::Semicolon(_) | Terminator::ClosingTag(_) | Terminator::TagPair(..) => {
            Some(terminator.span().end.offset)
        }
    }
}

impl Cx<'_> {
    /// The span of a statement, excluding a missing terminator's unreliable range.
    pub(super) fn stmt_span(&self, statement: &Statement<'_>) -> Span {
        let whole = span(statement.span());
        if let Statement::Constant(c) = statement {
            // Mago's span for a constant starts at `const`, leaving its attributes outside it.
            return with_attributes(whole, &c.attribute_lists);
        }
        if let Statement::Expression(e) = statement {
            let start = span(e.expression.span());
            return match terminator_end(&e.terminator) {
                Some(end) => Span::new(start.start, end),
                None => start,
            };
        }
        whole
    }

    pub(crate) fn block(&mut self, block: &Block<'_>) -> Vec<Stmt> {
        let inside = Span::new(block.left_brace.end.offset, block.right_brace.start.offset);
        self.stmts(block.statements.iter(), inside)
    }

    /// A single statement used as a body; braces are unwrapped.
    pub(super) fn body(&mut self, statement: &Statement<'_>) -> Vec<Stmt> {
        if let Statement::Block(b) = statement {
            self.block(b)
        } else {
            vec![self.stmt(statement)]
        }
    }

    pub(crate) fn stmt(&mut self, statement: &Statement<'_>) -> Stmt {
        let at = self.fit(self.stmt_span(statement));
        if !self.enter() {
            self.leave();
            let diagnostic = Some(self.depth_error(at));
            return Node::new(at, StmtKind::Error { diagnostic });
        }
        let kind = self.bounded(at.end, |cx| cx.stmt_kind(statement));
        self.leave();
        Node::new(at, kind)
    }

    /// One arm per Mago variant. `Statement` is `#[non_exhaustive]`, so the single wildcard arm is
    /// required by the compiler: a variant added by a Mago upgrade becomes an explicit unsupported
    /// node, which the conformance suite reports.
    #[allow(
        clippy::wildcard_enum_match_arm,
        reason = "the enum is non_exhaustive; the arm is the Unsupported conversion"
    )]
    fn stmt_kind(&mut self, statement: &Statement<'_>) -> StmtKind {
        match statement {
            Statement::OpeningTag(t) => StmtKind::OpenTag {
                short: matches!(t, mago_syntax::cst::OpeningTag::Short(_)),
            },
            Statement::ClosingTag(_) => StmtKind::CloseTag,
            Statement::Inline(i) => StmtKind::InlineHtml {
                shebang: matches!(i.kind, mago_syntax::cst::InlineKind::Shebang),
                raw: text(i.value),
            },
            Statement::Namespace(n) => self.namespace(n),
            Statement::Use(u) => StmtKind::Use {
                items: use_items(u),
            },
            Statement::Class(c) => StmtKind::ClassLike(Box::new(self.class(c))),
            Statement::Interface(c) => StmtKind::ClassLike(Box::new(self.interface(c))),
            Statement::Trait(c) => StmtKind::ClassLike(Box::new(self.trait_decl(c))),
            Statement::Enum(c) => StmtKind::ClassLike(Box::new(self.enum_decl(c))),
            Statement::Function(f) => StmtKind::Function(Box::new(self.function(f))),
            Statement::Block(b) => StmtKind::Block(self.block(b)),
            Statement::Constant(c) => self.constant(c),
            Statement::If(i) => self.if_stmt(i),
            Statement::While(w) => self.while_stmt(w),
            Statement::DoWhile(d) => StmtKind::DoWhile {
                body: self.body(d.statement),
                condition: self.expr(d.condition),
            },
            Statement::For(f) => self.for_stmt(f),
            Statement::Foreach(f) => self.foreach(f),
            Statement::Switch(s) => self.switch(s),
            Statement::Try(t) => self.try_stmt(t),
            Statement::Break(b) => StmtKind::Break(b.level.map(|l| self.expr(l))),
            Statement::Continue(c) => StmtKind::Continue(c.level.map(|l| self.expr(l))),
            Statement::Return(r) => StmtKind::Return(r.value.map(|v| self.expr(v))),
            Statement::Declare(d) => self.declare(d),
            Statement::Expression(e) => StmtKind::Expr(self.expr(e.expression)),
            Statement::Echo(e) => self.echo(e, false),
            Statement::EchoTag(e) => StmtKind::Echo {
                values: e.values.iter().map(|v| self.expr(v)).collect(),
                short_tag: true,
            },
            Statement::Global(g) => {
                StmtKind::Global(g.variables.iter().map(|v| self.variable_expr(v)).collect())
            }
            Statement::Static(s) => StmtKind::StaticVars(self.static_vars(s)),
            Statement::Unset(u) => StmtKind::Unset(u.values.iter().map(|v| self.expr(v)).collect()),
            Statement::Goto(g) => StmtKind::Goto(text(g.label.value)),
            Statement::Label(l) => StmtKind::Label(text(l.name.value)),
            Statement::HaltCompiler(_) => StmtKind::HaltCompiler,
            Statement::Noop(_) => StmtKind::Noop,
            _ => StmtKind::Unsupported {
                kind: "Statement".into(),
            },
        }
    }

    fn constant(&mut self, c: &mago_syntax::cst::Constant<'_>) -> StmtKind {
        StmtKind::Const {
            attributes: self.attributes(c.attribute_lists.iter()),
            items: c
                .items
                .iter()
                .map(|i| bleedthrough_syntax::ConstItem {
                    span: self.fit(span(i.span())),
                    name: text(i.name.value),
                    value: self.expr(i.value),
                })
                .collect(),
        }
    }

    fn echo(&mut self, echo: &Echo<'_>, short_tag: bool) -> StmtKind {
        StmtKind::Echo {
            values: echo.values.iter().map(|v| self.expr(v)).collect(),
            short_tag,
        }
    }

    fn static_vars(&mut self, s: &mago_syntax::cst::Static<'_>) -> Vec<StaticVar> {
        use mago_syntax::cst::StaticItem;
        s.items
            .iter()
            .map(|item| {
                let (var, init) = match item {
                    StaticItem::Abstract(a) => (&a.variable, None),
                    StaticItem::Concrete(c) => (&c.variable, Some(self.expr(c.value))),
                };
                StaticVar {
                    span: self.fit(span(item.span())),
                    name: variable_name(var.name),
                    init,
                }
            })
            .collect()
    }

    fn namespace(&mut self, n: &mago_syntax::cst::Namespace<'_>) -> StmtKind {
        use mago_syntax::cst::NamespaceBody;
        let (braced, body) = match &n.body {
            NamespaceBody::BraceDelimited(b) => (true, self.block(b)),
            NamespaceBody::Implicit(i) => {
                let start = i.terminator.span().end.offset;
                let end = span(n.span()).end;
                (
                    false,
                    self.stmts(i.statements.iter(), Span::new(start, end)),
                )
            }
        };
        StmtKind::Namespace {
            name: n.name.as_ref().map(name),
            braced,
            body,
        }
    }

    /// Statements of a colon-delimited body, bounded by the colon and the closing keyword.
    fn colon_body<'a>(
        &mut self,
        colon: mago_span::Span,
        statements: impl Iterator<Item = &'a Statement<'a>>,
        end: mago_span::Span,
    ) -> Vec<Stmt> {
        self.stmts(statements, Span::new(colon.end.offset, end.start.offset))
    }

    fn if_stmt(&mut self, i: &If<'_>) -> StmtKind {
        let condition = self.expr(i.condition);
        match &i.body {
            IfBody::Statement(s) => StmtKind::If {
                condition,
                then: self.body(s.statement),
                else_ifs: s
                    .else_if_clauses
                    .iter()
                    .map(|c| ElseIf {
                        span: self.fit(span(c.span())),
                        condition: self.expr(c.condition),
                        body: self.body(c.statement),
                    })
                    .collect(),
                otherwise: s.else_clause.as_ref().map(|e| self.body(e.statement)),
            },
            IfBody::ColonDelimited(c) => self.if_colon(condition, c),
        }
    }

    fn if_colon(
        &mut self,
        condition: Expr,
        c: &mago_syntax::cst::IfColonDelimitedBody<'_>,
    ) -> StmtKind {
        let keywords: Vec<u32> = c
            .else_if_clauses
            .iter()
            .map(|e| e.elseif.span.start.offset)
            .chain(c.else_clause.iter().map(|e| e.r#else.span.start.offset))
            .chain(std::iter::once(c.endif.span.start.offset))
            .collect();
        let then = self.stmts(
            c.statements.iter(),
            Span::new(c.colon.end.offset, keywords[0]),
        );
        let else_ifs = c
            .else_if_clauses
            .iter()
            .enumerate()
            .map(|(index, e)| ElseIf {
                span: self.fit(span(e.span())),
                condition: self.expr(e.condition),
                body: self.stmts(
                    e.statements.iter(),
                    Span::new(e.colon.end.offset, keywords[index + 1]),
                ),
            })
            .collect();
        let otherwise = c.else_clause.as_ref().map(|e| {
            let end = keywords[keywords.len() - 1];
            self.stmts(e.statements.iter(), Span::new(e.colon.end.offset, end))
        });
        StmtKind::If {
            condition,
            then,
            else_ifs,
            otherwise,
        }
    }

    fn while_stmt(&mut self, w: &While<'_>) -> StmtKind {
        let condition = self.expr(w.condition);
        let body = match &w.body {
            WhileBody::Statement(s) => self.body(s),
            WhileBody::ColonDelimited(c) => {
                self.colon_body(c.colon, c.statements.iter(), c.end_while.span)
            }
        };
        StmtKind::While { condition, body }
    }

    fn for_stmt(&mut self, f: &mago_syntax::cst::For<'_>) -> StmtKind {
        use mago_syntax::cst::ForBody;
        let init = f.initializations.iter().map(|e| self.expr(e)).collect();
        let condition = f.conditions.iter().map(|e| self.expr(e)).collect();
        let step = f.increments.iter().map(|e| self.expr(e)).collect();
        let body = match &f.body {
            ForBody::Statement(s) => self.body(s),
            ForBody::ColonDelimited(c) => {
                self.colon_body(c.colon, c.statements.iter(), c.end_for.span)
            }
        };
        StmtKind::For {
            init,
            condition,
            step,
            body,
        }
    }

    fn foreach(&mut self, f: &Foreach<'_>) -> StmtKind {
        let subject = self.expr(f.expression);
        let (key, value) = match &f.target {
            ForeachTarget::Value(v) => (None, self.expr(v.value)),
            ForeachTarget::KeyValue(kv) => (Some(self.expr(kv.key)), self.expr(kv.value)),
        };
        let body = match &f.body {
            ForeachBody::Statement(s) => self.body(s),
            ForeachBody::ColonDelimited(c) => {
                self.colon_body(c.colon, c.statements.iter(), c.end_foreach.span)
            }
        };
        StmtKind::Foreach {
            subject,
            key,
            value,
            body,
        }
    }

    fn switch(&mut self, s: &Switch<'_>) -> StmtKind {
        let subject = self.expr(s.expression);
        let (cases, end) = match &s.body {
            SwitchBody::BraceDelimited(b) => (&b.cases, b.right_brace.start.offset),
            SwitchBody::ColonDelimited(c) => (&c.cases, c.end_switch.span.start.offset),
        };
        let all: Vec<&MagoCase<'_>> = cases.iter().collect();
        let starts: Vec<u32> = all.iter().map(|c| span(c.span()).start).collect();
        let cases = self.each_bounded(&all, &starts, end, |cx, case| cx.switch_case(case, end));
        StmtKind::Switch { subject, cases }
    }

    fn switch_case(&mut self, case: &MagoCase<'_>, end: u32) -> SwitchCase {
        use mago_syntax::cst::SwitchCaseSeparator as Sep;
        // The case ends where the next one starts (the bound set by the caller).
        let end = end.min(self.hi);
        let (test, separator, statements) = match case {
            MagoCase::Expression(e) => (Some(self.expr(e.expression)), &e.separator, &e.statements),
            MagoCase::Default(d) => (None, &d.separator, &d.statements),
        };
        let sep_end = match separator {
            Sep::Colon(s) | Sep::SemiColon(s) => s.end.offset,
        };
        SwitchCase {
            span: self.fit(span(case.span())),
            test,
            body: self.stmts(statements.iter(), Span::new(sep_end, end)),
        }
    }

    fn try_stmt(&mut self, t: &Try<'_>) -> StmtKind {
        StmtKind::Try {
            body: self.block(&t.block),
            catches: t
                .catch_clauses
                .iter()
                .map(|c| Catch {
                    span: self.fit(span(c.span())),
                    types: self.type_hint(&c.hint),
                    variable: c.variable.as_ref().map(|v| variable_name(v.name)),
                    body: self.block(&c.block),
                })
                .collect(),
            finally: t.finally_clause.as_ref().map(|f| self.block(&f.block)),
        }
    }

    fn declare(&mut self, d: &Declare<'_>) -> StmtKind {
        let directives = d
            .items
            .iter()
            .map(|i| Directive {
                span: self.fit(span(i.span())),
                name: text(i.name.value),
                value: self.expr(i.value),
            })
            .collect();
        let body = match &d.body {
            DeclareBody::Statement(s) => {
                if matches!(s, Statement::Noop(_)) {
                    None
                } else {
                    Some(self.body(s))
                }
            }
            DeclareBody::ColonDelimited(c) => {
                Some(self.colon_body(c.colon, c.statements.iter(), c.end_declare.span))
            }
        };
        StmtKind::Declare { directives, body }
    }
}

fn use_items(u: &Use<'_>) -> Vec<UseItem> {
    let single = |kind: UseKind, item: &mago_syntax::cst::UseItem<'_>, prefix: Option<&str>| {
        let written = match prefix {
            Some(p) => format!(
                "{p}\\{}",
                String::from_utf8_lossy(identifier_bytes(&item.name))
            ),
            None => String::from_utf8_lossy(identifier_bytes(&item.name)).into_owned(),
        };
        UseItem {
            span: span(item.span()),
            kind,
            name: bleedthrough_syntax::Name::from_written(span(item.name.span()), &written),
            alias: item.alias.as_ref().map(|a| text(a.identifier.value)),
        }
    };
    match &u.items {
        UseItems::Sequence(s) => s
            .items
            .iter()
            .map(|i| single(UseKind::Class, i, None))
            .collect(),
        UseItems::TypedSequence(s) => s
            .items
            .iter()
            .map(|i| single(use_kind(Some(&s.r#type)), i, None))
            .collect(),
        UseItems::TypedList(l) => {
            let prefix = String::from_utf8_lossy(identifier_bytes(&l.namespace)).into_owned();
            l.items
                .iter()
                .map(|i| single(use_kind(Some(&l.r#type)), i, Some(&prefix)))
                .collect()
        }
        UseItems::MixedList(l) => {
            let prefix = String::from_utf8_lossy(identifier_bytes(&l.namespace)).into_owned();
            l.items
                .iter()
                .map(|m| single(use_kind(m.r#type.as_ref()), &m.item, Some(&prefix)))
                .collect()
        }
    }
}

fn identifier_bytes<'a>(identifier: &'a mago_syntax::cst::Identifier<'a>) -> &'a [u8] {
    match identifier {
        mago_syntax::cst::Identifier::Local(i) => i.value,
        mago_syntax::cst::Identifier::Qualified(i) => i.value,
        mago_syntax::cst::Identifier::FullyQualified(i) => i.value,
    }
}
