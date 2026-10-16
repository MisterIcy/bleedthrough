//! Generic strategies: PHP names, bounded PHP-ish source, relative paths, versions and bytes.

use proptest::prelude::*;

/// A PHP identifier (ASCII letters, digits and underscore, not starting with a digit).
pub fn php_identifier() -> impl Strategy<Value = String> {
    "[a-zA-Z_][a-zA-Z0-9_]{0,15}"
}

/// A PHP name with one to four namespace segments, optionally fully qualified (leading backslash).
pub fn php_qualified_name() -> impl Strategy<Value = String> {
    (
        any::<bool>(),
        prop::collection::vec(php_identifier(), 1..=4),
    )
        .prop_map(|(rooted, segments)| {
            format!("{}{}", if rooted { "\\" } else { "" }, segments.join("\\"))
        })
}

/// A short PHP-looking file: an open tag and up to eight statements built from a fixed set of shapes.
/// Not guaranteed to be valid PHP; bounded to keep runs fast.
pub fn php_source() -> impl Strategy<Value = String> {
    let statement = prop_oneof![
        (php_identifier(), php_identifier()).prop_map(|(a, b)| format!("${a} = ${b};")),
        (php_identifier(), "[ -~&&[^'\\\\]]{0,12}").prop_map(|(a, s)| format!("${a} = '{s}';")),
        php_identifier().prop_map(|a| format!("echo ${a};")),
        (php_identifier(), php_identifier()).prop_map(|(f, a)| format!("{f}(${a});")),
        (php_identifier(), php_identifier())
            .prop_map(|(f, a)| format!("function {f}(${a}) {{ return ${a}; }}")),
        php_identifier().prop_map(|c| format!("class {c} {{ public $x; }}")),
        (php_identifier(), php_identifier())
            .prop_map(|(a, b)| format!("if (${a}) {{ echo ${b}; }}")),
    ];
    prop::collection::vec(statement, 0..=8).prop_map(|statements| {
        let mut text = String::from("<?php\n");
        for statement in statements {
            text.push_str(&statement);
            text.push('\n');
        }
        text
    })
}

/// A relative path of one to four segments: no leading slash, no `.` or `..` segment.
pub fn relative_path() -> impl Strategy<Value = String> {
    prop::collection::vec("[a-z0-9_][a-z0-9_.-]{0,7}", 1..=4)
        .prop_filter("a segment must not be . or ..", |segments| {
            segments.iter().all(|s| s != "." && s != "..")
        })
        .prop_map(|segments| segments.join("/"))
}

/// A `(major, minor, patch)` version triple with small components.
pub fn version_triple() -> impl Strategy<Value = (u32, u32, u32)> {
    (0u32..=9, 0u32..=99, 0u32..=99)
}

/// Arbitrary bytes, at most `max_len` of them.
pub fn byte_vec(max_len: usize) -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..=max_len)
}

/// Near-valid input: a valid base text with one random edit (delete a range, duplicate a range, replace a
/// byte, or truncate), returned as bytes because an edit may split a character.
pub fn mutated(base: impl Strategy<Value = String>) -> impl Strategy<Value = Vec<u8>> {
    (
        base,
        0usize..4,
        any::<prop::sample::Index>(),
        any::<prop::sample::Index>(),
        any::<u8>(),
    )
        .prop_map(|(text, kind, a, b, byte)| {
            let mut bytes = text.into_bytes();
            if bytes.is_empty() {
                return vec![byte];
            }
            let (start, end) = {
                let (x, y) = (a.index(bytes.len()), b.index(bytes.len()));
                (x.min(y), x.max(y))
            };
            match kind {
                0 => {
                    bytes.drain(start..=end);
                }
                1 => {
                    let copy = bytes[start..=end].to_vec();
                    bytes.splice(end..end, copy);
                }
                2 => bytes[start] = byte,
                _ => bytes.truncate(start),
            }
            bytes
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::{Settings, check_with};

    fn run<S: Strategy>(strategy: S, test: impl Fn(S::Value) -> Result<(), TestCaseError>)
    where
        S::Value: std::fmt::Debug,
    {
        check_with(
            Settings {
                seed: 1,
                cases: 128,
            },
            None,
            strategy,
            test,
        )
        .expect("property holds");
    }

    #[test]
    fn identifiers_do_not_start_with_a_digit() {
        run(php_identifier(), |id| {
            prop_assert!(!id.is_empty() && !id.starts_with(|c: char| c.is_ascii_digit()));
            Ok(())
        });
    }

    #[test]
    fn qualified_names_have_no_empty_segments() {
        run(php_qualified_name(), |name| {
            let body = name.strip_prefix('\\').unwrap_or(&name);
            prop_assert!(body.split('\\').all(|s| !s.is_empty()));
            Ok(())
        });
    }

    #[test]
    fn source_is_bounded_and_opens_with_a_tag() {
        run(php_source(), |text| {
            prop_assert!(text.starts_with("<?php\n") && text.len() < 2_000);
            Ok(())
        });
    }

    #[test]
    fn paths_are_relative_and_clean() {
        run(relative_path(), |path| {
            prop_assert!(!path.starts_with('/'));
            prop_assert!(
                path.split('/')
                    .all(|s| !s.is_empty() && s != "." && s != "..")
            );
            Ok(())
        });
    }

    #[test]
    fn byte_vectors_respect_their_bound() {
        run(byte_vec(10), |bytes| {
            prop_assert!(bytes.len() <= 10);
            Ok(())
        });
    }

    #[test]
    fn mutation_changes_or_shortens_the_base_for_some_inputs() {
        let changed = std::cell::Cell::new(false);
        run(mutated(php_source()), |bytes| {
            if !bytes.starts_with(b"<?php\n") {
                changed.set(true);
            }
            Ok(())
        });
        assert!(changed.get());
    }
}
