//! One pass over a syntax tree producing its [`FileSummary`]. Tolerant: `Error` and `Unsupported`
//! nodes are skipped and flagged, never fatal.

use std::collections::BTreeSet;

use bleedthrough_syntax::{
    Attribute, CallTarget, ClassLike, ClassLikeKind, Comment, Expr, ExprKind, FunctionLike,
    Literal, Member, MemberKind, Modifier, Name, NodeRef, Param, Span, Stmt, StmtKind, SyntaxTree,
    TraitAdaptationKind, UnaryOp, Visitor, docblock_before, walk,
};

use super::model::{
    Adaptation, CaseDecl, ClassDecl, ClassKind, ConstDecl, ConstantDecl, ConstantVia, FileSummary,
    FunctionDecl, FunctionRef, MemberOrigin, MethodDecl, ParamDecl, PropertyDecl, TraitUse,
    Visibility,
};
use crate::diag::SymbolDiagnostic;
use crate::docblock::{DocBlock, DocMethodParam, PropertyAccess, TypeContext, parse_docblock};
use crate::name::{Fqn, NameContext, ResolvedName};
use crate::native::{EffectiveType, TypeSource, merge_types, native_type};
use crate::scope::{FileScopes, NoDeclarations, Scope};
use crate::special::ClassContext;
use crate::types::{Atom, Type};

/// Summarises one file. `path` is only recorded; `source` must be the text `tree` was parsed from.
pub fn summarize(path: &str, tree: &SyntaxTree, source: &str) -> FileSummary {
    let scopes = FileScopes::build(tree.statements());
    let mut extractor = Extractor {
        comments: tree.comments(),
        source: source.as_bytes(),
        scopes: &scopes,
        classes: Vec::new(),
        functions: Vec::new(),
        constants: Vec::new(),
        diagnostics: scopes.diagnostics().to_vec(),
    };
    extractor.statements(tree.statements());
    let mut summary = FileSummary {
        path: path.into(),
        classes: extractor.classes,
        functions: extractor.functions,
        constants: extractor.constants,
        referenced_classes: Vec::new(),
        declaration_classes: Vec::new(),
        referenced_functions: Vec::new(),
        diagnostics: extractor.diagnostics,
        incomplete: tree.hole_counts() != (0, 0),
    };
    summary.diagnostics.sort();
    let (classes, declared, functions) = references(tree, &scopes, &summary);
    summary.referenced_classes = classes;
    summary.declaration_classes = declared;
    summary.referenced_functions = functions;
    summary
}

struct Extractor<'a> {
    comments: &'a [Comment],
    source: &'a [u8],
    scopes: &'a FileScopes,
    classes: Vec<ClassDecl>,
    functions: Vec<FunctionDecl>,
    constants: Vec<ConstantDecl>,
    diagnostics: Vec<SymbolDiagnostic>,
}

/// The names in force where a declaration sits.
#[derive(Clone, Copy)]
struct Site<'s> {
    scope: &'s Scope,
    class: &'s ClassContext,
}

/// What a signature contributes besides its parameters and return type.
struct Signature {
    params: Vec<ParamDecl>,
    ret: EffectiveType,
    templates: Vec<Box<str>>,
    docblock: Option<Box<str>>,
    promoted: Vec<PropertyDecl>,
}

impl Extractor<'_> {
    fn statements(&mut self, list: &[Stmt]) {
        for stmt in list {
            match &stmt.kind {
                StmtKind::Namespace { body, .. } | StmtKind::Block(body) => self.statements(body),
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
                StmtKind::ClassLike(class) => self.class(class, stmt.span),
                StmtKind::Function(function) => self.function(function, stmt.span),
                StmtKind::Const { items, .. } => {
                    let scope = self.scopes.at(stmt.span.start);
                    for item in items {
                        self.constants.push(ConstantDecl {
                            name: scope.qualify(&item.name),
                            span: item.span,
                            ty: literal_type(&item.value),
                            via: ConstantVia::Const,
                            builtin: None,
                        });
                    }
                }
                StmtKind::Expr(expr) => self.define(expr),
                _ => {}
            }
        }
    }

    /// `define('NAME', value)` with a literal name.
    fn define(&mut self, expr: &Expr) {
        use bleedthrough_syntax::ArgKind;
        let ExprKind::Call {
            target: CallTarget::Function(callee),
            args,
        } = &expr.kind
        else {
            return;
        };
        let ExprKind::Name(callee) = &callee.kind else {
            return;
        };
        if !callee.text.eq_ignore_ascii_case("define") {
            return;
        }
        let (Some(first), Some(second)) = (args.first(), args.get(1)) else {
            return;
        };
        let (ArgKind::Positional(name), ArgKind::Positional(value)) = (&first.kind, &second.kind)
        else {
            return;
        };
        if let Some(text) = string_value(name) {
            self.constants.push(ConstantDecl {
                name: Fqn::new(&text),
                span: expr.span,
                ty: literal_type(value),
                via: ConstantVia::Define,
                builtin: None,
            });
        }
    }

    fn docblock_text(&self, start: u32) -> Option<&Comment> {
        docblock_before(self.comments, self.source, start)
    }

    /// Parses the docblock before `start`, collecting its diagnostics.
    fn doc(
        &mut self,
        start: u32,
        scope: &Scope,
        class: &ClassContext,
        templates: &[Box<str>],
    ) -> (DocBlock, Option<Box<str>>) {
        let Some(comment) = self.docblock_text(start) else {
            return (DocBlock::default(), None);
        };
        let ctx = TypeContext::new()
            .with_scope(scope)
            .with_class(class)
            .with_templates(templates);
        let mut block = parse_docblock(&comment.text, comment.span.start, &ctx);
        let text: Box<str> = comment.text.as_ref().into();
        self.diagnostics.append(&mut block.diagnostics);
        (block, Some(text))
    }

    fn function(&mut self, function: &FunctionLike, span: Span) {
        let scope = self.scopes.at(span.start).clone();
        let class = ClassContext::none();
        let signature = self.signature(
            function,
            span,
            Site {
                scope: &scope,
                class: &class,
            },
            &[],
        );
        self.functions.push(FunctionDecl {
            name: scope.qualify(&function.name),
            span,
            params: signature.params,
            ret: signature.ret,
            by_ref: function.by_ref,
            attributes: attributes(&function.attributes, &scope),
            docblock: signature.docblock,
            templates: signature.templates,
            builtin: None,
        });
    }

    fn signature(
        &mut self,
        function: &FunctionLike,
        span: Span,
        site: Site<'_>,
        outer: &[Box<str>],
    ) -> Signature {
        let Site { scope, class } = site;
        let (doc, text) = self.doc(span.start, scope, class, outer);
        let mut params = Vec::new();
        let mut promoted = Vec::new();
        let constructor = function.name.eq_ignore_ascii_case("__construct");
        for param in &function.params {
            let doc_ty = doc
                .params
                .iter()
                .find(|p| *p.name == *param.name)
                .and_then(|p| p.ty.clone());
            let decl = self.param(param, doc_ty, scope, class);
            if constructor && is_promoted(param) {
                promoted.push(promoted_property(param, &decl, scope));
            }
            params.push(decl);
        }
        let native = function
            .return_type
            .as_ref()
            .map(|t| native_type(t, scope, class));
        let ret = merge_types(
            native,
            doc.returns.as_ref().map(|r| r.ty.clone()),
            span,
            &mut self.diagnostics,
        );
        Signature {
            params,
            ret,
            templates: doc.templates.iter().map(|t| t.name.clone()).collect(),
            docblock: text,
            promoted,
        }
    }

    fn param(
        &mut self,
        param: &Param,
        doc_ty: Option<Type>,
        scope: &Scope,
        class: &ClassContext,
    ) -> ParamDecl {
        let mut native = param
            .type_hint
            .as_ref()
            .map(|t| native_type(t, scope, class));
        if param.default.as_ref().is_some_and(is_null_literal) {
            native = native.map(|t| t.union(&Type::null()));
        }
        ParamDecl {
            name: param.name.clone(),
            ty: merge_types(native, doc_ty, param.span, &mut self.diagnostics),
            by_ref: param.by_ref,
            variadic: param.variadic,
            has_default: param.default.is_some(),
            attributes: attributes(&param.attributes, scope),
        }
    }

    fn class(&mut self, class: &ClassLike, span: Span) {
        let Some(short) = &class.name else {
            return;
        };
        let scope = self.scopes.at(span.start).clone();
        let name = scope.qualify(short);
        let kind = class_kind(class.kind);
        let resolve = |names: &[Name]| -> Vec<Fqn> {
            names.iter().filter_map(|n| class_name(&scope, n)).collect()
        };
        let (parent, interfaces) = match class.kind {
            ClassLikeKind::Interface => (None, resolve(&class.extends)),
            _ => (
                class.extends.first().and_then(|n| class_name(&scope, n)),
                resolve(&class.implements),
            ),
        };
        let context = ClassContext::new(name.clone(), class.kind, parent.clone());
        let (doc, text) = self.doc(span.start, &scope, &context, &[]);
        let templates: Vec<Box<str>> = doc.templates.iter().map(|t| t.name.clone()).collect();
        let mut decl = ClassDecl {
            name,
            kind,
            span,
            is_abstract: class.modifiers.contains(&Modifier::Abstract),
            is_final: class.modifiers.contains(&Modifier::Final),
            is_readonly: class.modifiers.contains(&Modifier::Readonly),
            parent,
            interfaces,
            trait_uses: Vec::new(),
            backing_type: class
                .backing_type
                .as_ref()
                .map(|t| native_type(t, &scope, &context)),
            constants: Vec::new(),
            properties: Vec::new(),
            methods: Vec::new(),
            cases: Vec::new(),
            templates,
            inherits: doc.inherits.clone(),
            attributes: attributes(&class.attributes, &scope),
            docblock: text,
            builtin: None,
        };
        for member in &class.members {
            self.member(
                member,
                &mut decl,
                Site {
                    scope: &scope,
                    class: &context,
                },
            );
        }
        add_docblock_members(&doc, &mut decl);
        self.classes.push(decl);
    }

    fn member(&mut self, member: &Member, decl: &mut ClassDecl, site: Site<'_>) {
        let Site { scope, class: ctx } = site;
        match &member.kind {
            MemberKind::TraitUse {
                traits,
                adaptations,
            } => decl.trait_uses.push(trait_use(traits, adaptations, scope)),
            MemberKind::Const {
                modifiers,
                type_hint,
                items,
                ..
            } => {
                for item in items {
                    let ty = match type_hint {
                        Some(hint) => native_type(hint, scope, ctx),
                        None => literal_type(&item.value),
                    };
                    decl.constants.push(ConstDecl {
                        name: item.name.clone(),
                        span: item.span,
                        visibility: visibility(modifiers),
                        is_final: modifiers.contains(&Modifier::Final),
                        ty,
                    });
                }
            }
            MemberKind::Property {
                attributes: attrs,
                modifiers,
                type_hint,
                items,
                hooks,
            } => {
                let (doc, _) = self.doc(member.span.start, scope, ctx, &decl.templates);
                for item in items {
                    let doc_ty = doc
                        .vars
                        .iter()
                        .find(|v| v.name.as_deref().is_none_or(|n| n == &*item.name))
                        .map(|v| v.ty.clone());
                    let native = type_hint.as_ref().map(|t| native_type(t, scope, ctx));
                    decl.properties.push(PropertyDecl {
                        name: item.name.clone(),
                        span: item.span,
                        visibility: visibility(modifiers),
                        set_visibility: set_visibility(modifiers),
                        is_static: modifiers.contains(&Modifier::Static),
                        is_readonly: modifiers.contains(&Modifier::Readonly) || decl.is_readonly,
                        ty: merge_types(native, doc_ty, item.span, &mut self.diagnostics),
                        has_default: item.default.is_some(),
                        hooks: hooks.iter().map(|h| h.name.clone()).collect(),
                        origin: MemberOrigin::Declared,
                        attributes: attributes(attrs, scope),
                    });
                }
            }
            MemberKind::Method(function) => self.method(function, member.span, decl, site),
            MemberKind::EnumCase { name, value, .. } => decl.cases.push(CaseDecl {
                name: name.clone(),
                span: member.span,
                value: value.as_ref().map(literal_type),
            }),
            _ => {}
        }
    }

    fn method(
        &mut self,
        function: &FunctionLike,
        span: Span,
        decl: &mut ClassDecl,
        site: Site<'_>,
    ) {
        let scope = site.scope;
        let signature = self.signature(function, span, site, &decl.templates);
        let in_interface = decl.kind == ClassKind::Interface;
        for mut property in signature.promoted {
            property.is_readonly |= decl.is_readonly;
            decl.properties.push(property);
        }
        decl.methods.push(MethodDecl {
            name: function.name.clone(),
            span,
            visibility: visibility(&function.modifiers),
            is_static: function.modifiers.contains(&Modifier::Static),
            is_final: function.modifiers.contains(&Modifier::Final),
            is_abstract: function.modifiers.contains(&Modifier::Abstract)
                || (in_interface && function.body.is_none()),
            params: signature.params,
            ret: signature.ret,
            by_ref: function.by_ref,
            origin: MemberOrigin::Declared,
            attributes: attributes(&function.attributes, scope),
            docblock: signature.docblock,
            templates: signature.templates,
        });
    }
}

fn class_kind(kind: ClassLikeKind) -> ClassKind {
    match kind {
        ClassLikeKind::Interface => ClassKind::Interface,
        ClassLikeKind::Trait => ClassKind::Trait,
        ClassLikeKind::Enum => ClassKind::Enum,
        _ => ClassKind::Class,
    }
}

fn class_name(scope: &Scope, name: &Name) -> Option<Fqn> {
    scope
        .resolve(name, NameContext::Class, &NoDeclarations)
        .fqn()
        .cloned()
}

fn attributes(list: &[Attribute], scope: &Scope) -> Vec<Fqn> {
    list.iter()
        .filter_map(|a| class_name(scope, &a.name))
        .collect()
}

fn visibility(modifiers: &[Modifier]) -> Visibility {
    if modifiers.contains(&Modifier::Private) {
        Visibility::Private
    } else if modifiers.contains(&Modifier::Protected) {
        Visibility::Protected
    } else {
        Visibility::Public
    }
}

fn set_visibility(modifiers: &[Modifier]) -> Option<Visibility> {
    modifiers.iter().find_map(|m| match m {
        Modifier::PublicSet => Some(Visibility::Public),
        Modifier::ProtectedSet => Some(Visibility::Protected),
        Modifier::PrivateSet => Some(Visibility::Private),
        _ => None,
    })
}

fn is_promoted(param: &Param) -> bool {
    !param.hooks.is_empty()
        || param
            .modifiers
            .iter()
            .any(|m| !matches!(m, Modifier::Static | Modifier::Final | Modifier::Abstract))
}

fn promoted_property(param: &Param, decl: &ParamDecl, scope: &Scope) -> PropertyDecl {
    PropertyDecl {
        name: param.name.clone(),
        span: param.span,
        visibility: visibility(&param.modifiers),
        set_visibility: set_visibility(&param.modifiers),
        is_static: false,
        is_readonly: param.modifiers.contains(&Modifier::Readonly),
        ty: decl.ty.clone(),
        has_default: false,
        hooks: param.hooks.iter().map(|h| h.name.clone()).collect(),
        origin: MemberOrigin::Promoted,
        attributes: attributes(&param.attributes, scope),
    }
}

fn trait_use(
    traits: &[Name],
    adaptations: &[bleedthrough_syntax::TraitAdaptation],
    scope: &Scope,
) -> TraitUse {
    let modifier = |m: Modifier| match m {
        Modifier::Private => Some(Visibility::Private),
        Modifier::Protected => Some(Visibility::Protected),
        Modifier::Public => Some(Visibility::Public),
        _ => None,
    };
    let converted = adaptations
        .iter()
        .filter_map(|a| match &a.kind {
            TraitAdaptationKind::Precedence {
                trait_name,
                method,
                instead_of,
            } => Some(Adaptation::InsteadOf {
                trait_name: class_name(scope, trait_name)?,
                method: method.clone(),
                excluded: instead_of
                    .iter()
                    .filter_map(|n| class_name(scope, n))
                    .collect(),
            }),
            TraitAdaptationKind::Alias {
                trait_name,
                method,
                visibility,
                alias,
            } => Some(Adaptation::Alias {
                trait_name: trait_name.as_ref().and_then(|n| class_name(scope, n)),
                method: method.clone(),
                visibility: visibility.and_then(modifier),
                alias: alias.clone(),
            }),
            _ => None,
        })
        .collect();
    TraitUse {
        traits: traits.iter().filter_map(|n| class_name(scope, n)).collect(),
        adaptations: converted,
    }
}

/// Adds `@property*` and `@method` members that no real member of the same name shadows.
fn add_docblock_members(doc: &DocBlock, decl: &mut ClassDecl) {
    for tag in &doc.properties {
        if decl.properties.iter().any(|p| p.name == tag.name) {
            continue;
        }
        decl.properties.push(PropertyDecl {
            name: tag.name.clone(),
            span: tag.span,
            visibility: Visibility::Public,
            set_visibility: None,
            is_static: false,
            is_readonly: tag.access == PropertyAccess::Read,
            ty: EffectiveType {
                ty: tag.ty.clone(),
                source: TypeSource::Docblock,
            },
            has_default: false,
            hooks: Vec::new(),
            origin: MemberOrigin::Docblock,
            attributes: Vec::new(),
        });
    }
    for tag in &doc.methods {
        if decl
            .methods
            .iter()
            .any(|m| m.name.eq_ignore_ascii_case(&tag.name))
        {
            continue;
        }
        let ret = if tag.ret.has_unknown() {
            EffectiveType::absent()
        } else {
            EffectiveType {
                ty: tag.ret.clone(),
                source: TypeSource::Docblock,
            }
        };
        decl.methods.push(MethodDecl {
            name: tag.name.clone(),
            span: tag.span,
            visibility: Visibility::Public,
            is_static: tag.is_static,
            is_final: false,
            is_abstract: false,
            params: tag.params.iter().map(doc_method_param).collect(),
            ret,
            by_ref: false,
            origin: MemberOrigin::Docblock,
            attributes: Vec::new(),
            docblock: None,
            templates: Vec::new(),
        });
    }
}

fn doc_method_param(param: &DocMethodParam) -> ParamDecl {
    ParamDecl {
        name: param.name.clone(),
        ty: param
            .ty
            .clone()
            .map_or_else(EffectiveType::absent, |ty| EffectiveType {
                ty,
                source: TypeSource::Docblock,
            }),
        by_ref: param.by_ref,
        variadic: param.variadic,
        has_default: param.optional,
        attributes: Vec::new(),
    }
}

fn is_null_literal(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Literal(Literal::Null) => true,
        ExprKind::ConstFetch(name) => name.text.eq_ignore_ascii_case("null"),
        ExprKind::Parenthesized(inner) => is_null_literal(inner),
        _ => false,
    }
}

/// The value of a plain string literal, when it has no interpolation.
pub(crate) fn string_value(expr: &Expr) -> Option<String> {
    use bleedthrough_syntax::QuoteKind;
    let ExprKind::Literal(Literal::String { raw, quote }) = &expr.kind else {
        return None;
    };
    let inner = raw
        .strip_prefix(['\'', '"'])
        .and_then(|r| r.strip_suffix(['\'', '"']))
        .unwrap_or(raw);
    match quote {
        QuoteKind::Single => Some(inner.replace("\\\\", "\\").replace("\\'", "'")),
        _ if !inner.contains('\\') => Some(inner.to_string()),
        _ => None,
    }
}

pub(crate) fn int_value(raw: &str) -> Option<i64> {
    let clean: String = raw.chars().filter(|c| *c != '_').collect();
    let lower = clean.to_ascii_lowercase();
    if let Some(hex) = lower.strip_prefix("0x") {
        i64::from_str_radix(hex, 16).ok()
    } else if let Some(bin) = lower.strip_prefix("0b") {
        i64::from_str_radix(bin, 2).ok()
    } else if let Some(oct) = lower.strip_prefix("0o") {
        i64::from_str_radix(oct, 8).ok()
    } else if lower.len() > 1 && lower.starts_with('0') {
        i64::from_str_radix(&lower[1..], 8).ok()
    } else {
        lower.parse().ok()
    }
}

/// The type of a constant expression when it is a simple literal, else `unknown`.
pub(crate) fn literal_type(expr: &Expr) -> Type {
    match &expr.kind {
        ExprKind::Literal(Literal::Int { raw }) => {
            int_value(raw).map_or_else(Type::int, |v| Type::of(Atom::IntLiteral(v)))
        }
        ExprKind::Literal(Literal::Float { .. }) => Type::float(),
        ExprKind::Literal(Literal::String { .. }) => string_value(expr)
            .map_or_else(Type::string, |s| Type::of(Atom::StringLiteral(s.into()))),
        ExprKind::Literal(Literal::Bool(b)) => Type::of(if *b { Atom::True } else { Atom::False }),
        ExprKind::Literal(Literal::Null) => Type::null(),
        ExprKind::ConstFetch(name) => match name.text.to_ascii_lowercase().as_str() {
            "true" => Type::of(Atom::True),
            "false" => Type::of(Atom::False),
            "null" => Type::null(),
            _ => Type::unknown(),
        },
        ExprKind::Unary {
            op: UnaryOp::Neg,
            operand,
        } => match &operand.kind {
            ExprKind::Literal(Literal::Int { raw }) => int_value(raw)
                .and_then(i64::checked_neg)
                .map_or_else(Type::int, |v| Type::of(Atom::IntLiteral(v))),
            ExprKind::Literal(Literal::Float { .. }) => Type::float(),
            _ => Type::unknown(),
        },
        ExprKind::Array { .. } => Type::array_of(Type::array_key(), Type::mixed()),
        ExprKind::Parenthesized(inner) => literal_type(inner),
        _ => Type::unknown(),
    }
}

/// The class names a summary's declarations mention, sorted and distinct.
pub fn declaration_classes(summary: &FileSummary) -> Vec<Fqn> {
    let mut names: Vec<Fqn> = Vec::new();
    for class in &summary.classes {
        names.extend(class.parent.iter().cloned());
        names.extend(class.interfaces.iter().cloned());
        names.extend(
            class
                .trait_uses
                .iter()
                .flat_map(|u| u.traits.iter().cloned()),
        );
        names.extend(class.attributes.iter().cloned());
        let types = class
            .inherits
            .iter()
            .chain(class.backing_type.iter())
            .chain(class.constants.iter().map(|c| &c.ty))
            .chain(class.properties.iter().map(|p| &p.ty.ty))
            .chain(class.methods.iter().flat_map(|m| {
                m.params
                    .iter()
                    .map(|p| &p.ty.ty)
                    .chain(std::iter::once(&m.ret.ty))
            }));
        types.for_each(|t| t.collect_class_names(&mut names));
    }
    for function in &summary.functions {
        for t in function
            .params
            .iter()
            .map(|p| &p.ty.ty)
            .chain(std::iter::once(&function.ret.ty))
        {
            t.collect_class_names(&mut names);
        }
    }
    let set: BTreeSet<Fqn> = names.into_iter().collect();
    set.into_iter().collect()
}

/// The classes (all, then declaration-level) and functions a file refers to.
type Found = (Vec<Fqn>, Vec<Fqn>, Vec<FunctionRef>);

/// Collects the classes and functions a file refers to.
fn references(tree: &SyntaxTree, scopes: &FileScopes, summary: &FileSummary) -> Found {
    let mut refs = Refs {
        scopes,
        classes: BTreeSet::new(),
        functions: BTreeSet::new(),
        skip: BTreeSet::new(),
    };
    for stmt in tree.statements() {
        walk(NodeRef::Stmt(stmt), &mut refs);
    }
    let declared = declaration_classes(summary);
    refs.classes.extend(declared.iter().cloned());
    (
        refs.classes.into_iter().collect(),
        declared,
        refs.functions.into_iter().collect(),
    )
}

struct Refs<'a> {
    scopes: &'a FileScopes,
    classes: BTreeSet<Fqn>,
    functions: BTreeSet<FunctionRef>,
    /// Names already accounted for as something other than a class: `(start, end)`.
    skip: BTreeSet<(u32, u32)>,
}

impl Refs<'_> {
    fn key(name: &Name) -> (u32, u32) {
        (name.span.start, name.span.end)
    }

    fn function(&mut self, name: &Name) {
        let scope = self.scopes.at(name.span.start);
        match scope.resolve(name, NameContext::Function, &NoDeclarations) {
            ResolvedName::Resolved(primary) => {
                self.functions.insert(FunctionRef {
                    primary,
                    fallback: None,
                });
            }
            ResolvedName::FunctionOrConst { namespaced, global } => {
                self.functions.insert(FunctionRef {
                    primary: namespaced,
                    fallback: Some(global),
                });
            }
            _ => {}
        }
    }

    fn class(&mut self, name: &Name) {
        let special = ["self", "static", "parent"]
            .iter()
            .any(|s| name.text.eq_ignore_ascii_case(s));
        if special && name.kind == bleedthrough_syntax::NameKind::Unqualified {
            return;
        }
        if let Some(fqn) = class_name(self.scopes.at(name.span.start), name) {
            self.classes.insert(fqn);
        }
    }
}

impl Visitor for Refs<'_> {
    fn enter(&mut self, node: NodeRef<'_>) -> bool {
        match node {
            NodeRef::Stmt(Stmt {
                kind:
                    StmtKind::Namespace {
                        name: Some(name), ..
                    },
                ..
            }) => {
                self.skip.insert(Self::key(name));
            }
            NodeRef::UseItem(item) => {
                self.skip.insert(Self::key(&item.name));
            }
            NodeRef::Expr(expr) => match &expr.kind {
                ExprKind::ConstFetch(name) => {
                    self.skip.insert(Self::key(name));
                }
                ExprKind::Call {
                    target: CallTarget::Function(callee),
                    ..
                }
                | ExprKind::CallableRef {
                    target: CallTarget::Function(callee),
                } => {
                    if let ExprKind::Name(name) = &callee.kind {
                        self.skip.insert(Self::key(name));
                        self.function(name);
                    }
                }
                _ => {}
            },
            NodeRef::Name(name) if !self.skip.contains(&Self::key(name)) => self.class(name),
            _ => {}
        }
        true
    }
}
