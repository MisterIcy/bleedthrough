//! The syntax tree of one file.

use crate::stmt::Stmt;
use crate::trivia::Comment;

/// Term: exempt
///
/// The immutable syntax tree of one file: top-level statements in source order, all comments in
/// source order, and the length of the source in bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxTree {
    statements: Vec<Stmt>,
    comments: Vec<Comment>,
    source_len: u32,
}

impl SyntaxTree {
    /// Builds a tree.
    pub fn new(statements: Vec<Stmt>, comments: Vec<Comment>, source_len: u32) -> Self {
        Self {
            statements,
            comments,
            source_len,
        }
    }

    /// The tree of an empty or unparsed file of `source_len` bytes.
    pub fn empty(source_len: u32) -> Self {
        Self::new(Vec::new(), Vec::new(), source_len)
    }

    /// Top-level statements in source order.
    pub fn statements(&self) -> &[Stmt] {
        &self.statements
    }

    /// Every comment in source order.
    pub fn comments(&self) -> &[Comment] {
        &self.comments
    }

    /// Length of the source file in bytes.
    pub fn source_len(&self) -> u32 {
        self.source_len
    }

    /// Number of `Unsupported` and `Error` nodes, in that order.
    pub fn hole_counts(&self) -> (usize, usize) {
        let mut counts = (0, 0);
        crate::walk::visit_tree(self, &mut |node| {
            if node.is_unsupported() {
                counts.0 += 1;
            }
            if node.is_error() {
                counts.1 += 1;
            }
        });
        counts
    }
}
