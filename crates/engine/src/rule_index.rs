//! The taint rules, indexed once by the symbol they name, so a call is classified by one lookup.

use std::collections::BTreeMap;

use bleedthrough_rules::{
    ArgSel, PhpVersion, RuleBody, RuleId, RuleSet, SinkClass, SymbolKind, SymbolRef,
};
use bleedthrough_symbols::{Fqn, Subtyping, SymbolTable};

use crate::label::ClassSet;

/// Term: Rule
///
/// Which rules apply to this run: the modelled PHP version and whether paranoid rules are on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record of settings")]
pub struct Gate {
    /// The PHP version modelled; `None` applies every rule whatever its bounds.
    pub php: Option<PhpVersion>,
    /// Whether paranoid rules run.
    pub paranoid: bool,
}

impl Gate {
    fn admits(&self, rule: &bleedthrough_rules::Rule) -> bool {
        if rule.paranoid() && !self.paranoid {
            return false;
        }
        match self.php {
            None => true,
            Some(version) => {
                rule.php_min().is_none_or(|min| version >= min)
                    && rule.php_max().is_none_or(|max| version <= max)
            }
        }
    }
}

/// Term: Rule
///
/// A source, with the rules that name it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct SourceDef {
    /// The symbol.
    pub symbol: SymbolRef,
    /// The rules that list it, as positions in [`RuleIndex::rules`].
    pub rules: Vec<u32>,
}

/// Term: Rule
///
/// A sink of one rule.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct SinkEntry {
    /// The rule, as a position in [`RuleIndex::rules`].
    pub rule: u32,
    /// The symbol.
    pub symbol: SymbolRef,
    /// The vulnerable arguments.
    pub args: Vec<ArgSel>,
    /// The class of the sink.
    pub class: SinkClass,
}

/// Term: Rule
///
/// A sanitizer of one rule.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct SanitizerEntry {
    /// The rule, as a position in [`RuleIndex::rules`].
    pub rule: u32,
    /// The symbol.
    pub symbol: SymbolRef,
    /// The classes it clears.
    pub clears: ClassSet,
}

/// Term: Rule
///
/// A propagator of one rule.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct PropagatorEntry {
    /// The rule, as a position in [`RuleIndex::rules`].
    pub rule: u32,
    /// The symbol.
    pub symbol: SymbolRef,
    /// The inputs that flow to the result.
    pub from: Vec<ArgSel>,
}

/// Term: Rule
///
/// What is being looked up: the resolved symbol of a call, constructor or superglobal.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum Subject<'a> {
    /// A function, by fully qualified name.
    Function(&'a str),
    /// A static method of a class.
    Static {
        /// The class that declares it.
        class: &'a Fqn,
        /// The method name.
        method: &'a str,
    },
    /// An instance method of a class.
    Instance {
        /// The class that declares it.
        class: &'a Fqn,
        /// The method name.
        method: &'a str,
    },
    /// The constructor of a class.
    Constructor {
        /// The class.
        class: &'a Fqn,
    },
    /// A superglobal, written with its `$`.
    Superglobal(&'a str),
}

/// Term: Rule
///
/// The roles a symbol plays, as positions into the index's lists.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct Matches {
    /// Source definitions.
    pub sources: Vec<u32>,
    /// Sink entries.
    pub sinks: Vec<u32>,
    /// Sanitizer entries.
    pub sanitizers: Vec<u32>,
    /// Propagator entries.
    pub propagators: Vec<u32>,
}

impl Matches {
    /// Whether the symbol plays no role.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
            && self.sinks.is_empty()
            && self.sanitizers.is_empty()
            && self.propagators.is_empty()
    }

    fn push(&mut self, role: Role, at: u32) {
        match role {
            Role::Source => self.sources.push(at),
            Role::Sink => self.sinks.push(at),
            Role::Sanitizer => self.sanitizers.push(at),
            Role::Propagator => self.propagators.push(at),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Source,
    Sink,
    Sanitizer,
    Propagator,
}

/// One class-qualified entry under a method name.
#[derive(Debug, Clone)]
struct MethodRule {
    class: Fqn,
    kind: SymbolKind,
    subclasses: bool,
    role: Role,
    at: u32,
}

type Entries = Vec<(Role, u32)>;

/// Term: Rule
///
/// The active taint rules, indexed by symbol.
#[derive(Debug, Clone, Default)]
pub struct RuleIndex {
    rules: Vec<RuleId>,
    sources: Vec<SourceDef>,
    sinks: Vec<SinkEntry>,
    sanitizers: Vec<SanitizerEntry>,
    propagators: Vec<PropagatorEntry>,
    functions: BTreeMap<String, Entries>,
    superglobals: BTreeMap<String, Entries>,
    methods: BTreeMap<String, Vec<MethodRule>>,
}

impl RuleIndex {
    /// Indexes the taint rules of `set` that `gate` admits. Pattern rules are not part of it.
    pub fn build(set: &RuleSet, gate: &Gate) -> Self {
        let mut index = Self::default();
        let mut source_at: BTreeMap<SymbolRef, u32> = BTreeMap::new();
        for rule in set.iter() {
            let RuleBody::Taint(body) = rule.body() else {
                continue;
            };
            if !gate.admits(rule) {
                continue;
            }
            let rule_at = index.rules.len() as u32;
            index.rules.push(rule.id().clone());
            for symbol in body.sources() {
                let (at, fresh) = match source_at.get(symbol) {
                    Some(at) => (*at, false),
                    None => {
                        index.sources.push(SourceDef {
                            symbol: symbol.clone(),
                            rules: Vec::new(),
                        });
                        let at = (index.sources.len() - 1) as u32;
                        source_at.insert(symbol.clone(), at);
                        (at, true)
                    }
                };
                let rules = &mut index.sources[at as usize].rules;
                if !rules.contains(&rule_at) {
                    rules.push(rule_at);
                }
                if fresh {
                    index.register(symbol, Role::Source, at);
                }
            }
            for sink in body.sinks() {
                let at = index.sinks.len() as u32;
                index.sinks.push(SinkEntry {
                    rule: rule_at,
                    symbol: sink.symbol().clone(),
                    args: sink.vulnerable_args().to_vec(),
                    class: sink.class(),
                });
                index.register(sink.symbol(), Role::Sink, at);
            }
            for sanitizer in body.sanitizers() {
                let at = index.sanitizers.len() as u32;
                index.sanitizers.push(SanitizerEntry {
                    rule: rule_at,
                    symbol: sanitizer.symbol().clone(),
                    clears: ClassSet::from_classes(sanitizer.clears()),
                });
                index.register(sanitizer.symbol(), Role::Sanitizer, at);
            }
            for propagator in body.propagators() {
                let at = index.propagators.len() as u32;
                index.propagators.push(PropagatorEntry {
                    rule: rule_at,
                    symbol: propagator.symbol().clone(),
                    from: propagator.from().to_vec(),
                });
                index.register(propagator.symbol(), Role::Propagator, at);
            }
        }
        index
    }

    fn register(&mut self, symbol: &SymbolRef, role: Role, at: u32) {
        match symbol.kind() {
            SymbolKind::Function => self
                .functions
                .entry(qualified_key(symbol, symbol.name()))
                .or_default()
                .push((role, at)),
            SymbolKind::Superglobal => self
                .superglobals
                .entry(symbol.name().to_string())
                .or_default()
                .push((role, at)),
            kind => {
                let class = Fqn::in_namespace(
                    &symbol.namespace().join("\\"),
                    symbol.class().unwrap_or_default(),
                );
                self.methods
                    .entry(symbol.name().to_ascii_lowercase())
                    .or_default()
                    .push(MethodRule {
                        class,
                        kind,
                        subclasses: symbol.include_subclasses(),
                        role,
                        at,
                    });
            }
        }
    }

    /// The active rules, in the order their positions refer to.
    pub fn rules(&self) -> &[RuleId] {
        &self.rules
    }

    /// The sources.
    pub fn sources(&self) -> &[SourceDef] {
        &self.sources
    }

    /// The sinks.
    pub fn sinks(&self) -> &[SinkEntry] {
        &self.sinks
    }

    /// The sanitizers.
    pub fn sanitizers(&self) -> &[SanitizerEntry] {
        &self.sanitizers
    }

    /// The propagators.
    pub fn propagators(&self) -> &[PropagatorEntry] {
        &self.propagators
    }

    /// Whether any rule is active.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The roles `subject` plays. A method entry matches the class that declares the method, or,
    /// with `+`, any subclass of it.
    pub fn lookup(&self, table: &SymbolTable, subject: Subject<'_>) -> Matches {
        let mut out = Matches::default();
        match subject {
            Subject::Function(name) => {
                let key = Fqn::new(name).key().to_string();
                for (role, at) in self.functions.get(&key).into_iter().flatten() {
                    out.push(*role, *at);
                }
            }
            Subject::Superglobal(name) => {
                for (role, at) in self.superglobals.get(name).into_iter().flatten() {
                    out.push(*role, *at);
                }
            }
            Subject::Static { class, method } => {
                self.lookup_method(table, class, method, SymbolKind::StaticMethod, &mut out);
            }
            Subject::Instance { class, method } => {
                self.lookup_method(table, class, method, SymbolKind::InstanceMethod, &mut out);
            }
            Subject::Constructor { class } => {
                self.lookup_method(
                    table,
                    class,
                    "__construct",
                    SymbolKind::Constructor,
                    &mut out,
                );
            }
        }
        out
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "the lookup subject, split into its parts"
    )]
    fn lookup_method(
        &self,
        table: &SymbolTable,
        class: &Fqn,
        method: &str,
        kind: SymbolKind,
        out: &mut Matches,
    ) {
        let key = method.to_ascii_lowercase();
        for rule in self.methods.get(&key).into_iter().flatten() {
            if rule.kind != kind {
                continue;
            }
            let hit = rule.class == *class
                || (rule.subclasses && table.is_subtype(class, &rule.class) == Subtyping::Yes);
            if hit {
                out.push(rule.role, rule.at);
            }
        }
    }
}

fn qualified_key(symbol: &SymbolRef, last: &str) -> String {
    let mut parts: Vec<&str> = symbol.namespace().iter().map(String::as_str).collect();
    parts.push(last);
    Fqn::new(&parts.join("\\")).key().to_string()
}
