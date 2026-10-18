//! The control-flow graph of one unit.

use std::collections::BTreeMap;

use bleedthrough_syntax::Span;

use crate::ids::{BlockId, LocalId, ValueId};
use crate::instr::Instr;

/// Term: exempt
///
/// What kind of code a graph holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum UnitKind {
    /// A function declaration.
    Function,
    /// A method.
    Method,
    /// A closure (`function () use (...) {}`).
    Closure,
    /// An arrow function.
    ArrowFn,
    /// A property hook body.
    Hook,
    /// The top-level code of a file.
    Main,
}

impl UnitKind {
    /// The stable lowercase name used in dumps.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Closure => "closure",
            Self::ArrowFn => "arrow-fn",
            Self::Hook => "hook",
            Self::Main => "main",
        }
    }
}

/// Term: exempt
///
/// Identifies a unit inside one file: where it starts and ends and its display name. Keys sort by
/// position, which is the canonical unit order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct UnitKey {
    /// The source of the unit.
    pub span: Span,
    /// The display name inference gave the unit: `App\Foo::bar`, `{main}`, `{closure}@12`.
    pub name: Box<str>,
}

/// Term: exempt
///
/// A parameter of a unit. Parameters are the first locals of the graph, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct ParamInfo {
    /// The local holding the argument.
    pub local: LocalId,
    /// The name without `$`.
    pub name: Box<str>,
    /// Whether it is taken by reference.
    pub by_ref: bool,
    /// Whether it collects the remaining arguments.
    pub variadic: bool,
    /// Where it is declared.
    pub span: Span,
}

/// Term: exempt
///
/// A variable a closure takes from the unit that created it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct Capture {
    /// The local holding the captured value inside the closure.
    pub local: LocalId,
    /// The name without `$`.
    pub name: Box<str>,
    /// Whether it is captured by reference.
    pub by_ref: bool,
}

/// Term: exempt
///
/// How a block ends.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Terminator {
    /// Continue in another block.
    Goto(BlockId),
    /// Continue in one of two blocks by a condition.
    Branch {
        /// The condition.
        cond: ValueId,
        /// Where a true condition goes.
        then: BlockId,
        /// Where a false condition goes.
        otherwise: BlockId,
    },
    /// Continue in any of the blocks: an over-approximation of a choice the graph does not model.
    Choice(Vec<BlockId>),
    /// Leave the unit with a value; control reaches the exit block.
    Return(Option<ValueId>),
    /// Throw; control reaches the block's handlers.
    Throw(ValueId),
    /// The exit block's end: the unit has finished.
    Exit,
    /// Control does not continue: `exit`, `die`, or code after a jump.
    Unreachable,
}

/// Term: exempt
///
/// A basic block: instructions that run in sequence, a terminator, and the handlers an exception
/// raised anywhere inside the block may reach.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct Block {
    /// The instructions in execution order.
    pub instrs: Vec<Instr>,
    /// How the block ends.
    pub term: Terminator,
    /// Blocks an exception raised at any point inside this block may continue in.
    pub handlers: Vec<BlockId>,
}

/// Term: exempt
///
/// Which lowering bound was reached for a graph. A bound reached makes the graph conservative,
/// not wrong, and is reported as a limit hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record of flags")]
pub struct LimitsHit {
    /// Expressions nested deeper than the depth bound were flattened into unknown instructions.
    pub depth: bool,
    /// The unit exceeded the block bound; the rest of its code was not lowered.
    pub blocks: bool,
    /// The unit exceeded the instruction bound; the rest of its code was not lowered.
    pub instructions: bool,
}

impl LimitsHit {
    /// Whether any bound was reached.
    pub fn any(&self) -> bool {
        self.depth || self.blocks || self.instructions
    }
}

/// Term: exempt
///
/// The control-flow graph of one function, method, closure, arrow function, property hook or the
/// top-level code of a file. Blocks and values are numbered in the order lowering meets them in
/// the source.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct Graph {
    /// Which unit it is.
    pub key: UnitKey,
    /// What kind of code it holds.
    pub kind: UnitKind,
    /// The class-like that declares a method or hook, as inference names it.
    pub owner: Option<Box<str>>,
    /// Whether the unit has no `$this`.
    pub is_static: bool,
    /// The parameters.
    pub params: Vec<ParamInfo>,
    /// The variables a closure takes from its creator.
    pub captures: Vec<Capture>,
    /// Local names in id order; names starting with `%` are synthetic.
    pub locals: Vec<Box<str>>,
    /// The blocks in id order.
    pub blocks: Vec<Block>,
    /// The first block.
    pub entry: BlockId,
    /// The single exit block.
    pub exit: BlockId,
    /// How many values the graph defines.
    pub value_count: u32,
    /// Whether an error node was met inside the unit: the graph is conservative.
    pub recovered: bool,
    /// Unknown instructions by syntax kind.
    pub unknowns: BTreeMap<Box<str>, u32>,
    /// Which lowering bounds were reached.
    pub limits: LimitsHit,
}

impl Graph {
    /// The block with this id.
    pub fn block(&self, id: BlockId) -> &Block {
        &self.blocks[id.index()]
    }

    /// The name of a local without `$`.
    pub fn local_name(&self, local: LocalId) -> &str {
        self.locals.get(local.index()).map_or("?", |n| n)
    }

    /// The successors of a block along normal control flow, in a fixed order.
    pub fn successors(&self, id: BlockId) -> Vec<BlockId> {
        match &self.block(id).term {
            Terminator::Goto(b) => vec![*b],
            Terminator::Branch {
                then, otherwise, ..
            } => vec![*then, *otherwise],
            Terminator::Choice(targets) => targets.clone(),
            Terminator::Return(_) => vec![self.exit],
            Terminator::Throw(_) => self.block(id).handlers.clone(),
            Terminator::Exit | Terminator::Unreachable => Vec::new(),
        }
    }

    /// Number of instructions.
    pub fn instruction_count(&self) -> usize {
        self.blocks.iter().map(|b| b.instrs.len()).sum()
    }

    /// Total unknown instructions.
    pub fn unknown_count(&self) -> u32 {
        self.unknowns.values().sum()
    }

    /// The blocks reachable from the entry along normal and exceptional edges, in reverse
    /// post-order. Computed with an explicit stack.
    pub fn reverse_post_order(&self) -> Vec<BlockId> {
        let mut seen = vec![false; self.blocks.len()];
        let mut post = Vec::with_capacity(self.blocks.len());
        type Frame = (BlockId, Vec<BlockId>, usize);
        let mut stack: Vec<Frame> = Vec::new();
        seen[self.entry.index()] = true;
        stack.push((self.entry, self.all_successors(self.entry), 0));
        while let Some((block, succ, next)) = stack.last_mut() {
            if let Some(&target) = succ.get(*next) {
                *next += 1;
                if !seen[target.index()] {
                    seen[target.index()] = true;
                    let more = self.all_successors(target);
                    stack.push((target, more, 0));
                }
            } else {
                post.push(*block);
                stack.pop();
            }
        }
        post.reverse();
        post
    }

    /// Normal successors followed by the block's handlers.
    pub fn all_successors(&self, id: BlockId) -> Vec<BlockId> {
        let mut out = self.successors(id);
        for handler in &self.block(id).handlers {
            if !out.contains(handler) {
                out.push(*handler);
            }
        }
        out
    }
}
