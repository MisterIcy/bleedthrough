//! Byte-determinism: repeated emission and shuffled input give identical bytes.

mod common;

use bleedthrough_app::Format;
use bleedthrough_findings::Report;

fn reordered(report: &Report) -> Report {
    let mut findings = report.findings().to_vec();
    findings.reverse();
    Report::new(
        report.run().clone(),
        findings,
        report.coverage().clone(),
        *report.suppressed(),
    )
}

#[test]
fn emitting_twice_is_byte_identical() {
    for (name, report) in common::all() {
        for format in Format::ALL {
            assert_eq!(
                common::emit(format, &report).0,
                common::emit(format, &report).0,
                "{name} {format}"
            );
        }
    }
}

#[test]
fn shuffled_input_gives_identical_bytes() {
    for (name, report) in common::all() {
        let shuffled = reordered(&report);
        for format in Format::ALL {
            assert_eq!(
                common::emit(format, &report).0,
                common::emit(format, &shuffled).0,
                "{name} {format}"
            );
        }
    }
}

#[test]
fn output_is_utf8_without_control_sequences() {
    for (name, report) in common::all() {
        for format in Format::ALL {
            let text = String::from_utf8(common::emit(format, &report).0).unwrap();
            assert!(!text.contains('\u{1b}'), "{name} {format}");
        }
    }
}
