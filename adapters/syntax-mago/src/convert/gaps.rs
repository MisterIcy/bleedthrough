//! Statement sequences and gap filling.
//!
//! Mago drops statements it cannot parse, including inside nested statement lists. Every sequence
//! is therefore checked for source regions that are neither whitespace, a comment, nor a converted
//! statement; each such region becomes an explicit error statement. No source byte is silently lost.

use bleedthrough_syntax::{Node, Span, Stmt, StmtKind};
use mago_syntax::cst::{Program, Statement};

use super::Cx;

impl Cx<'_> {
    /// Converts a statement sequence inside `bounds`, inserting error statements for gaps.
    pub(crate) fn stmts<'a>(
        &mut self,
        statements: impl Iterator<Item = &'a Statement<'a>>,
        bounds: Span,
    ) -> Vec<Stmt> {
        let list: Vec<&Statement<'_>> = statements.collect();
        let starts: Vec<u32> = list.iter().map(|s| self.stmt_span(s).start).collect();
        let converted = self.each_bounded(&list, &starts, bounds.end, |cx, s| cx.stmt(s));
        self.with_gaps(converted, bounds, |diagnostic| StmtKind::Error {
            diagnostic,
        })
    }

    /// Inserts an error node, built by `error`, for every region of `bounds` that is neither
    /// whitespace, a comment, nor covered by one of `items`. The items keep their order.
    pub(super) fn with_gaps<K>(
        &mut self,
        items: Vec<Node<K>>,
        bounds: Span,
        error: fn(Option<usize>) -> K,
    ) -> Vec<Node<K>> {
        let mut out = Vec::with_capacity(items.len());
        let mut cursor = bounds.start;
        let mut items = items.into_iter().peekable();
        while let Some(mut item) = items.next() {
            // A missing terminator is reported by Mago with the range of the token that follows;
            // the item ends where the next one (or the sequence) begins, never beyond.
            let limit = items.peek().map_or(bounds.end, |next| next.span.start);
            if item.span.end > limit {
                item.span.end = limit.max(item.span.start);
            }
            self.fill(&mut out, cursor, item.span.start, error);
            cursor = cursor.max(item.span.end);
            out.push(item);
        }
        self.fill(&mut out, cursor, bounds.end, error);
        out
    }

    /// Pushes an error node for the significant bytes in `from..to`, if any.
    fn fill<K>(
        &mut self,
        out: &mut Vec<Node<K>>,
        from: u32,
        to: u32,
        error: fn(Option<usize>) -> K,
    ) {
        let Some(gap) = self.significant(from, to) else {
            return;
        };
        let diagnostic = self.diagnostic_for(gap);
        out.push(Node::new(gap, error(Some(diagnostic))));
    }

    /// The smallest span covering the bytes in `from..to` that are not whitespace or comments.
    fn significant(&self, from: u32, to: u32) -> Option<Span> {
        if from >= to {
            return None;
        }
        let bytes = self.source.get(from as usize..to as usize)?;
        let mut first = None;
        let mut last = from;
        for (offset, byte) in bytes.iter().enumerate() {
            let at = from + u32::try_from(offset).ok()?;
            if byte.is_ascii_whitespace() || self.in_trivia(at) {
                continue;
            }
            first.get_or_insert(at);
            last = at + 1;
        }
        first.map(|start| Span::new(start, last))
    }

    fn in_trivia(&self, at: u32) -> bool {
        let index = self.trivia.partition_point(|t| t.end <= at);
        self.trivia.get(index).is_some_and(|t| t.start <= at)
    }
}

/// Converts the whole program; the top level is bounded by the file.
pub(crate) fn convert_program(cx: &mut Cx<'_>, program: &Program<'_>) -> Vec<Stmt> {
    let bounds = Span::new(0, cx.source_len());
    cx.stmts(program.statements.iter(), bounds)
}
