//! Board ranks (rows `1` through `8`).

use core::fmt;

/// A rank (row) of the chessboard, from `First` (rank `1`) to `Eighth` (rank `8`).
///
/// The discriminant is the zero-based index, so `Rank::First as u8 == 0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum Rank {
    /// Rank `1` (index 0).
    First,
    /// Rank `2` (index 1).
    Second,
    /// Rank `3` (index 2).
    Third,
    /// Rank `4` (index 3).
    Fourth,
    /// Rank `5` (index 4).
    Fifth,
    /// Rank `6` (index 5).
    Sixth,
    /// Rank `7` (index 6).
    Seventh,
    /// Rank `8` (index 7).
    Eighth,
}

impl Rank {
    /// All ranks, from `First` to `Eighth`.
    pub const ALL: [Rank; 8] = [
        Rank::First,
        Rank::Second,
        Rank::Third,
        Rank::Fourth,
        Rank::Fifth,
        Rank::Sixth,
        Rank::Seventh,
        Rank::Eighth,
    ];

    /// Returns the rank with the given zero-based index, or `None` if `index >= 8`.
    ///
    /// ```
    /// use mce::Rank;
    /// assert_eq!(Rank::new(0), Some(Rank::First));
    /// assert_eq!(Rank::new(7), Some(Rank::Eighth));
    /// assert_eq!(Rank::new(8), None);
    /// ```
    #[must_use]
    #[inline]
    pub const fn new(index: u8) -> Option<Rank> {
        match index {
            0 => Some(Rank::First),
            1 => Some(Rank::Second),
            2 => Some(Rank::Third),
            3 => Some(Rank::Fourth),
            4 => Some(Rank::Fifth),
            5 => Some(Rank::Sixth),
            6 => Some(Rank::Seventh),
            7 => Some(Rank::Eighth),
            _ => None,
        }
    }

    /// Returns the zero-based index of this rank (`First` is `0`, `Eighth` is `7`).
    #[must_use]
    #[inline]
    pub const fn index(self) -> u8 {
        self as u8
    }

    /// Returns the digit character for this rank (`'1'`..=`'8'`).
    #[must_use]
    #[inline]
    pub const fn char(self) -> char {
        (b'1' + self.index()) as char
    }

    /// Parses a rank from its digit character, accepting `'1'`..=`'8'`.
    ///
    /// ```
    /// use mce::Rank;
    /// assert_eq!(Rank::from_char('4'), Some(Rank::Fourth));
    /// assert_eq!(Rank::from_char('9'), None);
    /// assert_eq!(Rank::from_char('0'), None);
    /// ```
    #[must_use]
    #[inline]
    pub const fn from_char(ch: char) -> Option<Rank> {
        match ch {
            '1'..='8' => Rank::new(ch as u8 - b'1'),
            _ => None,
        }
    }

    /// Returns the rank `delta` rows to the north (or south, for negative
    /// `delta`), or `None` if the result falls off the board.
    #[must_use]
    #[inline]
    pub const fn offset(self, delta: i8) -> Option<Rank> {
        let index = self.index() as i8 + delta;
        if index < 0 || index > 7 {
            None
        } else {
            Rank::new(index as u8)
        }
    }
}

impl fmt::Display for Rank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Rank::First => "1",
            Rank::Second => "2",
            Rank::Third => "3",
            Rank::Fourth => "4",
            Rank::Fifth => "5",
            Rank::Sixth => "6",
            Rank::Seventh => "7",
            Rank::Eighth => "8",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_round_trip() {
        for (i, rank) in Rank::ALL.into_iter().enumerate() {
            assert_eq!(rank.index() as usize, i);
            assert_eq!(Rank::new(i as u8), Some(rank));
        }
        assert_eq!(Rank::new(8), None);
        assert_eq!(Rank::new(255), None);
    }

    #[test]
    fn char_round_trip() {
        for rank in Rank::ALL {
            assert_eq!(Rank::from_char(rank.char()), Some(rank));
            assert_eq!(rank.to_string(), rank.char().to_string());
        }
        assert_eq!(Rank::from_char('0'), None);
        assert_eq!(Rank::from_char('9'), None);
        assert_eq!(Rank::from_char('a'), None);
    }

    #[test]
    fn offset_masking() {
        assert_eq!(Rank::First.offset(-1), None);
        assert_eq!(Rank::First.offset(1), Some(Rank::Second));
        assert_eq!(Rank::Eighth.offset(1), None);
        assert_eq!(Rank::Eighth.offset(-7), Some(Rank::First));
        assert_eq!(Rank::Fourth.offset(0), Some(Rank::Fourth));
    }
}
