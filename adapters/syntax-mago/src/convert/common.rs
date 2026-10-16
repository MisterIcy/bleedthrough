//! Names, literals, arguments and attributes.

use bleedthrough_syntax::{
    Arg, ArgKind, Attribute, Expr, ExprKind, Literal, Name, Node, QuoteKind, Span, StringPart,
    StringPartKind,
};
use mago_span::HasSpan;
use mago_syntax::cst::{
    Argument, ArgumentList, AttributeList, Identifier, LiteralStringKind, PartialArgument,
    PartialArgumentList, StringPart as MagoStringPart,
};

use super::Cx;
use crate::errors::span;

/// `at`, widened to start at the first attribute list when there is one. Mago's spans for some
/// declarations (constants) start after their attributes, which would leave those attributes
/// looking like unconverted source.
pub(crate) fn with_attributes(
    at: bleedthrough_syntax::Span,
    lists: &mago_syntax::cst::Sequence<'_, AttributeList<'_>>,
) -> bleedthrough_syntax::Span {
    match lists.first() {
        Some(first) => {
            bleedthrough_syntax::Span::new(span(first.span()).start.min(at.start), at.end)
        }
        None => at,
    }
}

pub(crate) fn text(bytes: &[u8]) -> Box<str> {
    String::from_utf8_lossy(bytes).into()
}

/// Variable names are kept without the leading `$`.
pub(crate) fn variable_name(bytes: &[u8]) -> Box<str> {
    text(bytes.strip_prefix(b"$").unwrap_or(bytes))
}

pub(crate) fn name(identifier: &Identifier<'_>) -> Name {
    let (at, value) = match identifier {
        Identifier::Local(i) => (i.span, i.value),
        Identifier::Qualified(i) => (i.span, i.value),
        Identifier::FullyQualified(i) => (i.span, i.value),
    };
    Name::from_written(span(at), &String::from_utf8_lossy(value))
}

pub(crate) fn string_literal(kind: &LiteralStringKind, raw: &[u8]) -> Literal {
    let quote = match kind {
        LiteralStringKind::SingleQuoted => QuoteKind::Single,
        LiteralStringKind::DoubleQuoted => QuoteKind::Double,
    };
    Literal::String {
        raw: text(raw),
        quote,
    }
}

impl Cx<'_> {
    pub(crate) fn string_part(&mut self, part: &MagoStringPart<'_>) -> StringPart {
        match part {
            MagoStringPart::Literal(l) => Node::new(
                self.fit(span(l.span)),
                StringPartKind::Text { raw: text(l.raw) },
            ),
            MagoStringPart::Expression(e) => {
                let inner = self.expr(e);
                Node::new(inner.span, StringPartKind::Interpolation(Box::new(inner)))
            }
            MagoStringPart::BracedExpression(b) => {
                let inner = self.expr(b.expression);
                let at = span(b.left_brace).join(span(b.right_brace));
                Node::new(at, StringPartKind::Interpolation(Box::new(inner)))
            }
        }
    }

    fn positional(ellipsis: bool, value: Expr, at: Span) -> Arg {
        let kind = if ellipsis {
            ArgKind::Spread(value)
        } else {
            ArgKind::Positional(value)
        };
        Node::new(at, kind)
    }

    pub(crate) fn argument(&mut self, argument: &Argument<'_>) -> Arg {
        match argument {
            Argument::Positional(p) => {
                let value = self.expr(p.value);
                Self::positional(p.ellipsis.is_some(), value, self.fit(span(argument.span())))
            }
            Argument::Named(n) => {
                let value = self.expr(n.value);
                let kind = ArgKind::Named {
                    name: text(n.name.value),
                    value,
                };
                Node::new(self.fit(span(argument.span())), kind)
            }
        }
    }

    pub(crate) fn arguments(&mut self, list: &ArgumentList<'_>) -> Vec<Arg> {
        list.arguments.iter().map(|a| self.argument(a)).collect()
    }

    /// Arguments of an attribute or a partial application; placeholders are not arguments of any
    /// supported construct and become unsupported expressions.
    pub(crate) fn partial_arguments(&mut self, list: &PartialArgumentList<'_>) -> Vec<Arg> {
        list.arguments
            .iter()
            .map(|a| self.partial_argument(a))
            .collect()
    }

    fn partial_argument(&mut self, argument: &PartialArgument<'_>) -> Arg {
        let at = self.fit(span(argument.span()));
        match argument {
            PartialArgument::Positional(p) => {
                let value = self.expr(p.value);
                Self::positional(p.ellipsis.is_some(), value, at)
            }
            PartialArgument::Named(n) => {
                let value = self.expr(n.value);
                Node::new(
                    at,
                    ArgKind::Named {
                        name: text(n.name.value),
                        value,
                    },
                )
            }
            PartialArgument::NamedPlaceholder(_)
            | PartialArgument::Placeholder(_)
            | PartialArgument::VariadicPlaceholder(_) => {
                let hole = ExprKind::Unsupported {
                    kind: "Placeholder".into(),
                };
                Node::new(at, ArgKind::Positional(Node::new(at, hole)))
            }
        }
    }

    pub(crate) fn attributes<'a>(
        &mut self,
        lists: impl IntoIterator<Item = &'a AttributeList<'a>>,
    ) -> Vec<Attribute> {
        let mut out = Vec::new();
        for list in lists {
            for attribute in list.attributes.iter() {
                let args = match &attribute.argument_list {
                    Some(arguments) => self.partial_arguments(arguments),
                    None => Vec::new(),
                };
                out.push(Attribute {
                    span: self.fit(span(attribute.span())),
                    name: name(&attribute.name),
                    args,
                });
            }
        }
        out
    }
}
