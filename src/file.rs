//! Board files (columns `a` through `h`).

#[cfg(test)]
use alloc::string::ToString;
use core::fmt;

/// A file (column) of the chessboard, from `A` (`a`-file) to `H` (`h`-file).
///
/// The discriminant is the zero-based index, so `File::A as u8 == 0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum File {
    /// The `a`-file (index 0).
    A,
    /// The `b`-file (index 1).
    B,
    /// The `c`-file (index 2).
    C,
    /// The `d`-file (index 3).
    D,
    /// The `e`-file (index 4).
    E,
    /// The `f`-file (index 5).
    F,
    /// The `g`-file (index 6).
    G,
    /// The `h`-file (index 7).
    H,
}

impl File {
    /// All files, from `A` to `H`.
    pub const ALL: [File; 8] = [
        File::A,
        File::B,
        File::C,
        File::D,
        File::E,
        File::F,
        File::G,
        File::H,
    ];

    /// Returns the file with the given zero-based index, or `None` if `index >= 8`.
    ///
    /// ```
    /// use mce::File;
    /// assert_eq!(File::new(0), Some(File::A));
    /// assert_eq!(File::new(7), Some(File::H));
    /// assert_eq!(File::new(8), None);
    /// ```
    #[must_use]
    #[inline]
    pub const fn new(index: u8) -> Option<File> {
        match index {
            0 => Some(File::A),
            1 => Some(File::B),
            2 => Some(File::C),
            3 => Some(File::D),
            4 => Some(File::E),
            5 => Some(File::F),
            6 => Some(File::G),
            7 => Some(File::H),
            _ => None,
        }
    }

    /// Returns the zero-based index of this file (`A` is `0`, `H` is `7`).
    #[must_use]
    #[inline]
    pub const fn index(self) -> u8 {
        self as u8
    }

    /// Returns the lowercase character for this file (`'a'`..=`'h'`).
    #[must_use]
    #[inline]
    pub const fn char(self) -> char {
        (b'a' + self.index()) as char
    }

    /// Parses a file from its character, accepting `'a'`..=`'h'` and `'A'`..=`'H'`.
    ///
    /// ```
    /// use mce::File;
    /// assert_eq!(File::from_char('c'), Some(File::C));
    /// assert_eq!(File::from_char('C'), Some(File::C));
    /// assert_eq!(File::from_char('i'), None);
    /// ```
    #[must_use]
    #[inline]
    pub const fn from_char(ch: char) -> Option<File> {
        match ch {
            'a'..='h' => File::new(ch as u8 - b'a'),
            'A'..='H' => File::new(ch as u8 - b'A'),
            _ => None,
        }
    }

    /// Returns the file `delta` columns to the east (or west, for negative
    /// `delta`), or `None` if the result falls off the board.
    #[must_use]
    #[inline]
    pub const fn offset(self, delta: i8) -> Option<File> {
        let index = self.index() as i8 + delta;
        if index < 0 || index > 7 {
            None
        } else {
            File::new(index as u8)
        }
    }
}

impl fmt::Display for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            File::A => "a",
            File::B => "b",
            File::C => "c",
            File::D => "d",
            File::E => "e",
            File::F => "f",
            File::G => "g",
            File::H => "h",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_round_trip() {
        for (i, file) in File::ALL.into_iter().enumerate() {
            assert_eq!(file.index() as usize, i);
            assert_eq!(File::new(i as u8), Some(file));
        }
        assert_eq!(File::new(8), None);
        assert_eq!(File::new(255), None);
    }

    #[test]
    fn char_round_trip() {
        for file in File::ALL {
            assert_eq!(File::from_char(file.char()), Some(file));
            assert_eq!(
                File::from_char(file.char().to_ascii_uppercase()),
                Some(file)
            );
            assert_eq!(file.to_string(), file.char().to_string());
        }
        assert_eq!(File::from_char('i'), None);
        assert_eq!(File::from_char('1'), None);
    }

    #[test]
    fn offset_masking() {
        assert_eq!(File::A.offset(-1), None);
        assert_eq!(File::A.offset(1), Some(File::B));
        assert_eq!(File::H.offset(1), None);
        assert_eq!(File::H.offset(-7), Some(File::A));
        assert_eq!(File::D.offset(0), Some(File::D));
    }
}
