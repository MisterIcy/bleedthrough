//! The stable text form of types, parsed back by the docblock type parser.

use std::fmt::{self, Display, Formatter, Write};

use super::{Atom, CallableSig, ShapeEntry, ShapeKey, Type};

impl Display for Type {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let atoms = self.atoms();
        if atoms.is_empty() {
            return f.write_str("never");
        }
        let (nulls, others): (Vec<&Atom>, Vec<&Atom>) =
            atoms.iter().partition(|a| matches!(a, Atom::Null));
        let ordered: Vec<&Atom> = others.into_iter().chain(nulls).collect();
        let many = ordered.len() > 1;
        for (i, atom) in ordered.into_iter().enumerate() {
            if i > 0 {
                f.write_char('|')?;
            }
            match atom {
                Atom::Intersection(_) if many => write!(f, "({atom})")?,
                _ => write!(f, "{atom}")?,
            }
        }
        Ok(())
    }
}

impl Display for Atom {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Atom::Array {
                key,
                value,
                non_empty,
            } => {
                let name = if *non_empty {
                    "non-empty-array"
                } else {
                    "array"
                };
                write!(f, "{name}<{}, {value}>", KeyDisplay(key))
            }
            Atom::List { value, non_empty } => {
                let name = if *non_empty { "non-empty-list" } else { "list" };
                write!(f, "{name}<{value}>")
            }
            Atom::Shape { entries, sealed } => write_shape(f, entries, *sealed),
            Atom::Named(object) => {
                f.write_str(object.name.as_str())?;
                write_args(f, &object.args)
            }
            Atom::Intersection(parts) => write_joined(f, parts, "&"),
            Atom::Callable(sig) => write_callable(f, sig.as_deref()),
            Atom::Iterable { key, value } => {
                if key.is_mixed() && value.is_mixed() {
                    f.write_str("iterable")
                } else {
                    write!(f, "iterable<{key}, {value}>")
                }
            }
            other => write_simple(f, other),
        }
    }
}

struct KeyDisplay<'a>(&'a Type);

impl Display for KeyDisplay<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if *self.0 == Type::array_key() {
            f.write_str("array-key")
        } else {
            write!(f, "{}", self.0)
        }
    }
}

fn write_simple(f: &mut Formatter<'_>, atom: &Atom) -> fmt::Result {
    match atom {
        Atom::Null => f.write_str("null"),
        Atom::Void => f.write_str("void"),
        Atom::Mixed => f.write_str("mixed"),
        Atom::Unknown => f.write_str("unknown"),
        Atom::True => f.write_str("true"),
        Atom::False => f.write_str("false"),
        Atom::Bool => f.write_str("bool"),
        Atom::Int => f.write_str("int"),
        Atom::IntLiteral(n) => write!(f, "{n}"),
        Atom::IntRange { min, max } => write_range(f, *min, *max),
        Atom::Float => f.write_str("float"),
        Atom::String => f.write_str("string"),
        Atom::NonEmptyString => f.write_str("non-empty-string"),
        Atom::NumericString => f.write_str("numeric-string"),
        Atom::StringLiteral(text) => write_quoted(f, text),
        Atom::ClassString(None) => f.write_str("class-string"),
        Atom::ClassString(Some(bound)) => write!(f, "class-string<{bound}>"),
        Atom::Object => f.write_str("object"),
        Atom::EnumCase { enum_name, case } => write!(f, "{enum_name}::{case}"),
        Atom::Static(class) => write!(f, "static({class})"),
        Atom::Template(name) => f.write_str(name),
        Atom::Resource => f.write_str("resource"),
        _ => Err(fmt::Error),
    }
}

fn write_range(f: &mut Formatter<'_>, min: Option<i64>, max: Option<i64>) -> fmt::Result {
    match (min, max) {
        (Some(1), None) => f.write_str("positive-int"),
        (Some(0), None) => f.write_str("non-negative-int"),
        (None, Some(-1)) => f.write_str("negative-int"),
        (None, Some(0)) => f.write_str("non-positive-int"),
        _ => {
            f.write_str("int<")?;
            match min {
                Some(n) => write!(f, "{n}")?,
                None => f.write_str("min")?,
            }
            f.write_str(", ")?;
            match max {
                Some(n) => write!(f, "{n}")?,
                None => f.write_str("max")?,
            }
            f.write_char('>')
        }
    }
}

fn write_quoted(f: &mut Formatter<'_>, text: &str) -> fmt::Result {
    f.write_char('\'')?;
    for c in text.chars() {
        if c == '\'' || c == '\\' {
            f.write_char('\\')?;
        }
        f.write_char(c)?;
    }
    f.write_char('\'')
}

fn write_joined<T: Display>(f: &mut Formatter<'_>, items: &[T], separator: &str) -> fmt::Result {
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            f.write_str(separator)?;
        }
        write!(f, "{item}")?;
    }
    Ok(())
}

fn write_args(f: &mut Formatter<'_>, args: &[Type]) -> fmt::Result {
    if args.is_empty() {
        return Ok(());
    }
    f.write_char('<')?;
    write_joined(f, args, ", ")?;
    f.write_char('>')
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn write_shape(f: &mut Formatter<'_>, entries: &[ShapeEntry], sealed: bool) -> fmt::Result {
    f.write_str("array{")?;
    for (i, entry) in entries.iter().enumerate() {
        if i > 0 {
            f.write_str(", ")?;
        }
        match &entry.key {
            ShapeKey::Int(n) => write!(f, "{n}")?,
            ShapeKey::Str(s) if is_identifier(s) => f.write_str(s)?,
            ShapeKey::Str(s) => write_quoted(f, s)?,
        }
        f.write_str(if entry.optional { "?: " } else { ": " })?;
        write!(f, "{}", entry.value)?;
    }
    if !sealed {
        f.write_str(if entries.is_empty() { "..." } else { ", ..." })?;
    }
    f.write_char('}')
}

fn write_callable(f: &mut Formatter<'_>, sig: Option<&CallableSig>) -> fmt::Result {
    f.write_str("callable")?;
    let Some(sig) = sig else {
        return Ok(());
    };
    f.write_char('(')?;
    for (i, param) in sig.params.iter().enumerate() {
        if i > 0 {
            f.write_str(", ")?;
        }
        if param.variadic {
            f.write_str("...")?;
        }
        write!(f, "{}", param.ty)?;
        if param.optional {
            f.write_char('=')?;
        }
    }
    write!(f, "): {}", sig.ret)
}
