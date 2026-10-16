//! Accessors, canonical text, the regular-expression subset and the loading port, through the
//! public API only. Written against the surviving mutants of `cargo xtask mutants`.

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

fn taint(sources: &[&str]) -> BodyDraft {
    BodyDraft::Taint(TaintDraft {
        sources: sources.iter().map(|s| (*s).into()).collect(),
        sinks: vec![SinkDraft {
            symbol: "mysqli_query".into(),
            args: vec!["2".into()],
            class: "sql".into(),
        }],
        sanitizers: vec![SanitizerDraft {
            symbol: "escapeshellarg".into(),
            clears: vec!["os-command".into(), "sql".into()],
        }],
        propagators: vec![PropagatorDraft {
            symbol: "PDO->quote".into(),
            from: vec!["this".into(), "1".into()],
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
        body: Some(taint(&["$_GET", "$_POST"])),
        ..RuleDraft::default()
    }
}

fn fields(errors: &[RuleError]) -> Vec<&str> {
    errors.iter().map(RuleError::field).collect()
}

/// spec: rule-schema/Every rule has complete metadata
#[test]
fn a_rule_exposes_everything_it_was_given() {
    let mut d = draft("BT0001");
    d.area = Some("injection".into());
    d.references = vec!["https://example.test/a".into(), "CWE-89".into()];
    d.paranoid = true;
    d.php_min = Some("8.1".into());
    d.php_max = Some("8.3".into());
    d.pack = Some("symfony".into());
    let rule = d.validate().unwrap();
    assert_eq!(rule.title(), "SQL injection");
    assert_eq!(rule.description(), "Tainted data reaches an SQL query.");
    assert_eq!(rule.remediation(), "Use prepared statements.");
    assert_eq!(rule.area(), Some("injection"));
    assert_eq!(rule.references(), ["https://example.test/a", "CWE-89"]);
    assert!(rule.paranoid());
    assert_eq!(rule.php_min().map(|v| (v.major(), v.minor())), Some((8, 1)));
    assert_eq!(rule.php_max().map(|v| (v.major(), v.minor())), Some((8, 3)));
    assert_eq!(rule.pack(), Some("symfony"));

    let bare = draft("BT0002").validate().unwrap();
    assert_eq!(bare.area(), None);
    assert!(bare.references().is_empty());
    assert!(!bare.paranoid());
    assert_eq!((bare.php_min(), bare.php_max()), (None, None));
    assert_eq!(bare.pack(), None);
}

#[test]
fn php_versions_need_plain_digits_and_may_be_equal() {
    for bad in [
        "+8.3", "8.+3", "8.", ".3", "8", "8.x", " 8.3", "8.3.1", "-8.3",
    ] {
        assert!(PhpVersion::parse(bad).is_err(), "{bad:?}");
    }
    let v = PhpVersion::parse("8.10").unwrap();
    assert_eq!((v.major(), v.minor()), (8, 10));
    assert_eq!(v.to_string(), "8.10");

    let mut d = draft("BT0001");
    (d.php_min, d.php_max) = (Some("8.2".into()), Some("8.2".into()));
    assert!(
        d.validate().is_ok(),
        "an equal minimum and maximum is one version"
    );
}

#[test]
fn a_taint_body_exposes_its_parts() {
    let rule = draft("BT0001").validate().unwrap();
    let RuleBody::Taint(t) = rule.body() else {
        panic!("a taint body")
    };
    assert_eq!(t.sources().len(), 2);
    assert_eq!(t.propagators().len(), 1);
    assert_eq!(
        t.propagators()[0].from(),
        [ArgSel::Position(1), ArgSel::Receiver]
    );
    assert_eq!(t.sanitizers()[0].clears().len(), 2);
    assert!(t.sanitizers()[0].clears().contains(&SinkClass::Sql));
}

#[test]
fn sinks_need_a_callable_symbol_and_only_propagators_may_name_the_receiver() {
    let sink = |symbol: &str, args: &[&str]| {
        let mut d = draft("BT0001");
        d.body = Some(BodyDraft::Taint(TaintDraft {
            sources: vec!["$_GET".into()],
            sinks: vec![SinkDraft {
                symbol: symbol.into(),
                args: args.iter().map(|a| (*a).into()).collect(),
                class: "sql".into(),
            }],
            ..TaintDraft::default()
        }));
        d.validate()
    };
    assert!(sink("PDO->query", &["1"]).is_ok());
    assert!(sink("new PDO", &["1"]).is_ok());
    assert!(sink("PDO::query", &["1"]).is_ok());
    assert!(
        sink("$_GET", &["1"]).is_err(),
        "a superglobal is not callable"
    );
    assert!(
        sink("class PDO", &["1"]).is_err(),
        "a class is not callable"
    );
    let errors = sink("PDO->query", &["this"]).unwrap_err();
    assert_eq!(fields(&errors), ["body.sinks[0].args[0]"]);
}

/// spec: rule-schema/Rules are tagged or rejected
#[test]
fn one_bad_tag_is_one_error() {
    let mut d = draft("BT0001");
    d.cwe = vec!["nonsense".into()];
    assert_eq!(fields(&d.validate().unwrap_err()), ["cwe[0]"]);

    let mut d = draft("BT0001");
    d.owasp = vec!["nonsense".into()];
    assert_eq!(fields(&d.validate().unwrap_err()), ["owasp[0]"]);
}

#[test]
fn an_extra_category_needs_a_reason() {
    let mut d = draft("BT0001");
    d.owasp_extra = vec![ExtraCategoryDraft {
        category: "OWASP:2021:A02".into(),
        reason: String::new(),
    }];
    assert!(d.validate().is_err());
    d.owasp_extra[0].reason = "leaks secrets".into();
    assert!(d.validate().is_ok());
}

/// spec: rule-schema/Loading is deterministic and hashable
#[test]
fn canonical_text_is_stable_under_ordering_and_names_every_part() {
    let a = {
        let mut d = draft("BT0001");
        d.body = Some(taint(&["$_GET", "$_POST"]));
        d.validate().unwrap()
    };
    let b = {
        let mut d = draft("BT0001");
        d.body = Some(taint(&["$_POST", "$_GET"]));
        d.validate().unwrap()
    };
    assert_eq!(a.canonical(), b.canonical());
    let text = a.canonical();
    for needle in [
        "body=taint",
        "source=$_GET",
        "source=$_POST",
        "sink=mysqli_query args=2 class=sql",
        "sanitizer=escapeshellarg clears=",
        "propagator=PDO->quote from=",
    ] {
        assert!(text.contains(needle), "{needle} in\n{text}");
    }
    let sources: Vec<_> = text.lines().filter(|l| l.starts_with("source=")).collect();
    assert_eq!(sources, ["source=$_GET", "source=$_POST"]);
}

#[test]
fn canonical_text_of_a_pattern_rule_names_its_shape() {
    let mut d = draft("BT0002");
    d.cwe = vec!["CWE-95".into()];
    d.body = Some(BodyDraft::Pattern(PatternDraft {
        shape: "call".into(),
        symbol: Some("eval".into()),
        args: vec![ArgConditionDraft {
            arg: "1".into(),
            test: "non-literal".into(),
            value: None,
        }],
    }));
    let text = d.validate().unwrap().canonical();
    assert!(text.contains("body=pattern"), "{text}");
    assert!(text.contains("shape=call eval"), "{text}");
    assert!(text.contains("arg="), "{text}");
}

/// spec: rule-schema/Pattern rules are simple and bounded
#[test]
fn the_regular_expression_subset() {
    for ok in [
        "abc",
        "^a.c$",
        "[a-z0-9_]",
        "[^a-z]",
        "[]a]",
        "[^]a]",
        "\\d+",
        "a{2}",
        "a{1,3}",
        "a*?",
        "[\\]]x",
    ] {
        assert_eq!(validate_regex(ok), Ok(()), "{ok}");
    }
    for bad in [
        "", "(a)", "a|b", "*a", "a**", "a{1,}", "a{3,1}", "a{1001}", "a{x}", "a{", "[a", "[]",
        "[^]", "[a[b]", "]", "}", "\\1", "\\b", "a\\", "a{2}{3}", "^*", "a{,3}",
    ] {
        assert!(validate_regex(bad).is_err(), "{bad:?}");
    }
    let unclosed = validate_regex("x[ab").unwrap_err();
    assert!(
        unclosed.contains("not closed") && unclosed.contains('1'),
        "{unclosed}"
    );
    let nested = validate_regex("[a[b]").unwrap_err();
    assert!(nested.contains("inside a class"), "{nested}");
}

#[derive(Default)]
struct Fixed(SourceOutput);

impl RuleSource for Fixed {
    fn load(&self) -> SourceOutput {
        self.0.clone()
    }
}

fn located(draft: RuleDraft, file: &str, line: u32, keys: &[(&str, u32)]) -> LocatedRule {
    LocatedRule {
        draft,
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

/// spec: rule-schema/Unknown keys and bad values are errors
#[test]
fn a_located_error_exposes_file_line_key_and_message() {
    let e = LocatedError::new("rules/a.toml", Some(4), Some("title".into()), "missing");
    assert_eq!(e.file(), "rules/a.toml");
    assert_eq!(e.line(), Some(4));
    assert_eq!(e.key(), Some("title"));
    assert_eq!(e.message(), "missing");
    assert_eq!(e.to_string(), "rules/a.toml:4: title: missing");
    let bare = LocatedError::new("rules/a.toml", None, None, "bad");
    assert_eq!((bare.line(), bare.key()), (None, None));
}

#[test]
fn a_rule_error_is_located_at_the_key_and_names_the_rule() {
    let mut d = draft("BT0001");
    d.remediation = None;
    let source = Fixed(SourceOutput {
        rules: vec![located(
            d,
            "rules/a.toml",
            3,
            &[("title", 4), ("remediation", 9)],
        )],
        ..SourceOutput::default()
    });
    let errors = load_rule_set(&source).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].file(), "rules/a.toml");
    assert_eq!(errors[0].line(), Some(9));
    assert_eq!(errors[0].key(), Some("remediation"));
    assert!(errors[0].message().contains("BT0001"), "{}", errors[0]);
}

#[test]
fn an_error_the_source_already_reported_is_not_repeated_but_others_are() {
    let mut d = draft("BT0001");
    d.remediation = None;
    let rule = located(d, "rules/a.toml", 3, &[("remediation", 9)]);
    let reported = |file: &str, line: u32| {
        Fixed(SourceOutput {
            rules: vec![rule.clone()],
            errors: vec![LocatedError::new(file, Some(line), None, "wrong type")],
            ..SourceOutput::default()
        })
    };
    // same file and line: the source's own error stands alone
    assert_eq!(
        load_rule_set(&reported("rules/a.toml", 9))
            .unwrap_err()
            .len(),
        1
    );
    // another line of the same file, or the same line of another file: both are reported
    assert_eq!(
        load_rule_set(&reported("rules/a.toml", 8))
            .unwrap_err()
            .len(),
        2
    );
    assert_eq!(
        load_rule_set(&reported("rules/b.toml", 9))
            .unwrap_err()
            .len(),
        2
    );
}

#[test]
fn a_source_error_fails_the_load_even_when_every_rule_is_valid() {
    let source = Fixed(SourceOutput {
        rules: vec![located(draft("BT0001"), "rules/a.toml", 1, &[])],
        errors: vec![LocatedError::new("rules/b.toml", None, None, "unreadable")],
        ..SourceOutput::default()
    });
    let errors = load_rule_set(&source).unwrap_err();
    assert_eq!(errors[0].file(), "rules/b.toml");

    let clean = Fixed(SourceOutput {
        rules: vec![located(draft("BT0001"), "rules/a.toml", 1, &[])],
        ..SourceOutput::default()
    });
    assert_eq!(load_rule_set(&clean).unwrap().len(), 1);
}

#[test]
fn duplicate_and_retired_identifiers_are_located_at_the_rule() {
    let source = Fixed(SourceOutput {
        rules: vec![
            located(draft("BT0001"), "rules/a.toml", 1, &[]),
            located(draft("BT0001"), "rules/b.toml", 7, &[]),
            located(draft("BT0003"), "rules/c.toml", 2, &[]),
        ],
        retired: vec!["BT0003".into(), "nonsense".into()],
        ..SourceOutput::default()
    });
    let errors = load_rule_set(&source).unwrap_err();
    let places: Vec<_> = errors.iter().map(|e| (e.file(), e.line())).collect();
    assert!(places.contains(&("rules/b.toml", Some(7))), "{places:?}");
    assert!(places.contains(&("rules/c.toml", Some(2))), "{places:?}");
    assert!(places.contains(&("rules/RETIRED.md", None)), "{places:?}");
}

#[test]
fn a_rule_set_can_be_looked_up_and_may_be_empty() {
    let empty = RuleSet::new(vec![], &[]).unwrap();
    assert!(empty.is_empty());
    assert_eq!(empty.len(), 0);

    let rule = draft("BT0001").validate().unwrap();
    let set = RuleSet::new(vec![rule.clone()], &[]).unwrap();
    assert!(!set.is_empty());
    let id = RuleId::new("BT0001").unwrap();
    assert_eq!(set.get(&id), Some(&rule));
    assert_eq!(set.get(&RuleId::new("BT0009").unwrap()), None);
}

#[test]
fn a_rule_set_error_carries_the_origin_of_the_rule() {
    let rule = |file: &str, line: u32| {
        draft("BT0001").validate().unwrap().with_origin(Origin {
            file: file.into(),
            line,
        })
    };
    let errors = RuleSet::new(vec![rule("a.toml", 1), rule("b.toml", 5)], &[]).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(
        errors[0].origin().map(|o| (o.file.as_str(), o.line)),
        Some(("b.toml", 5))
    );
}
