//! The in-process scoring of domain findings reduces them exactly as reading the `json` report does.

mod common;

use bleedthrough_app::Format;
use bleedthrough_eval::report;

#[test]
fn domain_findings_reduce_as_the_json_report_does() {
    for (name, built) in common::all() {
        let (bytes, _) = common::emit(Format::Json, &built);
        let text = String::from_utf8(bytes).expect("utf-8");
        let from_json = report::read(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        let in_process = report::from_domain(built.findings());
        assert_eq!(in_process, from_json, "{name}");
    }
}
