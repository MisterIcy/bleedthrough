//! Constant folding over IR values: just enough to evaluate include paths and pattern conditions.
//! Literals, concatenation, `__DIR__`, `__FILE__`, `dirname(...)`, and global or class constants
//! whose declared value is a literal. Nothing else is evaluated; a value read from a variable is
//! never folded.

use bleedthrough_ir::{ConstValue, Graph, Instr, InstrKind, InterpPart, ValueId};
use bleedthrough_symbols::{Atom, Fqn, SymbolTable, Type};
use bleedthrough_syntax::{MagicConstantKind, StringKind};

use crate::include::normalise_path;

/// Deepest chain of operands the folder follows.
const MAX_DEPTH: usize = 64;

/// Term: Call graph
///
/// The result of folding a value.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Folded {
    /// The value is always this string.
    Value(Box<str>),
    /// The value is not known statically.
    NotFolded,
}

impl Folded {
    /// The folded string, if any.
    pub fn value(&self) -> Option<&str> {
        match self {
            Self::Value(text) => Some(text),
            Self::NotFolded => None,
        }
    }
}

/// Term: Call graph
///
/// Folds the values of one graph, knowing the file it came from and the symbol table.
#[derive(Debug)]
pub struct Folder<'a> {
    defs: Vec<Option<&'a Instr>>,
    table: &'a SymbolTable,
    file: &'a str,
}

impl<'a> Folder<'a> {
    /// A folder for the values of `graph`, which lives in `file`.
    pub fn new(graph: &'a Graph, table: &'a SymbolTable, file: &'a str) -> Self {
        let mut defs = vec![None; graph.value_count as usize];
        for block in &graph.blocks {
            for instr in &block.instrs {
                if let Some(dest) = instr.dest
                    && let Some(slot) = defs.get_mut(dest.index())
                {
                    *slot = Some(instr);
                }
            }
        }
        Self { defs, table, file }
    }

    /// Folds `value` to a string when it is a constant expression.
    pub fn fold(&self, value: ValueId) -> Folded {
        self.text(value, 0)
            .map_or(Folded::NotFolded, |text| Folded::Value(text.into()))
    }

    fn text(&self, value: ValueId, depth: usize) -> Option<String> {
        if depth > MAX_DEPTH {
            return None;
        }
        let instr = (*self.defs.get(value.index())?)?;
        match &instr.kind {
            InstrKind::Const(constant) => const_text(constant),
            InstrKind::Copy(inner) => self.text(*inner, depth + 1),
            InstrKind::Magic(MagicConstantKind::File) => Some(self.file.to_string()),
            InstrKind::Magic(MagicConstantKind::Dir) => Some(dirname(self.file, 1)),
            InstrKind::Concat(parts) => parts
                .iter()
                .map(|part| self.text(part.value, depth + 1))
                .collect(),
            InstrKind::Interp { kind, parts } => self.interpolated(*kind, parts, depth),
            InstrKind::Call {
                callee: bleedthrough_ir::Callee::Function(name),
                args,
                ..
            } if name.text.eq_ignore_ascii_case("dirname") => self.dirname_call(args, depth),
            InstrKind::ConstFetch(name) => self.global_constant(&name.text),
            InstrKind::ClassConst {
                class,
                name: bleedthrough_ir::MemberRef::Named(member),
            } => {
                let [only] = class.classes.as_slice() else {
                    return None;
                };
                let found = self.table.class_constant(only, member, None);
                let candidate = found.candidates.first()?;
                literal_text(&candidate.member.ty)
            }
            _ => None,
        }
    }

    fn interpolated(&self, kind: StringKind, parts: &[InterpPart], depth: usize) -> Option<String> {
        if !matches!(kind, StringKind::DoubleQuoted | StringKind::Heredoc) {
            return None;
        }
        let mut out = String::new();
        for part in parts {
            match part {
                InterpPart::Text { raw, .. } if !raw.contains('\\') => out.push_str(raw),
                InterpPart::Value(operand) => out.push_str(&self.text(operand.value, depth + 1)?),
                _ => return None,
            }
        }
        Some(out)
    }

    fn dirname_call(&self, args: &[bleedthrough_ir::CallArg], depth: usize) -> Option<String> {
        if args.iter().any(|a| a.spread || a.name.is_some()) {
            return None;
        }
        let levels = match args {
            [_] => 1,
            [_, levels] => self
                .text(levels.value, depth + 1)?
                .parse::<usize>()
                .ok()
                .filter(|n| (1..=64).contains(n))?,
            _ => return None,
        };
        let path = self.text(args.first()?.value, depth + 1)?;
        Some(dirname(&path, levels))
    }

    fn global_constant(&self, written: &str) -> Option<String> {
        if written == "DIRECTORY_SEPARATOR" {
            return Some("/".to_string());
        }
        let decl = self.table.constant(&Fqn::new(written))?;
        literal_text(&decl.ty)
    }
}

fn const_text(constant: &ConstValue) -> Option<String> {
    match constant {
        ConstValue::Str(text) => text.as_deref().map(str::to_string),
        ConstValue::Int(raw) => Some(raw.to_string()),
        ConstValue::Bool(true) => Some("1".to_string()),
        ConstValue::Bool(false) | ConstValue::Null => Some(String::new()),
        _ => None,
    }
}

fn literal_text(ty: &Type) -> Option<String> {
    match ty.atoms() {
        [Atom::StringLiteral(text)] => Some(text.to_string()),
        [Atom::IntLiteral(n)] => Some(n.to_string()),
        _ => None,
    }
}

/// PHP's `dirname`, applied `levels` times, on a normalised relative or absolute path.
fn dirname(path: &str, levels: usize) -> String {
    let mut current = path.to_string();
    for _ in 0..levels {
        current = match current.rfind('/') {
            None => ".".to_string(),
            Some(0) => "/".to_string(),
            Some(at) => current[..at].to_string(),
        };
    }
    if current.is_empty() {
        ".".to_string()
    } else {
        normalise_path(&current)
    }
}

#[cfg(test)]
mod tests {
    use super::dirname;

    #[test]
    fn dirname_follows_php() {
        assert_eq!(dirname("src/a.php", 1), "src");
        assert_eq!(dirname("a.php", 1), ".");
        assert_eq!(dirname("/a.php", 1), "/");
        assert_eq!(dirname("/x/y/z.php", 2), "/x");
        assert_eq!(dirname("a/b/c.php", 5), ".");
    }
}
