//! Linking class-likes: parents, interfaces and traits as edges, with cycle cutting and the
//! transitive "has an unresolved ancestor" flag.

use std::collections::{BTreeMap, BTreeSet};

use super::{DeclLocation, SymbolTable, TableDiagnostic, TableDiagnosticCode};
use crate::name::Fqn;
use crate::summary::{ClassDecl, ClassKind};

/// A node being explored, its edge targets and the next edge to follow.
type Frame = (Box<str>, Vec<Box<str>>, usize);

/// The direct inheritance edges of one class-like after cycles were cut.
#[derive(Debug, Clone, PartialEq, Default)]
pub(super) struct Links {
    pub parent: Option<Fqn>,
    pub interfaces: Vec<Fqn>,
    pub traits: Vec<Fqn>,
    /// Direct ancestors named but absent from the table.
    pub missing: Vec<Fqn>,
    /// Whether an edge of this class-like was cut to break a cycle.
    pub cyclic: bool,
    /// Whether a direct or transitive ancestor is absent.
    pub incomplete: bool,
}

impl Links {
    /// Every direct edge target in lookup order: parent, interfaces, traits.
    pub fn targets(&self) -> impl Iterator<Item = &Fqn> {
        self.parent
            .iter()
            .chain(self.interfaces.iter())
            .chain(self.traits.iter())
    }
}

/// Edges a declaration states, in a stable order.
fn declared_edges(class: &ClassDecl) -> Links {
    let mut traits: Vec<Fqn> = Vec::new();
    for usage in &class.trait_uses {
        for t in &usage.traits {
            if !traits.contains(t) {
                traits.push(t.clone());
            }
        }
    }
    let mut interfaces: Vec<Fqn> = Vec::new();
    for i in &class.interfaces {
        if !interfaces.contains(i) {
            interfaces.push(i.clone());
        }
    }
    Links {
        parent: class.parent.clone(),
        interfaces,
        traits,
        ..Links::default()
    }
}

/// The interfaces every enum implements. They are linked when the table has them and ignored
/// otherwise, so a table without built-ins does not mark every enum incomplete.
fn implicit_enum_interfaces(class: &ClassDecl, table: &SymbolTable) -> Vec<Fqn> {
    if class.kind != ClassKind::Enum {
        return Vec::new();
    }
    let mut names = vec![Fqn::new("UnitEnum")];
    if class.backing_type.is_some() {
        names.push(Fqn::new("BackedEnum"));
    }
    names
        .into_iter()
        .filter(|n| table.classes.contains_key(n.key()))
        .collect()
}

pub(super) fn link(table: &mut SymbolTable) {
    let mut links: BTreeMap<Box<str>, Links> = BTreeMap::new();
    for (key, slot) in &table.classes {
        let class = table.class_at(*slot);
        let mut edges = declared_edges(class);
        for implicit in implicit_enum_interfaces(class, table) {
            if !edges.interfaces.contains(&implicit) {
                edges.interfaces.push(implicit);
            }
        }
        links.insert(key.clone(), edges);
    }
    let cycles = cut_cycles(&mut links);
    for cycle in cycles {
        let locations: Vec<DeclLocation> = cycle
            .iter()
            .filter_map(|k| table.classes.get(k.as_ref()))
            .map(|s| super::location(&table.files[s.file], table.class_at(*s).span))
            .collect();
        let names: Vec<String> = cycle
            .iter()
            .filter_map(|k| table.classes.get(k.as_ref()))
            .map(|s| table.class_at(*s).name.to_string())
            .collect();
        table.diagnostics.push(TableDiagnostic {
            code: TableDiagnosticCode::InheritanceCycle,
            message: format!("inheritance cycle: {}", names.join(" -> ")),
            locations,
        });
    }
    mark_incomplete(&mut links, table);
    let mut children: BTreeMap<Box<str>, BTreeSet<Box<str>>> = BTreeMap::new();
    for (key, l) in &links {
        for target in l.targets() {
            children
                .entry(target.key().into())
                .or_default()
                .insert(key.clone());
        }
    }
    table.children = children
        .into_iter()
        .map(|(k, v)| (k, v.into_iter().collect()))
        .collect();
    table.links = links;
}

/// Depth-first search without recursion. An edge into a node still on the stack closes a cycle;
/// it is removed and the cycle (as the keys on the stack from that node) is reported.
fn cut_cycles(links: &mut BTreeMap<Box<str>, Links>) -> Vec<Vec<Box<str>>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        OnStack,
        Done,
    }
    let mut marks: BTreeMap<Box<str>, Mark> = BTreeMap::new();
    let mut cycles = Vec::new();
    let roots: Vec<Box<str>> = links.keys().cloned().collect();
    for root in roots {
        if marks.contains_key(&root) {
            continue;
        }
        let mut stack: Vec<Frame> = Vec::new();
        marks.insert(root.clone(), Mark::OnStack);
        let targets = target_keys(links, &root);
        stack.push((root, targets, 0));
        while let Some((node, targets, next)) = stack.last_mut() {
            if *next >= targets.len() {
                marks.insert(node.clone(), Mark::Done);
                stack.pop();
                continue;
            }
            let target = targets[*next].clone();
            *next += 1;
            let node = node.clone();
            if !links.contains_key(&target) {
                continue;
            }
            match marks.get(&target) {
                Some(Mark::Done) => {}
                Some(Mark::OnStack) => {
                    let from = stack.iter().position(|(k, _, _)| *k == target).unwrap_or(0);
                    cycles.push(stack[from..].iter().map(|(k, _, _)| k.clone()).collect());
                    remove_edge(links, &node, &target);
                }
                None => {
                    marks.insert(target.clone(), Mark::OnStack);
                    let next_targets = target_keys(links, &target);
                    stack.push((target, next_targets, 0));
                }
            }
        }
    }
    cycles
}

fn target_keys(links: &BTreeMap<Box<str>, Links>, node: &str) -> Vec<Box<str>> {
    links
        .get(node)
        .map(|l| l.targets().map(|t| Box::<str>::from(t.key())).collect())
        .unwrap_or_default()
}

fn remove_edge(links: &mut BTreeMap<Box<str>, Links>, from: &str, to: &str) {
    if let Some(l) = links.get_mut(from) {
        if l.parent.as_ref().is_some_and(|p| p.key() == to) {
            l.parent = None;
        }
        l.interfaces.retain(|i| i.key() != to);
        l.traits.retain(|t| t.key() != to);
        l.cyclic = true;
    }
}

/// Sets `missing` and `incomplete`. With cycles cut the graph is acyclic, so a post-order pass
/// (again without recursion) settles every node once.
fn mark_incomplete(links: &mut BTreeMap<Box<str>, Links>, table: &SymbolTable) {
    for l in links.values_mut() {
        let missing: Vec<Fqn> = l
            .targets()
            .filter(|t| !table.classes.contains_key(t.key()))
            .cloned()
            .collect();
        l.incomplete = !missing.is_empty();
        l.missing = missing;
    }
    let order: Vec<Box<str>> = post_order(links);
    for key in order {
        let inherited = links.get(&key).is_some_and(|l| {
            l.targets()
                .any(|t| links.get(t.key()).is_some_and(|tl| tl.incomplete))
        });
        if inherited && let Some(l) = links.get_mut(&key) {
            l.incomplete = true;
        }
    }
}

fn post_order(links: &BTreeMap<Box<str>, Links>) -> Vec<Box<str>> {
    let mut seen: BTreeSet<Box<str>> = BTreeSet::new();
    let mut order = Vec::new();
    for root in links.keys() {
        if seen.contains(root) {
            continue;
        }
        seen.insert(root.clone());
        let mut stack: Vec<Frame> = vec![(root.clone(), target_keys(links, root), 0)];
        while let Some((node, targets, next)) = stack.last_mut() {
            if *next >= targets.len() {
                order.push(node.clone());
                stack.pop();
                continue;
            }
            let target = targets[*next].clone();
            *next += 1;
            if links.contains_key(&target) && seen.insert(target.clone()) {
                let next_targets = target_keys(links, &target);
                stack.push((target, next_targets, 0));
            }
        }
    }
    order
}
