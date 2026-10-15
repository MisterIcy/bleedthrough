use crate::draft::{
    ArgConditionDraft, BodyDraft, PatternDraft, PropagatorDraft, SanitizerDraft, SinkDraft,
    TaintDraft,
};
use crate::error::{RuleError, prefixed};
use crate::regex::validate_regex;
use crate::symbol::{ArgSel, SinkClass, SymbolKind, SymbolRef};

/// Term: Sink
///
/// A sink of a taint rule: the symbol, which arguments are vulnerable and its class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sink {
    symbol: SymbolRef,
    vulnerable_args: Vec<ArgSel>,
    class: SinkClass,
}

impl Sink {
    /// The sink symbol.
    pub fn symbol(&self) -> &SymbolRef {
        &self.symbol
    }

    /// The vulnerable arguments; taint reaching any other argument is not considered.
    pub fn vulnerable_args(&self) -> &[ArgSel] {
        &self.vulnerable_args
    }

    /// The sink class.
    pub fn class(&self) -> SinkClass {
        self.class
    }
}

/// Term: Sanitizer
///
/// A sanitizer of a taint rule and the sink classes it neutralises taint for. It clears no
/// other class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sanitizer {
    symbol: SymbolRef,
    clears: Vec<SinkClass>,
}

impl Sanitizer {
    /// The sanitizer symbol.
    pub fn symbol(&self) -> &SymbolRef {
        &self.symbol
    }

    /// The classes cleared, sorted.
    pub fn clears(&self) -> &[SinkClass] {
        &self.clears
    }

    /// Whether taint for `class` is neutralised by this sanitizer.
    pub fn clears_class(&self, class: SinkClass) -> bool {
        self.clears.contains(&class)
    }
}

/// Term: Propagator
///
/// A propagator of a taint rule: which inputs flow to its result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Propagator {
    symbol: SymbolRef,
    from: Vec<ArgSel>,
}

impl Propagator {
    /// The propagator symbol.
    pub fn symbol(&self) -> &SymbolRef {
        &self.symbol
    }

    /// The inputs that flow to the result.
    pub fn from(&self) -> &[ArgSel] {
        &self.from
    }
}

/// Term: Rule
///
/// The body of a taint rule: flows described as data, without engine logic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaintBody {
    sources: Vec<SymbolRef>,
    sinks: Vec<Sink>,
    sanitizers: Vec<Sanitizer>,
    propagators: Vec<Propagator>,
}

impl TaintBody {
    /// Where taint enters.
    pub fn sources(&self) -> &[SymbolRef] {
        &self.sources
    }

    /// Where tainted data is dangerous.
    pub fn sinks(&self) -> &[Sink] {
        &self.sinks
    }

    /// What neutralises taint, per class.
    pub fn sanitizers(&self) -> &[Sanitizer] {
        &self.sanitizers
    }

    /// What carries taint to a result.
    pub fn propagators(&self) -> &[Propagator] {
        &self.propagators
    }
}

/// Term: Rule
///
/// A test on one argument of a call.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ArgTest {
    /// The argument is a literal.
    Literal,
    /// The argument is not a literal.
    NonLiteral,
    /// The argument is not passed.
    Absent,
    /// The argument is a string literal equal to the value.
    Equals(String),
    /// The argument is a string literal matching the (restricted) regular expression.
    Matches(String),
}

/// Term: Rule
///
/// A condition on one argument of a matched call.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ArgCondition {
    arg: ArgSel,
    test: ArgTest,
}

impl ArgCondition {
    /// The argument tested.
    pub fn arg(&self) -> &ArgSel {
        &self.arg
    }

    /// The test.
    pub fn test(&self) -> &ArgTest {
        &self.test
    }
}

/// Term: Rule
///
/// The closed set of pattern shapes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PatternShape {
    /// A call to a named symbol, optionally constrained by argument conditions.
    Call {
        /// The called symbol.
        symbol: SymbolRef,
        /// All conditions must hold.
        conditions: Vec<ArgCondition>,
    },
    /// Any use of a named function, class or construct.
    Use {
        /// The used symbol.
        symbol: SymbolRef,
    },
    /// An `include` or `require` whose path is not a literal.
    IncludeNonLiteral,
}

/// Term: Rule
///
/// The body of a pattern rule: one of a closed set of shapes, never arbitrary code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternBody {
    shape: PatternShape,
}

impl PatternBody {
    /// The shape.
    pub fn shape(&self) -> &PatternShape {
        &self.shape
    }
}

/// Term: Rule
///
/// The body of a rule.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RuleBody {
    /// A taint rule.
    Taint(TaintBody),
    /// A pattern rule.
    Pattern(PatternBody),
}

fn callable(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Function
            | SymbolKind::StaticMethod
            | SymbolKind::InstanceMethod
            | SymbolKind::Constructor
    )
}

fn symbol(
    text: &str,
    field: &str,
    what: &str,
    ok: fn(SymbolKind) -> bool,
    errors: &mut Vec<RuleError>,
) -> Option<SymbolRef> {
    match SymbolRef::parse(text) {
        Ok(s) if ok(s.kind()) => Some(s),
        Ok(s) => {
            errors.push(RuleError::new(
                field,
                format!("{what} cannot be {text:?} ({:?})", s.kind()),
            ));
            None
        }
        Err(e) => {
            errors.push(RuleError::new(field, e));
            None
        }
    }
}

fn args(
    list: &[String],
    field: &str,
    allow_receiver: bool,
    errors: &mut Vec<RuleError>,
) -> Vec<ArgSel> {
    if list.is_empty() {
        errors.push(RuleError::new(field, "needs at least one argument"));
    }
    let mut out = Vec::new();
    for (i, text) in list.iter().enumerate() {
        match ArgSel::parse(text) {
            Ok(ArgSel::Receiver) if !allow_receiver => {
                errors.push(RuleError::new(
                    format!("{field}[{i}]"),
                    "`this` is only valid for the inputs of a method propagator",
                ));
            }
            Ok(a) => out.push(a),
            Err(e) => errors.push(RuleError::new(format!("{field}[{i}]"), e)),
        }
    }
    out.sort();
    out.dedup();
    out
}

fn sink(i: usize, d: &SinkDraft, errors: &mut Vec<RuleError>) -> Option<Sink> {
    let f = format!("sinks[{i}]");
    let symbol = symbol(
        &d.symbol,
        &format!("{f}.symbol"),
        "a sink",
        callable,
        errors,
    );
    let vulnerable_args = args(&d.args, &format!("{f}.args"), false, errors);
    let class = SinkClass::parse(&d.class)
        .map_err(|e| errors.push(RuleError::new(format!("{f}.class"), e)))
        .ok();
    Some(Sink {
        symbol: symbol?,
        vulnerable_args,
        class: class?,
    })
}

fn sanitizer(i: usize, d: &SanitizerDraft, errors: &mut Vec<RuleError>) -> Option<Sanitizer> {
    let f = format!("sanitizers[{i}]");
    let sym = symbol(
        &d.symbol,
        &format!("{f}.symbol"),
        "a sanitizer",
        callable,
        errors,
    );
    if d.clears.is_empty() {
        errors.push(RuleError::new(
            format!("{f}.clears"),
            "needs at least one sink class",
        ));
    }
    let mut clears = Vec::new();
    for (j, name) in d.clears.iter().enumerate() {
        match SinkClass::parse(name) {
            Ok(c) => clears.push(c),
            Err(e) => errors.push(RuleError::new(format!("{f}.clears[{j}]"), e)),
        }
    }
    clears.sort();
    clears.dedup();
    Some(Sanitizer {
        symbol: sym?,
        clears,
    })
}

fn propagator(i: usize, d: &PropagatorDraft, errors: &mut Vec<RuleError>) -> Option<Propagator> {
    let f = format!("propagators[{i}]");
    let sym = symbol(
        &d.symbol,
        &format!("{f}.symbol"),
        "a propagator",
        callable,
        errors,
    )?;
    let method = matches!(sym.kind(), SymbolKind::InstanceMethod);
    let from = args(&d.from, &format!("{f}.from"), method, errors);
    Some(Propagator { symbol: sym, from })
}

type One<T, D> = fn(usize, &D, &mut Vec<RuleError>) -> Option<T>;

fn collect<T, D>(items: &[D], errors: &mut Vec<RuleError>, one: One<T, D>) -> Vec<T> {
    items
        .iter()
        .enumerate()
        .filter_map(|(i, d)| one(i, d, errors))
        .collect()
}

fn taint(d: &TaintDraft, errors: &mut Vec<RuleError>) -> Option<TaintBody> {
    let mut sources = Vec::new();
    for (i, text) in d.sources.iter().enumerate() {
        let any = |_| true;
        sources.extend(symbol(
            text,
            &format!("sources[{i}]"),
            "a source",
            any,
            errors,
        ));
    }
    if d.sources.is_empty() {
        errors.push(RuleError::new(
            "sources",
            "a taint rule needs at least one source",
        ));
    }
    if d.sinks.is_empty() {
        errors.push(RuleError::new(
            "sinks",
            "a taint rule needs at least one sink",
        ));
    }
    let sinks = collect(&d.sinks, errors, sink);
    let sanitizers = collect(&d.sanitizers, errors, sanitizer);
    let propagators = collect(&d.propagators, errors, propagator);
    sources.sort();
    sources.dedup();
    Some(TaintBody {
        sources,
        sinks,
        sanitizers,
        propagators,
    })
}

fn condition(i: usize, d: &ArgConditionDraft, errors: &mut Vec<RuleError>) -> Option<ArgCondition> {
    let f = format!("args[{i}]");
    let arg = ArgSel::parse(&d.arg)
        .map_err(|e| errors.push(RuleError::new(format!("{f}.arg"), e)))
        .ok();
    let need_value = |errors: &mut Vec<RuleError>| match &d.value {
        Some(v) => Some(v.clone()),
        None => {
            errors.push(RuleError::new(
                format!("{f}.value"),
                format!("`{}` needs a value", d.test),
            ));
            None
        }
    };
    let test = match d.test.as_str() {
        "literal" => Some(ArgTest::Literal),
        "non-literal" => Some(ArgTest::NonLiteral),
        "absent" => Some(ArgTest::Absent),
        "equals" => need_value(errors).map(ArgTest::Equals),
        "matches" => need_value(errors).and_then(|v| match validate_regex(&v) {
            Ok(()) => Some(ArgTest::Matches(v)),
            Err(e) => {
                errors.push(RuleError::new(
                    format!("{f}.value"),
                    format!("regular expression rejected: {e}"),
                ));
                None
            }
        }),
        other => {
            errors.push(RuleError::new(
                format!("{f}.test"),
                format!("unknown test {other:?}; known tests: literal, non-literal, absent, equals, matches"),
            ));
            None
        }
    };
    Some(ArgCondition {
        arg: arg?,
        test: test?,
    })
}

fn pattern(d: &PatternDraft, errors: &mut Vec<RuleError>) -> Option<PatternBody> {
    let named = |errors: &mut Vec<RuleError>, ok: fn(SymbolKind) -> bool| match &d.symbol {
        Some(s) => symbol(s, "symbol", "the symbol of this shape", ok, errors),
        None => {
            errors.push(RuleError::new(
                "symbol",
                format!("shape `{}` needs a symbol", d.shape),
            ));
            None
        }
    };
    let shape = match d.shape.as_str() {
        "call" => {
            let sym = named(errors, callable);
            let conditions = collect(&d.args, errors, condition);
            sym.map(|symbol| PatternShape::Call { symbol, conditions })
        }
        "use" => {
            let no_super = |k: SymbolKind| k != SymbolKind::Superglobal;
            named(errors, no_super).map(|symbol| PatternShape::Use { symbol })
        }
        "include-non-literal" => Some(PatternShape::IncludeNonLiteral),
        other => {
            errors.push(RuleError::new(
                "shape",
                format!(
                    "unsupported shape {other:?}; supported shapes: call, use, include-non-literal"
                ),
            ));
            None
        }
    };
    if d.shape != "call" && !d.args.is_empty() {
        errors.push(RuleError::new(
            "args",
            "argument conditions are only valid for the `call` shape",
        ));
    }
    shape.map(|shape| PatternBody { shape })
}

/// Validates a body draft; problems are appended to `errors` under the `body` prefix.
pub(crate) fn validate_body(d: &BodyDraft, errors: &mut Vec<RuleError>) -> Option<RuleBody> {
    let mut own = Vec::new();
    let body = match d {
        BodyDraft::Taint(t) => taint(t, &mut own).map(RuleBody::Taint),
        BodyDraft::Pattern(p) => pattern(p, &mut own).map(RuleBody::Pattern),
    };
    let failed = !own.is_empty();
    errors.extend(prefixed("body", own));
    if failed { None } else { body }
}

impl RuleBody {
    /// Appends a canonical text form: sets are sorted, so declaration order does not matter.
    pub(crate) fn canonical(&self, out: &mut Vec<String>) {
        match self {
            Self::Taint(t) => {
                out.push("body=taint".to_string());
                sorted(out, "source", t.sources.iter().map(ToString::to_string));
                sorted(
                    out,
                    "sink",
                    t.sinks.iter().map(|s| {
                        format!(
                            "{} args={} class={}",
                            s.symbol,
                            join(&s.vulnerable_args),
                            s.class
                        )
                    }),
                );
                sorted(
                    out,
                    "sanitizer",
                    t.sanitizers
                        .iter()
                        .map(|s| format!("{} clears={}", s.symbol, join(&s.clears))),
                );
                sorted(
                    out,
                    "propagator",
                    t.propagators
                        .iter()
                        .map(|p| format!("{} from={}", p.symbol, join(&p.from))),
                );
            }
            Self::Pattern(p) => {
                out.push("body=pattern".to_string());
                match &p.shape {
                    PatternShape::Call { symbol, conditions } => {
                        out.push(format!("shape=call {symbol}"));
                        sorted(
                            out,
                            "arg",
                            conditions.iter().map(|c| format!("{} {:?}", c.arg, c.test)),
                        );
                    }
                    PatternShape::Use { symbol } => out.push(format!("shape=use {symbol}")),
                    PatternShape::IncludeNonLiteral => {
                        out.push("shape=include-non-literal".to_string())
                    }
                }
            }
        }
    }
}

fn join<T: ToString>(items: &[T]) -> String {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn sorted(out: &mut Vec<String>, key: &str, items: impl Iterator<Item = String>) {
    let mut lines: Vec<String> = items.map(|i| format!("{key}={i}")).collect();
    lines.sort();
    out.extend(lines);
}
