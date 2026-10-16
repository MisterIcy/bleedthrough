//! The built-in data compiled into the binary, behind the `BuiltinDataSource` port.

use bleedthrough_builtins::{BuiltinData, BuiltinDataSource, SourceError, format};

/// The canonical data file, `stubs/builtin.data`, embedded at build time.
const EMBEDDED: &str = include_str!("../../../stubs/builtin.data");

/// Term: Built-in data
///
/// The source that parses the data embedded in the binary: no file is read at run time.
#[derive(Debug, Clone, Copy, Default)]
pub struct EmbeddedBuiltinData;

impl BuiltinDataSource for EmbeddedBuiltinData {
    fn load(&self) -> Result<BuiltinData, SourceError> {
        format::read(EMBEDDED).map_err(|e| SourceError {
            message: format!("the embedded built-in data is invalid: {e}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bleedthrough_builtins::{Kind, PhpVersion};

    #[test]
    fn the_embedded_data_loads_and_answers_lookups() {
        let data = EmbeddedBuiltinData.load().expect("embedded data parses");
        let version = PhpVersion::parse("8.3").expect("a supported version");
        assert!(matches!(
            data.lookup(Kind::Function, "strlen", version),
            bleedthrough_builtins::Lookup::Found(_)
        ));
        assert_eq!(EmbeddedBuiltinData.load().expect("again"), data);
    }
}
