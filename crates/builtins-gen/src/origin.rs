//! Where a stub file comes from: its extension or SAPI, and whether it is excluded.

/// An explicit, reasoned exclusion of stub files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exclusion {
    /// Directory (relative to the release tree) whose stubs are excluded.
    pub directory: &'static str,
    pub reason: &'static str,
}

/// Test-only and template extensions. Their symbols are not part of any PHP installation's API.
pub const EXCLUSIONS: &[Exclusion] = &[
    Exclusion {
        directory: "ext/zend_test",
        reason: "test-only extension used by php-src's own test suite",
    },
    Exclusion {
        directory: "ext/dl_test",
        reason: "test-only extension used to test dl()",
    },
    Exclusion {
        directory: "ext/skeleton",
        reason: "template for writing new extensions",
    },
];

/// The exclusion that applies to the file at `path` (relative to the release tree).
pub fn excluded(path: &str) -> Option<&'static Exclusion> {
    EXCLUSIONS.iter().find(|e| {
        path.strip_prefix(e.directory)
            .is_some_and(|r| r.starts_with('/'))
    })
}

/// The extension and SAPI a stub file belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    pub extension: String,
    pub sapi: Option<String>,
}

/// The origin of the stub file at `path`; `None` when the path is in no known place.
///
/// `Zend/` and `main/` are the core; `ext/<name>/` is the extension `<name>`; `sapi/<name>/`
/// belongs to the SAPI `<name>`, which is also used as its extension name here.
pub fn origin_of(path: &str) -> Option<Origin> {
    let mut parts = path.split('/');
    let first = parts.next()?;
    match first {
        "Zend" | "main" => Some(Origin {
            extension: "core".to_string(),
            sapi: None,
        }),
        "ext" => parts.next().map(|name| Origin {
            extension: name.to_string(),
            sapi: None,
        }),
        "sapi" => parts.next().map(|name| Origin {
            extension: name.to_string(),
            sapi: Some(name.to_string()),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_map_to_extensions_and_sapis() {
        assert_eq!(
            origin_of("Zend/zend_enum.stub.php").unwrap().extension,
            "core"
        );
        assert_eq!(
            origin_of("main/streams/userspace.stub.php")
                .unwrap()
                .extension,
            "core"
        );
        assert_eq!(
            origin_of("ext/pdo_mysql/a.stub.php").unwrap().extension,
            "pdo_mysql"
        );
        let fpm = origin_of("sapi/fpm/fpm/fpm_main.stub.php").unwrap();
        assert_eq!(
            (fpm.extension.as_str(), fpm.sapi.as_deref()),
            ("fpm", Some("fpm"))
        );
        assert_eq!(origin_of("elsewhere/a.stub.php"), None);
    }

    #[test]
    fn test_and_template_extensions_are_excluded_by_directory() {
        assert!(excluded("ext/zend_test/test.stub.php").is_some());
        assert!(excluded("ext/dl_test/dl_test.stub.php").is_some());
        assert!(excluded("ext/skeleton/skeleton.stub.php").is_some());
        assert!(excluded("ext/zend_testing/a.stub.php").is_none());
        assert!(excluded("ext/standard/a.stub.php").is_none());
    }
}
