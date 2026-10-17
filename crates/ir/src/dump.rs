//! The textual dump of a graph: stable, line-per-instruction, used by golden tests and diagnostics.
//!
//! ```text
//! unit App\Foo::bar kind=method span=10..120 owner=App\Foo static=false recovered=false
//! params: l0 $a, l1 &$b
//! b0 (entry):
//!   v0 = load local $a @12..14
//!   v1 = call fn strlen(v0) res=resolved [function strlen] @20..30
//!   term goto b1
//! ```

use std::fmt::Write as _;

use bleedthrough_syntax::Span;

use crate::graph::{Graph, Terminator};
use crate::ids::ValueId;
use crate::instr::{
    ArrayElem, CallArg, Callee, ClassForm, ClassOperand, ConstValue, Instr, InstrKind, InterpPart,
    MemberRef, Place, Resolution, TypeClass,
};

/// Renders a graph.
pub fn dump(graph: &Graph) -> String {
    render(graph, None)
}

/// Renders a graph with the source text of every instruction after it.
pub fn dump_with_source(graph: &Graph, source: &str) -> String {
    render(graph, Some(source))
}

fn render(graph: &Graph, source: Option<&str>) -> String {
    let mut out = String::new();
    header(graph, &mut out);
    for (index, block) in graph.blocks.iter().enumerate() {
        let id = crate::BlockId::new(u32::try_from(index).unwrap_or(u32::MAX));
        let role = if id == graph.entry {
            " (entry)"
        } else if id == graph.exit {
            " (exit)"
        } else {
            ""
        };
        let _ = writeln!(out, "{id}{role}:");
        for instr in &block.instrs {
            let _ = writeln!(
                out,
                "  {}{}",
                instr_text(graph, instr),
                tail(instr.span, source)
            );
        }
        if !block.handlers.is_empty() {
            let handlers: Vec<String> = block.handlers.iter().map(ToString::to_string).collect();
            let _ = writeln!(out, "  handlers {}", handlers.join(", "));
        }
        let _ = writeln!(out, "  term {}", terminator_text(&block.term));
    }
    if !graph.unknowns.is_empty() {
        let list: Vec<String> = graph
            .unknowns
            .iter()
            .map(|(k, n)| format!("{k}={n}"))
            .collect();
        let _ = writeln!(out, "unknowns: {}", list.join(", "));
    }
    out
}

fn header(graph: &Graph, out: &mut String) {
    let _ = writeln!(
        out,
        "unit {} kind={} span={}..{} owner={} static={} recovered={}",
        graph.key.name,
        graph.kind.as_str(),
        graph.key.span.start,
        graph.key.span.end,
        graph.owner.as_deref().unwrap_or("-"),
        graph.is_static,
        graph.recovered,
    );
    if !graph.params.is_empty() {
        let list: Vec<String> = graph
            .params
            .iter()
            .map(|p| {
                format!(
                    "{} {}{}${}",
                    p.local,
                    if p.by_ref { "&" } else { "" },
                    if p.variadic { "..." } else { "" },
                    p.name
                )
            })
            .collect();
        let _ = writeln!(out, "params: {}", list.join(", "));
    }
    if !graph.captures.is_empty() {
        let list: Vec<String> = graph
            .captures
            .iter()
            .map(|c| format!("{} {}${}", c.local, if c.by_ref { "&" } else { "" }, c.name))
            .collect();
        let _ = writeln!(out, "captures: {}", list.join(", "));
    }
    if graph.limits.any() {
        let mut hit = Vec::new();
        if graph.limits.depth {
            hit.push("depth");
        }
        if graph.limits.blocks {
            hit.push("blocks");
        }
        if graph.limits.instructions {
            hit.push("instructions");
        }
        let _ = writeln!(out, "limits: {}", hit.join(", "));
    }
}

fn tail(span: Span, source: Option<&str>) -> String {
    let mut text = format!(" @{}..{}", span.start, span.end);
    if let Some(src) = source {
        text.push_str("  ; ");
        text.push_str(&snippet(src, span));
    }
    text
}

fn snippet(source: &str, span: Span) -> String {
    let raw = source
        .get(span.start as usize..span.end as usize)
        .unwrap_or("?");
    let flat = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > 50 {
        let cut: String = flat.chars().take(47).collect();
        format!("{cut}...")
    } else {
        flat
    }
}

fn terminator_text(term: &Terminator) -> String {
    match term {
        Terminator::Goto(b) => format!("goto {b}"),
        Terminator::Branch {
            cond,
            then,
            otherwise,
        } => format!("branch {cond} ? {then} : {otherwise}"),
        Terminator::Choice(targets) => {
            let list: Vec<String> = targets.iter().map(ToString::to_string).collect();
            format!("choice {}", list.join(", "))
        }
        Terminator::Return(Some(v)) => format!("return {v}"),
        Terminator::Return(None) => "return".to_string(),
        Terminator::Throw(v) => format!("throw {v}"),
        Terminator::Exit => "exit".to_string(),
        Terminator::Unreachable => "unreachable".to_string(),
    }
}

fn values(list: &[ValueId]) -> String {
    let items: Vec<String> = list.iter().map(ToString::to_string).collect();
    items.join(", ")
}

fn member(m: &MemberRef) -> String {
    match m {
        MemberRef::Named(n) => n.to_string(),
        MemberRef::Dynamic(v) => format!("{{{v}}}"),
        MemberRef::Missing => "<missing>".to_string(),
    }
}

fn class(c: &ClassOperand) -> String {
    let form = match &c.form {
        ClassForm::Named(n) => n.text.to_string(),
        ClassForm::SelfRef => "self".to_string(),
        ClassForm::Parent => "parent".to_string(),
        ClassForm::Static => "static".to_string(),
        ClassForm::Value(v) => format!("{{{v}}}"),
        ClassForm::Anonymous => "<anonymous>".to_string(),
    };
    if c.classes.is_empty() {
        form
    } else {
        let names: Vec<&str> = c.classes.iter().map(|f| f.as_str()).collect();
        format!("{form}<{}>", names.join("|"))
    }
}

fn place(graph: &Graph, p: &Place) -> String {
    match p {
        Place::Local(l) => {
            let name = graph.local_name(*l);
            if name.starts_with('%') {
                format!("local {name}")
            } else {
                format!("local ${name}")
            }
        }
        Place::Prop {
            object,
            name,
            owners,
        } => {
            let owner = if owners.is_empty() {
                String::new()
            } else {
                let names: Vec<&str> = owners.iter().map(|f| f.as_str()).collect();
                format!("<{}>", names.join("|"))
            };
            format!("prop {object}{owner}->{}", member(name))
        }
        Place::StaticProp { class: c, name } => format!("static {}::${}", class(c), member(name)),
        Place::Global(n) => format!("global ${n}"),
    }
}

fn callee(c: &Callee) -> String {
    match c {
        Callee::Function(n) => format!("fn {}", n.text),
        Callee::Value(v) => format!("value {v}"),
        Callee::Method {
            receiver,
            name,
            nullsafe,
        } => format!(
            "method {receiver}{}{}",
            if *nullsafe { "?->" } else { "->" },
            member(name)
        ),
        Callee::Static { class: c, name } => format!("static {}::{}", class(c), member(name)),
    }
}

fn args(list: &[CallArg]) -> String {
    let items: Vec<String> = list
        .iter()
        .map(|a| {
            let name = a
                .name
                .as_deref()
                .map_or(String::new(), |n| format!("{n}: "));
            format!("{name}{}{}", if a.spread { "..." } else { "" }, a.value)
        })
        .collect();
    items.join(", ")
}

fn resolution(r: &Resolution) -> String {
    match r {
        Resolution::Facts(f) => f.to_string(),
        Resolution::Missing => "none".to_string(),
    }
}

fn constant(c: &ConstValue) -> String {
    match c {
        ConstValue::Int(raw) => format!("int {raw}"),
        ConstValue::Float(raw) => format!("float {raw}"),
        ConstValue::Str(Some(s)) => format!("str {s:?}"),
        ConstValue::Str(None) => "str ?".to_string(),
        ConstValue::Bool(b) => format!("bool {b}"),
        ConstValue::Null => "null".to_string(),
    }
}

fn array(items: &[ArrayElem]) -> String {
    let list: Vec<String> = items
        .iter()
        .map(|i| {
            let key = i.key.map_or(String::new(), |k| format!("{k} => "));
            format!("{key}{}{}", if i.spread { "..." } else { "" }, i.value)
        })
        .collect();
    list.join(", ")
}

fn interp_parts(parts: &[InterpPart]) -> String {
    let list: Vec<String> = parts
        .iter()
        .map(|p| match p {
            InterpPart::Text { raw, .. } => format!("{raw:?}"),
            InterpPart::Value(o) => format!("{}@{}..{}", o.value, o.span.start, o.span.end),
        })
        .collect();
    list.join(" ")
}

fn instr_text(graph: &Graph, instr: &Instr) -> String {
    let body = body_text(graph, &instr.kind);
    match instr.dest {
        Some(dest) => {
            let ty = if instr.ty == TypeClass::Unknown {
                String::new()
            } else {
                format!(" :{}", instr.ty.as_str())
            };
            format!("{dest} = {body}{ty}")
        }
        None => body,
    }
}

#[allow(clippy::too_many_lines, reason = "one arm per instruction kind")]
fn body_text(graph: &Graph, kind: &InstrKind) -> String {
    match kind {
        InstrKind::Const(c) => format!("const {}", constant(c)),
        InstrKind::Magic(m) => format!("magic {m:?}"),
        InstrKind::ConstFetch(n) => format!("const-fetch {}", n.text),
        InstrKind::ClassConst { class: c, name } => {
            format!("class-const {}::{}", class(c), member(name))
        }
        InstrKind::Superglobal(n) => format!("superglobal {n}"),
        InstrKind::Load(p) => format!("load {}", place(graph, p)),
        InstrKind::LoadElem { base, index } => match index {
            Some(i) => format!("load-elem {base}[{i}]"),
            None => format!("load-elem {base}[]"),
        },
        InstrKind::Store {
            place: p,
            value,
            weak,
        } => format!(
            "store{} {} = {value}",
            if *weak { ".weak" } else { "" },
            place(graph, p)
        ),
        InstrKind::Concat(parts) => {
            let list: Vec<String> = parts
                .iter()
                .map(|o| format!("{}@{}..{}", o.value, o.span.start, o.span.end))
                .collect();
            format!("concat {}", list.join(", "))
        }
        InstrKind::Interp { kind, parts } => format!("interp {kind:?} [{}]", interp_parts(parts)),
        InstrKind::Binary { op, lhs, rhs } => format!("binary {op:?} {lhs}, {rhs}"),
        InstrKind::Unary { op, operand } => format!("unary {op:?} {operand}"),
        InstrKind::Cast { kind, operand } => format!("cast {kind:?} {operand}"),
        InstrKind::Copy(v) => format!("copy {v}"),
        InstrKind::Call {
            callee: c,
            args: a,
            resolution: r,
        } => format!("call {}({}) res={}", callee(c), args(a), resolution(r)),
        InstrKind::New {
            class: c,
            args: a,
            resolution: r,
        } => format!("new {}({}) res={}", class(c), args(a), resolution(r)),
        InstrKind::CallableRef {
            callee: c,
            resolution: r,
        } => format!("callable-ref {} res={}", callee(c), resolution(r)),
        InstrKind::Include { kind, path } => format!("include {kind:?} {path}"),
        InstrKind::Eval(v) => format!("eval {v}"),
        InstrKind::Echo(o) => format!("echo {}@{}..{}", o.value, o.span.start, o.span.end),
        InstrKind::Print(v) => format!("print {v}"),
        InstrKind::Exit(Some(v)) => format!("exit {v}"),
        InstrKind::Exit(None) => "exit".to_string(),
        InstrKind::Array(items) => format!("array [{}]", array(items)),
        InstrKind::Closure { unit, captures } => {
            let list: Vec<String> = captures
                .iter()
                .map(|c| {
                    format!(
                        "{}{}->{}",
                        if c.by_ref { "&" } else { "" },
                        c.outer,
                        c.inner
                    )
                })
                .collect();
            format!(
                "closure {} @{}..{} captures [{}]",
                unit.name,
                unit.span.start,
                unit.span.end,
                list.join(", ")
            )
        }
        InstrKind::Yield { key, value } => {
            let k = key.map_or("-".to_string(), |v| v.to_string());
            let v = value.map_or("-".to_string(), |v| v.to_string());
            format!("yield {k} => {v}")
        }
        InstrKind::YieldFrom(v) => format!("yield-from {v}"),
        InstrKind::Test { kind, operands } => {
            format!("test {} [{}]", kind.as_str(), values(operands))
        }
        InstrKind::Iter { subject, part } => format!("iter {subject} {part:?}"),
        InstrKind::Unknown { kind, operands } => {
            format!("unknown {kind} [{}]", values(operands))
        }
    }
}
