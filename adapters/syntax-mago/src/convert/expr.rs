//! Expressions.

use bleedthrough_syntax::{
    ArrayItem, ArrayItemKind, ArrayStyle, AssignOp, BinaryOp, CallTarget, CastKind, ClassRefKind,
    Expr, ExprKind, IncludeKind, Literal, MagicConstantKind, MatchArm, MemberName, Node,
    StringKind, UnaryOp,
};
use mago_span::HasSpan;
use mago_syntax::cst::{
    Access, ArrayElement, AssignmentOperator, BinaryOperator, Call, ClassLikeConstantSelector,
    ClassLikeMemberSelector, CompositeString, Construct, Expression, Literal as MagoLiteral,
    MagicConstant, MatchArm as MagoMatchArm, PartialApplication, UnaryPostfixOperator,
    UnaryPrefixOperator, Variable, Yield,
};

use super::Cx;
use super::common::{name, string_literal, text, variable_name};
use crate::errors::span;

fn binary_op(op: &BinaryOperator<'_>) -> BinaryOp {
    use BinaryOperator as O;
    match op {
        O::Addition(_) => BinaryOp::Add,
        O::Subtraction(_) => BinaryOp::Sub,
        O::Multiplication(_) => BinaryOp::Mul,
        O::Division(_) => BinaryOp::Div,
        O::Modulo(_) => BinaryOp::Mod,
        O::Exponentiation(_) => BinaryOp::Pow,
        O::BitwiseAnd(_) => BinaryOp::BitAnd,
        O::BitwiseOr(_) => BinaryOp::BitOr,
        O::BitwiseXor(_) => BinaryOp::BitXor,
        O::LeftShift(_) => BinaryOp::Shl,
        O::RightShift(_) => BinaryOp::Shr,
        O::NullCoalesce(_) => BinaryOp::Coalesce,
        O::Equal(_) => BinaryOp::Eq,
        O::NotEqual(_) => BinaryOp::NotEq,
        O::Identical(_) => BinaryOp::Identical,
        O::NotIdentical(_) => BinaryOp::NotIdentical,
        O::AngledNotEqual(_) => BinaryOp::AngledNotEq,
        O::LessThan(_) => BinaryOp::Lt,
        O::LessThanOrEqual(_) => BinaryOp::Le,
        O::GreaterThan(_) => BinaryOp::Gt,
        O::GreaterThanOrEqual(_) => BinaryOp::Ge,
        O::Spaceship(_) => BinaryOp::Spaceship,
        O::StringConcat(_) => BinaryOp::Concat,
        O::Instanceof(_) => BinaryOp::Instanceof,
        O::And(_) => BinaryOp::And,
        O::Or(_) => BinaryOp::Or,
        O::LowAnd(_) => BinaryOp::LowAnd,
        O::LowOr(_) => BinaryOp::LowOr,
        O::LowXor(_) => BinaryOp::LowXor,
    }
}

fn assign_op(op: &AssignmentOperator) -> AssignOp {
    use AssignmentOperator as O;
    match op {
        O::Assign(_) => AssignOp::Assign,
        O::Addition(_) => AssignOp::Add,
        O::Subtraction(_) => AssignOp::Sub,
        O::Multiplication(_) => AssignOp::Mul,
        O::Division(_) => AssignOp::Div,
        O::Modulo(_) => AssignOp::Mod,
        O::Exponentiation(_) => AssignOp::Pow,
        O::Concat(_) => AssignOp::Concat,
        O::BitwiseAnd(_) => AssignOp::BitAnd,
        O::BitwiseOr(_) => AssignOp::BitOr,
        O::BitwiseXor(_) => AssignOp::BitXor,
        O::LeftShift(_) => AssignOp::Shl,
        O::RightShift(_) => AssignOp::Shr,
        O::Coalesce(_) => AssignOp::Coalesce,
    }
}

/// A prefix operator is either a cast or a plain unary operator.
enum Prefix {
    Cast(CastKind),
    Unary(UnaryOp),
}

fn prefix(op: &UnaryPrefixOperator<'_>) -> Prefix {
    use UnaryPrefixOperator as O;
    match op {
        O::ErrorControl(_) => Prefix::Unary(UnaryOp::ErrorSuppress),
        O::Reference(_) => Prefix::Unary(UnaryOp::Reference),
        O::BitwiseNot(_) => Prefix::Unary(UnaryOp::BitNot),
        O::Not(_) => Prefix::Unary(UnaryOp::Not),
        O::PreIncrement(_) => Prefix::Unary(UnaryOp::PreInc),
        O::PreDecrement(_) => Prefix::Unary(UnaryOp::PreDec),
        O::Plus(_) => Prefix::Unary(UnaryOp::Plus),
        O::Negation(_) => Prefix::Unary(UnaryOp::Neg),
        O::ArrayCast(..) => Prefix::Cast(CastKind::Array),
        O::BoolCast(..) | O::BooleanCast(..) => Prefix::Cast(CastKind::Bool),
        O::DoubleCast(..) | O::RealCast(..) | O::FloatCast(..) => Prefix::Cast(CastKind::Float),
        O::IntCast(..) | O::IntegerCast(..) => Prefix::Cast(CastKind::Int),
        O::ObjectCast(..) => Prefix::Cast(CastKind::Object),
        O::UnsetCast(..) => Prefix::Cast(CastKind::Unset),
        O::StringCast(..) => Prefix::Cast(CastKind::String),
        O::BinaryCast(..) => Prefix::Cast(CastKind::Binary),
        O::VoidCast(..) => Prefix::Cast(CastKind::Void),
    }
}

fn magic(constant: &MagicConstant<'_>) -> MagicConstantKind {
    match constant {
        MagicConstant::Line(_) => MagicConstantKind::Line,
        MagicConstant::File(_) => MagicConstantKind::File,
        MagicConstant::Directory(_) => MagicConstantKind::Dir,
        MagicConstant::Trait(_) => MagicConstantKind::Trait,
        MagicConstant::Method(_) => MagicConstantKind::Method,
        MagicConstant::Function(_) => MagicConstantKind::Function,
        MagicConstant::Property(_) => MagicConstantKind::Property,
        MagicConstant::Namespace(_) => MagicConstantKind::Namespace,
        MagicConstant::Class(_) => MagicConstantKind::Class,
    }
}

fn literal(value: &MagoLiteral<'_>) -> Literal {
    match value {
        MagoLiteral::String(s) => string_literal(&s.kind, s.raw),
        MagoLiteral::Integer(i) => Literal::Int { raw: text(i.raw) },
        MagoLiteral::Float(f) => Literal::Float { raw: text(f.raw) },
        MagoLiteral::True(_) => Literal::Bool(true),
        MagoLiteral::False(_) => Literal::Bool(false),
        MagoLiteral::Null(_) => Literal::Null,
    }
}

impl Cx<'_> {
    pub(crate) fn expr(&mut self, expression: &Expression<'_>) -> Expr {
        let at = self.fit(span(expression.span()));
        if !self.enter() {
            self.leave();
            let diagnostic = Some(self.depth_error(at));
            return Node::new(at, ExprKind::Error { diagnostic });
        }
        let kind = self.bounded(at.end, |cx| cx.expr_kind(expression));
        self.leave();
        Node::new(at, kind)
    }

    fn boxed(&mut self, expression: &Expression<'_>) -> Box<Expr> {
        Box::new(self.expr(expression))
    }

    /// One arm per Mago variant. `Expression` is `#[non_exhaustive]`, so the single wildcard arm
    /// is required by the compiler: a variant added by a Mago upgrade becomes an explicit
    /// unsupported node, which the conformance suite reports.
    #[allow(
        clippy::wildcard_enum_match_arm,
        reason = "the enum is non_exhaustive; the arm is the Unsupported conversion"
    )]
    fn expr_kind(&mut self, expression: &Expression<'_>) -> ExprKind {
        use Expression as E;
        match expression {
            E::Binary(b) => ExprKind::Binary {
                op: binary_op(&b.operator),
                lhs: self.boxed(b.lhs),
                rhs: self.boxed(b.rhs),
            },
            E::UnaryPrefix(u) => self.unary_prefix(&u.operator, u.operand),
            E::UnaryPostfix(u) => self.unary_postfix(&u.operator, u.operand),
            E::Parenthesized(p) => ExprKind::Parenthesized(self.boxed(p.expression)),
            E::Literal(l) => ExprKind::Literal(literal(l)),
            E::CompositeString(c) => self.composite(c),
            E::Assignment(a) => ExprKind::Assign {
                op: assign_op(&a.operator),
                target: self.boxed(a.lhs),
                value: self.boxed(a.rhs),
            },
            E::Conditional(c) => ExprKind::Conditional {
                condition: self.boxed(c.condition),
                then: c.then.map(|t| self.boxed(t)),
                otherwise: self.boxed(c.r#else),
            },
            E::Array(a) => self.array(ArrayStyle::Short, a.elements.iter()),
            E::LegacyArray(a) => self.array(ArrayStyle::Long, a.elements.iter()),
            E::List(l) => self.array(ArrayStyle::List, l.elements.iter()),
            E::ArrayAccess(a) => ExprKind::Index {
                array: self.boxed(a.array),
                index: Some(self.boxed(a.index)),
            },
            E::ArrayAppend(a) => ExprKind::Index {
                array: self.boxed(a.array),
                index: None,
            },
            E::AnonymousClass(a) => self.anonymous_class(a),
            E::Closure(c) => self.closure(c),
            E::ArrowFunction(a) => self.arrow_function(a),
            E::Variable(v) => self.variable_expr(v).kind,
            E::ConstantAccess(c) => ExprKind::ConstFetch(name(&c.name)),
            E::Identifier(i) => ExprKind::Name(name(i)),
            E::Match(m) => ExprKind::Match {
                subject: self.boxed(m.expression),
                arms: m.arms.iter().map(|a| self.match_arm(a)).collect(),
            },
            E::Yield(y) => self.yield_expr(y),
            E::Construct(c) => self.construct(c),
            E::Throw(t) => ExprKind::Throw(self.boxed(t.exception)),
            E::Clone(c) => ExprKind::Clone(self.boxed(c.object)),
            E::Call(c) => self.call(c),
            E::PartialApplication(p) => self.partial(p),
            E::Access(a) => self.access(a),
            E::Parent(_) => ExprKind::ClassRef(ClassRefKind::Parent),
            E::Static(_) => ExprKind::ClassRef(ClassRefKind::Static),
            E::Self_(_) => ExprKind::ClassRef(ClassRefKind::SelfRef),
            E::Instantiation(i) => ExprKind::New {
                class: self.boxed(i.class),
                args: i
                    .argument_list
                    .as_ref()
                    .map_or_else(Vec::new, |l| self.arguments(l)),
            },
            E::MagicConstant(m) => ExprKind::MagicConstant(magic(m)),
            E::Pipe(p) => ExprKind::Pipe {
                input: self.boxed(p.input),
                callable: self.boxed(p.callable),
            },
            E::Error(at) => ExprKind::Error {
                diagnostic: self.link(span(*at)),
            },
            _ => ExprKind::Unsupported {
                kind: "Expression".into(),
            },
        }
    }

    fn unary_postfix(&mut self, op: &UnaryPostfixOperator, operand: &Expression<'_>) -> ExprKind {
        let op = match op {
            UnaryPostfixOperator::PostIncrement(_) => UnaryOp::PostInc,
            UnaryPostfixOperator::PostDecrement(_) => UnaryOp::PostDec,
        };
        ExprKind::Unary {
            op,
            operand: self.boxed(operand),
        }
    }

    fn unary_prefix(&mut self, op: &UnaryPrefixOperator<'_>, operand: &Expression<'_>) -> ExprKind {
        let operand = self.boxed(operand);
        match prefix(op) {
            Prefix::Cast(kind) => ExprKind::Cast { kind, operand },
            Prefix::Unary(op) => ExprKind::Unary { op, operand },
        }
    }

    fn composite(&mut self, composite: &CompositeString<'_>) -> ExprKind {
        let (kind, parts) = match composite {
            CompositeString::ShellExecute(s) => (StringKind::ShellExec, &s.parts),
            CompositeString::Interpolated(s) => (StringKind::DoubleQuoted, &s.parts),
            CompositeString::Document(d) => {
                let kind = match d.kind {
                    mago_syntax::cst::DocumentKind::Heredoc => StringKind::Heredoc,
                    mago_syntax::cst::DocumentKind::Nowdoc => StringKind::Nowdoc,
                };
                (kind, &d.parts)
            }
        };
        ExprKind::Interpolated {
            kind,
            parts: parts.iter().map(|p| self.string_part(p)).collect(),
        }
    }

    fn array<'a>(
        &mut self,
        style: ArrayStyle,
        elements: impl Iterator<Item = &'a ArrayElement<'a>>,
    ) -> ExprKind {
        ExprKind::Array {
            style,
            items: elements.map(|e| self.array_item(e)).collect(),
        }
    }

    fn array_item(&mut self, element: &ArrayElement<'_>) -> ArrayItem {
        let at = self.fit(span(element.span()));
        let kind = match element {
            ArrayElement::KeyValue(kv) => ArrayItemKind::KeyValue {
                key: self.expr(kv.key),
                value: self.expr(kv.value),
            },
            ArrayElement::Value(v) => ArrayItemKind::Value(self.expr(v.value)),
            ArrayElement::Variadic(v) => ArrayItemKind::Spread(self.expr(v.value)),
            ArrayElement::Missing(_) => ArrayItemKind::Missing,
        };
        Node::new(at, kind)
    }

    fn match_arm(&mut self, arm: &MagoMatchArm<'_>) -> MatchArm {
        let at = self.fit(span(arm.span()));
        match arm {
            MagoMatchArm::Expression(e) => MatchArm {
                span: at,
                conditions: e.conditions.iter().map(|c| self.expr(c)).collect(),
                is_default: false,
                body: self.expr(e.expression),
            },
            MagoMatchArm::Default(d) => MatchArm {
                span: at,
                conditions: Vec::new(),
                is_default: true,
                body: self.expr(d.expression),
            },
        }
    }

    fn yield_expr(&mut self, expression: &Yield<'_>) -> ExprKind {
        match expression {
            Yield::Value(v) => ExprKind::Yield {
                key: None,
                value: v.value.map(|e| self.boxed(e)),
            },
            Yield::Pair(p) => ExprKind::Yield {
                key: Some(self.boxed(p.key)),
                value: Some(self.boxed(p.value)),
            },
            Yield::From(f) => ExprKind::YieldFrom(self.boxed(f.iterator)),
        }
    }

    fn construct(&mut self, construct: &Construct<'_>) -> ExprKind {
        match construct {
            Construct::Isset(i) => ExprKind::Isset(i.values.iter().map(|v| self.expr(v)).collect()),
            Construct::Empty(e) => ExprKind::Empty(self.boxed(e.value)),
            Construct::Eval(e) => ExprKind::Eval(self.boxed(e.value)),
            Construct::Print(p) => ExprKind::Print(self.boxed(p.value)),
            Construct::Include(i) => self.include(IncludeKind::Include, i.value),
            Construct::IncludeOnce(i) => self.include(IncludeKind::IncludeOnce, i.value),
            Construct::Require(i) => self.include(IncludeKind::Require, i.value),
            Construct::RequireOnce(i) => self.include(IncludeKind::RequireOnce, i.value),
            Construct::Exit(e) => self.exit(false, e.arguments.as_ref()),
            Construct::Die(d) => self.exit(true, d.arguments.as_ref()),
        }
    }

    fn include(&mut self, kind: IncludeKind, value: &Expression<'_>) -> ExprKind {
        ExprKind::Include {
            kind,
            path: self.boxed(value),
        }
    }

    fn exit(
        &mut self,
        die: bool,
        arguments: Option<&mago_syntax::cst::ArgumentList<'_>>,
    ) -> ExprKind {
        ExprKind::Exit {
            die,
            args: arguments.map_or_else(Vec::new, |a| self.arguments(a)),
        }
    }

    pub(crate) fn variable_expr(&mut self, variable: &Variable<'_>) -> Expr {
        let at = self.fit(span(variable.span()));
        let kind = match variable {
            Variable::Direct(d) => ExprKind::Variable(variable_name(d.name)),
            Variable::Indirect(i) => ExprKind::VariableVariable(self.boxed(i.expression)),
            Variable::Nested(n) => {
                ExprKind::VariableVariable(Box::new(self.variable_expr(n.variable)))
            }
        };
        Node::new(at, kind)
    }

    pub(crate) fn member_selector(&mut self, selector: &ClassLikeMemberSelector<'_>) -> MemberName {
        match selector {
            ClassLikeMemberSelector::Identifier(i) => MemberName::Identifier {
                name: text(i.value),
                span: self.fit(span(i.span)),
            },
            ClassLikeMemberSelector::Variable(v) => {
                MemberName::Dynamic(Box::new(self.variable_expr(v)))
            }
            ClassLikeMemberSelector::Expression(e) => MemberName::Dynamic(self.boxed(e.expression)),
            ClassLikeMemberSelector::Missing(at) => MemberName::Missing(span(*at)),
        }
    }

    fn constant_selector(&mut self, selector: &ClassLikeConstantSelector<'_>) -> MemberName {
        match selector {
            ClassLikeConstantSelector::Identifier(i) => MemberName::Identifier {
                name: text(i.value),
                span: self.fit(span(i.span)),
            },
            ClassLikeConstantSelector::Expression(e) => {
                MemberName::Dynamic(self.boxed(e.expression))
            }
            ClassLikeConstantSelector::Missing(at) => MemberName::Missing(span(*at)),
        }
    }

    fn access(&mut self, access: &Access<'_>) -> ExprKind {
        match access {
            Access::Property(p) => ExprKind::Property {
                object: self.boxed(p.object),
                member: self.member_selector(&p.property),
                nullsafe: false,
            },
            Access::NullSafeProperty(p) => ExprKind::Property {
                object: self.boxed(p.object),
                member: self.member_selector(&p.property),
                nullsafe: true,
            },
            Access::StaticProperty(p) => ExprKind::StaticProperty {
                class: self.boxed(p.class),
                name: Box::new(self.variable_expr(&p.property)),
            },
            Access::ClassConstant(c) => ExprKind::ClassConst {
                class: self.boxed(c.class),
                member: self.constant_selector(&c.constant),
            },
        }
    }

    fn call(&mut self, call: &Call<'_>) -> ExprKind {
        let (target, list) = match call {
            Call::Function(f) => (
                CallTarget::Function(self.boxed(f.function)),
                &f.argument_list,
            ),
            Call::Method(m) => {
                let target = CallTarget::Method {
                    object: self.boxed(m.object),
                    member: self.member_selector(&m.method),
                    nullsafe: false,
                };
                (target, &m.argument_list)
            }
            Call::NullSafeMethod(m) => {
                let target = CallTarget::Method {
                    object: self.boxed(m.object),
                    member: self.member_selector(&m.method),
                    nullsafe: true,
                };
                (target, &m.argument_list)
            }
            Call::StaticMethod(m) => {
                let target = CallTarget::Static {
                    class: self.boxed(m.class),
                    member: self.member_selector(&m.method),
                };
                (target, &m.argument_list)
            }
        };
        ExprKind::Call {
            target,
            args: self.arguments(list),
        }
    }

    /// First-class callables (`f(...)`) are modelled; true partial application is not part of
    /// PHP 8.0 to 8.5 and stays an explicit unsupported node.
    fn partial(&mut self, partial: &PartialApplication<'_>) -> ExprKind {
        let (target, list) = match partial {
            PartialApplication::Function(f) => (
                CallTarget::Function(self.boxed(f.function)),
                &f.argument_list,
            ),
            PartialApplication::Method(m) => {
                let target = CallTarget::Method {
                    object: self.boxed(m.object),
                    member: self.member_selector(&m.method),
                    nullsafe: false,
                };
                (target, &m.argument_list)
            }
            PartialApplication::StaticMethod(m) => {
                let target = CallTarget::Static {
                    class: self.boxed(m.class),
                    member: self.member_selector(&m.method),
                };
                (target, &m.argument_list)
            }
        };
        if list.is_first_class_callable() {
            ExprKind::CallableRef { target }
        } else {
            ExprKind::Unsupported {
                kind: "PartialApplication".into(),
            }
        }
    }
}
