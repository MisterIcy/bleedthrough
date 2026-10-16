//! Fully qualified names and the outcome of resolving a written name.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};

use bleedthrough_syntax::Span;

/// Term: Fully qualified name
///
/// A name with its namespace written out and no leading backslash. Equality, ordering and hashing
/// use a lower-cased key (PHP matches class and function names case-insensitively); the spelling it
/// was declared with is kept for display. Constants are case-sensitive in their last segment: use
/// [`Fqn::constant_key`] for those.
#[derive(Debug, Clone)]
pub struct Fqn {
    display: Box<str>,
    key: Box<str>,
}

impl Fqn {
    /// A name from its text; a leading `\` is dropped.
    pub fn new(text: &str) -> Self {
        let text = text.strip_prefix('\\').unwrap_or(text);
        Self {
            display: text.into(),
            key: text.to_ascii_lowercase().into(),
        }
    }

    /// The name of `short` inside `namespace` (the global namespace is the empty string).
    pub fn in_namespace(namespace: &str, short: &str) -> Self {
        if namespace.is_empty() {
            Self::new(short)
        } else {
            Self::new(&format!("{namespace}\\{short}"))
        }
    }

    /// The declared spelling.
    pub fn as_str(&self) -> &str {
        &self.display
    }

    /// The lower-cased lookup key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The namespace part (empty for a global name), as declared.
    pub fn namespace(&self) -> &str {
        self.display
            .rsplit_once('\\')
            .map_or("", |(namespace, _)| namespace)
    }

    /// The last segment, as declared.
    pub fn short_name(&self) -> &str {
        self.display
            .rsplit_once('\\')
            .map_or(&*self.display, |(_, short)| short)
    }

    /// The lookup key of a constant: the namespace lower-cased, the short name exact.
    pub fn constant_key(&self) -> String {
        match self.display.rsplit_once('\\') {
            Some((namespace, short)) => format!("{}\\{short}", namespace.to_ascii_lowercase()),
            None => self.display.to_string(),
        }
    }
}

impl PartialEq for Fqn {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Eq for Fqn {}

impl Hash for Fqn {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}

impl PartialOrd for Fqn {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Fqn {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key.cmp(&other.key)
    }
}

impl fmt::Display for Fqn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display)
    }
}

/// Term: exempt
///
/// What a written name is looked up as; the three live in separate PHP namespaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NameContext {
    /// A class, interface, trait or enum.
    Class,
    /// A function.
    Function,
    /// A constant.
    Const,
}

/// Term: exempt
///
/// Why a name could not be resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NameFailure {
    /// `self` outside any class-like.
    SelfOutsideClass,
    /// `static` outside any class-like.
    StaticOutsideClass,
    /// `parent` outside any class-like.
    ParentOutsideClass,
    /// `parent` in a class that has no parent.
    ParentWithoutParent,
    /// The name is empty (an error node produced it).
    Empty,
}

impl NameFailure {
    /// A message naming the construct.
    pub const fn message(self) -> &'static str {
        match self {
            Self::SelfOutsideClass => "`self` used outside a class-like",
            Self::StaticOutsideClass => "`static` used outside a class-like",
            Self::ParentOutsideClass => "`parent` used outside a class-like",
            Self::ParentWithoutParent => "`parent` used in a class that has no parent",
            Self::Empty => "empty name",
        }
    }
}

/// Term: Fully qualified name
///
/// The result of resolving a written name. Resolution never guesses: what cannot be decided is
/// reported as such.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolvedName {
    /// The name is this fully qualified name.
    Resolved(Fqn),
    /// `static`: a late-static-binding marker bound to this class-like (the using class, for a trait).
    LateStatic(Fqn),
    /// An unqualified function or constant whose namespaced declaration may or may not exist: PHP
    /// uses the namespaced one if it does and the global one otherwise.
    FunctionOrConst {
        /// The candidate in the current namespace.
        namespaced: Fqn,
        /// The global fallback.
        global: Fqn,
    },
    /// The name is computed at run time (`new $class`).
    Dynamic(Span),
    /// The name cannot be resolved.
    Failed {
        /// Why.
        reason: NameFailure,
        /// Where the name is.
        span: Span,
    },
}

impl ResolvedName {
    /// The single fully qualified name, if the resolution is certain.
    pub fn fqn(&self) -> Option<&Fqn> {
        match self {
            Self::Resolved(fqn) | Self::LateStatic(fqn) => Some(fqn),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equality_ignores_case_but_display_keeps_the_declared_spelling() {
        let a = Fqn::new("App\\Db\\Connection");
        let b = Fqn::new("\\app\\DB\\connection");
        assert_eq!(a, b);
        assert_eq!(a.as_str(), "App\\Db\\Connection");
        assert_eq!(b.as_str(), "app\\DB\\connection");
        assert_eq!(a.to_string(), "App\\Db\\Connection");
        assert_eq!(a.key(), "app\\db\\connection");
    }

    #[test]
    fn segments_and_namespaces() {
        let fqn = Fqn::new("App\\Http\\Request");
        assert_eq!(fqn.namespace(), "App\\Http");
        assert_eq!(fqn.short_name(), "Request");
        assert_eq!(Fqn::new("strlen").namespace(), "");
        assert_eq!(Fqn::in_namespace("", "strlen"), Fqn::new("strlen"));
        assert_eq!(Fqn::in_namespace("App", "f").as_str(), "App\\f");
    }

    #[test]
    fn constants_are_case_sensitive_in_the_last_segment() {
        let a = Fqn::new("App\\FOO");
        let b = Fqn::new("app\\FOO");
        let c = Fqn::new("App\\foo");
        assert_eq!(a.constant_key(), b.constant_key());
        assert_ne!(a.constant_key(), c.constant_key());
    }

    #[test]
    fn ordering_is_by_key() {
        let mut names = [Fqn::new("B"), Fqn::new("a"), Fqn::new("c")];
        names.sort();
        let shown: Vec<_> = names.iter().map(Fqn::as_str).collect();
        assert_eq!(shown, ["a", "B", "c"]);
    }
}
