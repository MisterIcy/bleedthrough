//! Composer reading and class location over a fake project with a vendor directory.

use bleedthrough_fs::{ComposerProject, MemFs};
use bleedthrough_symbols::{ClassLocator, Fqn};

const MANIFEST: &str = r#"{
  "autoload": {
    "psr-4": { "App\\": "src/", "App\\Models\\": ["src/Models/", "legacy/Models/"] },
    "psr-0": { "Old_": "lib/" },
    "classmap": ["generated/"],
    "files": ["src/helpers.php"],
    "exclude-from-classmap": ["generated/skip/"]
  },
  "autoload-dev": { "psr-4": { "Tests\\": "tests/" } }
}"#;

const LOCK: &str = r#"{
  "packages": [
    { "name": "zeta/zed", "version": "1.2.0", "source": { "reference": "abc" },
      "autoload": { "psr-4": { "Zed\\": "src/" }, "files": ["src/boot.php"] } },
    { "name": "acme/lib", "version": "2.0.1", "dist": { "reference": "def" },
      "autoload": { "psr-4": { "Acme\\": "src/" } } }
  ],
  "packages-dev": [
    { "name": "dev/tool", "version": "0.1.0", "autoload": { "psr-4": { "Dev\\": "../../../../escape/" } } }
  ]
}"#;

fn fake_project() -> MemFs {
    MemFs::new()
        .with_file("composer.json", MANIFEST)
        .with_file("composer.lock", LOCK)
        .with_file(
            "src/Http/Kernel.php",
            "<?php namespace App\\Http; class Kernel {}",
        )
        .with_file("src/Models/User.php", "<?php")
        .with_file("legacy/Models/User.php", "<?php")
        .with_file("legacy/Models/Legacy.php", "<?php")
        .with_file("src/helpers.php", "<?php")
        .with_file("lib/Old/Thing.php", "<?php")
        .with_file("tests/UnitTest.php", "<?php")
        .with_file(
            "generated/Map.php",
            "<?php namespace Gen; class Mapped {} interface Api {}",
        )
        .with_file("generated/skip/Skipped.php", "<?php class Skipped {}")
        .with_file("vendor/acme/lib/src/Client.php", "<?php")
        .with_file("vendor/zeta/zed/src/Zed.php", "<?php")
        .with_file("vendor/zeta/zed/src/boot.php", "<?php")
}

fn locate(project: &ComposerProject, fs: &MemFs, class: &str) -> Option<String> {
    project
        .locator(fs)
        .locate(&Fqn::new(class))
        .map(String::from)
}

#[test]
fn project_and_vendor_classes_are_located() {
    let fs = fake_project();
    let project = ComposerProject::read(&fs);
    assert_eq!(
        project
            .diagnostics()
            .iter()
            .filter(|d| d.file != "composer.lock")
            .count(),
        0
    );
    assert_eq!(
        locate(&project, &fs, "App\\Http\\Kernel").as_deref(),
        Some("src/Http/Kernel.php")
    );
    assert_eq!(
        locate(&project, &fs, "Acme\\Client").as_deref(),
        Some("vendor/acme/lib/src/Client.php")
    );
    assert_eq!(
        locate(&project, &fs, "Tests\\UnitTest").as_deref(),
        Some("tests/UnitTest.php")
    );
    assert_eq!(
        locate(&project, &fs, "Old_Thing").as_deref(),
        Some("lib/Old/Thing.php")
    );
    assert_eq!(
        locate(&project, &fs, "Gen\\Api").as_deref(),
        Some("generated/Map.php")
    );
    assert_eq!(locate(&project, &fs, "Nope\\Missing"), None);
}

#[test]
fn longest_prefix_and_first_directory_win() {
    let fs = fake_project();
    let project = ComposerProject::read(&fs);
    assert_eq!(
        locate(&project, &fs, "App\\Models\\User").as_deref(),
        Some("src/Models/User.php")
    );
    assert_eq!(
        locate(&project, &fs, "App\\Models\\Legacy").as_deref(),
        Some("legacy/Models/Legacy.php")
    );
}

#[test]
fn excluded_classmap_paths_are_not_scanned() {
    let fs = fake_project();
    let project = ComposerProject::read(&fs);
    assert_eq!(locate(&project, &fs, "Skipped"), None);
}

#[test]
fn packages_are_ordered_by_name_with_versions() {
    let fs = fake_project();
    let project = ComposerProject::read(&fs);
    let listed: Vec<_> = project
        .packages()
        .iter()
        .map(|p| {
            (
                p.name.as_str(),
                p.version.as_str(),
                p.reference.as_deref(),
                p.dev,
            )
        })
        .collect();
    assert_eq!(
        listed,
        [
            ("acme/lib", "2.0.1", Some("def"), false),
            ("dev/tool", "0.1.0", None, true),
            ("zeta/zed", "1.2.0", Some("abc"), false),
        ]
    );
}

#[test]
fn files_entries_are_reported() {
    let fs = fake_project();
    let project = ComposerProject::read(&fs);
    assert_eq!(
        project.autoload_files(),
        ["src/helpers.php", "vendor/zeta/zed/src/boot.php"]
    );
}

#[test]
fn a_path_escaping_the_project_is_ignored_with_a_diagnostic() {
    let fs = fake_project();
    let project = ComposerProject::read(&fs);
    assert!(
        project
            .diagnostics()
            .iter()
            .any(|d| d.message.contains("leaves the project"))
    );
    assert_eq!(locate(&project, &fs, "Dev\\Tool"), None);
}

#[test]
fn a_symlink_out_of_the_project_is_rejected() {
    let fs = fake_project().with_escaping_link("src/Models");
    let project = ComposerProject::read(&fs);
    assert_eq!(
        locate(&project, &fs, "App\\Models\\User").as_deref(),
        Some("legacy/Models/User.php")
    );
}

#[test]
fn a_malformed_lock_file_keeps_project_loading() {
    let fs = fake_project().with_file("composer.lock", "{ not json");
    let project = ComposerProject::read(&fs);
    assert!(
        project
            .diagnostics()
            .iter()
            .any(|d| d.file == "composer.lock" && d.message.contains("not valid JSON"))
    );
    assert!(project.packages().is_empty());
    assert_eq!(
        locate(&project, &fs, "App\\Http\\Kernel").as_deref(),
        Some("src/Http/Kernel.php")
    );
}

#[test]
fn missing_files_give_diagnostics_and_no_rules() {
    let fs = MemFs::new();
    let project = ComposerProject::read(&fs);
    assert_eq!(project.diagnostics().len(), 2);
    assert_eq!(locate(&project, &fs, "Any\\Thing"), None);
}

#[test]
fn reading_is_deterministic() {
    let fs = fake_project();
    assert_eq!(ComposerProject::read(&fs), ComposerProject::read(&fs));
}
