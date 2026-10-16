//! Release-related checks: the packaging definitions carry NOTICE, and the licensing decision on
//! the derived built-in data is recorded before a release.

use std::fs;
use std::path::Path;

pub const ARCHIVE_PATH: &str = "release/archive.toml";
pub const DOCKERFILE_PATH: &str = "Dockerfile";
pub const DECISION_PATH: &str = "docs/licensing-builtins.md";

/// NOTICE must be in the release archive definition (and exist) and in the Docker image.
pub fn check_packaging(root: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    match fs::read_to_string(root.join(ARCHIVE_PATH)) {
        Err(_) => errors.push(format!("{ARCHIVE_PATH} is missing")),
        Ok(text) => match text.parse::<toml::Table>() {
            Err(e) => errors.push(format!("{ARCHIVE_PATH}: {e}")),
            Ok(table) => {
                let include: Vec<&str> = table
                    .get("archive")
                    .and_then(|a| a.get("include"))
                    .and_then(|i| i.as_array())
                    .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();
                if !include.contains(&"NOTICE") {
                    errors.push(format!(
                        "{ARCHIVE_PATH}: the release archive must include NOTICE"
                    ));
                }
                for file in include.iter().filter(|f| !root.join(f).is_file()) {
                    errors.push(format!("{ARCHIVE_PATH}: `{file}` does not exist"));
                }
            }
        },
    }
    match fs::read_to_string(root.join(DOCKERFILE_PATH)) {
        Err(_) => errors.push(format!("{DOCKERFILE_PATH} is missing")),
        Ok(text) => {
            let copies_notice = text.lines().any(|line| {
                let mut words = line.split_whitespace();
                words.next().is_some_and(|w| w.eq_ignore_ascii_case("COPY"))
                    && words.any(|w| w == "NOTICE")
            });
            if !copies_notice {
                errors.push(format!("{DOCKERFILE_PATH}: the image must COPY NOTICE"));
            }
        }
    }
    errors
}

fn field<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|l| l.strip_prefix(name).and_then(|rest| rest.strip_prefix(':')))
        .map(str::trim)
}

/// The release gate: the owner's decision on the licensing of the derived data is recorded.
pub fn check_decision(root: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(root.join(DECISION_PATH)) else {
        return vec![format!("{DECISION_PATH} is missing")];
    };
    if field(&text, "Status") != Some("decided") {
        return vec![format!(
            "{DECISION_PATH}: the licensing position of the built-in data is unconfirmed; the owner must record a decision before release"
        )];
    }
    ["Decided by", "Date", "Decision"]
        .iter()
        .filter(|name| field(&text, name).is_none_or(str::is_empty))
        .map(|name| format!("{DECISION_PATH}: `{name}` is empty"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("xtask-release-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("release")).unwrap();
        dir
    }

    #[test]
    fn packaging_needs_notice_in_archive_and_image() {
        let dir = scratch("pack");
        fs::write(dir.join("NOTICE"), "n").unwrap();
        fs::write(dir.join("README.md"), "r").unwrap();
        fs::write(
            dir.join(ARCHIVE_PATH),
            "[archive]\ninclude = [\"README.md\", \"NOTICE\"]\n",
        )
        .unwrap();
        fs::write(
            dir.join(DOCKERFILE_PATH),
            "FROM scratch\nCOPY LICENSE-MIT NOTICE /doc/\n",
        )
        .unwrap();
        assert_eq!(check_packaging(&dir), Vec::<String>::new());

        fs::write(
            dir.join(ARCHIVE_PATH),
            "[archive]\ninclude = [\"README.md\"]\n",
        )
        .unwrap();
        fs::write(dir.join(DOCKERFILE_PATH), "FROM scratch\n# COPY NOTICE\n").unwrap();
        let errors = check_packaging(&dir).join("\n");
        assert!(errors.contains("archive must include NOTICE"), "{errors}");
        assert!(errors.contains("image must COPY NOTICE"), "{errors}");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_release_gate_fails_until_a_decision_is_recorded() {
        let dir = scratch("decision");
        fs::create_dir_all(dir.join("docs")).unwrap();
        let path = dir.join(DECISION_PATH);
        fs::write(
            &path,
            "Status: unconfirmed\nDecided by:\nDate:\nDecision:\n",
        )
        .unwrap();
        assert!(check_decision(&dir)[0].contains("unconfirmed"));
        fs::write(
            &path,
            "Status: decided\nDecided by: A\nDate:\nDecision: ship\n",
        )
        .unwrap();
        assert_eq!(
            check_decision(&dir),
            vec![format!("{DECISION_PATH}: `Date` is empty")]
        );
        fs::write(
            &path,
            "Status: decided\nDecided by: A\nDate: 2026-11-01\nDecision: ship\n",
        )
        .unwrap();
        assert!(check_decision(&dir).is_empty());
        let _ = fs::remove_dir_all(dir);
    }
}
