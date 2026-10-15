//! Resilience tests: every prefix of a fixture and a reproducible set of seeded mutations.
//!
//! The generator is a small deterministic PRNG of our own (no dependency), so a failure message
//! that carries the seed is enough to reproduce the exact input.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};

use bleedthrough_syntax::{
    ParseOptions, ParseQuality, ParsedFile, ParserBackend, RecoveryMode, check_invariants,
};

use crate::corpus::{Corpus, Fixture};
use crate::dump::dump_tree;
use crate::suite::{Failure, failure};

/// A SplitMix64 generator: tiny, fast and fully determined by its seed.
#[derive(Debug, Clone)]
pub struct Prng(u64);

impl Prng {
    /// Starts a sequence from `seed`.
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `0..bound` (`bound` must be positive).
    pub fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next_u64() % u64::try_from(bound.max(1)).unwrap_or(1)).unwrap_or(0)
    }
}

/// Bytes that change the structure of PHP code when inserted.
const INTERESTING: &[u8] = b"{}()[];,'\"`$<>?/*#\\\n\t &|^!=:.-+%@~\0\xff\xc3";

/// The ways a mutation can alter a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationKind {
    /// Replace one byte.
    Flip,
    /// Remove a short range.
    Delete,
    /// Insert a structural byte.
    Insert,
    /// Repeat a short range in place.
    Duplicate,
}

/// A mutated source with a description sufficient to reproduce it from the seed.
#[derive(Debug, Clone)]
pub struct Mutation {
    /// The kind of change.
    pub kind: MutationKind,
    /// The mutated bytes.
    pub bytes: Vec<u8>,
    /// A human-readable account of the change.
    pub description: String,
}

/// Applies one mutation chosen and positioned by `seed`.
pub fn mutate(source: &[u8], seed: u64) -> Mutation {
    let mut rng = Prng::new(seed);
    let kind = match rng.below(4) {
        0 => MutationKind::Flip,
        1 => MutationKind::Delete,
        2 => MutationKind::Insert,
        _ => MutationKind::Duplicate,
    };
    let at = rng.below(source.len().max(1));
    let width = 1 + rng.below(8);
    let mut bytes = source.to_vec();
    let description = match kind {
        MutationKind::Flip => {
            let value = INTERESTING[rng.below(INTERESTING.len())];
            if let Some(b) = bytes.get_mut(at) {
                *b = value;
            }
            format!("flip byte {at} to {value:#04x}")
        }
        MutationKind::Delete => {
            let end = (at + width).min(bytes.len());
            bytes.drain(at.min(bytes.len())..end);
            format!("delete bytes {at}..{end}")
        }
        MutationKind::Insert => {
            let value = INTERESTING[rng.below(INTERESTING.len())];
            bytes.insert(at.min(bytes.len()), value);
            format!("insert {value:#04x} at {at}")
        }
        MutationKind::Duplicate => {
            let end = (at + width).min(bytes.len());
            let chunk = bytes[at.min(bytes.len())..end].to_vec();
            let position = at.min(bytes.len());
            bytes.splice(position..position, chunk);
            format!("duplicate bytes {at}..{end}")
        }
    };
    Mutation {
        kind,
        bytes,
        description,
    }
}

/// The seed for the `index`-th mutation of a fixture, derived from the run's base seed.
pub fn derive_seed(base: u64, path: &str, index: usize) -> u64 {
    let mut rng = Prng::new(base);
    let mut hash = rng.next_u64();
    for byte in path.bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01B3);
    }
    Prng::new(hash ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)).next_u64()
}

/// How much resilience testing to do.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RobustnessOptions {
    /// Check every `prefix_stride`-th prefix (1 checks all of them).
    pub prefix_stride: usize,
    /// How many seeded mutations per fixture.
    pub mutations: usize,
    /// The base seed; failures print the derived per-mutation seed.
    pub seed: u64,
    /// A single parse slower than this counts as a hang.
    pub slow_parse: Duration,
}

impl Default for RobustnessOptions {
    fn default() -> Self {
        Self {
            prefix_stride: 1,
            mutations: 25,
            seed: 0x0B1E_ED70,
            slow_parse: Duration::from_secs(10),
        }
    }
}

fn guarded_parse(
    backend: &dyn ParserBackend,
    path: &str,
    bytes: &[u8],
    options: &ParseOptions,
    slow: Duration,
) -> Result<ParsedFile, String> {
    let started = Instant::now();
    let parsed = catch_unwind(AssertUnwindSafe(|| backend.parse(path, bytes, options)))
        .map_err(|_| "the backend panicked".to_string())?;
    let elapsed = started.elapsed();
    if elapsed > slow {
        return Err(format!("the parse took {elapsed:?}"));
    }
    Ok(parsed)
}

/// One generated input to parse.
struct Variant<'a> {
    subject: &'a str,
    bytes: &'a [u8],
    options: &'a ParseOptions,
}

fn check_variant(
    backend: &dyn ParserBackend,
    variant: &Variant<'_>,
    robustness: &RobustnessOptions,
    out: &mut Vec<Failure>,
) -> Option<ParsedFile> {
    let Variant {
        subject,
        bytes,
        options,
    } = variant;
    match guarded_parse(backend, subject, bytes, options, robustness.slow_parse) {
        Err(message) => {
            out.push(failure(subject, "resilience", message));
            None
        }
        Ok(parsed) => {
            if parsed.quality() != ParseQuality::Failed {
                for v in check_invariants(parsed.tree(), bytes) {
                    out.push(failure(
                        subject,
                        "invariants",
                        format!("{:?}: {}", v.kind, v.message),
                    ));
                }
            }
            Some(parsed)
        }
    }
}

fn modes() -> [(RecoveryMode, &'static str); 2] {
    [
        (RecoveryMode::Strict, "strict"),
        (RecoveryMode::Editor, "editor"),
    ]
}

/// Parses every prefix of `bytes` in both recovery modes.
pub fn check_prefixes(
    backend: &dyn ParserBackend,
    path: &str,
    bytes: &[u8],
    robustness: &RobustnessOptions,
) -> Vec<Failure> {
    let mut out = Vec::new();
    let reference = backend.parse(path, bytes, &ParseOptions::default());
    for (mode, name) in modes() {
        let mut options = ParseOptions::default();
        options.recovery = mode;
        let stride = robustness.prefix_stride.max(1);
        let mut cuts: Vec<usize> = (0..bytes.len()).step_by(stride).collect();
        cuts.push(bytes.len());
        for cut in cuts {
            let subject = format!("{path}#prefix={cut},{name}");
            let variant = Variant {
                subject: &subject,
                bytes: &bytes[..cut],
                options: &options,
            };
            let parsed = check_variant(backend, &variant, robustness, &mut out);
            if cut == bytes.len()
                && mode == RecoveryMode::Strict
                && let Some(parsed) = parsed
                && dump_tree(parsed.tree()) != dump_tree(reference.tree())
            {
                out.push(failure(
                    &subject,
                    "prefix-identity",
                    "the full-length prefix differs from the unmodified fixture",
                ));
            }
        }
    }
    out
}

/// Parses the seeded mutations of `bytes`.
pub fn check_mutations(
    backend: &dyn ParserBackend,
    path: &str,
    bytes: &[u8],
    robustness: &RobustnessOptions,
) -> Vec<Failure> {
    let mut out = Vec::new();
    for index in 0..robustness.mutations {
        let seed = derive_seed(robustness.seed, path, index);
        let mutation = mutate(bytes, seed);
        for (mode, name) in modes() {
            let mut options = ParseOptions::default();
            options.recovery = mode;
            let subject = format!("{path}#seed={seed},{name} ({})", mutation.description);
            let variant = Variant {
                subject: &subject,
                bytes: &mutation.bytes,
                options: &options,
            };
            check_variant(backend, &variant, robustness, &mut out);
        }
    }
    out
}

/// Runs the prefix and mutation tests for every valid-group fixture of the corpus.
pub fn run_robustness(
    backend: &dyn ParserBackend,
    corpus: &Corpus,
    robustness: &RobustnessOptions,
) -> Vec<Failure> {
    let mut out = Vec::new();
    for fixture in corpus.fixtures.iter().filter(|f| is_valid_group(f)) {
        let Ok(bytes) = corpus.read(fixture) else {
            continue;
        };
        out.extend(check_prefixes(backend, &fixture.path, &bytes, robustness));
        out.extend(check_mutations(backend, &fixture.path, &bytes, robustness));
    }
    out
}

fn is_valid_group(fixture: &Fixture) -> bool {
    matches!(
        fixture.group,
        crate::corpus::Group::Valid | crate::corpus::Group::Realistic
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prng_is_reproducible_and_spreads() {
        let mut a = Prng::new(7);
        let mut b = Prng::new(7);
        let xs: Vec<u64> = (0..5).map(|_| a.next_u64()).collect();
        let ys: Vec<u64> = (0..5).map(|_| b.next_u64()).collect();
        assert_eq!(xs, ys);
        assert_ne!(xs[0], xs[1]);
        assert_ne!(Prng::new(8).next_u64(), xs[0]);
    }

    #[test]
    fn below_stays_in_range() {
        let mut rng = Prng::new(1);
        assert!((0..1000).all(|_| rng.below(7) < 7));
        assert_eq!(rng.below(0), 0);
    }

    #[test]
    fn a_recorded_seed_reproduces_the_same_mutation() {
        let source = b"<?php function f() { return [1, 2, 3]; }";
        let seed = derive_seed(42, "v8.0/x.php", 3);
        let first = mutate(source, seed);
        let again = mutate(source, seed);
        assert_eq!(first.bytes, again.bytes);
        assert_eq!(first.description, again.description);
    }

    #[test]
    fn seeds_differ_per_fixture_and_index() {
        assert_ne!(derive_seed(1, "a.php", 0), derive_seed(1, "b.php", 0));
        assert_ne!(derive_seed(1, "a.php", 0), derive_seed(1, "a.php", 1));
        assert_ne!(derive_seed(1, "a.php", 0), derive_seed(2, "a.php", 0));
    }

    #[test]
    fn every_mutation_kind_changes_a_nonempty_source() {
        let source = b"<?php echo 'hello world';";
        let mut seen = std::collections::BTreeSet::new();
        for i in 0..200 {
            let m = mutate(source, derive_seed(9, "x", i));
            seen.insert(format!("{:?}", m.kind));
        }
        assert_eq!(seen.len(), 4);
    }

    #[test]
    fn mutating_an_empty_source_does_not_panic() {
        for i in 0..50 {
            let _ = mutate(b"", i);
        }
    }
}
