//! A generic set of board squares over an arbitrary [`Geometry`].
//!
//! This is the parallel generic analogue of the concrete [`crate::Bitboard`];
//! see the [module docs](super) for why the two hierarchies are separate.

use core::cmp::Ordering;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::ops::{
    BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not, Shl, ShlAssign, Shr,
    ShrAssign,
};

use super::backing::BitboardBacking;
use super::square::Square;
use super::Geometry;

/// A set of board squares for a board of geometry `G`, represented as a mask
/// over `G::Bits`.
///
/// Bit `i` corresponds to the square with index `i` in little-endian rank-file
/// order, exactly as the concrete [`crate::Bitboard`] does for 8x8. Bits at or
/// above `G::SQUARES` are never on the board; the set operations keep them
/// clear via [`Geometry::BOARD_MASK`].
#[repr(transparent)]
pub struct Bitboard<G: Geometry>(pub G::Bits);

// Manual impls so the geometry marker `G` (a zero-sized type) need not itself
// implement these traits; the bound is on `G::Bits` instead.
impl<G: Geometry> Clone for Bitboard<G> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<G: Geometry> Copy for Bitboard<G> {}

impl<G: Geometry> PartialEq for Bitboard<G> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<G: Geometry> Eq for Bitboard<G> {}

impl<G: Geometry> PartialOrd for Bitboard<G>
where
    G::Bits: Ord,
{
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<G: Geometry> Ord for Bitboard<G>
where
    G::Bits: Ord,
{
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl<G: Geometry> Hash for Bitboard<G>
where
    G::Bits: Hash,
{
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<G: Geometry> Default for Bitboard<G> {
    #[inline]
    fn default() -> Self {
        Bitboard::EMPTY
    }
}

impl<G: Geometry> Bitboard<G> {
    /// The empty set.
    pub const EMPTY: Bitboard<G> = Bitboard(G::Bits::ZERO);

    /// The set of all `G::SQUARES` on-board squares (the `SQUARES` low bits).
    ///
    /// Unlike the concrete `u64` `FULL` (which is `!0`), this is masked to the
    /// board so off-board high bits stay clear even for partial-width backings.
    pub const FULL: Bitboard<G> = Bitboard(G::BOARD_MASK);

    /// The first file (file `0`).
    pub const FILE_A: Bitboard<G> = Bitboard(G::FILE_A_MASK);

    /// The last file (file `WIDTH - 1`).
    pub const LAST_FILE: Bitboard<G> = Bitboard(G::LAST_FILE_MASK);

    /// Returns the bitboard containing only the given square.
    #[must_use]
    #[inline]
    pub fn from_square(square: Square<G>) -> Bitboard<G> {
        Bitboard(G::Bits::bit(square.index() as u32))
    }

    /// Returns `true` if the set contains no squares.
    #[must_use]
    #[inline]
    pub fn is_empty(self) -> bool {
        self.0.is_zero()
    }

    /// Returns `true` if the set contains the given square.
    #[must_use]
    #[inline]
    pub fn contains(self, square: Square<G>) -> bool {
        !(self.0 & G::Bits::bit(square.index() as u32)).is_zero()
    }

    /// Adds the given square to the set.
    #[inline]
    pub fn set(&mut self, square: Square<G>) {
        self.0 = self.0 | G::Bits::bit(square.index() as u32);
    }

    /// Removes the given square from the set.
    #[inline]
    pub fn clear(&mut self, square: Square<G>) {
        self.0 = self.0 & !G::Bits::bit(square.index() as u32);
    }

    /// Toggles membership of the given square.
    #[inline]
    pub fn toggle(&mut self, square: Square<G>) {
        self.0 = self.0 ^ G::Bits::bit(square.index() as u32);
    }

    /// Returns a copy with the given square added.
    #[must_use]
    #[inline]
    pub fn with(self, square: Square<G>) -> Bitboard<G> {
        Bitboard(self.0 | G::Bits::bit(square.index() as u32))
    }

    /// Returns a copy with the given square removed.
    #[must_use]
    #[inline]
    pub fn without(self, square: Square<G>) -> Bitboard<G> {
        Bitboard(self.0 & !G::Bits::bit(square.index() as u32))
    }

    /// Returns the number of squares in the set.
    #[must_use]
    #[inline]
    pub fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// Returns the least-significant set square (the lowest index), or `None`
    /// if the set is empty.
    #[must_use]
    #[inline]
    pub fn lsb(self) -> Option<Square<G>> {
        if self.0.is_zero() {
            None
        } else {
            // `trailing_zeros` is in `0..SQUARES` for a non-zero on-board value.
            Some(Square::new(self.0.trailing_zeros() as u8))
        }
    }

    /// Removes and returns the least-significant set square, or `None` if the
    /// set is empty.
    #[inline]
    pub fn pop_lsb(&mut self) -> Option<Square<G>> {
        let square = self.lsb()?;
        self.0 = self.0.clear_lowest();
        Some(square)
    }

    /// Shifts every square one rank toward the last rank (north), dropping
    /// squares that leave the board.
    #[must_use]
    #[inline]
    pub fn north(self) -> Bitboard<G> {
        Bitboard((self.0 << G::WIDTH as u32) & G::BOARD_MASK)
    }

    /// Shifts every square one rank toward the first rank (south).
    #[must_use]
    #[inline]
    pub fn south(self) -> Bitboard<G> {
        Bitboard(self.0 >> G::WIDTH as u32)
    }

    /// Shifts every square one file toward the last file (east), masking off
    /// squares that would wrap past the last file.
    #[must_use]
    #[inline]
    pub fn east(self) -> Bitboard<G> {
        Bitboard(((self.0 & !G::LAST_FILE_MASK) << 1) & G::BOARD_MASK)
    }

    /// Shifts every square one file toward the first file (west), masking off
    /// squares that would wrap past the first file.
    #[must_use]
    #[inline]
    pub fn west(self) -> Bitboard<G> {
        Bitboard((self.0 & !G::FILE_A_MASK) >> 1)
    }

    /// Shifts one square north-east.
    #[must_use]
    #[inline]
    pub fn north_east(self) -> Bitboard<G> {
        Bitboard(((self.0 & !G::LAST_FILE_MASK) << (G::WIDTH as u32 + 1)) & G::BOARD_MASK)
    }

    /// Shifts one square north-west.
    #[must_use]
    #[inline]
    pub fn north_west(self) -> Bitboard<G> {
        Bitboard(((self.0 & !G::FILE_A_MASK) << (G::WIDTH as u32 - 1)) & G::BOARD_MASK)
    }

    /// Shifts one square south-east.
    #[must_use]
    #[inline]
    pub fn south_east(self) -> Bitboard<G> {
        Bitboard((self.0 & !G::LAST_FILE_MASK) >> (G::WIDTH as u32 - 1))
    }

    /// Shifts one square south-west.
    #[must_use]
    #[inline]
    pub fn south_west(self) -> Bitboard<G> {
        Bitboard((self.0 & !G::FILE_A_MASK) >> (G::WIDTH as u32 + 1))
    }
}

/// Iterator over the set squares of a [`Bitboard`], yielded lowest-index-first.
pub struct Squares<G: Geometry>(Bitboard<G>);

impl<G: Geometry> fmt::Debug for Squares<G>
where
    G::Bits: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Squares").field(&self.0).finish()
    }
}

impl<G: Geometry> Clone for Squares<G> {
    #[inline]
    fn clone(&self) -> Self {
        Squares(self.0)
    }
}

impl<G: Geometry> Iterator for Squares<G> {
    type Item = Square<G>;

    #[inline]
    fn next(&mut self) -> Option<Square<G>> {
        self.0.pop_lsb()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.0.count() as usize;
        (len, Some(len))
    }
}

impl<G: Geometry> ExactSizeIterator for Squares<G> {
    #[inline]
    fn len(&self) -> usize {
        self.0.count() as usize
    }
}

impl<G: Geometry> core::iter::FusedIterator for Squares<G> {}

impl<G: Geometry> IntoIterator for Bitboard<G> {
    type Item = Square<G>;
    type IntoIter = Squares<G>;

    #[inline]
    fn into_iter(self) -> Squares<G> {
        Squares(self)
    }
}

impl<G: Geometry> FromIterator<Square<G>> for Bitboard<G> {
    fn from_iter<I: IntoIterator<Item = Square<G>>>(iter: I) -> Bitboard<G> {
        let mut bb = Bitboard::EMPTY;
        for square in iter {
            bb.set(square);
        }
        bb
    }
}

impl<G: Geometry> Extend<Square<G>> for Bitboard<G> {
    fn extend<I: IntoIterator<Item = Square<G>>>(&mut self, iter: I) {
        for square in iter {
            self.set(square);
        }
    }
}

impl<G: Geometry> From<Square<G>> for Bitboard<G> {
    #[inline]
    fn from(square: Square<G>) -> Bitboard<G> {
        Bitboard::from_square(square)
    }
}

macro_rules! bitwise_binop {
    ($trait:ident, $method:ident, $op:tt, $assign_trait:ident, $assign_method:ident) => {
        impl<G: Geometry> $trait for Bitboard<G> {
            type Output = Bitboard<G>;
            #[inline]
            fn $method(self, rhs: Bitboard<G>) -> Bitboard<G> {
                Bitboard(self.0 $op rhs.0)
            }
        }

        impl<G: Geometry> $assign_trait for Bitboard<G> {
            #[inline]
            fn $assign_method(&mut self, rhs: Bitboard<G>) {
                self.0 = self.0 $op rhs.0;
            }
        }
    };
}

bitwise_binop!(BitAnd, bitand, &, BitAndAssign, bitand_assign);
bitwise_binop!(BitOr, bitor, |, BitOrAssign, bitor_assign);
bitwise_binop!(BitXor, bitxor, ^, BitXorAssign, bitxor_assign);

impl<G: Geometry> Not for Bitboard<G> {
    type Output = Bitboard<G>;
    /// Complements the set within the board (off-board high bits stay clear).
    #[inline]
    fn not(self) -> Bitboard<G> {
        Bitboard(!self.0 & G::BOARD_MASK)
    }
}

impl<G: Geometry> Shl<u32> for Bitboard<G> {
    type Output = Bitboard<G>;
    #[inline]
    fn shl(self, rhs: u32) -> Bitboard<G> {
        Bitboard((self.0 << rhs) & G::BOARD_MASK)
    }
}

impl<G: Geometry> ShlAssign<u32> for Bitboard<G> {
    #[inline]
    fn shl_assign(&mut self, rhs: u32) {
        self.0 = (self.0 << rhs) & G::BOARD_MASK;
    }
}

impl<G: Geometry> Shr<u32> for Bitboard<G> {
    type Output = Bitboard<G>;
    #[inline]
    fn shr(self, rhs: u32) -> Bitboard<G> {
        Bitboard(self.0 >> rhs)
    }
}

impl<G: Geometry> ShrAssign<u32> for Bitboard<G> {
    #[inline]
    fn shr_assign(&mut self, rhs: u32) {
        self.0 = self.0 >> rhs;
    }
}

impl<G: Geometry> fmt::Debug for Bitboard<G>
where
    G::Bits: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Bitboard").field(&self.0).finish()
    }
}

impl<G: Geometry> fmt::Display for Bitboard<G> {
    /// Renders the board as `HEIGHT` rows of `WIDTH` cells, the last rank at the
    /// top, using `#` for occupied squares and `.` for empty ones.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let width = G::WIDTH;
        for rank in (0..G::HEIGHT).rev() {
            for file in 0..width {
                let square = Square::new(rank * width + file);
                f.write_str(if self.contains(square) { "#" } else { "." })?;
                if file + 1 < width {
                    f.write_str(" ")?;
                }
            }
            if rank > 0 {
                f.write_str("\n")?;
            }
        }
        Ok(())
    }
}
