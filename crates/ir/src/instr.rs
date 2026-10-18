//! Instructions: simple operations over single-assignment temporaries and named locals.

use bleedthrough_symbols::{CallResolution, Fqn};
use bleedthrough_syntax::{
    BinaryOp, CastKind, ClassRefKind, IncludeKind, MagicConstantKind, Name, Span, StringKind,
    UnaryOp,
};

use crate::graph::UnitKey;
use crate::ids::{LocalId, ValueId};

/// Term: exempt
///
/// What a value can carry, summarised from the inferred type at lowering time. It is all the
/// engine needs from the type model to prune taint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum TypeClass {
    /// Certainly numeric, boolean, null, `void` or an enum case: it cannot carry string, array or
    /// object data.
    Inert,
    /// A known type that may carry string, array or object data.
    Data,
    /// The type is unknown or `mixed`, or no type was recorded.
    Unknown,
}

impl TypeClass {
    /// The stable lowercase name used in dumps.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Inert => "inert",
            Self::Data => "data",
            Self::Unknown => "unknown",
        }
    }
}

/// Term: exempt
///
/// A literal value. Strings are decoded when the escape rules make the value certain and `None`
/// otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConstValue {
    /// An integer, as written.
    Int(Box<str>),
    /// A float, as written.
    Float(Box<str>),
    /// A string with its value when certain.
    Str(Option<Box<str>>),
    /// `true` or `false`.
    Bool(bool),
    /// `null`.
    Null,
}

/// Term: exempt
///
/// A value an instruction operates on together with the source it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain pair")]
pub struct Operand {
    /// The value.
    pub value: ValueId,
    /// The source of the operand.
    pub span: Span,
}

/// Term: exempt
///
/// A part of an interpolated string.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum InterpPart {
    /// Literal text between interpolations.
    Text {
        /// The raw text.
        raw: Box<str>,
        /// Where it is.
        span: Span,
    },
    /// An interpolated expression.
    Value(Operand),
}

/// Term: exempt
///
/// A member name after `->` or `::`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MemberRef {
    /// A name written in the source.
    Named(Box<str>),
    /// A name computed at run time.
    Dynamic(ValueId),
    /// The name is missing (an error node produced it).
    Missing,
}

/// Term: exempt
///
/// How a class was written.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ClassForm {
    /// A class name as written.
    Named(Name),
    /// `self`.
    SelfRef,
    /// `parent`.
    Parent,
    /// `static`.
    Static,
    /// A computed class: a variable or any other expression.
    Value(ValueId),
    /// An anonymous class.
    Anonymous,
}

/// Term: exempt
///
/// A class used by `new`, `::` or `instanceof`: how it was written and the class names the type
/// facts resolved it to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct ClassOperand {
    /// How it was written.
    pub form: ClassForm,
    /// The classes it may denote, sorted and distinct; empty when unknown.
    pub classes: Vec<Fqn>,
}

impl ClassOperand {
    /// The operand for a relative class reference.
    pub fn relative(kind: ClassRefKind, classes: Vec<Fqn>) -> Self {
        let form = match kind {
            ClassRefKind::SelfRef => ClassForm::SelfRef,
            ClassRefKind::Parent => ClassForm::Parent,
            _ => ClassForm::Static,
        };
        Self { form, classes }
    }
}

/// Term: exempt
///
/// A storage location that can be read and written.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Place {
    /// A local variable.
    Local(LocalId),
    /// A property of an object value; `owners` are the classes the receiver's type names.
    Prop {
        /// The receiver.
        object: ValueId,
        /// The property name.
        name: MemberRef,
        /// The classes the receiver may be, sorted and distinct; empty when unknown.
        owners: Vec<Fqn>,
    },
    /// A static property.
    StaticProp {
        /// The class.
        class: ClassOperand,
        /// The property name.
        name: MemberRef,
    },
    /// A global variable by name (`global $x`, `$GLOBALS['x']`).
    Global(Box<str>),
}

/// Term: exempt
///
/// What a call invokes, as written.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Callee {
    /// A function named in the source.
    Function(Name),
    /// A callable value: a closure, a string, an array.
    Value(ValueId),
    /// An instance method.
    Method {
        /// The receiver.
        receiver: ValueId,
        /// The method name.
        name: MemberRef,
        /// Whether the call is `?->`.
        nullsafe: bool,
    },
    /// A static method.
    Static {
        /// The class.
        class: ClassOperand,
        /// The method name.
        name: MemberRef,
    },
}

/// Term: exempt
///
/// One argument of a call.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct CallArg {
    /// The argument's value.
    pub value: ValueId,
    /// The parameter name for a named argument.
    pub name: Option<Box<str>>,
    /// Whether the argument is spread (`...$args`).
    pub spread: bool,
    /// The source of the argument.
    pub span: Span,
}

/// Term: exempt
///
/// The call resolution that type inference computed for a call expression, or the explicit marker
/// that there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Resolution {
    /// Inference resolved (or explicitly failed to resolve) this call.
    Facts(CallResolution),
    /// Inference recorded nothing for this expression.
    Missing,
}

/// Term: exempt
///
/// An element of an array literal.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct ArrayElem {
    /// The key, if written.
    pub key: Option<ValueId>,
    /// The value.
    pub value: ValueId,
    /// Whether the element is spread (`...$other`).
    pub spread: bool,
}

/// Term: exempt
///
/// What a [`InstrKind::Test`] checks. Every test yields a boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TestKind {
    /// `isset(...)`.
    Isset,
    /// `empty(...)`.
    Empty,
    /// The outcome of a short-circuit operator.
    Logical,
    /// A condition the lowering synthesised, such as "the iterator has another item".
    Synthetic,
}

impl TestKind {
    /// The stable lowercase name used in dumps.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Isset => "isset",
            Self::Empty => "empty",
            Self::Logical => "logical",
            Self::Synthetic => "synthetic",
        }
    }
}

/// Term: exempt
///
/// Which half of an item a `foreach` produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum IterPart {
    /// The key.
    Key,
    /// The value.
    Value,
}

/// Term: exempt
///
/// A variable a closure takes from its creator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct CaptureBinding {
    /// The local in the creating unit.
    pub outer: LocalId,
    /// The local in the closure.
    pub inner: LocalId,
    /// Whether the capture is by reference.
    pub by_ref: bool,
}

/// Term: exempt
///
/// The operation an instruction performs.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum InstrKind {
    /// A literal.
    Const(ConstValue),
    /// A magic constant such as `__DIR__`.
    Magic(MagicConstantKind),
    /// A constant read by name.
    ConstFetch(Name),
    /// A class constant, including `Foo::class`.
    ClassConst {
        /// The class.
        class: ClassOperand,
        /// The constant name.
        name: MemberRef,
    },
    /// A read of a superglobal such as `$_GET` (stored without the `$`).
    Superglobal(Box<str>),
    /// A read of a place.
    Load(Place),
    /// A read of an array element.
    LoadElem {
        /// The array.
        base: ValueId,
        /// The key; `None` for `$a[]`.
        index: Option<ValueId>,
    },
    /// A write to a place. A weak write (an element write) adds to the place instead of
    /// replacing it.
    Store {
        /// Where.
        place: Place,
        /// What.
        value: ValueId,
        /// Whether the write only adds to what the place holds.
        weak: bool,
    },
    /// String concatenation: the result depends on every operand.
    Concat(Vec<Operand>),
    /// An interpolated string, heredoc or backtick command.
    Interp {
        /// The form.
        kind: StringKind,
        /// Text and values in source order.
        parts: Vec<InterpPart>,
    },
    /// A binary operation other than concatenation and the short-circuit operators.
    Binary {
        /// The operator.
        op: BinaryOp,
        /// Left operand.
        lhs: ValueId,
        /// Right operand.
        rhs: ValueId,
    },
    /// A unary operation.
    Unary {
        /// The operator.
        op: UnaryOp,
        /// The operand.
        operand: ValueId,
    },
    /// A cast.
    Cast {
        /// The target type.
        kind: CastKind,
        /// The operand.
        operand: ValueId,
    },
    /// A plain copy, such as `clone`.
    Copy(ValueId),
    /// A call.
    Call {
        /// What is called.
        callee: Callee,
        /// The arguments in source order.
        args: Vec<CallArg>,
        /// The resolution from type inference.
        resolution: Resolution,
    },
    /// `new`.
    New {
        /// The class.
        class: ClassOperand,
        /// The constructor arguments.
        args: Vec<CallArg>,
        /// The resolution of the constructor.
        resolution: Resolution,
    },
    /// A first-class callable such as `strlen(...)`.
    CallableRef {
        /// What is referenced.
        callee: Callee,
        /// The resolution from type inference.
        resolution: Resolution,
    },
    /// `include`, `include_once`, `require` or `require_once`.
    Include {
        /// Which one.
        kind: IncludeKind,
        /// The path.
        path: ValueId,
    },
    /// `eval`.
    Eval(ValueId),
    /// One value of an `echo` statement.
    Echo(Operand),
    /// `print`; the result is `1`.
    Print(ValueId),
    /// `exit` or `die`, with the message or status if given.
    Exit(Option<ValueId>),
    /// An array literal.
    Array(Vec<ArrayElem>),
    /// A closure or arrow function creation; the body is the graph with this key.
    Closure {
        /// The closure's graph.
        unit: UnitKey,
        /// The variables it takes.
        captures: Vec<CaptureBinding>,
    },
    /// `yield`; the result is the value sent in.
    Yield {
        /// The key.
        key: Option<ValueId>,
        /// The yielded value.
        value: Option<ValueId>,
    },
    /// `yield from`.
    YieldFrom(ValueId),
    /// A boolean-valued test.
    Test {
        /// What is tested.
        kind: TestKind,
        /// The tested values.
        operands: Vec<ValueId>,
    },
    /// One item of a `foreach` subject.
    Iter {
        /// The iterated value.
        subject: ValueId,
        /// Key or value.
        part: IterPart,
    },
    /// A construct lowering does not model. The result depends on every operand.
    Unknown {
        /// The syntax kind, for the count.
        kind: Box<str>,
        /// What could be seen of the construct.
        operands: Vec<ValueId>,
    },
}

/// Term: exempt
///
/// One instruction: what it does, the value it defines and the source it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct Instr {
    /// The source of the instruction.
    pub span: Span,
    /// The value it defines, if any.
    pub dest: Option<ValueId>,
    /// What the defined value can carry.
    pub ty: TypeClass,
    /// The operation.
    pub kind: InstrKind,
}

impl Instr {
    /// Every value the instruction reads, in a fixed order.
    pub fn operands(&self) -> Vec<ValueId> {
        let mut out = Vec::new();
        collect_operands(&self.kind, &mut out);
        out
    }
}

fn place_values(place: &Place, out: &mut Vec<ValueId>) {
    match place {
        Place::Prop { object, name, .. } => {
            out.push(*object);
            member_value(name, out);
        }
        Place::StaticProp { class, name } => {
            class_value(class, out);
            member_value(name, out);
        }
        Place::Local(_) | Place::Global(_) => {}
    }
}

fn member_value(name: &MemberRef, out: &mut Vec<ValueId>) {
    if let MemberRef::Dynamic(v) = name {
        out.push(*v);
    }
}

fn class_value(class: &ClassOperand, out: &mut Vec<ValueId>) {
    if let ClassForm::Value(v) = &class.form {
        out.push(*v);
    }
}

fn callee_values(callee: &Callee, out: &mut Vec<ValueId>) {
    match callee {
        Callee::Function(_) => {}
        Callee::Value(v) => out.push(*v),
        Callee::Method { receiver, name, .. } => {
            out.push(*receiver);
            member_value(name, out);
        }
        Callee::Static { class, name } => {
            class_value(class, out);
            member_value(name, out);
        }
    }
}

fn collect_operands(kind: &InstrKind, out: &mut Vec<ValueId>) {
    match kind {
        InstrKind::Const(_)
        | InstrKind::Magic(_)
        | InstrKind::ConstFetch(_)
        | InstrKind::Superglobal(_)
        | InstrKind::Closure { .. } => {}
        InstrKind::ClassConst { class, name } => {
            class_value(class, out);
            member_value(name, out);
        }
        InstrKind::Load(place) => place_values(place, out),
        InstrKind::LoadElem { base, index } => {
            out.push(*base);
            out.extend(index);
        }
        InstrKind::Store { place, value, .. } => {
            place_values(place, out);
            out.push(*value);
        }
        InstrKind::Concat(parts) => out.extend(parts.iter().map(|p| p.value)),
        InstrKind::Interp { parts, .. } => out.extend(parts.iter().filter_map(|p| match p {
            InterpPart::Value(o) => Some(o.value),
            InterpPart::Text { .. } => None,
        })),
        InstrKind::Binary { lhs, rhs, .. } => out.extend([*lhs, *rhs]),
        InstrKind::Unary { operand, .. }
        | InstrKind::Cast { operand, .. }
        | InstrKind::Copy(operand)
        | InstrKind::Eval(operand)
        | InstrKind::Print(operand)
        | InstrKind::YieldFrom(operand) => out.push(*operand),
        InstrKind::Call { callee, args, .. } => {
            callee_values(callee, out);
            out.extend(args.iter().map(|a| a.value));
        }
        InstrKind::New { class, args, .. } => {
            class_value(class, out);
            out.extend(args.iter().map(|a| a.value));
        }
        InstrKind::CallableRef { callee, .. } => callee_values(callee, out),
        InstrKind::Include { path, .. } => out.push(*path),
        InstrKind::Echo(o) => out.push(o.value),
        InstrKind::Exit(v) => out.extend(v),
        InstrKind::Array(items) => {
            for item in items {
                out.extend(item.key);
                out.push(item.value);
            }
        }
        InstrKind::Yield { key, value } => {
            out.extend(key);
            out.extend(value);
        }
        InstrKind::Test { operands, .. } | InstrKind::Unknown { operands, .. } => {
            out.extend(operands.iter().copied());
        }
        InstrKind::Iter { subject, .. } => out.push(*subject),
    }
}
