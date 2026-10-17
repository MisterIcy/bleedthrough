//! The include-resolver port and the path forms that cross it.

/// Term: Call graph
///
/// An include path the engine could evaluate, normalised: `.` and `..` segments and repeated
/// separators are folded away, and no separator trails. A path is absolute when it starts with `/`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FoldedPath(Box<str>);

impl FoldedPath {
    /// The normalised form of `text`.
    pub fn new(text: &str) -> Self {
        Self(normalise_path(text).into())
    }

    /// The path text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether the path is absolute.
    pub fn is_absolute(&self) -> bool {
        self.0.starts_with('/')
    }
}

/// Term: Call graph
///
/// Folds `.` and `..` segments and repeated separators out of `text`. A relative path that climbs
/// above its start keeps its leading `..` segments; the empty path becomes `.`.
pub fn normalise_path(text: &str) -> String {
    let absolute = text.starts_with('/');
    let mut kept: Vec<&str> = Vec::new();
    for segment in text.split('/') {
        match segment {
            "" | "." => {}
            ".." => match kept.last() {
                Some(&last) if last != ".." => {
                    kept.pop();
                }
                _ if absolute => {}
                _ => kept.push(".."),
            },
            other => kept.push(other),
        }
    }
    let joined = kept.join("/");
    match (absolute, joined.is_empty()) {
        (true, _) => format!("/{joined}"),
        (false, true) => ".".to_string(),
        (false, false) => joined,
    }
}

/// Term: Call graph
///
/// Port: maps an evaluated include path to the project files it may name. The application layer
/// implements it over the file list and the include path; the engine only folds and asks.
pub trait IncludeResolver {
    /// The project files `path` may denote when written in the file `from`, in a fixed order.
    /// Empty when no file matches.
    fn resolve(&self, from: &str, path: &FoldedPath) -> Vec<Box<str>>;
}

/// Term: Call graph
///
/// A resolver that finds nothing: every include is then reported as unresolved.
#[derive(Debug, Clone, Copy, Default)]
#[allow(clippy::exhaustive_structs, reason = "a unit type")]
pub struct NoIncludes;

impl IncludeResolver for NoIncludes {
    fn resolve(&self, _from: &str, _path: &FoldedPath) -> Vec<Box<str>> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_normalised() {
        assert_eq!(normalise_path("./a//b/../c.php"), "a/c.php");
        assert_eq!(normalise_path("../x.php"), "../x.php");
        assert_eq!(normalise_path("a/../../x.php"), "../x.php");
        assert_eq!(normalise_path("/a/../../b"), "/b");
        assert_eq!(normalise_path(""), ".");
        assert_eq!(normalise_path("/"), "/");
        assert!(FoldedPath::new("/etc/x").is_absolute());
    }
}
