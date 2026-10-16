use crate::{Location, ModelError};

/// Term: Trace
///
/// The role of one step in a trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum StepKind {
    /// Where untrusted data enters.
    Source,
    /// Taint carried onward unchanged.
    Propagation,
    /// A call that the flow passes through.
    Call,
    /// A sanitizer that was considered and judged insufficient.
    SanitizerConsidered,
    /// Where the data becomes dangerous.
    Sink,
}

impl StepKind {
    /// The stable wire name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Propagation => "propagation",
            Self::Call => "call",
            Self::SanitizerConsidered => "sanitizer_considered",
            Self::Sink => "sink",
        }
    }
}

/// Term: Trace
///
/// One step of a trace: a location, a kind and a short label.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Step {
    location: Location,
    kind: StepKind,
    label: String,
}

impl Step {
    /// Builds a step.
    pub fn new(location: Location, kind: StepKind, label: impl Into<String>) -> Self {
        Self {
            location,
            kind,
            label: label.into(),
        }
    }

    /// Where the step is.
    pub fn location(&self) -> &Location {
        &self.location
    }

    /// The role of the step.
    pub fn kind(&self) -> StepKind {
        self.kind
    }

    /// The short label.
    pub fn label(&self) -> &str {
        &self.label
    }
}

/// Term: Trace
///
/// An ordered flow from a source to a sink. Over-long traces are truncated in the middle and
/// marked, keeping both ends.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Trace {
    steps: Vec<Step>,
    truncated: bool,
}

impl Trace {
    /// Validates source-to-sink shape, then truncates the middle when longer than `max_steps`
    /// (which must be at least 2).
    pub fn new(mut steps: Vec<Step>, max_steps: usize) -> Result<Self, ModelError> {
        if max_steps < 2 {
            return Err(ModelError::TraceLimitTooSmall(max_steps));
        }
        if steps.len() < 2 {
            return Err(ModelError::TraceTooShort);
        }
        let starts_at_source = steps.first().is_some_and(|s| s.kind == StepKind::Source);
        let ends_at_sink = steps.last().is_some_and(|s| s.kind == StepKind::Sink);
        if !starts_at_source || !ends_at_sink {
            return Err(ModelError::TraceNotSourceToSink);
        }
        let truncated = steps.len() > max_steps;
        if truncated {
            let tail = max_steps / 2;
            let head = max_steps - tail;
            let end = steps.split_off(steps.len() - tail);
            steps.truncate(head);
            steps.extend(end);
        }
        Ok(Self { steps, truncated })
    }

    /// The steps, source first.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// Whether the middle was cut.
    pub fn truncated(&self) -> bool {
        self.truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Position, RelPath};

    fn step(n: u32, kind: StepKind) -> Step {
        let p = Position::new(n, 1).unwrap();
        let loc = Location::new(RelPath::new("a.php").unwrap(), p, p, 0, 0).unwrap();
        Step::new(loc, kind, format!("s{n}"))
    }

    fn flow(len: u32) -> Vec<Step> {
        let mut v = vec![step(1, StepKind::Source)];
        v.extend((2..len).map(|n| step(n, StepKind::Propagation)));
        v.push(step(len, StepKind::Sink));
        v
    }

    #[test]
    fn short_trace_is_kept_whole() {
        let t = Trace::new(flow(4), 10).unwrap();
        assert_eq!((t.steps().len(), t.truncated()), (4, false));
    }

    /// spec: finding-model/Traces explain the flow
    #[test]
    fn over_limit_trace_keeps_both_ends_and_is_marked() {
        for max in [2, 3, 6] {
            let t = Trace::new(flow(20), max).unwrap();
            assert_eq!(t.steps().len(), max);
            assert!(t.truncated());
            assert_eq!(t.steps()[0].kind(), StepKind::Source);
            assert_eq!(t.steps()[max - 1].kind(), StepKind::Sink);
            assert_eq!(t.steps()[max - 1].label(), "s20");
        }
    }

    #[test]
    fn shape_violations_fail() {
        assert_eq!(
            Trace::new(flow(2)[..1].to_vec(), 5).unwrap_err(),
            ModelError::TraceTooShort
        );
        let mut bad = flow(3);
        bad[0] = step(1, StepKind::Call);
        assert_eq!(
            Trace::new(bad, 5).unwrap_err(),
            ModelError::TraceNotSourceToSink
        );
        assert!(Trace::new(flow(3), 1).is_err());
    }
}
