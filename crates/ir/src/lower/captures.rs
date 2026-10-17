//! Iterative walks over an expression that must not recurse: the variables it mentions.

use std::collections::BTreeSet;

use bleedthrough_syntax::{Expr, ExprKind, NodeRef, Param, Span};

const SPECIAL: [&str; 9] = [
    "this", "_GET", "_POST", "_COOKIE", "_FILES", "_REQUEST", "_SERVER", "_ENV", "_SESSION",
];

fn plain(name: &str) -> &str {
    name.strip_prefix('$').unwrap_or(name)
}

/// Every variable name read inside `root`, in source order with the span of its first mention.
/// Uses an explicit stack, so the nesting depth of `root` does not matter.
fn variables(root: NodeRef<'_>) -> Vec<(String, Span)> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        match node {
            NodeRef::Expr(e) => {
                if let ExprKind::Variable(name) = &e.kind
                    && seen.insert(plain(name).to_string())
                {
                    out.push((plain(name).to_string(), e.span));
                }
            }
            NodeRef::ClosureUse(u) if seen.insert(plain(&u.name).to_string()) => {
                out.push((plain(&u.name).to_string(), u.span));
            }
            _ => {}
        }
        let mut children = node.children();
        children.reverse();
        stack.extend(children);
    }
    out
}

/// The variables mentioned inside an over-deep node, capped to keep the stand-in small.
pub(super) fn flatten_variables(node: NodeRef<'_>) -> Vec<(String, Span)> {
    let mut vars = variables(node);
    vars.truncate(64);
    vars
}

/// The variables an arrow function takes by value: everything its body mentions except its
/// parameters, `$this` and superglobals, in name order.
pub(super) fn arrow_captures(params: &[Param], body: &Expr) -> Vec<(Box<str>, bool)> {
    let own: BTreeSet<&str> = params.iter().map(|p| plain(&p.name)).collect();
    let names: BTreeSet<String> = variables(NodeRef::Expr(body))
        .into_iter()
        .map(|(name, _)| name)
        .filter(|n| !own.contains(n.as_str()) && !SPECIAL.contains(&n.as_str()))
        .collect();
    names
        .into_iter()
        .map(|n| (n.into_boxed_str(), false))
        .collect()
}
