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
//!
//! # Packed representation
//!
//! A [`Move`] is stored as a single packed `u16` so that move buffers stay
//! small and copies stay cheap on the hot generation paths. The public API
//! ([`Move::from`], [`Move::to`], [`Move::kind`], the classifiers, [`Move::new`]
//! and [`Move::drop`]) is unchanged; [`MoveKind`] is a decoded *view* produced
//! by [`Move::kind`] rather than a stored field.
//!
//! ## Bit layout (`u16`)
//!
//! ```text
//! bit:  15 14 13 12 | 11 10  9  8  7  6 |  5  4  3  2  1  0
//!       \---flag---/ \------from-------/ \-------to--------/
//! ```
//!
//! * **`to`** (bits 0–5): destination square index `0..64`.
//! * **`from`** (bits 6–11): origin square index `0..64`. For a [`Drop`] this
//!   field is otherwise redundant (a drop has `from == to`), so it instead
//!   carries the dropped [`Role`] as its index `0..6`; the decoder rebuilds
//!   `from == to == target` for the public accessors.
//! * **`flag`** (bits 12–15): the kind code, one of:
//!
//!   | code | kind |
//!   |------|------|
//!   | 0 | [`Quiet`] |
//!   | 1 | [`Capture`] |
//!   | 2 | [`DoublePawnPush`] |
//!   | 3 | [`EnPassant`] |
//!   | 4 | [`CastleKingside`] |
//!   | 5 | [`CastleQueenside`] |
//!   | 6 | [`Drop`] (role in the `from` bits) |
//!   | 7..=11 | [`Promotion`] to knight, bishop, rook, queen, king |
//!
//! A promotion's *capture* flavour is not stored: a promoting pawn pushes
//! straight ahead when it does not capture and steps diagonally when it does, so
//! `from.file() != to.file()` recovers the capture bit exactly. This keeps all
//! five promotion roles (the four standard ones plus the antichess king
//! promotion), in both capturing and non-capturing forms, inside the 4-bit
//! flag with codes to spare.
//!
//! [`Quiet`]: MoveKind::Quiet
//! [`Capture`]: MoveKind::Capture
//! [`DoublePawnPush`]: MoveKind::DoublePawnPush
//! [`EnPassant`]: MoveKind::EnPassant
//! [`CastleKingside`]: MoveKind::CastleKingside
//! [`CastleQueenside`]: MoveKind::CastleQueenside
//! [`Drop`]: MoveKind::Drop
//! [`Promotion`]: MoveKind::Promotion

#[cfg(test)]
use alloc::format;
use alloc::{string::String, string::ToString};
use core::fmt;

use crate::{Role, Square};

/// The kind of a [`Move`], recording the special semantics needed to apply it
/// and to serialize it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
    /// Crazyhouse drop: a pocketed piece placed on an empty square.
    ///
    /// A drop has no distinct origin; the move's `from` and `to` are both set to
    /// the target square. No standard-chess move generator emits this kind — it
    /// exists for the crazyhouse variant — but the notation plumbing for it lives
    /// here so the shared move type is stable across variants.
    Drop {
        /// The role of the pocketed piece being placed.
        role: Role,
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

    /// Returns `true` if this move kind is a pocket drop.
    #[must_use]
    #[inline]
    pub const fn is_drop(self) -> bool {
        matches!(self, MoveKind::Drop { .. })
    }

    /// Returns the dropped role if this is a drop, otherwise `None`.
    #[must_use]
    #[inline]
    pub const fn drop_role(self) -> Option<Role> {
        match self {
            MoveKind::Drop { role } => Some(role),
            _ => None,
        }
    }
}

/// Maps a [`Role`] to its compact `0..6` index, used to pack the dropped role
/// into the `from` bits and to pick a promotion flag code.
#[inline]
const fn role_index(role: Role) -> u16 {
    match role {
        Role::Pawn => 0,
        Role::Knight => 1,
        Role::Bishop => 2,
        Role::Rook => 3,
        Role::Queen => 4,
        Role::King => 5,
    }
}

/// Inverse of [`role_index`]; panics on an out-of-range index, which the
/// internal encoders never produce.
#[inline]
const fn role_from_index(index: u16) -> Role {
    match index {
        0 => Role::Pawn,
        1 => Role::Knight,
        2 => Role::Bishop,
        3 => Role::Rook,
        4 => Role::Queen,
        5 => Role::King,
        _ => unreachable!(),
    }
}

// Flag codes occupying bits 12..16 of the packed word. See the module docs.
const FLAG_QUIET: u16 = 0;
const FLAG_CAPTURE: u16 = 1;
const FLAG_DOUBLE_PUSH: u16 = 2;
const FLAG_EN_PASSANT: u16 = 3;
const FLAG_CASTLE_K: u16 = 4;
const FLAG_CASTLE_Q: u16 = 5;
const FLAG_DROP: u16 = 6;
/// First promotion flag code; the promoted role's index is added to it, so the
/// five promotion codes are `7..=11` for knight, bishop, rook, queen, king.
const FLAG_PROMO_BASE: u16 = 7;

const TO_SHIFT: u16 = 0;
const FROM_SHIFT: u16 = 6;
const FLAG_SHIFT: u16 = 12;
const SQ_MASK: u16 = 0x3f;
const FLAG_MASK: u16 = 0xf;

/// A chess move: where a piece moves from, where it moves to, and the
/// [`MoveKind`] that gives it its special meaning.
///
/// Stored as a packed `u16` (see the module docs
/// for the bit layout). Castling is encoded as the king's move (for example
/// `e1` to `g1`), matching the UCI convention for standard chess.
///
/// ```
/// use mcr::{Move, MoveKind, Square};
/// let m = Move::new(Square::E2, Square::E4, MoveKind::DoublePawnPush);
/// assert_eq!(m.to_uci(), "e2e4");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Move(u16);

impl Move {
    /// Packs `from`, `to`, and a `flag` code into the wire representation.
    #[inline]
    const fn pack(from: u16, to: u16, flag: u16) -> Move {
        Move((flag << FLAG_SHIFT) | (from << FROM_SHIFT) | (to << TO_SHIFT))
    }

    /// Creates a move from its origin, destination, and kind.
    #[must_use]
    #[inline]
    pub const fn new(from: Square, to: Square, kind: MoveKind) -> Move {
        let from = from.index() as u16;
        let to = to.index() as u16;
        match kind {
            MoveKind::Quiet => Move::pack(from, to, FLAG_QUIET),
            MoveKind::Capture => Move::pack(from, to, FLAG_CAPTURE),
            MoveKind::DoublePawnPush => Move::pack(from, to, FLAG_DOUBLE_PUSH),
            MoveKind::EnPassant => Move::pack(from, to, FLAG_EN_PASSANT),
            MoveKind::CastleKingside => Move::pack(from, to, FLAG_CASTLE_K),
            MoveKind::CastleQueenside => Move::pack(from, to, FLAG_CASTLE_Q),
            // The capture flavour is recovered geometrically from the files of
            // `from` and `to`, so it is not stored.
            MoveKind::Promotion { role, .. } => {
                Move::pack(from, to, FLAG_PROMO_BASE + role_index(role))
            }
            // A drop is `from == to == square`; the role rides in the otherwise
            // redundant `from` field.
            MoveKind::Drop { role } => Move::pack(role_index(role), to, FLAG_DROP),
        }
    }

    /// Creates a pocket-drop move placing a piece of `role` on `square`.
    ///
    /// A drop has no distinct origin, so both [`Move::from`] and [`Move::to`]
    /// report `square`.
    #[must_use]
    #[inline]
    pub const fn drop(role: Role, square: Square) -> Move {
        Move::pack(role_index(role), square.index() as u16, FLAG_DROP)
    }

    /// The raw flag code in bits 12–15.
    #[inline]
    const fn flag(self) -> u16 {
        (self.0 >> FLAG_SHIFT) & FLAG_MASK
    }

    /// The square the moving piece starts on.
    ///
    /// For castling this is the king's square. For a drop this is the target
    /// square (a drop has no distinct origin).
    #[must_use]
    #[inline]
    pub const fn from(self) -> Square {
        // A drop stores its role (not a square) in the `from` bits, so its
        // public origin is the target square instead.
        if self.flag() == FLAG_DROP {
            return self.to();
        }
        Square::new(((self.0 >> FROM_SHIFT) & SQ_MASK) as u8)
    }

    /// The square the moving piece ends on.
    ///
    /// For castling this is the king's destination (two squares toward the
    /// rook). For en passant it is the empty target square, not the captured
    /// pawn's square.
    #[must_use]
    #[inline]
    pub const fn to(self) -> Square {
        Square::new(((self.0 >> TO_SHIFT) & SQ_MASK) as u8)
    }

    /// The [`MoveKind`] of this move, decoded from the packed flag.
    #[must_use]
    #[inline]
    pub const fn kind(self) -> MoveKind {
        match self.flag() {
            FLAG_QUIET => MoveKind::Quiet,
            FLAG_CAPTURE => MoveKind::Capture,
            FLAG_DOUBLE_PUSH => MoveKind::DoublePawnPush,
            FLAG_EN_PASSANT => MoveKind::EnPassant,
            FLAG_CASTLE_K => MoveKind::CastleKingside,
            FLAG_CASTLE_Q => MoveKind::CastleQueenside,
            FLAG_DROP => MoveKind::Drop {
                role: role_from_index((self.0 >> FROM_SHIFT) & SQ_MASK),
            },
            flag => MoveKind::Promotion {
                role: role_from_index(flag - FLAG_PROMO_BASE),
                capture: self.is_promotion_capture(),
            },
        }
    }

    /// Whether this move is a promotion that also captures, recovered from the
    /// geometry: a promoting pawn captures iff it changes file.
    ///
    /// Only meaningful when the flag is a promotion code; callers gate on that.
    #[inline]
    const fn is_promotion_capture(self) -> bool {
        let from_file = (self.0 >> FROM_SHIFT) & 0x7;
        let to_file = (self.0 >> TO_SHIFT) & 0x7;
        from_file != to_file
    }

    /// Returns `true` if this move captures a piece (ordinary, en passant, or a
    /// capturing promotion).
    #[must_use]
    #[inline]
    pub const fn is_capture(self) -> bool {
        match self.flag() {
            FLAG_CAPTURE | FLAG_EN_PASSANT => true,
            FLAG_QUIET | FLAG_DOUBLE_PUSH | FLAG_CASTLE_K | FLAG_CASTLE_Q | FLAG_DROP => false,
            // Promotion: capture flavour is geometric.
            _ => self.is_promotion_capture(),
        }
    }

    /// Returns `true` if this move is castling.
    #[must_use]
    #[inline]
    pub const fn is_castle(self) -> bool {
        matches!(self.flag(), FLAG_CASTLE_K | FLAG_CASTLE_Q)
    }

    /// Returns the promotion role, if this move is a promotion.
    #[must_use]
    #[inline]
    pub const fn promotion(self) -> Option<Role> {
        let flag = self.flag();
        if flag >= FLAG_PROMO_BASE {
            Some(role_from_index(flag - FLAG_PROMO_BASE))
        } else {
            None
        }
    }

    /// Returns `true` if this move is a pocket drop.
    #[must_use]
    #[inline]
    pub const fn is_drop(self) -> bool {
        self.flag() == FLAG_DROP
    }

    /// Returns the dropped role, if this move is a drop.
    #[must_use]
    #[inline]
    pub const fn drop_role(self) -> Option<Role> {
        if self.flag() == FLAG_DROP {
            Some(role_from_index((self.0 >> FROM_SHIFT) & SQ_MASK))
        } else {
            None
        }
    }

    /// Formats this move in UCI long algebraic notation.
    ///
    /// The format is the origin square, the destination square, and — for
    /// promotions — the lowercase letter of the promotion role. Castling is
    /// rendered as the king's two-square move (`e1g1`, `e1c1`).
    ///
    /// ```
    /// use mcr::{Move, MoveKind, Role, Square};
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
        // Crazyhouse drops use the `{ROLE}@{square}` form, e.g. `N@f3`.
        if let Some(role) = self.drop_role() {
            let mut s = String::with_capacity(4);
            s.push(role.upper_char());
            s.push('@');
            s.push_str(&self.to().to_string());
            return s;
        }
        let mut s = String::with_capacity(5);
        s.push_str(&self.from().to_string());
        s.push_str(&self.to().to_string());
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
    fn move_is_two_bytes() {
        assert_eq!(core::mem::size_of::<Move>(), 2);
    }

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
    fn round_trips_every_kind() {
        // Every (from, to, kind) combination the generators emit must survive a
        // pack/decode round trip, including both promotion capture flavours.
        let cases = [
            (Square::E2, Square::E4, MoveKind::Quiet),
            (Square::D4, Square::E5, MoveKind::Capture),
            (Square::E2, Square::E4, MoveKind::DoublePawnPush),
            (Square::D5, Square::E6, MoveKind::EnPassant),
            (Square::E1, Square::G1, MoveKind::CastleKingside),
            (Square::E1, Square::C1, MoveKind::CastleQueenside),
        ];
        for (from, to, kind) in cases {
            let m = Move::new(from, to, kind);
            assert_eq!(m.from(), from);
            assert_eq!(m.to(), to);
            assert_eq!(m.kind(), kind);
            assert_eq!(m.is_capture(), kind.is_capture());
            assert_eq!(m.is_castle(), kind.is_castle());
        }
    }

    #[test]
    fn promotion_capture_is_geometric() {
        for role in [
            Role::Knight,
            Role::Bishop,
            Role::Rook,
            Role::Queen,
            Role::King,
        ] {
            // Straight push: non-capturing promotion.
            let push = Move::new(
                Square::E7,
                Square::E8,
                MoveKind::Promotion {
                    role,
                    capture: false,
                },
            );
            assert_eq!(
                push.kind(),
                MoveKind::Promotion {
                    role,
                    capture: false
                }
            );
            assert!(!push.is_capture());
            assert_eq!(push.promotion(), Some(role));

            // Diagonal step: capturing promotion (file changes).
            let cap = Move::new(
                Square::D7,
                Square::E8,
                MoveKind::Promotion {
                    role,
                    capture: true,
                },
            );
            assert_eq!(
                cap.kind(),
                MoveKind::Promotion {
                    role,
                    capture: true
                }
            );
            assert!(cap.is_capture());
            assert_eq!(cap.promotion(), Some(role));
        }
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
    fn drop_classification_and_uci() {
        let d = Move::drop(Role::Knight, Square::F3);
        assert!(d.is_drop());
        assert!(d.kind().is_drop());
        assert_eq!(d.drop_role(), Some(Role::Knight));
        assert_eq!(d.kind().drop_role(), Some(Role::Knight));
        // A drop has no distinct origin: from == to == the target square.
        assert_eq!(d.from(), Square::F3);
        assert_eq!(d.to(), Square::F3);
        assert!(!d.is_capture());
        assert!(!d.is_castle());
        assert_eq!(d.promotion(), None);
        assert_eq!(d.to_uci(), "N@f3");
        assert_eq!(d.to_string(), "N@f3");
        // Non-drop kinds report no drop role.
        assert!(!MoveKind::Quiet.is_drop());
        assert_eq!(MoveKind::Quiet.drop_role(), None);
        assert_eq!(Move::drop(Role::Pawn, Square::E4).to_uci(), "P@e4");
        // Every droppable role round-trips through the packed `from` field.
        for role in Role::ALL {
            let m = Move::drop(role, Square::C6);
            assert_eq!(m.drop_role(), Some(role));
            assert_eq!(m.to(), Square::C6);
            assert_eq!(m.from(), Square::C6);
            assert_eq!(m.kind(), MoveKind::Drop { role });
        }
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
