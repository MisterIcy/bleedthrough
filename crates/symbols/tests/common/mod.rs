//! Test support: a parser for the subset of PHP the symbols tests are written in, building the
//! owned syntax model with real byte spans. The domain crates may not depend on a parser, so the
//! trees these tests analyse come from here. It accepts what the tests need and panics on the rest.
#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::type_complexity,
    reason = "tests are long tables of cases and the test parser is a dispatch over PHP syntax"
)]
#![allow(
    dead_code,
    reason = "each test file uses a different part of the support code"
)]

pub use bleedthrough_symbols::*;
use bleedthrough_syntax::{
    Arg, ArgKind, ArrayItem, ArrayItemKind, ArrayStyle, AssignOp, Attribute, BinaryOp, BuiltinType,
    CallTarget, CastKind, Catch, ClassLike, ClassLikeKind, ClassRefKind, ClosureUse, Comment,
    CommentKind, ConstItem, ElseIf, Expr, ExprKind, FunctionLike, IncludeKind, Literal,
    MagicConstantKind, MatchArm, Member, MemberKind, MemberName, Modifier, Name, Node, Param,
    PropertyItem, QuoteKind, Span, StaticVar, Stmt, StmtKind, StringKind, StringPartKind,
    SwitchCase, SyntaxTree, TraitAdaptationKind, TypeHint, TypeKind, UnaryOp, UseItem, UseKind,
};

// ---------------------------------------------------------------------------------------------
// Lexer

#[derive(Debug, Clone, PartialEq)]
enum T {
    Var(String),
    Id(String),
    Int(String),
    Float(String),
    Str(String),
    Op(&'static str),
    Cast(CastKind),
    Eof,
}

#[derive(Debug, Clone)]
struct Tok {
    t: T,
    s: u32,
    e: u32,
}

const OPS: &[&str] = &[
    "<=>", "===", "!==", "**=", "??=", "...", "<<=", ">>=", "?->", "|>", "==", "!=", "<>", "<=",
    ">=", "&&", "||", "??", "++", "--", "->", "=>", "::", "+=", "-=", "*=", "/=", ".=", "%=", "&=",
    "|=", "^=", "<<", ">>", "**", "+", "-", "*", "/", "%", "=", "<", ">", "!", ".", "?", ":", ";",
    ",", "(", ")", "[", "]", "{", "}", "&", "|", "^", "~", "@", "#", "$",
];

fn cast_of(word: &str) -> Option<CastKind> {
    Some(match word.to_ascii_lowercase().as_str() {
        "int" | "integer" => CastKind::Int,
        "bool" | "boolean" => CastKind::Bool,
        "float" | "double" => CastKind::Float,
        "string" => CastKind::String,
        "array" => CastKind::Array,
        "object" => CastKind::Object,
        _ => return None,
    })
}

fn lex(src: &str) -> (Vec<Tok>, Vec<Comment>) {
    let b = src.as_bytes();
    let mut i = 0usize;
    let mut toks = Vec::new();
    let mut comments = Vec::new();
    let at = |i: usize| u32::try_from(i).expect("small source");
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if src[i..].starts_with("/*") {
            let end = src[i + 2..].find("*/").map_or(b.len(), |p| i + 2 + p + 2);
            let kind = if src[i..].starts_with("/**") && end > i + 4 {
                CommentKind::Doc
            } else {
                CommentKind::Block
            };
            comments.push(Comment {
                kind,
                span: Span::new(at(i), at(end)),
                text: src[i..end].into(),
            });
            i = end;
        } else if src[i..].starts_with("//") || (c == b'#' && !src[i..].starts_with("#[")) {
            let end = src[i..].find('\n').map_or(b.len(), |p| i + p);
            let kind = if c == b'#' {
                CommentKind::Hash
            } else {
                CommentKind::Line
            };
            comments.push(Comment {
                kind,
                span: Span::new(at(i), at(end)),
                text: src[i..end].into(),
            });
            i = end;
        } else if c == b'$'
            && i + 1 < b.len()
            && (b[i + 1].is_ascii_alphabetic() || b[i + 1] == b'_')
        {
            let mut j = i + 1;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                j += 1;
            }
            toks.push(Tok {
                t: T::Var(src[i + 1..j].to_string()),
                s: at(i),
                e: at(j),
            });
            i = j;
        } else if c.is_ascii_alphabetic() || c == b'_' || c == b'\\' {
            let mut j = i + 1;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_' || b[j] == b'\\') {
                j += 1;
            }
            toks.push(Tok {
                t: T::Id(src[i..j].to_string()),
                s: at(i),
                e: at(j),
            });
            i = j;
        } else if c.is_ascii_digit() || (c == b'.' && i + 1 < b.len() && b[i + 1].is_ascii_digit())
        {
            let mut j = i;
            let mut float = false;
            while j < b.len() {
                let d = b[j];
                if d.is_ascii_alphanumeric() || d == b'_' {
                    if (d == b'e' || d == b'E') && !src[i..j].to_ascii_lowercase().starts_with("0x")
                    {
                        float = true;
                        if j + 1 < b.len() && (b[j + 1] == b'+' || b[j + 1] == b'-') {
                            j += 1;
                        }
                    }
                    j += 1;
                } else if d == b'.' && j + 1 < b.len() && b[j + 1].is_ascii_digit() && !float {
                    float = true;
                    j += 1;
                } else if d == b'.' && !float && !(j + 1 < b.len() && b[j + 1] == b'.') {
                    // `1.` is a float, `1..` is not
                    let next = b.get(j + 1).copied().unwrap_or(b' ');
                    if next == b'=' || next == b'.' || next.is_ascii_alphabetic() {
                        break;
                    }
                    float = true;
                    j += 1;
                } else {
                    break;
                }
            }
            let raw = src[i..j].to_string();
            let t = if float { T::Float(raw) } else { T::Int(raw) };
            toks.push(Tok {
                t,
                s: at(i),
                e: at(j),
            });
            i = j;
        } else if c == b'\'' || c == b'"' || c == b'`' {
            let mut j = i + 1;
            while j < b.len() && b[j] != c {
                if b[j] == b'\\' {
                    j += 1;
                }
                j += 1;
            }
            j += 1;
            toks.push(Tok {
                t: T::Str(src[i..j.min(b.len())].to_string()),
                s: at(i),
                e: at(j.min(b.len())),
            });
            i = j;
        } else if c == b'(' {
            // a cast: `(int)`
            let rest = &src[i + 1..];
            let close = rest.find(')');
            let cast = close.and_then(|p| cast_of(rest[..p].trim()).map(|k| (k, p)));
            if let Some((kind, p)) = cast {
                let j = i + 1 + p + 1;
                toks.push(Tok {
                    t: T::Cast(kind),
                    s: at(i),
                    e: at(j),
                });
                i = j;
            } else {
                toks.push(Tok {
                    t: T::Op("("),
                    s: at(i),
                    e: at(i + 1),
                });
                i += 1;
            }
        } else {
            let op = OPS
                .iter()
                .find(|o| src[i..].starts_with(**o))
                .unwrap_or_else(|| panic!("unexpected byte {:?} at {i}", c as char));
            toks.push(Tok {
                t: T::Op(op),
                s: at(i),
                e: at(i + op.len()),
            });
            i += op.len();
        }
    }
    let end = at(b.len());
    toks.push(Tok {
        t: T::Eof,
        s: end,
        e: end,
    });
    (toks, comments)
}

// ---------------------------------------------------------------------------------------------
// Parser

/// Parses `src` into a syntax tree, panicking on anything outside the supported subset.
pub fn parse(src: &str) -> SyntaxTree {
    let (toks, comments) = lex(src);
    let mut p = P { toks, pos: 0, src };
    let mut stmts = Vec::new();
    while !p.at_eof() {
        stmts.push(p.stmt());
    }
    SyntaxTree::new(
        stmts,
        comments,
        u32::try_from(src.len()).expect("small source"),
    )
}

struct P<'a> {
    toks: Vec<Tok>,
    pos: usize,
    src: &'a str,
}

fn sp(s: u32, e: u32) -> Span {
    Span::new(s, e)
}

fn binary_op(op: &str) -> Option<(BinaryOp, u8, bool)> {
    Some(match op {
        "||" => (BinaryOp::Or, 7, false),
        "&&" => (BinaryOp::And, 8, false),
        "|" => (BinaryOp::BitOr, 9, false),
        "^" => (BinaryOp::BitXor, 10, false),
        "&" => (BinaryOp::BitAnd, 11, false),
        "==" => (BinaryOp::Eq, 12, false),
        "!=" => (BinaryOp::NotEq, 12, false),
        "<>" => (BinaryOp::AngledNotEq, 12, false),
        "===" => (BinaryOp::Identical, 12, false),
        "!==" => (BinaryOp::NotIdentical, 12, false),
        "<=>" => (BinaryOp::Spaceship, 12, false),
        "<" => (BinaryOp::Lt, 13, false),
        "<=" => (BinaryOp::Le, 13, false),
        ">" => (BinaryOp::Gt, 13, false),
        ">=" => (BinaryOp::Ge, 13, false),
        "." => (BinaryOp::Concat, 14, false),
        "<<" => (BinaryOp::Shl, 15, false),
        ">>" => (BinaryOp::Shr, 15, false),
        "+" => (BinaryOp::Add, 16, false),
        "-" => (BinaryOp::Sub, 16, false),
        "*" => (BinaryOp::Mul, 17, false),
        "/" => (BinaryOp::Div, 17, false),
        "%" => (BinaryOp::Mod, 17, false),
        "**" => (BinaryOp::Pow, 21, true),
        "??" => (BinaryOp::Coalesce, 6, true),
        _ => return None,
    })
}

fn assign_op(op: &str) -> Option<AssignOp> {
    Some(match op {
        "=" => AssignOp::Assign,
        "+=" => AssignOp::Add,
        "-=" => AssignOp::Sub,
        "*=" => AssignOp::Mul,
        "/=" => AssignOp::Div,
        "%=" => AssignOp::Mod,
        "**=" => AssignOp::Pow,
        ".=" => AssignOp::Concat,
        "&=" => AssignOp::BitAnd,
        "|=" => AssignOp::BitOr,
        "^=" => AssignOp::BitXor,
        "<<=" => AssignOp::Shl,
        ">>=" => AssignOp::Shr,
        "??=" => AssignOp::Coalesce,
        _ => return None,
    })
}

fn builtin(word: &str) -> Option<BuiltinType> {
    Some(match word.to_ascii_lowercase().as_str() {
        "null" => BuiltinType::Null,
        "true" => BuiltinType::True,
        "false" => BuiltinType::False,
        "array" => BuiltinType::Array,
        "callable" => BuiltinType::Callable,
        "static" => BuiltinType::Static,
        "self" => BuiltinType::SelfType,
        "parent" => BuiltinType::Parent,
        "void" => BuiltinType::Void,
        "never" => BuiltinType::Never,
        "float" => BuiltinType::Float,
        "bool" => BuiltinType::Bool,
        "int" => BuiltinType::Int,
        "string" => BuiltinType::String,
        "object" => BuiltinType::Object,
        "mixed" => BuiltinType::Mixed,
        "iterable" => BuiltinType::Iterable,
        _ => return None,
    })
}

fn modifier(word: &str) -> Option<Modifier> {
    Some(match word.to_ascii_lowercase().as_str() {
        "static" => Modifier::Static,
        "final" => Modifier::Final,
        "abstract" => Modifier::Abstract,
        "readonly" => Modifier::Readonly,
        "public" => Modifier::Public,
        "protected" => Modifier::Protected,
        "private" => Modifier::Private,
        _ => return None,
    })
}

fn magic(word: &str) -> Option<MagicConstantKind> {
    Some(match word.to_ascii_uppercase().as_str() {
        "__LINE__" => MagicConstantKind::Line,
        "__FILE__" => MagicConstantKind::File,
        "__DIR__" => MagicConstantKind::Dir,
        "__TRAIT__" => MagicConstantKind::Trait,
        "__METHOD__" => MagicConstantKind::Method,
        "__FUNCTION__" => MagicConstantKind::Function,
        "__NAMESPACE__" => MagicConstantKind::Namespace,
        "__CLASS__" => MagicConstantKind::Class,
        _ => return None,
    })
}

impl P<'_> {
    fn tok(&self) -> &Tok {
        &self.toks[self.pos]
    }

    fn peek_at(&self, n: usize) -> &T {
        &self.toks[(self.pos + n).min(self.toks.len() - 1)].t
    }

    fn at_eof(&self) -> bool {
        matches!(self.tok().t, T::Eof)
    }

    fn start(&self) -> u32 {
        self.tok().s
    }

    fn prev_end(&self) -> u32 {
        self.toks[self.pos.saturating_sub(1)].e
    }

    fn bump(&mut self) -> Tok {
        let t = self.toks[self.pos].clone();
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
        t
    }

    fn is_op(&self, op: &str) -> bool {
        matches!(&self.tok().t, T::Op(o) if *o == op)
    }

    fn op_at(&self, n: usize, op: &str) -> bool {
        matches!(self.peek_at(n), T::Op(o) if *o == op)
    }

    fn eat_op(&mut self, op: &str) -> bool {
        let hit = self.is_op(op);
        if hit {
            self.bump();
        }
        hit
    }

    fn expect_op(&mut self, op: &str) {
        assert!(
            self.eat_op(op),
            "expected `{op}` at {} but found {:?} in {:?}",
            self.start(),
            self.tok().t,
            self.src
        );
    }

    fn is_kw(&self, kw: &str) -> bool {
        matches!(&self.tok().t, T::Id(w) if w.eq_ignore_ascii_case(kw))
    }

    fn kw_at(&self, n: usize, kw: &str) -> bool {
        matches!(self.peek_at(n), T::Id(w) if w.eq_ignore_ascii_case(kw))
    }

    fn eat_kw(&mut self, kw: &str) -> bool {
        let hit = self.is_kw(kw);
        if hit {
            self.bump();
        }
        hit
    }

    fn ident(&mut self) -> String {
        match self.bump().t {
            T::Id(w) => w,
            other => panic!("expected an identifier, found {other:?} in {:?}", self.src),
        }
    }

    fn name(&mut self) -> Name {
        let tok = self.bump();
        let T::Id(w) = tok.t else {
            panic!("expected a name, found {:?} in {:?}", tok.t, self.src);
        };
        Name::from_written(sp(tok.s, tok.e), &w)
    }

    // ---- statements ----

    fn body(&mut self) -> Vec<Stmt> {
        if self.eat_op("{") {
            let mut out = Vec::new();
            while !self.is_op("}") {
                out.push(self.stmt());
            }
            self.expect_op("}");
            out
        } else {
            vec![self.stmt()]
        }
    }

    fn block_inner(&mut self) -> Vec<Stmt> {
        self.expect_op("{");
        let mut out = Vec::new();
        while !self.is_op("}") {
            out.push(self.stmt());
        }
        self.expect_op("}");
        out
    }

    fn mk(&self, start: u32, kind: StmtKind) -> Stmt {
        Node::new(sp(start, self.prev_end()), kind)
    }

    #[allow(clippy::too_many_lines, reason = "a dispatch over statement keywords")]
    fn stmt(&mut self) -> Stmt {
        let start = self.start();
        let attributes = self.attrs();
        if !attributes.is_empty() {
            let mut stmt = self.stmt();
            match &mut stmt.kind {
                StmtKind::ClassLike(c) => c.attributes = attributes,
                StmtKind::Function(f) => f.attributes = attributes,
                _ => {}
            }
            stmt.span = sp(start, stmt.span.end);
            return stmt;
        }
        if self.eat_op(";") {
            return self.mk(start, StmtKind::Noop);
        }
        if self.is_op("{") {
            let body = self.block_inner();
            return self.mk(start, StmtKind::Block(body));
        }
        let T::Id(word) = self.tok().t.clone() else {
            return self.expr_stmt(start);
        };
        let lower = word.to_ascii_lowercase();
        match lower.as_str() {
            "namespace" if !self.op_at(1, "::") => {
                self.bump();
                let name = if matches!(self.tok().t, T::Id(_)) {
                    Some(self.name())
                } else {
                    None
                };
                if self.is_op("{") {
                    let body = self.block_inner();
                    self.mk(
                        start,
                        StmtKind::Namespace {
                            name,
                            braced: true,
                            body,
                        },
                    )
                } else {
                    self.expect_op(";");
                    // an unbraced namespace owns the statements up to the next one
                    let mut body = Vec::new();
                    while !self.at_eof() && !(self.is_kw("namespace") && !self.op_at(1, "::")) {
                        body.push(self.stmt());
                    }
                    self.mk(
                        start,
                        StmtKind::Namespace {
                            name,
                            braced: false,
                            body,
                        },
                    )
                }
            }
            "use" => {
                self.bump();
                let kind = if self.eat_kw("function") {
                    UseKind::Function
                } else if self.eat_kw("const") {
                    UseKind::Const
                } else {
                    UseKind::Class
                };
                let mut items = Vec::new();
                loop {
                    let item_start = self.start();
                    let name = self.name();
                    let alias = self.eat_kw("as").then(|| self.ident().into());
                    items.push(UseItem {
                        span: sp(item_start, self.prev_end()),
                        kind,
                        name,
                        alias,
                    });
                    if !self.eat_op(",") {
                        break;
                    }
                }
                self.expect_op(";");
                self.mk(start, StmtKind::Use { items })
            }
            "function"
                if matches!(self.peek_at(1), T::Id(_))
                    || (self.op_at(1, "&") && matches!(self.peek_at(2), T::Id(_))) =>
            {
                self.bump();
                let f = self.function_rest(vec![]);
                self.mk(start, StmtKind::Function(Box::new(f)))
            }
            "abstract" | "final" | "readonly" | "class" | "interface" | "trait" => {
                let class = self.class_like();
                self.mk(start, StmtKind::ClassLike(Box::new(class)))
            }
            "enum" if matches!(self.peek_at(1), T::Id(_)) => {
                let class = self.class_like();
                self.mk(start, StmtKind::ClassLike(Box::new(class)))
            }
            "if" => {
                self.bump();
                self.expect_op("(");
                let condition = self.expr();
                self.expect_op(")");
                let then = self.body();
                let mut else_ifs = Vec::new();
                let mut otherwise = None;
                loop {
                    let branch_start = self.start();
                    if self.is_kw("elseif") || (self.is_kw("else") && self.kw_at(1, "if")) {
                        if self.is_kw("else") {
                            self.bump();
                        }
                        self.bump();
                        self.expect_op("(");
                        let condition = self.expr();
                        self.expect_op(")");
                        let body = self.body();
                        else_ifs.push(ElseIf {
                            span: sp(branch_start, self.prev_end()),
                            condition,
                            body,
                        });
                    } else if self.eat_kw("else") {
                        otherwise = Some(self.body());
                        break;
                    } else {
                        break;
                    }
                }
                self.mk(
                    start,
                    StmtKind::If {
                        condition,
                        then,
                        else_ifs,
                        otherwise,
                    },
                )
            }
            "while" => {
                self.bump();
                self.expect_op("(");
                let condition = self.expr();
                self.expect_op(")");
                let body = self.body();
                self.mk(start, StmtKind::While { condition, body })
            }
            "do" => {
                self.bump();
                let body = self.body();
                assert!(self.eat_kw("while"));
                self.expect_op("(");
                let condition = self.expr();
                self.expect_op(")");
                self.expect_op(";");
                self.mk(start, StmtKind::DoWhile { body, condition })
            }
            "for" => {
                self.bump();
                self.expect_op("(");
                let init = self.expr_list_until(";");
                self.expect_op(";");
                let condition = self.expr_list_until(";");
                self.expect_op(";");
                let step = self.expr_list_until(")");
                self.expect_op(")");
                let body = self.body();
                self.mk(
                    start,
                    StmtKind::For {
                        init,
                        condition,
                        step,
                        body,
                    },
                )
            }
            "foreach" => {
                self.bump();
                self.expect_op("(");
                let subject = self.expr();
                assert!(self.eat_kw("as"));
                let first = self.foreach_target();
                let (key, value) = if self.eat_op("=>") {
                    (Some(first), self.foreach_target())
                } else {
                    (None, first)
                };
                self.expect_op(")");
                let body = self.body();
                self.mk(
                    start,
                    StmtKind::Foreach {
                        subject,
                        key,
                        value,
                        body,
                    },
                )
            }
            "switch" => {
                self.bump();
                self.expect_op("(");
                let subject = self.expr();
                self.expect_op(")");
                self.expect_op("{");
                let mut cases = Vec::new();
                while !self.is_op("}") {
                    let case_start = self.start();
                    let test = if self.eat_kw("default") {
                        None
                    } else {
                        assert!(self.eat_kw("case"));
                        Some(self.expr())
                    };
                    if !self.eat_op(":") {
                        self.expect_op(";");
                    }
                    let mut body = Vec::new();
                    while !self.is_op("}") && !self.is_kw("case") && !self.is_kw("default") {
                        body.push(self.stmt());
                    }
                    cases.push(SwitchCase {
                        span: sp(case_start, self.prev_end()),
                        test,
                        body,
                    });
                }
                self.expect_op("}");
                self.mk(start, StmtKind::Switch { subject, cases })
            }
            "try" => {
                self.bump();
                let body = self.block_inner();
                let mut catches = Vec::new();
                let mut finally = None;
                loop {
                    let catch_start = self.start();
                    if self.eat_kw("catch") {
                        self.expect_op("(");
                        let types = self.catch_types();
                        let variable = match &self.tok().t {
                            T::Var(v) => {
                                let v = v.clone();
                                self.bump();
                                Some(v.into())
                            }
                            _ => None,
                        };
                        self.expect_op(")");
                        let body = self.block_inner();
                        catches.push(Catch {
                            span: sp(catch_start, self.prev_end()),
                            types,
                            variable,
                            body,
                        });
                    } else if self.eat_kw("finally") {
                        finally = Some(self.block_inner());
                        break;
                    } else {
                        break;
                    }
                }
                self.mk(
                    start,
                    StmtKind::Try {
                        body,
                        catches,
                        finally,
                    },
                )
            }
            "declare" if self.op_at(1, "(") => {
                self.bump();
                self.expect_op("(");
                let mut directives = Vec::new();
                while !self.is_op(")") {
                    let d_start = self.start();
                    let name = self.ident();
                    self.expect_op("=");
                    let value = self.expr();
                    directives.push(bleedthrough_syntax::Directive {
                        span: sp(d_start, self.prev_end()),
                        name: name.into(),
                        value,
                    });
                    if !self.eat_op(",") {
                        break;
                    }
                }
                self.expect_op(")");
                let body = if self.is_op("{") {
                    Some(self.block_inner())
                } else {
                    self.expect_op(";");
                    None
                };
                self.mk(start, StmtKind::Declare { directives, body })
            }
            "return" => {
                self.bump();
                let value = (!self.is_op(";")).then(|| self.expr());
                self.expect_op(";");
                self.mk(start, StmtKind::Return(value))
            }
            "break" | "continue" => {
                self.bump();
                let value = (!self.is_op(";")).then(|| self.expr());
                self.expect_op(";");
                let kind = if lower == "break" {
                    StmtKind::Break(value)
                } else {
                    StmtKind::Continue(value)
                };
                self.mk(start, kind)
            }
            "echo" => {
                self.bump();
                let mut values = vec![self.expr()];
                while self.eat_op(",") {
                    values.push(self.expr());
                }
                self.expect_op(";");
                self.mk(
                    start,
                    StmtKind::Echo {
                        values,
                        short_tag: false,
                    },
                )
            }
            "global" => {
                self.bump();
                let mut vars = vec![self.expr()];
                while self.eat_op(",") {
                    vars.push(self.expr());
                }
                self.expect_op(";");
                self.mk(start, StmtKind::Global(vars))
            }
            "unset" if self.op_at(1, "(") => {
                self.bump();
                self.expect_op("(");
                let vars = self.expr_list_until(")");
                self.expect_op(")");
                self.expect_op(";");
                self.mk(start, StmtKind::Unset(vars))
            }
            "static" if matches!(self.peek_at(1), T::Var(_)) => {
                self.bump();
                let mut vars = Vec::new();
                loop {
                    let var_start = self.start();
                    let T::Var(name) = self.bump().t else {
                        panic!("expected a variable");
                    };
                    let init = self.eat_op("=").then(|| self.expr());
                    vars.push(StaticVar {
                        span: sp(var_start, self.prev_end()),
                        name: name.into(),
                        init,
                    });
                    if !self.eat_op(",") {
                        break;
                    }
                }
                self.expect_op(";");
                self.mk(start, StmtKind::StaticVars(vars))
            }
            "const" => {
                self.bump();
                let items = self.const_items();
                self.expect_op(";");
                self.mk(
                    start,
                    StmtKind::Const {
                        attributes: vec![],
                        items,
                    },
                )
            }
            _ => self.expr_stmt(start),
        }
    }

    fn expr_stmt(&mut self, start: u32) -> Stmt {
        let e = self.expr();
        self.expect_op(";");
        self.mk(start, StmtKind::Expr(e))
    }

    fn foreach_target(&mut self) -> Expr {
        let start = self.start();
        if self.eat_op("&") {
            let inner = self.unary();
            return Node::new(
                sp(start, self.prev_end()),
                ExprKind::Unary {
                    op: UnaryOp::Reference,
                    operand: Box::new(inner),
                },
            );
        }
        self.unary()
    }

    fn catch_types(&mut self) -> TypeHint {
        let start = self.start();
        let mut parts = vec![self.single_type()];
        while self.eat_op("|") {
            parts.push(self.single_type());
        }
        if parts.len() == 1 {
            parts.remove(0)
        } else {
            Node::new(sp(start, self.prev_end()), TypeKind::Union(parts))
        }
    }

    fn expr_list_until(&mut self, close: &str) -> Vec<Expr> {
        let mut out = Vec::new();
        while !self.is_op(close) {
            out.push(self.expr());
            if !self.eat_op(",") {
                break;
            }
        }
        out
    }

    fn const_items(&mut self) -> Vec<ConstItem> {
        let mut items = Vec::new();
        loop {
            let item_start = self.start();
            let name = self.ident();
            self.expect_op("=");
            let value = self.expr();
            items.push(ConstItem {
                span: sp(item_start, self.prev_end()),
                name: name.into(),
                value,
            });
            if !self.eat_op(",") {
                break;
            }
        }
        items
    }

    // ---- declarations ----

    fn modifiers(&mut self) -> Vec<Modifier> {
        let mut out = Vec::new();
        while let T::Id(w) = &self.tok().t {
            let Some(m) = modifier(w) else { break };
            // `static` starting an expression or `static::` is not a modifier here
            if matches!(m, Modifier::Static) && self.op_at(1, "::") {
                break;
            }
            self.bump();
            let m = if self.is_op("(") && matches!(self.peek_at(1), T::Id(s) if s == "set") {
                self.bump();
                self.bump();
                self.expect_op(")");
                match m {
                    Modifier::Public => Modifier::PublicSet,
                    Modifier::Protected => Modifier::ProtectedSet,
                    _ => Modifier::PrivateSet,
                }
            } else {
                m
            };
            out.push(m);
        }
        out
    }

    /// Zero or more `#[Name(args), Other]` groups.
    fn attrs(&mut self) -> Vec<Attribute> {
        let mut out = Vec::new();
        while self.is_op("#") && self.op_at(1, "[") {
            self.bump();
            self.bump();
            loop {
                let start = self.start();
                let name = self.name();
                let args = if self.is_op("(") { self.args() } else { vec![] };
                out.push(Attribute {
                    span: sp(start, self.prev_end()),
                    name,
                    args,
                });
                if !self.eat_op(",") || self.is_op("]") {
                    break;
                }
            }
            self.expect_op("]");
        }
        out
    }

    fn class_like(&mut self) -> ClassLike {
        let modifiers = self.modifiers();
        let kind = match self.ident().to_ascii_lowercase().as_str() {
            "class" => ClassLikeKind::Class,
            "interface" => ClassLikeKind::Interface,
            "trait" => ClassLikeKind::Trait,
            "enum" => ClassLikeKind::Enum,
            other => panic!("expected a class-like keyword, found {other}"),
        };
        let name = self.ident();
        let backing_type = if kind == ClassLikeKind::Enum && self.eat_op(":") {
            Some(self.type_hint())
        } else {
            None
        };
        let mut extends = Vec::new();
        let mut implements = Vec::new();
        if self.eat_kw("extends") {
            extends.push(self.name());
            while self.eat_op(",") {
                extends.push(self.name());
            }
        }
        if self.eat_kw("implements") {
            implements.push(self.name());
            while self.eat_op(",") {
                implements.push(self.name());
            }
        }
        self.expect_op("{");
        let mut members = Vec::new();
        while !self.is_op("}") {
            members.push(self.member());
        }
        self.expect_op("}");
        ClassLike {
            kind,
            name: Some(name.into()),
            attributes: vec![],
            modifiers,
            extends,
            implements,
            backing_type,
            members,
        }
    }

    fn member(&mut self) -> Member {
        let start = self.start();
        let attributes = self.attrs();
        let mut kind = self.member_kind();
        match &mut kind {
            MemberKind::Method(f) => f.attributes = attributes,
            MemberKind::Property { attributes: a, .. }
            | MemberKind::Const { attributes: a, .. }
            | MemberKind::EnumCase { attributes: a, .. } => *a = attributes,
            _ => {}
        }
        Node::new(sp(start, self.prev_end()), kind)
    }

    fn member_kind(&mut self) -> MemberKind {
        if self.eat_kw("use") {
            let mut traits = vec![self.name()];
            while self.eat_op(",") {
                traits.push(self.name());
            }
            let mut adaptations = Vec::new();
            if self.eat_op("{") {
                while !self.is_op("}") {
                    adaptations.push(self.adaptation());
                }
                self.expect_op("}");
            } else {
                self.expect_op(";");
            }
            return MemberKind::TraitUse {
                traits,
                adaptations,
            };
        }
        if self.is_kw("case") {
            self.bump();
            let name = self.ident();
            let value = self.eat_op("=").then(|| self.expr());
            self.expect_op(";");
            return MemberKind::EnumCase {
                attributes: vec![],
                name: name.into(),
                value,
            };
        }
        let mut modifiers = self.modifiers();
        if self.eat_kw("var") {
            modifiers.push(Modifier::Public);
        }
        if self.eat_kw("const") {
            let type_hint = (matches!(self.peek_at(0), T::Id(_))
                && matches!(self.peek_at(1), T::Id(_) | T::Op("|")))
            .then(|| self.type_hint());
            let items = self.const_items();
            self.expect_op(";");
            return MemberKind::Const {
                attributes: vec![],
                modifiers,
                type_hint,
                items,
            };
        }
        if self.eat_kw("function") {
            let f = self.function_rest(modifiers);
            return MemberKind::Method(Box::new(f));
        }
        let type_hint = (!matches!(self.tok().t, T::Var(_))).then(|| self.type_hint());
        let mut items = Vec::new();
        loop {
            let item_start = self.start();
            let T::Var(name) = self.bump().t else {
                panic!("expected a property in {:?}", self.src);
            };
            let default = self.eat_op("=").then(|| self.expr());
            items.push(PropertyItem {
                span: sp(item_start, self.prev_end()),
                name: name.into(),
                default,
            });
            if !self.eat_op(",") {
                break;
            }
        }
        self.expect_op(";");
        MemberKind::Property {
            attributes: vec![],
            modifiers,
            type_hint,
            items,
            hooks: vec![],
        }
    }

    fn adaptation(&mut self) -> bleedthrough_syntax::TraitAdaptation {
        let start = self.start();
        let first_tok = self.start();
        let first = self.ident();
        let (trait_name, method) = if self.eat_op("::") {
            let n = Name::from_written(sp(first_tok, first_tok + first.len() as u32), &first);
            (Some(n), self.ident())
        } else {
            (None, first)
        };
        let kind = if self.eat_kw("insteadof") {
            let mut instead_of = vec![self.name()];
            while self.eat_op(",") {
                instead_of.push(self.name());
            }
            TraitAdaptationKind::Precedence {
                trait_name: trait_name.expect("insteadof needs a trait"),
                method: method.into(),
                instead_of,
            }
        } else {
            assert!(self.eat_kw("as"));
            let visibility = match &self.tok().t {
                T::Id(w)
                    if matches!(
                        w.to_ascii_lowercase().as_str(),
                        "public" | "protected" | "private"
                    ) =>
                {
                    let m = modifier(w);
                    self.bump();
                    m
                }
                _ => None,
            };
            let alias = if matches!(self.tok().t, T::Id(_)) {
                Some(self.ident().into())
            } else {
                None
            };
            TraitAdaptationKind::Alias {
                trait_name,
                method: method.into(),
                visibility,
                alias,
            }
        };
        self.expect_op(";");
        Node::new(sp(start, self.prev_end()), kind)
    }

    /// After `function`: `[&] name (params) [: type] { body } | ;`.
    fn function_rest(&mut self, modifiers: Vec<Modifier>) -> FunctionLike {
        let by_ref = self.eat_op("&");
        let name = self.ident();
        let params = self.params();
        let return_type = self.eat_op(":").then(|| self.type_hint());
        let body = if self.is_op("{") {
            Some(self.block_inner())
        } else {
            self.expect_op(";");
            None
        };
        FunctionLike {
            name: name.into(),
            attributes: vec![],
            modifiers,
            by_ref,
            params,
            return_type,
            body,
        }
    }

    fn params(&mut self) -> Vec<Param> {
        self.expect_op("(");
        let mut out = Vec::new();
        while !self.is_op(")") {
            let start = self.start();
            let attributes = self.attrs();
            let modifiers = self.modifiers();
            let type_hint =
                (!matches!(self.tok().t, T::Var(_)) && !self.is_op("&") && !self.is_op("..."))
                    .then(|| self.type_hint());
            let by_ref = self.eat_op("&");
            let variadic = self.eat_op("...");
            let T::Var(name) = self.bump().t else {
                panic!("expected a parameter name in {:?}", self.src);
            };
            let default = self.eat_op("=").then(|| self.expr());
            out.push(Param {
                span: sp(start, self.prev_end()),
                attributes,
                modifiers,
                type_hint,
                by_ref,
                variadic,
                name: name.into(),
                default,
                hooks: vec![],
            });
            if !self.eat_op(",") {
                break;
            }
        }
        self.expect_op(")");
        out
    }

    // ---- types ----

    fn single_type(&mut self) -> TypeHint {
        let start = self.start();
        if self.eat_op("?") {
            let inner = self.single_type();
            return Node::new(
                sp(start, self.prev_end()),
                TypeKind::Nullable(Box::new(inner)),
            );
        }
        if self.eat_op("(") {
            let inner = self.type_hint();
            self.expect_op(")");
            return Node::new(
                sp(start, self.prev_end()),
                TypeKind::Parenthesized(Box::new(inner)),
            );
        }
        let tok = self.bump();
        let T::Id(w) = tok.t else {
            panic!("expected a type, found {:?} in {:?}", tok.t, self.src);
        };
        let kind = match builtin(&w) {
            Some(b) => TypeKind::Builtin(b),
            None => TypeKind::Named(Name::from_written(sp(tok.s, tok.e), &w)),
        };
        Node::new(sp(tok.s, tok.e), kind)
    }

    fn type_hint(&mut self) -> TypeHint {
        let start = self.start();
        let first = self.single_type();
        if self.is_op("|") {
            let mut parts = vec![first];
            while self.eat_op("|") {
                parts.push(self.single_type());
            }
            return Node::new(sp(start, self.prev_end()), TypeKind::Union(parts));
        }
        let intersection_next = self.is_op("&")
            && !matches!(self.peek_at(1), T::Var(_))
            && !self.op_at(1, "...")
            && !self.op_at(1, "&");
        if intersection_next {
            let mut parts = vec![first];
            while self.is_op("&") && !matches!(self.peek_at(1), T::Var(_)) && !self.op_at(1, "...")
            {
                self.bump();
                parts.push(self.single_type());
            }
            return Node::new(sp(start, self.prev_end()), TypeKind::Intersection(parts));
        }
        first
    }

    // ---- expressions ----

    fn mke(&self, start: u32, kind: ExprKind) -> Expr {
        Node::new(sp(start, self.prev_end()), kind)
    }

    pub(crate) fn expr(&mut self) -> Expr {
        self.expr_bp(0)
    }

    fn is_lvalue(e: &Expr) -> bool {
        matches!(
            e.kind,
            ExprKind::Variable(_)
                | ExprKind::VariableVariable(_)
                | ExprKind::Index { .. }
                | ExprKind::Property { .. }
                | ExprKind::StaticProperty { .. }
                | ExprKind::Parenthesized(_)
                | ExprKind::Array { .. }
        )
    }

    fn expr_bp(&mut self, min_bp: u8) -> Expr {
        let start = self.start();
        let mut lhs = self.unary();
        loop {
            let t = self.tok().t.clone();
            match t {
                T::Op(op) => {
                    if let Some(aop) = assign_op(op).filter(|_| Self::is_lvalue(&lhs)) {
                        self.bump();
                        let by_ref = aop == AssignOp::Assign && self.is_op("&");
                        let value = if by_ref {
                            let ref_start = self.start();
                            self.bump();
                            let inner = self.expr_bp(4);
                            Node::new(
                                sp(ref_start, self.prev_end()),
                                ExprKind::Unary {
                                    op: UnaryOp::Reference,
                                    operand: Box::new(inner),
                                },
                            )
                        } else {
                            self.expr_bp(4)
                        };
                        lhs = self.mke(
                            start,
                            ExprKind::Assign {
                                op: aop,
                                target: Box::new(lhs),
                                value: Box::new(value),
                            },
                        );
                        continue;
                    }
                    if op == "?" && min_bp <= 5 {
                        self.bump();
                        let then = if self.is_op(":") {
                            None
                        } else {
                            Some(Box::new(self.expr_bp(5)))
                        };
                        self.expect_op(":");
                        let otherwise = self.expr_bp(6);
                        lhs = self.mke(
                            start,
                            ExprKind::Conditional {
                                condition: Box::new(lhs),
                                then,
                                otherwise: Box::new(otherwise),
                            },
                        );
                        continue;
                    }
                    if op == "|>" && min_bp <= 14 {
                        self.bump();
                        let callable = self.expr_bp(15);
                        lhs = self.mke(
                            start,
                            ExprKind::Pipe {
                                input: Box::new(lhs),
                                callable: Box::new(callable),
                            },
                        );
                        continue;
                    }
                    let Some((bop, bp, right)) = binary_op(op) else {
                        break;
                    };
                    if bp < min_bp {
                        break;
                    }
                    self.bump();
                    let rhs = self.expr_bp(if right { bp } else { bp + 1 });
                    lhs = self.mke(
                        start,
                        ExprKind::Binary {
                            op: bop,
                            lhs: Box::new(lhs),
                            rhs: Box::new(rhs),
                        },
                    );
                }
                T::Id(w) => {
                    let lower = w.to_ascii_lowercase();
                    let (bop, bp) = match lower.as_str() {
                        "or" => (BinaryOp::LowOr, 1),
                        "xor" => (BinaryOp::LowXor, 2),
                        "and" => (BinaryOp::LowAnd, 3),
                        "instanceof" => (BinaryOp::Instanceof, 19),
                        _ => break,
                    };
                    if bp < min_bp {
                        break;
                    }
                    self.bump();
                    let rhs = if bop == BinaryOp::Instanceof {
                        self.class_operand()
                    } else {
                        self.expr_bp(bp + 1)
                    };
                    lhs = self.mke(
                        start,
                        ExprKind::Binary {
                            op: bop,
                            lhs: Box::new(lhs),
                            rhs: Box::new(rhs),
                        },
                    );
                }
                _ => break,
            }
        }
        lhs
    }

    /// The class operand of `new` and `instanceof`.
    /// The operand of `new`. As in the reference PHP parser, `static`, `self` and `parent` are
    /// plain names here.
    fn class_operand(&mut self) -> Expr {
        // `new (Name)`: a parenthesised bare name. The reference parser reads it as a constant
        // fetch; this dialect keeps the shape so that code handling it is exercised.
        if self.is_op("(")
            && matches!(self.toks.get(self.pos + 1).map(|t| &t.t), Some(T::Id(_)))
            && matches!(self.toks.get(self.pos + 2).map(|t| &t.t), Some(T::Op(")")))
        {
            let start = self.start();
            self.bump();
            let inner = self.class_operand();
            self.bump();
            return self.mke(start, ExprKind::Parenthesized(Box::new(inner)));
        }
        if let T::Id(w) = self.tok().t.clone() {
            let name_tok = self.bump();
            return Node::new(
                sp(name_tok.s, name_tok.e),
                ExprKind::Name(Name::from_written(sp(name_tok.s, name_tok.e), &w)),
            );
        }
        self.unary_no_call()
    }

    fn unary_no_call(&mut self) -> Expr {
        let start = self.start();
        if let T::Var(v) = self.tok().t.clone() {
            self.bump();
            let mut e = self.mke(start, ExprKind::Variable(v.into()));
            while self.is_op("->") || self.is_op("?->") || self.is_op("::") {
                e = self.postfix_step(start, e, false);
            }
            return e;
        }
        self.unary()
    }

    #[allow(
        clippy::too_many_lines,
        reason = "a dispatch over prefix operators and keywords"
    )]
    fn unary(&mut self) -> Expr {
        let start = self.start();
        let t = self.tok().t.clone();
        match t {
            T::Op(op) => {
                if op == "$" {
                    self.bump();
                    let inner = self.unary_no_call();
                    return self.mke(start, ExprKind::VariableVariable(Box::new(inner)));
                }
                let unary_op = match op {
                    "!" => Some((UnaryOp::Not, 18)),
                    "-" => Some((UnaryOp::Neg, 20)),
                    "+" => Some((UnaryOp::Plus, 20)),
                    "~" => Some((UnaryOp::BitNot, 20)),
                    "@" => Some((UnaryOp::ErrorSuppress, 20)),
                    "&" => Some((UnaryOp::Reference, 20)),
                    "++" => Some((UnaryOp::PreInc, 20)),
                    "--" => Some((UnaryOp::PreDec, 20)),
                    _ => None,
                };
                if let Some((u, bp)) = unary_op {
                    self.bump();
                    let operand = self.expr_bp(bp);
                    return self.mke(
                        start,
                        ExprKind::Unary {
                            op: u,
                            operand: Box::new(operand),
                        },
                    );
                }
                self.postfix_expr()
            }
            T::Cast(kind) => {
                self.bump();
                let operand = self.expr_bp(20);
                self.mke(
                    start,
                    ExprKind::Cast {
                        kind,
                        operand: Box::new(operand),
                    },
                )
            }
            T::Id(w) => {
                let lower = w.to_ascii_lowercase();
                match lower.as_str() {
                    "new" if self.kw_at(1, "class") => {
                        self.bump();
                        self.bump();
                        let args = if self.is_op("(") { self.args() } else { vec![] };
                        let mut extends = Vec::new();
                        let mut implements = Vec::new();
                        if self.eat_kw("extends") {
                            extends.push(self.name());
                        }
                        if self.eat_kw("implements") {
                            implements.push(self.name());
                            while self.eat_op(",") {
                                implements.push(self.name());
                            }
                        }
                        self.expect_op("{");
                        let mut members = Vec::new();
                        while !self.is_op("}") {
                            members.push(self.member());
                        }
                        self.expect_op("}");
                        let class = ClassLike {
                            kind: ClassLikeKind::Class,
                            name: None,
                            attributes: vec![],
                            modifiers: vec![],
                            extends,
                            implements,
                            backing_type: None,
                            members,
                        };
                        self.mke(
                            start,
                            ExprKind::AnonymousClass {
                                args,
                                class: Box::new(class),
                            },
                        )
                    }
                    "new" => {
                        self.bump();
                        let class = self.class_operand();
                        let args = if self.is_op("(") { self.args() } else { vec![] };
                        let node = self.mke(
                            start,
                            ExprKind::New {
                                class: Box::new(class),
                                args,
                            },
                        );
                        self.postfix_from(start, node)
                    }
                    "clone" => {
                        self.bump();
                        let operand = self.expr_bp(20);
                        self.mke(start, ExprKind::Clone(Box::new(operand)))
                    }
                    "print" => {
                        self.bump();
                        let operand = self.expr_bp(5);
                        self.mke(start, ExprKind::Print(Box::new(operand)))
                    }
                    "throw" => {
                        self.bump();
                        let operand = self.expr_bp(0);
                        self.mke(start, ExprKind::Throw(Box::new(operand)))
                    }
                    "yield" => {
                        self.bump();
                        if self.eat_kw("from") {
                            let operand = self.expr_bp(5);
                            return self.mke(start, ExprKind::YieldFrom(Box::new(operand)));
                        }
                        if self.is_op(";") || self.is_op(")") || self.is_op(",") || self.is_op("]")
                        {
                            return self.mke(
                                start,
                                ExprKind::Yield {
                                    key: None,
                                    value: None,
                                },
                            );
                        }
                        let first = self.expr_bp(5);
                        if self.eat_op("=>") {
                            let value = self.expr_bp(5);
                            self.mke(
                                start,
                                ExprKind::Yield {
                                    key: Some(Box::new(first)),
                                    value: Some(Box::new(value)),
                                },
                            )
                        } else {
                            self.mke(
                                start,
                                ExprKind::Yield {
                                    key: None,
                                    value: Some(Box::new(first)),
                                },
                            )
                        }
                    }
                    "include" | "include_once" | "require" | "require_once" => {
                        self.bump();
                        let kind = match lower.as_str() {
                            "include" => IncludeKind::Include,
                            "include_once" => IncludeKind::IncludeOnce,
                            "require" => IncludeKind::Require,
                            _ => IncludeKind::RequireOnce,
                        };
                        let path = self.expr_bp(5);
                        self.mke(
                            start,
                            ExprKind::Include {
                                kind,
                                path: Box::new(path),
                            },
                        )
                    }
                    _ => self.postfix_expr(),
                }
            }
            _ => self.postfix_expr(),
        }
    }

    fn postfix_expr(&mut self) -> Expr {
        let start = self.start();
        let primary = self.primary();
        self.postfix_from(start, primary)
    }

    fn postfix_from(&mut self, start: u32, mut e: Expr) -> Expr {
        loop {
            if self.is_op("[")
                || self.is_op("->")
                || self.is_op("?->")
                || self.is_op("::")
                || self.is_op("(")
                || self.is_op("++")
                || self.is_op("--")
            {
                e = self.postfix_step(start, e, true);
            } else {
                break;
            }
        }
        e
    }

    fn member_name(&mut self) -> MemberName {
        if self.is_op("{") {
            self.bump();
            let inner = self.expr();
            self.expect_op("}");
            return MemberName::Dynamic(Box::new(inner));
        }
        let tok = self.bump();
        match tok.t {
            T::Id(w) => MemberName::Identifier {
                name: w.into(),
                span: sp(tok.s, tok.e),
            },
            T::Var(v) => MemberName::Dynamic(Box::new(Node::new(
                sp(tok.s, tok.e),
                ExprKind::Variable(v.into()),
            ))),
            other => panic!("unsupported member name {other:?}"),
        }
    }

    fn postfix_step(&mut self, start: u32, e: Expr, calls: bool) -> Expr {
        if self.eat_op("[") {
            if self.eat_op("]") {
                return self.mke(
                    start,
                    ExprKind::Index {
                        array: Box::new(e),
                        index: None,
                    },
                );
            }
            let index = self.expr();
            self.expect_op("]");
            return self.mke(
                start,
                ExprKind::Index {
                    array: Box::new(e),
                    index: Some(Box::new(index)),
                },
            );
        }
        if self.is_op("->") || self.is_op("?->") {
            let nullsafe = self.is_op("?->");
            self.bump();
            let member = self.member_name();
            if calls && self.is_op("(") {
                if self.first_class_callable() {
                    return self.mke(
                        start,
                        ExprKind::CallableRef {
                            target: CallTarget::Method {
                                object: Box::new(e),
                                member,
                                nullsafe,
                            },
                        },
                    );
                }
                let args = self.args();
                return self.mke(
                    start,
                    ExprKind::Call {
                        target: CallTarget::Method {
                            object: Box::new(e),
                            member,
                            nullsafe,
                        },
                        args,
                    },
                );
            }
            return self.mke(
                start,
                ExprKind::Property {
                    object: Box::new(e),
                    member,
                    nullsafe,
                },
            );
        }
        if self.eat_op("::") {
            if let T::Var(v) = self.tok().t.clone() {
                let var_start = self.start();
                self.bump();
                let name = Node::new(sp(var_start, self.prev_end()), ExprKind::Variable(v.into()));
                return self.mke(
                    start,
                    ExprKind::StaticProperty {
                        class: Box::new(e),
                        name: Box::new(name),
                    },
                );
            }
            let member = self.member_name();
            if calls && self.is_op("(") {
                if self.first_class_callable() {
                    return self.mke(
                        start,
                        ExprKind::CallableRef {
                            target: CallTarget::Static {
                                class: Box::new(e),
                                member,
                            },
                        },
                    );
                }
                let args = self.args();
                return self.mke(
                    start,
                    ExprKind::Call {
                        target: CallTarget::Static {
                            class: Box::new(e),
                            member,
                        },
                        args,
                    },
                );
            }
            return self.mke(
                start,
                ExprKind::ClassConst {
                    class: Box::new(e),
                    member,
                },
            );
        }
        if self.is_op("(") {
            if self.first_class_callable() {
                return self.mke(
                    start,
                    ExprKind::CallableRef {
                        target: CallTarget::Function(Box::new(e)),
                    },
                );
            }
            let args = self.args();
            return self.mke(
                start,
                ExprKind::Call {
                    target: CallTarget::Function(Box::new(e)),
                    args,
                },
            );
        }
        let op = if self.eat_op("++") {
            UnaryOp::PostInc
        } else {
            self.expect_op("--");
            UnaryOp::PostDec
        };
        self.mke(
            start,
            ExprKind::Unary {
                op,
                operand: Box::new(e),
            },
        )
    }

    /// `(...)` right here: consumes it and answers true.
    fn first_class_callable(&mut self) -> bool {
        if self.op_at(0, "(") && self.op_at(1, "...") && self.op_at(2, ")") {
            self.bump();
            self.bump();
            self.bump();
            true
        } else {
            false
        }
    }

    fn args(&mut self) -> Vec<Arg> {
        self.expect_op("(");
        let mut out = Vec::new();
        while !self.is_op(")") {
            let start = self.start();
            let kind = if self.eat_op("...") {
                ArgKind::Spread(self.expr())
            } else if matches!(self.tok().t, T::Id(_)) && self.op_at(1, ":") && !self.op_at(1, "::")
            {
                let name = self.ident();
                self.expect_op(":");
                ArgKind::Named {
                    name: name.into(),
                    value: self.expr(),
                }
            } else {
                ArgKind::Positional(self.expr())
            };
            out.push(Node::new(sp(start, self.prev_end()), kind));
            if !self.eat_op(",") {
                break;
            }
        }
        self.expect_op(")");
        out
    }

    fn array_items(&mut self, close: &str) -> Vec<ArrayItem> {
        let mut items = Vec::new();
        while !self.is_op(close) {
            let start = self.start();
            if self.is_op(",") {
                self.bump();
                items.push(Node::new(sp(start, start), ArrayItemKind::Missing));
                continue;
            }
            let kind = if self.eat_op("...") {
                ArrayItemKind::Spread(self.expr())
            } else {
                let first = self.expr();
                if self.eat_op("=>") {
                    ArrayItemKind::KeyValue {
                        key: first,
                        value: self.expr(),
                    }
                } else {
                    ArrayItemKind::Value(first)
                }
            };
            items.push(Node::new(sp(start, self.prev_end()), kind));
            if !self.eat_op(",") {
                break;
            }
        }
        items
    }

    #[allow(clippy::too_many_lines, reason = "a dispatch over primary expressions")]
    fn primary(&mut self) -> Expr {
        let start = self.start();
        let tok = self.bump();
        match tok.t.clone() {
            T::Var(v) => self.mke(start, ExprKind::Variable(v.into())),
            T::Int(raw) => self.mke(start, ExprKind::Literal(Literal::Int { raw: raw.into() })),
            T::Float(raw) => self.mke(start, ExprKind::Literal(Literal::Float { raw: raw.into() })),
            T::Str(raw) => self.string(&tok.s, &tok.e, raw),
            T::Op("(") => {
                // `(Name)::`: as for `new (Name)`, keep a parenthesised bare name a name.
                let bare = matches!(self.toks.get(self.pos).map(|t| &t.t), Some(T::Id(_)))
                    && matches!(self.toks.get(self.pos + 1).map(|t| &t.t), Some(T::Op(")")))
                    && matches!(self.toks.get(self.pos + 2).map(|t| &t.t), Some(T::Op("::")));
                let inner = if bare {
                    self.class_operand()
                } else {
                    self.expr()
                };
                self.expect_op(")");
                self.mke(start, ExprKind::Parenthesized(Box::new(inner)))
            }
            T::Op("[") => {
                let items = self.array_items("]");
                self.expect_op("]");
                self.mke(
                    start,
                    ExprKind::Array {
                        style: ArrayStyle::Short,
                        items,
                    },
                )
            }
            T::Id(w) => self.id_primary(start, &tok, &w),
            other => panic!("unexpected {other:?} at {start} in {:?}", self.src),
        }
    }

    fn string(&mut self, start: &u32, end: &u32, raw: String) -> Expr {
        let span = sp(*start, *end);
        if raw.starts_with('\'') {
            return Node::new(
                span,
                ExprKind::Literal(Literal::String {
                    raw: raw.into(),
                    quote: QuoteKind::Single,
                }),
            );
        }
        let shell = raw.starts_with('`');
        let inner = &raw[1..raw.len() - 1];
        if !inner.contains('$') && !shell {
            return Node::new(
                span,
                ExprKind::Literal(Literal::String {
                    raw: raw.into(),
                    quote: QuoteKind::Double,
                }),
            );
        }
        let base = *start + 1;
        let mut parts = Vec::new();
        let b = inner.as_bytes();
        let mut i = 0usize;
        let mut text_start = 0usize;
        let at = |i: usize| base + u32::try_from(i).expect("small source");
        while i < b.len() {
            if b[i] == b'$'
                && i + 1 < b.len()
                && (b[i + 1].is_ascii_alphabetic() || b[i + 1] == b'_')
            {
                if text_start < i {
                    parts.push(Node::new(
                        sp(at(text_start), at(i)),
                        StringPartKind::Text {
                            raw: inner[text_start..i].into(),
                        },
                    ));
                }
                let mut j = i + 1;
                while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                    j += 1;
                }
                let var = Node::new(sp(at(i), at(j)), ExprKind::Variable(inner[i + 1..j].into()));
                parts.push(Node::new(
                    sp(at(i), at(j)),
                    StringPartKind::Interpolation(Box::new(var)),
                ));
                i = j;
                text_start = j;
            } else {
                i += 1;
            }
        }
        if text_start < b.len() {
            parts.push(Node::new(
                sp(at(text_start), at(b.len())),
                StringPartKind::Text {
                    raw: inner[text_start..].into(),
                },
            ));
        }
        Node::new(
            span,
            ExprKind::Interpolated {
                kind: if shell {
                    StringKind::ShellExec
                } else {
                    StringKind::DoubleQuoted
                },
                parts,
            },
        )
    }

    #[allow(clippy::too_many_lines, reason = "a dispatch over keyword expressions")]
    fn id_primary(&mut self, start: u32, tok: &Tok, w: &str) -> Expr {
        let lower = w.to_ascii_lowercase();
        let name_span = sp(tok.s, tok.e);
        match lower.as_str() {
            // the lower-case keywords are literals; other spellings stay constant fetches
            _ if matches!(w, "true" | "false" | "null") => {
                let literal = match w {
                    "true" => Literal::Bool(true),
                    "false" => Literal::Bool(false),
                    _ => Literal::Null,
                };
                self.mke(start, ExprKind::Literal(literal))
            }
            "self" if self.is_op("::") => {
                self.mke(start, ExprKind::ClassRef(ClassRefKind::SelfRef))
            }
            "parent" if self.is_op("::") => {
                self.mke(start, ExprKind::ClassRef(ClassRefKind::Parent))
            }
            "static" if self.is_op("::") => {
                self.mke(start, ExprKind::ClassRef(ClassRefKind::Static))
            }
            "array" if self.is_op("(") => {
                self.bump();
                let items = self.array_items(")");
                self.expect_op(")");
                self.mke(
                    start,
                    ExprKind::Array {
                        style: ArrayStyle::Long,
                        items,
                    },
                )
            }
            "list" if self.is_op("(") => {
                self.bump();
                let items = self.array_items(")");
                self.expect_op(")");
                self.mke(
                    start,
                    ExprKind::Array {
                        style: ArrayStyle::List,
                        items,
                    },
                )
            }
            "isset" => {
                self.expect_op("(");
                let items = self.expr_list_until(")");
                self.expect_op(")");
                self.mke(start, ExprKind::Isset(items))
            }
            "empty" => {
                self.expect_op("(");
                let inner = self.expr();
                self.expect_op(")");
                self.mke(start, ExprKind::Empty(Box::new(inner)))
            }
            "eval" => {
                self.expect_op("(");
                let inner = self.expr();
                self.expect_op(")");
                self.mke(start, ExprKind::Eval(Box::new(inner)))
            }
            "exit" | "die" => {
                let args = if self.is_op("(") { self.args() } else { vec![] };
                self.mke(
                    start,
                    ExprKind::Exit {
                        die: lower == "die",
                        args,
                    },
                )
            }
            "match" if self.is_op("(") => {
                self.expect_op("(");
                let subject = self.expr();
                self.expect_op(")");
                self.expect_op("{");
                let mut arms = Vec::new();
                while !self.is_op("}") {
                    let arm_start = self.start();
                    let mut conditions = Vec::new();
                    let is_default = self.eat_kw("default");
                    if !is_default {
                        loop {
                            conditions.push(self.expr());
                            if !self.eat_op(",") || self.is_op("=>") {
                                break;
                            }
                        }
                    }
                    self.expect_op("=>");
                    let body = self.expr();
                    arms.push(MatchArm {
                        span: sp(arm_start, self.prev_end()),
                        conditions,
                        is_default,
                        body,
                    });
                    if !self.eat_op(",") {
                        break;
                    }
                }
                self.expect_op("}");
                self.mke(
                    start,
                    ExprKind::Match {
                        subject: Box::new(subject),
                        arms,
                    },
                )
            }
            "function" | "fn" => self.closure(start, lower == "fn", false),
            "static" if self.is_kw("function") || self.is_kw("fn") => {
                let arrow = self.is_kw("fn");
                self.bump();
                self.closure(start, arrow, true)
            }
            _ => {
                if let Some(kind) = magic(w) {
                    return self.mke(start, ExprKind::MagicConstant(kind));
                }
                let name = Name::from_written(name_span, w);
                if self.is_op("(") || self.is_op("::") {
                    self.mke(start, ExprKind::Name(name))
                } else {
                    self.mke(start, ExprKind::ConstFetch(name))
                }
            }
        }
    }

    fn closure(&mut self, start: u32, arrow: bool, is_static: bool) -> Expr {
        let by_ref = self.eat_op("&");
        let params = self.params();
        if arrow {
            let return_type = self.eat_op(":").then(|| Box::new(self.type_hint()));
            self.expect_op("=>");
            let body = self.expr_bp(4);
            return self.mke(
                start,
                ExprKind::ArrowFn {
                    attributes: vec![],
                    is_static,
                    by_ref,
                    params,
                    return_type,
                    body: Box::new(body),
                },
            );
        }
        let mut uses = Vec::new();
        if self.eat_kw("use") {
            self.expect_op("(");
            while !self.is_op(")") {
                let use_start = self.start();
                let by_ref = self.eat_op("&");
                let T::Var(name) = self.bump().t else {
                    panic!("expected a captured variable");
                };
                uses.push(ClosureUse {
                    span: sp(use_start, self.prev_end()),
                    name: name.into(),
                    by_ref,
                });
                if !self.eat_op(",") {
                    break;
                }
            }
            self.expect_op(")");
        }
        let return_type = self.eat_op(":").then(|| Box::new(self.type_hint()));
        let body = self.block_inner();
        self.mke(
            start,
            ExprKind::Closure {
                attributes: vec![],
                is_static,
                by_ref,
                params,
                uses,
                return_type,
                body,
            },
        )
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers for the tests

/// The summary of one file.
pub fn summary(path: &str, src: &str) -> FileSummary {
    summarize(path, &parse(src), src)
}

/// A symbol table over files `(path, source)`.
pub fn table_of(files: &[(&str, &str)]) -> SymbolTable {
    SymbolTable::build(files.iter().map(|(p, s)| summary(p, s)).collect())
}

/// A table of one source file named `a.php`.
pub fn table(src: &str) -> SymbolTable {
    table_of(&[("a.php", src)])
}

/// The facts of every unit of `src`, analysed against a table built from `src` itself.
pub fn infer(src: &str) -> Vec<FunctionFacts> {
    infer_with(src, &table(src), &InferLimits::default())
}

/// The facts of `src` against a given table and limits.
pub fn infer_with(src: &str, table: &SymbolTable, limits: &InferLimits) -> Vec<FunctionFacts> {
    infer_file(&parse(src), table, limits)
}

/// The unit called `name` among `facts`.
pub fn unit<'a>(facts: &'a [FunctionFacts], name: &str) -> &'a FunctionFacts {
    facts.iter().find(|f| &*f.name == name).unwrap_or_else(|| {
        panic!(
            "no unit {name}; have {:?}",
            facts.iter().map(|f| &f.name).collect::<Vec<_>>()
        )
    })
}

pub fn key_text<'a>(src: &'a str, k: &ExprKey) -> &'a str {
    &src[k.start as usize..k.end as usize]
}

/// The type text of the `nth` (0-based) expression whose source text is exactly `needle`.
pub fn ty_nth(facts: &FunctionFacts, src: &str, needle: &str, nth: usize) -> String {
    facts
        .expr_types
        .iter()
        .filter(|(k, _)| key_text(src, k) == needle)
        .nth(nth)
        .map(|(_, t)| t.to_string())
        .unwrap_or_else(|| panic!("no expression {needle:?} #{nth} in {:?}", facts.name))
}

/// The type text of the first expression whose source text is exactly `needle`.
pub fn ty(facts: &FunctionFacts, src: &str, needle: &str) -> String {
    ty_nth(facts, src, needle, 0)
}

/// The type text of the last expression whose source text is exactly `needle` (it has seen the
/// most flow).
pub fn ty_last(facts: &FunctionFacts, src: &str, needle: &str) -> String {
    let all: Vec<_> = facts
        .expr_types
        .iter()
        .filter(|(k, _)| key_text(src, k) == needle)
        .collect();
    all.last()
        .map(|(_, t)| t.to_string())
        .unwrap_or_else(|| panic!("no expression {needle:?} in {:?}", facts.name))
}

/// The call resolution rendered for the first call expression whose text is `needle`.
pub fn call(facts: &FunctionFacts, src: &str, needle: &str) -> String {
    facts
        .calls
        .iter()
        .find(|(k, _)| key_text(src, k) == needle)
        .map(|(_, r)| r.to_string())
        .unwrap_or_else(|| panic!("no call {needle:?} in {:?}", facts.name))
}

/// Parses the type text `text` in the global scope.
pub fn t(text: &str) -> Type {
    parse_type(text, &TypeContext::new(), 0)
        .ty
        .unwrap_or_else(|| panic!("not a type: {text}"))
}

/// The type of `$var` at the end of a function body `body`, whose parameters are `$c, $d, $e`
/// (untyped) and `array $xs`.
pub fn fl(body: &str, var: &str) -> String {
    let src = format!("function f($c, $d, $e, array $xs) {{ {body} $end = ${var}; }}");
    let facts = infer(&src);
    ty_last(unit(&facts, "f"), &src, &format!("$end = ${var}"))
}

/// The resolution of the call-like expression written `needle`, displayed; the last one if it
/// occurs several times.
pub fn call_last(facts: &FunctionFacts, src: &str, needle: &str) -> String {
    facts
        .calls
        .iter()
        .rfind(|(k, _)| key_text(src, k) == needle)
        .map(|(_, r)| r.to_string())
        .unwrap_or_else(|| {
            panic!(
                "no call {needle:?} in {:?}; have {:?}",
                facts.name,
                facts
                    .calls
                    .iter()
                    .map(|(k, _)| key_text(src, k))
                    .collect::<Vec<_>>()
            )
        })
}

/// The type of `expr` evaluated after `setup` inside a function with a fixed set of typed
/// parameters: `int $i, float $f, string $s, bool $b, ?int $n, array $a, mixed $m, $u`.
pub fn ex(setup: &str, expr: &str) -> String {
    let src = format!(
        "function f(int $i, float $f, string $s, bool $b, ?int $n, array $a, mixed $m, $u) {{ {setup} $r = {expr}; }}"
    );
    let facts = infer(&src);
    ty_last(unit(&facts, "f"), &src, expr)
}

/// Declarations the narrowing tests share.
pub const PRELUDE: &str = "interface I {} class A {} class B extends A implements I {} class C {} final class F {} enum E { case X; case Y; }";

/// The type of `$v` in the true and the false branch of `if (cond)`, for a function whose
/// parameters are `params`, with `doc` as its docblock body. A branch the analysis considers
/// unreachable gives `dead`.
pub fn nr(doc: &str, params: &str, cond: &str) -> (String, String) {
    let comment = if doc.is_empty() {
        String::new()
    } else {
        format!("/** {doc} */")
    };
    let src = format!(
        "{PRELUDE}\n{comment} function f({params}) {{ if ({cond}) {{ $yes = $v; }} else {{ $no = $v; }} }}"
    );
    let facts = infer(&src);
    let f = unit(&facts, "f");
    let get = |needle: &str| {
        f.expr_types
            .iter()
            .find(|(k, _)| &src[k.start as usize..k.end as usize] == needle)
            .map_or_else(|| "dead".to_string(), |(_, t)| t.to_string())
    };
    (get("$yes = $v"), get("$no = $v"))
}
