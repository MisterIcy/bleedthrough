//! A lazy lexer over the text of a docblock type expression. It is pulled one token at a time, so
//! description text after a type is never scanned.

/// A token of a type expression.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Token {
    /// A name or keyword, possibly with `\` and `-` (`non-empty-array`, `\App\Foo`).
    Ident(String),
    Int(i64),
    Float,
    /// A quoted string, unescaped.
    Str(String),
    /// A `$name`.
    Var(String),
    /// A single punctuation character: `| & < > ( ) [ ] { } , : ? = * ~`.
    Punct(char),
    /// `...`
    Ellipsis,
    /// `::`
    DoubleColon,
    /// A character or construct that starts no token (an unterminated string, a stray byte).
    Bad(char),
    End,
}

/// A token with the byte range it covers.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Lexed {
    pub token: Token,
    pub start: usize,
    pub end: usize,
}

pub(super) struct Lexer<'a> {
    src: &'a str,
    pos: usize,
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '\\' || !c.is_ascii()
}

fn is_ident_part(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\\' || !c.is_ascii()
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    /// The source after the position of the lexer (the last token returned), without leading blanks.
    pub fn rest_trimmed(&self) -> &'a str {
        self.rest().trim_start()
    }

    fn peek_char(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn skip_space(&mut self) {
        let trimmed = self.rest().trim_start();
        self.pos = self.src.len() - trimmed.len();
    }

    /// The next token.
    pub fn next_token(&mut self) -> Lexed {
        self.skip_space();
        let start = self.pos;
        let token = match self.peek_char() {
            None => Token::End,
            Some(c) => self.lex_from(c),
        };
        Lexed {
            token,
            start,
            end: self.pos,
        }
    }

    fn lex_from(&mut self, c: char) -> Token {
        let rest = self.rest();
        if rest.starts_with("...") {
            self.pos += 3;
            return Token::Ellipsis;
        }
        if rest.starts_with("::") {
            self.pos += 2;
            return Token::DoubleColon;
        }
        match c {
            '\'' | '"' => self.string(c),
            '$' => self.variable(),
            c if c.is_ascii_digit() || (c == '-' && self.digit_follows()) => self.number(),
            c if is_ident_start(c) => self.ident(),
            '|' | '&' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ':' | '?' | '='
            | '*' | '~' => {
                self.pos += 1;
                Token::Punct(c)
            }
            other => {
                self.pos += other.len_utf8();
                Token::Bad(other)
            }
        }
    }

    fn digit_follows(&self) -> bool {
        self.rest()
            .chars()
            .nth(1)
            .is_some_and(|c| c.is_ascii_digit())
    }

    fn take_while(&mut self, keep: impl Fn(char) -> bool) -> &'a str {
        let rest = self.rest();
        let len = rest.find(|c| !keep(c)).unwrap_or(rest.len());
        self.pos += len;
        &rest[..len]
    }

    fn ident(&mut self) -> Token {
        let start = self.pos;
        loop {
            self.take_while(is_ident_part);
            let rest = self.rest();
            let mut chars = rest.chars();
            let hyphen_then_letter =
                chars.next() == Some('-') && chars.next().is_some_and(|c| c.is_alphabetic());
            if hyphen_then_letter {
                self.pos += 1;
            } else {
                break;
            }
        }
        Token::Ident(self.src[start..self.pos].to_string())
    }

    fn variable(&mut self) -> Token {
        self.pos += 1;
        let name = self.take_while(|c| c.is_alphanumeric() || c == '_' || !c.is_ascii());
        if name.is_empty() {
            Token::Bad('$')
        } else {
            Token::Var(name.to_string())
        }
    }

    fn number(&mut self) -> Token {
        let start = self.pos;
        if self.peek_char() == Some('-') {
            self.pos += 1;
        }
        self.take_while(|c| c.is_ascii_digit() || c == '_');
        let is_float = self.rest().starts_with('.')
            && self
                .rest()
                .chars()
                .nth(1)
                .is_some_and(|c| c.is_ascii_digit());
        if is_float {
            self.pos += 1;
            self.take_while(|c| c.is_ascii_digit() || c == '_');
            return Token::Float;
        }
        let text = self.src[start..self.pos].replace('_', "");
        text.parse().map_or(Token::Bad('0'), Token::Int)
    }

    fn string(&mut self, quote: char) -> Token {
        self.pos += 1;
        let mut text = String::new();
        loop {
            let Some(c) = self.peek_char() else {
                return Token::Bad(quote);
            };
            self.pos += c.len_utf8();
            if c == quote {
                return Token::Str(text);
            }
            if c == '\\' {
                match self.peek_char() {
                    Some(next) if next == quote || next == '\\' => {
                        self.pos += next.len_utf8();
                        text.push(next);
                    }
                    _ => text.push('\\'),
                }
            } else {
                text.push(c);
            }
        }
    }
}
