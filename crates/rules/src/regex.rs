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
}
