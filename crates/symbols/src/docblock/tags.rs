//! The typed content of a docblock: parameter, return, variable, property, method and template tags.

use bleedthrough_syntax::Span;

use super::split::{DocTag, split_tags};
use super::typeparse::{Locator, TypeContext, TypeParse, parse_prefix};
use crate::diag::{SymbolDiagnostic, SymbolDiagnosticCode};
use crate::types::Type;

/// Term: Type
///
/// A `@param` tag.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct DocParam {
    /// The parameter name without `$`.
    pub name: Box<str>,
    /// The documented type, if the tag gave one that parsed.
    pub ty: Option<Type>,
    /// Whether the tag marks the parameter variadic (`...$name`).
    pub variadic: bool,
    /// Whether the tag marks the parameter by reference (`&$name`).
    pub by_ref: bool,
    /// Where the tag is.
    pub span: Span,
}

/// Term: Type
///
/// A `@return` tag.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct DocReturn {
    /// The documented type.
    pub ty: Type,
    /// Where the tag is.
    pub span: Span,
}

/// Term: Type
///
/// A `@var` tag.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct DocVar {
    /// The variable name without `$`, when the tag names one.
    pub name: Option<Box<str>>,
    /// The documented type.
    pub ty: Type,
    /// Where the tag is.
    pub span: Span,
}

/// Term: Type
///
/// Which accesses a `@property` tag declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PropertyAccess {
    /// `@property`
    ReadWrite,
    /// `@property-read`
    Read,
    /// `@property-write`
    Write,
}

/// Term: Type
///
/// A `@property`, `@property-read` or `@property-write` tag.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct DocProperty {
    /// The property name without `$`.
    pub name: Box<str>,
    /// The documented type.
    pub ty: Type,
    /// The declared accesses.
    pub access: PropertyAccess,
    /// Where the tag is.
    pub span: Span,
}

/// Term: Type
///
/// A parameter of a `@method` tag.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct DocMethodParam {
    /// The parameter name without `$`.
    pub name: Box<str>,
    /// Its type, if given.
    pub ty: Option<Type>,
    /// Whether it is variadic.
    pub variadic: bool,
    /// Whether it is by reference.
    pub by_ref: bool,
    /// Whether it has a default.
    pub optional: bool,
}

/// Term: Type
///
/// A `@method` tag: a magic method.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct DocMethod {
    /// The method name.
    pub name: Box<str>,
    /// Whether the tag says `static`.
    pub is_static: bool,
    /// The return type; unknown when the tag gave none.
    pub ret: Type,
    /// The parameters.
    pub params: Vec<DocMethodParam>,
    /// Where the tag is.
    pub span: Span,
}

/// Term: Type
///
/// A `@template` declaration, recorded for substitution.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct DocTemplate {
    /// The template parameter name.
    pub name: Box<str>,
    /// Its upper bound (`of Bound`), if any.
    pub bound: Option<Type>,
}

/// Term: Type
///
/// Everything the supported tags of one docblock say, with diagnostics for what did not parse.
#[derive(Debug, Clone, PartialEq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct DocBlock {
    /// `@param` tags in order.
    pub params: Vec<DocParam>,
    /// The `@return` tag.
    pub returns: Option<DocReturn>,
    /// `@var` tags in order.
    pub vars: Vec<DocVar>,
    /// `@property*` tags in order.
    pub properties: Vec<DocProperty>,
    /// `@method` tags in order.
    pub methods: Vec<DocMethod>,
    /// `@template*` declarations in order.
    pub templates: Vec<DocTemplate>,
    /// Types named by `@extends`, `@implements` and `@use`, with their type arguments.
    pub inherits: Vec<Type>,
    /// Errors and notes.
    pub diagnostics: Vec<SymbolDiagnostic>,
}

/// Parses the tags of the docblock `comment`, which starts at file offset `start`.
pub fn parse_docblock(comment: &str, start: u32, ctx: &TypeContext<'_>) -> DocBlock {
    let tags = split_tags(comment, start);
    let mut names: Vec<Box<str>> = ctx.template_names().to_vec();
    names.extend(
        tags.iter()
            .filter(|t| is_template(&t.name))
            .filter_map(template_name),
    );
    let ctx = ctx.with_templates(&names);
    let mut block = DocBlock::default();
    let mut collector = Collector {
        ctx: &ctx,
        block: &mut block,
        replace: false,
    };
    for tag in tags.iter().filter(|t| !t.prefixed) {
        collector.tag(tag);
    }
    // A `psalm-` or `phpstan-` tag replaces the plain tag for the same subject.
    collector.replace = true;
    for tag in tags.iter().filter(|t| t.prefixed) {
        collector.tag(tag);
    }
    block
}

fn is_template(name: &str) -> bool {
    matches!(
        name,
        "template" | "template-covariant" | "template-contravariant"
    )
}

fn template_name(tag: &DocTag) -> Option<Box<str>> {
    let name = leading_word(&tag.body);
    (!name.is_empty()).then(|| name.into())
}

fn leading_word(text: &str) -> &str {
    let end = text
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(text.len());
    &text[..end]
}

struct Collector<'a, 'c> {
    ctx: &'a TypeContext<'c>,
    block: &'a mut DocBlock,
    replace: bool,
}

impl Collector<'_, '_> {
    fn tag(&mut self, tag: &DocTag) {
        match tag.name.as_str() {
            "param" => self.param(tag),
            "return" => self.returns(tag),
            "var" => self.var(tag),
            "property" => self.property(tag, PropertyAccess::ReadWrite),
            "property-read" => self.property(tag, PropertyAccess::Read),
            "property-write" => self.property(tag, PropertyAccess::Write),
            "method" => self.method(tag),
            "template" | "template-covariant" | "template-contravariant" => self.template(tag),
            "extends"
            | "implements"
            | "use"
            | "template-extends"
            | "template-implements"
            | "template-use" => self.inherit(tag),
            _ => {}
        }
    }

    /// Parses the type at the start of the body, recording its diagnostics.
    fn type_at(&mut self, tag: &DocTag, from: usize) -> Option<(Type, usize)> {
        let text = &tag.body[from..];
        let loc = Locator::mapped(&tag.offsets[from.min(tag.offsets.len())..]);
        let TypeParse {
            ty,
            consumed,
            diagnostics,
        } = parse_prefix(text, self.ctx, loc);
        self.block.diagnostics.extend(diagnostics);
        ty.map(|t| (t, from + consumed))
    }

    fn syntax_error(&mut self, tag: &DocTag, message: &str) {
        self.block.diagnostics.push(SymbolDiagnostic::new(
            SymbolDiagnosticCode::DocblockSyntax,
            message,
            tag.span,
        ));
    }

    fn param(&mut self, tag: &DocTag) {
        let before = self.block.diagnostics.len();
        match self.param_decl(tag, 0) {
            Some((param, _)) => self.put_param(DocParam {
                name: param.name,
                ty: param.ty,
                variadic: param.variadic,
                by_ref: param.by_ref,
                span: tag.span,
            }),
            // A type that failed to parse has already been reported.
            None if self.block.diagnostics.len() == before => {
                self.syntax_error(tag, "`@param` needs a parameter name");
            }
            None => {}
        }
    }

    /// `[type] [&][...]$name` starting at byte `from`; returns the parameter and where it ends.
    fn param_decl(&mut self, tag: &DocTag, from: usize) -> Option<(DocMethodParam, usize)> {
        let rest = &tag.body[from..];
        let starts_bare = rest.starts_with(['$', '&', '.']);
        let (ty, after) = if starts_bare {
            (None, from)
        } else {
            let (ty, end) = self.type_at(tag, from)?;
            (Some(ty), end)
        };
        let tail = &tag.body[after..];
        let trimmed = tail.trim_start();
        let mut position = after + (tail.len() - trimmed.len());
        let mut remaining = trimmed;
        let by_ref = remaining.starts_with('&');
        if by_ref {
            remaining = remaining[1..].trim_start();
            position = tag.body.len() - remaining.len();
        }
        let variadic = remaining.starts_with("...");
        if variadic {
            remaining = &remaining[3..];
            position = tag.body.len() - remaining.len();
        }
        let name = remaining.strip_prefix('$').map(leading_word)?;
        if name.is_empty() {
            return None;
        }
        position += 1 + name.len();
        let optional = tag.body[position..].trim_start().starts_with('=');
        Some((
            DocMethodParam {
                name: name.into(),
                ty,
                variadic,
                by_ref,
                optional,
            },
            position,
        ))
    }

    fn put_param(&mut self, param: DocParam) {
        let slot = self
            .block
            .params
            .iter_mut()
            .find(|p| self.replace && p.name == param.name);
        match slot {
            Some(slot) => *slot = param,
            None => self.block.params.push(param),
        }
    }

    fn returns(&mut self, tag: &DocTag) {
        if tag.body.trim().is_empty() {
            return;
        }
        if let Some((ty, _)) = self.type_at(tag, 0) {
            self.block.returns = Some(DocReturn { ty, span: tag.span });
        }
    }

    fn var(&mut self, tag: &DocTag) {
        let Some((ty, end)) = self.type_at(tag, 0) else {
            return;
        };
        let name = tag.body[end..]
            .trim_start()
            .strip_prefix('$')
            .map(leading_word)
            .filter(|n| !n.is_empty())
            .map(Box::<str>::from);
        let var = DocVar {
            name,
            ty,
            span: tag.span,
        };
        let slot = self
            .block
            .vars
            .iter_mut()
            .find(|v| self.replace && v.name == var.name);
        match slot {
            Some(slot) => *slot = var,
            None => self.block.vars.push(var),
        }
    }

    fn property(&mut self, tag: &DocTag, access: PropertyAccess) {
        let Some((ty, end)) = self.type_at(tag, 0) else {
            return;
        };
        let name = tag.body[end..]
            .trim_start()
            .strip_prefix('$')
            .map(leading_word)
            .filter(|n| !n.is_empty());
        match name {
            Some(name) => self.block.properties.push(DocProperty {
                name: name.into(),
                ty,
                access,
                span: tag.span,
            }),
            None => self.syntax_error(tag, "a property tag needs a `$name`"),
        }
    }

    fn template(&mut self, tag: &DocTag) {
        let name = leading_word(&tag.body);
        if name.is_empty() {
            self.syntax_error(tag, "a template tag needs a name");
            return;
        }
        let after = tag.body[name.len()..].trim_start();
        let keyword = ["of ", "as "].iter().find(|k| after.starts_with(**k));
        let bound = keyword.and_then(|k| {
            let from = tag.body.len() - after.len() + k.len();
            self.type_at(tag, from).map(|(ty, _)| ty)
        });
        self.block.templates.push(DocTemplate {
            name: name.into(),
            bound,
        });
    }

    fn inherit(&mut self, tag: &DocTag) {
        if let Some((ty, _)) = self.type_at(tag, 0) {
            self.block.inherits.push(ty);
        }
    }

    fn method(&mut self, tag: &DocTag) {
        match self.method_decl(tag) {
            Some(method) => self.block.methods.push(method),
            None => self.syntax_error(tag, "cannot read the `@method` signature"),
        }
    }

    fn method_decl(&mut self, tag: &DocTag) -> Option<DocMethod> {
        let mut from = 0usize;
        let mut is_static = false;
        let body = &tag.body;
        if let Some(rest) = body.strip_prefix("static ") {
            let tail = rest.trim_start();
            if !tail.starts_with('(') && looks_like_more_than_a_name(tail) {
                is_static = true;
                from = body.len() - tail.len();
            }
        }
        let (ret, name_at) = if starts_with_method_name(&body[from..]) {
            (Type::unknown(), from)
        } else {
            let (ty, end) = self.type_at(tag, from)?;
            (
                ty,
                end + (body[end..].len() - body[end..].trim_start().len()),
            )
        };
        let name = leading_word(&body[name_at..]);
        let after_name = name_at + name.len();
        if name.is_empty() || !body[after_name..].trim_start().starts_with('(') {
            return None;
        }
        let open = after_name + (body[after_name..].len() - body[after_name..].trim_start().len());
        let params = self.method_params(tag, open + 1)?;
        Some(DocMethod {
            name: name.into(),
            is_static,
            ret,
            params,
            span: tag.span,
        })
    }

    fn method_params(&mut self, tag: &DocTag, mut at: usize) -> Option<Vec<DocMethodParam>> {
        let mut params = Vec::new();
        loop {
            let rest = &tag.body[at..];
            let skipped = rest.len() - rest.trim_start().len();
            at += skipped;
            if tag.body[at..].starts_with(')') {
                return Some(params);
            }
            let (param, end) = self.param_decl(tag, at)?;
            params.push(param);
            at = skip_default(&tag.body, end)?;
            if tag.body[at..].starts_with(',') {
                at += 1;
            } else if !tag.body[at..].starts_with(')') {
                return None;
            }
        }
    }
}

fn looks_like_more_than_a_name(text: &str) -> bool {
    // `static getFoo()` is a method named getFoo; `static Foo getFoo()` has a return type.
    let word = leading_word(text);
    !word.is_empty() && !text[word.len()..].trim_start().starts_with('(')
}

fn starts_with_method_name(text: &str) -> bool {
    let word = leading_word(text);
    let lower = word.to_ascii_lowercase();
    !word.is_empty()
        && lower != "callable"
        && lower != "closure"
        && text[word.len()..].trim_start().starts_with('(')
}

/// Skips an optional `= default` up to the next top-level `,` or `)`.
fn skip_default(body: &str, from: usize) -> Option<usize> {
    let rest = &body[from..];
    let trimmed = rest.trim_start();
    let start = from + (rest.len() - trimmed.len());
    if !trimmed.starts_with('=') {
        return Some(start);
    }
    let mut depth = 0i32;
    for (i, c) in trimmed.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth > 0 => depth -= 1,
            ')' | ',' if depth == 0 => return Some(start + i),
            _ => {}
        }
    }
    None
}
