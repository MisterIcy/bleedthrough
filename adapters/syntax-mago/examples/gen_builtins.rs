//! Runs the built-in data generator with the Mago backend.
//!
//! ```text
//! cargo run -p bleedthrough-syntax-mago --example gen_builtins -- [--root DIR] [--out DIR]
//! ```
//!
//! Reads `third_party/php-src/php-8.N.*`, `stubs/conflicts.toml` and `stubs/bundling.toml` and
//! `stubs/reflection/php-8.N.json` under
//! the root (default: the current directory) and writes `builtin.data` and `coverage.txt` into
//! the output directory (default: `<root>/stubs`). `cargo xtask gen-builtins` runs this.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bleedthrough_builtins_gen::PhpVersion;
use bleedthrough_builtins_gen::fixture::Fixture;
use bleedthrough_builtins_gen::generate::generate;
use bleedthrough_builtins_gen::release::{NamedRelease, ReleaseInput};
use bleedthrough_syntax_mago::MagoBackend;

fn releases(root: &Path) -> Result<Vec<NamedRelease>, String> {
    let dir = root.join("third_party/php-src");
    let mut found = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let Some(version) = name.strip_prefix("php-") else {
            continue;
        };
        let mut parts = version.split('.');
        let (major, minor) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        let version = PhpVersion::parse(&format!("{major}.{minor}"))
            .ok_or_else(|| format!("{name}: not a supported PHP version"))?;
        found.push((
            name,
            ReleaseInput {
                version,
                root: path,
            },
        ));
    }
    found.sort_by_key(|(_, r)| r.version);
    if found.windows(2).any(|w| w[0].1.version == w[1].1.version) {
        return Err("two vendored releases for one PHP minor version".to_string());
    }
    Ok(found)
}

fn run() -> Result<(), Vec<String>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
    };
    let root = PathBuf::from(value("--root").map_or(".", String::as_str));
    let out = value("--out").map_or_else(|| root.join("stubs"), PathBuf::from);
    let read =
        |p: PathBuf| std::fs::read_to_string(&p).map_err(|e| vec![format!("{}: {e}", p.display())]);
    let releases = releases(&root).map_err(|e| vec![e])?;
    let mut fixtures = Vec::new();
    for (_, release) in &releases {
        let path = root.join(format!("stubs/reflection/php-{}.json", release.version));
        let fixture = Fixture::parse(&read(path.clone())?)
            .map_err(|e| vec![format!("{}: {e}", path.display())])?;
        fixtures.push(fixture);
    }
    let generated = generate(
        &MagoBackend::new(),
        &releases,
        &read(root.join("stubs/conflicts.toml"))?,
        &read(root.join("stubs/bundling.toml"))?,
        &fixtures,
    )?;
    std::fs::create_dir_all(&out).map_err(|e| vec![e.to_string()])?;
    for (name, text) in [
        ("builtin.data", &generated.data),
        ("coverage.txt", &generated.coverage),
    ] {
        std::fs::write(out.join(name), text).map_err(|e| vec![format!("{name}: {e}")])?;
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(problems) => {
            problems.iter().for_each(|p| eprintln!("error: {p}"));
            eprintln!("gen_builtins: FAILED ({} problem(s))", problems.len());
            ExitCode::FAILURE
        }
    }
}
