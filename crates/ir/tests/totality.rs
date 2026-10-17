//! Lowering accepts every tree the syntax model can represent: one hand-built node per expression
//! and statement kind, error nodes, and trees nested far deeper than the depth bound.

use bleedthrough_ir::{LowerConfig, UnitKind, dump, lower_file, lower_file_with};
use bleedthrough_symbols::ExprTag;
use bleedthrough_syntax::{
    Arg, ArgKind, ArrayItemKind, ArrayStyle, AssignOp, BinaryOp, CallTarget, CastKind, Catch,
    ClassLike, ClassLikeKind, ClassRefKind, ClosureUse, Directive, ElseIf, Expr, ExprKind,
    FunctionLike, IncludeKind, Literal, MagicConstantKind, MatchArm, MemberName, Name, NameKind,
    Node, Param, Span, StaticVar, Stmt, StmtKind, StringKind, StringPartKind, SwitchCase,
    SyntaxTree, TypeHint, TypeKind, UnaryOp, UseItem, UseKind,
};

fn sp() -> Span {
    Span::new(1, 2)
}

fn e(kind: ExprKind) -> Expr {
    Node::new(sp(), kind)
}

fn s(kind: StmtKind) -> Stmt {
    Node::new(sp(), kind)
}

fn var(name: &str) -> Expr {
    e(ExprKind::Variable(name.into()))
}

fn name(text: &str) -> Name {
    Name {
        span: sp(),
        kind: NameKind::Unqualified,
        text: text.into(),
    }
}

fn int(n: &str) -> Expr {
    e(ExprKind::Literal(Literal::Int { raw: n.into() }))
}

fn boxed(x: Expr) -> Box<Expr> {
    Box::new(x)
}

fn arg(x: Expr) -> Arg {
    Node::new(sp(), ArgKind::Positional(x))
}

fn param(n: &str) -> Param {
    Param {
        span: sp(),
        attributes: vec![],
        modifiers: vec![],
        type_hint: None,
        by_ref: false,
        variadic: false,
        name: n.into(),
        default: None,
        hooks: vec![],
    }
}

fn class(kind: ClassLikeKind) -> ClassLike {
    ClassLike {
        kind,
        name: Some("C".into()),
        attributes: vec![],
        modifiers: vec![],
        extends: vec![],
        implements: vec![],
        backing_type: None,
        members: vec![],
    }
}

fn function() -> FunctionLike {
    FunctionLike {
        name: "f".into(),
        attributes: vec![],
        modifiers: vec![],
        by_ref: false,
        params: vec![param("a")],
        return_type: None,
        body: Some(vec![s(StmtKind::Return(Some(var("a"))))]),
    }
}

/// One expression of every kind.
#[allow(clippy::too_many_lines, reason = "a fixture listing one node per kind")]
fn every_expression() -> Vec<Expr> {
    let call_fn = CallTarget::Function(boxed(e(ExprKind::Name(name("f")))));
    vec![
        int("1"),
        e(ExprKind::Interpolated {
            kind: StringKind::DoubleQuoted,
            parts: vec![
                Node::new(sp(), StringPartKind::Text { raw: "a".into() }),
                Node::new(sp(), StringPartKind::Interpolation(boxed(var("x")))),
            ],
        }),
        var("x"),
        e(ExprKind::VariableVariable(boxed(var("x")))),
        e(ExprKind::Name(name("Foo"))),
        e(ExprKind::ConstFetch(name("PHP_EOL"))),
        e(ExprKind::ClassRef(ClassRefKind::SelfRef)),
        e(ExprKind::MagicConstant(MagicConstantKind::Dir)),
        e(ExprKind::Array {
            style: ArrayStyle::Short,
            items: vec![
                Node::new(sp(), ArrayItemKind::Value(var("a"))),
                Node::new(
                    sp(),
                    ArrayItemKind::KeyValue {
                        key: int("1"),
                        value: var("b"),
                    },
                ),
                Node::new(sp(), ArrayItemKind::Spread(var("c"))),
                Node::new(sp(), ArrayItemKind::Missing),
            ],
        }),
        e(ExprKind::Index {
            array: boxed(var("a")),
            index: Some(boxed(int("0"))),
        }),
        e(ExprKind::Property {
            object: boxed(var("o")),
            member: MemberName::Identifier {
                name: "p".into(),
                span: sp(),
            },
            nullsafe: true,
        }),
        e(ExprKind::StaticProperty {
            class: boxed(e(ExprKind::Name(name("C")))),
            name: boxed(var("p")),
        }),
        e(ExprKind::ClassConst {
            class: boxed(e(ExprKind::Name(name("C")))),
            member: MemberName::Missing(sp()),
        }),
        e(ExprKind::Call {
            target: call_fn.clone(),
            args: vec![arg(var("a"))],
        }),
        e(ExprKind::CallableRef { target: call_fn }),
        e(ExprKind::New {
            class: boxed(e(ExprKind::Name(name("C")))),
            args: vec![],
        }),
        e(ExprKind::AnonymousClass {
            args: vec![],
            class: Box::new(class(ClassLikeKind::Class)),
        }),
        e(ExprKind::Unary {
            op: UnaryOp::PostInc,
            operand: boxed(var("i")),
        }),
        e(ExprKind::Cast {
            kind: CastKind::Int,
            operand: boxed(var("a")),
        }),
        e(ExprKind::Binary {
            op: BinaryOp::And,
            lhs: boxed(var("a")),
            rhs: boxed(var("b")),
        }),
        e(ExprKind::Assign {
            op: AssignOp::Concat,
            target: boxed(var("a")),
            value: boxed(var("b")),
        }),
        e(ExprKind::Conditional {
            condition: boxed(var("a")),
            then: None,
            otherwise: boxed(var("b")),
        }),
        e(ExprKind::Match {
            subject: boxed(var("a")),
            arms: vec![MatchArm {
                span: sp(),
                conditions: vec![int("1")],
                is_default: false,
                body: var("x"),
            }],
        }),
        e(ExprKind::Closure {
            attributes: vec![],
            is_static: false,
            by_ref: false,
            params: vec![param("p")],
            uses: vec![ClosureUse {
                span: sp(),
                name: "x".into(),
                by_ref: true,
            }],
            return_type: None,
            body: vec![s(StmtKind::Return(Some(var("x"))))],
        }),
        e(ExprKind::ArrowFn {
            attributes: vec![],
            is_static: false,
            by_ref: false,
            params: vec![param("p")],
            return_type: None,
            body: boxed(var("x")),
        }),
        e(ExprKind::Yield {
            key: Some(boxed(int("1"))),
            value: Some(boxed(var("v"))),
        }),
        e(ExprKind::YieldFrom(boxed(var("g")))),
        e(ExprKind::Throw(boxed(var("t")))),
        e(ExprKind::Clone(boxed(var("o")))),
        e(ExprKind::Print(boxed(var("s")))),
        e(ExprKind::Include {
            kind: IncludeKind::Require,
            path: boxed(var("p")),
        }),
        e(ExprKind::Eval(boxed(var("code")))),
        e(ExprKind::Exit {
            die: false,
            args: vec![arg(var("m"))],
        }),
        e(ExprKind::Isset(vec![var("a")])),
        e(ExprKind::Empty(boxed(var("a")))),
        e(ExprKind::Pipe {
            input: boxed(var("a")),
            callable: boxed(var("f")),
        }),
        e(ExprKind::Parenthesized(boxed(var("a")))),
        e(ExprKind::Error { diagnostic: None }),
        e(ExprKind::Unsupported {
            kind: "weird".into(),
        }),
    ]
}

/// One statement of every kind.
#[allow(clippy::too_many_lines, reason = "a fixture listing one node per kind")]
fn every_statement() -> Vec<Stmt> {
    let body = || vec![s(StmtKind::Expr(var("a")))];
    vec![
        s(StmtKind::OpenTag { short: false }),
        s(StmtKind::CloseTag),
        s(StmtKind::InlineHtml {
            shebang: false,
            raw: "<b>".into(),
        }),
        s(StmtKind::Namespace {
            name: Some(name("N")),
            braced: true,
            body: body(),
        }),
        s(StmtKind::Use {
            items: vec![UseItem {
                span: sp(),
                kind: UseKind::Class,
                name: name("A"),
                alias: None,
            }],
        }),
        s(StmtKind::ClassLike(Box::new(class(ClassLikeKind::Class)))),
        s(StmtKind::Function(Box::new(function()))),
        s(StmtKind::Const {
            attributes: vec![],
            items: vec![],
        }),
        s(StmtKind::Block(body())),
        s(StmtKind::Expr(var("a"))),
        s(StmtKind::Echo {
            values: vec![var("a"), var("b")],
            short_tag: false,
        }),
        s(StmtKind::Return(Some(var("a")))),
        s(StmtKind::If {
            condition: var("a"),
            then: body(),
            else_ifs: vec![ElseIf {
                span: sp(),
                condition: var("b"),
                body: body(),
            }],
            otherwise: Some(body()),
        }),
        s(StmtKind::While {
            condition: var("a"),
            body: body(),
        }),
        s(StmtKind::DoWhile {
            body: body(),
            condition: var("a"),
        }),
        s(StmtKind::For {
            init: vec![var("i")],
            condition: vec![var("c")],
            step: vec![var("i")],
            body: body(),
        }),
        s(StmtKind::Foreach {
            subject: var("a"),
            key: Some(var("k")),
            value: var("v"),
            body: body(),
        }),
        s(StmtKind::Switch {
            subject: var("a"),
            cases: vec![
                SwitchCase {
                    span: sp(),
                    test: Some(int("1")),
                    body: body(),
                },
                SwitchCase {
                    span: sp(),
                    test: None,
                    body: body(),
                },
            ],
        }),
        s(StmtKind::Break(Some(int("2")))),
        s(StmtKind::Continue(None)),
        s(StmtKind::Try {
            body: body(),
            catches: vec![Catch {
                span: sp(),
                types: TypeHint {
                    span: sp(),
                    kind: TypeKind::Named(name("E")),
                },
                variable: Some("e".into()),
                body: body(),
            }],
            finally: Some(body()),
        }),
        s(StmtKind::Goto("end".into())),
        s(StmtKind::Label("end".into())),
        s(StmtKind::Declare {
            directives: vec![Directive {
                span: sp(),
                name: "strict_types".into(),
                value: int("1"),
            }],
            body: Some(body()),
        }),
        s(StmtKind::Global(vec![var("g")])),
        s(StmtKind::StaticVars(vec![StaticVar {
            span: sp(),
            name: "n".into(),
            init: Some(int("0")),
        }])),
        s(StmtKind::Unset(vec![
            var("a"),
            e(ExprKind::Index {
                array: boxed(var("b")),
                index: None,
            }),
        ])),
        s(StmtKind::HaltCompiler),
        s(StmtKind::Noop),
        s(StmtKind::Error { diagnostic: None }),
        s(StmtKind::Unsupported {
            kind: "weird".into(),
        }),
    ]
}

fn tree(statements: Vec<Stmt>) -> SyntaxTree {
    SyntaxTree::new(statements, vec![], 100)
}

/// spec: ir-lowering/Lowering is total over the syntax model
#[test]
fn every_expression_kind_lowers() {
    let exprs = every_expression();
    let mut tags: Vec<&str> = exprs
        .iter()
        .map(|x| ExprTag::of(&x.kind).as_str())
        .collect();
    tags.sort_unstable();
    tags.dedup();
    assert_eq!(
        tags.len(),
        39,
        "the fixture must cover every expression kind: {tags:?}"
    );
    for expr in exprs {
        let label = ExprTag::of(&expr.kind).as_str();
        let t = tree(vec![s(StmtKind::Expr(expr))]);
        let graphs = lower_file(&t, &[]);
        assert!(!graphs.is_empty(), "{label}");
        assert_eq!(
            dump_all(&graphs),
            dump_all(&lower_file(&t, &[])),
            "{label} is deterministic"
        );
    }
}

/// spec: ir-lowering/Lowering is total over the syntax model
#[test]
fn every_statement_kind_lowers() {
    let stmts = every_statement();
    assert_eq!(stmts.len(), 31);
    for (index, stmt) in stmts.into_iter().enumerate() {
        let t = tree(vec![stmt]);
        let graphs = lower_file(&t, &[]);
        assert!(!graphs.is_empty(), "statement #{index}");
        assert_eq!(
            dump_all(&graphs),
            dump_all(&lower_file(&t, &[])),
            "statement #{index}"
        );
    }
}

fn dump_all(graphs: &[bleedthrough_ir::Graph]) -> String {
    graphs.iter().map(dump).collect::<Vec<_>>().join("\n")
}

/// spec: ir-lowering/Lowering is total over the syntax model
#[test]
fn an_unsupported_expression_becomes_a_counted_unknown_instruction_with_its_operands() {
    let t = tree(vec![s(StmtKind::Expr(e(ExprKind::VariableVariable(
        boxed(var("x")),
    ))))]);
    let graphs = lower_file(&t, &[]);
    let main = &graphs[0];
    assert_eq!(main.unknowns.get("variable-variable"), Some(&1));
    let text = dump(main);
    assert!(text.contains("unknown variable-variable [v0]"), "{text}");

    let t = tree(vec![s(StmtKind::Expr(e(ExprKind::Unsupported {
        kind: "weird".into(),
    })))]);
    let graphs = lower_file(&t, &[]);
    assert_eq!(graphs[0].unknowns.get("unsupported:weird"), Some(&1));
    assert!(
        !graphs[0].recovered,
        "an unsupported kind is not a parse error"
    );
}

/// spec: ir-lowering/Lowering is total over the syntax model
#[test]
fn an_error_node_marks_only_its_own_graph_recovered() {
    let t = tree(vec![
        s(StmtKind::Function(Box::new(function()))),
        s(StmtKind::Expr(e(ExprKind::Error { diagnostic: None }))),
    ]);
    let graphs = lower_file(&t, &[]);
    assert_eq!(graphs.len(), 2);
    let main = graphs
        .iter()
        .find(|g| g.kind == UnitKind::Main)
        .expect("main");
    let func = graphs
        .iter()
        .find(|g| g.kind == UnitKind::Function)
        .expect("function");
    assert!(main.recovered);
    assert!(!func.recovered, "the remaining graphs are unaffected");
    assert_eq!(main.unknowns.get("error"), Some(&1));

    let t = tree(vec![s(StmtKind::Error { diagnostic: None })]);
    assert!(lower_file(&t, &[])[0].recovered);
}

fn nested_parens(depth: usize) -> Expr {
    let mut x = var("seed");
    for _ in 0..depth {
        x = e(ExprKind::Unary {
            op: UnaryOp::Not,
            operand: boxed(x),
        });
    }
    x
}

/// spec: analysis-limits/Adversarial input terminates within bounded resources
#[test]
fn nesting_past_the_depth_bound_is_flattened_not_recursed() {
    let t = tree(vec![s(StmtKind::Expr(e(ExprKind::Assign {
        op: AssignOp::Assign,
        target: boxed(var("out")),
        value: boxed(nested_parens(300)),
    })))]);
    let config = LowerConfig {
        max_depth: 64,
        ..LowerConfig::default()
    };
    let graphs = lower_file_with(&t, &[], &config);
    let main = &graphs[0];
    assert!(main.limits.depth, "the depth bound was reached");
    assert_eq!(main.unknowns.get("depth-limit"), Some(&1));
    let text = dump(main);
    assert!(text.contains("limits: depth"), "{text}");
    assert!(
        text.contains("unknown depth-limit [v"),
        "the variables inside are kept as operands: {text}"
    );
    assert!(text.contains("load local $seed"), "{text}");
}

/// spec: analysis-limits/Adversarial input terminates within bounded resources
#[test]
fn a_long_concatenation_chain_keeps_every_operand() {
    let mut chain = var("v0");
    for i in 1..2000 {
        chain = e(ExprKind::Binary {
            op: BinaryOp::Concat,
            lhs: boxed(chain),
            rhs: boxed(var(&format!("v{i}"))),
        });
    }
    let t = tree(vec![s(StmtKind::Expr(e(ExprKind::Assign {
        op: AssignOp::Assign,
        target: boxed(var("out")),
        value: boxed(chain),
    })))]);
    // The chain is 2000 deep but its left spine is walked iteratively.
    let config = LowerConfig {
        max_depth: 8,
        ..LowerConfig::default()
    };
    let graphs = lower_file_with(&t, &[], &config);
    let main = &graphs[0];
    assert!(!main.limits.depth, "a flat chain needs no depth");
    let concat = main
        .blocks
        .iter()
        .flat_map(|b| &b.instrs)
        .find_map(|i| match &i.kind {
            bleedthrough_ir::InstrKind::Concat(parts) => Some(parts.len()),
            _ => None,
        })
        .expect("a concat instruction");
    assert_eq!(concat, 2000, "one concat over all operands");
}

/// spec: analysis-limits/Adversarial input terminates within bounded resources
#[test]
fn block_and_instruction_bounds_stop_lowering_conservatively() {
    let many: Vec<Stmt> = (0..200)
        .map(|_| {
            s(StmtKind::If {
                condition: var("c"),
                then: vec![s(StmtKind::Expr(var("a")))],
                else_ifs: vec![],
                otherwise: None,
            })
        })
        .collect();
    let t = tree(many);
    let config = LowerConfig {
        max_blocks: 50,
        max_instructions: 100,
        ..LowerConfig::default()
    };
    let main = &lower_file_with(&t, &[], &config)[0];
    assert!(main.limits.blocks);
    assert!(main.limits.instructions);
    assert!(main.blocks.len() <= 51);
    assert!(main.instruction_count() <= 100);
}
