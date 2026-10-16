//! A tolerant scan of PHP text for the class-likes it declares, for `classmap` directories. It is
//! not a parser: it skips comments and strings and reads `namespace` and `class`-like keywords.

/// The fully qualified names of the classes, interfaces, traits and enums declared in `source`.
pub fn declared_classes(source: &str) -> Vec<String> {
    let tokens = tokens(source);
    let mut found = Vec::new();
    let mut namespace = String::new();
    let mut i = 0;
    while i < tokens.len() {
        let word = tokens[i].as_str();
        let previous = i.checked_sub(1).map(|p| tokens[p].as_str());
        match word.to_ascii_lowercase().as_str() {
            "namespace" if previous != Some("\\") => {
                if let Some(name) = tokens.get(i + 1).filter(|t| is_name(t)) {
                    namespace.clone_from(name);
                } else {
                    namespace.clear();
                }
            }
            "class" | "interface" | "trait" | "enum"
                if previous != Some("::") && previous != Some("new") && previous != Some("->") =>
            {
                if let Some(name) = tokens.get(i + 1).filter(|t| is_ident(t)) {
                    let reserved = ["extends", "implements", "{"];
                    if !reserved.contains(&name.to_ascii_lowercase().as_str()) {
                        found.push(if namespace.is_empty() {
                            name.clone()
                        } else {
                            format!("{namespace}\\{name}")
                        });
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    found
}

fn is_ident(token: &str) -> bool {
    token
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_' || !c.is_ascii())
        && token
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || !c.is_ascii())
}

fn is_name(token: &str) -> bool {
    !token.is_empty() && token.split('\\').all(is_ident)
}

/// Words (names may contain `\`), `::`, `->` and single punctuation characters; comments, strings
/// and inline HTML are dropped.
fn tokens(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut in_php = false;
    while i < bytes.len() {
        if !in_php {
            match find(bytes, i, b"<?") {
                Some(at) => {
                    i = at + 2;
                    if bytes[i..].starts_with(b"php") {
                        i += 3;
                    }
                    in_php = true;
                }
                None => break,
            }
            continue;
        }
        let c = bytes[i];
        match c {
            b'?' if bytes.get(i + 1) == Some(&b'>') => {
                in_php = false;
                i += 2;
            }
            b'#' if bytes.get(i + 1) != Some(&b'[') => i = line_end(bytes, i),
            b'/' if bytes.get(i + 1) == Some(&b'/') => i = line_end(bytes, i),
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i = find(bytes, i + 2, b"*/").map_or(bytes.len(), |at| at + 2);
            }
            b'\'' | b'"' => i = skip_string(bytes, i),
            b':' if bytes.get(i + 1) == Some(&b':') => {
                out.push("::".to_string());
                i += 2;
            }
            b'-' if bytes.get(i + 1) == Some(&b'>') => {
                out.push("->".to_string());
                i += 2;
            }
            _ if c.is_ascii_alphabetic() || c == b'_' || c == b'\\' || c >= 0x80 => {
                let start = i;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric()
                        || bytes[i] == b'_'
                        || bytes[i] == b'\\'
                        || bytes[i] >= 0x80)
                {
                    i += 1;
                }
                let word = &source[start..i];
                out.push(word.trim_start_matches('\\').to_string());
            }
            _ if c.is_ascii_whitespace() => i += 1,
            _ => {
                out.push((c as char).to_string());
                i += 1;
            }
        }
    }
    out
}

fn find(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    bytes
        .get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

fn line_end(bytes: &[u8], from: usize) -> usize {
    bytes[from..]
        .iter()
        .position(|b| *b == b'\n')
        .map_or(bytes.len(), |p| from + p + 1)
}

fn skip_string(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b if b == quote => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declarations_are_found_with_their_namespace() {
        let src = "<?php\nnamespace App\\Util;\n/* class Hidden {} */\n// class AlsoHidden\nfinal class Real extends Base {}\ninterface Contract {}\ntrait Helps {}\nenum Mode: int {}\n$x = 'class Nope {}';\n$y = Foo::class;\n$z = new class {};\n";
        assert_eq!(
            declared_classes(src),
            [
                "App\\Util\\Real",
                "App\\Util\\Contract",
                "App\\Util\\Helps",
                "App\\Util\\Mode"
            ]
        );
    }

    #[test]
    fn braced_namespaces_and_inline_html_work() {
        let src = "<html>class Not {}<?php\nnamespace A { class One {} }\nnamespace B { class Two {} }\n?>trailing class Nor {}";
        assert_eq!(declared_classes(src), ["A\\One", "B\\Two"]);
    }
}
