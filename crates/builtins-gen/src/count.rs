//! The declaration counter and the coverage comparison.
//!
//! The counter walks the syntax tree with the generic visitor and shares no code with the
//! extractor, so a declaration the extractor skips is noticed: what the parser found must equal
//! what was produced, per file.

use std::collections::BTreeMap;

use bleedthrough_builtins::Kind;
use bleedthrough_syntax::{ClassLikeKind, MemberKind, NodeRef, StmtKind, SyntaxTree, visit_tree};

use crate::extract::Extracted;

/// A declaration by kind and unqualified name (the last segment of its name).
pub type Found = (Kind, String);

/// Every declaration in the tree: functions, class-likes, constants, properties, methods, cases.
pub fn declarations(tree: &SyntaxTree) -> Vec<Found> {
    let mut found = Vec::new();
    visit_tree(tree, &mut |node| match node {
        NodeRef::Stmt(stmt) => match &stmt.kind {
            StmtKind::Function(function) => found.push((Kind::Function, function.name.to_string())),
            StmtKind::ClassLike(class) => {
                let kind = match class.kind {
                    ClassLikeKind::Class => Kind::Class,
                    ClassLikeKind::Interface => Kind::Interface,
                    ClassLikeKind::Trait => Kind::Trait,
                    ClassLikeKind::Enum => Kind::Enum,
                    _ => return,
                };
                found.push((kind, class.name.as_deref().unwrap_or_default().to_string()));
            }
            StmtKind::Const { items, .. } => {
                found.extend(items.iter().map(|i| (Kind::Constant, i.name.to_string())));
            }
            _ => {}
        },
        NodeRef::Member(member) => match &member.kind {
            MemberKind::Const { items, .. } => {
                found.extend(
                    items
                        .iter()
                        .map(|i| (Kind::ClassConstant, i.name.to_string())),
                );
            }
            MemberKind::Property { items, .. } => {
                found.extend(items.iter().map(|i| (Kind::Property, i.name.to_string())));
            }
            MemberKind::Method(function) => found.push((Kind::Method, function.name.to_string())),
            MemberKind::EnumCase { name, .. } => found.push((Kind::EnumCase, name.to_string())),
            _ => {}
        },
        _ => {}
    });
    found
}

/// The unqualified name of a produced record.
fn short_name(item: &Extracted) -> String {
    let name = &item.record.name;
    let name = name.rsplit("::").next().unwrap_or(name);
    name.rsplit('\\').next().unwrap_or(name).to_string()
}

/// How many declarations of each kind were found and produced in one file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Coverage {
    pub found: BTreeMap<Kind, usize>,
    pub produced: BTreeMap<Kind, usize>,
    /// Found but not produced, by kind and name.
    pub missing: Vec<Found>,
    /// Produced although nothing of that kind and name was found.
    pub unexpected: Vec<Found>,
}

impl Coverage {
    pub fn is_complete(&self) -> bool {
        self.missing.is_empty() && self.unexpected.is_empty()
    }
}

/// Compares what the parser found in `tree` with the records produced from the file.
pub fn compare(tree: &SyntaxTree, produced: &[Extracted]) -> Coverage {
    let found = declarations(tree);
    let made: Vec<Found> = produced
        .iter()
        .map(|i| (i.record.kind(), short_name(i)))
        .collect();
    let mut coverage = Coverage::default();
    for (kind, _) in &found {
        *coverage.found.entry(*kind).or_default() += 1;
    }
    for (kind, _) in &made {
        *coverage.produced.entry(*kind).or_default() += 1;
    }
    let mut remaining = made;
    for item in found {
        match remaining.iter().position(|m| *m == item) {
            Some(index) => {
                remaining.swap_remove(index);
            }
            None => coverage.missing.push(item),
        }
    }
    coverage.unexpected = remaining;
    coverage
}

#[cfg(test)]
mod tests {
    use super::*;
    use bleedthrough_builtins::{Detail, FunctionDetail, PhpVersion, Record, VersionMask};
    use bleedthrough_syntax::{FunctionLike, Span, Stmt, StmtKind as K};

    fn function(name: &str) -> Stmt {
        Stmt::new(
            Span::new(0, 1),
            K::Function(Box::new(FunctionLike {
                name: name.into(),
                attributes: Vec::new(),
                modifiers: Vec::new(),
                by_ref: false,
                params: Vec::new(),
                return_type: None,
                body: None,
            })),
        )
    }

    fn produced(name: &str) -> Extracted {
        let mask = VersionMask::of(PhpVersion::parse("8.5").unwrap());
        Extracted {
            record: Record::new(
                name,
                mask,
                "core",
                Detail::Function(FunctionDetail::default()),
            ),
            file: "f".into(),
            guard: None,
        }
    }

    #[test]
    fn a_declaration_that_yields_no_record_is_named() {
        let tree = SyntaxTree::new(vec![function("a"), function("b")], Vec::new(), 2);
        let complete = compare(&tree, &[produced("a"), produced("B\\b")]);
        assert!(complete.is_complete(), "{complete:?}");
        let missing = compare(&tree, &[produced("a")]);
        assert_eq!(missing.missing, vec![(Kind::Function, "b".to_string())]);
        assert!(!missing.is_complete());
        let extra = compare(&tree, &[produced("a"), produced("b"), produced("c")]);
        assert_eq!(extra.unexpected, vec![(Kind::Function, "c".to_string())]);
    }
}
