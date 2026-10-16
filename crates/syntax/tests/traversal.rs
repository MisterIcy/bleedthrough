//! Child order of every node kind and the kind names and labels used by dumps, on trees built by
//! hand. The shared conformance suites need a parser; these tests need none, so a mutation run of
//! this crate alone is judged by them.

use bleedthrough_syntax::*;

fn sp(n: u32) -> Span {
    Span::new(n, n + 1)
}

fn v(n: u32) -> Expr {
    Node::new(sp(n), ExprKind::Variable(format!("v{n}").into()))
}

fn bv(n: u32) -> Box<Expr> {
    Box::new(v(n))
}

fn ex(kind: ExprKind) -> Expr {
    Node::new(sp(0), kind)
}

fn st(n: u32, kind: StmtKind) -> Stmt {
    Node::new(sp(n), kind)
}

fn stmt(kind: StmtKind) -> Stmt {
    st(0, kind)
}

fn echo(n: u32) -> Stmt {
    st(
        n,
        StmtKind::Echo {
            values: vec![],
            short_tag: false,
        },
    )
}

fn name(n: u32) -> Name {
    Name::from_written(sp(n), &format!("N{n}"))
}

fn ty(n: u32) -> TypeHint {
    Node::new(sp(n), TypeKind::Builtin(BuiltinType::Int))
}

fn arg(n: u32) -> Arg {
    Node::new(sp(n), ArgKind::Positional(v(n)))
}

fn attr(n: u32) -> Attribute {
    Attribute {
        span: sp(n),
        name: name(n),
        args: vec![],
    }
}

fn param(n: u32) -> Param {
    Param {
        span: sp(n),
        attributes: vec![],
        modifiers: vec![],
        type_hint: None,
        by_ref: false,
        variadic: false,
        name: format!("p{n}").into(),
        default: None,
        hooks: vec![],
    }
}

fn hook(n: u32) -> PropertyHook {
    PropertyHook {
        span: sp(n),
        attributes: vec![],
        modifiers: vec![],
        by_ref: false,
        name: "get".into(),
        params: vec![],
        body: HookBody::Abstract,
    }
}

fn const_item(n: u32) -> ConstItem {
    ConstItem {
        span: sp(n),
        name: "C".into(),
        value: v(n),
    }
}

fn function(name: &str) -> FunctionLike {
    FunctionLike {
        name: name.into(),
        attributes: vec![],
        modifiers: vec![],
        by_ref: false,
        params: vec![],
        return_type: None,
        body: None,
    }
}

fn class(kind: ClassLikeKind) -> ClassLike {
    ClassLike {
        kind,
        name: None,
        attributes: vec![],
        modifiers: vec![],
        extends: vec![],
        implements: vec![],
        backing_type: None,
        members: vec![],
    }
}

fn member(n: u32) -> Member {
    Node::new(
        sp(n),
        MemberKind::Unsupported {
            kind: "member".into(),
        },
    )
}

fn starts(node: NodeRef<'_>) -> Vec<u32> {
    node.children().iter().map(|c| c.span().start).collect()
}

fn of_expr(kind: ExprKind) -> Vec<u32> {
    starts(NodeRef::Expr(&ex(kind)))
}

fn of_stmt(kind: StmtKind) -> Vec<u32> {
    starts(NodeRef::Stmt(&stmt(kind)))
}

fn of_member(kind: MemberKind) -> Vec<u32> {
    starts(NodeRef::Member(&Node::new(sp(0), kind)))
}

fn of_type(kind: TypeKind) -> Vec<u32> {
    starts(NodeRef::Type(&Node::new(sp(0), kind)))
}

fn ident(text: &str) -> MemberName {
    MemberName::Identifier {
        name: text.into(),
        span: sp(0),
    }
}

fn dynamic(n: u32) -> MemberName {
    MemberName::Dynamic(bv(n))
}

// ---- children of expressions ----

/// spec: syntax-model/Spans are byte offsets with consistent nesting
#[test]
fn leaf_expressions_have_no_children() {
    for kind in [
        ExprKind::Literal(Literal::Null),
        ExprKind::Variable("x".into()),
        ExprKind::ClassRef(ClassRefKind::SelfRef),
        ExprKind::MagicConstant(MagicConstantKind::Line),
        ExprKind::Error { diagnostic: None },
        ExprKind::Unsupported { kind: "k".into() },
    ] {
        assert_eq!(of_expr(kind), Vec::<u32>::new());
    }
}

#[test]
fn names_strings_and_arrays_list_their_parts() {
    assert_eq!(of_expr(ExprKind::Name(name(1))), [1]);
    assert_eq!(of_expr(ExprKind::ConstFetch(name(1))), [1]);
    let text = Node::new(sp(1), StringPartKind::Text { raw: "t".into() });
    let interpolation = Node::new(sp(2), StringPartKind::Interpolation(bv(3)));
    assert_eq!(
        of_expr(ExprKind::Interpolated {
            kind: StringKind::DoubleQuoted,
            parts: vec![text, interpolation],
        }),
        [1, 2]
    );
    let items = vec![
        Node::new(sp(1), ArrayItemKind::Value(v(1))),
        Node::new(sp(2), ArrayItemKind::Missing),
    ];
    assert_eq!(
        of_expr(ExprKind::Array {
            style: ArrayStyle::Short,
            items
        }),
        [1, 2]
    );
}

#[test]
fn accesses_list_their_operands_and_dynamic_member_names() {
    let index = |index| ExprKind::Index {
        array: bv(1),
        index,
    };
    assert_eq!(of_expr(index(Some(bv(2)))), [1, 2]);
    assert_eq!(of_expr(index(None)), [1]);

    let property = |member| ExprKind::Property {
        object: bv(1),
        member,
        nullsafe: false,
    };
    assert_eq!(of_expr(property(dynamic(2))), [1, 2]);
    assert_eq!(of_expr(property(ident("p"))), [1]);
    assert_eq!(of_expr(property(MemberName::Missing(sp(0)))), [1]);

    assert_eq!(
        of_expr(ExprKind::StaticProperty {
            class: bv(1),
            name: bv(2)
        }),
        [1, 2]
    );
    let constant = |member| ExprKind::ClassConst {
        class: bv(1),
        member,
    };
    assert_eq!(of_expr(constant(dynamic(2))), [1, 2]);
    assert_eq!(of_expr(constant(ident("C"))), [1]);
}

#[test]
fn calls_list_target_then_arguments() {
    let call = |target| ExprKind::Call {
        target,
        args: vec![arg(8), arg(9)],
    };
    assert_eq!(of_expr(call(CallTarget::Function(bv(1)))), [1, 8, 9]);
    let method = |member| CallTarget::Method {
        object: bv(1),
        member,
        nullsafe: false,
    };
    assert_eq!(of_expr(call(method(dynamic(2)))), [1, 2, 8, 9]);
    assert_eq!(of_expr(call(method(ident("m")))), [1, 8, 9]);
    let statik = |member| CallTarget::Static {
        class: bv(1),
        member,
    };
    assert_eq!(of_expr(call(statik(dynamic(2)))), [1, 2, 8, 9]);
    assert_eq!(of_expr(call(statik(ident("m")))), [1, 8, 9]);

    let reference = |target| ExprKind::CallableRef { target };
    assert_eq!(of_expr(reference(CallTarget::Function(bv(1)))), [1]);
    assert_eq!(of_expr(reference(method(dynamic(2)))), [1, 2]);
    assert_eq!(of_expr(reference(statik(dynamic(2)))), [1, 2]);
}

#[test]
fn instantiations_list_class_and_arguments() {
    assert_eq!(
        of_expr(ExprKind::New {
            class: bv(1),
            args: vec![arg(2)]
        }),
        [1, 2]
    );
    let mut anonymous = class(ClassLikeKind::Class);
    anonymous.attributes = vec![attr(2)];
    anonymous.members = vec![member(3)];
    assert_eq!(
        of_expr(ExprKind::AnonymousClass {
            args: vec![arg(1)],
            class: Box::new(anonymous)
        }),
        [1, 2, 3]
    );
}

#[test]
fn operators_list_their_operands_in_order() {
    let unary = ExprKind::Unary {
        op: UnaryOp::Not,
        operand: bv(1),
    };
    assert_eq!(of_expr(unary), [1]);
    let cast = ExprKind::Cast {
        kind: CastKind::Int,
        operand: bv(1),
    };
    assert_eq!(of_expr(cast), [1]);
    let binary = ExprKind::Binary {
        op: BinaryOp::Add,
        lhs: bv(1),
        rhs: bv(2),
    };
    assert_eq!(of_expr(binary), [1, 2]);
    let assign = ExprKind::Assign {
        op: AssignOp::Assign,
        target: bv(1),
        value: bv(2),
    };
    assert_eq!(of_expr(assign), [1, 2]);
    let conditional = |then| ExprKind::Conditional {
        condition: bv(1),
        then,
        otherwise: bv(3),
    };
    assert_eq!(of_expr(conditional(Some(bv(2)))), [1, 2, 3]);
    assert_eq!(of_expr(conditional(None)), [1, 3]);
    assert_eq!(
        of_expr(ExprKind::Pipe {
            input: bv(1),
            callable: bv(2)
        }),
        [1, 2]
    );
}

#[test]
fn match_closures_and_arrow_functions_list_their_parts() {
    let arm = MatchArm {
        span: sp(2),
        conditions: vec![],
        is_default: true,
        body: v(9),
    };
    assert_eq!(
        of_expr(ExprKind::Match {
            subject: bv(1),
            arms: vec![arm]
        }),
        [1, 2]
    );
    let closure = ExprKind::Closure {
        attributes: vec![attr(1)],
        is_static: false,
        by_ref: false,
        params: vec![param(2)],
        uses: vec![ClosureUse {
            span: sp(3),
            name: "u".into(),
            by_ref: false,
        }],
        return_type: Some(Box::new(ty(4))),
        body: vec![echo(5)],
    };
    assert_eq!(of_expr(closure), [1, 2, 3, 4, 5]);
    let arrow = |return_type| ExprKind::ArrowFn {
        attributes: vec![attr(1)],
        is_static: false,
        by_ref: false,
        params: vec![param(2)],
        return_type,
        body: bv(4),
    };
    assert_eq!(of_expr(arrow(Some(Box::new(ty(3))))), [1, 2, 3, 4]);
    assert_eq!(of_expr(arrow(None)), [1, 2, 4]);
}

#[test]
fn single_operand_expressions_list_their_operand() {
    for kind in [
        ExprKind::YieldFrom(bv(1)),
        ExprKind::Throw(bv(1)),
        ExprKind::Clone(bv(1)),
        ExprKind::Print(bv(1)),
        ExprKind::Eval(bv(1)),
        ExprKind::Empty(bv(1)),
        ExprKind::Parenthesized(bv(1)),
        ExprKind::VariableVariable(bv(1)),
        ExprKind::Include {
            kind: IncludeKind::Require,
            path: bv(1),
        },
    ] {
        assert_eq!(of_expr(kind.clone()), [1], "{kind:?}");
    }
    let y = |key, value| ExprKind::Yield { key, value };
    assert_eq!(of_expr(y(Some(bv(1)), Some(bv(2)))), [1, 2]);
    assert_eq!(of_expr(y(None, Some(bv(2)))), [2]);
    assert_eq!(of_expr(y(Some(bv(1)), None)), [1]);
    assert_eq!(
        of_expr(ExprKind::Exit {
            die: false,
            args: vec![arg(1), arg(2)]
        }),
        [1, 2]
    );
    assert_eq!(of_expr(ExprKind::Isset(vec![v(1), v(2)])), [1, 2]);
}

// ---- children of statements ----

#[test]
fn leaf_statements_have_no_children() {
    for kind in [
        StmtKind::OpenTag { short: false },
        StmtKind::CloseTag,
        StmtKind::InlineHtml {
            shebang: false,
            raw: "x".into(),
        },
        StmtKind::Goto("l".into()),
        StmtKind::Label("l".into()),
        StmtKind::HaltCompiler,
        StmtKind::Noop,
        StmtKind::Error { diagnostic: None },
        StmtKind::Unsupported { kind: "k".into() },
        StmtKind::Return(None),
        StmtKind::Break(None),
        StmtKind::Continue(None),
    ] {
        assert_eq!(of_stmt(kind), Vec::<u32>::new());
    }
}

#[test]
fn simple_statements_list_their_expressions() {
    assert_eq!(of_stmt(StmtKind::Expr(v(1))), [1]);
    for kind in [
        StmtKind::Return(Some(v(1))),
        StmtKind::Break(Some(v(1))),
        StmtKind::Continue(Some(v(1))),
    ] {
        assert_eq!(of_stmt(kind), [1]);
    }
    assert_eq!(
        of_stmt(StmtKind::Echo {
            values: vec![v(1), v(2)],
            short_tag: false
        }),
        [1, 2]
    );
    assert_eq!(of_stmt(StmtKind::Global(vec![v(1), v(2)])), [1, 2]);
    assert_eq!(of_stmt(StmtKind::Unset(vec![v(1), v(2)])), [1, 2]);
    assert_eq!(of_stmt(StmtKind::Block(vec![echo(1), echo(2)])), [1, 2]);
    let vars = vec![
        StaticVar {
            span: sp(1),
            name: "a".into(),
            init: None,
        },
        StaticVar {
            span: sp(2),
            name: "b".into(),
            init: None,
        },
    ];
    assert_eq!(of_stmt(StmtKind::StaticVars(vars)), [1, 2]);
    let item = |n| UseItem {
        span: sp(n),
        kind: UseKind::Class,
        name: name(n + 10),
        alias: None,
    };
    assert_eq!(
        of_stmt(StmtKind::Use {
            items: vec![item(1), item(2)]
        }),
        [1, 2]
    );
}

#[test]
fn declarations_list_attributes_types_and_bodies() {
    let mut c = class(ClassLikeKind::Enum);
    c.attributes = vec![attr(1)];
    c.backing_type = Some(ty(2));
    c.extends = vec![name(3)];
    c.implements = vec![name(4)];
    c.members = vec![member(5), member(6)];
    assert_eq!(
        of_stmt(StmtKind::ClassLike(Box::new(c))),
        [1, 2, 3, 4, 5, 6]
    );

    let mut f = function("f");
    f.attributes = vec![attr(1)];
    f.params = vec![param(2)];
    f.return_type = Some(ty(3));
    f.body = Some(vec![echo(4)]);
    assert_eq!(of_stmt(StmtKind::Function(Box::new(f))), [1, 2, 3, 4]);
    assert_eq!(
        of_stmt(StmtKind::Function(Box::new(function("g")))),
        Vec::<u32>::new()
    );

    assert_eq!(
        of_stmt(StmtKind::Const {
            attributes: vec![attr(1)],
            items: vec![const_item(2)]
        }),
        [1, 2]
    );
}

#[test]
fn namespaces_and_conditionals_list_their_parts_in_order() {
    let namespace = |name| StmtKind::Namespace {
        name,
        braced: true,
        body: vec![echo(2), echo(3)],
    };
    assert_eq!(of_stmt(namespace(Some(name(1)))), [1, 2, 3]);
    assert_eq!(of_stmt(namespace(None)), [2, 3]);

    let else_if = ElseIf {
        span: sp(3),
        condition: v(9),
        body: vec![],
    };
    let cond = |otherwise| StmtKind::If {
        condition: v(1),
        then: vec![echo(2)],
        else_ifs: vec![else_if.clone()],
        otherwise,
    };
    assert_eq!(of_stmt(cond(Some(vec![echo(4)]))), [1, 2, 3, 4]);
    assert_eq!(of_stmt(cond(None)), [1, 2, 3]);

    let switch = StmtKind::Switch {
        subject: v(1),
        cases: vec![
            SwitchCase {
                span: sp(2),
                test: None,
                body: vec![],
            },
            SwitchCase {
                span: sp(3),
                test: None,
                body: vec![],
            },
        ],
    };
    assert_eq!(of_stmt(switch), [1, 2, 3]);
}

#[test]
fn loops_list_their_parts_in_source_order() {
    assert_eq!(
        of_stmt(StmtKind::While {
            condition: v(1),
            body: vec![echo(2)]
        }),
        [1, 2]
    );
    assert_eq!(
        of_stmt(StmtKind::DoWhile {
            body: vec![echo(1)],
            condition: v(2)
        }),
        [1, 2]
    );
    assert_eq!(
        of_stmt(StmtKind::For {
            init: vec![v(1)],
            condition: vec![v(2)],
            step: vec![v(3)],
            body: vec![echo(4)]
        }),
        [1, 2, 3, 4]
    );
    let foreach = |key| StmtKind::Foreach {
        subject: v(1),
        key,
        value: v(3),
        body: vec![echo(4)],
    };
    assert_eq!(of_stmt(foreach(Some(v(2)))), [1, 2, 3, 4]);
    assert_eq!(of_stmt(foreach(None)), [1, 3, 4]);
}

#[test]
fn try_and_declare_list_their_parts() {
    let catch = Catch {
        span: sp(2),
        types: ty(9),
        variable: None,
        body: vec![],
    };
    let tr = |finally| StmtKind::Try {
        body: vec![echo(1)],
        catches: vec![catch.clone()],
        finally,
    };
    assert_eq!(of_stmt(tr(Some(vec![echo(3)]))), [1, 2, 3]);
    assert_eq!(of_stmt(tr(None)), [1, 2]);

    let directive = Directive {
        span: sp(1),
        name: "strict_types".into(),
        value: v(9),
    };
    let declare = |body| StmtKind::Declare {
        directives: vec![directive.clone()],
        body,
    };
    assert_eq!(of_stmt(declare(Some(vec![echo(2)]))), [1, 2]);
    assert_eq!(of_stmt(declare(None)), [1]);
}

// ---- children of members, types and the small nodes ----

#[test]
fn members_list_their_parts() {
    assert_eq!(
        of_member(MemberKind::TraitUse {
            traits: vec![name(1)],
            adaptations: vec![Node::new(
                sp(2),
                TraitAdaptationKind::Alias {
                    trait_name: None,
                    method: "m".into(),
                    visibility: None,
                    alias: None
                }
            )]
        }),
        [1, 2]
    );
    assert_eq!(
        of_member(MemberKind::Const {
            attributes: vec![attr(1)],
            modifiers: vec![],
            type_hint: Some(ty(2)),
            items: vec![const_item(3)]
        }),
        [1, 2, 3]
    );
    assert_eq!(
        of_member(MemberKind::Property {
            attributes: vec![attr(1)],
            modifiers: vec![],
            type_hint: Some(ty(2)),
            items: vec![PropertyItem {
                span: sp(3),
                name: "p".into(),
                default: None
            }],
            hooks: vec![hook(4)]
        }),
        [1, 2, 3, 4]
    );
    let mut method = function("m");
    method.params = vec![param(1)];
    assert_eq!(of_member(MemberKind::Method(Box::new(method))), [1]);
    let case = |value| MemberKind::EnumCase {
        attributes: vec![attr(1)],
        name: "A".into(),
        value,
    };
    assert_eq!(of_member(case(Some(v(2)))), [1, 2]);
    assert_eq!(of_member(case(None)), [1]);
    assert_eq!(
        of_member(MemberKind::Error { diagnostic: None }),
        Vec::<u32>::new()
    );
}

#[test]
fn types_list_their_parts() {
    assert_eq!(of_type(TypeKind::Named(name(1))), [1]);
    assert_eq!(of_type(TypeKind::Nullable(Box::new(ty(1)))), [1]);
    assert_eq!(of_type(TypeKind::Parenthesized(Box::new(ty(1)))), [1]);
    assert_eq!(of_type(TypeKind::Union(vec![ty(1), ty(2)])), [1, 2]);
    assert_eq!(of_type(TypeKind::Intersection(vec![ty(1), ty(2)])), [1, 2]);
    assert_eq!(
        of_type(TypeKind::Builtin(BuiltinType::Int)),
        Vec::<u32>::new()
    );
}

#[test]
fn arguments_array_items_and_string_parts_list_their_expression() {
    for kind in [
        ArgKind::Positional(v(1)),
        ArgKind::Spread(v(1)),
        ArgKind::Named {
            name: "n".into(),
            value: v(1),
        },
    ] {
        assert_eq!(starts(NodeRef::Arg(&Node::new(sp(0), kind))), [1]);
    }
    for kind in [ArrayItemKind::Value(v(1)), ArrayItemKind::Spread(v(1))] {
        assert_eq!(starts(NodeRef::ArrayItem(&Node::new(sp(0), kind))), [1]);
    }
    let pair = ArrayItemKind::KeyValue {
        key: v(1),
        value: v(2),
    };
    assert_eq!(starts(NodeRef::ArrayItem(&Node::new(sp(0), pair))), [1, 2]);
    let missing = Node::new(sp(0), ArrayItemKind::Missing);
    assert!(missing_has_no_children(&missing));

    let text = Node::new(sp(0), StringPartKind::Text { raw: "t".into() });
    assert_eq!(starts(NodeRef::StringPart(&text)), Vec::<u32>::new());
    let part = Node::new(sp(0), StringPartKind::Interpolation(bv(1)));
    assert_eq!(starts(NodeRef::StringPart(&part)), [1]);
}

fn missing_has_no_children(item: &ArrayItem) -> bool {
    NodeRef::ArrayItem(item).children().is_empty()
}

#[test]
fn match_arms_params_attributes_and_use_items_list_their_parts() {
    let arm = MatchArm {
        span: sp(0),
        conditions: vec![v(1), v(2)],
        is_default: false,
        body: v(3),
    };
    assert_eq!(starts(NodeRef::MatchArm(&arm)), [1, 2, 3]);

    let mut p = param(0);
    p.attributes = vec![attr(1)];
    p.type_hint = Some(ty(2));
    p.default = Some(v(3));
    p.hooks = vec![hook(4)];
    assert_eq!(starts(NodeRef::Param(&p)), [1, 2, 3, 4]);

    let mut a = attr(0);
    a.name = name(1);
    a.args = vec![arg(2)];
    assert_eq!(starts(NodeRef::Attribute(&a)), [1, 2]);

    let u = UseItem {
        span: sp(0),
        kind: UseKind::Class,
        name: name(1),
        alias: None,
    };
    assert_eq!(starts(NodeRef::UseItem(&u)), [1]);
}

#[test]
fn clauses_and_items_list_their_parts() {
    let else_if = ElseIf {
        span: sp(0),
        condition: v(1),
        body: vec![echo(2)],
    };
    assert_eq!(starts(NodeRef::ElseIf(&else_if)), [1, 2]);

    let case = |test| SwitchCase {
        span: sp(0),
        test,
        body: vec![echo(2)],
    };
    assert_eq!(starts(NodeRef::SwitchCase(&case(Some(v(1))))), [1, 2]);
    assert_eq!(starts(NodeRef::SwitchCase(&case(None))), [2]);

    let catch = Catch {
        span: sp(0),
        types: ty(1),
        variable: None,
        body: vec![echo(2)],
    };
    assert_eq!(starts(NodeRef::Catch(&catch)), [1, 2]);

    let var = |init| StaticVar {
        span: sp(0),
        name: "s".into(),
        init,
    };
    assert_eq!(starts(NodeRef::StaticVar(&var(Some(v(1))))), [1]);
    assert_eq!(starts(NodeRef::StaticVar(&var(None))), Vec::<u32>::new());

    let directive = Directive {
        span: sp(0),
        name: "d".into(),
        value: v(1),
    };
    assert_eq!(starts(NodeRef::Directive(&directive)), [1]);

    let mut c = const_item(0);
    c.value = v(1);
    assert_eq!(starts(NodeRef::ConstItem(&c)), [1]);

    let property = |default| PropertyItem {
        span: sp(0),
        name: "p".into(),
        default,
    };
    assert_eq!(starts(NodeRef::PropertyItem(&property(Some(v(1))))), [1]);
    assert_eq!(
        starts(NodeRef::PropertyItem(&property(None))),
        Vec::<u32>::new()
    );

    let use_by = ClosureUse {
        span: sp(0),
        name: "u".into(),
        by_ref: false,
    };
    assert_eq!(starts(NodeRef::ClosureUse(&use_by)), Vec::<u32>::new());
    assert_eq!(starts(NodeRef::Name(&name(1))), Vec::<u32>::new());
}

#[test]
fn hooks_and_trait_adaptations_list_their_parts() {
    let with = |body| {
        let mut h = hook(0);
        h.attributes = vec![attr(1)];
        h.params = vec![param(2)];
        h.body = body;
        h
    };
    assert_eq!(starts(NodeRef::Hook(&with(HookBody::Abstract))), [1, 2]);
    assert_eq!(
        starts(NodeRef::Hook(&with(HookBody::Block(vec![echo(3)])))),
        [1, 2, 3]
    );
    assert_eq!(
        starts(NodeRef::Hook(&with(HookBody::Expr(bv(3))))),
        [1, 2, 3]
    );

    let precedence = Node::new(
        sp(0),
        TraitAdaptationKind::Precedence {
            trait_name: name(1),
            method: "m".into(),
            instead_of: vec![name(2), name(3)],
        },
    );
    assert_eq!(starts(NodeRef::TraitAdaptation(&precedence)), [1, 2, 3]);
    let alias = |trait_name| {
        Node::new(
            sp(0),
            TraitAdaptationKind::Alias {
                trait_name,
                method: "m".into(),
                visibility: None,
                alias: None,
            },
        )
    };
    assert_eq!(starts(NodeRef::TraitAdaptation(&alias(Some(name(1))))), [1]);
    assert_eq!(
        starts(NodeRef::TraitAdaptation(&alias(None))),
        Vec::<u32>::new()
    );
}

#[test]
fn walking_visits_every_node_before_its_children_and_can_skip() {
    let tree = SyntaxTree::new(
        vec![stmt(StmtKind::Expr(ex(ExprKind::Binary {
            op: BinaryOp::Add,
            lhs: bv(1),
            rhs: bv(2),
        })))],
        vec![],
        10,
    );
    let mut seen = Vec::new();
    visit_tree(&tree, &mut |n| seen.push(n.kind_name()));
    assert_eq!(seen, ["Expr", "Binary", "Variable", "Variable"]);

    struct SkipBinary(Vec<&'static str>);
    impl Visitor for SkipBinary {
        fn enter(&mut self, node: NodeRef<'_>) -> bool {
            self.0.push(node.kind_name());
            node.kind_name() != "Binary"
        }
    }
    let mut skip = SkipBinary(Vec::new());
    walk(NodeRef::Stmt(&tree.statements()[0]), &mut skip);
    assert_eq!(skip.0, ["Expr", "Binary"]);
    assert_eq!(tree.hole_counts(), (0, 0));
}

#[test]
fn child_lists_are_the_statement_and_member_runs() {
    let mut c = class(ClassLikeKind::Class);
    c.members = vec![member(1), member(2)];
    let class_stmt = stmt(StmtKind::ClassLike(Box::new(c)));
    let lists = NodeRef::Stmt(&class_stmt).child_lists();
    assert_eq!(lists.len(), 1);
    assert_eq!(lists[0].len(), 2);

    let block = stmt(StmtKind::Block(vec![echo(1), echo(2), echo(3)]));
    let lists = NodeRef::Stmt(&block).child_lists();
    assert_eq!(lists.iter().map(Vec::len).collect::<Vec<_>>(), [3]);
}

#[test]
fn holes_are_counted_and_recognised() {
    let tree = SyntaxTree::new(
        vec![
            stmt(StmtKind::Unsupported { kind: "k".into() }),
            stmt(StmtKind::Error { diagnostic: None }),
            stmt(StmtKind::Expr(ex(ExprKind::Error { diagnostic: None }))),
        ],
        vec![],
        3,
    );
    assert_eq!(tree.hole_counts(), (1, 2));
    let ty_unsupported: TypeHint = Node::new(sp(0), TypeKind::Unsupported { kind: "k".into() });
    assert!(NodeRef::Type(&ty_unsupported).is_unsupported());
    assert!(!NodeRef::Type(&ty_unsupported).is_error());
    let member_error: Member = Node::new(sp(0), MemberKind::Error { diagnostic: None });
    assert!(NodeRef::Member(&member_error).is_error());
    assert!(!NodeRef::Member(&member_error).is_unsupported());
}

// ---- kind names and labels ----

fn expr_label(kind: ExprKind) -> Option<String> {
    NodeRef::Expr(&ex(kind)).label()
}

fn stmt_label(kind: StmtKind) -> Option<String> {
    NodeRef::Stmt(&stmt(kind)).label()
}

fn member_label(kind: MemberKind) -> Option<String> {
    NodeRef::Member(&Node::new(sp(0), kind)).label()
}

#[test]
fn every_node_family_names_its_kind() {
    assert_eq!(NodeRef::Expr(&v(1)).kind_name(), "Variable");
    assert_eq!(
        NodeRef::Expr(&ex(ExprKind::Literal(Literal::Null))).kind_name(),
        "Literal"
    );
    assert_eq!(NodeRef::Stmt(&echo(1)).kind_name(), "Echo");
    assert_eq!(NodeRef::Stmt(&stmt(StmtKind::Noop)).kind_name(), "Noop");
    assert_eq!(NodeRef::Type(&ty(1)).kind_name(), "Builtin");
    assert_eq!(NodeRef::Member(&member(1)).kind_name(), "Unsupported");
    let small = [
        (NodeRef::Arg(&arg(1)).kind_name(), "Arg"),
        (NodeRef::Param(&param(1)).kind_name(), "Param"),
        (NodeRef::Attribute(&attr(1)).kind_name(), "Attribute"),
        (NodeRef::Name(&name(1)).kind_name(), "Name"),
        (NodeRef::Hook(&hook(1)).kind_name(), "Hook"),
        (NodeRef::ConstItem(&const_item(1)).kind_name(), "ConstItem"),
    ];
    for (actual, expected) in small {
        assert_eq!(actual, expected);
    }
}

#[test]
fn literals_are_labelled_with_their_raw_text() {
    let label = |l| expr_label(ExprKind::Literal(l));
    assert_eq!(
        label(Literal::Int { raw: "42".into() }).as_deref(),
        Some("int \"42\"")
    );
    assert_eq!(
        label(Literal::Float { raw: "1.5".into() }).as_deref(),
        Some("float \"1.5\"")
    );
    assert_eq!(
        label(Literal::String {
            raw: "a".into(),
            quote: QuoteKind::Single
        })
        .as_deref(),
        Some("string Single \"a\"")
    );
    assert_eq!(label(Literal::Bool(true)).as_deref(), Some("bool true"));
    assert_eq!(label(Literal::Null).as_deref(), Some("null"));
}

#[test]
fn expressions_are_labelled_with_their_scalar_attributes() {
    let l = |kind| expr_label(kind);
    assert_eq!(
        l(ExprKind::Interpolated {
            kind: StringKind::Heredoc,
            parts: vec![]
        })
        .as_deref(),
        Some("Heredoc")
    );
    assert_eq!(l(ExprKind::Variable("x".into())).as_deref(), Some("\"x\""));
    assert_eq!(
        l(ExprKind::Name(name(1))).as_deref(),
        Some("Unqualified \"N1\"")
    );
    assert_eq!(
        l(ExprKind::ConstFetch(name(2))).as_deref(),
        Some("Unqualified \"N2\"")
    );
    assert_eq!(
        l(ExprKind::ClassRef(ClassRefKind::Parent)).as_deref(),
        Some("Parent")
    );
    assert_eq!(
        l(ExprKind::MagicConstant(MagicConstantKind::Dir)).as_deref(),
        Some("Dir")
    );
    assert_eq!(
        l(ExprKind::Array {
            style: ArrayStyle::Long,
            items: vec![]
        })
        .as_deref(),
        Some("Long")
    );
    let property = |member, nullsafe| ExprKind::Property {
        object: bv(1),
        member,
        nullsafe,
    };
    assert_eq!(
        l(property(ident("p"), true)).as_deref(),
        Some("\"p\" nullsafe=true")
    );
    assert_eq!(
        l(property(dynamic(2), false)).as_deref(),
        Some("dynamic nullsafe=false")
    );
    assert_eq!(
        l(property(MemberName::Missing(sp(0)), false)).as_deref(),
        Some("missing nullsafe=false")
    );
    assert_eq!(
        l(ExprKind::ClassConst {
            class: bv(1),
            member: ident("C")
        })
        .as_deref(),
        Some("\"C\"")
    );
    assert_eq!(
        l(ExprKind::Index {
            array: bv(1),
            index: None
        }),
        None
    );
    assert_eq!(l(ExprKind::Isset(vec![])), None);
}

#[test]
fn calls_are_labelled_with_their_target() {
    let call = |target| ExprKind::Call {
        target,
        args: vec![],
    };
    assert_eq!(
        expr_label(call(CallTarget::Function(bv(1)))).as_deref(),
        Some("function")
    );
    assert_eq!(
        expr_label(call(CallTarget::Method {
            object: bv(1),
            member: ident("m"),
            nullsafe: true
        }))
        .as_deref(),
        Some("method \"m\" nullsafe=true")
    );
    assert_eq!(
        expr_label(ExprKind::CallableRef {
            target: CallTarget::Static {
                class: bv(1),
                member: ident("s")
            }
        })
        .as_deref(),
        Some("static \"s\"")
    );
}

#[test]
fn operators_closures_and_constructs_are_labelled() {
    assert_eq!(
        expr_label(ExprKind::Unary {
            op: UnaryOp::Not,
            operand: bv(1)
        })
        .as_deref(),
        Some("Not")
    );
    assert_eq!(
        expr_label(ExprKind::Cast {
            kind: CastKind::Int,
            operand: bv(1)
        })
        .as_deref(),
        Some("Int")
    );
    assert_eq!(
        expr_label(ExprKind::Binary {
            op: BinaryOp::Add,
            lhs: bv(1),
            rhs: bv(2)
        })
        .as_deref(),
        Some("Add")
    );
    assert_eq!(
        expr_label(ExprKind::Assign {
            op: AssignOp::Concat,
            target: bv(1),
            value: bv(2)
        })
        .as_deref(),
        Some("Concat")
    );
    assert_eq!(
        expr_label(ExprKind::Closure {
            attributes: vec![],
            is_static: true,
            by_ref: false,
            params: vec![],
            uses: vec![],
            return_type: None,
            body: vec![]
        })
        .as_deref(),
        Some("static=true by_ref=false")
    );
    assert_eq!(
        expr_label(ExprKind::ArrowFn {
            attributes: vec![],
            is_static: false,
            by_ref: true,
            params: vec![],
            return_type: None,
            body: bv(1)
        })
        .as_deref(),
        Some("static=false by_ref=true")
    );
    assert_eq!(
        expr_label(ExprKind::Include {
            kind: IncludeKind::Require,
            path: bv(1)
        })
        .as_deref(),
        Some("Require")
    );
    assert_eq!(
        expr_label(ExprKind::Exit {
            die: true,
            args: vec![]
        })
        .as_deref(),
        Some("die=true")
    );
    assert_eq!(
        expr_label(ExprKind::Unsupported { kind: "x".into() }).as_deref(),
        Some("\"x\"")
    );
}

#[test]
fn statements_are_labelled_with_their_scalar_attributes() {
    assert_eq!(
        stmt_label(StmtKind::OpenTag { short: true }).as_deref(),
        Some("short=true")
    );
    assert_eq!(
        stmt_label(StmtKind::InlineHtml {
            shebang: false,
            raw: "<b>".into()
        })
        .as_deref(),
        Some("shebang=false \"<b>\"")
    );
    let namespace = |name| StmtKind::Namespace {
        name,
        braced: true,
        body: vec![],
    };
    assert_eq!(
        stmt_label(namespace(Some(Name::from_written(sp(0), "App")))).as_deref(),
        Some("\"App\" braced=true")
    );
    assert_eq!(
        stmt_label(namespace(None)).as_deref(),
        Some("none braced=true")
    );

    let mut c = class(ClassLikeKind::Class);
    c.name = Some("Foo".into());
    c.modifiers = vec![Modifier::Final];
    assert_eq!(
        stmt_label(StmtKind::ClassLike(Box::new(c))).as_deref(),
        Some("Class \"Foo\" modifiers=[Final]")
    );
    assert_eq!(
        stmt_label(StmtKind::ClassLike(Box::new(class(ClassLikeKind::Trait)))).as_deref(),
        Some("Trait \"\" modifiers=[]")
    );
    let mut f = function("f");
    f.by_ref = true;
    assert_eq!(
        stmt_label(StmtKind::Function(Box::new(f))).as_deref(),
        Some("\"f\" by_ref=true modifiers=[]")
    );
    assert_eq!(
        stmt_label(StmtKind::Echo {
            values: vec![],
            short_tag: true
        })
        .as_deref(),
        Some("short_tag=true")
    );
    assert_eq!(
        stmt_label(StmtKind::Goto("l".into())).as_deref(),
        Some("\"l\"")
    );
    assert_eq!(
        stmt_label(StmtKind::Label("m".into())).as_deref(),
        Some("\"m\"")
    );
    assert_eq!(
        stmt_label(StmtKind::Unsupported { kind: "k".into() }).as_deref(),
        Some("\"k\"")
    );
    assert_eq!(stmt_label(StmtKind::Noop), None);
}

#[test]
fn members_are_labelled_with_their_scalar_attributes() {
    assert_eq!(
        member_label(MemberKind::Const {
            attributes: vec![],
            modifiers: vec![Modifier::Public],
            type_hint: None,
            items: vec![]
        })
        .as_deref(),
        Some("modifiers=[Public]")
    );
    assert_eq!(
        member_label(MemberKind::Property {
            attributes: vec![],
            modifiers: vec![Modifier::Static],
            type_hint: None,
            items: vec![],
            hooks: vec![]
        })
        .as_deref(),
        Some("modifiers=[Static]")
    );
    let mut m = function("m");
    m.by_ref = true;
    m.modifiers = vec![Modifier::Static];
    assert_eq!(
        member_label(MemberKind::Method(Box::new(m))).as_deref(),
        Some("\"m\" by_ref=true modifiers=[Static]")
    );
    assert_eq!(
        member_label(MemberKind::EnumCase {
            attributes: vec![],
            name: "A".into(),
            value: None
        })
        .as_deref(),
        Some("\"A\"")
    );
    assert_eq!(
        member_label(MemberKind::Unsupported { kind: "k".into() }).as_deref(),
        Some("\"k\"")
    );
    assert_eq!(
        member_label(MemberKind::TraitUse {
            traits: vec![],
            adaptations: vec![]
        }),
        None
    );
}

#[test]
fn the_small_nodes_are_labelled_with_their_scalar_attributes() {
    let t = |kind| NodeRef::Type(&Node::new(sp(0), kind)).label();
    assert_eq!(
        t(TypeKind::Builtin(BuiltinType::Int)).as_deref(),
        Some("Int")
    );
    assert_eq!(
        t(TypeKind::Unsupported { kind: "k".into() }).as_deref(),
        Some("\"k\"")
    );
    assert_eq!(t(TypeKind::Named(name(1))), None);

    let a = |kind| NodeRef::Arg(&Node::new(sp(0), kind)).label();
    assert_eq!(
        a(ArgKind::Named {
            name: "n".into(),
            value: v(1)
        })
        .as_deref(),
        Some("named \"n\"")
    );
    assert_eq!(a(ArgKind::Spread(v(1))).as_deref(), Some("spread"));
    assert_eq!(a(ArgKind::Positional(v(1))), None);

    let i = |kind| NodeRef::ArrayItem(&Node::new(sp(0), kind)).label();
    assert_eq!(i(ArrayItemKind::Spread(v(1))).as_deref(), Some("spread"));
    assert_eq!(i(ArrayItemKind::Missing).as_deref(), Some("missing"));
    assert_eq!(i(ArrayItemKind::Value(v(1))), None);

    let p = |kind| NodeRef::StringPart(&Node::new(sp(0), kind)).label();
    assert_eq!(
        p(StringPartKind::Text { raw: "t".into() }).as_deref(),
        Some("text \"t\"")
    );
    assert_eq!(p(StringPartKind::Interpolation(bv(1))), None);

    let arm = MatchArm {
        span: sp(0),
        conditions: vec![],
        is_default: true,
        body: v(1),
    };
    assert_eq!(
        NodeRef::MatchArm(&arm).label().as_deref(),
        Some("default=true")
    );
}

#[test]
fn parameters_names_and_use_items_are_labelled() {
    let mut param = param(0);
    param.name = "p".into();
    param.by_ref = true;
    param.modifiers = vec![Modifier::Readonly];
    assert_eq!(
        NodeRef::Param(&param).label().as_deref(),
        Some("\"p\" by_ref=true variadic=false modifiers=[Readonly]")
    );
    let fq = Name::from_written(sp(0), "\\A");
    assert_eq!(
        NodeRef::Name(&fq).label().as_deref(),
        Some("FullyQualified \"A\"")
    );
    let use_item = UseItem {
        span: sp(0),
        kind: UseKind::Function,
        name: name(1),
        alias: Some("x".into()),
    };
    assert_eq!(
        NodeRef::UseItem(&use_item).label().as_deref(),
        Some("Function alias=Some(\"x\")")
    );
    let catch = Catch {
        span: sp(0),
        types: ty(1),
        variable: Some("e".into()),
        body: vec![],
    };
    assert_eq!(
        NodeRef::Catch(&catch).label().as_deref(),
        Some("variable=Some(\"e\")")
    );
}

#[test]
fn clauses_and_items_are_labelled() {
    let var = StaticVar {
        span: sp(0),
        name: "s".into(),
        init: None,
    };
    assert_eq!(NodeRef::StaticVar(&var).label().as_deref(), Some("\"s\""));
    let directive = Directive {
        span: sp(0),
        name: "d".into(),
        value: v(1),
    };
    assert_eq!(
        NodeRef::Directive(&directive).label().as_deref(),
        Some("\"d\"")
    );
    assert_eq!(
        NodeRef::ConstItem(&const_item(0)).label().as_deref(),
        Some("\"C\"")
    );
    let property = PropertyItem {
        span: sp(0),
        name: "p".into(),
        default: None,
    };
    assert_eq!(
        NodeRef::PropertyItem(&property).label().as_deref(),
        Some("\"p\"")
    );
    let capture = ClosureUse {
        span: sp(0),
        name: "u".into(),
        by_ref: true,
    };
    assert_eq!(
        NodeRef::ClosureUse(&capture).label().as_deref(),
        Some("\"u\" by_ref=true")
    );
    assert_eq!(NodeRef::Attribute(&attr(1)).label(), None);
}

#[test]
fn hooks_and_trait_adaptations_are_labelled() {
    let with = |body| {
        let mut h = hook(0);
        h.body = body;
        h
    };
    assert_eq!(
        NodeRef::Hook(&with(HookBody::Abstract)).label().as_deref(),
        Some("\"get\" by_ref=false modifiers=[] abstract")
    );
    assert_eq!(
        NodeRef::Hook(&with(HookBody::Block(vec![])))
            .label()
            .as_deref(),
        Some("\"get\" by_ref=false modifiers=[] block")
    );
    assert_eq!(
        NodeRef::Hook(&with(HookBody::Expr(bv(1))))
            .label()
            .as_deref(),
        Some("\"get\" by_ref=false modifiers=[] expr")
    );
    let precedence = Node::new(
        sp(0),
        TraitAdaptationKind::Precedence {
            trait_name: name(1),
            method: "m".into(),
            instead_of: vec![],
        },
    );
    assert_eq!(
        NodeRef::TraitAdaptation(&precedence).label().as_deref(),
        Some("insteadof \"m\"")
    );
    let alias = Node::new(
        sp(0),
        TraitAdaptationKind::Alias {
            trait_name: None,
            method: "m".into(),
            visibility: Some(Modifier::Public),
            alias: Some("a".into()),
        },
    );
    assert_eq!(
        NodeRef::TraitAdaptation(&alias).label().as_deref(),
        Some("alias \"m\" Some(Public) Some(\"a\")")
    );
}
