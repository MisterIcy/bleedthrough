//! The restricted regular-expression subset allowed in argument conditions.
//!
//! Allowed: literal characters, `.`, character classes (`[a-z0-9_]`, optionally negated, with
//! ranges and escapes), the anchors `^` and `$`, escapes of punctuation and of `\d \w \s`, and a
//! quantifier (`*`, `+`, `?` or bounded `{m}`, `{m,n}`) on a single atom. Groups, alternation,
//! back-references, look-around and unbounded `{m,}` are rejected, so no pattern can nest
//! quantifiers and every pattern can be matched in linear time by any engine.

/// The largest repetition bound accepted.
const MAX_BOUND: u32 = 1000;

/// Checks a pattern against the subset; the error names the offending construct and position.
pub fn validate_regex(pattern: &str) -> Result<(), String> {
    if pattern.is_empty() {
        return Err("regular expression is empty".to_string());
    }
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    let mut quantifiable = false;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '(' | ')' | '|' => {
                return Err(format!(
                    "`{c}` (groups and alternation) at {i} is outside the allowed subset"
                ));
            }
            '*' | '+' | '?' | '{' => {
                if !quantifiable {
                    return Err(format!("quantifier `{c}` at {i} has nothing to repeat"));
                }
                i = quantifier_end(&chars, i)?;
                quantifiable = false;
                continue;
            }
            '[' => {
                i = class_end(&chars, i)?;
                quantifiable = true;
                continue;
            }
            '\\' => {
                escape_ok(&chars, i)?;
                i += 2;
                quantifiable = true;
                continue;
            }
            '^' | '$' => quantifiable = false,
            ']' | '}' => return Err(format!("unmatched `{c}` at {i}")),
            _ => quantifiable = true,
        }
        i += 1;
    }
    Ok(())
}

fn escape_ok(chars: &[char], i: usize) -> Result<(), String> {
    match chars.get(i + 1) {
        Some(c) if c.is_ascii_digit() && *c != '0' => Err(format!(
            "back-reference `\\{c}` at {i} is outside the allowed subset"
        )),
        Some('b' | 'B' | 'A' | 'z' | 'Z' | 'p' | 'P' | 'k') => Err(format!(
            "escape `\\{}` at {i} is outside the allowed subset",
            chars[i + 1]
        )),
        Some(_) => Ok(()),
        None => Err("pattern ends with a lone `\\`".to_string()),
    }
}

fn class_end(chars: &[char], start: usize) -> Result<usize, String> {
    let mut i = start + 1;
    if chars.get(i) == Some(&'^') {
        i += 1;
    }
    let body_start = i;
    while i < chars.len() {
        match chars[i] {
            '\\' => {
                escape_ok(chars, i)?;
                i += 2;
            }
            '[' => {
                return Err(format!(
                    "`[` inside a class at {i} is outside the allowed subset"
                ));
            }
            ']' if i > body_start => return Ok(i + 1),
            _ => i += 1,
        }
    }
    Err(format!("character class opened at {start} is not closed"))
}

fn quantifier_end(chars: &[char], i: usize) -> Result<usize, String> {
    let mut end = if chars[i] == '{' {
        braces_end(chars, i)?
    } else {
        i + 1
    };
    if chars.get(end) == Some(&'?') {
        end += 1; // lazy variant: same language, still linear
    }
    if matches!(chars.get(end), Some('*' | '+' | '?' | '{')) {
        return Err(format!(
            "stacked quantifier at {end} is outside the allowed subset"
        ));
    }
    Ok(end)
}

fn braces_end(chars: &[char], start: usize) -> Result<usize, String> {
    let close = chars[start..]
        .iter()
        .position(|c| *c == '}')
        .ok_or_else(|| format!("repetition opened at {start} is not closed"))?
        + start;
    let inner: String = chars[start + 1..close].iter().collect();
    let bound = |s: &str| {
        s.parse::<u32>()
            .ok()
            .filter(|n| *n <= MAX_BOUND && !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
    };
    let ok = match inner.split_once(',') {
        Some((lo, hi)) => matches!((bound(lo), bound(hi)), (Some(a), Some(b)) if a <= b),
        None => bound(&inner).is_some(),
    };
    if ok {
        Ok(close + 1)
    } else {
        Err(format!(
            "repetition `{{{inner}}}` at {start} must be `{{m}}` or `{{m,n}}` with bounds up to {MAX_BOUND}"
        ))
    }
}

/// One character test.
#[derive(Debug, Clone)]
enum Class {
    Any,
    Char(char),
    Set { negated: bool, items: Vec<Item> },
}

#[derive(Debug, Clone)]
enum Item {
    Range(char, char),
    Digit,
    Word,
    Space,
    NotDigit,
    NotWord,
    NotSpace,
}

#[derive(Debug, Clone)]
enum Step {
    Start,
    End,
    Repeat { class: Class, min: u32, max: u32 },
}

fn escape_item(c: char) -> Item {
    match c {
        'd' => Item::Digit,
        'w' => Item::Word,
        's' => Item::Space,
        'D' => Item::NotDigit,
        'W' => Item::NotWord,
        'S' => Item::NotSpace,
        other => {
            let c = literal_escape(other);
            Item::Range(c, c)
        }
    }
}

fn literal_escape(c: char) -> char {
    match c {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        'f' => '\u{c}',
        'v' => '\u{b}',
        'e' => '\u{1b}',
        '0' => '\0',
        other => other,
    }
}

impl Item {
    fn accepts(&self, c: char) -> bool {
        match self {
            Self::Range(lo, hi) => (*lo..=*hi).contains(&c),
            Self::Digit => c.is_ascii_digit(),
            Self::Word => c.is_ascii_alphanumeric() || c == '_',
            Self::Space => matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{b}' | '\u{c}'),
            Self::NotDigit => !c.is_ascii_digit(),
            Self::NotWord => !(c.is_ascii_alphanumeric() || c == '_'),
            Self::NotSpace => !matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{b}' | '\u{c}'),
        }
    }
}

impl Class {
    fn accepts(&self, c: char) -> bool {
        match self {
            Self::Any => c != '\n',
            Self::Char(x) => *x == c,
            Self::Set { negated, items } => items.iter().any(|i| i.accepts(c)) != *negated,
        }
    }
}

/// Parses a pattern the validator accepts. `None` for one it would reject.
fn parse_steps(pattern: &str) -> Option<Vec<Step>> {
    validate_regex(pattern).ok()?;
    let chars: Vec<char> = pattern.chars().collect();
    let mut steps: Vec<Step> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let class = match chars[i] {
            '^' => {
                steps.push(Step::Start);
                i += 1;
                continue;
            }
            '$' => {
                steps.push(Step::End);
                i += 1;
                continue;
            }
            '.' => {
                i += 1;
                Class::Any
            }
            '[' => {
                let end = class_end(&chars, i).ok()?;
                let class = parse_set(&chars[i + 1..end - 1]);
                i = end;
                class
            }
            '\\' => {
                let c = chars[i + 1];
                i += 2;
                match escape_item(c) {
                    Item::Range(x, _) => Class::Char(x),
                    item => Class::Set {
                        negated: false,
                        items: vec![item],
                    },
                }
            }
            c => {
                i += 1;
                Class::Char(c)
            }
        };
        let (min, max, next) = match chars.get(i) {
            Some('*') => (0, u32::MAX, i + 1),
            Some('+') => (1, u32::MAX, i + 1),
            Some('?') => (0, 1, i + 1),
            Some('{') => {
                let close = i + chars[i..].iter().position(|c| *c == '}')?;
                let inner: String = chars[i + 1..close].iter().collect();
                let (lo, hi) = match inner.split_once(',') {
                    Some((lo, hi)) => (lo.parse().ok()?, hi.parse().ok()?),
                    None => {
                        let n = inner.parse().ok()?;
                        (n, n)
                    }
                };
                (lo, hi, close + 1)
            }
            _ => (1, 1, i),
        };
        i = if chars.get(next) == Some(&'?') && next != i {
            next + 1
        } else {
            next
        };
        steps.push(Step::Repeat { class, min, max });
    }
    Some(steps)
}

fn parse_set(body: &[char]) -> Class {
    let (negated, body) = match body.first() {
        Some('^') => (true, &body[1..]),
        _ => (false, body),
    };
    let mut items = Vec::new();
    let mut i = 0;
    while i < body.len() {
        let first = if body[i] == '\\' && i + 1 < body.len() {
            i += 2;
            escape_item(body[i - 1])
        } else {
            i += 1;
            Item::Range(body[i - 1], body[i - 1])
        };
        if let Item::Range(lo, _) = first
            && body.get(i) == Some(&'-')
            && i + 1 < body.len()
        {
            let hi = if body[i + 1] == '\\' && i + 2 < body.len() {
                i += 3;
                literal_escape(body[i - 1])
            } else {
                i += 2;
                body[i - 1]
            };
            items.push(Item::Range(lo, hi));
        } else {
            items.push(first);
        }
    }
    Class::Set { negated, items }
}

/// Whether `text` contains a match of `pattern` (anchors `^` and `$` pin the ends), for a pattern
/// in the allowed subset; a pattern outside it never matches. Runs in time proportional to the
/// text, the pattern and the repetition bounds, whatever the input.
pub fn regex_matches(pattern: &str, text: &str) -> bool {
    let Some(steps) = parse_steps(pattern) else {
        return false;
    };
    let text: Vec<char> = text.chars().collect();
    let n = text.len();
    let mut reach = vec![true; n + 1];
    for step in &steps {
        let mut next = vec![false; n + 1];
        match step {
            Step::Start => next[0] = reach[0],
            Step::End => next[n] = reach[n],
            Step::Repeat { class, min, max } => {
                for (from, live) in reach.iter().enumerate() {
                    if !live {
                        continue;
                    }
                    let mut taken = 0u32;
                    let mut at = from;
                    loop {
                        if taken >= *min {
                            next[at] = true;
                        }
                        if taken >= *max || at >= n || !class.accepts(text[at]) {
                            break;
                        }
                        taken += 1;
                        at += 1;
                    }
                }
            }
        }
        reach = next;
    }
    reach.iter().any(|r| *r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_allowed_subset_is_accepted() {
        for ok in [
            "^abc$",
            "a.c",
            "[a-z0-9_]+",
            "[^/]*",
            "\\.php$",
            "^[A-Za-z_]\\w{0,63}$",
            "\\d{3}",
            "a+?",
            "x{2,5}",
            "[\\]a]",
        ] {
            assert_eq!(validate_regex(ok), Ok(()), "{ok}");
        }
    }

    #[test]
    fn everything_else_is_rejected() {
        for bad in [
            "",
            "(a)",
            "a|b",
            "(a+)+",
            "a**",
            "a{2,}",
            "a{1001}",
            "a{3,2}",
            "*a",
            "^*",
            "\\1",
            "\\bword",
            "[a",
            "a]",
            "a\\",
            "a{",
            "(?=a)",
            "a{x}",
            "a{1}{2}",
            "[[:alpha:]]",
        ] {
            assert!(validate_regex(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn matching_follows_the_subset() {
        assert!(regex_matches("^[A-Za-z_]{1,32}$", "session_id"));
        assert!(!regex_matches("^[A-Za-z_]{1,32}$", "session-id"));
        assert!(!regex_matches("^[A-Za-z_]{1,3}$", "abcd"));
        assert!(regex_matches("abc", "xxabcxx"));
        assert!(!regex_matches("^abc", "xabc"));
        assert!(regex_matches("a.c$", "xxabc"));
        assert!(regex_matches("\\d+\\.php", "v12.php"));
        assert!(regex_matches("[^0-9]+", "ab"));
        assert!(!regex_matches("^[^0-9]+$", "a1"));
        assert!(regex_matches("colou?r", "color"));
        assert!(
            !regex_matches("(a)", "a"),
            "outside the subset never matches"
        );
    }

    #[test]
    fn matching_is_linear_on_pathological_input() {
        let text = "a".repeat(5_000);
        assert!(!regex_matches("^a*a*a*a*a*b$", &text));
    }
}
