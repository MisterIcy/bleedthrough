//! Build guards: the C-preprocessor lines (`#if`, `#ifdef`, ...) that php-src puts in stub files.
//!
//! The lines are PHP `#` comments, so they are read from the tree's comments and each directive
//! has a byte offset. A guard stack turns them into the condition that holds at any offset.

use bleedthrough_syntax::{Comment, CommentKind};

/// One level of the guard stack: the branches seen so far in an `#if` group.
#[derive(Debug, Clone)]
struct Level {
    /// The own conditions of the earlier branches, to be negated for `#elif` and `#else`.
    earlier: Vec<String>,
    /// The own condition of the current branch (`None` after `#else`).
    own: Option<String>,
    /// What holds in the current branch: its own condition and the negation of the earlier ones.
    current: String,
}

/// The conditions holding at each point of one file.
#[derive(Debug, Clone, Default)]
pub struct GuardMap {
    /// `(offset, condition holding from that offset)`, sorted by offset.
    points: Vec<(u32, Option<String>)>,
}

/// A guard line that cannot be interpreted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardError {
    pub offset: u32,
    pub message: String,
}

impl GuardMap {
    /// Reads the directives among `comments` (in source order).
    pub fn scan(comments: &[Comment]) -> Result<Self, GuardError> {
        let mut stack: Vec<Level> = Vec::new();
        let mut points = Vec::new();
        for comment in comments.iter().filter(|c| c.kind == CommentKind::Hash) {
            let Some((directive, rest)) = split_directive(&comment.text) else {
                continue;
            };
            let at = comment.span.start;
            let error = |message: String| GuardError {
                offset: at,
                message,
            };
            match directive {
                "if" | "ifdef" | "ifndef" => {
                    let condition = condition_of(directive, rest).map_err(error)?;
                    stack.push(Level {
                        earlier: Vec::new(),
                        own: Some(condition.clone()),
                        current: condition,
                    });
                }
                "elif" | "else" => {
                    let level = stack
                        .last_mut()
                        .ok_or_else(|| error(format!("`#{directive}` without `#if`")))?;
                    level.earlier.extend(level.own.take());
                    let mut parts: Vec<String> = level.earlier.iter().map(|c| negate(c)).collect();
                    if directive == "elif" {
                        let own = condition_of("if", rest).map_err(error)?;
                        parts.push(own.clone());
                        level.own = Some(own);
                    }
                    level.current = join(&parts);
                }
                "endif" => {
                    stack
                        .pop()
                        .ok_or_else(|| error("`#endif` without `#if`".to_string()))?;
                }
                other => return Err(error(format!("unknown directive `#{other}`"))),
            }
            let active: Vec<String> = stack.iter().map(|l| l.current.clone()).collect();
            points.push((
                comment.span.end,
                (!active.is_empty()).then(|| join(&active)),
            ));
        }
        if !stack.is_empty() {
            return Err(GuardError {
                offset: comments.last().map_or(0, |c| c.span.end),
                message: "`#if` without `#endif`".to_string(),
            });
        }
        Ok(Self { points })
    }

    /// The condition that holds at `offset`, `None` when unguarded.
    pub fn at(&self, offset: u32) -> Option<&str> {
        let index = self.points.partition_point(|(from, _)| *from <= offset);
        index
            .checked_sub(1)
            .and_then(|i| self.points[i].1.as_deref())
    }

    /// Whether any directive lies strictly inside `start..end`.
    pub fn changes_within(&self, start: u32, end: u32) -> bool {
        self.points.iter().any(|(at, _)| *at > start && *at < end)
    }
}

/// `#  ifdef X` becomes `("ifdef", "X")`; `None` for a `#` comment that is not a directive.
fn split_directive(text: &str) -> Option<(&str, &str)> {
    let body = text.strip_prefix('#')?.trim_start();
    let end = body
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(body.len());
    let (word, rest) = body.split_at(end);
    matches!(word, "if" | "ifdef" | "ifndef" | "elif" | "else" | "endif").then_some((word, rest))
}

fn condition_of(directive: &str, rest: &str) -> Result<String, String> {
    let rest = strip_comments(rest);
    if rest.is_empty() {
        return Err(format!("`#{directive}` has no condition"));
    }
    Ok(match directive {
        "ifdef" => format!("defined({rest})"),
        "ifndef" => format!("!defined({rest})"),
        _ => rest,
    })
}

/// Removes `/* ... */` and `// ...` comments and collapses whitespace.
fn strip_comments(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        rest = match rest[start + 2..].find("*/") {
            Some(end) => &rest[start + 2 + end + 2..],
            None => "",
        };
    }
    out.push_str(rest);
    if let Some(at) = out.find("//") {
        out.truncate(at);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_atomic(condition: &str) -> bool {
    !condition.contains("&&") && !condition.contains("||") && !condition.contains(' ')
        || (condition.starts_with("defined(")
            && condition.ends_with(')')
            && !condition.contains(' '))
}

/// The negation of a condition, in the canonical spelling.
fn negate(condition: &str) -> String {
    if let Some(inner) = condition.strip_prefix('!')
        && is_atomic(inner)
    {
        return inner.to_string();
    }
    if is_atomic(condition) {
        format!("!{condition}")
    } else {
        format!("!({condition})")
    }
}

/// Conjunction of conditions; operands that contain `||` are parenthesised.
fn join(parts: &[String]) -> String {
    if let [only] = parts {
        return only.clone();
    }
    parts
        .iter()
        .map(|p| {
            if p.contains("||") {
                format!("({p})")
            } else {
                p.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" && ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bleedthrough_syntax::Span;

    /// One hash comment per line, each 10 bytes apart.
    fn map(lines: &[&str]) -> GuardMap {
        let comments: Vec<Comment> = lines
            .iter()
            .enumerate()
            .map(|(i, text)| Comment {
                kind: CommentKind::Hash,
                span: Span::new(i as u32 * 100, i as u32 * 100 + 50),
                text: (*text).into(),
            })
            .collect();
        GuardMap::scan(&comments).unwrap()
    }

    #[test]
    fn nested_guards_join_with_and() {
        let m = map(&["#ifdef A", "#if B >= 2", "#endif", "#endif"]);
        assert_eq!(m.at(10), None);
        assert_eq!(m.at(60), Some("defined(A)"));
        assert_eq!(m.at(160), Some("defined(A) && B >= 2"));
        assert_eq!(m.at(260), Some("defined(A)"));
        assert_eq!(m.at(360), None);
    }

    #[test]
    fn else_negates_every_earlier_branch() {
        let m = map(&["#ifdef A", "#elif B", "#else", "#endif /* A */"]);
        assert_eq!(m.at(60), Some("defined(A)"));
        assert_eq!(m.at(160), Some("!defined(A) && B"));
        assert_eq!(m.at(260), Some("!defined(A) && !B"));
        assert_eq!(m.at(360), None);
    }

    #[test]
    fn compound_conditions_are_parenthesised_where_needed() {
        let m = map(&["#if X || Y", "#else", "#endif"]);
        assert_eq!(m.at(60), Some("X || Y"));
        assert_eq!(m.at(160), Some("!(X || Y)"));
    }

    #[test]
    fn trailing_comments_and_spacing_are_removed() {
        let m = map(&["#  if  defined(A)   /* why */", "# endif"]);
        assert_eq!(m.at(60), Some("defined(A)"));
    }

    #[test]
    fn non_directives_are_ignored_and_broken_groups_fail() {
        let m = map(&["# a plain comment", "#ifdef A", "#endif"]);
        assert_eq!(m.at(160), Some("defined(A)"));
        let bad = |text: &str| {
            let c = Comment {
                kind: CommentKind::Hash,
                span: Span::new(0, 10),
                text: text.into(),
            };
            GuardMap::scan(&[c]).unwrap_err().message
        };
        assert!(bad("#endif").contains("without"));
        assert!(bad("#if").contains("no condition"));
        assert!(bad("#ifdef A").contains("without `#endif`"));
    }

    #[test]
    fn a_directive_inside_a_span_is_detected() {
        let m = map(&["#ifdef A", "#endif"]);
        assert!(m.changes_within(40, 120));
        assert!(!m.changes_within(60, 99));
    }
}
