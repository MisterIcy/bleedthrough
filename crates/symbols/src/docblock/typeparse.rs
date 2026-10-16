//! A recursive-descent parser for the PHPDoc and Psalm type subset, over text pulled lazily from the
//! lexer. It never panics and never loops: every loop consumes a token, and nesting is bounded.

use bleedthrough_syntax::{ClassRefKind, Name, Span};

use super::keywords::{is_unsupported_generic, simple_keyword};
use super::lexer::{Lexed, Lexer, Token};
use crate::diag::{SymbolDiagnostic, SymbolDiagnosticCode};
use crate::name::{Fqn, NameContext, ResolvedName};
use crate::scope::{NoDeclarations, Scope};
use crate::special::ClassContext;
use crate::types::{Atom, CallableParam, CallableSig, NamedObject, ShapeEntry, ShapeKey, Type};

/// Deepest nesting of generics, parentheses and shapes accepted.
const MAX_DEPTH: usize = 24;

/// Term: Type
///
/// What the type parser needs to turn written names into types: the import scope, the enclosing
/// class-like (for `self`, `static`, `parent`) and the template parameters in scope.
#[derive(Debug, Clone, Copy, Default)]
pub struct TypeContext<'a> {
    scope: Option<&'a Scope>,
    class: Option<&'a ClassContext>,
    templates: &'a [Box<str>],
}

impl<'a> TypeContext<'a> {
    /// A context in which names are taken as fully qualified and `self` is unresolvable.
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolves names through `scope`.
    pub fn with_scope(mut self, scope: &'a Scope) -> Self {
        self.scope = Some(scope);
        self
    }

    /// Resolves `self`, `static` and `parent` against `class`.
    pub fn with_class(mut self, class: &'a ClassContext) -> Self {
        self.class = Some(class);
        self
    }

    /// The template parameter names in scope.
    pub(super) fn template_names(&self) -> &'a [Box<str>] {
        self.templates
    }

    /// Treats these names as template parameters.
    pub fn with_templates(mut self, templates: &'a [Box<str>]) -> Self {
        self.templates = templates;
        self
    }
}

/// Where the bytes of a piece of text lie in the file, so diagnostics carry file offsets.
#[derive(Debug, Clone, Copy)]
pub(super) struct Locator<'a> {
    base: u32,
    offsets: Option<&'a [u32]>,
}

impl<'a> Locator<'a> {
    /// Text that starts at `base` and is contiguous in the file.
    pub fn contiguous(base: u32) -> Self {
        Self {
            base,
            offsets: None,
        }
    }

    /// Text assembled from several places: byte `i` lies at `offsets[i]`.
    pub fn mapped(offsets: &'a [u32]) -> Self {
        Self {
            base: 0,
            offsets: Some(offsets),
        }
    }

    pub fn at(&self, index: usize) -> u32 {
        match self.offsets {
            Some(map) => map.get(index).or(map.last()).copied().unwrap_or(self.base),
            None => self
                .base
                .saturating_add(u32::try_from(index).unwrap_or(u32::MAX)),
        }
    }

    pub fn span(&self, start: usize, end: usize) -> Span {
        Span::new(self.at(start), self.at(end.max(start)).max(self.at(start)))
    }
}

/// Term: Type
///
/// The outcome of parsing one type expression: the type if it could be parsed, how much of the text
/// it covered, and diagnostics (errors and notes about ignored constructs).
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct TypeParse {
    /// The type, or `None` when the text is not a type.
    pub ty: Option<Type>,
    /// Bytes of the text the type covers; the rest is not part of it.
    pub consumed: usize,
    /// Syntax errors and notes.
    pub diagnostics: Vec<SymbolDiagnostic>,
}

struct Fail {
    message: String,
    at: usize,
}

type PResult<T> = Result<T, Fail>;

struct Parser<'a, 'c> {
    lexer: Lexer<'a>,
    cur: Lexed,
    ctx: &'c TypeContext<'c>,
    loc: Locator<'c>,
    notes: Vec<SymbolDiagnostic>,
}

/// Parses the type expression at the start of `text`; anything after it is left alone.
pub(super) fn parse_prefix(text: &str, ctx: &TypeContext<'_>, loc: Locator<'_>) -> TypeParse {
    let mut lexer = Lexer::new(text);
    let cur = lexer.next_token();
    let mut parser = Parser {
        lexer,
        cur,
        ctx,
        loc,
        notes: Vec::new(),
    };
    match parser.union(0) {
        Ok(ty) => {
            let consumed = parser.cur.start.min(text.len());
            TypeParse {
                ty: Some(ty),
                consumed,
                diagnostics: parser.notes,
            }
        }
        Err(fail) => {
            let span = parser.loc.span(fail.at, fail.at + 1);
            let mut diagnostics = parser.notes;
            diagnostics.push(SymbolDiagnostic::new(
                SymbolDiagnosticCode::DocblockSyntax,
                fail.message,
                span,
            ));
            TypeParse {
                ty: None,
                consumed: text.len(),
                diagnostics,
            }
        }
    }
}

/// Parses `text` as one complete type: trailing text is an error.
pub fn parse_type(text: &str, ctx: &TypeContext<'_>, base: u32) -> TypeParse {
    let mut result = parse_prefix(text, ctx, Locator::contiguous(base));
    if result.ty.is_some() && !text[result.consumed..].trim().is_empty() {
        let loc = Locator::contiguous(base);
        result.ty = None;
        result.diagnostics.push(SymbolDiagnostic::new(
            SymbolDiagnosticCode::DocblockSyntax,
            "unexpected text after the type",
            loc.span(result.consumed, text.len()),
        ));
    }
    result
}

impl Parser<'_, '_> {
    fn advance(&mut self) -> Lexed {
        let next = self.lexer.next_token();
        std::mem::replace(&mut self.cur, next)
    }

    fn fail<T>(&self, message: impl Into<String>) -> PResult<T> {
        Err(Fail {
            message: message.into(),
            at: self.cur.start,
        })
    }

    fn is_punct(&self, c: char) -> bool {
        self.cur.token == Token::Punct(c)
    }

    fn eat(&mut self, c: char) -> bool {
        let found = self.is_punct(c);
        if found {
            self.advance();
        }
        found
    }

    fn expect(&mut self, c: char) -> PResult<()> {
        if self.eat(c) {
            Ok(())
        } else {
            self.fail(format!("expected `{c}`"))
        }
    }

    fn note(&mut self, start: usize, message: impl Into<String>) {
        let span = self.loc.span(start, self.cur.start.max(start));
        self.notes.push(SymbolDiagnostic::new(
            SymbolDiagnosticCode::DocblockUnsupported,
            message,
            span,
        ));
    }

    /// Whether the source right after the current token starts with `prefix`.
    fn next_starts_with(&self, prefix: &str) -> bool {
        self.lexer.rest_trimmed().starts_with(prefix)
    }

    fn union(&mut self, depth: usize) -> PResult<Type> {
        if depth > MAX_DEPTH {
            return self.fail("type is nested too deeply");
        }
        let mut result = self.intersection(depth)?;
        while self.eat('|') {
            let next = self.intersection(depth)?;
            result = result.union(&next);
        }
        Ok(result)
    }

    fn intersection(&mut self, depth: usize) -> PResult<Type> {
        let start = self.cur.start;
        let first = self.postfix(depth)?;
        let mut parts = vec![first];
        while self.is_punct('&') && !self.next_starts_with("$") && !self.next_starts_with("...") {
            self.advance();
            parts.push(self.postfix(depth)?);
        }
        if parts.len() == 1 {
            return Ok(parts.remove(0));
        }
        if parts.iter().any(|p| p.atoms().len() != 1) {
            self.note(start, "an intersection of unions is not supported");
            return Ok(Type::unknown());
        }
        let atoms = parts.iter().map(|p| p.atoms()[0].clone()).collect();
        Ok(Type::of(Atom::Intersection(atoms)))
    }

    fn postfix(&mut self, depth: usize) -> PResult<Type> {
        if self.eat('?') {
            return Ok(self.postfix(depth)?.union(&Type::null()));
        }
        let mut result = self.primary(depth)?;
        while self.is_punct('[') && self.next_starts_with("]") {
            self.advance();
            self.advance();
            result = Type::array_of(Type::array_key(), result);
        }
        Ok(result)
    }

    fn primary(&mut self, depth: usize) -> PResult<Type> {
        let start = self.cur.start;
        match self.cur.token.clone() {
            Token::Punct('(') => {
                self.advance();
                let inner = self.union(depth + 1)?;
                self.expect(')')?;
                Ok(inner)
            }
            Token::Int(n) => {
                self.advance();
                Ok(Type::of(Atom::IntLiteral(n)))
            }
            Token::Float => {
                self.advance();
                Ok(Type::float())
            }
            Token::Str(text) => {
                self.advance();
                Ok(Type::of(Atom::StringLiteral(text.into())))
            }
            Token::Var(name) if name == "this" => {
                self.advance();
                Ok(self.class_ref(ClassRefKind::Static, start))
            }
            Token::Ident(written) => {
                self.advance();
                self.named(&written, start, depth)
            }
            _ => self.fail("expected a type"),
        }
    }

    fn class_ref(&mut self, kind: ClassRefKind, start: usize) -> Type {
        let span = self.loc.span(start, self.cur.start);
        let resolved = self.ctx.class.map_or_else(
            || ClassContext::none().resolve(kind, span),
            |c| c.resolve(kind, span),
        );
        match resolved {
            ResolvedName::Resolved(fqn) => Type::object(fqn),
            ResolvedName::LateStatic(fqn) => Type::of(Atom::Static(fqn)),
            _ => {
                self.note(start, "`self`, `static` or `parent` outside a class-like");
                Type::unknown()
            }
        }
    }

    fn named(&mut self, written: &str, start: usize, depth: usize) -> PResult<Type> {
        let lower = written.to_ascii_lowercase();
        if self.ctx.templates.iter().any(|t| &**t == written) {
            return Ok(Type::of(Atom::Template(written.into())));
        }
        let adjacent = start + written.len() == self.cur.start;
        if lower == "int" && self.is_punct('<') {
            return self.int_range(depth);
        }
        if let Some(simple) = simple_keyword(&lower) {
            return Ok(simple);
        }
        match lower.as_str() {
            "array" | "non-empty-array" => self.array(&lower, depth),
            "list" | "non-empty-list" => self.list(&lower, depth),
            "iterable" => self.iterable(depth),
            "class-string" => self.class_string(depth),
            "callable" => self.callable(depth, start, adjacent),
            "closure" | "\\closure" if adjacent && self.is_punct('(') => {
                self.callable(depth, start, adjacent)
            }
            "self" => Ok(self.class_ref(ClassRefKind::SelfRef, start)),
            "static" => self.static_ref(start, adjacent),
            "parent" => Ok(self.class_ref(ClassRefKind::Parent, start)),
            _ => self.class_name(written, &lower, start, depth),
        }
    }
}

impl Parser<'_, '_> {
    /// `<A, B, ...>` if present.
    fn generic_args(&mut self, depth: usize) -> PResult<Option<Vec<Type>>> {
        if !self.eat('<') {
            return Ok(None);
        }
        let mut args = vec![self.union(depth + 1)?];
        while self.eat(',') {
            args.push(self.union(depth + 1)?);
        }
        self.expect('>')?;
        Ok(Some(args))
    }

    fn array(&mut self, lower: &str, depth: usize) -> PResult<Type> {
        let non_empty = lower.starts_with("non-empty");
        if self.is_punct('{') && !non_empty {
            self.advance();
            return self.shape(depth);
        }
        let (key, value) = match self.generic_args(depth)? {
            None => (Type::array_key(), Type::mixed()),
            Some(mut args) if args.len() == 1 => (Type::array_key(), args.remove(0)),
            Some(mut args) if args.len() == 2 => {
                let value = args.remove(1);
                (args.remove(0), value)
            }
            Some(_) => return self.fail("an array takes one or two type arguments"),
        };
        Ok(Type::of(Atom::Array {
            key: Box::new(key),
            value: Box::new(value),
            non_empty,
        }))
    }

    fn list(&mut self, lower: &str, depth: usize) -> PResult<Type> {
        let non_empty = lower.starts_with("non-empty");
        if self.is_punct('{') && !non_empty {
            self.advance();
            return self.shape(depth);
        }
        let value = match self.generic_args(depth)? {
            None => Type::mixed(),
            Some(mut args) if args.len() == 1 => args.remove(0),
            Some(_) => return self.fail("a list takes one type argument"),
        };
        Ok(Type::of(Atom::List {
            value: Box::new(value),
            non_empty,
        }))
    }

    fn iterable(&mut self, depth: usize) -> PResult<Type> {
        let (key, value) = match self.generic_args(depth)? {
            None => (Type::mixed(), Type::mixed()),
            Some(mut args) if args.len() == 1 => (Type::mixed(), args.remove(0)),
            Some(mut args) if args.len() == 2 => {
                let value = args.remove(1);
                (args.remove(0), value)
            }
            Some(_) => return self.fail("iterable takes one or two type arguments"),
        };
        Ok(Type::of(Atom::Iterable {
            key: Box::new(key),
            value: Box::new(value),
        }))
    }

    fn class_string(&mut self, depth: usize) -> PResult<Type> {
        let start = self.cur.start;
        let Some(args) = self.generic_args(depth)? else {
            return Ok(Type::of(Atom::ClassString(None)));
        };
        let bound = match args.as_slice() {
            [only] => match only.atoms() {
                [Atom::Named(object)] if object.args.is_empty() => Some(object.name.clone()),
                _ => None,
            },
            _ => None,
        };
        if bound.is_none() {
            self.note(
                start,
                "only a single class name is kept as a class-string bound",
            );
        }
        Ok(Type::of(Atom::ClassString(bound)))
    }

    fn static_ref(&mut self, start: usize, adjacent: bool) -> PResult<Type> {
        if adjacent && self.is_punct('(') {
            // The rendered form `static(Foo)` names the bound class.
            self.advance();
            let bound = self.union(1)?;
            self.expect(')')?;
            return Ok(match bound.atoms() {
                [Atom::Named(object)] => Type::of(Atom::Static(object.name.clone())),
                _ => Type::unknown(),
            });
        }
        Ok(self.class_ref(ClassRefKind::Static, start))
    }

    fn int_range(&mut self, _depth: usize) -> PResult<Type> {
        self.expect('<')?;
        let min = self.range_bound("min")?;
        self.expect(',')?;
        let max = self.range_bound("max")?;
        self.expect('>')?;
        Ok(Type::of(Atom::IntRange { min, max }))
    }

    fn range_bound(&mut self, open: &str) -> PResult<Option<i64>> {
        match self.cur.token.clone() {
            Token::Int(n) => {
                self.advance();
                Ok(Some(n))
            }
            Token::Ident(word) if word.eq_ignore_ascii_case(open) => {
                self.advance();
                Ok(None)
            }
            _ => self.fail("expected an integer bound"),
        }
    }

    fn callable(&mut self, depth: usize, start: usize, adjacent: bool) -> PResult<Type> {
        if !(adjacent && self.is_punct('(')) {
            return Ok(Type::of(Atom::Callable(None)));
        }
        if depth + 1 > MAX_DEPTH {
            return self.fail("type is nested too deeply");
        }
        self.advance();
        let mut params = Vec::new();
        while !self.is_punct(')') {
            params.push(self.callable_param(depth)?);
            if !self.eat(',') {
                break;
            }
        }
        self.expect(')')?;
        let ret = if self.eat(':') {
            self.postfix(depth + 1)?
        } else {
            self.note(start, "a callable without a return type is taken as mixed");
            Type::mixed()
        };
        Ok(Type::of(Atom::Callable(Some(Box::new(CallableSig {
            params,
            ret,
        })))))
    }

    fn callable_param(&mut self, depth: usize) -> PResult<CallableParam> {
        let mut variadic = matches!(self.cur.token, Token::Ellipsis);
        if variadic {
            self.advance();
        }
        let ty = self.union(depth + 1)?;
        if self.is_punct('&') {
            self.advance();
        }
        if matches!(self.cur.token, Token::Ellipsis) {
            variadic = true;
            self.advance();
        }
        if matches!(self.cur.token, Token::Var(_)) {
            self.advance();
        }
        let optional = self.eat('=');
        Ok(CallableParam {
            ty,
            optional,
            variadic,
        })
    }

    fn shape(&mut self, depth: usize) -> PResult<Type> {
        if depth + 1 > MAX_DEPTH {
            return self.fail("type is nested too deeply");
        }
        let mut entries = Vec::new();
        let mut sealed = true;
        let mut next_index: i64 = 0;
        while !self.is_punct('}') {
            if matches!(self.cur.token, Token::Ellipsis) {
                self.advance();
                sealed = false;
                break;
            }
            let entry = self.shape_entry(depth, &mut next_index)?;
            entries.push(entry);
            if !self.eat(',') {
                break;
            }
        }
        self.expect('}')?;
        Ok(Type::of(Atom::Shape { entries, sealed }))
    }

    fn shape_entry(&mut self, depth: usize, next_index: &mut i64) -> PResult<ShapeEntry> {
        let optional_key = self.next_starts_with("?:");
        let plain_key = self.next_starts_with(":") && !self.next_starts_with("::");
        let key = match (&self.cur.token, optional_key || plain_key) {
            (Token::Ident(name), true) => Some(ShapeKey::Str(name.as_str().into())),
            (Token::Str(text), true) => Some(ShapeKey::Str(text.as_str().into())),
            (Token::Int(n), true) => Some(ShapeKey::Int(*n)),
            _ => None,
        };
        let (key, optional) = if let Some(key) = key {
            self.advance();
            let optional = self.eat('?');
            self.expect(':')?;
            (key, optional)
        } else {
            let key = ShapeKey::Int(*next_index);
            *next_index += 1;
            (key, false)
        };
        let value = self.union(depth + 1)?;
        Ok(ShapeEntry {
            key,
            value,
            optional,
        })
    }

    fn class_name(
        &mut self,
        written: &str,
        lower: &str,
        start: usize,
        depth: usize,
    ) -> PResult<Type> {
        if is_unsupported_generic(lower) {
            self.generic_args(depth)?;
            self.note(start, format!("`{written}` is not supported"));
            return Ok(Type::unknown());
        }
        let span = self.loc.span(start, self.cur.start);
        let name = Name::from_written(span, written);
        let resolved = match self.ctx.scope {
            Some(scope) => scope.resolve(&name, NameContext::Class, &NoDeclarations),
            None => ResolvedName::Resolved(Fqn::new(&name.text)),
        };
        let ResolvedName::Resolved(fqn) = resolved else {
            self.note(start, format!("`{written}` cannot be resolved"));
            return Ok(Type::unknown());
        };
        if matches!(self.cur.token, Token::DoubleColon) {
            return self.enum_case(fqn, start);
        }
        let args = self.generic_args(depth)?.unwrap_or_default();
        if lower == "object" && args.is_empty() && self.is_punct('{') {
            return Ok(Type::of(Atom::Object));
        }
        Ok(Type::of(Atom::Named(NamedObject { name: fqn, args })))
    }

    fn enum_case(&mut self, enum_name: Fqn, start: usize) -> PResult<Type> {
        self.advance();
        match self.cur.token.clone() {
            Token::Ident(case) => {
                self.advance();
                Ok(Type::of(Atom::EnumCase {
                    enum_name,
                    case: case.into(),
                }))
            }
            _ => {
                self.advance();
                self.note(start, "class constant wildcards are not supported");
                Ok(Type::unknown())
            }
        }
    }
}
