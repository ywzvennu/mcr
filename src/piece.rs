//! Piece roles and colored pieces, with FEN-character conversions.

#[cfg(test)]
use alloc::{string::ToString, vec::Vec};
use core::fmt;

use crate::Color;

/// The kind of a piece, independent of color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Role {
    /// A pawn.
    Pawn,
    /// A knight.
    Knight,
    /// A bishop.
    Bishop,
    /// A rook.
    Rook,
    /// A queen.
    Queen,
    /// A king.
    King,
}

impl Role {
    /// All roles, in increasing value order (pawn first, king last).
    pub const ALL: [Role; 6] = [
        Role::Pawn,
        Role::Knight,
        Role::Bishop,
        Role::Rook,
        Role::Queen,
        Role::King,
    ];

    /// Returns the lowercase character used for this role in FEN and SAN.
    ///
    /// ```
    /// use mcr::Role;
    /// assert_eq!(Role::Knight.char(), 'n');
    /// ```
    #[must_use]
    #[inline]
    pub const fn char(self) -> char {
        match self {
            Role::Pawn => 'p',
            Role::Knight => 'n',
            Role::Bishop => 'b',
            Role::Rook => 'r',
            Role::Queen => 'q',
            Role::King => 'k',
        }
    }

    /// Returns the uppercase character used for this role.
    #[must_use]
    #[inline]
    pub const fn upper_char(self) -> char {
        match self {
            Role::Pawn => 'P',
            Role::Knight => 'N',
            Role::Bishop => 'B',
            Role::Rook => 'R',
            Role::Queen => 'Q',
            Role::King => 'K',
        }
    }

    /// Parses a role from its character, accepting either case.
    ///
    /// ```
    /// use mcr::Role;
    /// assert_eq!(Role::from_char('N'), Some(Role::Knight));
    /// assert_eq!(Role::from_char('q'), Some(Role::Queen));
    /// assert_eq!(Role::from_char('x'), None);
    /// ```
    #[must_use]
    #[inline]
    pub const fn from_char(ch: char) -> Option<Role> {
        match ch {
            'p' | 'P' => Some(Role::Pawn),
            'n' | 'N' => Some(Role::Knight),
            'b' | 'B' => Some(Role::Bishop),
            'r' | 'R' => Some(Role::Rook),
            'q' | 'Q' => Some(Role::Queen),
            'k' | 'K' => Some(Role::King),
            _ => None,
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Role::Pawn => "pawn",
            Role::Knight => "knight",
            Role::Bishop => "bishop",
            Role::Rook => "rook",
            Role::Queen => "queen",
            Role::King => "king",
        })
    }
}

/// A colored piece: a [`Role`] together with a [`Color`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Piece {
    /// The side the piece belongs to.
    pub color: Color,
    /// The kind of piece.
    pub role: Role,
}

impl Piece {
    /// Creates a new colored piece.
    #[must_use]
    #[inline]
    pub const fn new(color: Color, role: Role) -> Piece {
        Piece { color, role }
    }

    /// Returns the FEN character for this piece: uppercase for white, lowercase
    /// for black.
    ///
    /// ```
    /// use mcr::{Color, Piece, Role};
    /// assert_eq!(Piece::new(Color::White, Role::Pawn).char(), 'P');
    /// assert_eq!(Piece::new(Color::Black, Role::Knight).char(), 'n');
    /// ```
    #[must_use]
    #[inline]
    pub const fn char(self) -> char {
        match self.color {
            Color::White => self.role.upper_char(),
            Color::Black => self.role.char(),
        }
    }

    /// Parses a piece from its FEN character. Uppercase letters yield white
    /// pieces, lowercase yield black pieces.
    ///
    /// ```
    /// use mcr::{Color, Piece, Role};
    /// assert_eq!(Piece::from_char('K'), Some(Piece::new(Color::White, Role::King)));
    /// assert_eq!(Piece::from_char('q'), Some(Piece::new(Color::Black, Role::Queen)));
    /// assert_eq!(Piece::from_char('1'), None);
    /// ```
    #[must_use]
    #[inline]
    pub const fn from_char(ch: char) -> Option<Piece> {
        let color = if ch.is_ascii_uppercase() {
            Color::White
        } else {
            Color::Black
        };
        match Role::from_char(ch) {
            Some(role) => Some(Piece::new(color, role)),
            None => None,
        }
    }
}

impl fmt::Display for Piece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.char() {
            'P' => "P",
            'N' => "N",
            'B' => "B",
            'R' => "R",
            'Q' => "Q",
            'K' => "K",
            'p' => "p",
            'n' => "n",
            'b' => "b",
            'r' => "r",
            'q' => "q",
            _ => "k",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_char_round_trip() {
        for role in Role::ALL {
            assert_eq!(Role::from_char(role.char()), Some(role));
            assert_eq!(Role::from_char(role.upper_char()), Some(role));
            assert_eq!(role.char().to_ascii_uppercase(), role.upper_char());
        }
        assert_eq!(Role::from_char('x'), None);
        assert_eq!(Role::from_char('1'), None);
    }

    #[test]
    fn piece_char_round_trip_all_twelve() {
        let mut chars = Vec::new();
        for color in Color::ALL {
            for role in Role::ALL {
                let piece = Piece::new(color, role);
                let ch = piece.char();
                chars.push(ch);
                assert_eq!(Piece::from_char(ch), Some(piece));
                assert_eq!(piece.to_string(), ch.to_string());
            }
        }
        // All twelve characters are distinct.
        chars.sort_unstable();
        chars.dedup();
        assert_eq!(chars.len(), 12);
    }

    #[test]
    fn piece_char_case_matches_color() {
        assert_eq!(Piece::new(Color::White, Role::Pawn).char(), 'P');
        assert_eq!(Piece::new(Color::Black, Role::Pawn).char(), 'p');
        assert!(Piece::from_char('R').unwrap().color.is_white());
        assert!(Piece::from_char('r').unwrap().color.is_black());
    }

    #[test]
    fn piece_from_invalid_char() {
        assert_eq!(Piece::from_char('1'), None);
        assert_eq!(Piece::from_char(' '), None);
        assert_eq!(Piece::from_char('Z'), None);
    }

    #[test]
    fn role_display() {
        assert_eq!(Role::Pawn.to_string(), "pawn");
        assert_eq!(Role::King.to_string(), "king");
    }
}
