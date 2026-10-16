//! The merge: per-version extractions become records with version masks.
//!
//! Two steps. Within one version, every declaration of a symbol is combined into one record
//! (same-signature declarations from several SAPIs or guard branches merge; real conflicts need a
//! reviewed entry). Across versions, records that are equal except for their mask merge, so there
//! is one record per distinct signature with non-overlapping masks.

use std::collections::BTreeMap;

use bleedthrough_builtins::{Detail, Kind, PhpVersion, Record, VersionMask};

use crate::conflicts::{Conflict, Conflicts};
use crate::extract::{Extracted, GenError};

/// What the merge did, for the coverage report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeReport {
    /// Symbols (per version) whose several declarations were equal and merged.
    pub merged_declarations: usize,
    /// Alias targets that name no function or method in the data.
    pub unresolved_aliases: Vec<String>,
}

type Key = (Kind, String);

/// The extraction of one PHP version.
pub type VersionItems = (PhpVersion, Vec<Extracted>);

/// The merged records and what the merge did.
pub type Merged = (Vec<Record>, MergeReport);

/// One symbol's resolved record in each version that has it.
type PerVersion = Vec<(PhpVersion, Record)>;

fn alias_of(detail: &Detail) -> Option<&String> {
    match detail {
        Detail::Function(f) => f.alias_of.as_ref(),
        Detail::Method(m) => m.alias_of.as_ref(),
        _ => None,
    }
}

fn set_alias(detail: &mut Detail, alias: Option<String>) {
    match detail {
        Detail::Function(f) => f.alias_of = alias,
        Detail::Method(m) => m.alias_of = alias,
        _ => {}
    }
}

fn clear_value(detail: &mut Detail) {
    match detail {
        Detail::Constant(c) | Detail::ClassConstant(c) => c.value = None,
        Detail::EnumCase(c) => c.value = None,
        Detail::Property(p) => p.default = None,
        _ => {}
    }
}

/// What must be equal for declarations to merge: everything except where they were declared.
fn identity(record: &Record) -> Record {
    let mut copy = record.clone();
    set_alias(&mut copy.detail, None);
    copy.sapi.clear();
    copy.guards.clear();
    copy.extension.clear();
    copy
}

fn all_equal(items: &[Extracted]) -> bool {
    let first = identity(&items[0].record);
    items.iter().all(|i| identity(&i.record) == first)
}

fn describe(items: &[Extracted]) -> String {
    items
        .iter()
        .map(|i| format!("{} [{}]", i.file, i.guard.as_deref().unwrap_or("unguarded")))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Combines equal declarations of one symbol in one version into one record.
fn combine(mut items: Vec<Extracted>, extension: Option<&str>) -> Record {
    items.sort_by(|a, b| {
        (&a.record.sapi, &a.file, &a.guard).cmp(&(&b.record.sapi, &b.file, &b.guard))
    });
    let mut record = items[0].record.clone();
    record.sapi = items
        .iter()
        .flat_map(|i| i.record.sapi.iter().cloned())
        .collect();
    record.sapi.sort();
    record.sapi.dedup();
    record.guards = if items.iter().any(|i| i.guard.is_none()) {
        Vec::new()
    } else {
        let mut guards: Vec<String> = items.iter().filter_map(|i| i.guard.clone()).collect();
        guards.sort();
        guards.dedup();
        guards
    };
    let first_alias = alias_of(&items[0].record.detail).cloned();
    let agree = items
        .iter()
        .all(|i| alias_of(&i.record.detail) == first_alias.as_ref());
    set_alias(&mut record.detail, if agree { first_alias } else { None });
    if let Some(extension) = extension {
        record.extension = extension.to_string();
    }
    record
}

/// Resolves the declarations of one symbol in one version into one record.
fn resolve(
    key: &Key,
    mut items: Vec<Extracted>,
    conflicts: &mut Conflicts,
    report: &mut MergeReport,
) -> Result<Record, String> {
    items.sort_by(|a, b| (&a.file, &a.guard).cmp(&(&b.file, &b.guard)));
    let mut extension = None;
    if items.len() > 1 && !all_equal(&items) {
        let entry: Conflict = conflicts.take(key.0, &key.1).ok_or_else(|| {
            format!(
                "{} `{}` is declared more than once with different details ({}); add a reviewed entry to stubs/conflicts.toml",
                key.0.as_str(),
                items[0].record.name,
                describe(&items)
            )
        })?;
        if let Some(guard) = &entry.keep_guard {
            items.retain(|i| i.guard.as_deref() == Some(guard.as_str()));
            if items.is_empty() {
                return Err(format!(
                    "the conflict entry for `{}` keeps the guard `{guard}`, which no declaration has",
                    entry.name
                ));
            }
        }
        if let Some(sapi) = &entry.drop_sapi {
            items.retain(|i| !i.record.sapi.contains(sapi));
            if items.is_empty() {
                return Err(format!(
                    "the conflict entry for `{}` drops every declaration",
                    entry.name
                ));
            }
        }
        if entry.clear_value {
            items
                .iter_mut()
                .for_each(|i| clear_value(&mut i.record.detail));
        }
        if !all_equal(&items) {
            return Err(format!(
                "the conflict entry for `{}` does not make its declarations equal ({})",
                entry.name,
                describe(&items)
            ));
        }
        extension = entry.extension;
    }
    if items.len() > 1 {
        report.merged_declarations += 1;
    }
    Ok(combine(items, extension.as_deref()))
}

/// Merges the extractions of all versions into records with masks.
pub fn merge(
    versions: Vec<VersionItems>,
    conflicts: &mut Conflicts,
) -> Result<Merged, Vec<GenError>> {
    let mut report = MergeReport::default();
    let mut errors = Vec::new();
    let mut by_key: BTreeMap<Key, PerVersion> = BTreeMap::new();
    for (version, items) in versions {
        let mut groups: BTreeMap<Key, Vec<Extracted>> = BTreeMap::new();
        for item in items {
            groups.entry(item.record.key()).or_default().push(item);
        }
        for (key, group) in groups {
            match resolve(&key, group, conflicts, &mut report) {
                Ok(record) => by_key.entry(key).or_default().push((version, record)),
                Err(message) => errors.push(GenError {
                    file: format!("php-8.{}", version.minor()),
                    offset: 0,
                    message,
                }),
            }
        }
    }
    for entry in conflicts.unused() {
        errors.push(GenError {
            file: "stubs/conflicts.toml".to_string(),
            offset: 0,
            message: format!(
                "the entry for `{}` resolves no conflict in any version; remove it",
                entry.name
            ),
        });
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let mut records: Vec<Record> = Vec::new();
    for (_, per_version) in by_key {
        let mut merged: Vec<Record> = Vec::new();
        for (version, record) in per_version {
            let mask = VersionMask::of(version);
            let same = merged.iter_mut().find(|m| {
                let mut other = record.clone();
                other.mask = m.mask;
                **m == other
            });
            match same {
                Some(existing) => existing.mask = existing.mask.union(mask),
                None => merged.push(record),
            }
        }
        records.extend(merged);
    }
    report.unresolved_aliases = unresolved_aliases(&records);
    Ok((records, report))
}

/// Alias targets that no record names (for the report, not an error: some are C-only).
fn unresolved_aliases(records: &[Record]) -> Vec<String> {
    let known: std::collections::BTreeSet<(Kind, String)> =
        records.iter().map(Record::key).collect();
    let mut missing: Vec<String> = records
        .iter()
        .filter_map(|r| {
            let target = alias_of(&r.detail)?;
            let kind = if target.contains("::") {
                Kind::Method
            } else {
                Kind::Function
            };
            let key = (kind, bleedthrough_builtins::normalise_name(kind, target));
            (!known.contains(&key)).then(|| format!("{} -> {target}", r.name))
        })
        .collect();
    missing.sort();
    missing.dedup();
    missing
}

#[cfg(test)]
mod tests {
    use super::*;
    use bleedthrough_builtins::{ConstantDetail, FunctionDetail, Param, Signature};

    fn v(minor: u8) -> PhpVersion {
        PhpVersion::parse(&format!("8.{minor}")).unwrap()
    }

    fn function(
        name: &str,
        version: u8,
        params: &[&str],
        place: (&str, Option<&str>, Option<&str>),
    ) -> Extracted {
        let (file, sapi, guard) = place;
        let signature = Signature {
            params: params
                .iter()
                .map(|p| Param {
                    name: (*p).to_string(),
                    type_text: "int".into(),
                    ..Param::default()
                })
                .collect(),
            return_type: "int".into(),
            ..Signature::default()
        };
        let mut record = Record::new(
            name,
            VersionMask::of(v(version)),
            sapi.unwrap_or("standard"),
            Detail::Function(FunctionDetail {
                signature,
                ..FunctionDetail::default()
            }),
        );
        record.sapi = sapi.map(str::to_string).into_iter().collect();
        Extracted {
            record,
            file: file.into(),
            guard: guard.map(str::to_string),
        }
    }

    fn run(items: Vec<(u8, Vec<Extracted>)>) -> Vec<Record> {
        let versions = items.into_iter().map(|(m, i)| (v(m), i)).collect();
        merge(versions, &mut Conflicts::none()).unwrap().0
    }

    #[test]
    fn a_function_added_in_8_3_gets_the_mask_8_3_to_8_5() {
        let f = |m| function("new_fn", m, &["a"], ("a.stub.php", None, None));
        let records = run(vec![
            (2, vec![]),
            (3, vec![f(3)]),
            (4, vec![f(4)]),
            (5, vec![f(5)]),
        ]);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].mask.to_string(), "8.3-8.5");
    }

    #[test]
    fn a_changed_signature_gives_one_record_per_form_with_non_overlapping_masks() {
        let old = |m| function("f", m, &["a"], ("a.stub.php", None, None));
        let new = |m| function("f", m, &["a", "b"], ("a.stub.php", None, None));
        let records = run(vec![
            (0, vec![old(0)]),
            (1, vec![new(1)]),
            (2, vec![new(2)]),
        ]);
        let masks: Vec<String> = records.iter().map(|r| r.mask.to_string()).collect();
        assert_eq!(masks, ["8.0", "8.1-8.2"]);
    }

    #[test]
    fn non_contiguous_masks_are_kept() {
        let f = |m| function("f", m, &["a"], ("a.stub.php", None, None));
        let g = |m| function("f", m, &["a", "b"], ("a.stub.php", None, None));
        let records = run(vec![(0, vec![f(0)]), (1, vec![g(1)]), (2, vec![f(2)])]);
        let masks: Vec<String> = records.iter().map(|r| r.mask.to_string()).collect();
        assert_eq!(masks, ["8.0,8.2", "8.1"]);
    }

    #[test]
    fn the_same_signature_from_two_sapis_is_one_record_listing_both() {
        let items = vec![
            function(
                "getallheaders",
                4,
                &[],
                ("sapi/cgi/a.stub.php", Some("cgi"), None),
            ),
            function(
                "getallheaders",
                4,
                &[],
                ("sapi/fpm/b.stub.php", Some("fpm"), None),
            ),
        ];
        let (records, report) = merge(vec![(v(4), items)], &mut Conflicts::none()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].sapi, ["cgi", "fpm"]);
        assert_eq!(report.merged_declarations, 1);
    }

    #[test]
    fn differing_alias_targets_are_dropped_and_agreeing_ones_kept() {
        let mut a = function("g", 4, &[], ("sapi/cgi/a.stub.php", Some("cgi"), None));
        let mut b = function("g", 4, &[], ("sapi/fpm/b.stub.php", Some("fpm"), None));
        for (item, target) in [(&mut a, "x"), (&mut b, "y")] {
            if let Detail::Function(f) = &mut item.record.detail {
                f.alias_of = Some(target.into());
            }
        }
        let records = run(vec![(4, vec![a.clone(), b])]);
        assert_eq!(alias_of(&records[0].detail), None);
        let mut c = a.clone();
        c.file = "sapi/fpm/c.stub.php".into();
        c.record.sapi = vec!["fpm".into()];
        let records = run(vec![(4, vec![a, c])]);
        assert_eq!(alias_of(&records[0].detail).map(String::as_str), Some("x"));
    }

    #[test]
    fn both_branches_of_a_guard_are_one_record_with_both_alternatives() {
        let items = vec![
            function("f", 5, &["a"], ("x.stub.php", None, Some("defined(A)"))),
            function("f", 5, &["a"], ("x.stub.php", None, Some("!defined(A)"))),
        ];
        // Equal in both branches and unguarded alternatives are both present: guards list both.
        let (records, _) = merge(vec![(v(5), items)], &mut Conflicts::none()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].guards, ["!defined(A)", "defined(A)"]);
        let items = vec![
            function("f", 5, &["a"], ("x.stub.php", None, Some("defined(A)"))),
            function("f", 5, &["a"], ("y.stub.php", None, None)),
        ];
        let (records, _) = merge(vec![(v(5), items)], &mut Conflicts::none()).unwrap();
        assert!(records[0].guards.is_empty());
    }

    #[test]
    fn an_unlisted_conflict_fails_and_a_listed_one_is_resolved_and_must_be_used() {
        let items = || {
            vec![
                function(
                    "ldap_connect",
                    5,
                    &["a"],
                    ("l.stub.php", None, Some("!defined(X)")),
                ),
                function(
                    "ldap_connect",
                    5,
                    &["a", "b"],
                    ("l.stub.php", None, Some("defined(X)")),
                ),
            ]
        };
        let errors = merge(vec![(v(5), items())], &mut Conflicts::none()).unwrap_err();
        assert!(errors[0].message.contains("conflicts.toml"), "{errors:?}");

        let text = "[[conflict]]\nkind = \"function\"\nname = \"ldap_connect\"\nkeep_guard = \"!defined(X)\"\nreason = \"r\"\n";
        let (records, _) =
            merge(vec![(v(5), items())], &mut Conflicts::parse(text).unwrap()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].guards, ["!defined(X)"]);

        let stale = "[[conflict]]\nkind = \"function\"\nname = \"nothing\"\nreason = \"r\"\n";
        let errors =
            merge(vec![(v(5), items())], &mut Conflicts::parse(stale).unwrap()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("resolves no conflict"))
        );
    }

    #[test]
    fn a_conflict_can_clear_a_differing_value() {
        let constant = |value: &str, file: &str, ext: &str| {
            let mut record = Record::new(
                "P",
                VersionMask::of(v(5)),
                ext,
                Detail::Constant(ConstantDetail {
                    value: Some(value.into()),
                    ..ConstantDetail::default()
                }),
            );
            record.sapi.clear();
            Extracted {
                record,
                file: file.into(),
                guard: None,
            }
        };
        let text = "[[conflict]]\nkind = \"const\"\nname = \"P\"\nclear_value = true\nextension = \"standard\"\nreason = \"r\"\n";
        let items = vec![
            constant("\"a\"", "a.stub.php", "openssl"),
            constant("\"b\"", "b.stub.php", "sodium"),
        ];
        let (records, _) =
            merge(vec![(v(5), items)], &mut Conflicts::parse(text).unwrap()).unwrap();
        assert_eq!(records[0].extension, "standard");
        let Detail::Constant(c) = &records[0].detail else {
            panic!()
        };
        assert_eq!(c.value, None);
    }

    #[test]
    fn unresolvable_aliases_are_reported() {
        let mut a = function("alias_fn", 5, &[], ("a.stub.php", None, None));
        if let Detail::Function(f) = &mut a.record.detail {
            f.alias_of = Some("missing_target".into());
        }
        let (_, report) = merge(vec![(v(5), vec![a])], &mut Conflicts::none()).unwrap();
        assert_eq!(report.unresolved_aliases, ["alias_fn -> missing_target"]);
    }
}
