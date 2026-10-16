//! Bleedthrough tooling: architecture checks run locally (`cargo xtask <command>`) and in CI.

mod api;
mod apiscan;
mod declaration;
mod glossary;
mod layering;
mod manifest;
mod meta;
mod notice;
mod policy;
mod reach;
mod release;
mod vendor;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use declaration::{Declaration, Layer};
use meta::Package;

const USAGE: &str = "\
usage: cargo xtask <command>

commands:
  check-layering   layer rule, workspace inheritance, domain purity, parser isolation and pins
  check-api        public-API snapshots (--update to rewrite; `check-api --help` for limits)
  check-glossary   public domain types name a glossary term
  check-builtins   vendored sources match the manifest (more as the pipeline grows)
  check-release    pre-release gate: the licensing decision on the built-in data is recorded
                   (not part of check-all; fails until the owner decides)
  gen-notice       rewrite NOTICE from the vendored licence files
  check-all        every check above
  vendor-php-src   fetch one php-src release's stubs into third_party/ and the manifest (network;
                   `vendor-php-src --help`)";

struct Workspace {
    root: PathBuf,
    declaration: Declaration,
    packages: Vec<Package>,
    graph: reach::Graph,
}

fn load() -> Result<Workspace, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(PathBuf::from)
        .unwrap_or_default();
    let text = read(&root.join("architecture.toml"))?;
    Ok(Workspace {
        declaration: Declaration::parse(&text)?,
        packages: meta::load()?,
        graph: meta::load_graph()?,
        root,
    })
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn check_layering(ws: &Workspace) -> Result<Vec<String>, String> {
    let mut errors = layering::check(&ws.declaration, &ws.packages);
    let manifests = ws
        .packages
        .iter()
        .map(|p| read(&p.manifest).map(|text| (p.name.clone(), text)))
        .collect::<Result<Vec<_>, _>>()?;
    errors.extend(policy::check_inheritance(&manifests));
    errors.extend(policy::check_workspace_root(&read(
        &ws.root.join("Cargo.toml"),
    )?));
    errors.extend(policy::check_purity(&ws.declaration, &ws.packages));
    errors.extend(policy::check_parser_backends(&ws.declaration, &ws.packages));
    let members: Vec<String> = ws.packages.iter().map(|p| p.name.clone()).collect();
    errors.extend(reach::check(&ws.declaration, &ws.graph, &members));
    Ok(errors)
}

fn check_glossary(ws: &Workspace) -> Result<Vec<String>, String> {
    let (terms, mut errors) = glossary::parse_terms(&read(&ws.root.join("docs/glossary.md"))?);
    for package in ws
        .packages
        .iter()
        .filter(|p| ws.declaration.layer_of(&p.name) == Some(Layer::Domain))
    {
        match api::scan_package(package) {
            Ok(entries) => errors.extend(glossary::check_types(&package.name, &entries, &terms)),
            Err(e) => errors.push(format!("{}: {e}", package.name)),
        }
    }
    Ok(errors)
}

/// Offline checks of the built-in data pipeline's inputs and outputs.
fn check_builtins(ws: &Workspace) -> Result<Vec<String>, String> {
    let releases = manifest::load(&ws.root)?;
    let mut errors = manifest::verify(&ws.root, &releases)?;
    errors.extend(notice::verify(&ws.root, &releases)?);
    errors.extend(release::check_packaging(&ws.root));
    Ok(errors)
}

fn run(command: &str, rest: &[String]) -> Result<Vec<String>, String> {
    if command == "check-api" && rest.iter().any(|a| a == "--help") {
        println!("{}", api::HELP);
        return Ok(Vec::new());
    }
    if command == "vendor-php-src" {
        if rest.iter().any(|a| a == "--help") {
            println!("{}", vendor::HELP);
            return Ok(Vec::new());
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(PathBuf::from)
            .unwrap_or_default();
        for line in vendor::run_command(rest, &root)? {
            println!("{line}");
        }
        return Ok(Vec::new());
    }
    let ws = load()?;
    let update = rest.iter().any(|a| a == "--update");
    match command {
        "check-layering" => check_layering(&ws),
        "check-api" => Ok(api::run(&ws.declaration, &ws.packages, &ws.root, update)),
        "check-glossary" => check_glossary(&ws),
        "check-builtins" => check_builtins(&ws),
        "check-release" => Ok(release::check_decision(&ws.root)),
        "gen-notice" => {
            notice::write(&ws.root, &manifest::load(&ws.root)?)?;
            Ok(Vec::new())
        }
        "check-all" => {
            let mut errors = check_layering(&ws)?;
            errors.extend(api::run(&ws.declaration, &ws.packages, &ws.root, false));
            errors.extend(check_glossary(&ws)?);
            errors.extend(check_builtins(&ws)?);
            Ok(errors)
        }
        other => Err(format!("unknown command `{other}`\n{USAGE}")),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((command, rest)) = args.split_first() else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    match run(command, rest) {
        Ok(errors) if errors.is_empty() => {
            println!("{command}: ok");
            ExitCode::SUCCESS
        }
        Ok(errors) => {
            errors.iter().for_each(|e| eprintln!("error: {e}"));
            eprintln!("{command}: FAILED ({} problem(s))", errors.len());
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
