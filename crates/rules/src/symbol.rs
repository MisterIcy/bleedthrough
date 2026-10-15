use std::fmt;

/// Term: Rule
///
/// The closed vocabulary of sink classes. A sanitizer clears taint only for the classes it
/// declares. Extending the vocabulary means editing this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum SinkClass {
    /// An SQL query.
    Sql,
    /// An operating-system command.
    OsCommand,
    /// Code evaluation.
    CodeEval,
    /// A file-system path.
    FilePath,
    /// A file inclusion (`include`, `require`).
    FileInclusion,
    /// Deserialization.
    Deserialization,
    /// XML parsing.
    XmlParsing,
    /// An outgoing network request.
    OutgoingRequest,
    /// A redirect target.
    RedirectTarget,
    /// An HTTP header.
    Header,
    /// An LDAP query.
    LdapQuery,
    /// An XPath query.
    XpathQuery,
    /// A log entry.
    Log,
}

impl SinkClass {
    const ALL: [(Self, &'static str); 13] = [
        (Self::Sql, "sql"),
        (Self::OsCommand, "os-command"),
        (Self::CodeEval, "code-eval"),
        (Self::FilePath, "file-path"),
        (Self::FileInclusion, "file-inclusion"),
        (Self::Deserialization, "deserialization"),
        (Self::XmlParsing, "xml-parsing"),
        (Self::OutgoingRequest, "outgoing-request"),
        (Self::RedirectTarget, "redirect-target"),
        (Self::Header, "header"),
        (Self::LdapQuery, "ldap-query"),
        (Self::XpathQuery, "xpath-query"),
        (Self::Log, "log"),
    ];

    /// The name used in rule files.
    pub fn as_str(&self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(c, _)| c == self)
            .map_or("", |(_, n)| n)
    }

    /// Parses a name from the vocabulary; an unknown name is an error naming it.
    pub fn parse(name: &str) -> Result<Self, String> {
        Self::ALL
            .iter()
            .find(|(_, n)| *n == name)
            .map(|(c, _)| *c)
            .ok_or_else(|| {
                let known: Vec<&str> = Self::ALL.iter().map(|(_, n)| *n).collect();
                format!(
                    "unknown sink class {name:?}; known classes: {}",
                    known.join(", ")
                )
            })
    }
}

impl fmt::Display for SinkClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Term: Rule
///
/// What kind of PHP symbol a [`SymbolRef`] names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum SymbolKind {
    /// A function, global or namespaced.
    Function,
    /// A static method, `Class::method`.
    StaticMethod,
    /// An instance method, `Class->method`.
    InstanceMethod,
    /// A constructor, `new Class`.
    Constructor,
    /// A class, `class Name`, for use patterns.
    Class,
    /// A superglobal such as `$_GET`.
    Superglobal,
}

const SUPERGLOBALS: [&str; 9] = [
    "$_GET",
    "$_POST",
    "$_COOKIE",
    "$_FILES",
    "$_REQUEST",
    "$_SERVER",
    "$_ENV",
    "$_SESSION",
    "$GLOBALS",
];

/// Term: Rule
///
/// A reference to a PHP symbol by name, as written with its namespace; no resolution happens
/// here. A leading backslash on a global name is dropped. Text forms: `func`, `\ns\func`, `\ns\Class::method` (static), `\ns\Class->method`
/// (instance; a trailing `+` also matches subclasses), `new \ns\Class` (constructor),
/// `class \ns\Class` and `$_GET` (superglobal).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymbolRef {
    kind: SymbolKind,
    namespace: Vec<String>,
    class: Option<String>,
    name: String,
    include_subclasses: bool,
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || !c.is_ascii())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || !c.is_ascii())
}

/// Splits `\a\b\name` into the namespace segments and the last segment.
fn split_qualified(text: &str) -> Result<(Vec<String>, String), String> {
    let trimmed = text.strip_prefix('\\').unwrap_or(text);
    let mut parts: Vec<&str> = trimmed.split('\\').collect();
    if parts.iter().any(|p| !is_ident(p)) {
        return Err(format!("invalid PHP name {text:?}"));
    }
    let last = parts.pop().unwrap_or_default().to_string();
    Ok((parts.into_iter().map(str::to_string).collect(), last))
}

impl SymbolRef {
    /// Parses one of the text forms; the error says what is wrong.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if let Some(rest) = text.strip_prefix("new ") {
            let (ns, class) = split_qualified(rest.trim())?;
            return Ok(Self::new(
                SymbolKind::Constructor,
                ns,
                Some(class),
                "__construct",
            ));
        }
        if let Some(rest) = text.strip_prefix("class ") {
            let (ns, class) = split_qualified(rest.trim())?;
            return Ok(Self::new(SymbolKind::Class, ns, Some(class), ""));
        }
        if text.starts_with('$') {
            return if SUPERGLOBALS.contains(&text) {
                Ok(Self::new(SymbolKind::Superglobal, vec![], None, text))
            } else {
                Err(format!("{text:?} is not a PHP superglobal"))
            };
        }
        let (body, subclasses) = match text.strip_suffix('+') {
            Some(b) => (b, true),
            None => (text, false),
        };
        if let Some((class, method)) = body.split_once("->") {
            return Self::method(SymbolKind::InstanceMethod, class, method, subclasses);
        }
        if let Some((class, method)) = body.split_once("::") {
            if method == "__construct" {
                return Self::method(SymbolKind::Constructor, class, method, subclasses);
            }
            return Self::method(SymbolKind::StaticMethod, class, method, subclasses);
        }
        if subclasses {
            return Err(format!(
                "`+` (include subclasses) applies to methods only: {text:?}"
            ));
        }
        let (ns, name) = split_qualified(body)?;
        if ns.is_empty() && matches!(name.as_str(), "new" | "class") {
            return Err(format!("`{name}` needs a class name after it"));
        }
        Ok(Self::new(SymbolKind::Function, ns, None, &name))
    }

    fn method(
        kind: SymbolKind,
        class: &str,
        method: &str,
        subclasses: bool,
    ) -> Result<Self, String> {
        let (ns, class) = split_qualified(class)?;
        if !is_ident(method) {
            return Err(format!("invalid method name {method:?}"));
        }
        let mut symbol = Self::new(kind, ns, Some(class), method);
        symbol.include_subclasses = subclasses;
        Ok(symbol)
    }

    fn new(kind: SymbolKind, namespace: Vec<String>, class: Option<String>, name: &str) -> Self {
        Self {
            kind,
            namespace,
            class,
            name: name.to_string(),
            include_subclasses: false,
        }
    }

    /// The kind of symbol.
    pub fn kind(&self) -> SymbolKind {
        self.kind
    }

    /// The namespace segments, outermost first; empty for the global namespace.
    pub fn namespace(&self) -> &[String] {
        &self.namespace
    }

    /// The class name, for methods, constructors and classes.
    pub fn class(&self) -> Option<&str> {
        self.class.as_deref()
    }

    /// The function or method name, or the superglobal; empty for a class.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether a method also matches the same method of subclasses.
    pub fn include_subclasses(&self) -> bool {
        self.include_subclasses
    }
}

impl fmt::Display for SymbolRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let qualified = |f: &mut fmt::Formatter<'_>, last: &str| {
            for segment in &self.namespace {
                write!(f, "\\{segment}")?;
            }
            if self.namespace.is_empty() {
                f.write_str(last)
            } else {
                write!(f, "\\{last}")
            }
        };
        match self.kind {
            SymbolKind::Function => qualified(f, &self.name),
            SymbolKind::Superglobal => f.write_str(&self.name),
            SymbolKind::Class => {
                f.write_str("class ")?;
                qualified(f, self.class.as_deref().unwrap_or_default())
            }
            SymbolKind::Constructor => {
                f.write_str("new ")?;
                qualified(f, self.class.as_deref().unwrap_or_default())
            }
            SymbolKind::StaticMethod | SymbolKind::InstanceMethod => {
                qualified(f, self.class.as_deref().unwrap_or_default())?;
                let arrow = if self.kind == SymbolKind::StaticMethod {
                    "::"
                } else {
                    "->"
                };
                write!(f, "{arrow}{}", self.name)?;
                if self.include_subclasses {
                    f.write_str("+")
                } else {
                    Ok(())
                }
            }
        }
    }
}

/// Term: Rule
///
/// Which argument of a call a rule refers to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ArgSel {
    /// A position, counting from 1.
    Position(u32),
    /// A parameter name, without `$`.
    Name(String),
    /// The object a method is called on (propagators of instance methods only).
    Receiver,
}

impl ArgSel {
    /// Parses `1`, `2`, ..., a parameter name, or `this` for the receiver.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text == "this" {
            return Ok(Self::Receiver);
        }
        if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) {
            return match text.parse::<u32>() {
                Ok(n) if n >= 1 => Ok(Self::Position(n)),
                _ => Err(format!("argument position {text:?} must be 1 or more")),
            };
        }
        if is_ident(text) {
            Ok(Self::Name(text.to_string()))
        } else {
            Err(format!("invalid argument selector {text:?}"))
        }
    }
}

impl fmt::Display for ArgSel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Position(n) => write!(f, "{n}"),
            Self::Name(n) => f.write_str(n),
            Self::Receiver => f.write_str("this"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(text: &str) -> SymbolRef {
        let s = SymbolRef::parse(text).unwrap();
        assert_eq!(s.to_string(), text);
        s
    }

    #[test]
    fn the_symbol_forms_parse() {
        let f = round_trip("mysqli_query");
        assert_eq!(
            (f.kind(), f.name(), f.namespace().len()),
            (SymbolKind::Function, "mysqli_query", 0)
        );
        let f = round_trip("\\App\\Db\\run");
        assert_eq!(
            (f.namespace(), f.name()),
            (&["App".to_string(), "Db".to_string()][..], "run")
        );
        let s = round_trip("PDO::quote");
        assert_eq!(
            (s.kind(), s.class(), s.name()),
            (SymbolKind::StaticMethod, Some("PDO"), "quote")
        );
        let i = round_trip("PDO->query+");
        assert!(i.include_subclasses() && i.kind() == SymbolKind::InstanceMethod);
        assert!(!round_trip("PDO->query").include_subclasses());
        let c = round_trip("new PDO");
        assert_eq!(
            (c.kind(), c.class(), c.name()),
            (SymbolKind::Constructor, Some("PDO"), "__construct")
        );
        assert_eq!(
            SymbolRef::parse("\\PDO::__construct").unwrap().kind(),
            SymbolKind::Constructor
        );
        assert_eq!(round_trip("class Phar").kind(), SymbolKind::Class);
        assert_eq!(round_trip("$_GET").kind(), SymbolKind::Superglobal);
    }

    #[test]
    fn bad_symbols_are_rejected() {
        for bad in [
            "", "$foo", "a b", "a::", "::b", "f+", "new ", "\\\\f", "1abc", "A->b-c",
        ] {
            assert!(SymbolRef::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn sink_classes_are_a_closed_vocabulary() {
        for (class, name) in SinkClass::ALL {
            assert_eq!(SinkClass::parse(name).unwrap(), class);
            assert_eq!(class.to_string(), name);
        }
        assert!(SinkClass::parse("xss").unwrap_err().contains("\"xss\""));
    }

    #[test]
    fn argument_selectors() {
        assert_eq!(ArgSel::parse("2").unwrap(), ArgSel::Position(2));
        assert_eq!(
            ArgSel::parse("query").unwrap(),
            ArgSel::Name("query".into())
        );
        assert_eq!(ArgSel::parse("this").unwrap(), ArgSel::Receiver);
        assert!(
            ArgSel::parse("0").is_err()
                && ArgSel::parse("").is_err()
                && ArgSel::parse("a-b").is_err()
        );
    }
}
