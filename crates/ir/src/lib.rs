//! **Layer:** Domain.
//!
//! **Responsibility:** the intermediate representation and flow graphs analysed by the engine: one
//! control-flow graph per function-like unit, lowered from the owned syntax tree with the call
//! resolutions type inference computed.
//!
//! **Permitted workspace dependencies:** `bleedthrough-symbols`, `bleedthrough-syntax`.
//!
//! Lowering is total over the syntax model, deterministic, and free of unbounded recursion.

#![warn(clippy::exhaustive_enums, clippy::exhaustive_structs)]

mod dump;
mod graph;
mod ids;
mod instr;
mod lower;

pub use dump::{dump, dump_with_source};
pub use graph::{Block, Capture, Graph, LimitsHit, ParamInfo, Terminator, UnitKey, UnitKind};
pub use ids::{BlockId, LocalId, ValueId};
pub use instr::{
    ArrayElem, CallArg, Callee, CaptureBinding, ClassForm, ClassOperand, ConstValue, Instr,
    InstrKind, InterpPart, IterPart, MemberRef, Operand, Place, Resolution, TestKind, TypeClass,
};
pub use lower::{LowerConfig, lower_file, lower_file_with};
