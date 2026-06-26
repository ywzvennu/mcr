//! A tiny, fully deterministic PRNG for the seeded position generator.
//!
//! The benchmark suite generates hundreds of positions by playing seeded random
//! legal games. For the numbers to be comparable across runs (and machines), the
//! generation must be *exactly* reproducible: same seed in, same positions out,
//! forever. We therefore avoid every nondeterministic source (no system clock,
//! no thread RNG, no hash-map iteration order) and use [splitmix64], a
//! well-distributed, statelessly-seedable 64-bit generator with a single `u64`
//! of state. It is the algorithm xoshiro recommends for seeding, is trivial to
//! reimplement identically, and needs no allocation.
//!
//! [splitmix64]: https://prng.di.unimi.it/splitmix64.c

/// A `splitmix64` generator: one `u64` of state, advanced by a fixed increment
/// and finalized with the standard avalanche mixer. Deterministic for a seed.
#[derive(Clone, Debug)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Create a generator seeded with `seed`. Every seed yields a distinct,
    /// reproducible stream.
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Return the next 64-bit value and advance the state.
    ///
    /// This is the reference `splitmix64` `next()`: add the golden-ratio
    /// increment, then run the two-shift/two-multiply avalanche.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Return a uniformly distributed value in `0..n` (for `n > 0`).
    ///
    /// Uses Lemire's multiply-shift reduction, which is fast and (for the small
    /// `n` used here — legal-move counts, ply counts, 960 ids) effectively
    /// unbiased. `n == 0` is a programming error and panics.
    pub fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "SplitMix64::below(0)");
        ((self.next_u64() as u128 * n as u128) >> 64) as u64
    }

    /// Pick an index in `0..len` (convenience for slicing move lists).
    pub fn index(&mut self, len: usize) -> usize {
        self.below(len as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_stream() {
        // The same seed must always produce the same first values; this pins the
        // generated suite so its numbers stay comparable across runs.
        let mut a = SplitMix64::new(0x0DDB_1A5E_5EED_1234);
        let mut b = SplitMix64::new(0x0DDB_1A5E_5EED_1234);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn reference_vectors() {
        // Reference splitmix64 outputs for seed 0 (from the canonical C source).
        let mut r = SplitMix64::new(0);
        assert_eq!(r.next_u64(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(r.next_u64(), 0x6E78_9E6A_A1B9_65F4);
        assert_eq!(r.next_u64(), 0x06C4_5D18_8009_454F);
    }

    #[test]
    fn below_is_in_range() {
        let mut r = SplitMix64::new(42);
        for _ in 0..10_000 {
            assert!(r.below(20) < 20);
            assert!(r.below(1) == 0);
        }
    }
}
