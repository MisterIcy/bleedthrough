//! Test hygiene (`cargo xtask check-tests`): every ignored test carries a reason, and no test that runs
//! by default invokes the PHP interpreter or the container runtime (see docs/testing.md).

use std::fs;
use std::path::{Path, PathBuf};

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use quote::ToTokens;
use syn::{Attribute, Expr, ExprLit, Item, Lit, Meta};

/// Directories searched for test sources, relative to the repository root.
const ROOTS: [&str; 3] = ["crates", "adapters", "xtask"];

/// Helper functions of the interpreter module that run a real interpreter.
const INTERPRETER_CALLS: [&str; 2] = ["local_version", "run_version"];

/// One `#[ignore]`d test, as reported to the user.
#[derive(Debug, PartialEq, Eq)]
pub struct Ignored {
    pub test: String,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct Report {
    pub ignored: Vec<Ignored>,
    pub errors: Vec<String>,
}

/// Scans one source text; `file` only labels the messages.
pub fn scan_source(file: &str, source: &str) -> Result<Report, String> {
    let parsed = syn::parse_file(source).map_err(|e| format!("{file}: {e}"))?;
    let mut report = Report::default();
    walk(&parsed.items, file, &mut report);
    Ok(report)
}

fn walk(items: &[Item], file: &str, report: &mut Report) {
    for item in items {
        match item {
            Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    walk(inner, file, report);
                }
            }
            Item::Fn(f) if is_test(&f.attrs) => {
                let name = format!("{file}::{}", f.sig.ident);
                match ignore_reason(&f.attrs) {
                    Some(None) => report
                        .errors
                        .push(format!("{name}: `#[ignore]` without a reason string")),
                    Some(Some(reason)) => report.ignored.push(Ignored { test: name, reason }),
                    None => {
                        let body = flatten(f.block.to_token_stream());
                        if let Some(call) = interpreter_use(&body) {
                            report.errors.push(format!(
                                "{name}: runs {call} without being ignored (add `#[ignore = \"needs ...\"]`)"
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn is_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        let path = a.path();
        path.is_ident("test")
            || path
                .segments
                .last()
                .is_some_and(|s| s.ident == "test" && path.segments.len() > 1)
    })
}

/// `None`: not ignored. `Some(None)`: ignored without a usable reason. `Some(Some(r))`: ignored with `r`.
fn ignore_reason(attrs: &[Attribute]) -> Option<Option<String>> {
    let attr = attrs.iter().find(|a| a.path().is_ident("ignore"))?;
    let reason = match &attr.meta {
        Meta::NameValue(nv) => match &nv.value {
            Expr::Lit(ExprLit {
                lit: Lit::Str(s), ..
            }) => Some(s.value()).filter(|r| !r.trim().is_empty()),
            _ => None,
        },
        _ => None,
    };
    Some(reason)
}

/// Tokens as text without whitespace; string literals other than the two tool names become `_`, so
/// sample sources inside strings never count as calls.
fn flatten(stream: TokenStream) -> String {
    let mut out = String::new();
    for tree in stream {
        match tree {
            TokenTree::Group(g) => {
                let (open, close) = match g.delimiter() {
                    Delimiter::Parenthesis => ("(", ")"),
                    Delimiter::Brace => ("{", "}"),
                    Delimiter::Bracket => ("[", "]"),
                    Delimiter::None => ("", ""),
                };
                out.push_str(open);
                out.push_str(&flatten(g.stream()));
                out.push_str(close);
            }
            TokenTree::Ident(i) => {
                out.push_str(&i.to_string());
                out.push(' ');
            }
            TokenTree::Punct(p) => out.push(p.as_char()),
            TokenTree::Literal(l) => {
                let text = l.to_string();
                out.push_str(if text == "\"php\"" || text == "\"docker\"" {
                    &text
                } else {
                    "_"
                });
            }
        }
    }
    out
}

fn interpreter_use(flat: &str) -> Option<String> {
    let tight: String = flat.chars().filter(|c| !c.is_whitespace()).collect();
    for tool in ["php", "docker"] {
        if tight.contains(&format!("Command::new(\"{tool}\"")) {
            return Some(format!("`{tool}`"));
        }
    }
    INTERPRETER_CALLS
        .iter()
        .find(|name| {
            flat.match_indices(*name).any(|(at, _)| {
                let before = flat[..at].chars().next_back();
                let after = flat[at + name.len()..].chars().next();
                !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
                    && matches!(after, Some('(') | Some(' '))
                    && flat[at + name.len()..].trim_start().starts_with('(')
            })
        })
        .map(|name| format!("the interpreter helper `{name}`"))
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if name != "target" {
                rust_files(&path, out)?;
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// Scans the workspace sources; returns the report with relative file labels.
pub fn scan_workspace(root: &Path) -> Result<Report, String> {
    let mut files = Vec::new();
    for dir in ROOTS {
        rust_files(&root.join(dir), &mut files)?;
    }
    let mut total = Report::default();
    for file in files {
        let label = file
            .strip_prefix(root)
            .unwrap_or(&file)
            .display()
            .to_string();
        let source = fs::read_to_string(&file).map_err(|e| format!("{label}: {e}"))?;
        let report = scan_source(&label, &source)?;
        total.ignored.extend(report.ignored);
        total.errors.extend(report.errors);
    }
    Ok(total)
}

/// The command: prints the ignored tests with their reasons and returns the problems.
pub fn run(root: &Path) -> Result<Vec<String>, String> {
    let report = scan_workspace(root)?;
    println!("ignored tests: {}", report.ignored.len());
    for ignored in &report.ignored {
        println!("  {} -- {}", ignored.test, ignored.reason);
    }
    Ok(report.errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(source: &str) -> Report {
        scan_source("sample.rs", source).expect("sample parses")
    }

    #[test]
    fn an_ignore_without_a_reason_is_reported_by_test_name() {
        let report = scan("#[test]\n#[ignore]\nfn quiet() {}");
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("sample.rs::quiet"));
    }

    #[test]
    fn an_ignore_with_an_empty_reason_is_reported() {
        let report = scan("#[test]\n#[ignore = \"  \"]\nfn blank() {}");
        assert_eq!(report.errors.len(), 1);
    }

    #[test]
    fn an_ignore_with_a_reason_is_counted_with_it() {
        let report = scan("#[test]\n#[ignore = \"needs php\"]\nfn loud() {}");
        assert!(report.errors.is_empty());
        assert_eq!(
            report.ignored,
            vec![Ignored {
                test: "sample.rs::loud".into(),
                reason: "needs php".into()
            }]
        );
    }

    #[test]
    fn a_default_test_running_php_is_reported() {
        let report = scan("#[test]\nfn lint() { std::process::Command::new(\"php\").output(); }");
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("`php`"));
    }

    #[test]
    fn a_default_test_running_docker_or_the_interpreter_helper_is_reported() {
        let docker = scan("#[test]\nfn d() { Command::new(\"docker\").status(); }");
        let helper = scan("#[test]\nfn h() { let v = local_version(); }");
        assert_eq!(docker.errors.len(), 1);
        assert_eq!(helper.errors.len(), 1);
    }

    #[test]
    fn an_ignored_test_may_run_the_interpreter() {
        let report = scan("#[test]\n#[ignore = \"needs php\"]\nfn ok() { Command::new(\"php\"); }");
        assert!(report.errors.is_empty());
    }

    #[test]
    fn sample_sources_inside_strings_are_not_calls() {
        let report = scan("#[test]\nfn s() { let t = \"Command::new(\\\"php\\\")\"; }");
        assert!(report.errors.is_empty());
    }

    #[test]
    fn tests_in_nested_modules_are_scanned_and_other_functions_ignored() {
        let report = scan(
            "mod tests { #[test] fn t() { Command::new(\"php\"); } }\nfn helper() { Command::new(\"php\"); }",
        );
        assert_eq!(report.errors.len(), 1);
    }

    #[test]
    fn a_longer_name_is_not_the_helper() {
        let report = scan("#[test]\nfn n() { my_local_version_x(); }");
        assert!(report.errors.is_empty());
    }
}
