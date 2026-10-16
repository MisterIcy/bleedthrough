//! A failing destination is an error, never success.

mod common;

use std::io::{self, Write};

use bleedthrough_app::{EmitContext, Format};

/// Accepts `ok_writes` write calls, then fails every call (including flush).
struct Failing {
    ok_writes: usize,
    calls: usize,
    written: Vec<u8>,
}

impl Failing {
    fn new(ok_writes: usize) -> Self {
        Self {
            ok_writes,
            calls: 0,
            written: Vec::new(),
        }
    }
}

impl Write for Failing {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.calls >= self.ok_writes {
            return Err(io::Error::other("disk full"));
        }
        self.calls += 1;
        self.written.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.calls >= self.ok_writes {
            Err(io::Error::other("flush failed"))
        } else {
            Ok(())
        }
    }
}

/// spec: report-emitters/Emission failures are reported
#[test]
fn every_emitter_errors_when_the_writer_fails() {
    let src = common::sources();
    let ctx = EmitContext::new().with_sources(&src);
    for (name, report) in common::all() {
        for format in Format::ALL {
            let emitter = bleedthrough_emit::emitter_for(format).unwrap();
            let total = {
                let mut counting = Failing::new(usize::MAX);
                emitter.emit(&report, &ctx, &mut counting).unwrap();
                counting.calls
            };
            for ok in 0..=total {
                let mut out = Failing::new(ok);
                let result = emitter.emit(&report, &ctx, &mut out);
                assert!(
                    result.is_err(),
                    "{name} {format}: reported success after write failure at call {ok}"
                );
            }
        }
    }
}

#[test]
fn interrupted_ndjson_has_no_summary_and_no_partial_line() {
    let report = common::mixed();
    let emitter = bleedthrough_emit::emitter_for(Format::Ndjson).unwrap();
    let records = report.findings().len() + 2;
    for ok in 0..records {
        let mut out = Failing::new(ok);
        assert!(
            emitter
                .emit(&report, &EmitContext::new(), &mut out)
                .is_err()
        );
        let text = String::from_utf8(out.written).unwrap();
        assert!(
            text.is_empty() || text.ends_with('\n'),
            "partial line after {ok} writes"
        );
        assert!(
            !text.contains("\"type\":\"summary\""),
            "summary present after {ok} writes"
        );
        for line in text.lines() {
            serde_json::from_str::<serde_json::Value>(line).expect("complete record");
        }
    }
}
