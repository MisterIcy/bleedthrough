//! The canonical text form of a summary: golden dumps for tests and the input of the fingerprint.

use std::fmt::{self, Write as _};

use sha2::{Digest, Sha256};

use super::model::{
    Adaptation, ClassDecl, FileSummary, FunctionDecl, MethodDecl, ParamDecl, PropertyDecl,
};
use crate::native::EffectiveType;

/// Term: File summary
///
/// A content hash of a summary's declarations, independent of comments, whitespace and positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fingerprint([u8; 32]);

impl Fingerprint {
    /// The raw digest bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

impl FileSummary {
    /// The declaration fingerprint: it covers every declaration (names, kinds, modifiers, member
    /// signatures, effective types and where they came from, hierarchy) and nothing else: no
    /// spans, raw docblock text, diagnostics, path or body references.
    pub fn fingerprint(&self) -> Fingerprint {
        let digest = Sha256::digest(declarations(self).as_bytes());
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&digest);
        Fingerprint(bytes)
    }
}

/// The full golden text of a summary: declarations, then references and diagnostics.
pub fn dump_summary(summary: &FileSummary) -> String {
    let mut out = format!("file {}\n", summary.path);
    if summary.incomplete {
        out.push_str("incomplete\n");
    }
    out.push_str(&declarations(summary));
    if !summary.referenced_classes.is_empty() {
        let names: Vec<_> = summary
            .referenced_classes
            .iter()
            .map(|c| c.as_str())
            .collect();
        let _ = writeln!(out, "references classes: {}", names.join(", "));
    }
    if !summary.declaration_classes.is_empty() {
        let names: Vec<_> = summary
            .declaration_classes
            .iter()
            .map(|c| c.as_str())
            .collect();
        let _ = writeln!(out, "references in declarations: {}", names.join(", "));
    }
    if !summary.referenced_functions.is_empty() {
        let names: Vec<String> = summary
            .referenced_functions
            .iter()
            .map(|f| match &f.fallback {
                Some(g) => format!("{} | {}", f.primary, g),
                None => f.primary.to_string(),
            })
            .collect();
        let _ = writeln!(out, "references functions: {}", names.join(", "));
    }
    for d in &summary.diagnostics {
        let _ = writeln!(
            out,
            "diagnostic {} {}..{}: {}",
            d.code.as_str(),
            d.span.start,
            d.span.end,
            d.message
        );
    }
    out
}

fn declarations(summary: &FileSummary) -> String {
    let mut out = String::new();
    for class in &summary.classes {
        class_text(&mut out, class);
    }
    for function in &summary.functions {
        function_text(&mut out, function);
    }
    for constant in &summary.constants {
        let _ = writeln!(
            out,
            "constant {} [{}]: {}",
            constant.name,
            match constant.via {
                super::model::ConstantVia::Const => "const",
                _ => "define",
            },
            constant.ty
        );
    }
    out
}

fn flags(list: &[(bool, &str)]) -> String {
    let words: Vec<&str> = list.iter().filter(|(on, _)| *on).map(|(_, w)| *w).collect();
    words.join(" ")
}

fn names<T: fmt::Display>(items: &[T]) -> String {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn effective(ty: &EffectiveType) -> String {
    format!("{} ({})", ty.ty, ty.source.as_str())
}

fn class_text(out: &mut String, class: &ClassDecl) {
    let f = flags(&[
        (class.is_abstract, "abstract"),
        (class.is_final, "final"),
        (class.is_readonly, "readonly"),
    ]);
    let _ = writeln!(out, "{} {} [{f}]", class.kind.as_str(), class.name);
    if let Some(parent) = &class.parent {
        let _ = writeln!(out, "  extends {parent}");
    }
    if !class.interfaces.is_empty() {
        let _ = writeln!(out, "  implements {}", names(&class.interfaces));
    }
    if let Some(backing) = &class.backing_type {
        let _ = writeln!(out, "  backed by {backing}");
    }
    if !class.templates.is_empty() {
        let _ = writeln!(out, "  templates {}", names(&class.templates));
    }
    if !class.inherits.is_empty() {
        let _ = writeln!(out, "  inherits {}", names(&class.inherits));
    }
    if !class.attributes.is_empty() {
        let _ = writeln!(out, "  attributes {}", names(&class.attributes));
    }
    for usage in &class.trait_uses {
        let _ = writeln!(out, "  use {}", names(&usage.traits));
        for adaptation in &usage.adaptations {
            adaptation_text(out, adaptation);
        }
    }
    for c in &class.constants {
        let _ = writeln!(
            out,
            "  const {} [{}]: {}",
            c.name,
            flags(&[(true, c.visibility.as_str()), (c.is_final, "final")]),
            c.ty
        );
    }
    for case in &class.cases {
        match &case.value {
            Some(v) => {
                let _ = writeln!(out, "  case {} = {v}", case.name);
            }
            None => {
                let _ = writeln!(out, "  case {}", case.name);
            }
        }
    }
    for p in &class.properties {
        property_text(out, p);
    }
    for m in &class.methods {
        method_text(out, m);
    }
}

fn adaptation_text(out: &mut String, adaptation: &Adaptation) {
    match adaptation {
        Adaptation::InsteadOf {
            trait_name,
            method,
            excluded,
        } => {
            let _ = writeln!(
                out,
                "    {trait_name}::{method} insteadof {}",
                names(excluded)
            );
        }
        Adaptation::Alias {
            trait_name,
            method,
            visibility,
            alias,
        } => {
            let from = trait_name
                .as_ref()
                .map_or_else(|| method.to_string(), |t| format!("{t}::{method}"));
            let vis = visibility.map_or(String::new(), |v| format!(" {}", v.as_str()));
            let name = alias.as_deref().map_or(String::new(), |a| format!(" {a}"));
            let _ = writeln!(out, "    {from} as{vis}{name}");
        }
    }
}

fn property_text(out: &mut String, p: &PropertyDecl) {
    let set = p
        .set_visibility
        .map_or(String::new(), |v| format!("({}-set)", v.as_str()));
    let f = flags(&[
        (true, p.visibility.as_str()),
        (p.is_static, "static"),
        (p.is_readonly, "readonly"),
        (p.has_default, "default"),
    ]);
    let hooks = if p.hooks.is_empty() {
        String::new()
    } else {
        format!(" hooks {}", names(&p.hooks))
    };
    let _ = writeln!(
        out,
        "  property ${} [{f}{set}] {}: {}{hooks}",
        p.name,
        p.origin.as_str(),
        effective(&p.ty)
    );
}

fn param_text(p: &ParamDecl) -> String {
    format!(
        "{}{}${}: {}{}",
        if p.by_ref { "&" } else { "" },
        if p.variadic { "..." } else { "" },
        p.name,
        effective(&p.ty),
        if p.has_default { " =" } else { "" }
    )
}

fn method_text(out: &mut String, m: &MethodDecl) {
    let f = flags(&[
        (true, m.visibility.as_str()),
        (m.is_static, "static"),
        (m.is_final, "final"),
        (m.is_abstract, "abstract"),
        (m.by_ref, "by-ref"),
    ]);
    let params: Vec<String> = m.params.iter().map(param_text).collect();
    let _ = writeln!(
        out,
        "  method {}({}) -> {} [{f}] {}",
        m.name,
        params.join("; "),
        effective(&m.ret),
        m.origin.as_str()
    );
    if !m.templates.is_empty() {
        let _ = writeln!(out, "    templates {}", names(&m.templates));
    }
}

fn function_text(out: &mut String, function: &FunctionDecl) {
    let params: Vec<String> = function.params.iter().map(param_text).collect();
    let _ = writeln!(
        out,
        "function {}({}) -> {}{}",
        function.name,
        params.join("; "),
        effective(&function.ret),
        if function.by_ref { " [by-ref]" } else { "" }
    );
    if !function.templates.is_empty() {
        let _ = writeln!(out, "  templates {}", names(&function.templates));
    }
}
