//! Board squares, indexed `0..64` in little-endian rank-file order.

use alloc::borrow::ToOwned;
use alloc::string::String;
#[cfg(test)]
use alloc::string::ToString;
use core::fmt;
use core::str::FromStr;

use crate::{File, Rank};

/// A square of the chessboard.
///
/// Squares are numbered `0..64` using the little-endian rank-file mapping: the
/// index is `rank * 8 + file`, so `a1 == 0`, `b1 == 1`, …, `h1 == 7`, `a2 == 8`,
/// …, `h8 == 63`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Square(u8);

/// The error returned when constructing a [`Square`] from an out-of-range index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InvalidSquareIndex(pub u8);

impl fmt::Display for InvalidSquareIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "square index {} is out of range (must be 0..64)", self.0)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for InvalidSquareIndex {}

/// The error returned when parsing a [`Square`] from an algebraic string fails.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParseSquareError(String);

impl fmt::Display for ParseSquareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid square {:?}: expected algebraic form like \"e4\"",
            self.0
        )
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ParseSquareError {}

impl Square {
    /// Creates a square from its index, panicking if `index >= 64`.
    ///
    /// Prefer [`Square::try_new`] when the index is not statically known.
    ///
    /// # Panics
    ///
    /// Panics if `index >= 64`.
    #[must_use]
    #[inline]
    pub const fn new(index: u8) -> Square {
        assert!(index < 64, "square index out of range");
        Square(index)
    }

    /// Creates a square from its index, returning `None` if `index >= 64`.
    #[must_use]
    #[inline]
    pub const fn try_new(index: u8) -> Option<Square> {
        if index < 64 {
            Some(Square(index))
        } else {
            None
        }
    }

    /// Builds a square from a file and rank.
    ///
    /// ```
    /// use mcr::{File, Rank, Square};
    /// assert_eq!(Square::from_file_rank(File::E, Rank::Fourth), Square::new(28));
    /// ```
    #[must_use]
    #[inline]
    pub const fn from_file_rank(file: File, rank: Rank) -> Square {
        Square(rank.index() * 8 + file.index())
    }

    /// Returns the zero-based index of this square (`0..64`).
    #[must_use]
    #[inline]
    pub const fn index(self) -> u8 {
        self.0
    }

    /// Returns the file of this square.
    #[must_use]
    #[inline]
    pub const fn file(self) -> File {
        // `self.0 % 8` is always in `0..8`, so this never panics.
        match File::new(self.0 % 8) {
            Some(file) => file,
            None => unreachable!(),
        }
    }

    /// Returns the rank of this square.
    #[must_use]
    #[inline]
    pub const fn rank(self) -> Rank {
        // `self.0 / 8` is always in `0..8`, so this never panics.
        match Rank::new(self.0 / 8) {
            Some(rank) => rank,
            None => unreachable!(),
        }
    }

    /// Returns the square `df` files east and `dr` ranks north of this one,
    /// or `None` if the destination falls off the board.
    ///
    /// Negative deltas move west / south.
    ///
    /// ```
    /// use mcr::Square;
    /// // e4 one file east, one rank north is f5.
    /// assert_eq!(Square::new(28).offset(1, 1), Some(Square::new(37)));
    /// // a1 cannot move west.
    /// assert_eq!(Square::new(0).offset(-1, 0), None);
    /// ```
    #[must_use]
    #[inline]
    pub const fn offset(self, df: i8, dr: i8) -> Option<Square> {
        let file = match self.file().offset(df) {
            Some(file) => file,
            None => return None,
        };
        let rank = match self.rank().offset(dr) {
            Some(rank) => rank,
            None => return None,
        };
        Some(Square::from_file_rank(file, rank))
    }
}

impl TryFrom<u8> for Square {
    type Error = InvalidSquareIndex;

    #[inline]
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Square::try_new(value).ok_or(InvalidSquareIndex(value))
    }
}

impl From<Square> for u8 {
    #[inline]
    fn from(square: Square) -> u8 {
        square.index()
    }
}

impl From<Square> for usize {
    #[inline]
    fn from(square: Square) -> usize {
        square.index() as usize
    }
}

impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.file(), self.rank())
    }
}

impl FromStr for Square {
    type Err = ParseSquareError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut chars = s.chars();
        let err = || ParseSquareError(s.to_owned());
        let file_ch = chars.next().ok_or_else(err)?;
        let rank_ch = chars.next().ok_or_else(err)?;
        if chars.next().is_some() {
            return Err(err());
        }
        let file = File::from_char(file_ch).ok_or_else(err)?;
        let rank = Rank::from_char(rank_ch).ok_or_else(err)?;
        Ok(Square::from_file_rank(file, rank))
    }
}

/// Associated constants for every square, named in algebraic notation (`A1`..=`H8`).
impl Square {
    /// The a1 square (index 0).
    pub const A1: Square = Square(0);
    /// The b1 square (index 1).
    pub const B1: Square = Square(1);
    /// The c1 square (index 2).
    pub const C1: Square = Square(2);
    /// The d1 square (index 3).
    pub const D1: Square = Square(3);
    /// The e1 square (index 4).
    pub const E1: Square = Square(4);
    /// The f1 square (index 5).
    pub const F1: Square = Square(5);
    /// The g1 square (index 6).
    pub const G1: Square = Square(6);
    /// The h1 square (index 7).
    pub const H1: Square = Square(7);

    /// The a2 square (index 8).
    pub const A2: Square = Square(8);
    /// The b2 square (index 9).
    pub const B2: Square = Square(9);
    /// The c2 square (index 10).
    pub const C2: Square = Square(10);
    /// The d2 square (index 11).
    pub const D2: Square = Square(11);
    /// The e2 square (index 12).
    pub const E2: Square = Square(12);
    /// The f2 square (index 13).
    pub const F2: Square = Square(13);
    /// The g2 square (index 14).
    pub const G2: Square = Square(14);
    /// The h2 square (index 15).
    pub const H2: Square = Square(15);

    /// The a3 square (index 16).
    pub const A3: Square = Square(16);
    /// The b3 square (index 17).
    pub const B3: Square = Square(17);
    /// The c3 square (index 18).
    pub const C3: Square = Square(18);
    /// The d3 square (index 19).
    pub const D3: Square = Square(19);
    /// The e3 square (index 20).
    pub const E3: Square = Square(20);
    /// The f3 square (index 21).
    pub const F3: Square = Square(21);
    /// The g3 square (index 22).
    pub const G3: Square = Square(22);
    /// The h3 square (index 23).
    pub const H3: Square = Square(23);

    /// The a4 square (index 24).
    pub const A4: Square = Square(24);
    /// The b4 square (index 25).
    pub const B4: Square = Square(25);
    /// The c4 square (index 26).
    pub const C4: Square = Square(26);
    /// The d4 square (index 27).
    pub const D4: Square = Square(27);
    /// The e4 square (index 28).
    pub const E4: Square = Square(28);
    /// The f4 square (index 29).
    pub const F4: Square = Square(29);
    /// The g4 square (index 30).
    pub const G4: Square = Square(30);
    /// The h4 square (index 31).
    pub const H4: Square = Square(31);

    /// The a5 square (index 32).
    pub const A5: Square = Square(32);
    /// The b5 square (index 33).
    pub const B5: Square = Square(33);
    /// The c5 square (index 34).
    pub const C5: Square = Square(34);
    /// The d5 square (index 35).
    pub const D5: Square = Square(35);
    /// The e5 square (index 36).
    pub const E5: Square = Square(36);
    /// The f5 square (index 37).
    pub const F5: Square = Square(37);
    /// The g5 square (index 38).
    pub const G5: Square = Square(38);
    /// The h5 square (index 39).
    pub const H5: Square = Square(39);

    /// The a6 square (index 40).
    pub const A6: Square = Square(40);
    /// The b6 square (index 41).
    pub const B6: Square = Square(41);
    /// The c6 square (index 42).
    pub const C6: Square = Square(42);
    /// The d6 square (index 43).
    pub const D6: Square = Square(43);
    /// The e6 square (index 44).
    pub const E6: Square = Square(44);
    /// The f6 square (index 45).
    pub const F6: Square = Square(45);
    /// The g6 square (index 46).
    pub const G6: Square = Square(46);
    /// The h6 square (index 47).
    pub const H6: Square = Square(47);

    /// The a7 square (index 48).
    pub const A7: Square = Square(48);
    /// The b7 square (index 49).
    pub const B7: Square = Square(49);
    /// The c7 square (index 50).
    pub const C7: Square = Square(50);
    /// The d7 square (index 51).
    pub const D7: Square = Square(51);
    /// The e7 square (index 52).
    pub const E7: Square = Square(52);
    /// The f7 square (index 53).
    pub const F7: Square = Square(53);
    /// The g7 square (index 54).
    pub const G7: Square = Square(54);
    /// The h7 square (index 55).
    pub const H7: Square = Square(55);

    /// The a8 square (index 56).
    pub const A8: Square = Square(56);
    /// The b8 square (index 57).
    pub const B8: Square = Square(57);
    /// The c8 square (index 58).
    pub const C8: Square = Square(58);
    /// The d8 square (index 59).
    pub const D8: Square = Square(59);
    /// The e8 square (index 60).
    pub const E8: Square = Square(60);
    /// The f8 square (index 61).
    pub const F8: Square = Square(61);
    /// The g8 square (index 62).
    pub const G8: Square = Square(62);
    /// The h8 square (index 63).
    pub const H8: Square = Square(63);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{File, Rank};

    #[test]
    fn file_rank_round_trip_all_64() {
        for index in 0..64u8 {
            let square = Square::new(index);
            let file = square.file();
            let rank = square.rank();
            assert_eq!(Square::from_file_rank(file, rank), square);
            assert_eq!(square.index(), index);
        }
    }

    #[test]
    fn file_rank_mapping_is_little_endian() {
        assert_eq!(Square::from_file_rank(File::A, Rank::First).index(), 0);
        assert_eq!(Square::from_file_rank(File::B, Rank::First).index(), 1);
        assert_eq!(Square::from_file_rank(File::H, Rank::First).index(), 7);
        assert_eq!(Square::from_file_rank(File::A, Rank::Second).index(), 8);
        assert_eq!(Square::from_file_rank(File::H, Rank::Eighth).index(), 63);
        assert_eq!(Square::from_file_rank(File::E, Rank::Fourth).index(), 28);
    }

    #[test]
    fn string_round_trip_all_64() {
        for index in 0..64u8 {
            let square = Square::new(index);
            let text = square.to_string();
            assert_eq!(text.parse::<Square>(), Ok(square));
        }
        assert_eq!(Square::new(0).to_string(), "a1");
        assert_eq!(Square::new(63).to_string(), "h8");
        assert_eq!("e4".parse::<Square>(), Ok(Square::new(28)));
    }

    #[test]
    fn parse_errors() {
        assert!("".parse::<Square>().is_err());
        assert!("e".parse::<Square>().is_err());
        assert!("e44".parse::<Square>().is_err());
        assert!("i4".parse::<Square>().is_err());
        assert!("e9".parse::<Square>().is_err());
        assert!("4e".parse::<Square>().is_err());
    }

    #[test]
    fn try_from_range() {
        assert_eq!(Square::try_from(0u8), Ok(Square::new(0)));
        assert_eq!(Square::try_from(63u8), Ok(Square::new(63)));
        assert!(Square::try_from(64u8).is_err());
        assert!(Square::try_from(255u8).is_err());
        assert_eq!(Square::try_new(64), None);
    }

    #[test]
    fn offset_geometry() {
        let e4 = Square::new(28);
        assert_eq!(e4.offset(1, 1), Some(Square::F5));
        assert_eq!(e4.offset(-1, -1), Some(Square::D3));
        assert_eq!(e4.offset(0, 0), Some(e4));
        // Off-board in each direction.
        assert_eq!(Square::A1.offset(-1, 0), None);
        assert_eq!(Square::A1.offset(0, -1), None);
        assert_eq!(Square::H8.offset(1, 0), None);
        assert_eq!(Square::H8.offset(0, 1), None);
    }

    #[test]
    fn assoc_consts() {
        assert_eq!(Square::A1, Square::new(0));
        assert_eq!(Square::H1, Square::new(7));
        assert_eq!(Square::A8, Square::new(56));
        assert_eq!(Square::H8, Square::new(63));
        assert_eq!(Square::E4, Square::new(28));
        assert_eq!(Square::D5, Square::new(35));
    }

    #[test]
    fn into_integer_conversions() {
        assert_eq!(u8::from(Square::E4), 28);
        assert_eq!(usize::from(Square::E4), 28);
    }
}
