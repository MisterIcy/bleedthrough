//! Declaration extraction: one stub file's syntax tree becomes records of one PHP version.
//!
//! Types and default values are read from the tree and the source text, class names are resolved
//! to fully qualified names, build guards and docblock annotations are attached, and anything that
//! cannot be turned into a record is reported as an error with its file and byte offset.

use std::collections::{BTreeMap, BTreeSet};

use bleedthrough_builtins::{
    CaseDetail, ClassDetail, ConstantDetail, Deprecation, Detail, FunctionDetail, MethodDetail,
    Param, PhpVersion, PropertyDetail, Record, Signature, VersionMask, Visibility,
};
use bleedthrough_syntax::{
    Arg, ArgKind, Attribute, ClassLike, ClassLikeKind, Comment, ConstItem, Expr, FunctionLike,
    Member, MemberKind, Modifier, Name, NameKind, NodeRef, ParsedFile, Span, Stmt, StmtKind,
    TypeHint, TypeKind, UseKind, docblock_before,
};

use crate::annotations::{Docblock, TAGS, Use};
use crate::guards::GuardMap;
use crate::origin::Origin;

/// A record found in a stub file, with where it was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extracted {
    pub record: Record,
    /// The file, relative to the release tree.
    pub file: String,
    /// The build guard that holds at the declaration, `None` when unguarded.
    pub guard: Option<String>,
}

/// A problem that stops generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenError {
    pub file: String,
    pub offset: u32,
    pub message: String,
}

impl std::fmt::Display for GenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}: {}", self.file, self.offset, self.message)
    }
}

/// The result of extracting one file.
#[derive(Debug, Default)]
pub struct FileExtraction {
    pub items: Vec<Extracted>,
    pub errors: Vec<GenError>,
}

/// Attributes the stubs use, split by what they mean for the data.
const KNOWN_ATTRIBUTES: &[&str] = &[
    "Deprecated",
    "SensitiveParameter",
    "NoDiscard",
    "Attribute",
    "AllowDynamicProperties",
    "ReturnTypeWillChange",
    "Override",
    "Closure",
];

/// Extracts the records of one parsed stub file.
pub fn extract_file(
    file: &str,
    source: &[u8],
    parsed: &ParsedFile,
    version: PhpVersion,
    origin: &Origin,
) -> FileExtraction {
    let comments = parsed.tree().comments();
    let guards = match GuardMap::scan(comments) {
        Ok(guards) => guards,
        Err(e) => {
            return FileExtraction {
                items: Vec::new(),
                errors: vec![GenError {
                    file: file.to_string(),
                    offset: e.offset,
                    message: e.message,
                }],
            };
        }
    };
    let mut cx = Cx {
        file,
        source,
        comments,
        guards,
        mask: VersionMask::of(version),
        origin,
        namespace: String::new(),
        uses: BTreeMap::new(),
        consumed: BTreeSet::new(),
        readonly_class: false,
        out: FileExtraction::default(),
    };
    cx.statements(parsed.tree().statements());
    cx.check_vocabulary_and_orphans();
    cx.out
}

struct Cx<'a> {
    file: &'a str,
    source: &'a [u8],
    comments: &'a [Comment],
    guards: GuardMap,
    mask: VersionMask,
    origin: &'a Origin,
    namespace: String,
    /// Imported class names: lowercase alias to the full name.
    uses: BTreeMap<String, String>,
    /// Start offsets of the docblocks attached to a declaration.
    consumed: BTreeSet<u32>,
    /// Inside a `readonly` class, whose properties are all readonly.
    readonly_class: bool,
    out: FileExtraction,
}

/// What the attributes of a declaration say.
#[derive(Default)]
struct Attrs {
    deprecation: Option<Deprecation>,
    no_discard: bool,
    sensitive: bool,
}

impl Cx<'_> {
    fn error(&mut self, offset: u32, message: impl Into<String>) {
        self.out.errors.push(GenError {
            file: self.file.to_string(),
            offset,
            message: message.into(),
        });
    }

    fn text(&self, span: Span) -> String {
        let bytes = self
            .source
            .get(span.start as usize..span.end as usize)
            .unwrap_or_default();
        String::from_utf8_lossy(bytes).into_owned()
    }

    // ---- statements ----------------------------------------------------------------------

    fn statements(&mut self, statements: &[Stmt]) {
        for stmt in statements {
            match &stmt.kind {
                StmtKind::Namespace { name, braced, body } => {
                    let saved = (self.namespace.clone(), self.uses.clone());
                    self.namespace = name.as_ref().map_or(String::new(), |n| n.text.to_string());
                    self.uses.clear();
                    self.statements(body);
                    if *braced {
                        (self.namespace, self.uses) = saved;
                    }
                }
                StmtKind::Use { items } => {
                    for item in items.iter().filter(|i| i.kind == UseKind::Class) {
                        let alias = item.alias.clone().unwrap_or_else(|| {
                            item.name
                                .text
                                .rsplit('\\')
                                .next()
                                .unwrap_or_default()
                                .into()
                        });
                        self.uses
                            .insert(alias.to_ascii_lowercase(), item.name.text.to_string());
                    }
                }
                StmtKind::OpenTag { .. } | StmtKind::Noop | StmtKind::Declare { .. } => {}
                // `require "zend_constants.stub.php";` only orders the stub generator's input.
                StmtKind::Expr(expr) if NodeRef::Expr(expr).kind_name() == "Include" => {}
                StmtKind::Function(function) => self.function(stmt.span, function),
                StmtKind::Const { attributes, items } => {
                    self.global_constants(stmt.span, attributes, items);
                }
                StmtKind::ClassLike(class) => self.class_like(stmt.span, class),
                _ => {
                    let kind = NodeRef::Stmt(stmt).kind_name();
                    self.error(
                        stmt.span.start,
                        format!("unexpected `{kind}` statement in a stub"),
                    );
                }
            }
        }
    }

    /// The start of a declaration including its attributes, where a docblock may precede it.
    fn start_with(&self, span: Span, attributes: &[Attribute]) -> u32 {
        attributes
            .iter()
            .map(|a| a.span.start)
            .fold(span.start, u32::min)
    }

    /// The docblock attached to the declaration starting at `start`, marking it as used.
    fn docblock(&mut self, start: u32) -> Docblock {
        match docblock_before(self.comments, self.source, start) {
            Some(comment) => {
                self.consumed.insert(comment.span.start);
                Docblock::parse(&comment.text)
            }
            None => Docblock::default(),
        }
    }

    fn guard(&mut self, span: Span, class_like: bool) -> Option<String> {
        if !class_like && self.guards.changes_within(span.start, span.end) {
            self.error(
                span.start,
                "a build guard directive lies inside a declaration",
            );
        }
        self.guards.at(span.start).map(str::to_string)
    }

    fn push(
        &mut self,
        mut record: Record,
        guard: Option<String>,
        deprecation: Option<Deprecation>,
    ) {
        record.extension.clone_from(&self.origin.extension);
        record.sapi = self.origin.sapi.clone().into_iter().collect();
        record.deprecation = deprecation;
        self.out.items.push(Extracted {
            record,
            file: self.file.to_string(),
            guard,
        });
    }

    fn qualified(&self, name: &str) -> String {
        if self.namespace.is_empty() {
            name.to_string()
        } else {
            format!("{}\\{name}", self.namespace)
        }
    }

    // ---- names and types -----------------------------------------------------------------

    fn resolve(&self, name: &Name) -> String {
        match name.kind {
            NameKind::FullyQualified => name.text.to_string(),
            NameKind::Relative => self.qualified(&name.text),
            _ => {
                let (first, rest) = match name.text.split_once('\\') {
                    Some((first, rest)) => (first, Some(rest)),
                    None => (&*name.text, None),
                };
                match (self.uses.get(&first.to_ascii_lowercase()), rest) {
                    (Some(full), Some(rest)) => format!("{full}\\{rest}"),
                    (Some(full), None) => full.clone(),
                    (None, _) => self.qualified(&name.text),
                }
            }
        }
    }

    fn type_text(&mut self, hint: &TypeHint) -> String {
        match &hint.kind {
            TypeKind::Named(name) => self.resolve(name),
            TypeKind::Builtin(_) => self.text(hint.span).to_ascii_lowercase(),
            TypeKind::Nullable(inner) => format!("?{}", self.type_text(inner)),
            TypeKind::Union(parts) => self.join_types(parts, "|"),
            TypeKind::Intersection(parts) => self.join_types(parts, "&"),
            TypeKind::Parenthesized(inner) => format!("({})", self.type_text(inner)),
            TypeKind::Error { .. } | TypeKind::Unsupported { .. } => {
                self.error(hint.span.start, "a type could not be read");
                String::new()
            }
            _ => {
                self.error(hint.span.start, "a type form the generator does not know");
                String::new()
            }
        }
    }

    fn join_types(&mut self, parts: &[TypeHint], separator: &str) -> String {
        parts
            .iter()
            .map(|p| self.type_text(p))
            .collect::<Vec<_>>()
            .join(separator)
    }

    fn opt_type(&mut self, hint: Option<&TypeHint>) -> String {
        hint.map(|h| self.type_text(h)).unwrap_or_default()
    }

    /// A docblock type with the class names of a namespaced file resolved.
    fn doc_type(&self, text: &str) -> String {
        if self.namespace.is_empty() && self.uses.is_empty() {
            return text.to_string();
        }
        let mut out = String::new();
        let mut word = String::new();
        let flush = |word: &mut String, out: &mut String, next: Option<char>| {
            if !word.is_empty() {
                let starts_upper = word.chars().next().is_some_and(|c| c.is_ascii_uppercase());
                if (starts_upper || word.contains('\\')) && next != Some(':') {
                    let kind = if word.starts_with('\\') {
                        NameKind::FullyQualified
                    } else if word.contains('\\') {
                        NameKind::Qualified
                    } else {
                        NameKind::Unqualified
                    };
                    let name = Name {
                        span: Span::new(0, 0),
                        kind,
                        text: word.trim_start_matches('\\').into(),
                    };
                    out.push_str(&self.resolve(&name));
                } else {
                    out.push_str(word);
                }
                word.clear();
            }
        };
        let chars: Vec<char> = text.chars().collect();
        for (i, c) in chars.iter().enumerate() {
            if c.is_alphanumeric() || *c == '_' || *c == '\\' || (*c == '-' && !word.is_empty()) {
                word.push(*c);
            } else {
                flush(&mut word, &mut out, Some(*c));
                out.push(*c);
            }
            let _ = i;
        }
        flush(&mut word, &mut out, None);
        out
    }

    /// Source text of an expression with whitespace collapsed outside string literals.
    fn expr_text(&self, expr: &Expr) -> String {
        collapse(&self.text(expr.span))
    }

    /// A default or constant value as data: `UNKNOWN` (a value only C knows) is no value.
    fn value_text(&self, expr: &Expr) -> Option<String> {
        let text = self.expr_text(expr);
        (text != "UNKNOWN").then_some(text)
    }

    // ---- attributes ------------------------------------------------------------------------

    fn attrs(&mut self, attributes: &[Attribute]) -> Attrs {
        let mut out = Attrs::default();
        for attribute in attributes {
            let name = attribute.name.text.trim_start_matches('\\').to_string();
            if !KNOWN_ATTRIBUTES.contains(&name.as_str()) {
                self.error(attribute.span.start, format!("unknown attribute `{name}`"));
                continue;
            }
            match name.as_str() {
                "Deprecated" => out.deprecation = Some(self.deprecation(&attribute.args)),
                "NoDiscard" => out.no_discard = true,
                "SensitiveParameter" => out.sensitive = true,
                _ => {}
            }
        }
        out
    }

    fn deprecation(&self, args: &[Arg]) -> Deprecation {
        let mut deprecation = Deprecation::default();
        for (position, arg) in args.iter().enumerate() {
            let (name, value) = match &arg.kind {
                ArgKind::Named { name, value } => (&**name, value),
                ArgKind::Positional(value) => {
                    (if position == 0 { "message" } else { "since" }, value)
                }
                _ => continue,
            };
            let text = unquote(&self.text(value.span));
            match name {
                "since" => deprecation.since = text,
                "message" => deprecation.message = text,
                _ => {}
            }
        }
        deprecation
    }

    // ---- functions -------------------------------------------------------------------------

    fn signature(&mut self, function: &FunctionLike, doc: &Docblock, span: Span) -> Signature {
        let mut params = Vec::new();
        for param in &function.params {
            let attrs = self.attrs(&param.attributes);
            let doc_type = match doc.param_type(&param.name) {
                Ok(t) => t.map(|t| self.doc_type(&t)).unwrap_or_default(),
                Err(e) => {
                    self.error(span.start, format!("@param ${}: {e}", param.name));
                    String::new()
                }
            };
            let default = param.default.as_ref().and_then(|d| self.value_text(d));
            params.push(Param {
                name: param.name.to_string(),
                type_text: self.opt_type(param.type_hint.as_ref()),
                doc_type,
                by_ref: param.by_ref,
                optional: param.default.is_some() || param.variadic,
                default,
                variadic: param.variadic,
                sensitive: attrs.sensitive,
            });
        }
        let declared = self.opt_type(function.return_type.as_ref());
        let tentative = doc.has("tentative-return-type");
        let doc_return = match doc.type_of("return") {
            Ok(t) => t.map(|t| self.doc_type(&t)).unwrap_or_default(),
            Err(e) => {
                self.error(span.start, format!("@return: {e}"));
                String::new()
            }
        };
        Signature {
            params,
            return_type: if tentative {
                String::new()
            } else {
                declared.clone()
            },
            tentative_return_type: if tentative { declared } else { String::new() },
            doc_return_type: doc_return,
            return_by_ref: function.by_ref,
        }
    }

    fn deprecation_of(attrs: &Attrs, doc: &Docblock) -> Option<Deprecation> {
        attrs
            .deprecation
            .clone()
            .or_else(|| doc.has("deprecated").then(Deprecation::default))
    }

    fn alias_of(doc: &Docblock) -> Option<String> {
        ["alias", "implementation-alias"]
            .iter()
            .find_map(|t| doc.tag(t))
            .and_then(|t| t.rest.split_whitespace().next())
            .map(str::to_string)
    }

    fn function(&mut self, span: Span, function: &FunctionLike) {
        let start = self.start_with(span, &function.attributes);
        let doc = self.docblock(start);
        let attrs = self.attrs(&function.attributes);
        let guard = self.guard(Span::new(start, span.end), false);
        let detail = FunctionDetail {
            signature: self.signature(function, &doc, span),
            alias_of: Self::alias_of(&doc),
            no_discard: attrs.no_discard,
        };
        let deprecation = Self::deprecation_of(&attrs, &doc);
        let record = Record::new(
            self.qualified(&function.name),
            self.mask,
            "",
            Detail::Function(detail),
        );
        self.push(record, guard, deprecation);
    }

    // ---- constants -------------------------------------------------------------------------

    fn global_constants(&mut self, span: Span, attributes: &[Attribute], items: &[ConstItem]) {
        let start = self.start_with(span, attributes);
        let doc = self.docblock(start);
        let attrs = self.attrs(attributes);
        let guard = self.guard(Span::new(start, span.end), false);
        let type_text = self.var_type(&doc, span);
        for item in items {
            let detail = ConstantDetail {
                type_text: type_text.clone(),
                value: if doc.has("cvalue") {
                    None
                } else {
                    self.value_text(&item.value)
                },
                visibility: Visibility::Public,
                is_final: false,
            };
            let deprecation = Self::deprecation_of(&attrs, &doc);
            let record = Record::new(
                self.qualified(&item.name),
                self.mask,
                "",
                Detail::Constant(detail),
            );
            self.push(record, guard.clone(), deprecation);
        }
    }

    fn var_type(&mut self, doc: &Docblock, span: Span) -> String {
        match doc.type_of("var") {
            Ok(t) => t.map(|t| self.doc_type(&t)).unwrap_or_default(),
            Err(e) => {
                self.error(span.start, format!("@var: {e}"));
                String::new()
            }
        }
    }

    // ---- classes ---------------------------------------------------------------------------

    fn class_like(&mut self, span: Span, class: &ClassLike) {
        let start = self.start_with(span, &class.attributes);
        let doc = self.docblock(start);
        let attrs = self.attrs(&class.attributes);
        let guard = self.guard(span, true);
        let Some(short) = class.name.as_deref() else {
            self.error(span.start, "an anonymous class in a stub");
            return;
        };
        let name = self.qualified(short);
        let detail = ClassDetail {
            is_abstract: class.modifiers.contains(&Modifier::Abstract),
            is_final: class.modifiers.contains(&Modifier::Final),
            is_readonly: class.modifiers.contains(&Modifier::Readonly),
            extends: class.extends.iter().map(|n| self.resolve(n)).collect(),
            implements: class.implements.iter().map(|n| self.resolve(n)).collect(),
            backing_type: self.opt_type(class.backing_type.as_ref()),
        };
        let detail = match class.kind {
            ClassLikeKind::Class => Detail::Class(detail),
            ClassLikeKind::Interface => Detail::Interface(detail),
            ClassLikeKind::Trait => Detail::Trait(detail),
            ClassLikeKind::Enum => Detail::Enum(detail),
            _ => {
                self.error(span.start, "a class-like kind the generator does not know");
                return;
            }
        };
        let deprecation = Self::deprecation_of(&attrs, &doc);
        let record = Record::new(name.clone(), self.mask, "", detail);
        self.push(record, guard, deprecation);
        self.readonly_class = class.modifiers.contains(&Modifier::Readonly);
        for member in &class.members {
            self.member(&name, member);
        }
        self.readonly_class = false;
    }

    fn member(&mut self, class: &str, member: &Member) {
        match &member.kind {
            MemberKind::Const { .. } => self.class_constants(class, member),
            MemberKind::Property { .. } => self.properties(class, member),
            MemberKind::Method(function) => self.method(class, member.span, function),
            MemberKind::EnumCase { .. } => self.enum_case(class, member),
            MemberKind::TraitUse { .. } => {
                self.error(member.span.start, "a trait use in a stub is not modelled");
            }
            MemberKind::Error { .. } | MemberKind::Unsupported { .. } => {
                self.error(member.span.start, "a member could not be read");
            }
            _ => self.error(
                member.span.start,
                "a member kind the generator does not know",
            ),
        }
    }

    fn class_constants(&mut self, class: &str, member: &Member) {
        let MemberKind::Const {
            attributes,
            modifiers,
            type_hint,
            items,
        } = &member.kind
        else {
            return;
        };
        let span = member.span;
        let type_hint = type_hint.as_ref();
        let start = self.start_with(span, attributes);
        let doc = self.docblock(start);
        let attrs = self.attrs(attributes);
        let guard = self.guard(Span::new(start, span.end), false);
        let declared = self.opt_type(type_hint);
        let type_text = if declared.is_empty() {
            self.var_type(&doc, span)
        } else {
            declared
        };
        for item in items {
            let detail = ConstantDetail {
                type_text: type_text.clone(),
                value: if doc.has("cvalue") {
                    None
                } else {
                    self.value_text(&item.value)
                },
                visibility: visibility(modifiers),
                is_final: modifiers.contains(&Modifier::Final),
            };
            let record = Record::new(
                format!("{class}::{}", item.name),
                self.mask,
                "",
                Detail::ClassConstant(detail),
            );
            self.push(record, guard.clone(), Self::deprecation_of(&attrs, &doc));
        }
    }

    fn properties(&mut self, class: &str, member: &Member) {
        let MemberKind::Property {
            attributes,
            modifiers,
            type_hint,
            items,
            ..
        } = &member.kind
        else {
            return;
        };
        let span = member.span;
        let type_hint = type_hint.as_ref();
        let start = self.start_with(span, attributes);
        let doc = self.docblock(start);
        let attrs = self.attrs(attributes);
        let guard = self.guard(Span::new(start, span.end), false);
        let type_text = self.opt_type(type_hint);
        let doc_type = self.var_type(&doc, span);
        for item in items {
            let detail = PropertyDetail {
                type_text: type_text.clone(),
                doc_type: doc_type.clone(),
                default: item.default.as_ref().and_then(|d| self.value_text(d)),
                visibility: visibility(modifiers),
                is_static: modifiers.contains(&Modifier::Static),
                is_readonly: self.readonly_class
                    || modifiers.contains(&Modifier::Readonly)
                    || doc.has("readonly"),
                is_virtual: doc.has("virtual"),
            };
            let record = Record::new(
                format!("{class}::{}", item.name),
                self.mask,
                "",
                Detail::Property(detail),
            );
            self.push(record, guard.clone(), Self::deprecation_of(&attrs, &doc));
        }
    }

    fn method(&mut self, class: &str, span: Span, function: &FunctionLike) {
        let start = self.start_with(span, &function.attributes);
        let doc = self.docblock(start);
        let attrs = self.attrs(&function.attributes);
        let guard = self.guard(Span::new(start, span.end), false);
        let detail = MethodDetail {
            signature: self.signature(function, &doc, span),
            alias_of: Self::alias_of(&doc),
            no_discard: attrs.no_discard,
            visibility: visibility(&function.modifiers),
            is_static: function.modifiers.contains(&Modifier::Static),
            is_final: function.modifiers.contains(&Modifier::Final),
            is_abstract: function.modifiers.contains(&Modifier::Abstract),
        };
        let record = Record::new(
            format!("{class}::{}", function.name),
            self.mask,
            "",
            Detail::Method(detail),
        );
        self.push(record, guard, Self::deprecation_of(&attrs, &doc));
    }

    fn enum_case(&mut self, class: &str, member: &Member) {
        let MemberKind::EnumCase {
            attributes,
            name,
            value,
        } = &member.kind
        else {
            return;
        };
        let span = member.span;
        let value = value.as_ref();
        let start = self.start_with(span, attributes);
        let doc = self.docblock(start);
        let attrs = self.attrs(attributes);
        let guard = self.guard(Span::new(start, span.end), false);
        let detail = CaseDetail {
            value: value.and_then(|v| self.value_text(v)),
        };
        let record = Record::new(
            format!("{class}::{name}"),
            self.mask,
            "",
            Detail::EnumCase(detail),
        );
        self.push(record, guard, Self::deprecation_of(&attrs, &doc));
    }

    // ---- vocabulary ------------------------------------------------------------------------

    /// Every docblock must use only known tags, and a docblock with facts must belong to a
    /// declaration (a fact in a docblock that nothing consumed would be lost silently).
    fn check_vocabulary_and_orphans(&mut self) {
        let comments = self.comments;
        for comment in comments
            .iter()
            .filter(|c| c.kind == bleedthrough_syntax::CommentKind::Doc)
        {
            let doc = Docblock::parse(&comment.text);
            for tag in &doc.tags {
                match TAGS.iter().find(|(name, _)| *name == tag.name) {
                    None => self.error(
                        comment.span.start,
                        format!("unknown annotation `@{}`", tag.name),
                    ),
                    Some((_, Use::Fact)) if !self.consumed.contains(&comment.span.start) => self
                        .error(
                            comment.span.start,
                            format!("`@{}` in a docblock that precedes no declaration", tag.name),
                        ),
                    Some(_) => {}
                }
            }
        }
    }
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

/// Removes the quotes of a PHP string literal; text that is not one is returned as written.
fn unquote(text: &str) -> String {
    let text = text.trim();
    let quote = text.chars().next();
    if matches!(quote, Some('\'' | '"')) && text.len() >= 2 && text.ends_with(quote.unwrap_or('\''))
    {
        let inner = &text[1..text.len() - 1];
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some(n @ ('\\' | '\'' | '"')) => out.push(n),
                    Some(n) => {
                        out.push('\\');
                        out.push(n);
                    }
                    None => out.push('\\'),
                }
            } else {
                out.push(c);
            }
        }
        out
    } else {
        text.to_string()
    }
}

/// Collapses runs of whitespace to one space, except inside string literals.
fn collapse(text: &str) -> String {
    let mut out = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut pending_space = false;
    for c in text.trim().chars() {
        if let Some(q) = quote {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
        } else if c.is_whitespace() {
            pending_space = true;
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            if c == '\'' || c == '"' {
                quote = Some(c);
            }
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitespace_collapses_outside_strings_only() {
        assert_eq!(collapse("  A  |\n   B "), "A | B");
        assert_eq!(collapse("'a   b'  .  \"c  d\""), "'a   b' . \"c  d\"");
        assert_eq!(collapse(r"'it\'s  x'  +  1"), r"'it\'s  x' + 1");
    }

    #[test]
    fn string_literals_lose_their_quotes() {
        assert_eq!(unquote("'8.4'"), "8.4");
        assert_eq!(unquote(r"'it\'s'"), "it's");
        assert_eq!(unquote("PHP_X"), "PHP_X");
    }
}
