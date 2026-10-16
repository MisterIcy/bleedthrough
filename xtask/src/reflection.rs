//! `cargo xtask refresh-reflection`: records what real PHP interpreters report about their
//! built-in symbols, as checked-in fixtures (`stubs/reflection/php-8.N.json`).
//!
//! This runs the official PHP images, which are executables from the network, so it is manual and
//! opt-in: without `--run` it only prints what it would run. Required CI never calls it. The
//! controls are the digest pin (`fixtures/php/images.toml`, shared with the syntax ledger), the
//! reviewed script (`stubs/reflection/dump.php`), no network inside the container, a read-only
//! mount, and a fixture that contains only data.

use std::fs;
use std::path::Path;
use std::process::Command;

pub const HELP: &str = "\
usage: cargo xtask refresh-reflection [--run] [--version 8.N]...

Without --run, prints the images and digests that would be run and the commands, and executes
nothing. With --run, runs stubs/reflection/dump.php in each pinned official PHP image (needs Docker
and pulls the image if absent) and writes stubs/reflection/php-8.N.json. --version limits the run
to the given versions. Never run by CI.";

const IMAGES: &str = "fixtures/php/images.toml";
const SCRIPT: &str = "stubs/reflection/dump.php";
pub const FIXTURE_DIR: &str = "stubs/reflection";
const VERSIONS: [&str; 6] = ["8.0", "8.1", "8.2", "8.3", "8.4", "8.5"];

/// One interpreter to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub version: String,
    /// The image, pinned by digest, for example `php@sha256:...`.
    pub image: String,
    /// The fixture this step writes, relative to the repository root.
    pub output: String,
}

impl Step {
    /// The `docker` arguments of this step.
    pub fn docker_args(&self, script: &Path) -> Vec<String> {
        [
            "run".to_string(),
            "--rm".into(),
            "--network".into(),
            "none".into(),
            "-e".into(),
            format!("BT_IMAGE={}", self.image),
            "-v".into(),
            format!("{}:/dump.php:ro", script.display()),
            self.image.clone(),
            "php".into(),
            "/dump.php".into(),
        ]
        .to_vec()
    }
}

/// The steps for `versions` (all when empty), from the pinned images.
pub fn plan(images_toml: &str, versions: &[String]) -> Result<Vec<Step>, String> {
    let table: toml::Table = images_toml.parse().map_err(|e| format!("{IMAGES}: {e}"))?;
    let images = table
        .get("images")
        .and_then(|i| i.as_table())
        .ok_or_else(|| format!("{IMAGES}: no [images] table"))?;
    if let Some(bad) = versions.iter().find(|v| !VERSIONS.contains(&v.as_str())) {
        return Err(format!("`{bad}` is not one of {}", VERSIONS.join(", ")));
    }
    VERSIONS
        .iter()
        .filter(|v| versions.is_empty() || versions.iter().any(|w| w == *v))
        .map(|version| {
            let image = images
                .get(*version)
                .and_then(|i| i.as_str())
                .ok_or_else(|| format!("{IMAGES}: no image for PHP {version}"))?;
            if !image.contains("@sha256:") {
                return Err(format!(
                    "{IMAGES}: the image for PHP {version} is not pinned by digest"
                ));
            }
            Ok(Step {
                version: (*version).to_string(),
                image: image.to_string(),
                output: format!("{FIXTURE_DIR}/php-{version}.json"),
            })
        })
        .collect()
}

/// A sanity check of a dump before it is written: JSON object of the expected shape and version.
pub fn check_dump(version: &str, output: &str) -> Result<(), String> {
    let starts = format!("\"php_version\":\"{version}.");
    if !output.starts_with("{\n") || !output.trim_end().ends_with('}') {
        return Err("the output is not the dump's JSON object".to_string());
    }
    if !output.contains(&starts) {
        return Err(format!("the interpreter is not PHP {version}"));
    }
    for section in [
        "\"extensions\":[",
        "\"functions\":{",
        "\"classes\":{",
        "\"constants\":{",
    ] {
        if !output.contains(section) {
            return Err(format!("the dump has no {section}"));
        }
    }
    Ok(())
}

pub fn run_command(rest: &[String], root: &Path) -> Result<Vec<String>, String> {
    let execute = rest.iter().any(|a| a == "--run");
    let mut versions = Vec::new();
    let mut args = rest.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--run" => {}
            "--version" => versions.push(args.next().ok_or("--version needs a value")?.clone()),
            other => return Err(format!("unknown argument `{other}`\n{HELP}")),
        }
    }
    let images = fs::read_to_string(root.join(IMAGES)).map_err(|e| format!("{IMAGES}: {e}"))?;
    let steps = plan(&images, &versions)?;
    let script = root.join(SCRIPT);
    if !script.is_file() {
        return Err(format!("{SCRIPT} is missing"));
    }
    println!(
        "refresh-reflection: {} ({} image(s))",
        if execute {
            "running"
        } else {
            "dry run, nothing is executed; pass --run to execute"
        },
        steps.len()
    );
    for step in &steps {
        println!("PHP {}: {}", step.version, step.image);
        println!("  docker {}", step.docker_args(&script).join(" "));
        println!("  writes {}", step.output);
    }
    if !execute {
        return Ok(Vec::new());
    }
    let mut problems = Vec::new();
    for step in &steps {
        println!("running PHP {} ...", step.version);
        let result = Command::new("docker")
            .args(step.docker_args(&script))
            .output()
            .map_err(|e| format!("cannot run docker: {e}"))?;
        let stdout = String::from_utf8_lossy(&result.stdout).into_owned();
        if !result.status.success() {
            let err = String::from_utf8_lossy(&result.stderr);
            problems.push(format!(
                "PHP {}: docker failed: {}",
                step.version,
                err.trim()
            ));
            continue;
        }
        match check_dump(&step.version, &stdout) {
            Ok(()) => fs::write(root.join(&step.output), stdout)
                .map_err(|e| format!("{}: {e}", step.output))?,
            Err(e) => problems.push(format!("PHP {}: {e}", step.version)),
        }
    }
    Ok(problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    const IMAGES_TOML: &str = "[images]\n\"7.4\" = \"php@sha256:aa\"\n\"8.0\" = \"php@sha256:bb\"\n\"8.1\" = \"php@sha256:cc\"\n\"8.2\" = \"php@sha256:dd\"\n\"8.3\" = \"php@sha256:ee\"\n\"8.4\" = \"php@sha256:ff\"\n\"8.5\" = \"php@sha256:11\"\n";

    #[test]
    fn the_plan_lists_the_pinned_image_of_each_version() {
        let steps = plan(IMAGES_TOML, &[]).unwrap();
        assert_eq!(steps.len(), 6);
        assert_eq!(steps[0].image, "php@sha256:bb");
        assert_eq!(steps[5].output, "stubs/reflection/php-8.5.json");
        let one = plan(IMAGES_TOML, &["8.3".to_string()]).unwrap();
        assert_eq!(one.len(), 1);
        assert!(plan(IMAGES_TOML, &["7.4".to_string()]).is_err());
        assert!(
            plan(&IMAGES_TOML.replace("php@sha256:bb", "php:8.0"), &[])
                .unwrap_err()
                .contains("digest")
        );
    }

    #[test]
    fn the_container_has_no_network_and_a_read_only_script() {
        let step = &plan(IMAGES_TOML, &["8.4".to_string()]).unwrap()[0];
        let args = step
            .docker_args(Path::new("/repo/stubs/reflection/dump.php"))
            .join(" ");
        assert!(args.contains("--network none"), "{args}");
        assert!(
            args.contains("/repo/stubs/reflection/dump.php:/dump.php:ro"),
            "{args}"
        );
        assert!(args.contains("php@sha256:ff php /dump.php"), "{args}");
    }

    #[test]
    fn a_dump_of_the_wrong_version_or_shape_is_rejected() {
        let ok = "{\n\"php_version\":\"8.4.26\",\n\"extensions\":[],\n\"functions\":{\n},\n\"classes\":{\n},\n\"constants\":{\n}\n}\n";
        assert!(check_dump("8.4", ok).is_ok());
        assert!(check_dump("8.3", ok).unwrap_err().contains("not PHP 8.3"));
        assert!(check_dump("8.4", "garbage").is_err());
    }

    #[test]
    fn without_run_nothing_is_executed() {
        // The dry run is the default: this must return without needing docker.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        assert!(run_command(&[], root).unwrap().is_empty());
    }
}
