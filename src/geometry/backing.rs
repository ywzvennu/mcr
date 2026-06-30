//! The integer backing for a generic [`Bitboard`](super::Bitboard).
//!
//! A [`BitboardBacking`] is the minimal set of bit operations a generic
//! bitboard needs from its underlying unsigned integer, implemented here for
//! [`u64`] (the frozen 8x8 width), [`u128`] (everything from 9x9 up to a
//! 128-square board), and [`U256`] (the two-limb backing for boards beyond 128
//! squares, e.g. 12x12 Chu Shogi). The trait is deliberately tiny: only the
//! operations the bitboard layer actually uses, all `safe`, all
//! `const`-friendly.

use core::ops::{BitAnd, BitOr, BitXor, Not, Shl, Shr};

/// An unsigned integer usable as the backing store of a generic bitboard.
///
/// The bound list is exactly the algebra the bitboard layer needs: the bitwise
/// operators (with their right-hand side being `Self`), shifts by a `u32`, and
/// the inherent population-count / bit-scan helpers exposed as trait methods so
/// they can be called on a generic `Self`.
///
/// Implemented for [`u64`], [`u128`], and [`U256`]. No `unsafe` is involved
/// anywhere.
pub trait BitboardBacking:
    Copy
    + Eq
    + BitAnd<Output = Self>
    + BitOr<Output = Self>
    + BitXor<Output = Self>
    + Not<Output = Self>
    + Shl<u32, Output = Self>
    + Shr<u32, Output = Self>
{
    /// The all-zero value (the empty set).
    const ZERO: Self;

    /// The value `1`, i.e. the single low bit set (square index `0`).
    const ONE: Self;

    /// The total number of bits in this integer (`64`, `128`, or `256`).
    const BITS: u32;

    /// Returns the number of set bits.
    fn count_ones(self) -> u32;

    /// Returns the number of trailing zero bits (the index of the lowest set
    /// bit). For a zero value this is [`Self::BITS`].
    fn trailing_zeros(self) -> u32;

    /// Returns the number of leading zero bits (counting from the most
    /// significant bit). For a zero value this is [`Self::BITS`].
    ///
    /// Used by the cannon ray path to find the *highest* set bit on a masked
    /// occupancy — the nearest occupant along a south / west (descending-index)
    /// ray — in a single bit-scan instead of stepping square by square.
    fn leading_zeros(self) -> u32;

    /// Returns `true` if no bit is set.
    fn is_zero(self) -> bool;

    /// Returns the single-bit value `1 << shift`.
    ///
    /// `shift` must be `< Self::BITS`; callers in this crate only ever pass a
    /// validated square index, which is `< SQUARES <= BITS`.
    fn bit(shift: u32) -> Self;

    /// Returns `self` with its lowest set bit cleared (`self & (self - 1)`).
    ///
    /// For a zero value this returns zero.
    fn clear_lowest(self) -> Self;

    /// Wrapping integer subtraction (`self - rhs` modulo `2^BITS`).
    ///
    /// Used by the hyperbola-quintessence sliders, whose `o - 2s` step relies on
    /// the borrow propagating up to the first blocker; the wrap is intentional.
    fn wrapping_sub(self, rhs: Self) -> Self;

    /// Wrapping integer addition (`self + rhs` modulo `2^BITS`).
    ///
    /// Used only to form `2s = s + s` for the slider step without risking a
    /// debug-mode overflow panic when `s` is the top bit.
    fn wrapping_add(self, rhs: Self) -> Self;

    /// Returns the value with its bits in reverse order over the full backing
    /// width (`BITS` bits).
    ///
    /// The reverse direction of hyperbola quintessence operates on the
    /// bit-reversed line; the reversal is over the whole integer just as the
    /// frozen `u64` path reverses over all 64 bits before masking back to the
    /// line.
    fn reverse_bits(self) -> Self;
}

macro_rules! impl_backing {
    ($ty:ty) => {
        impl BitboardBacking for $ty {
            const ZERO: Self = 0;
            const ONE: Self = 1;
            const BITS: u32 = <$ty>::BITS;

            #[inline]
            fn count_ones(self) -> u32 {
                <$ty>::count_ones(self)
            }

            #[inline]
            fn trailing_zeros(self) -> u32 {
                <$ty>::trailing_zeros(self)
            }

            #[inline]
            fn leading_zeros(self) -> u32 {
                <$ty>::leading_zeros(self)
            }

            #[inline]
            fn is_zero(self) -> bool {
                self == 0
            }

            #[inline]
            fn bit(shift: u32) -> Self {
                1 << shift
            }

            #[inline]
            fn clear_lowest(self) -> Self {
                self & <$ty>::wrapping_sub(self, 1)
            }

            #[inline]
            fn wrapping_sub(self, rhs: Self) -> Self {
                <$ty>::wrapping_sub(self, rhs)
            }

            #[inline]
            fn wrapping_add(self, rhs: Self) -> Self {
                <$ty>::wrapping_add(self, rhs)
            }

            #[inline]
            fn reverse_bits(self) -> Self {
                <$ty>::reverse_bits(self)
            }
        }
    };
}

impl_backing!(u64);
impl_backing!(u128);

/// A 256-bit unsigned integer backing, stored as two `u128` limbs.
///
/// This is the multi-word backing used by boards whose square count exceeds the
/// 128-bit ceiling of [`u128`] — in particular the 12x12 = 144-square Chu Shogi
/// board, and the larger shogi variants that build on it. It implements exactly
/// the same [`BitboardBacking`] surface as the primitive backings, in pure safe
/// Rust, with every operation carried across the 128-bit limb boundary by hand.
///
/// The value is little-endian: [`U256::lo`] holds bits `0..128` and
/// [`U256::hi`] holds bits `128..256`. Numeric ordering therefore compares
/// [`hi`](U256::hi) first, which the manual [`Ord`] impl does.
///
/// Adding this backing does not perturb the [`u64`] / [`u128`] backings: those
/// impls are untouched, so every pre-existing geometry stays byte-identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct U256 {
    /// The low 128 bits (square indices `0..128`).
    pub lo: u128,
    /// The high 128 bits (square indices `128..256`).
    pub hi: u128,
}

impl U256 {
    /// Constructs a value from its low and high 128-bit limbs.
    #[must_use]
    #[inline]
    pub const fn from_parts(lo: u128, hi: u128) -> Self {
        U256 { lo, hi }
    }

    /// Returns the value with exactly bit `n` set (`n < 256`).
    ///
    /// Used by the `const` mask builders, which cannot use the (non-`const`)
    /// operator impls.
    #[must_use]
    #[inline]
    pub const fn from_bit(n: u32) -> Self {
        if n < 128 {
            U256 {
                lo: 1u128 << n,
                hi: 0,
            }
        } else {
            U256 {
                lo: 0,
                hi: 1u128 << (n - 128),
            }
        }
    }
}

impl core::cmp::Ord for U256 {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // Compare the high limb first: it holds the more significant bits.
        match self.hi.cmp(&other.hi) {
            core::cmp::Ordering::Equal => self.lo.cmp(&other.lo),
            non_eq => non_eq,
        }
    }
}

impl core::cmp::PartialOrd for U256 {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl BitAnd for U256 {
    type Output = Self;
    #[inline]
    fn bitand(self, rhs: Self) -> Self {
        U256 {
            lo: self.lo & rhs.lo,
            hi: self.hi & rhs.hi,
        }
    }
}

impl BitOr for U256 {
    type Output = Self;
    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        U256 {
            lo: self.lo | rhs.lo,
            hi: self.hi | rhs.hi,
        }
    }
}

impl BitXor for U256 {
    type Output = Self;
    #[inline]
    fn bitxor(self, rhs: Self) -> Self {
        U256 {
            lo: self.lo ^ rhs.lo,
            hi: self.hi ^ rhs.hi,
        }
    }
}

impl Not for U256 {
    type Output = Self;
    #[inline]
    fn not(self) -> Self {
        U256 {
            lo: !self.lo,
            hi: !self.hi,
        }
    }
}

impl Shl<u32> for U256 {
    type Output = Self;
    #[inline]
    fn shl(self, s: u32) -> Self {
        if s == 0 {
            self
        } else if s >= 256 {
            U256 { lo: 0, hi: 0 }
        } else if s >= 128 {
            // The whole low limb moves into the high limb.
            let shift = s - 128;
            // `shift` is in `0..128`, so the `<<` is well-defined.
            U256 {
                lo: 0,
                hi: self.lo << shift,
            }
        } else {
            // `s` is in `1..128`, so `128 - s` is in `1..128`: both shifts are
            // well-defined and the spill from `lo` carries into `hi`.
            U256 {
                lo: self.lo << s,
                hi: (self.hi << s) | (self.lo >> (128 - s)),
            }
        }
    }
}

impl Shr<u32> for U256 {
    type Output = Self;
    #[inline]
    fn shr(self, s: u32) -> Self {
        if s == 0 {
            self
        } else if s >= 256 {
            U256 { lo: 0, hi: 0 }
        } else if s >= 128 {
            // The whole high limb moves into the low limb.
            let shift = s - 128;
            U256 {
                lo: self.hi >> shift,
                hi: 0,
            }
        } else {
            // `s` is in `1..128`, so `128 - s` is in `1..128`.
            U256 {
                lo: (self.lo >> s) | (self.hi << (128 - s)),
                hi: self.hi >> s,
            }
        }
    }
}

impl BitboardBacking for U256 {
    const ZERO: Self = U256 { lo: 0, hi: 0 };
    const ONE: Self = U256 { lo: 1, hi: 0 };
    const BITS: u32 = 256;

    #[inline]
    fn count_ones(self) -> u32 {
        self.lo.count_ones() + self.hi.count_ones()
    }

    #[inline]
    fn trailing_zeros(self) -> u32 {
        if self.lo != 0 {
            self.lo.trailing_zeros()
        } else {
            // `hi.trailing_zeros()` is `128` when `hi` is also zero, giving the
            // documented `BITS` (256) result for an all-zero value.
            128 + self.hi.trailing_zeros()
        }
    }

    #[inline]
    fn leading_zeros(self) -> u32 {
        if self.hi != 0 {
            self.hi.leading_zeros()
        } else {
            // `lo.leading_zeros()` is `128` when `lo` is also zero, giving the
            // documented `BITS` (256) result for an all-zero value.
            128 + self.lo.leading_zeros()
        }
    }

    #[inline]
    fn is_zero(self) -> bool {
        self.lo == 0 && self.hi == 0
    }

    #[inline]
    fn bit(shift: u32) -> Self {
        U256::from_bit(shift)
    }

    #[inline]
    fn clear_lowest(self) -> Self {
        self & self.wrapping_sub(U256 { lo: 1, hi: 0 })
    }

    #[inline]
    fn wrapping_sub(self, rhs: Self) -> Self {
        let lo = self.lo.wrapping_sub(rhs.lo);
        let borrow = (self.lo < rhs.lo) as u128;
        let hi = self.hi.wrapping_sub(rhs.hi).wrapping_sub(borrow);
        U256 { lo, hi }
    }

    #[inline]
    fn wrapping_add(self, rhs: Self) -> Self {
        let lo = self.lo.wrapping_add(rhs.lo);
        let carry = (lo < self.lo) as u128;
        let hi = self.hi.wrapping_add(rhs.hi).wrapping_add(carry);
        U256 { lo, hi }
    }

    #[inline]
    fn reverse_bits(self) -> Self {
        // Reversing 256 bits swaps the two limbs and reverses each: bit `i` of
        // `lo` (`i < 128`) maps to bit `255 - i`, which lives in `hi`.
        U256 {
            lo: self.hi.reverse_bits(),
            hi: self.lo.reverse_bits(),
        }
    }
}

#[cfg(test)]
mod u256_tests {
    use super::{BitboardBacking, U256};

    // ----- bit-vector reference model -----------------------------------------

    /// A plain 256-bool reference, little-endian (index 0 is the lowest bit).
    fn to_bits(v: U256) -> [bool; 256] {
        let mut out = [false; 256];
        for (i, b) in out.iter_mut().enumerate() {
            *b = if i < 128 {
                v.lo >> i & 1 == 1
            } else {
                v.hi >> (i - 128) & 1 == 1
            };
        }
        out
    }

    fn from_bits(bits: &[bool; 256]) -> U256 {
        let mut lo = 0u128;
        let mut hi = 0u128;
        for (i, &b) in bits.iter().enumerate() {
            if b {
                if i < 128 {
                    lo |= 1u128 << i;
                } else {
                    hi |= 1u128 << (i - 128);
                }
            }
        }
        U256 { lo, hi }
    }

    /// A handful of structurally interesting values, including limb-boundary and
    /// high-square (>= 128) patterns.
    fn samples() -> [U256; 9] {
        [
            U256::ZERO,
            U256::ONE,
            U256 {
                lo: u128::MAX,
                hi: 0,
            }, // exactly the low limb
            U256 { lo: 0, hi: 1 }, // bit 128 only
            U256 {
                lo: 0,
                hi: u128::MAX,
            }, // exactly the high limb
            U256 {
                lo: u128::MAX,
                hi: u128::MAX,
            }, // all ones
            U256::from_bit(127),   // top of low limb
            U256::from_bit(143),   // Chu's highest square
            U256 {
                lo: 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210,
                hi: 0xdead_beef_cafe_f00d,
            },
        ]
    }

    #[test]
    fn bit_constructor_matches_reference() {
        for n in 0..256u32 {
            let mut bits = [false; 256];
            bits[n as usize] = true;
            assert_eq!(U256::bit(n), from_bits(&bits), "bit({n})");
        }
    }

    #[test]
    fn shl_matches_reference_all_shifts() {
        for v in samples() {
            let bits = to_bits(v);
            for s in 0..=256u32 {
                let mut expect = [false; 256];
                for i in 0..256usize {
                    if bits[i] && (i + s as usize) < 256 {
                        expect[i + s as usize] = true;
                    }
                }
                assert_eq!(v << s, from_bits(&expect), "{v:?} << {s}");
            }
        }
    }

    #[test]
    fn shr_matches_reference_all_shifts() {
        for v in samples() {
            let bits = to_bits(v);
            for s in 0..=256u32 {
                let mut expect = [false; 256];
                for i in 0..256usize {
                    if bits[i] && i >= s as usize {
                        expect[i - s as usize] = true;
                    }
                }
                assert_eq!(v >> s, from_bits(&expect), "{v:?} >> {s}");
            }
        }
    }

    #[test]
    fn boundary_shifts_cross_the_limb_seam() {
        // A bit just below the seam, shifted up across it and back.
        let v = U256::from_bit(120);
        assert_eq!(
            v << 8,
            U256::from_bit(128),
            "120 << 8 lands at 128 (into hi)"
        );
        assert_eq!(v << 7, U256::from_bit(127), "120 << 7 stays at top of lo");
        assert_eq!(
            U256::from_bit(128) >> 1,
            U256::from_bit(127),
            "128 >> 1 drops into lo"
        );
        // Shift by exactly the seam.
        assert_eq!(U256::ONE << 128, U256::from_bit(128));
        assert_eq!(U256::from_bit(200) >> 128, U256::from_bit(72));
        // Over- and exact-width shifts annihilate.
        assert_eq!(
            U256 {
                lo: u128::MAX,
                hi: u128::MAX
            } << 256,
            U256::ZERO
        );
        assert_eq!(
            U256 {
                lo: u128::MAX,
                hi: u128::MAX
            } >> 256,
            U256::ZERO
        );
        assert_eq!(U256::ONE << 0, U256::ONE);
    }

    #[test]
    fn bitwise_ops_match_reference() {
        for a in samples() {
            for b in samples() {
                let (ba, bb) = (to_bits(a), to_bits(b));
                let mut and = [false; 256];
                let mut or = [false; 256];
                let mut xor = [false; 256];
                for i in 0..256 {
                    and[i] = ba[i] && bb[i];
                    or[i] = ba[i] || bb[i];
                    xor[i] = ba[i] ^ bb[i];
                }
                assert_eq!(a & b, from_bits(&and));
                assert_eq!(a | b, from_bits(&or));
                assert_eq!(a ^ b, from_bits(&xor));
            }
            let mut n = to_bits(a);
            for bit in n.iter_mut() {
                *bit = !*bit;
            }
            assert_eq!(!a, from_bits(&n));
        }
    }

    #[test]
    fn population_and_bit_scans() {
        for v in samples() {
            let bits = to_bits(v);
            let pop = bits.iter().filter(|&&b| b).count() as u32;
            assert_eq!(v.count_ones(), pop, "count_ones {v:?}");

            let tz = bits.iter().position(|&b| b).map_or(256, |i| i as u32);
            assert_eq!(v.trailing_zeros(), tz, "trailing_zeros {v:?}");

            let lz = bits
                .iter()
                .rposition(|&b| b)
                .map_or(256, |i| 255 - i as u32);
            assert_eq!(v.leading_zeros(), lz, "leading_zeros {v:?}");

            assert_eq!(v.is_zero(), pop == 0);
        }
        // All-zero edge cases hit the documented `BITS` (256) result.
        assert_eq!(U256::ZERO.trailing_zeros(), 256);
        assert_eq!(U256::ZERO.leading_zeros(), 256);
    }

    #[test]
    fn reverse_bits_is_involution_and_swaps_ends() {
        for v in samples() {
            assert_eq!(v.reverse_bits().reverse_bits(), v, "involution");
            let bits = to_bits(v);
            let mut rev = [false; 256];
            for i in 0..256 {
                rev[255 - i] = bits[i];
            }
            assert_eq!(v.reverse_bits(), from_bits(&rev), "explicit reversal");
        }
        assert_eq!(U256::ONE.reverse_bits(), U256::from_bit(255));
    }

    #[test]
    fn wrapping_add_sub_carry_across_seam() {
        // Carry propagates out of the low limb into the high limb.
        let max_lo = U256 {
            lo: u128::MAX,
            hi: 0,
        };
        assert_eq!(max_lo.wrapping_add(U256::ONE), U256 { lo: 0, hi: 1 });
        // Borrow propagates from the high limb into the low limb.
        assert_eq!(U256 { lo: 0, hi: 1 }.wrapping_sub(U256::ONE), max_lo);
        // Wrap of the whole value.
        let all = U256 {
            lo: u128::MAX,
            hi: u128::MAX,
        };
        assert_eq!(all.wrapping_add(U256::ONE), U256::ZERO);
        assert_eq!(U256::ZERO.wrapping_sub(U256::ONE), all);
        // add then sub round-trips for the structured sample.
        for a in samples() {
            for b in samples() {
                assert_eq!(a.wrapping_add(b).wrapping_sub(b), a);
            }
        }
    }

    #[test]
    fn clear_lowest_clears_one_bit() {
        for v in samples() {
            if v.is_zero() {
                assert_eq!(v.clear_lowest(), U256::ZERO);
                continue;
            }
            let cleared = v.clear_lowest();
            // Exactly one fewer bit, and it is the lowest one.
            assert_eq!(cleared.count_ones(), v.count_ones() - 1);
            assert_eq!(cleared | U256::bit(v.trailing_zeros()), v);
            assert!((cleared & U256::bit(v.trailing_zeros())).is_zero());
        }
    }

    #[test]
    fn ord_is_high_limb_major() {
        assert!(
            U256 {
                lo: u128::MAX,
                hi: 0
            } < U256 { lo: 0, hi: 1 }
        );
        assert!(U256 { lo: 5, hi: 7 } < U256 { lo: 3, hi: 8 });
        assert_eq!(
            U256::from_bit(143).cmp(&U256::from_bit(143)),
            core::cmp::Ordering::Equal
        );
        assert!(U256::ZERO < U256::ONE);
    }
}
