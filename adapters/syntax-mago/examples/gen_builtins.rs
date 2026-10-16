//! Runs the built-in data generator with the Mago backend (`cargo xtask gen-builtins`).

use std::path::PathBuf;

use bleedthrough_builtins_gen::PhpVersion;
use bleedthrough_builtins_gen::release::{ReleaseInput, extract_release};
use bleedthrough_syntax_mago::MagoBackend;

fn main() {
    let root = PathBuf::from(std::env::args().nth(1).unwrap_or_default());
    let minor: u8 = std::env::args()
        .nth(2)
        .and_then(|m| m.parse().ok())
        .unwrap_or(5);
    let backend = MagoBackend::new();
    let version = PhpVersion::parse(&format!("8.{minor}")).expect("version");
    let out = extract_release(&backend, &ReleaseInput { version, root });
    for e in &out.errors {
        println!("ERR {e}");
    }
    let mut bad = 0;
    for item in &out.items {
        if let Err(e) = bleedthrough_builtins_gen::check_record(&item.record) {
            bad += 1;
            if bad < 40 {
                println!("INVALID {} {}: {e}", item.file, item.record.name);
            }
        }
    }
    println!("{bad} invalid");
    let mut by_key: std::collections::BTreeMap<
        _,
        Vec<&bleedthrough_builtins_gen::extract::Extracted>,
    > = Default::default();
    for item in &out.items {
        by_key.entry(item.record.key()).or_default().push(item);
    }
    let (mut same, mut differ) = (0, 0);
    for (key, items) in by_key.iter().filter(|(_, v)| v.len() > 1) {
        let identical = items
            .iter()
            .all(|i| i.record.detail == items[0].record.detail);
        if identical {
            same += 1
        } else {
            differ += 1
        }
        if !identical && differ < 25 {
            println!(
                "DIFFER {:?} {}: {}",
                key.0,
                key.1,
                items
                    .iter()
                    .map(|i| format!("{} [{}]", i.file, i.guard.clone().unwrap_or_default()))
                    .collect::<Vec<_>>()
                    .join(" | ")
            );
        }
    }
    println!("duplicates: {same} identical, {differ} differing");
    if std::env::args().nth(3).as_deref() == Some("--names") {
        for item in &out.items {
            println!("NAME {:?} {}", item.record.kind(), item.record.name);
        }
    } else if let Some(name) = std::env::args().nth(3) {
        for item in out
            .items
            .iter()
            .filter(|i| i.record.name.eq_ignore_ascii_case(&name))
        {
            println!("{:#?} guard={:?}", item.record, item.guard);
        }
    }
    println!("{} records, {} errors", out.items.len(), out.errors.len());
}
