//! Wall-clock sampling and its statistics.

use std::time::Instant;

/// The fewest timed iterations a run may use; a single sample says nothing about noise.
pub const MIN_ITERATIONS: u32 = 5;

/// How many passes to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    /// Passes whose time is discarded.
    pub warmups: u32,
    /// Timed passes; at least [`MIN_ITERATIONS`].
    pub iterations: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            warmups: 3,
            iterations: 10,
        }
    }
}

impl Config {
    /// Refuses a configuration with too few iterations.
    pub fn validate(&self) -> Result<(), String> {
        if self.iterations < MIN_ITERATIONS {
            return Err(format!(
                "{} timed iteration(s) is too few: the minimum is {MIN_ITERATIONS}, so that median and spread mean something",
                self.iterations
            ));
        }
        Ok(())
    }
}

/// Runs `pass` through the warm-ups, then times each of the iterations in nanoseconds.
pub fn sample(config: &Config, mut pass: impl FnMut()) -> Result<Vec<u64>, String> {
    config.validate()?;
    for _ in 0..config.warmups {
        pass();
    }
    let mut samples = Vec::new();
    for _ in 0..config.iterations {
        let start = Instant::now();
        pass();
        samples.push(u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX));
    }
    Ok(samples)
}

/// The median; the mean of the two middle values for an even count. `None` for no samples.
pub fn median(values: &[u64]) -> Option<u64> {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let n = sorted.len();
    match n {
        0 => None,
        _ if n % 2 == 1 => Some(sorted[n / 2]),
        _ => Some((sorted[n / 2 - 1] + sorted[n / 2]) / 2),
    }
}

/// The spread: the median absolute deviation from the median.
pub fn spread(values: &[u64]) -> Option<u64> {
    let m = median(values)?;
    let deviations: Vec<u64> = values.iter().map(|v| v.abs_diff(m)).collect();
    median(&deviations)
}

/// Summary of a set of samples.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// The samples in the order taken.
    pub samples_ns: Vec<u64>,
    /// Their median.
    pub median_ns: u64,
    /// Median absolute deviation.
    pub spread_ns: u64,
    /// The fastest sample.
    pub min_ns: u64,
}

impl Summary {
    /// Summarises samples; `None` when there are none.
    pub fn of(samples: &[u64]) -> Option<Self> {
        Some(Self {
            samples_ns: samples.to_vec(),
            median_ns: median(samples)?,
            spread_ns: spread(samples)?,
            min_ns: *samples.iter().min()?,
        })
    }

    /// Spread relative to the median (0 when the median is 0).
    pub fn noise(&self) -> f64 {
        if self.median_ns == 0 {
            0.0
        } else {
            self.spread_ns as f64 / self.median_ns as f64
        }
    }
}
