//! The integer backing for a generic [`Bitboard`](super::Bitboard).
//!
//! A [`BitboardBacking`] is the minimal set of bit operations a generic
//! bitboard needs from its underlying unsigned integer, implemented here for
//! [`u64`] (the frozen 8x8 width) and [`u128`] (everything from 9x9 up to a
//! 128-square board). The trait is deliberately tiny: only the operations the
//! bitboard layer actually uses, all `safe`, all `const`-friendly.

use core::ops::{BitAnd, BitOr, BitXor, Not, Shl, Shr};

/// An unsigned integer usable as the backing store of a generic bitboard.
///
/// The bound list is exactly the algebra the bitboard layer needs: the bitwise
/// operators (with their right-hand side being `Self`), shifts by a `u32`, and
/// the inherent population-count / bit-scan helpers exposed as trait methods so
/// they can be called on a generic `Self`.
///
/// Implemented for [`u64`] and [`u128`]. No `unsafe` is involved anywhere.
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

    /// The total number of bits in this integer (`64` or `128`).
    const BITS: u32;

    /// Returns the number of set bits.
    fn count_ones(self) -> u32;

    /// Returns the number of trailing zero bits (the index of the lowest set
    /// bit). For a zero value this is [`Self::BITS`].
    fn trailing_zeros(self) -> u32;

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
            fn is_zero(self) -> bool {
                self == 0
            }

            #[inline]
            fn bit(shift: u32) -> Self {
                1 << shift
            }

            #[inline]
            fn clear_lowest(self) -> Self {
                self & self.wrapping_sub(1)
            }
        }
    };
}

impl_backing!(u64);
impl_backing!(u128);
