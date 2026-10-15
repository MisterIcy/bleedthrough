//! Conversion of the Mago CST into the owned syntax tree.
//!
//! One module per node family. Every `match` over a Mago enum lists all variants (no wildcard arm),
//! so a Mago upgrade that adds a variant stops the build.
#![deny(clippy::wildcard_enum_match_arm)]

mod common;
mod decl;
mod expr;
mod gaps;
mod stmt;
mod types;

use bleedthrough_syntax::{Diagnostic, DiagnosticCode, Span};
use mago_syntax::error::ParseError;

use crate::errors;

/// Maximum nesting depth of the converted tree; deeper input becomes an error node.
const MAX_DEPTH: u32 = 512;

/// State shared by the whole conversion of one file.
pub(crate) struct Cx<'s> {
    source: &'s [u8],
    errors: Vec<Span>,
    diagnostics: Vec<Diagnostic>,
    trivia: Vec<Span>,
    depth: u32,
    depth_diagnostic: Option<usize>,
    /// No node may end after this offset: the start of the next sibling, or the end of the
    /// enclosing node. Mago gives a missing delimiter the range of the token that follows it, so
    /// its nodes can overshoot; the bound keeps every child inside its parent by construction.
    hi: u32,
}

impl<'s> Cx<'s> {
    /// Prepares the conversion: maps every Mago error to a diagnostic, in order.
    pub(crate) fn new(source: &'s [u8], errors: &[ParseError], trivia: Vec<Span>) -> Self {
        let diagnostics: Vec<Diagnostic> = errors
            .iter()
            .map(|e| errors::diagnostic(e, source))
            .collect();
        let reported = diagnostics.iter().map(|d| d.span).collect();
        Self {
            source,
            errors: reported,
            diagnostics,
            trivia,
            depth: 0,
            depth_diagnostic: None,
            hi: u32::MAX,
        }
    }

    /// The diagnostics so far: the mapped Mago errors followed by synthesized ones.
    pub(crate) fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    /// Enters one level of nesting; `false` means the limit is exceeded and the caller must stop.
    fn enter(&mut self) -> bool {
        self.depth += 1;
        self.depth <= MAX_DEPTH
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    /// The diagnostic for a nesting-limit hit, created once per file.
    fn depth_error(&mut self, at: Span) -> usize {
        if let Some(index) = self.depth_diagnostic {
            return index;
        }
        let diagnostic = Diagnostic::new(DiagnosticCode::RecursionLimit, "nesting is too deep", at);
        self.diagnostics.push(diagnostic);
        let index = self.diagnostics.len() - 1;
        self.depth_diagnostic = Some(index);
        index
    }

    /// A synthesized diagnostic for a region no Mago error explains.
    fn synthesize(&mut self, at: Span) -> usize {
        let diagnostic = Diagnostic::new(
            DiagnosticCode::UnsupportedConstruct,
            "the parser dropped this region without reporting an error",
            at,
        );
        self.diagnostics.push(diagnostic);
        self.diagnostics.len() - 1
    }

    /// The diagnostic that explains an error node at `at`: the Mago error nearest it, or a
    /// synthesized one when Mago reported nothing there. An error node never lacks a diagnostic.
    fn diagnostic_for(&mut self, at: Span) -> usize {
        match self.link(at) {
            Some(index) => index,
            None => self.synthesize(at),
        }
    }

    /// Best-effort link from an error node to the Mago error nearest its span.
    fn link(&self, at: Span) -> Option<usize> {
        let touching = |s: Span| s.start <= at.end && at.start <= s.end;
        self.errors.iter().position(|e| touching(*e))
    }

    /// `at` with its end cut to the current bound.
    fn fit(&self, at: Span) -> Span {
        Span::new(at.start.min(self.hi), at.end.min(self.hi))
    }

    /// Converts `items` in order, bounding each to end where the next begins (the last one at
    /// `end`). Mago reports a missing terminator or closer with the range of the token that
    /// follows it, which would otherwise make an item overlap its successor.
    fn each_bounded<T, N>(
        &mut self,
        items: &[T],
        starts: &[u32],
        end: u32,
        mut convert: impl FnMut(&mut Self, &T) -> N,
    ) -> Vec<N> {
        let mut out = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            let next = starts.get(index + 1).copied().unwrap_or(end);
            out.push(self.bounded(next, |cx| convert(cx, item)));
        }
        out
    }

    /// Runs `convert` with nodes bounded to end at or before `hi`.
    fn bounded<T>(&mut self, hi: u32, convert: impl FnOnce(&mut Self) -> T) -> T {
        let saved = self.hi;
        self.hi = saved.min(hi);
        let out = convert(self);
        self.hi = saved;
        out
    }

    pub(crate) fn source_len(&self) -> u32 {
        u32::try_from(self.source.len()).unwrap_or(u32::MAX)
    }
}

pub(crate) use gaps::convert_program;
