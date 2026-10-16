//! Field validators: what a record may hold. They keep the data to names, type text, flags,
//! versions and expression text; free-form sentences are rejected.

use crate::record::{
    ClassDetail, ConstantDetail, Detail, FunctionDetail, Kind, MethodDetail, Param, PropertyDetail,
    Record, Signature,
};

const MAX_TYPE: usize = 512;
const MAX_EXPR: usize = 1024;
const MAX_MESSAGE: usize = 200;

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || !c.is_ascii()
}

fn is_ident_part(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || !c.is_ascii()
}

/// A PHP identifier.
fn ident(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(is_ident_start) && chars.all(is_ident_part)
}

/// A namespaced name without a leading backslash.
fn qualified(text: &str) -> bool {
    !text.is_empty() && text.split('\\').all(ident)
}

/// Checks a symbol name for its kind: a qualified name, or `Class::member` for a member.
pub(crate) fn name(kind: Kind, text: &str) -> Result<(), String> {
    let ok = if kind.is_member() {
        text.split_once("::")
            .is_some_and(|(class, member)| qualified(class) && ident(member))
    } else {
        qualified(text)
    };
    ok.then_some(())
        .ok_or_else(|| format!("`{text}` is not a valid {} name", kind.as_str()))
}

/// A name that refers to another symbol (alias target, parent, interface).
fn reference(text: &str) -> Result<(), String> {
    let ok = match text.split_once("::") {
        Some((class, member)) => qualified(class) && ident(member),
        None => qualified(text),
    };
    ok.then_some(())
        .ok_or_else(|| format!("`{text}` is not a valid symbol reference"))
}

/// The word-and-symbol tokens of text outside string literals; `Err` on an unterminated string.
/// Strings are reported as one `"` token.
fn tokens(text: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' | '"' => {
                loop {
                    match chars.next() {
                        Some('\\') => {
                            chars.next();
                        }
                        Some(q) if q == c => break,
                        Some(_) => {}
                        None => return Err("unterminated string".to_string()),
                    }
                }
                out.push("\"".to_string());
            }
            c if c.is_whitespace() => out.push(" ".to_string()),
            c if is_ident_part(c) || c == '\\' || c == '-' || c == '$' => {
                let mut word = String::from(c);
                while let Some(&n) = chars.peek() {
                    if is_ident_part(n) || n == '\\' || n == '-' || n == '.' {
                        word.push(n);
                        chars.next();
                    } else {
                        break;
                    }
                }
                out.push(word);
            }
            other => out.push(other.to_string()),
        }
    }
    Ok(out)
}

fn is_word(token: &str) -> bool {
    token
        .chars()
        .next()
        .is_some_and(|c| is_ident_part(c) || c == '\\')
}

/// Two words separated only by white space, outside `allowed_first`, read as prose.
fn adjacent_words(tokens: &[String], allowed_first: &[&str]) -> Option<String> {
    let mut previous_word: Option<&str> = None;
    let mut gap = false;
    for token in tokens {
        if token == " " {
            gap = previous_word.is_some();
            continue;
        }
        if is_word(token) || token == "\"" {
            if let (true, Some(first)) = (gap, previous_word)
                && !allowed_first.contains(&first)
            {
                return Some(format!("`{first} {token}` reads as prose"));
            }
            previous_word = Some(token);
        } else {
            previous_word = None;
        }
        gap = false;
    }
    None
}

fn clean(text: &str, limit: usize, what: &str) -> Result<(), String> {
    if text.len() > limit {
        return Err(format!("{what} is longer than {limit} bytes"));
    }
    if text.chars().any(char::is_control) {
        return Err(format!("{what} contains a control character"));
    }
    if text != text.trim() {
        return Err(format!("{what} has leading or trailing white space"));
    }
    Ok(())
}

/// PHP type text, including the docblock extensions `array<K, V>`, `int<0, max>`, shapes and
/// callable signatures. Empty means no type. No quoted text, no `$`, no prose.
pub(crate) fn type_text(text: &str) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    clean(text, MAX_TYPE, "type")?;
    let allowed = |c: char| is_ident_part(c) || " \\|&?()<>,:{}[]*-.=".contains(c);
    if let Some(bad) = text.chars().find(|c| !allowed(*c)) {
        return Err(format!("type `{text}` contains `{bad}`"));
    }
    let tokens = tokens(text)?;
    match adjacent_words(&tokens, &[]) {
        Some(problem) => Err(format!("type `{text}`: {problem}")),
        None => Ok(()),
    }
}

/// A PHP expression as source text (default values and constant values).
pub(crate) fn expr_text(text: &str) -> Result<(), String> {
    clean(text, MAX_EXPR, "expression")?;
    if text.is_empty() {
        return Err("expression is empty".to_string());
    }
    let tokens = tokens(text).map_err(|e| format!("expression `{text}`: {e}"))?;
    match adjacent_words(&tokens, &["new"]) {
        Some(problem) => Err(format!("expression `{text}`: {problem}")),
        None => Ok(()),
    }
}

/// A preprocessor condition: identifiers, `defined(...)`, `!`, `&&`, `||` and parentheses.
fn guard_text(text: &str) -> Result<(), String> {
    clean(text, MAX_EXPR, "guard")?;
    if text.is_empty() {
        return Err("guard is empty".to_string());
    }
    let allowed = |c: char| is_ident_part(c) || " !&|()<>=,.+-*/".contains(c);
    if let Some(bad) = text.chars().find(|c| !allowed(*c)) {
        return Err(format!("guard `{text}` contains `{bad}`"));
    }
    let tokens = tokens(text)?;
    match adjacent_words(&tokens, &["defined"]) {
        Some(problem) => Err(format!("guard `{text}`: {problem}")),
        None => Ok(()),
    }
}

fn lower_word(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

fn param(p: &Param) -> Result<(), String> {
    if !ident(&p.name) {
        return Err(format!("`{}` is not a valid parameter name", p.name));
    }
    type_text(&p.type_text)?;
    type_text(&p.doc_type)?;
    if let Some(default) = &p.default {
        expr_text(default)?;
        if !p.optional {
            return Err(format!(
                "parameter ${} has a default but is not optional",
                p.name
            ));
        }
    }
    if p.variadic && !p.optional {
        return Err(format!("variadic parameter ${} must be optional", p.name));
    }
    Ok(())
}

fn signature(s: &Signature) -> Result<(), String> {
    s.params.iter().try_for_each(param)?;
    type_text(&s.return_type)?;
    type_text(&s.tentative_return_type)?;
    type_text(&s.doc_return_type)?;
    if !s.return_type.is_empty() && !s.tentative_return_type.is_empty() {
        return Err("a tentative return type excludes a declared one".to_string());
    }
    Ok(())
}

fn alias(target: Option<&String>) -> Result<(), String> {
    target.map_or(Ok(()), |t| reference(t))
}

fn function(d: &FunctionDetail) -> Result<(), String> {
    signature(&d.signature)?;
    alias(d.alias_of.as_ref())
}

fn method(d: &MethodDetail) -> Result<(), String> {
    signature(&d.signature)?;
    alias(d.alias_of.as_ref())?;
    if d.is_final && d.is_abstract {
        return Err("a method cannot be both final and abstract".to_string());
    }
    Ok(())
}

fn class(d: &ClassDetail) -> Result<(), String> {
    d.extends
        .iter()
        .chain(&d.implements)
        .try_for_each(|n| reference(n))?;
    type_text(&d.backing_type)
}

fn property(d: &PropertyDetail) -> Result<(), String> {
    type_text(&d.type_text)?;
    type_text(&d.doc_type)?;
    d.default.as_deref().map_or(Ok(()), expr_text)
}

fn constant(d: &ConstantDetail) -> Result<(), String> {
    type_text(&d.type_text)?;
    d.value.as_deref().map_or(Ok(()), expr_text)
}

/// Checks that a record holds only what the model allows. The message names the problem; the
/// caller adds the symbol or the line.
pub fn check_record(record: &Record) -> Result<(), String> {
    name(record.kind(), &record.name)?;
    if record.mask.is_empty() {
        return Err("the version mask is empty".to_string());
    }
    if !record.reflected.is_subset_of(record.mask) {
        return Err("the Reflection mask is not a subset of the version mask".to_string());
    }
    if !lower_word(&record.extension) {
        return Err(format!(
            "`{}` is not a valid extension name",
            record.extension
        ));
    }
    for sapi in &record.sapi {
        if !lower_word(sapi) {
            return Err(format!("`{sapi}` is not a valid SAPI name"));
        }
    }
    if !record.sapi.windows(2).all(|w| w[0] < w[1]) {
        return Err("SAPIs must be sorted and distinct".to_string());
    }
    record.guards.iter().try_for_each(|g| guard_text(g))?;
    if !record.guards.windows(2).all(|w| w[0] < w[1]) {
        return Err("guard alternatives must be sorted and distinct".to_string());
    }
    if let Some(d) = &record.deprecation {
        let since_ok = d.since.is_empty()
            || (d.since.split('.').count() == 2
                && d.since
                    .split('.')
                    .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())));
        if !since_ok {
            return Err(format!("`{}` is not a valid deprecation version", d.since));
        }
        clean(&d.message, MAX_MESSAGE, "deprecation message")?;
    }
    match &record.detail {
        Detail::Constant(d) | Detail::ClassConstant(d) => constant(d),
        Detail::Function(d) => function(d),
        Detail::Class(d) | Detail::Interface(d) | Detail::Trait(d) | Detail::Enum(d) => class(d),
        Detail::EnumCase(d) => d.value.as_deref().map_or(Ok(()), expr_text),
        Detail::Property(d) => property(d),
        Detail::Method(d) => method(d),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_accept_php_and_docblock_syntax() {
        for ok in [
            "",
            "int",
            "?int",
            "array|false",
            "(A&B)|null",
            "\\Foo\\Bar",
            "static",
            "array<int, string>",
            "int<0, max>",
            "list<array{a: int, b?: string}>",
            "callable(): void",
            "class-string<T>",
            "non-empty-string",
            "resource",
        ] {
            assert_eq!(type_text(ok), Ok(()), "{ok}");
        }
    }

    #[test]
    fn types_reject_prose() {
        for bad in [
            "the number of bytes written",
            "int or false",
            "int\n",
            "string # comment",
            "Returns the value",
            "'quoted'",
        ] {
            assert!(type_text(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn expressions_accept_php_and_reject_sentences() {
        for ok in [
            "null",
            "[]",
            "PHP_INT_MAX",
            "E_ALL & ~E_NOTICE",
            "'a b c'",
            "\"\\t\\n\"",
            "new Foo",
            "Foo::BAR",
            "-1",
            "1.5e3",
            "[1, 2, 'x']",
            "M_PI * 2",
        ] {
            assert_eq!(expr_text(ok), Ok(()), "{ok}");
        }
        for bad in ["the default is zero", "", " 1", "'open", "1\n2", "A B"] {
            assert!(expr_text(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn guards() {
        for ok in [
            "HAVE_GETHOSTNAME",
            "!HAVE_X",
            "defined(A) && !defined(B)",
            "ZEND_DEBUG == 1",
        ] {
            assert_eq!(guard_text(ok), Ok(()), "{ok}");
        }
        assert!(guard_text("only on some machines").is_err());
        assert!(guard_text("").is_err());
    }

    #[test]
    fn names() {
        assert!(name(Kind::Function, "Foo\\bar").is_ok());
        assert!(name(Kind::Function, "\\foo").is_err());
        assert!(name(Kind::Method, "Foo::bar").is_ok());
        assert!(name(Kind::Method, "Foo").is_err());
        assert!(name(Kind::Class, "A::b").is_err());
    }
}
