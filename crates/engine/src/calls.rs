//! What a call instruction may reach: the symbols to look up in the rule index, the resolution as
//! the analyses need it, and which arguments a selector denotes.

use bleedthrough_ir::{CallArg, Callee, ClassForm, ClassOperand, Resolution};
use bleedthrough_rules::ArgSel;
use bleedthrough_symbols::{
    CallResolution, Fqn, ParamDecl, SymbolTable, Target, TargetKind, UnresolvedReason,
};

use crate::rule_index::{Matches, Subject};

pub(crate) enum OwnedSubject {
    Function(String),
    Static(Fqn, String),
    Instance(Fqn, String),
    Constructor(Fqn),
}

impl OwnedSubject {
    pub(crate) fn as_subject(&self) -> Subject<'_> {
        match self {
            Self::Function(n) => Subject::Function(n),
            Self::Static(c, m) => Subject::Static {
                class: c,
                method: m,
            },
            Self::Instance(c, m) => Subject::Instance {
                class: c,
                method: m,
            },
            Self::Constructor(c) => Subject::Constructor { class: c },
        }
    }
}

/// A call resolution seen as the dataflow needs it.
pub(crate) enum CallInfo<'r> {
    Resolved {
        targets: &'r [Target],
        /// Part of the receiver did not resolve.
        partial: bool,
    },
    UnresolvedFunctionNotFound,
    Unresolved,
    Missing,
}

impl<'r> CallInfo<'r> {
    pub(crate) fn of(resolution: &'r Resolution) -> Self {
        match resolution {
            Resolution::Missing => Self::Missing,
            Resolution::Facts(CallResolution::Resolved { targets, residual }) => Self::Resolved {
                targets,
                partial: !residual.is_empty(),
            },
            Resolution::Facts(CallResolution::Unresolved(UnresolvedReason::FunctionNotFound)) => {
                Self::UnresolvedFunctionNotFound
            }
            Resolution::Facts(_) => Self::Unresolved,
            _ => Self::Missing,
        }
    }
}

pub(crate) fn subjects(
    callee: Option<&Callee>,
    new_class: Option<&ClassOperand>,
    info: &CallInfo<'_>,
) -> Vec<OwnedSubject> {
    let mut out: Vec<OwnedSubject> = Vec::new();
    let targets: &[Target] = match info {
        CallInfo::Resolved { targets, .. } => targets,
        _ => &[],
    };
    match callee {
        Some(Callee::Function(name)) => {
            if targets.is_empty() {
                if matches!(
                    info,
                    CallInfo::Missing | CallInfo::UnresolvedFunctionNotFound
                ) {
                    out.push(OwnedSubject::Function(name.text.to_string()));
                }
            } else {
                out.extend(
                    targets
                        .iter()
                        .filter(|t| t.kind == TargetKind::Function)
                        .map(|t| OwnedSubject::Function(t.name.to_string())),
                );
            }
        }
        Some(Callee::Method { .. }) => {
            for t in targets.iter().filter(|t| t.kind == TargetKind::Method) {
                for class in t.owner.iter().chain(&t.implementors) {
                    out.push(OwnedSubject::Instance(class.clone(), t.name.to_string()));
                }
            }
        }
        Some(Callee::Static { .. }) => {
            for t in targets.iter().filter(|t| t.kind == TargetKind::Method) {
                for class in t.owner.iter().chain(&t.implementors) {
                    out.push(OwnedSubject::Static(class.clone(), t.name.to_string()));
                }
            }
        }
        Some(Callee::Value(_)) | None => {}
        Some(_) => {}
    }
    if let Some(class) = new_class {
        let owners: Vec<&Fqn> = targets
            .iter()
            .filter(|t| t.kind == TargetKind::Constructor)
            .filter_map(|t| t.owner.as_ref())
            .collect();
        if owners.is_empty() {
            if matches!(class.form, ClassForm::Named(_)) {
                out.extend(class.classes.iter().cloned().map(OwnedSubject::Constructor));
            }
        } else {
            out.extend(owners.into_iter().cloned().map(OwnedSubject::Constructor));
        }
    }
    out
}

pub(crate) fn merge_matches(into: &mut Matches, from: Matches) {
    for (list, more) in [
        (&mut into.sources, from.sources),
        (&mut into.sinks, from.sinks),
        (&mut into.sanitizers, from.sanitizers),
        (&mut into.propagators, from.propagators),
        (&mut into.calls, from.calls),
        (&mut into.uses, from.uses),
    ] {
        for at in more {
            if !list.contains(&at) {
                list.push(at);
            }
        }
    }
}

pub(crate) fn multi_target(targets: &[Target]) -> bool {
    targets.len() > 1 || targets.iter().any(|t| t.implementors.len() > 1)
}

pub(crate) fn candidate_count(targets: &[Target]) -> u16 {
    let n: usize = targets.iter().map(|t| 1 + t.implementors.len()).sum();
    u16::try_from(n).unwrap_or(u16::MAX)
}

/// The positions in `args` a selector denotes. A spread argument covers every position from its
/// own onwards.
pub(crate) fn selected_args(
    sel: &ArgSel,
    args: &[CallArg],
    params: Option<&[ParamDecl]>,
) -> Vec<usize> {
    let mut out = Vec::new();
    let mut positional = 0usize;
    for (at, arg) in args.iter().enumerate() {
        let is_named = arg.name.is_some();
        let position = positional;
        if !is_named {
            positional += 1;
        }
        let hit = match sel {
            ArgSel::Position(n) => {
                let wanted = (*n as usize).saturating_sub(1);
                let named_here = arg.name.as_deref().is_some_and(|name| {
                    params
                        .and_then(|p| p.iter().position(|d| *d.name == *name))
                        .is_some_and(|at| at == wanted)
                });
                named_here
                    || (!is_named && (position == wanted || (arg.spread && position <= wanted)))
            }
            ArgSel::Name(name) => {
                arg.name.as_deref() == Some(name.as_str())
                    || (!is_named
                        && !arg.spread
                        && params
                            .and_then(|p| p.get(position))
                            .is_some_and(|p| *p.name == **name))
            }
            _ => false,
        };
        if hit {
            out.push(at);
        }
    }
    out
}

/// The declared parameters of the first target of a resolved call.
pub(crate) fn param_decls<'t>(
    table: &'t SymbolTable,
    info: &CallInfo<'_>,
) -> Option<&'t [ParamDecl]> {
    let CallInfo::Resolved { targets, .. } = info else {
        return None;
    };
    let target = targets.first()?;
    match (target.kind, &target.owner) {
        (TargetKind::Function, _) => table
            .function(&Fqn::new(&target.name))
            .map(|f| f.params.as_slice()),
        (TargetKind::Method, Some(owner)) => table
            .method(owner, &target.name, None)
            .candidates
            .first()
            .map(|c| c.member.params.as_slice()),
        (TargetKind::Constructor, Some(owner)) => table
            .method(owner, "__construct", None)
            .candidates
            .first()
            .map(|c| c.member.params.as_slice()),
        _ => None,
    }
}
