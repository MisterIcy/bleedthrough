//! Facts-to-IR conversions: type classification, class names and literal decoding.

use bleedthrough_symbols::{Atom, Fqn, Type};
use bleedthrough_syntax::{Literal, QuoteKind};

use crate::instr::{ConstValue, TypeClass};

/// Most class names recorded on one operand.
const MAX_CLASSES: usize = 16;

/// Summarises a type for taint pruning.
pub(super) fn classify(ty: &Type) -> TypeClass {
    let mut inert = true;
    for atom in ty.atoms() {
        match atom {
            Atom::Unknown | Atom::Mixed | Atom::Template(_) => return TypeClass::Unknown,
            Atom::Null
            | Atom::Void
            | Atom::True
            | Atom::False
            | Atom::Bool
            | Atom::Int
            | Atom::IntLiteral(_)
            | Atom::IntRange { .. }
            | Atom::Float
            | Atom::EnumCase { .. } => {}
            _ => inert = false,
        }
    }
    if inert {
        TypeClass::Inert
    } else {
        TypeClass::Data
    }
}

/// The class names a type denotes (objects, `static`, class strings), sorted and distinct.
pub(super) fn class_names(ty: &Type) -> Vec<Fqn> {
    let mut out: Vec<Fqn> = Vec::new();
    for atom in ty.atoms() {
        atom_classes(atom, &mut out);
    }
    out.sort();
    out.dedup();
    out.truncate(MAX_CLASSES);
    out
}

fn atom_classes(atom: &Atom, out: &mut Vec<Fqn>) {
    match atom {
        Atom::Named(object) => out.push(object.name.clone()),
        Atom::Static(name) | Atom::ClassString(Some(name)) => out.push(name.clone()),
        Atom::EnumCase { enum_name, .. } => out.push(enum_name.clone()),
        Atom::Intersection(parts) => {
            for part in parts {
                atom_classes(part, out);
            }
        }
        _ => {}
    }
}

/// The constant a literal denotes.
pub(super) fn constant(literal: &Literal) -> ConstValue {
    match literal {
        Literal::Int { raw } => ConstValue::Int(raw.clone()),
        Literal::Float { raw } => ConstValue::Float(raw.clone()),
        Literal::String { raw, quote } => ConstValue::Str(decode(raw, *quote).map(Into::into)),
        Literal::Bool(b) => ConstValue::Bool(*b),
        _ => ConstValue::Null,
    }
}

/// The value of a quoted string literal when its escapes make it certain.
fn decode(raw: &str, quote: QuoteKind) -> Option<String> {
    let inner = raw
        .strip_prefix(['\'', '"'])
        .and_then(|r| r.strip_suffix(['\'', '"']))
        .unwrap_or(raw);
    match quote {
        QuoteKind::Single => Some(inner.replace("\\\\", "\\").replace("\\'", "'")),
        _ => decode_double(inner),
    }
}

fn decode_double(inner: &str) -> Option<String> {
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next()? {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            '\\' => out.push('\\'),
            '"' => out.push('"'),
            '$' => out.push('$'),
            _ => return None,
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_quoted_strings_decode_two_escapes() {
        assert_eq!(
            decode("'a\\'b\\\\c\\n'", QuoteKind::Single).as_deref(),
            Some("a'b\\c\\n")
        );
    }

    #[test]
    fn double_quoted_strings_decode_known_escapes_only() {
        assert_eq!(
            decode("\"a\\tb\"", QuoteKind::Double).as_deref(),
            Some("a\tb")
        );
        assert_eq!(decode("\"\\x41\"", QuoteKind::Double), None);
        assert_eq!(
            decode("\"plain\"", QuoteKind::Double).as_deref(),
            Some("plain")
        );
    }

    #[test]
    fn type_classes() {
        assert_eq!(classify(&Type::int()), TypeClass::Inert);
        assert_eq!(
            classify(&Type::bool().union(&Type::null())),
            TypeClass::Inert
        );
        assert_eq!(classify(&Type::string()), TypeClass::Data);
        assert_eq!(
            classify(&Type::int().union(&Type::string())),
            TypeClass::Data
        );
        assert_eq!(classify(&Type::unknown()), TypeClass::Unknown);
        assert_eq!(classify(&Type::mixed()), TypeClass::Unknown);
        assert_eq!(classify(&Type::never()), TypeClass::Inert);
    }
}
