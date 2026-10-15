//! Declarations: functions, parameters, classes and their members.

use bleedthrough_syntax::{
    Attribute, ClassLike, ClassLikeKind, ClosureUse, ConstItem, ExprKind, FunctionLike, HookBody,
    Member, MemberKind, Modifier, Name, Node, Param, PropertyHook, PropertyItem, Span,
    TraitAdaptation, TraitAdaptationKind, TypeHint,
};
use mago_span::HasSpan;
use mago_syntax::cst::{
    AnonymousClass, ArrowFunction, AttributeList, Class, ClassLikeMember, Closure, Enum, Extends,
    Function, FunctionLikeParameter, FunctionLikeParameterList, FunctionLikeReturnTypeHint,
    Implements, Interface, Method, MethodBody, Modifier as MagoModifier, PropertyHook as MagoHook,
    PropertyHookBody, PropertyHookConcreteBody, PropertyHookList, Sequence, Trait, TraitUse,
    TraitUseAdaptation, TraitUseMethodReference, TraitUseSpecification,
};

use super::Cx;
use super::common::{name, text, variable_name};
use crate::errors::span;

pub(super) fn modifier(m: &MagoModifier<'_>) -> Modifier {
    match m {
        MagoModifier::Static(_) => Modifier::Static,
        MagoModifier::Final(_) => Modifier::Final,
        MagoModifier::Abstract(_) => Modifier::Abstract,
        MagoModifier::Readonly(_) => Modifier::Readonly,
        MagoModifier::Public(_) => Modifier::Public,
        MagoModifier::PublicSet(_) => Modifier::PublicSet,
        MagoModifier::Protected(_) => Modifier::Protected,
        MagoModifier::ProtectedSet(_) => Modifier::ProtectedSet,
        MagoModifier::Private(_) => Modifier::Private,
        MagoModifier::PrivateSet(_) => Modifier::PrivateSet,
    }
}

fn modifiers(list: &Sequence<'_, MagoModifier<'_>>) -> Vec<Modifier> {
    list.iter().map(modifier).collect()
}

fn names<'a>(list: impl Iterator<Item = &'a mago_syntax::cst::Identifier<'a>>) -> Vec<Name> {
    list.map(name).collect()
}

/// The pieces every class-like declaration shares.
struct Parts<'a> {
    kind: ClassLikeKind,
    name: Option<Box<str>>,
    attributes: Vec<Attribute>,
    modifiers: Vec<Modifier>,
    extends: Option<&'a Extends<'a>>,
    implements: Option<&'a Implements<'a>>,
    backing_type: Option<TypeHint>,
    /// The source between the braces, where member gaps are looked for.
    inside: Span,
}

impl Cx<'_> {
    fn return_type(&mut self, hint: Option<&FunctionLikeReturnTypeHint<'_>>) -> Option<TypeHint> {
        hint.map(|h| self.type_hint(&h.hint))
    }

    pub(crate) fn params(&mut self, list: &FunctionLikeParameterList<'_>) -> Vec<Param> {
        list.parameters.iter().map(|p| self.param(p)).collect()
    }

    fn param(&mut self, p: &FunctionLikeParameter<'_>) -> Param {
        Param {
            span: self.fit(span(p.span())),
            attributes: self.attributes(p.attribute_lists.iter()),
            modifiers: modifiers(&p.modifiers),
            type_hint: p.hint.as_ref().map(|h| self.type_hint(h)),
            by_ref: p.ampersand.is_some(),
            variadic: p.ellipsis.is_some(),
            name: variable_name(p.variable.name),
            default: p.default_value.as_ref().map(|d| self.expr(d.value)),
            hooks: p.hooks.as_ref().map_or_else(Vec::new, |h| self.hooks(h)),
        }
    }

    pub(crate) fn hooks(&mut self, list: &PropertyHookList<'_>) -> Vec<PropertyHook> {
        let hooks: Vec<&MagoHook<'_>> = list.hooks.iter().collect();
        let starts: Vec<u32> = hooks.iter().map(|h| span(h.span()).start).collect();
        let end = span(list.right_brace).start;
        self.each_bounded(&hooks, &starts, end, |cx, h| cx.hook(h))
    }

    fn hook(&mut self, hook: &MagoHook<'_>) -> PropertyHook {
        let body = match &hook.body {
            PropertyHookBody::Abstract(_) => HookBody::Abstract,
            PropertyHookBody::Concrete(PropertyHookConcreteBody::Block(b)) => {
                HookBody::Block(self.block(b))
            }
            PropertyHookBody::Concrete(PropertyHookConcreteBody::Expression(e)) => {
                HookBody::Expr(Box::new(self.expr(e.expression)))
            }
        };
        PropertyHook {
            span: self.fit(span(hook.span())),
            attributes: self.attributes(hook.attribute_lists.iter()),
            modifiers: modifiers(&hook.modifiers),
            by_ref: hook.ampersand.is_some(),
            name: text(hook.name.value),
            params: hook
                .parameter_list
                .as_ref()
                .map_or_else(Vec::new, |l| self.params(l)),
            body,
        }
    }

    pub(crate) fn function(&mut self, f: &Function<'_>) -> FunctionLike {
        FunctionLike {
            name: text(f.name.value),
            attributes: self.attributes(f.attribute_lists.iter()),
            modifiers: Vec::new(),
            by_ref: f.ampersand.is_some(),
            params: self.params(&f.parameter_list),
            return_type: self.return_type(f.return_type_hint.as_ref()),
            body: Some(self.block(&f.body)),
        }
    }

    fn method(&mut self, m: &Method<'_>) -> FunctionLike {
        let body = match &m.body {
            MethodBody::Abstract(_) => None,
            MethodBody::Concrete(b) => Some(self.block(b)),
        };
        FunctionLike {
            name: text(m.name.value),
            attributes: self.attributes(m.attribute_lists.iter()),
            modifiers: modifiers(&m.modifiers),
            by_ref: m.ampersand.is_some(),
            params: self.params(&m.parameter_list),
            return_type: self.return_type(m.return_type_hint.as_ref()),
            body,
        }
    }

    pub(crate) fn closure(&mut self, c: &Closure<'_>) -> ExprKind {
        let uses = c.use_clause.as_ref().map_or_else(Vec::new, |u| {
            u.variables
                .iter()
                .map(|v| ClosureUse {
                    span: self.fit(span(v.span())),
                    name: variable_name(v.variable.name),
                    by_ref: v.ampersand.is_some(),
                })
                .collect()
        });
        ExprKind::Closure {
            attributes: self.attributes(c.attribute_lists.iter()),
            is_static: c.r#static.is_some(),
            by_ref: c.ampersand.is_some(),
            params: self.params(&c.parameter_list),
            uses,
            return_type: self.return_type(c.return_type_hint.as_ref()).map(Box::new),
            body: self.block(&c.body),
        }
    }

    pub(crate) fn arrow_function(&mut self, a: &ArrowFunction<'_>) -> ExprKind {
        ExprKind::ArrowFn {
            attributes: self.attributes(a.attribute_lists.iter()),
            is_static: a.r#static.is_some(),
            by_ref: a.ampersand.is_some(),
            params: self.params(&a.parameter_list),
            return_type: self.return_type(a.return_type_hint.as_ref()).map(Box::new),
            body: Box::new(self.expr(a.expression)),
        }
    }

    fn class_like(
        &mut self,
        parts: Parts<'_>,
        members: &Sequence<'_, ClassLikeMember<'_>>,
    ) -> ClassLike {
        ClassLike {
            kind: parts.kind,
            name: parts.name,
            attributes: parts.attributes,
            modifiers: parts.modifiers,
            extends: parts
                .extends
                .map_or_else(Vec::new, |e| names(e.types.iter())),
            implements: parts
                .implements
                .map_or_else(Vec::new, |i| names(i.types.iter())),
            backing_type: parts.backing_type,
            members: self.members(members, parts.inside),
        }
    }

    fn simple_parts<'a>(
        &mut self,
        kind: ClassLikeKind,
        label: &[u8],
        lists: &Sequence<'_, AttributeList<'_>>,
        inside: Span,
    ) -> Parts<'a> {
        Parts {
            kind,
            name: Some(text(label)),
            attributes: self.attributes(lists.iter()),
            modifiers: Vec::new(),
            extends: None,
            implements: None,
            backing_type: None,
            inside,
        }
    }

    pub(crate) fn class(&mut self, c: &Class<'_>) -> ClassLike {
        let mut parts = self.simple_parts(
            ClassLikeKind::Class,
            c.name.value,
            &c.attribute_lists,
            braces(c.left_brace, c.right_brace),
        );
        parts.modifiers = modifiers(&c.modifiers);
        parts.extends = c.extends.as_ref();
        parts.implements = c.implements.as_ref();
        self.class_like(parts, &c.members)
    }

    pub(crate) fn interface(&mut self, i: &Interface<'_>) -> ClassLike {
        let mut parts = self.simple_parts(
            ClassLikeKind::Interface,
            i.name.value,
            &i.attribute_lists,
            braces(i.left_brace, i.right_brace),
        );
        parts.extends = i.extends.as_ref();
        self.class_like(parts, &i.members)
    }

    pub(crate) fn trait_decl(&mut self, t: &Trait<'_>) -> ClassLike {
        let parts = self.simple_parts(
            ClassLikeKind::Trait,
            t.name.value,
            &t.attribute_lists,
            braces(t.left_brace, t.right_brace),
        );
        self.class_like(parts, &t.members)
    }

    pub(crate) fn enum_decl(&mut self, e: &Enum<'_>) -> ClassLike {
        let mut parts = self.simple_parts(
            ClassLikeKind::Enum,
            e.name.value,
            &e.attribute_lists,
            braces(e.left_brace, e.right_brace),
        );
        parts.implements = e.implements.as_ref();
        parts.backing_type = e
            .backing_type_hint
            .as_ref()
            .map(|b| self.type_hint(&b.hint));
        self.class_like(parts, &e.members)
    }

    pub(crate) fn anonymous_class(&mut self, a: &AnonymousClass<'_>) -> ExprKind {
        let mut parts = self.simple_parts(
            ClassLikeKind::Class,
            b"",
            &a.attribute_lists,
            braces(a.left_brace, a.right_brace),
        );
        parts.name = None;
        parts.modifiers = modifiers(&a.modifiers);
        parts.extends = a.extends.as_ref();
        parts.implements = a.implements.as_ref();
        let args = a
            .argument_list
            .as_ref()
            .map_or_else(Vec::new, |l| self.partial_arguments(l));
        ExprKind::AnonymousClass {
            args,
            class: Box::new(self.class_like(parts, &a.members)),
        }
    }

    fn members(
        &mut self,
        members: &Sequence<'_, ClassLikeMember<'_>>,
        inside: Span,
    ) -> Vec<Member> {
        let list: Vec<&ClassLikeMember<'_>> = members.iter().collect();
        let starts: Vec<u32> = list.iter().map(|m| span(m.span()).start).collect();
        let converted = self.each_bounded(&list, &starts, inside.end, |cx, m| cx.member(m));
        self.with_gaps(converted, inside, |diagnostic| MemberKind::Error {
            diagnostic,
        })
    }

    fn member(&mut self, member: &ClassLikeMember<'_>) -> Member {
        let at = self.fit(span(member.span()));
        let kind = self.bounded(at.end, |cx| match member {
            ClassLikeMember::TraitUse(t) => cx.trait_use(t),
            ClassLikeMember::Constant(c) => cx.class_const(c),
            ClassLikeMember::Property(p) => cx.property(p),
            ClassLikeMember::EnumCase(c) => cx.enum_case(c),
            ClassLikeMember::Method(m) => MemberKind::Method(Box::new(cx.method(m))),
        });
        Node::new(at, kind)
    }

    fn class_const(&mut self, c: &mago_syntax::cst::ClassLikeConstant<'_>) -> MemberKind {
        MemberKind::Const {
            attributes: self.attributes(c.attribute_lists.iter()),
            modifiers: modifiers(&c.modifiers),
            type_hint: c.hint.as_ref().map(|h| self.type_hint(h)),
            items: c
                .items
                .iter()
                .map(|i| ConstItem {
                    span: self.fit(span(i.span())),
                    name: text(i.name.value),
                    value: self.expr(i.value),
                })
                .collect(),
        }
    }

    fn enum_case(&mut self, c: &mago_syntax::cst::EnumCase<'_>) -> MemberKind {
        use mago_syntax::cst::EnumCaseItem;
        let (case_name, value) = match &c.item {
            EnumCaseItem::Unit(u) => (text(u.name.value), None),
            EnumCaseItem::Backed(b) => (text(b.name.value), Some(self.expr(b.value))),
        };
        MemberKind::EnumCase {
            attributes: self.attributes(c.attribute_lists.iter()),
            name: case_name,
            value,
        }
    }

    fn property(&mut self, property: &mago_syntax::cst::Property<'_>) -> MemberKind {
        use mago_syntax::cst::Property;
        match property {
            Property::Plain(p) => MemberKind::Property {
                attributes: self.attributes(p.attribute_lists.iter()),
                modifiers: modifiers(&p.modifiers),
                type_hint: p.hint.as_ref().map(|h| self.type_hint(h)),
                items: p.items.iter().map(|i| self.property_item(i)).collect(),
                hooks: Vec::new(),
            },
            Property::Hooked(h) => MemberKind::Property {
                attributes: self.attributes(h.attribute_lists.iter()),
                modifiers: modifiers(&h.modifiers),
                type_hint: h.hint.as_ref().map(|t| self.type_hint(t)),
                items: vec![self.property_item(&h.item)],
                hooks: self.hooks(&h.hook_list),
            },
        }
    }

    fn property_item(&mut self, item: &mago_syntax::cst::PropertyItem<'_>) -> PropertyItem {
        use mago_syntax::cst::PropertyItem as P;
        let (var, default) = match item {
            P::Abstract(a) => (&a.variable, None),
            P::Concrete(c) => (&c.variable, Some(self.expr(c.value))),
        };
        PropertyItem {
            span: self.fit(span(item.span())),
            name: variable_name(var.name),
            default,
        }
    }

    fn trait_use(&mut self, t: &TraitUse<'_>) -> MemberKind {
        let adaptations = match &t.specification {
            TraitUseSpecification::Abstract(_) => Vec::new(),
            TraitUseSpecification::Concrete(c) => {
                let list: Vec<&TraitUseAdaptation<'_>> = c.adaptations.iter().collect();
                let starts: Vec<u32> = list.iter().map(|a| span(a.span()).start).collect();
                let end = span(c.right_brace).start;
                self.each_bounded(&list, &starts, end, |cx, a| cx.adaptation(a))
            }
        };
        MemberKind::TraitUse {
            traits: names(t.trait_names.iter()),
            adaptations,
        }
    }
}

/// The source strictly between a pair of braces.
fn braces(left: mago_span::Span, right: mago_span::Span) -> Span {
    Span::new(left.end.offset, right.start.offset)
}

impl Cx<'_> {
    fn adaptation(&self, a: &TraitUseAdaptation<'_>) -> TraitAdaptation {
        let at = self.fit(span(a.span()));
        let kind = match a {
            TraitUseAdaptation::Precedence(p) => TraitAdaptationKind::Precedence {
                trait_name: name(&p.method_reference.trait_name),
                method: text(p.method_reference.method_name.value),
                instead_of: names(p.trait_names.iter()),
            },
            TraitUseAdaptation::Alias(al) => {
                let (trait_name, method) = match &al.method_reference {
                    TraitUseMethodReference::Identifier(i) => (None, text(i.value)),
                    TraitUseMethodReference::Absolute(r) => {
                        (Some(name(&r.trait_name)), text(r.method_name.value))
                    }
                };
                TraitAdaptationKind::Alias {
                    trait_name,
                    method,
                    visibility: al.modifier.as_ref().map(modifier),
                    alias: al.alias.as_ref().map(|i| text(i.value)),
                }
            }
        };
        Node::new(at, kind)
    }
}
