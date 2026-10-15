//! Golden files: each format's output for the fixed report set. Set `BLESS=1` to rewrite them,
//! then review the diff like any other change.

mod common;

use bleedthrough_app::Format;

#[test]
fn output_matches_the_golden_files() {
    let bless = std::env::var_os("BLESS").is_some();
    let mut failures = Vec::new();
    for (name, report) in common::all() {
        for format in Format::ALL {
            let (actual, _) = common::emit(format, &report);
            let path = common::golden_dir().join(format!("{name}.{}", common::extension(format)));
            if bless {
                std::fs::write(&path, &actual).unwrap();
            } else if std::fs::read(&path).ok().as_deref() != Some(actual.as_slice()) {
                failures.push(path.display().to_string());
            }
        }
    }
    assert!(
        failures.is_empty(),
        "output differs from golden files (BLESS=1 to update): {failures:#?}"
    );
}
