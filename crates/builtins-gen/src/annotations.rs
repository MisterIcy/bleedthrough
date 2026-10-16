//! Docblock annotations (`@param`, `@tentative-return-type`, ...) and the vocabulary check.
//!
//! Only the facts an annotation carries are kept; explanatory prose in a docblock is discarded.
//! Every tag in the stubs must be in [`TAGS`], either as a fact the generator uses or as a tag that
//! is skipped on purpose with a reason. A tag that is in neither is an error, so that a new
//! annotation in a future PHP release is noticed instead of silently ignored.

use bleedthrough_syntax::Comment;

/// What the generator does with a tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Use {
    /// The tag carries a fact that ends up in the data.
    Fact,
    /// The tag is understood and deliberately not stored; the reason is reviewed with this table.
    Skipped(&'static str),
}

/// The annotation vocabulary of the stubs of PHP 8.0 to 8.5.
pub const TAGS: &[(&str, Use)] = &[
    ("param", Use::Fact),
    ("return", Use::Fact),
    ("var", Use::Fact),
    ("tentative-return-type", Use::Fact),
    ("alias", Use::Fact),
    ("implementation-alias", Use::Fact),
    ("deprecated", Use::Fact),
    ("readonly", Use::Fact),
    ("virtual", Use::Fact),
    (
        "cvalue",
        Use::Fact, // the value is a C expression: the constant has no PHP-visible value text
    ),
    (
        "genstubs-expose-comment-block",
        Use::Skipped("the block that follows is documentation prose exposed to Reflection"),
    ),
    (
        "refcount",
        Use::Skipped("a C-level hint about the returned value's reference count"),
    ),
    (
        "generate-class-entries",
        Use::Skipped("a build directive of the stub generator"),
    ),
    (
        "generate-function-entries",
        Use::Skipped("a build directive of the stub generator"),
    ),
    (
        "generate-legacy-arginfo",
        Use::Skipped("a build directive of the stub generator"),
    ),
    (
        "no-file-cache",
        Use::Skipped("a build directive of the stub generator"),
    ),
    (
        "no-verify",
        Use::Skipped("disables a consistency check of the stub generator"),
    ),
    (
        "undocumentable",
        Use::Skipped("tells the documentation tools to skip the symbol"),
    ),
    (
        "compile-time-eval",
        Use::Skipped("an optimiser hint (pure function); not modelled in the data"),
    ),
    (
        "frameless-function",
        Use::Skipped("an optimiser hint for the VM; not modelled in the data"),
    ),
    (
        "not-serializable",
        Use::Skipped("a runtime restriction on serialising instances; not modelled in the data"),
    ),
    (
        "strict-properties",
        Use::Skipped("forbids dynamic properties at runtime; not modelled in the data"),
    ),
    (
        "prefer-ref",
        Use::Skipped("a by-reference preference of one parameter; not modelled in the data"),
    ),
    ("link", Use::Skipped("a documentation link")),
    ("see", Use::Skipped("a documentation reference")),
    (
        "since",
        Use::Skipped("documentation of when a stub was added"),
    ),
    ("todo", Use::Skipped("a note for the maintainers")),
    (
        "internal",
        Use::Skipped("marks a stub for internal testing use"),
    ),
];

/// One `@tag rest` entry of a docblock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    /// The text after the tag name, continuation lines joined by a space.
    pub rest: String,
}

/// The tags of one docblock.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Docblock {
    pub tags: Vec<Tag>,
}

impl Docblock {
    /// Parses the tags of a `/** ... */` comment; lines that do not start with `@` are prose.
    pub fn parse(text: &str) -> Self {
        let body = text
            .strip_prefix("/**")
            .unwrap_or(text)
            .trim_end_matches("*/");
        let mut tags: Vec<Tag> = Vec::new();
        let mut exposed = false;
        for line in body.lines() {
            let line = line.trim().trim_start_matches('*').trim();
            if let Some(tag) = line.strip_prefix('@') {
                let end = tag.find(|c: char| c.is_whitespace()).unwrap_or(tag.len());
                let (name, rest) = tag.split_at(end);
                if name == "genstubs-expose-comment-block" {
                    exposed = true;
                }
                if !exposed || name == "genstubs-expose-comment-block" {
                    tags.push(Tag {
                        name: name.to_string(),
                        rest: rest.trim().to_string(),
                    });
                }
            } else if !line.is_empty()
                && !exposed
                && let Some(last) = tags.last_mut()
            {
                last.rest.push(' ');
                last.rest.push_str(line);
            }
        }
        Self { tags }
    }

    /// The first tag with this name.
    pub fn tag(&self, name: &str) -> Option<&Tag> {
        self.tags.iter().find(|t| t.name == name)
    }

    /// Whether the tag is present.
    pub fn has(&self, name: &str) -> bool {
        self.tag(name).is_some()
    }

    /// The `@param` type of the parameter `$name`, if documented.
    pub fn param_type(&self, name: &str) -> Result<Option<String>, String> {
        for tag in self.tags.iter().filter(|t| t.name == "param") {
            let (ty, rest) = split_type(&tag.rest)?;
            let variable = rest
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_start_matches('&')
                .trim_start_matches("...");
            if variable.strip_prefix('$') == Some(name) {
                return Ok(Some(ty));
            }
        }
        Ok(None)
    }

    /// The type after `@return` or `@var`, if present.
    pub fn type_of(&self, tag: &str) -> Result<Option<String>, String> {
        self.tag(tag)
            .map(|t| split_type(&t.rest).map(|(ty, _)| ty))
            .transpose()
    }
}

/// Splits `array<int, string> $x prose` into the type and the rest, balancing brackets.
fn split_type(text: &str) -> Result<(String, &str), String> {
    let mut depth = 0i32;
    let mut end = text.len();
    for (index, c) in text.char_indices() {
        match c {
            '<' | '(' | '{' | '[' => depth += 1,
            '>' | ')' | '}' | ']' => depth -= 1,
            c if c.is_whitespace() && depth <= 0 => {
                end = index;
                break;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(format!("unbalanced brackets in the type `{text}`"));
    }
    let (ty, rest) = text.split_at(end);
    if ty.is_empty() {
        return Err("the annotation has no type".to_string());
    }
    Ok((ty.to_string(), rest.trim_start()))
}

/// Checks the tags of a docblock against the vocabulary; the names that are in neither list.
pub fn unknown_tags(comment: &Comment) -> Vec<String> {
    Docblock::parse(&comment.text)
        .tags
        .into_iter()
        .filter(|t| !TAGS.iter().any(|(name, _)| *name == t.name))
        .map(|t| t.name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bleedthrough_syntax::{CommentKind, Span};

    fn comment(text: &str) -> Comment {
        Comment {
            kind: CommentKind::Doc,
            span: Span::new(0, text.len() as u32),
            text: text.into(),
        }
    }

    #[test]
    fn every_annotation_of_the_spec_is_read() {
        let doc = Docblock::parse(
            "/**\n * @param array<int, string> $items the items\n * @param resource $stream\n * @return string|false\n * @tentative-return-type\n * @alias other_name\n * @implementation-alias A::b\n * @deprecated\n * @readonly\n * @virtual\n * @cvalue ZEND_X\n * @var int\n * @refcount 1\n */",
        );
        assert_eq!(
            doc.param_type("items").unwrap().as_deref(),
            Some("array<int, string>")
        );
        assert_eq!(
            doc.param_type("stream").unwrap().as_deref(),
            Some("resource")
        );
        assert_eq!(doc.param_type("missing").unwrap(), None);
        assert_eq!(
            doc.type_of("return").unwrap().as_deref(),
            Some("string|false")
        );
        assert_eq!(doc.type_of("var").unwrap().as_deref(), Some("int"));
        assert_eq!(doc.tag("alias").unwrap().rest, "other_name");
        assert_eq!(doc.tag("implementation-alias").unwrap().rest, "A::b");
        for tag in [
            "tentative-return-type",
            "deprecated",
            "readonly",
            "virtual",
            "cvalue",
            "refcount",
        ] {
            assert!(doc.has(tag), "{tag}");
        }
        assert!(unknown_tags(&comment("/** @param int $x\n @refcount 1 */")).is_empty());
    }

    #[test]
    fn a_tag_the_generator_does_not_know_is_reported() {
        assert_eq!(
            unknown_tags(&comment("/** @brand-new-tag x */")),
            ["brand-new-tag"]
        );
    }

    #[test]
    fn prose_is_not_kept() {
        let doc = Docblock::parse(
            "/**\n * Explains at length what this does.\n * @param int $a The first number to add, secret-prose\n * @return int\n */",
        );
        let kept = format!("{doc:?}");
        // The continuation text is part of the rest of the tag, which only the type reader uses.
        assert_eq!(doc.param_type("a").unwrap().as_deref(), Some("int"));
        assert!(!kept.contains("Explains"));
        assert!(doc.tag("param").unwrap().rest.starts_with("int $a"));
    }

    #[test]
    fn the_exposed_comment_block_is_not_scanned() {
        let doc = Docblock::parse(
            "/**\n * @genstubs-expose-comment-block\n * Some text\n * @unknown-inside\n */",
        );
        assert_eq!(doc.tags.len(), 1);
    }

    #[test]
    fn malformed_types_are_errors() {
        assert!(
            Docblock::parse("/** @param array<int $x */")
                .param_type("x")
                .is_err()
        );
        assert!(Docblock::parse("/** @return */").type_of("return").is_err());
    }
}
