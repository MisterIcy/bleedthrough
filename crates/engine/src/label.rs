//! The taint domain: labels, label sets and the sink classes a label has been cleared for.

use std::cmp::Ordering;

use bleedthrough_rules::SinkClass;
use bleedthrough_symbols::Fqn;
use bleedthrough_syntax::Span;

use crate::callgraph::UnitId;

/// The most sanitizers a label remembers.
const MAX_APPLIED: usize = 4;

/// Term: Taint label
///
/// A set of sink classes, one bit each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ClassSet(u16);

const CLASS_NAMES: [&str; 13] = [
    "sql",
    "os-command",
    "code-eval",
    "file-path",
    "file-inclusion",
    "deserialization",
    "xml-parsing",
    "outgoing-request",
    "redirect-target",
    "header",
    "ldap-query",
    "xpath-query",
    "log",
];

impl ClassSet {
    /// No class.
    pub const EMPTY: Self = Self(0);
    /// Every class.
    pub const ALL: Self = Self(u16::MAX);

    fn bit(class: SinkClass) -> u16 {
        // A class added to the rule vocabulary and not yet known here shares the last bit, so it
        // is never treated as cleared by accident of its own.
        let at = CLASS_NAMES
            .iter()
            .position(|n| *n == class.as_str())
            .unwrap_or(15);
        1 << at
    }

    /// The set holding exactly `class`.
    pub fn of(class: SinkClass) -> Self {
        Self(Self::bit(class))
    }

    /// The set holding the given classes.
    pub fn from_classes<'a>(classes: impl IntoIterator<Item = &'a SinkClass>) -> Self {
        Self(classes.into_iter().fold(0, |acc, c| acc | Self::bit(*c)))
    }

    /// Whether `class` is in the set.
    pub fn contains(self, class: SinkClass) -> bool {
        self.0 & Self::bit(class) != 0
    }

    /// The classes in either set.
    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// The classes in both sets.
    pub fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// Whether no class is in the set.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// Term: Taint label
///
/// Where a tainted value comes from: a source occurrence, or (while a summary is computed) a
/// symbolic input of the unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Origin {
    /// A source named by a rule, read or called at `span` in `unit`.
    Source {
        /// The source in the rule index.
        source: u32,
        /// The unit that contains the occurrence.
        unit: UnitId,
        /// The occurrence.
        span: Span,
    },
    /// The parameter at this position (from 0).
    Param(u32),
    /// The receiver, `$this`.
    Receiver,
    /// The captured variable at this position (from 0).
    Capture(u32),
}

impl Origin {
    /// Whether the origin is a symbolic input rather than a concrete source.
    pub fn is_symbolic(self) -> bool {
        !matches!(self, Self::Source { .. })
    }
}

/// Term: Taint label
///
/// A sanitizer a label passed through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct Applied {
    /// The sanitizer entry in the rule index.
    pub entry: u32,
    /// The call that applied it.
    pub span: Span,
}

/// Term: Taint label
///
/// The marks a flow collects on the way, each of which lowers confidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record of marks")]
pub struct Marks {
    /// The flow passed an unresolved callee assumed to propagate.
    pub assumed_unresolved: bool,
    /// The flow was read from a property, static property or global written elsewhere.
    pub inferred_source: bool,
    /// The largest candidate count of a multi-target call the flow passed (0 for none).
    pub several: u16,
    /// A limit widened the analysis before this flow was found.
    pub widened: bool,
}

impl Marks {
    fn join(self, other: Self) -> Self {
        Self {
            assumed_unresolved: self.assumed_unresolved || other.assumed_unresolved,
            inferred_source: self.inferred_source || other.inferred_source,
            several: self.several.max(other.several),
            widened: self.widened || other.widened,
        }
    }
}

/// Term: Taint label
///
/// One origin of taint on a value with what has happened to it since: the classes it has been
/// cleared for, the sanitizers it passed and the marks it collected.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct Label {
    /// Where the taint comes from.
    pub origin: Origin,
    /// The sink classes the taint no longer counts for.
    pub cleared: ClassSet,
    /// The sanitizers applied, sorted, capped.
    pub applied: Vec<Applied>,
    /// The marks collected.
    pub marks: Marks,
}

impl Label {
    /// A fresh label: nothing cleared, nothing applied.
    pub fn new(origin: Origin) -> Self {
        Self {
            origin,
            cleared: ClassSet::EMPTY,
            applied: Vec::new(),
            marks: Marks::default(),
        }
    }

    /// This label seen through `outer`, the label of the value that stood for its symbolic origin:
    /// the origin is the outer one, and what was cleared, applied and marked on either side holds.
    #[must_use]
    pub fn compose(&self, outer: &Self) -> Self {
        let mut out = outer.clone();
        out.cleared = outer.cleared.union(self.cleared);
        for a in &self.applied {
            if let Err(at) = out.applied.binary_search(a) {
                out.applied.insert(at, *a);
            }
        }
        out.applied.truncate(MAX_APPLIED);
        out.marks = outer.marks.join(self.marks);
        out
    }

    /// The label for the same origin as seen through both `self` and `other`: only as clean as the
    /// dirtier of the two.
    fn merge(&mut self, other: &Self) {
        self.cleared = self.cleared.intersection(other.cleared);
        for a in &other.applied {
            if let Err(at) = self.applied.binary_search(a) {
                self.applied.insert(at, *a);
            }
        }
        self.applied.truncate(MAX_APPLIED);
        self.marks = self.marks.join(other.marks);
    }
}

/// Term: Taint label
///
/// The taint of a value: its labels, one per origin, in origin order. Empty means untainted.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Taint {
    labels: Vec<Label>,
    widened: bool,
}

impl Taint {
    /// Untainted.
    pub fn clean() -> Self {
        Self::default()
    }

    /// Tainted by one label.
    pub fn of(label: Label) -> Self {
        Self {
            labels: vec![label],
            widened: false,
        }
    }

    /// Whether no label is present.
    pub fn is_clean(&self) -> bool {
        self.labels.is_empty()
    }

    /// The labels, in origin order.
    pub fn labels(&self) -> &[Label] {
        &self.labels
    }

    /// The taint with each symbolic origin replaced by what `inputs` gives for it: the composition of
    /// each of its labels with the label itself. A symbolic origin `inputs` does not know is
    /// dropped; concrete origins stay.
    #[must_use]
    pub fn substitute<'t>(
        &self,
        cap: usize,
        inputs: impl Fn(&Origin) -> Option<&'t Taint>,
    ) -> Self {
        let mut out = Self::clean();
        out.widened = self.widened;
        for label in &self.labels {
            if !label.origin.is_symbolic() {
                out.join_in(&Self::of(label.clone()), cap);
                continue;
            }
            let Some(actual) = inputs(&label.origin) else {
                continue;
            };
            for outer in &actual.labels {
                out.join_in(&Self::of(label.compose(outer)), cap);
            }
            if actual.widened {
                out.widened = true;
            }
        }
        out
    }

    /// Only the labels whose origin is a concrete source.
    #[must_use]
    pub fn concrete_only(&self) -> Self {
        Self {
            labels: self
                .labels
                .iter()
                .filter(|l| !l.origin.is_symbolic())
                .cloned()
                .collect(),
            widened: self.widened,
        }
    }

    /// Whether the origin cap cut labels away.
    pub fn widened(&self) -> bool {
        self.widened
    }

    /// The union of both: labels of equal origin are merged, then the first `cap` origins are
    /// kept. Commutative, associative and idempotent.
    pub fn join(&self, other: &Self, cap: usize) -> Self {
        let mut out = self.clone();
        out.join_in(other, cap);
        out
    }

    /// [`Self::join`] into `self`; returns whether `self` changed.
    pub fn join_in(&mut self, other: &Self, cap: usize) -> bool {
        if other.labels.is_empty() && !other.widened {
            return false;
        }
        let before = self.clone();
        let mut merged: Vec<Label> = Vec::with_capacity(self.labels.len() + other.labels.len());
        let (mut a, mut b) = (
            self.labels.iter().peekable(),
            other.labels.iter().peekable(),
        );
        loop {
            match (a.peek(), b.peek()) {
                (Some(x), Some(y)) => match x.origin.cmp(&y.origin) {
                    Ordering::Less => merged.extend(a.next().cloned()),
                    Ordering::Greater => merged.extend(b.next().cloned()),
                    Ordering::Equal => {
                        let mut label = (*x).clone();
                        label.merge(y);
                        merged.push(label);
                        a.next();
                        b.next();
                    }
                },
                (Some(_), None) => merged.extend(a.next().cloned()),
                (None, Some(_)) => merged.extend(b.next().cloned()),
                (None, None) => break,
            }
        }
        let mut widened = self.widened || other.widened;
        if merged.len() > cap {
            merged.truncate(cap);
            widened = true;
        }
        self.labels = merged;
        self.widened = widened;
        *self != before
    }

    /// The same taint after a sanitizer: every label is additionally cleared for `classes` and
    /// remembers the sanitizer.
    #[must_use]
    pub fn cleared_for(&self, classes: ClassSet, applied: Applied) -> Self {
        let mut out = self.clone();
        for label in &mut out.labels {
            label.cleared = label.cleared.union(classes);
            if let Err(at) = label.applied.binary_search(&applied) {
                label.applied.insert(at, applied);
            }
            label.applied.truncate(MAX_APPLIED);
        }
        out
    }

    /// The same taint after a cast to a type that cannot carry data: nothing is left.
    #[must_use]
    pub fn cleared_entirely() -> Self {
        Self::clean()
    }

    /// The same taint with every label additionally passing through `f`.
    #[must_use]
    pub fn map_marks(&self, f: impl Fn(&mut Marks)) -> Self {
        let mut out = self.clone();
        for label in &mut out.labels {
            f(&mut label.marks);
        }
        out
    }

    /// The same taint with each label's origin replaced by `f`, which may drop a label by
    /// returning `None`; labels that end up with equal origins merge.
    #[must_use]
    pub fn map_origins(&self, cap: usize, f: impl Fn(&Label) -> Vec<Label>) -> Self {
        let mut out = Self::clean();
        for label in &self.labels {
            for mapped in f(label) {
                out.join_in(&Self::of(mapped), cap);
            }
        }
        out.widened |= self.widened;
        out
    }

    /// Marks the taint as cut by a limit.
    pub fn set_widened(&mut self) {
        self.widened = true;
    }
}

/// Term: Abstract location
///
/// A place in memory the analysis tracks outside any one function: a property of a class, a
/// static property or a global variable.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum HeapKey {
    /// A property: the name first so that reads with an unknown class can scan by name. The class
    /// is the declaring class's lookup key, or `?` when the class is unknown.
    Prop {
        /// The property name.
        name: Box<str>,
        /// The declaring class's key.
        class: Box<str>,
    },
    /// A static property.
    Static {
        /// The property name.
        name: Box<str>,
        /// The declaring class's key.
        class: Box<str>,
    },
    /// A global variable.
    Global(Box<str>),
}

impl HeapKey {
    /// The key of a property declared by `class`.
    pub fn prop(name: &str, class: Option<&Fqn>) -> Self {
        Self::Prop {
            name: name.into(),
            class: class.map_or("?".into(), |c| c.key().into()),
        }
    }

    /// The key of a static property declared by `class`.
    pub fn static_prop(name: &str, class: Option<&Fqn>) -> Self {
        Self::Static {
            name: name.into(),
            class: class.map_or("?".into(), |c| c.key().into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use bleedthrough_testkit::check;
    use bleedthrough_testkit::proptest::prelude::*;

    use super::*;

    const CAP: usize = 3;

    fn origin() -> impl Strategy<Value = Origin> {
        prop_oneof![
            (0u32..4, 0u32..3, 0u32..4).prop_map(|(source, unit, at)| Origin::Source {
                source,
                unit: UnitId::from_index(unit as usize),
                span: Span::new(at, at + 1),
            }),
            (0u32..3).prop_map(Origin::Param),
            Just(Origin::Receiver),
            (0u32..3).prop_map(Origin::Capture),
        ]
    }

    fn label() -> impl Strategy<Value = Label> {
        (
            origin(),
            0u16..(1 << 12),
            bleedthrough_testkit::proptest::collection::vec((0u32..4, 0u32..4), 0..4),
            (any::<bool>(), any::<bool>(), 0u16..4, any::<bool>()),
        )
            .prop_map(|(origin, cleared, applied, marks)| {
                let mut label = Label::new(origin);
                label.cleared = ClassSet(cleared);
                let mut applied: Vec<Applied> = applied
                    .into_iter()
                    .map(|(entry, at)| Applied {
                        entry,
                        span: Span::new(at, at + 1),
                    })
                    .collect();
                applied.sort();
                applied.dedup();
                label.applied = applied;
                label.marks = Marks {
                    assumed_unresolved: marks.0,
                    inferred_source: marks.1,
                    several: marks.2,
                    widened: marks.3,
                };
                label
            })
    }

    fn taint() -> impl Strategy<Value = Taint> {
        bleedthrough_testkit::proptest::collection::vec(label(), 0..5).prop_map(|labels| {
            labels.iter().fold(Taint::clean(), |acc, l| {
                acc.join(&Taint::of(l.clone()), CAP)
            })
        })
    }

    /// spec: taint-labels/Label domain
    #[test]
    fn join_is_commutative() {
        check((taint(), taint()), |(a, b)| {
            prop_assert_eq!(a.join(&b, CAP), b.join(&a, CAP));
            Ok(())
        });
    }

    /// spec: taint-labels/Label domain
    #[test]
    fn join_is_associative() {
        check((taint(), taint(), taint()), |(a, b, c)| {
            prop_assert_eq!(a.join(&b, CAP).join(&c, CAP), a.join(&b.join(&c, CAP), CAP));
            Ok(())
        });
    }

    /// spec: taint-labels/Label domain
    #[test]
    fn join_is_idempotent() {
        check(taint(), |a| {
            prop_assert_eq!(a.join(&a, CAP), a);
            Ok(())
        });
    }

    /// spec: taint-labels/Label domain
    #[test]
    fn clearing_never_adds_taint() {
        check(
            (taint(), 0u16..(1 << 12), 0u32..4),
            |(a, classes, entry)| {
                let applied = Applied {
                    entry,
                    span: Span::new(0, 1),
                };
                let cleared = a.cleared_for(ClassSet(classes), applied);
                prop_assert_eq!(cleared.labels().len(), a.labels().len());
                for (before, after) in a.labels().iter().zip(cleared.labels()) {
                    prop_assert_eq!(before.origin, after.origin);
                    // Every class cleared before stays cleared; nothing is un-cleared.
                    prop_assert_eq!(before.cleared.union(after.cleared), after.cleared);
                }
                Ok(())
            },
        );
    }

    /// spec: taint-labels/Label domain
    #[test]
    fn the_cap_bounds_the_origins() {
        check((taint(), taint()), |(a, b)| {
            prop_assert!(a.join(&b, CAP).labels().len() <= CAP);
            Ok(())
        });
    }
}
