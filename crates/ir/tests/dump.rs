//! The textual dump of hand-built graphs: the format every golden test depends on.

use std::collections::BTreeMap;

use bleedthrough_ir::{
    Block, BlockId, CallArg, Callee, Capture, ConstValue, Graph, Instr, InstrKind, InterpPart,
    LimitsHit, LocalId, MemberRef, Operand, ParamInfo, Place, Resolution, Terminator, TypeClass,
    UnitKey, UnitKind, ValueId, dump, dump_with_source,
};
use bleedthrough_syntax::{Name, NameKind, Span, StringKind};

fn v(n: u32) -> ValueId {
    ValueId::new(n)
}

fn b(n: u32) -> BlockId {
    BlockId::new(n)
}

fn instr(span: (u32, u32), dest: Option<u32>, ty: TypeClass, kind: InstrKind) -> Instr {
    Instr {
        span: Span::new(span.0, span.1),
        dest: dest.map(v),
        ty,
        kind,
    }
}

#[allow(clippy::too_many_lines, reason = "a fixture listing one node per kind")]
fn sample() -> Graph {
    let name = Name {
        span: Span::new(0, 6),
        kind: NameKind::Unqualified,
        text: "mysqli_query".into(),
    };
    let entry = Block {
        instrs: vec![
            instr(
                (0, 4),
                Some(0),
                TypeClass::Unknown,
                InstrKind::Load(Place::Local(LocalId::new(0))),
            ),
            instr(
                (5, 9),
                Some(1),
                TypeClass::Data,
                InstrKind::Const(ConstValue::Str(Some("id=".into()))),
            ),
            instr(
                (10, 20),
                Some(2),
                TypeClass::Data,
                InstrKind::Interp {
                    kind: StringKind::DoubleQuoted,
                    parts: vec![
                        InterpPart::Text {
                            raw: "id=".into(),
                            span: Span::new(11, 14),
                        },
                        InterpPart::Value(Operand {
                            value: v(0),
                            span: Span::new(14, 16),
                        }),
                    ],
                },
            ),
            instr(
                (21, 40),
                Some(3),
                TypeClass::Unknown,
                InstrKind::Call {
                    callee: Callee::Function(name),
                    args: vec![
                        CallArg {
                            value: v(2),
                            name: None,
                            spread: false,
                            span: Span::new(30, 32),
                        },
                        CallArg {
                            value: v(1),
                            name: Some("link".into()),
                            spread: true,
                            span: Span::new(34, 38),
                        },
                    ],
                    resolution: Resolution::Missing,
                },
            ),
            instr(
                (41, 50),
                None,
                TypeClass::Unknown,
                InstrKind::Store {
                    place: Place::Prop {
                        object: v(0),
                        name: MemberRef::Named("q".into()),
                        owners: vec![],
                    },
                    value: v(3),
                    weak: true,
                },
            ),
        ],
        term: Terminator::Branch {
            cond: v(3),
            then: b(1),
            otherwise: b(2),
        },
        handlers: vec![b(2)],
    };
    let ret = Block {
        instrs: vec![],
        term: Terminator::Return(Some(v(3))),
        handlers: vec![],
    };
    let exit = Block {
        instrs: vec![],
        term: Terminator::Exit,
        handlers: vec![],
    };
    Graph {
        key: UnitKey {
            span: Span::new(0, 60),
            name: "App\\Repo::find".into(),
        },
        kind: UnitKind::Method,
        owner: Some("App\\Repo".into()),
        is_static: false,
        params: vec![ParamInfo {
            local: LocalId::new(0),
            name: "id".into(),
            by_ref: true,
            variadic: false,
            span: Span::new(0, 3),
        }],
        captures: vec![Capture {
            local: LocalId::new(1),
            name: "db".into(),
            by_ref: false,
        }],
        locals: vec!["id".into(), "db".into()],
        blocks: vec![entry, ret, exit],
        entry: b(0),
        exit: b(2),
        value_count: 4,
        recovered: false,
        unknowns: BTreeMap::from([("variable-variable".into(), 2)]),
        limits: LimitsHit {
            depth: true,
            ..LimitsHit::default()
        },
    }
}

#[test]
fn a_hand_built_graph_has_a_stable_dump() {
    let expected = "\
unit App\\Repo::find kind=method span=0..60 owner=App\\Repo static=false recovered=false
params: l0 &$id
captures: l1 $db
limits: depth
b0 (entry):
  v0 = load local $id @0..4
  v1 = const str \"id=\" :data @5..9
  v2 = interp DoubleQuoted [\"id=\" v0@14..16] :data @10..20
  v3 = call fn mysqli_query(v2, link: ...v1) res=none @21..40
  store.weak prop v0->q = v3 @41..50
  handlers b2
  term branch v3 ? b1 : b2
b1:
  term return v3
b2 (exit):
  term exit
unknowns: variable-variable=2
";
    assert_eq!(dump(&sample()), expected);
}

#[test]
fn the_dump_can_carry_source_snippets() {
    let source = "x".repeat(80);
    let text = dump_with_source(&sample(), &source);
    assert!(text.contains("v0 = load local $id @0..4  ; xxxx"), "{text}");
    assert!(text.contains("..."), "long snippets are cut: {text}");
}

#[test]
fn successors_and_reverse_post_order_follow_edges_and_handlers() {
    let g = sample();
    assert_eq!(g.successors(b(0)), vec![b(1), b(2)]);
    assert_eq!(g.successors(b(1)), vec![b(2)], "a return reaches the exit");
    assert_eq!(g.successors(b(2)), vec![]);
    assert_eq!(g.reverse_post_order(), vec![b(0), b(1), b(2)]);
    assert_eq!(
        g.all_successors(b(0)),
        vec![b(1), b(2)],
        "handlers are not repeated"
    );
    assert_eq!(g.instruction_count(), 5);
    assert_eq!(g.unknown_count(), 2);
}

#[test]
fn operands_list_every_value_an_instruction_reads() {
    let g = sample();
    let call = &g.block(b(0)).instrs[3];
    assert_eq!(call.operands(), vec![v(2), v(1)]);
    let store = &g.block(b(0)).instrs[4];
    assert_eq!(store.operands(), vec![v(0), v(3)]);
}
