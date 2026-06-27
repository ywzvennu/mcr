//! The two sides of a chess game.

#[cfg(test)]
use alloc::string::ToString;
use core::fmt;

/// One of the two players / sides in a chess game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Color {
    /// The side that moves first.
    White,
    /// The side that moves second.
    Black,
}

impl Color {
    /// All colors, in their discriminant order.
    pub const ALL: [Color; 2] = [Color::White, Color::Black];

    /// Returns the opposing color.
    ///
    /// ```
    /// use mce::Color;
    /// assert_eq!(Color::White.opposite(), Color::Black);
    /// assert_eq!(Color::Black.opposite(), Color::White);
    /// ```
    #[must_use]
    #[inline]
    pub const fn opposite(self) -> Color {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }

    /// Returns `true` if this is [`Color::White`].
    #[must_use]
    #[inline]
    pub const fn is_white(self) -> bool {
        matches!(self, Color::White)
    }

    /// Returns `true` if this is [`Color::Black`].
    #[must_use]
    #[inline]
    pub const fn is_black(self) -> bool {
        matches!(self, Color::Black)
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Color::White => "white",
            Color::Black => "black",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opposite_is_an_involution() {
        for color in Color::ALL {
            assert_eq!(color.opposite().opposite(), color);
            assert_ne!(color.opposite(), color);
        }
    }

    #[test]
    fn predicates() {
        assert!(Color::White.is_white());
        assert!(!Color::White.is_black());
        assert!(Color::Black.is_black());
        assert!(!Color::Black.is_white());
    }

    #[test]
    fn display() {
        assert_eq!(Color::White.to_string(), "white");
        assert_eq!(Color::Black.to_string(), "black");
    }
}
