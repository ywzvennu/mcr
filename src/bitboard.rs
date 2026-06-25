//! A 64-bit set of squares.

use core::fmt;
use core::ops::{
    BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not, Shl, ShlAssign, Shr,
    ShrAssign,
};

use crate::Square;

/// A set of board squares, represented as a 64-bit mask.
///
/// Bit `i` (value `1 << i`) corresponds to the square with index `i` in
/// little-endian rank-file order, so bit `0` is `a1` and bit `63` is `h8`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct Bitboard(pub u64);

impl Bitboard {
    /// The empty set.
    pub const EMPTY: Bitboard = Bitboard(0);

    /// The set of all 64 squares.
    pub const FULL: Bitboard = Bitboard(!0);

    /// The `a`-file.
    pub const FILE_A: Bitboard = Bitboard(0x0101_0101_0101_0101);
    /// The `b`-file.
    pub const FILE_B: Bitboard = Bitboard(Self::FILE_A.0 << 1);
    /// The `c`-file.
    pub const FILE_C: Bitboard = Bitboard(Self::FILE_A.0 << 2);
    /// The `d`-file.
    pub const FILE_D: Bitboard = Bitboard(Self::FILE_A.0 << 3);
    /// The `e`-file.
    pub const FILE_E: Bitboard = Bitboard(Self::FILE_A.0 << 4);
    /// The `f`-file.
    pub const FILE_F: Bitboard = Bitboard(Self::FILE_A.0 << 5);
    /// The `g`-file.
    pub const FILE_G: Bitboard = Bitboard(Self::FILE_A.0 << 6);
    /// The `h`-file.
    pub const FILE_H: Bitboard = Bitboard(Self::FILE_A.0 << 7);

    /// Rank 1.
    pub const RANK_1: Bitboard = Bitboard(0x0000_0000_0000_00ff);
    /// Rank 2.
    pub const RANK_2: Bitboard = Bitboard(Self::RANK_1.0 << 8);
    /// Rank 3.
    pub const RANK_3: Bitboard = Bitboard(Self::RANK_1.0 << 16);
    /// Rank 4.
    pub const RANK_4: Bitboard = Bitboard(Self::RANK_1.0 << 24);
    /// Rank 5.
    pub const RANK_5: Bitboard = Bitboard(Self::RANK_1.0 << 32);
    /// Rank 6.
    pub const RANK_6: Bitboard = Bitboard(Self::RANK_1.0 << 40);
    /// Rank 7.
    pub const RANK_7: Bitboard = Bitboard(Self::RANK_1.0 << 48);
    /// Rank 8.
    pub const RANK_8: Bitboard = Bitboard(Self::RANK_1.0 << 56);

    /// The squares on the outer edge of the board (files `a`/`h`, ranks `1`/`8`).
    pub const EDGES: Bitboard =
        Bitboard(Self::FILE_A.0 | Self::FILE_H.0 | Self::RANK_1.0 | Self::RANK_8.0);

    /// Returns the bitboard containing only the given square.
    ///
    /// ```
    /// use mce::{Bitboard, Square};
    /// assert_eq!(Bitboard::from_square(Square::A1), Bitboard(1));
    /// ```
    #[must_use]
    #[inline]
    pub const fn from_square(square: Square) -> Bitboard {
        Bitboard(1u64 << square.index())
    }

    /// Returns `true` if the set contains no squares.
    #[must_use]
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns `true` if the set contains the given square.
    #[must_use]
    #[inline]
    pub const fn contains(self, square: Square) -> bool {
        self.0 & (1u64 << square.index()) != 0
    }

    /// Adds the given square to the set.
    #[inline]
    pub fn set(&mut self, square: Square) {
        self.0 |= 1u64 << square.index();
    }

    /// Removes the given square from the set.
    #[inline]
    pub fn clear(&mut self, square: Square) {
        self.0 &= !(1u64 << square.index());
    }

    /// Toggles membership of the given square.
    #[inline]
    pub fn toggle(&mut self, square: Square) {
        self.0 ^= 1u64 << square.index();
    }

    /// Returns a copy with the given square added.
    #[must_use]
    #[inline]
    pub const fn with(self, square: Square) -> Bitboard {
        Bitboard(self.0 | (1u64 << square.index()))
    }

    /// Returns a copy with the given square removed.
    #[must_use]
    #[inline]
    pub const fn without(self, square: Square) -> Bitboard {
        Bitboard(self.0 & !(1u64 << square.index()))
    }

    /// Returns the number of squares in the set.
    #[must_use]
    #[inline]
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// Returns the least-significant set square (the lowest index), or `None` if
    /// the set is empty.
    ///
    /// ```
    /// use mce::{Bitboard, Square};
    /// let bb = Bitboard::from_square(Square::C1) | Bitboard::from_square(Square::A1);
    /// assert_eq!(bb.lsb(), Some(Square::A1));
    /// assert_eq!(Bitboard::EMPTY.lsb(), None);
    /// ```
    #[must_use]
    #[inline]
    pub const fn lsb(self) -> Option<Square> {
        if self.0 == 0 {
            None
        } else {
            // `trailing_zeros` is in `0..64` for a non-zero value.
            Some(Square::new(self.0.trailing_zeros() as u8))
        }
    }

    /// Removes and returns the least-significant set square, or `None` if the set
    /// is empty.
    ///
    /// ```
    /// use mce::{Bitboard, Square};
    /// let mut bb = Bitboard::from_square(Square::A1) | Bitboard::from_square(Square::C1);
    /// assert_eq!(bb.pop_lsb(), Some(Square::A1));
    /// assert_eq!(bb.pop_lsb(), Some(Square::C1));
    /// assert_eq!(bb.pop_lsb(), None);
    /// ```
    #[inline]
    pub fn pop_lsb(&mut self) -> Option<Square> {
        let square = self.lsb()?;
        // Clear the lowest set bit.
        self.0 &= self.0 - 1;
        Some(square)
    }

    /// Shifts every square one rank toward rank 8 (north), dropping squares that
    /// leave the board.
    #[must_use]
    #[inline]
    pub const fn north(self) -> Bitboard {
        Bitboard(self.0 << 8)
    }

    /// Shifts every square one rank toward rank 1 (south).
    #[must_use]
    #[inline]
    pub const fn south(self) -> Bitboard {
        Bitboard(self.0 >> 8)
    }

    /// Shifts every square one file toward the `h`-file (east), masking off
    /// squares that would wrap past the `h`-file.
    #[must_use]
    #[inline]
    pub const fn east(self) -> Bitboard {
        Bitboard((self.0 & !Self::FILE_H.0) << 1)
    }

    /// Shifts every square one file toward the `a`-file (west), masking off
    /// squares that would wrap past the `a`-file.
    #[must_use]
    #[inline]
    pub const fn west(self) -> Bitboard {
        Bitboard((self.0 & !Self::FILE_A.0) >> 1)
    }

    /// Shifts one square north-east.
    #[must_use]
    #[inline]
    pub const fn north_east(self) -> Bitboard {
        Bitboard((self.0 & !Self::FILE_H.0) << 9)
    }

    /// Shifts one square north-west.
    #[must_use]
    #[inline]
    pub const fn north_west(self) -> Bitboard {
        Bitboard((self.0 & !Self::FILE_A.0) << 7)
    }

    /// Shifts one square south-east.
    #[must_use]
    #[inline]
    pub const fn south_east(self) -> Bitboard {
        Bitboard((self.0 & !Self::FILE_H.0) >> 7)
    }

    /// Shifts one square south-west.
    #[must_use]
    #[inline]
    pub const fn south_west(self) -> Bitboard {
        Bitboard((self.0 & !Self::FILE_A.0) >> 9)
    }
}

/// Iterator over the set squares of a [`Bitboard`], yielded lowest-index-first.
#[derive(Debug, Clone)]
pub struct Squares(Bitboard);

impl Iterator for Squares {
    type Item = Square;

    #[inline]
    fn next(&mut self) -> Option<Square> {
        self.0.pop_lsb()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.0.count() as usize;
        (len, Some(len))
    }
}

impl ExactSizeIterator for Squares {
    #[inline]
    fn len(&self) -> usize {
        self.0.count() as usize
    }
}

impl core::iter::FusedIterator for Squares {}

impl IntoIterator for Bitboard {
    type Item = Square;
    type IntoIter = Squares;

    #[inline]
    fn into_iter(self) -> Squares {
        Squares(self)
    }
}

impl FromIterator<Square> for Bitboard {
    fn from_iter<I: IntoIterator<Item = Square>>(iter: I) -> Bitboard {
        let mut bb = Bitboard::EMPTY;
        for square in iter {
            bb.set(square);
        }
        bb
    }
}

impl Extend<Square> for Bitboard {
    fn extend<I: IntoIterator<Item = Square>>(&mut self, iter: I) {
        for square in iter {
            self.set(square);
        }
    }
}

impl From<Square> for Bitboard {
    #[inline]
    fn from(square: Square) -> Bitboard {
        Bitboard::from_square(square)
    }
}

macro_rules! bitwise_binop {
    ($trait:ident, $method:ident, $op:tt, $assign_trait:ident, $assign_method:ident, $assign_op:tt) => {
        impl $trait for Bitboard {
            type Output = Bitboard;
            #[inline]
            fn $method(self, rhs: Bitboard) -> Bitboard {
                Bitboard(self.0 $op rhs.0)
            }
        }

        impl $assign_trait for Bitboard {
            #[inline]
            fn $assign_method(&mut self, rhs: Bitboard) {
                self.0 $assign_op rhs.0;
            }
        }
    };
}

bitwise_binop!(BitAnd, bitand, &, BitAndAssign, bitand_assign, &=);
bitwise_binop!(BitOr, bitor, |, BitOrAssign, bitor_assign, |=);
bitwise_binop!(BitXor, bitxor, ^, BitXorAssign, bitxor_assign, ^=);

impl Not for Bitboard {
    type Output = Bitboard;
    #[inline]
    fn not(self) -> Bitboard {
        Bitboard(!self.0)
    }
}

impl Shl<u32> for Bitboard {
    type Output = Bitboard;
    #[inline]
    fn shl(self, rhs: u32) -> Bitboard {
        Bitboard(self.0 << rhs)
    }
}

impl ShlAssign<u32> for Bitboard {
    #[inline]
    fn shl_assign(&mut self, rhs: u32) {
        self.0 <<= rhs;
    }
}

impl Shr<u32> for Bitboard {
    type Output = Bitboard;
    #[inline]
    fn shr(self, rhs: u32) -> Bitboard {
        Bitboard(self.0 >> rhs)
    }
}

impl ShrAssign<u32> for Bitboard {
    #[inline]
    fn shr_assign(&mut self, rhs: u32) {
        self.0 >>= rhs;
    }
}

impl fmt::Display for Bitboard {
    /// Renders the board as eight rows of eight cells, rank 8 at the top, using
    /// `#` for occupied squares and `.` for empty ones.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for rank in (0..8).rev() {
            for file in 0..8 {
                let square = Square::new(rank * 8 + file);
                f.write_str(if self.contains(square) { "#" } else { "." })?;
                if file < 7 {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Square;

    #[test]
    fn empty_and_full() {
        assert!(Bitboard::EMPTY.is_empty());
        assert_eq!(Bitboard::EMPTY.count(), 0);
        assert!(!Bitboard::FULL.is_empty());
        assert_eq!(Bitboard::FULL.count(), 64);
        assert_eq!(!Bitboard::EMPTY, Bitboard::FULL);
        assert_eq!(!Bitboard::FULL, Bitboard::EMPTY);
    }

    #[test]
    fn set_clear_contains_toggle() {
        let mut bb = Bitboard::EMPTY;
        assert!(!bb.contains(Square::E4));
        bb.set(Square::E4);
        assert!(bb.contains(Square::E4));
        assert_eq!(bb.count(), 1);
        bb.set(Square::E4);
        assert_eq!(bb.count(), 1);
        bb.clear(Square::E4);
        assert!(!bb.contains(Square::E4));
        bb.toggle(Square::A1);
        assert!(bb.contains(Square::A1));
        bb.toggle(Square::A1);
        assert!(!bb.contains(Square::A1));
    }

    #[test]
    fn from_square_matches_with_without() {
        for index in 0..64u8 {
            let square = Square::new(index);
            let bb = Bitboard::from_square(square);
            assert_eq!(bb.count(), 1);
            assert!(bb.contains(square));
            assert_eq!(bb, Bitboard::from(square));
            assert_eq!(Bitboard::EMPTY.with(square), bb);
            assert_eq!(bb.without(square), Bitboard::EMPTY);
        }
    }

    #[test]
    fn lsb_and_pop_order() {
        let squares = [Square::C1, Square::A1, Square::H8, Square::E4];
        let bb: Bitboard = squares.into_iter().collect();
        assert_eq!(bb.count(), 4);
        assert_eq!(bb.lsb(), Some(Square::A1));

        let mut work = bb;
        let mut popped = Vec::new();
        while let Some(sq) = work.pop_lsb() {
            popped.push(sq);
        }
        // pop_lsb yields lowest index first.
        assert_eq!(popped, vec![Square::A1, Square::C1, Square::E4, Square::H8]);
        assert!(work.is_empty());

        assert_eq!(Bitboard::EMPTY.lsb(), None);
        let mut empty = Bitboard::EMPTY;
        assert_eq!(empty.pop_lsb(), None);
    }

    #[test]
    fn iteration_is_lsb_first_and_complete() {
        let collected: Vec<Square> = Bitboard::FULL.into_iter().collect();
        assert_eq!(collected.len(), 64);
        for (i, square) in collected.iter().enumerate() {
            assert_eq!(square.index() as usize, i);
        }

        let bb: Bitboard = [Square::H8, Square::A1, Square::D4].into_iter().collect();
        let order: Vec<Square> = bb.into_iter().collect();
        assert_eq!(order, vec![Square::A1, Square::D4, Square::H8]);
        assert_eq!(bb.into_iter().len(), 3);
    }

    #[test]
    fn bitwise_operators() {
        let a = Bitboard::FILE_A;
        let one = Bitboard::RANK_1;
        assert_eq!((a & one), Bitboard::from_square(Square::A1));
        assert_eq!((a | one).count(), 8 + 8 - 1);
        assert_eq!((a ^ a), Bitboard::EMPTY);

        let mut x = Bitboard::EMPTY;
        x |= Bitboard::from_square(Square::E4);
        assert!(x.contains(Square::E4));
        x &= Bitboard::FULL;
        assert!(x.contains(Square::E4));
        x ^= Bitboard::from_square(Square::E4);
        assert!(x.is_empty());
    }

    #[test]
    fn shift_operators() {
        assert_eq!(Bitboard(1) << 1, Bitboard(2));
        assert_eq!(Bitboard(2) >> 1, Bitboard(1));
        let mut x = Bitboard(1);
        x <<= 3;
        assert_eq!(x, Bitboard(8));
        x >>= 2;
        assert_eq!(x, Bitboard(2));
    }

    #[test]
    fn file_and_rank_consts() {
        assert_eq!(Bitboard::FILE_A.count(), 8);
        assert_eq!(Bitboard::RANK_1.count(), 8);
        assert!(Bitboard::FILE_A.contains(Square::A1));
        assert!(Bitboard::FILE_A.contains(Square::A8));
        assert!(!Bitboard::FILE_A.contains(Square::B1));
        assert!(Bitboard::FILE_H.contains(Square::H4));
        assert!(Bitboard::RANK_8.contains(Square::A8));
        assert!(Bitboard::RANK_8.contains(Square::H8));
        // Files partition the board.
        let union = Bitboard::FILE_A
            | Bitboard::FILE_B
            | Bitboard::FILE_C
            | Bitboard::FILE_D
            | Bitboard::FILE_E
            | Bitboard::FILE_F
            | Bitboard::FILE_G
            | Bitboard::FILE_H;
        assert_eq!(union, Bitboard::FULL);
    }

    #[test]
    fn edges_const() {
        assert_eq!(Bitboard::EDGES.count(), 28);
        assert!(Bitboard::EDGES.contains(Square::A1));
        assert!(Bitboard::EDGES.contains(Square::H8));
        assert!(!Bitboard::EDGES.contains(Square::D4));
    }

    #[test]
    fn directional_shift_masking() {
        // East of the h-file wraps off the board: empty.
        assert_eq!(Bitboard::FILE_H.east(), Bitboard::EMPTY);
        // West of the a-file is empty.
        assert_eq!(Bitboard::FILE_A.west(), Bitboard::EMPTY);
        // North of rank 8 is empty.
        assert_eq!(Bitboard::RANK_8.north(), Bitboard::EMPTY);
        // South of rank 1 is empty.
        assert_eq!(Bitboard::RANK_1.south(), Bitboard::EMPTY);

        // No wraparound on a single square at the edges.
        assert_eq!(Bitboard::from_square(Square::H4).east(), Bitboard::EMPTY);
        assert_eq!(Bitboard::from_square(Square::A4).west(), Bitboard::EMPTY);
        assert_eq!(
            Bitboard::from_square(Square::H4).west(),
            Bitboard::from_square(Square::G4)
        );
        assert_eq!(
            Bitboard::from_square(Square::A4).east(),
            Bitboard::from_square(Square::B4)
        );

        // Interior square moves cleanly in all eight directions.
        let e4 = Bitboard::from_square(Square::E4);
        assert_eq!(e4.north(), Bitboard::from_square(Square::E5));
        assert_eq!(e4.south(), Bitboard::from_square(Square::E3));
        assert_eq!(e4.east(), Bitboard::from_square(Square::F4));
        assert_eq!(e4.west(), Bitboard::from_square(Square::D4));
        assert_eq!(e4.north_east(), Bitboard::from_square(Square::F5));
        assert_eq!(e4.north_west(), Bitboard::from_square(Square::D5));
        assert_eq!(e4.south_east(), Bitboard::from_square(Square::F3));
        assert_eq!(e4.south_west(), Bitboard::from_square(Square::D3));
    }

    #[test]
    fn diagonal_shift_masking_at_corners() {
        assert_eq!(
            Bitboard::from_square(Square::H8).north_east(),
            Bitboard::EMPTY
        );
        assert_eq!(
            Bitboard::from_square(Square::A8).north_west(),
            Bitboard::EMPTY
        );
        assert_eq!(
            Bitboard::from_square(Square::H1).south_east(),
            Bitboard::EMPTY
        );
        assert_eq!(
            Bitboard::from_square(Square::A1).south_west(),
            Bitboard::EMPTY
        );
        // h-file moving north-east must not wrap to the a-file.
        assert_eq!(
            Bitboard::from_square(Square::H4).north_east(),
            Bitboard::EMPTY
        );
        assert_eq!(
            Bitboard::from_square(Square::A4).north_west(),
            Bitboard::EMPTY
        );
    }

    #[test]
    fn extend_and_from_iter() {
        let mut bb = Bitboard::EMPTY;
        bb.extend([Square::A1, Square::B2]);
        assert_eq!(bb.count(), 2);
        assert!(bb.contains(Square::A1));
        assert!(bb.contains(Square::B2));
    }

    #[test]
    fn display_renders_grid() {
        let bb = Bitboard::from_square(Square::A1);
        let text = bb.to_string();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 8);
        // a1 is the bottom-left cell.
        assert_eq!(lines[7], "# . . . . . . .");
        assert_eq!(lines[0], ". . . . . . . .");
    }
}
