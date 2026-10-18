//! Pattern rules: the closed set of non-flow shapes, matched against the instructions of a unit.

use bleedthrough_ir::{
    CallArg, Callee, ClassForm, ClassOperand, ConstValue, Graph, Instr, InstrKind, ValueId,
};
use bleedthrough_rules::{ArgCondition, ArgTest, regex_matches};
use bleedthrough_symbols::{Fqn, SymbolTable};
use bleedthrough_syntax::Span;

use crate::callgraph::UnitId;
use crate::calls::{CallInfo, merge_matches, param_decls, selected_args, subjects};
use crate::fold::Folder;
use crate::rule_index::{Matches, PatternKind, RuleIndex, Subject};

/// Term: Pattern match
///
/// One place a pattern rule matched.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct PatternHit {
    /// The pattern entry in the rule index.
    pub pattern: u32,
    /// The unit that contains the match.
    pub unit: UnitId,
    /// The matched construct.
    pub span: Span,
    /// The call was matched by its written name alone: no target was resolved.
    pub by_name: bool,
    /// The file was parsed with recovery, so the construct may be misread.
    pub recovered: bool,
}

/// Matches every pattern rule against the instructions of `graph`, in instruction order.
pub fn scan_patterns(
    index: &RuleIndex,
    table: &SymbolTable,
    unit: UnitId,
    file: &str,
    graph: &Graph,
) -> Vec<PatternHit> {
    if index.patterns().is_empty() {
        return Vec::new();
    }
    let scan = Scan {
        index,
        table,
        unit,
        graph,
        folder: Folder::new(graph, table, file),
        defs: definitions(graph),
    };
    let mut hits: Vec<PatternHit> = Vec::new();
    for block in &graph.blocks {
        for ins in &block.instrs {
            scan.instr(ins, &mut hits);
        }
    }
    hits.sort();
    hits.dedup();
    hits
}

fn definitions(graph: &Graph) -> Vec<Option<&Instr>> {
    let mut defs = vec![None; graph.value_count as usize];
    for block in &graph.blocks {
        for ins in &block.instrs {
            if let Some(dest) = ins.dest
                && let Some(slot) = defs.get_mut(dest.index())
            {
                *slot = Some(ins);
            }
        }
    }
    defs
}

struct Scan<'a> {
    index: &'a RuleIndex,
    table: &'a SymbolTable,
    unit: UnitId,
    graph: &'a Graph,
    folder: Folder<'a>,
    defs: Vec<Option<&'a Instr>>,
}

impl Scan<'_> {
    fn hit(&self, pattern: u32, span: Span, by_name: bool, hits: &mut Vec<PatternHit>) {
        hits.push(PatternHit {
            pattern,
            unit: self.unit,
            span,
            by_name,
            recovered: self.graph.recovered,
        });
    }

    fn def(&self, value: ValueId) -> Option<&Instr> {
        self.defs.get(value.index()).copied().flatten()
    }

    fn instr(&self, ins: &Instr, hits: &mut Vec<PatternHit>) {
        match &ins.kind {
            InstrKind::Call {
                callee,
                args,
                resolution,
            } => {
                self.call(ins.span, Some(callee), None, args, resolution, hits);
                self.callable_arguments(args, hits);
            }
            InstrKind::New {
                class,
                args,
                resolution,
            } => {
                self.call(ins.span, None, Some(class), args, resolution, hits);
                self.callable_arguments(args, hits);
            }
            InstrKind::CallableRef { callee, resolution } => {
                self.call(ins.span, Some(callee), None, &[], resolution, hits);
            }
            InstrKind::ClassConst { class, .. } => self.class_use(class, ins.span, hits),
            InstrKind::Include { path, .. } if self.folder.fold(*path).value().is_none() => {
                for at in self.index.include_patterns() {
                    self.hit(*at, ins.span, false, hits);
                }
            }
            _ => {}
        }
    }

    fn class_use(&self, class: &ClassOperand, span: Span, hits: &mut Vec<PatternHit>) {
        if !matches!(class.form, ClassForm::Named(_)) {
            return;
        }
        for name in &class.classes {
            for at in self.index.lookup(self.table, Subject::Class(name)).uses {
                self.hit(at, span, false, hits);
            }
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "the call's pieces, read together"
    )]
    fn call(
        &self,
        span: Span,
        callee: Option<&Callee>,
        new_class: Option<&ClassOperand>,
        args: &[CallArg],
        resolution: &bleedthrough_ir::Resolution,
        hits: &mut Vec<PatternHit>,
    ) {
        let info = CallInfo::of(resolution);
        let by_name = !matches!(info, CallInfo::Resolved { .. });
        let mut matches = Matches::default();
        for subject in subjects(callee, new_class, &info) {
            merge_matches(
                &mut matches,
                self.index.lookup(self.table, subject.as_subject()),
            );
        }
        let params = param_decls(self.table, &info);
        for at in &matches.calls {
            let PatternKind::Call { conditions, .. } = &self.index.patterns()[*at as usize].kind
            else {
                continue;
            };
            if conditions
                .iter()
                .all(|c| self.condition_holds(c, args, params))
            {
                self.hit(*at, span, by_name, hits);
            }
        }
        for at in &matches.uses {
            self.hit(*at, span, by_name, hits);
        }
        if let Some(Callee::Static { class, .. }) = callee {
            self.class_use(class, span, hits);
        }
        if let Some(class) = new_class {
            self.class_use(class, span, hits);
        }
    }

    fn condition_holds(
        &self,
        condition: &ArgCondition,
        args: &[CallArg],
        params: Option<&[bleedthrough_symbols::ParamDecl]>,
    ) -> bool {
        let selected = selected_args(condition.arg(), args, params);
        let Some(first) = selected.first() else {
            return matches!(condition.test(), ArgTest::Absent);
        };
        let arg = &args[*first];
        let folded = if arg.spread {
            None
        } else {
            self.folder.fold(arg.value).value().map(str::to_string)
        };
        match condition.test() {
            ArgTest::Absent => false,
            ArgTest::Literal => !arg.spread && self.is_literal(arg.value, folded.is_some()),
            ArgTest::NonLiteral => arg.spread || !self.is_literal(arg.value, folded.is_some()),
            ArgTest::Equals(want) => folded.as_deref() == Some(want.as_str()),
            ArgTest::Matches(pattern) => folded
                .as_deref()
                .is_some_and(|text| regex_matches(pattern, text)),
            _ => false,
        }
    }

    fn is_literal(&self, value: ValueId, folded: bool) -> bool {
        folded || matches!(self.def(value).map(|d| &d.kind), Some(InstrKind::Const(_)))
    }

    /// Literal callable strings (`'system'`, `'A::run'`) and two-element arrays (`['A', 'run']`)
    /// passed as arguments name what they would call.
    fn callable_arguments(&self, args: &[CallArg], hits: &mut Vec<PatternHit>) {
        for arg in args {
            let Some(def) = self.def(arg.value) else {
                continue;
            };
            match &def.kind {
                InstrKind::Const(ConstValue::Str(Some(text))) => {
                    self.callable_text(text, def.span, hits);
                }
                InstrKind::Array(items) if items.len() == 2 => {
                    let texts: Vec<Option<&str>> = items
                        .iter()
                        .map(|item| match self.def(item.value).map(|d| &d.kind) {
                            Some(InstrKind::Const(ConstValue::Str(Some(t)))) => Some(&**t),
                            _ => None,
                        })
                        .collect();
                    if let [Some(class), Some(method)] = texts.as_slice() {
                        self.callable_pair(class, method, def.span, hits);
                    }
                }
                _ => {}
            }
        }
    }

    fn callable_text(&self, text: &str, span: Span, hits: &mut Vec<PatternHit>) {
        if let Some((class, method)) = text.split_once("::") {
            self.callable_pair(class, method, span, hits);
            return;
        }
        for at in self.index.lookup(self.table, Subject::Function(text)).uses {
            self.hit(at, span, false, hits);
        }
    }

    fn callable_pair(&self, class: &str, method: &str, span: Span, hits: &mut Vec<PatternHit>) {
        let class = Fqn::new(class);
        for subject in [
            Subject::Static {
                class: &class,
                method,
            },
            Subject::Instance {
                class: &class,
                method,
            },
        ] {
            for at in self.index.lookup(self.table, subject).uses {
                self.hit(at, span, false, hits);
            }
        }
    }
}
