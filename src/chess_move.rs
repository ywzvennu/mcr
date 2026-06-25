//! Chess moves: a from-square, a to-square, and a kind that distinguishes
//! quiet moves, captures, special pawn moves, castling, and promotions.
//!
//! A [`Move`] carries enough information to be applied to the position it was
//! generated from, and to be serialized to and from UCI long algebraic
//! notation. UCI is *context-sensitive* — the same `e1g1` denotes either a king
//! step or a castling move depending on the position — so parsing UCI back into
//! a [`Move`] is a method on the position
//! ([`Position::parse_uci`](crate::Position::parse_uci)) rather than a free
//! function here.

use core::fmt;

use crate::{Role, Square};

/// The kind of a [`Move`], recording the special semantics needed to apply it
/// and to serialize it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoveKind {
    /// A move to an empty square that is not any of the special cases below.
    Quiet,
    /// A capture of the piece standing on the destination square.
    Capture,
    /// A pawn's initial two-square advance, which sets an en-passant target.
    DoublePawnPush,
    /// An en-passant capture: a pawn captures an enemy pawn that has just made a
    /// double push, moving to the (empty) en-passant target square.
    EnPassant,
    /// King-side castling, encoded as the king's two-square move toward the
    /// h-file (`e1g1` / `e8g8` in standard chess).
    CastleKingside,
    /// Queen-side castling, encoded as the king's two-square move toward the
    /// a-file (`e1c1` / `e8c8` in standard chess).
    CastleQueenside,
    /// A pawn promotion to the given role, possibly while capturing.
    Promotion {
        /// The role the pawn promotes to (one of knight, bishop, rook, queen).
        role: Role,
        /// Whether the promoting push also captures a piece.
        capture: bool,
    },
}

impl MoveKind {
    /// Returns `true` if this move kind removes an enemy piece from the board.
    ///
    /// This includes ordinary captures, en-passant captures, and capturing
    /// promotions.
    #[must_use]
    #[inline]
    pub const fn is_capture(self) -> bool {
        matches!(
            self,
            MoveKind::Capture | MoveKind::EnPassant | MoveKind::Promotion { capture: true, .. }
        )
    }

    /// Returns `true` if this move kind is either form of castling.
    #[must_use]
    #[inline]
    pub const fn is_castle(self) -> bool {
        matches!(self, MoveKind::CastleKingside | MoveKind::CastleQueenside)
    }

    /// Returns the promotion role if this is a promotion, otherwise `None`.
    #[must_use]
    #[inline]
    pub const fn promotion(self) -> Option<Role> {
        match self {
            MoveKind::Promotion { role, .. } => Some(role),
            _ => None,
        }
    }
}

/// A chess move: where a piece moves from, where it moves to, and the
/// [`MoveKind`] that gives it its special meaning.
///
/// Castling is encoded as the king's move (for example `e1` to `g1`), matching
/// the UCI convention for standard chess.
///
/// ```
/// use mce::{Move, MoveKind, Square};
/// let m = Move::new(Square::E2, Square::E4, MoveKind::DoublePawnPush);
/// assert_eq!(m.to_uci(), "e2e4");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Move {
    from: Square,
    to: Square,
    kind: MoveKind,
}

impl Move {
    /// Creates a move from its origin, destination, and kind.
    #[must_use]
    #[inline]
    pub const fn new(from: Square, to: Square, kind: MoveKind) -> Move {
        Move { from, to, kind }
    }

    /// The square the moving piece starts on.
    ///
    /// For castling this is the king's square.
    #[must_use]
    #[inline]
    pub const fn from(self) -> Square {
        self.from
    }

    /// The square the moving piece ends on.
    ///
    /// For castling this is the king's destination (two squares toward the
    /// rook). For en passant it is the empty target square, not the captured
    /// pawn's square.
    #[must_use]
    #[inline]
    pub const fn to(self) -> Square {
        self.to
    }

    /// The [`MoveKind`] of this move.
    #[must_use]
    #[inline]
    pub const fn kind(self) -> MoveKind {
        self.kind
    }

    /// Returns `true` if this move captures a piece (ordinary, en passant, or a
    /// capturing promotion).
    #[must_use]
    #[inline]
    pub const fn is_capture(self) -> bool {
        self.kind.is_capture()
    }

    /// Returns `true` if this move is castling.
    #[must_use]
    #[inline]
    pub const fn is_castle(self) -> bool {
        self.kind.is_castle()
    }

    /// Returns the promotion role, if this move is a promotion.
    #[must_use]
    #[inline]
    pub const fn promotion(self) -> Option<Role> {
        self.kind.promotion()
    }

    /// Formats this move in UCI long algebraic notation.
    ///
    /// The format is the origin square, the destination square, and — for
    /// promotions — the lowercase letter of the promotion role. Castling is
    /// rendered as the king's two-square move (`e1g1`, `e1c1`).
    ///
    /// ```
    /// use mce::{Move, MoveKind, Role, Square};
    /// assert_eq!(
    ///     Move::new(Square::E2, Square::E4, MoveKind::DoublePawnPush).to_uci(),
    ///     "e2e4",
    /// );
    /// assert_eq!(
    ///     Move::new(
    ///         Square::E7,
    ///         Square::E8,
    ///         MoveKind::Promotion { role: Role::Queen, capture: false },
    ///     )
    ///     .to_uci(),
    ///     "e7e8q",
    /// );
    /// ```
    #[must_use]
    pub fn to_uci(self) -> String {
        let mut s = String::with_capacity(5);
        s.push_str(&self.from.to_string());
        s.push_str(&self.to.to_string());
        if let Some(role) = self.promotion() {
            s.push(role.char());
        }
        s
    }
}

impl fmt::Display for Move {
    /// Formats the move as UCI; see [`Move::to_uci`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_uci())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Role;

    #[test]
    fn accessors() {
        let m = Move::new(Square::E2, Square::E4, MoveKind::DoublePawnPush);
        assert_eq!(m.from(), Square::E2);
        assert_eq!(m.to(), Square::E4);
        assert_eq!(m.kind(), MoveKind::DoublePawnPush);
        assert!(!m.is_capture());
        assert!(!m.is_castle());
        assert_eq!(m.promotion(), None);
    }

    #[test]
    fn capture_classification() {
        assert!(MoveKind::Capture.is_capture());
        assert!(MoveKind::EnPassant.is_capture());
        assert!(MoveKind::Promotion {
            role: Role::Queen,
            capture: true
        }
        .is_capture());
        assert!(!MoveKind::Promotion {
            role: Role::Queen,
            capture: false
        }
        .is_capture());
        assert!(!MoveKind::Quiet.is_capture());
        assert!(!MoveKind::DoublePawnPush.is_capture());
        assert!(!MoveKind::CastleKingside.is_capture());
    }

    #[test]
    fn castle_classification() {
        assert!(MoveKind::CastleKingside.is_castle());
        assert!(MoveKind::CastleQueenside.is_castle());
        assert!(!MoveKind::Quiet.is_castle());
    }

    #[test]
    fn uci_formatting() {
        assert_eq!(
            Move::new(Square::E2, Square::E4, MoveKind::DoublePawnPush).to_uci(),
            "e2e4"
        );
        assert_eq!(
            Move::new(Square::E1, Square::G1, MoveKind::CastleKingside).to_uci(),
            "e1g1"
        );
        assert_eq!(
            Move::new(Square::E1, Square::C1, MoveKind::CastleQueenside).to_uci(),
            "e1c1"
        );
        for (role, ch) in [
            (Role::Knight, 'n'),
            (Role::Bishop, 'b'),
            (Role::Rook, 'r'),
            (Role::Queen, 'q'),
        ] {
            let m = Move::new(
                Square::E7,
                Square::E8,
                MoveKind::Promotion {
                    role,
                    capture: false,
                },
            );
            assert_eq!(m.to_uci(), format!("e7e8{ch}"));
            // Display matches to_uci.
            assert_eq!(m.to_string(), m.to_uci());
        }
        // Capturing promotion still renders just the destination + role.
        assert_eq!(
            Move::new(
                Square::D7,
                Square::E8,
                MoveKind::Promotion {
                    role: Role::Queen,
                    capture: true
                }
            )
            .to_uci(),
            "d7e8q"
        );
    }
}
