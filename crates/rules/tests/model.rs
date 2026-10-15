//! The rule model through its public API: validation, sets and the loading port.

use std::collections::BTreeMap;

use bleedthrough_rules::*;

fn metrics() -> MetricsDraft {
    let w = |s: &str| Some(s.to_string());
    MetricsDraft {
        attack_vector: w("network"),
        attack_complexity: w("low"),
        privileges_required: w("none"),
        user_interaction: w("none"),
        scope: w("unchanged"),
        confidentiality: w("high"),
        integrity: w("high"),
        availability: w("high"),
    }
}

fn sqli_body() -> BodyDraft {
    BodyDraft::Taint(TaintDraft {
        sources: vec!["$_GET".into(), "$_POST".into()],
        sinks: vec![SinkDraft {
            symbol: "mysqli_query".into(),
            args: vec!["2".into()],
            class: "sql".into(),
        }],
        sanitizers: vec![SanitizerDraft {
            symbol: "escapeshellarg".into(),
            clears: vec!["os-command".into()],
        }],
        propagators: vec![PropagatorDraft {
            symbol: "trim".into(),
            from: vec!["1".into()],
        }],
    })
}

fn draft(id: &str) -> RuleDraft {
    RuleDraft {
        id: Some(id.into()),
        title: Some("SQL injection".into()),
        description: Some("Tainted data reaches an SQL query.".into()),
        remediation: Some("Use prepared statements.".into()),
        cwe: vec!["CWE-89".into()],
        owasp: vec!["OWASP:2021:A03".into()],
        metrics: metrics(),
        body: Some(sqli_body()),
        ..RuleDraft::default()
    }
}

fn pattern(shape: &str, symbol: Option<&str>) -> RuleDraft {
    let mut d = draft("BT0002");
    d.cwe = vec!["CWE-95".into()];
    d.body = Some(BodyDraft::Pattern(PatternDraft {
        shape: shape.into(),
        symbol: symbol.map(Into::into),
        args: vec![],
    }));
    d
}

fn fields(errors: &[RuleError]) -> Vec<&str> {
    errors.iter().map(RuleError::field).collect()
}

#[test]
fn a_complete_rule_validates_and_derives_its_severity() {
    let rule = draft("BT0001").validate().unwrap();
    assert_eq!(rule.id().as_str(), "BT0001");
    assert_eq!(rule.severity().score_text().unwrap(), "9.8");
    assert!(matches!(rule.body(), RuleBody::Taint(_)));
}

#[test]
fn missing_remediation_names_the_rule_and_the_field() {
    let mut d = draft("BT0001");
    d.remediation = None;
    let errors = d.validate().unwrap_err();
    assert_eq!(fields(&errors), ["remediation"]);
    assert_eq!(errors[0].rule(), Some("BT0001"));
}

#[test]
fn a_bad_identifier_is_named() {
    let errors = draft("XX1").validate().unwrap_err();
    assert_eq!(fields(&errors), ["id"]);
    assert!(errors[0].message().contains("XX1"));
}

#[test]
fn an_untagged_rule_is_rejected() {
    let mut d = draft("BT0001");
    (d.cwe, d.owasp) = (vec![], vec![]);
    let errors = d.validate().unwrap_err();
    assert_eq!(fields(&errors), ["cwe", "owasp"]);
}

#[test]
fn unregistered_and_inconsistent_tags_are_rejected() {
    let mut d = draft("BT0001");
    d.cwe = vec!["CWE-79".into()];
    assert!(d.validate().unwrap_err()[0].message().contains("CWE-79"));
    let mut d = draft("BT0001");
    d.owasp = vec!["OWASP:2021:A02".into()];
    assert!(d.validate().unwrap_err()[0].message().contains("A02"));
    d.owasp_extra = vec![ExtraCategoryDraft {
        category: "OWASP:2021:A03".into(),
        reason: "".into(),
    }];
    assert!(d.validate().is_err());
    d.owasp = vec!["OWASP:2021:A03".into()];
    d.owasp_extra = vec![ExtraCategoryDraft {
        category: "OWASP:2021:A02".into(),
        reason: "leaks secrets".into(),
    }];
    assert!(d.validate().is_ok());
}

#[test]
fn metrics_are_all_required_and_valid() {
    let mut d = draft("BT0001");
    d.metrics.scope = None;
    d.metrics.attack_vector = Some("remote".into());
    let errors = d.validate().unwrap_err();
    assert_eq!(fields(&errors), ["metrics.attack_vector", "metrics.scope"]);
    assert!(errors[0].message().contains("remote"));
}

#[test]
fn php_versions_are_ordered() {
    let mut d = draft("BT0001");
    (d.php_min, d.php_max) = (Some("8.2".into()), Some("8.0".into()));
    assert_eq!(fields(&d.validate().unwrap_err()), ["php_min"]);
}

#[test]
fn every_problem_is_reported() {
    let mut d = draft("BT0001");
    d.title = None;
    d.remediation = Some("  ".into());
    d.body = None;
    assert_eq!(
        fields(&d.validate().unwrap_err()),
        ["title", "remediation", "body"]
    );
}

#[test]
fn only_the_declared_argument_position_is_vulnerable_and_sanitizers_are_per_class() {
    let rule = draft("BT0001").validate().unwrap();
    let RuleBody::Taint(t) = rule.body() else {
        panic!()
    };
    assert_eq!(t.sinks()[0].vulnerable_args(), [ArgSel::Position(2)]);
    assert_eq!(t.sinks()[0].class(), SinkClass::Sql);
    let s = &t.sanitizers()[0];
    assert!(s.clears_class(SinkClass::OsCommand));
    assert!(!s.clears_class(SinkClass::Sql));
}

#[test]
fn taint_body_problems_are_located() {
    let mut d = draft("BT0001");
    d.body = Some(BodyDraft::Taint(TaintDraft {
        sources: vec![],
        sinks: vec![SinkDraft {
            symbol: "f".into(),
            args: vec![],
            class: "xss".into(),
        }],
        sanitizers: vec![SanitizerDraft {
            symbol: "g".into(),
            clears: vec![],
        }],
        propagators: vec![PropagatorDraft {
            symbol: "h".into(),
            from: vec!["this".into()],
        }],
    }));
    let errors = d.validate().unwrap_err();
    assert_eq!(
        fields(&errors),
        [
            "body.sources",
            "body.sinks[0].args",
            "body.sinks[0].class",
            "body.sanitizers[0].clears",
            "body.propagators[0].from[0]",
        ]
    );
    assert!(errors[2].message().contains("\"xss\""));
}

#[test]
fn banned_function_and_non_literal_include_are_valid_patterns() {
    let rule = pattern("use", Some("eval")).validate().unwrap();
    let RuleBody::Pattern(p) = rule.body() else {
        panic!()
    };
    assert!(matches!(p.shape(), PatternShape::Use { .. }));
    assert!(pattern("include-non-literal", None).validate().is_ok());
}

#[test]
fn unsupported_shapes_and_missing_symbols_are_rejected() {
    let errors = pattern("expression", None).validate().unwrap_err();
    assert_eq!(fields(&errors), ["body.shape"]);
    assert!(errors[0].message().contains("\"expression\""));
    assert_eq!(
        fields(&pattern("call", None).validate().unwrap_err()),
        ["body.symbol"]
    );
}

#[test]
fn argument_conditions_and_the_regex_subset() {
    let call = |test: &str, value: Option<&str>| {
        let mut d = pattern("call", Some("setcookie"));
        d.body = Some(BodyDraft::Pattern(PatternDraft {
            shape: "call".into(),
            symbol: Some("setcookie".into()),
            args: vec![ArgConditionDraft {
                arg: "1".into(),
                test: test.into(),
                value: value.map(Into::into),
            }],
        }));
        d
    };
    assert!(call("non-literal", None).validate().is_ok());
    assert!(call("matches", Some("^[a-z]{1,8}$")).validate().is_ok());
    assert!(call("equals", Some("x")).validate().is_ok());
    assert_eq!(
        fields(&call("equals", None).validate().unwrap_err()),
        ["body.args[0].value"]
    );
    let rejected = call("matches", Some("(a+)+")).validate().unwrap_err();
    assert_eq!(fields(&rejected), ["body.args[0].value"]);
    assert_eq!(
        fields(&call("sometimes", None).validate().unwrap_err()),
        ["body.args[0].test"]
    );
}

fn at(rule: Rule, file: &str, line: u32) -> Rule {
    rule.with_origin(Origin {
        file: file.into(),
        line,
    })
}

#[test]
fn duplicate_identifiers_name_both_locations() {
    let a = at(draft("BT0001").validate().unwrap(), "a.toml", 3);
    let b = at(draft("BT0001").validate().unwrap(), "b.toml", 9);
    let errors = RuleSet::new(vec![a, b], &[]).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message().contains("a.toml:3") && errors[0].message().contains("b.toml:9"));
}

#[test]
fn the_canonical_form_ignores_insertion_order_and_origin() {
    let make = |ids: &[&str], file: &str| {
        let rules = ids
            .iter()
            .map(|i| at(draft(i).validate().unwrap(), file, 1))
            .collect();
        RuleSet::new(rules, &[]).unwrap()
    };
    let forward = make(&["BT0001", "BT0002", "BT0003"], "x.toml");
    let shuffled = make(&["BT0003", "BT0001", "BT0002"], "elsewhere/y.toml");
    assert_eq!(forward.canonical(), shuffled.canonical());
    let order: Vec<_> = shuffled
        .iter()
        .map(|r| r.id().as_str().to_string())
        .collect();
    assert_eq!(order, ["BT0001", "BT0002", "BT0003"]);
    let mut changed = draft("BT0003");
    changed.title = Some("Something else".into());
    let other = RuleSet::new(
        vec![
            at(draft("BT0001").validate().unwrap(), "x", 1),
            at(draft("BT0002").validate().unwrap(), "x", 1),
            at(changed.validate().unwrap(), "x", 1),
        ],
        &[],
    )
    .unwrap();
    assert_ne!(forward.canonical(), other.canonical());
}

#[test]
fn declaration_order_inside_a_rule_does_not_change_the_canonical_form() {
    let mut reversed = draft("BT0001");
    if let Some(BodyDraft::Taint(t)) = &mut reversed.body {
        t.sources.reverse();
    }
    assert_eq!(
        draft("BT0001").validate().unwrap().canonical(),
        reversed.validate().unwrap().canonical()
    );
}

#[test]
fn a_retired_identifier_cannot_be_reissued() {
    let retired = [RuleId::new("BT0001").unwrap()];
    let rule = at(draft("BT0001").validate().unwrap(), "a.toml", 1);
    let errors = RuleSet::new(vec![rule], &retired).unwrap_err();
    assert!(errors[0].message().contains("retired"));
}

struct Stub(SourceOutput);

impl RuleSource for Stub {
    fn load(&self) -> SourceOutput {
        self.0.clone()
    }
}

fn located(d: RuleDraft, file: &str, line: u32, keys: &[(&str, u32)]) -> LocatedRule {
    LocatedRule {
        draft: d,
        origin: Origin {
            file: file.into(),
            line,
        },
        lines: keys
            .iter()
            .map(|(k, l)| ((*k).to_string(), *l))
            .collect::<BTreeMap<_, _>>(),
    }
}

#[test]
fn a_stub_source_drives_validation_end_to_end() {
    let ok = Stub(SourceOutput {
        rules: vec![
            located(draft("BT0002"), "f.toml", 10, &[]),
            located(draft("BT0001"), "f.toml", 1, &[]),
        ],
        ..SourceOutput::default()
    });
    let set = load_rule_set(&ok).unwrap();
    assert_eq!(set.len(), 2);

    let mut bad = draft("BT0003");
    bad.remediation = None;
    bad.metrics.scope = None;
    let source = Stub(SourceOutput {
        rules: vec![
            located(bad, "g.toml", 20, &[("metrics.scope", 27)]),
            located(draft("BT0001"), "g.toml", 1, &[]),
        ],
        retired: vec!["BT0001".into()],
        errors: vec![LocatedError::new(
            "h.toml",
            Some(4),
            Some("remedation".into()),
            "unknown key",
        )],
    });
    let errors = load_rule_set(&source).unwrap_err();
    let rendered: Vec<String> = errors.iter().map(ToString::to_string).collect();
    assert_eq!(rendered[0], "h.toml:4: remedation: unknown key");
    assert!(
        rendered
            .iter()
            .any(|e| e.starts_with("g.toml:27: metrics.scope:"))
    );
    assert!(
        rendered
            .iter()
            .any(|e| e.starts_with("g.toml:20: remediation:"))
    );
    assert!(rendered.iter().any(|e| e.contains("retired")));
}
