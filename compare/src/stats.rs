//! Small statistics helpers for the CPU timing samples.
//!
//! Perft throughput samples are noisy (thermal drift, scheduler jitter), so the
//! benchmark reports robust summaries — median and min rather than mean — plus a
//! spread indicator so a reader can tell a real engine gap from measurement
//! noise. We report both the inter-quartile range (IQR, robust) and the
//! coefficient of variation (relative stddev) for each set of samples.

/// Summary statistics over a set of per-sample nanosecond timings.
#[derive(Clone, Copy, Debug)]
pub struct TimeStats {
    /// Median wall time, in seconds.
    pub median_s: f64,
    /// Fastest observed wall time, in seconds.
    pub min_s: f64,
    /// Inter-quartile range (Q3 − Q1), in seconds — robust spread.
    pub iqr_s: f64,
    /// Sample standard deviation, in seconds.
    pub stddev_s: f64,
    /// Number of samples summarised.
    pub n: usize,
}

impl TimeStats {
    /// Coefficient of variation (stddev / median) as a fraction; a unitless
    /// spread that is comparable across positions of different absolute speed.
    pub fn cv(&self) -> f64 {
        if self.median_s > 0.0 {
            self.stddev_s / self.median_s
        } else {
            0.0
        }
    }

    /// IQR as a fraction of the median; a robust relative spread that ignores
    /// the slow-outlier tail entirely.
    pub fn rel_iqr(&self) -> f64 {
        if self.median_s > 0.0 {
            self.iqr_s / self.median_s
        } else {
            0.0
        }
    }
}

/// Summarise a slice of nanosecond timings. `samples` must be non-empty.
pub fn summarize(samples: &[u64]) -> TimeStats {
    assert!(!samples.is_empty(), "need at least one sample");
    let mut s = samples.to_vec();
    s.sort_unstable();
    let n = s.len();

    let median_ns = percentile(&s, 0.50);
    let q1 = percentile(&s, 0.25);
    let q3 = percentile(&s, 0.75);
    let min_ns = s[0] as f64;

    let mean = s.iter().map(|&x| x as f64).sum::<f64>() / n as f64;
    let var = if n > 1 {
        s.iter().map(|&x| (x as f64 - mean).powi(2)).sum::<f64>() / (n - 1) as f64
    } else {
        0.0
    };

    const NS_PER_S: f64 = 1e9;
    TimeStats {
        median_s: median_ns / NS_PER_S,
        min_s: min_ns / NS_PER_S,
        iqr_s: (q3 - q1) / NS_PER_S,
        stddev_s: var.sqrt() / NS_PER_S,
        n,
    }
}

/// Linear-interpolated percentile of a pre-sorted ascending slice. `p` in 0..=1.
fn percentile(sorted: &[u64], p: f64) -> f64 {
    let n = sorted.len();
    if n == 1 {
        return sorted[0] as f64;
    }
    let rank = p * (n - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    let frac = rank - lo as f64;
    sorted[lo] as f64 * (1.0 - frac) + sorted[hi] as f64 * frac
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_of_odd_set() {
        let st = summarize(&[30, 10, 20]);
        assert!((st.median_s - 20e-9).abs() < 1e-18);
        assert!((st.min_s - 10e-9).abs() < 1e-18);
    }

    #[test]
    fn iqr_and_stddev_nonnegative() {
        let st = summarize(&[100, 110, 90, 105, 95, 120, 80]);
        assert!(st.iqr_s >= 0.0);
        assert!(st.stddev_s >= 0.0);
        assert!(st.cv() >= 0.0);
    }
}
