//! Smoke: every shipped format name resolves to an emitter and writes valid, non-empty output
//! for a minimal report. Adding a name to `Format::NAMES` without an emitter fails here.

mod common;

use bleedthrough_app::{EmitContext, Format};

#[test]
fn every_listed_name_emits_valid_non_empty_output() {
    let src = common::sources();
    let ctx = EmitContext::new().with_sources(&src);
    let report = common::full();
    for name in Format::NAMES {
        let format: Format = name.parse().unwrap_or_else(|e| panic!("{e}"));
        let emitter = bleedthrough_emit::emitter_for(format)
            .unwrap_or_else(|| panic!("no emitter wired for `{name}`"));
        let mut out = Vec::new();
        emitter.emit(&report, &ctx, &mut out).unwrap();
        let text =
            String::from_utf8(out).unwrap_or_else(|_| panic!("`{name}` output is not UTF-8"));
        assert!(!text.trim().is_empty(), "`{name}` produced no output");
        match format {
            Format::Json | Format::Sarif => assert!(
                serde_json::from_str::<serde_json::Value>(&text).is_ok(),
                "{name}"
            ),
            Format::Ndjson => assert!(
                text.lines()
                    .all(|l| serde_json::from_str::<serde_json::Value>(l).is_ok()),
                "{name}"
            ),
            _ => {}
        }
    }
}
