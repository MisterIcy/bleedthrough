//! Type declarations.

use bleedthrough_syntax::{BuiltinType, Node, TypeHint, TypeKind};
use mago_span::HasSpan;
use mago_syntax::cst::Hint;

use super::Cx;
use super::common::name;
use crate::errors::span;

impl Cx<'_> {
    pub(crate) fn type_hint(&mut self, hint: &Hint<'_>) -> TypeHint {
        let at = self.fit(span(hint.span()));
        if !self.enter() {
            self.leave();
            let diagnostic = Some(self.depth_error(at));
            return Node::new(at, TypeKind::Error { diagnostic });
        }
        let kind = self.type_kind(hint);
        self.leave();
        Node::new(at, kind)
    }

    fn type_kind(&mut self, hint: &Hint<'_>) -> TypeKind {
        use BuiltinType as B;
        let builtin = |b| TypeKind::Builtin(b);
        match hint {
            Hint::Identifier(i) => TypeKind::Named(name(i)),
            Hint::Parenthesized(p) => TypeKind::Parenthesized(Box::new(self.type_hint(p.hint))),
            Hint::Nullable(n) => TypeKind::Nullable(Box::new(self.type_hint(n.hint))),
            Hint::Union(_) => TypeKind::Union(self.flatten(hint, true)),
            Hint::Intersection(_) => TypeKind::Intersection(self.flatten(hint, false)),
            Hint::Null(_) => builtin(B::Null),
            Hint::True(_) => builtin(B::True),
            Hint::False(_) => builtin(B::False),
            Hint::Array(_) => builtin(B::Array),
            Hint::Callable(_) => builtin(B::Callable),
            Hint::Static(_) => builtin(B::Static),
            Hint::Self_(_) => builtin(B::SelfType),
            Hint::Parent(_) => builtin(B::Parent),
            Hint::Void(_) => builtin(B::Void),
            Hint::Never(_) => builtin(B::Never),
            Hint::Float(_) => builtin(B::Float),
            Hint::Bool(_) => builtin(B::Bool),
            Hint::Integer(_) => builtin(B::Int),
            Hint::String(_) => builtin(B::String),
            Hint::Object(_) => builtin(B::Object),
            Hint::Mixed(_) => builtin(B::Mixed),
            Hint::Iterable(_) => builtin(B::Iterable),
        }
    }

    /// Flattens a left-nested chain of unions (or intersections) into one list.
    fn flatten(&mut self, hint: &Hint<'_>, union: bool) -> Vec<TypeHint> {
        let mut right_to_left = Vec::new();
        let mut current = hint;
        loop {
            let (left, right) = match (current, union) {
                (Hint::Union(u), true) => (u.left, u.right),
                (Hint::Intersection(i), false) => (i.left, i.right),
                _ => break,
            };
            right_to_left.push(right);
            current = left;
        }
        let mut out = vec![self.type_hint(current)];
        out.extend(right_to_left.into_iter().rev().map(|h| self.type_hint(h)));
        out
    }
}
