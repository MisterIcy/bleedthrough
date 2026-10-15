//! The `json` and `ndjson` emitters.

use std::io::Write;

use bleedthrough_app::{EmitContext, EmitError, EmitOutcome, Emitter, Format};
use bleedthrough_findings::Report;
use serde::Serialize;

use crate::wire;

fn to_io(e: serde_json::Error) -> EmitError {
    EmitError::Io(e.into())
}

pub struct JsonEmitter;

impl Emitter for JsonEmitter {
    fn format(&self) -> Format {
        Format::Json
    }

    fn emit(
        &self,
        report: &Report,
        _: &EmitContext<'_>,
        out: &mut dyn Write,
    ) -> Result<EmitOutcome, EmitError> {
        let mut bytes = serde_json::to_vec_pretty(&wire::document(report)).map_err(to_io)?;
        bytes.push(b'\n');
        out.write_all(&bytes)?;
        out.flush()?;
        Ok(EmitOutcome::new())
    }
}

pub struct NdjsonEmitter;

/// Serializes one record with its newline and writes it in a single call, so an interruption
/// can only fall between records.
fn record(out: &mut dyn Write, value: &impl Serialize) -> Result<(), EmitError> {
    let mut bytes = serde_json::to_vec(value).map_err(to_io)?;
    bytes.push(b'\n');
    out.write_all(&bytes)?;
    Ok(())
}

impl Emitter for NdjsonEmitter {
    fn format(&self) -> Format {
        Format::Ndjson
    }

    fn emit(
        &self,
        report: &Report,
        _: &EmitContext<'_>,
        out: &mut dyn Write,
    ) -> Result<EmitOutcome, EmitError> {
        record(out, &wire::run_record(report))?;
        for finding in report.findings() {
            record(out, &wire::finding_record(finding))?;
        }
        record(out, &wire::summary_record(report))?;
        out.flush()?;
        Ok(EmitOutcome::new())
    }
}
