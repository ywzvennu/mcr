//! The wide move type for the generic large-board layer.
//!
//! This is the parallel generic analogue of the concrete [`crate::Move`]; see
//! `docs/fairy-variants-architecture.md` §2.4 for why a wider move is needed.
//! The concrete `Move(u16)` packs a 6-bit `from` / 6-bit `to` and a 4-bit flag,
//! which cannot index past square 63 nor name a role outside the standard six.
//! Large boards (up to 128 squares) and the extended [`WideRole`] set both
//! overflow that, so the generic layer uses a wider packed move.
//!
//! [`WideMove`] is **not** parametrised over a [`Geometry`](super::Geometry):
//! it stores raw `0..128` square indices and a [`WideRole`] index, exactly like
//! the concrete `Move` stores raw `0..64` indices independent of any board. A
//! geometry only enters when rendering a move to a string (the file/rank
//! alphabet depends on the board width/height), which is why
//! [`WideMove::to_uci`] takes the geometry as a type parameter.
//!
//! # Bit layout (`u32`)
//!
//! ```text
//! bit:  31 ... 26 | 25 24 23 22 21 | 20 19 18 17 16 | 15 ... 8 | 7 ... 0
//!       \--unused-/ \----kind-----/ \-----role-----/ \--from--/ \--to--/
//! ```
//!
//! * **`to`** (bits 0..8): destination square index, `0..128`.
//! * **`from`** (bits 8..16): origin square index, `0..128`. For a drop this is
//!   redundant (a drop has `from == to`), so the public [`WideMove::from`]
//!   reports the target square.
//! * **`role`** (bits 16..21): a [`WideRole`] index `0..COUNT`, used as the
//!   promotion role or the dropped role depending on the kind; unused (`0`)
//!   otherwise.
//! * **`kind`** (bits 21..26): the [`WideMoveKind`] tag. Five bits leave ample
//!   room for the future fairy kinds (gating, duck-placement, palace moves)
//!   beyond the codes used today.

use alloc::string::String;
use core::fmt;

use super::role::WideRole;
use super::{Geometry, Square};

/// The kind of a [`WideMove`], mirroring the concrete [`crate::MoveKind`] and
/// adding headroom for fairy mechanics.
///
/// The capture flavour of a promotion is carried explicitly (unlike the concrete
/// `Move`, which recovers it from file geometry) because on a non-power-of-two
/// board the geometric trick is less convenient and the extra tag bits are free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum WideMoveKind {
    /// A move to an empty square with no special semantics.
    Quiet,
    /// A capture of the piece on the destination square.
    Capture,
    /// A pawn's initial two-square advance, setting an en-passant target.
    DoublePawnPush,
    /// An en-passant capture.
    EnPassant,
    /// King-side castling, encoded as the king's move toward the last file.
    CastleKingside,
    /// Queen-side castling, encoded as the king's move toward the first file.
    CastleQueenside,
    /// A promotion to the given role, possibly while capturing.
    Promotion {
        /// The role the piece promotes to.
        role: WideRole,
        /// Whether the promoting move also captures.
        capture: bool,
    },
    /// A drop from hand: a pocketed piece placed on the target square.
    Drop {
        /// The role of the dropped piece.
        role: WideRole,
    },
}

impl WideMoveKind {
    /// Returns `true` if this kind removes an enemy piece.
    #[must_use]
    #[inline]
    pub const fn is_capture(self) -> bool {
        matches!(
            self,
            WideMoveKind::Capture
                | WideMoveKind::EnPassant
                | WideMoveKind::Promotion { capture: true, .. }
        )
    }

    /// Returns `true` if this kind is either form of castling.
    #[must_use]
    #[inline]
    pub const fn is_castle(self) -> bool {
        matches!(
            self,
            WideMoveKind::CastleKingside | WideMoveKind::CastleQueenside
        )
    }

    /// Returns the promotion role if this is a promotion, otherwise `None`.
    #[must_use]
    #[inline]
    pub const fn promotion(self) -> Option<WideRole> {
        match self {
            WideMoveKind::Promotion { role, .. } => Some(role),
            _ => None,
        }
    }

    /// Returns `true` if this kind is a drop.
    #[must_use]
    #[inline]
    pub const fn is_drop(self) -> bool {
        matches!(self, WideMoveKind::Drop { .. })
    }

    /// Returns the dropped role if this is a drop, otherwise `None`.
    #[must_use]
    #[inline]
    pub const fn drop_role(self) -> Option<WideRole> {
        match self {
            WideMoveKind::Drop { role } => Some(role),
            _ => None,
        }
    }
}

// Kind tag codes occupying bits 21..26 of the packed word.
const KIND_QUIET: u32 = 0;
const KIND_CAPTURE: u32 = 1;
const KIND_DOUBLE_PUSH: u32 = 2;
const KIND_EN_PASSANT: u32 = 3;
const KIND_CASTLE_K: u32 = 4;
const KIND_CASTLE_Q: u32 = 5;
const KIND_PROMOTION: u32 = 6;
const KIND_PROMOTION_CAPTURE: u32 = 7;
const KIND_DROP: u32 = 8;

const TO_SHIFT: u32 = 0;
const FROM_SHIFT: u32 = 8;
const ROLE_SHIFT: u32 = 16;
const KIND_SHIFT: u32 = 21;

const SQ_MASK: u32 = 0xff;
const ROLE_MASK: u32 = 0x1f;
const KIND_MASK: u32 = 0x1f;

/// A wide chess move over the generic large-board layer: an 8-bit `from`, an
/// 8-bit `to` (covering `0..128`), and a [`WideMoveKind`] carrying a
/// [`WideRole`] index for promotions and drops.
///
/// Stored as a packed `u32` (see the [module docs](self#bit-layout-u32)). The
/// concrete 8x8 [`crate::Move`] is untouched; this is the parallel wide move.
///
/// ```
/// use mce::geometry::{Chess8x8, Square, WideMove, WideMoveKind};
/// let m = WideMove::new(Square::<Chess8x8>::new(12), Square::new(28), WideMoveKind::DoublePawnPush);
/// assert_eq!(m.to_uci::<Chess8x8>(), "e2e4");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WideMove(u32);

impl WideMove {
    /// Packs `from`, `to`, a `role` index, and a `kind` tag into the word.
    #[inline]
    const fn pack(from: u8, to: u8, role: u32, kind: u32) -> WideMove {
        WideMove(
            (kind << KIND_SHIFT)
                | (role << ROLE_SHIFT)
                | ((from as u32) << FROM_SHIFT)
                | ((to as u32) << TO_SHIFT),
        )
    }

    /// Creates a move from its origin, destination, and kind, for a board of
    /// geometry `G`.
    ///
    /// The type parameter only fixes the square type; nothing about `G` is
    /// stored, so the same encoding is shared by every geometry.
    #[must_use]
    #[inline]
    pub fn new<G: Geometry>(from: Square<G>, to: Square<G>, kind: WideMoveKind) -> WideMove {
        let from = from.index();
        let to = to.index();
        match kind {
            WideMoveKind::Quiet => WideMove::pack(from, to, 0, KIND_QUIET),
            WideMoveKind::Capture => WideMove::pack(from, to, 0, KIND_CAPTURE),
            WideMoveKind::DoublePawnPush => WideMove::pack(from, to, 0, KIND_DOUBLE_PUSH),
            WideMoveKind::EnPassant => WideMove::pack(from, to, 0, KIND_EN_PASSANT),
            WideMoveKind::CastleKingside => WideMove::pack(from, to, 0, KIND_CASTLE_K),
            WideMoveKind::CastleQueenside => WideMove::pack(from, to, 0, KIND_CASTLE_Q),
            WideMoveKind::Promotion { role, capture } => WideMove::pack(
                from,
                to,
                role.index() as u32,
                if capture {
                    KIND_PROMOTION_CAPTURE
                } else {
                    KIND_PROMOTION
                },
            ),
            // A drop is `from == to == square`; the role rides in the role field.
            WideMoveKind::Drop { role } => WideMove::pack(to, to, role.index() as u32, KIND_DROP),
        }
    }

    /// Creates a drop move placing a piece of `role` on `square`.
    ///
    /// A drop has no distinct origin, so both [`WideMove::from`] and
    /// [`WideMove::to`] report `square`.
    #[must_use]
    #[inline]
    pub fn drop<G: Geometry>(role: WideRole, square: Square<G>) -> WideMove {
        let s = square.index();
        WideMove::pack(s, s, role.index() as u32, KIND_DROP)
    }

    /// The raw kind tag in bits 21..26.
    #[inline]
    const fn kind_tag(self) -> u32 {
        (self.0 >> KIND_SHIFT) & KIND_MASK
    }

    /// The raw role index in bits 16..21.
    #[inline]
    const fn role_index(self) -> usize {
        ((self.0 >> ROLE_SHIFT) & ROLE_MASK) as usize
    }

    /// Returns the origin square index, `0..128`.
    ///
    /// For a drop this equals [`WideMove::to_index`] (a drop has no distinct
    /// origin).
    #[must_use]
    #[inline]
    pub const fn from_index(self) -> u8 {
        ((self.0 >> FROM_SHIFT) & SQ_MASK) as u8
    }

    /// Returns the destination square index, `0..128`.
    #[must_use]
    #[inline]
    pub const fn to_index(self) -> u8 {
        ((self.0 >> TO_SHIFT) & SQ_MASK) as u8
    }

    /// Returns the origin square for a board of geometry `G`.
    ///
    /// # Panics
    ///
    /// Panics if the stored index is out of range for `G` — which only happens
    /// when a move built for one geometry is read back at a smaller one.
    #[must_use]
    #[inline]
    pub fn from<G: Geometry>(self) -> Square<G> {
        Square::new(self.from_index())
    }

    /// Returns the destination square for a board of geometry `G`.
    ///
    /// # Panics
    ///
    /// Panics if the stored index is out of range for `G`.
    #[must_use]
    #[inline]
    pub fn to<G: Geometry>(self) -> Square<G> {
        Square::new(self.to_index())
    }

    /// Returns the [`WideMoveKind`] decoded from the packed tag.
    #[must_use]
    #[inline]
    pub fn kind(self) -> WideMoveKind {
        match self.kind_tag() {
            KIND_QUIET => WideMoveKind::Quiet,
            KIND_CAPTURE => WideMoveKind::Capture,
            KIND_DOUBLE_PUSH => WideMoveKind::DoublePawnPush,
            KIND_EN_PASSANT => WideMoveKind::EnPassant,
            KIND_CASTLE_K => WideMoveKind::CastleKingside,
            KIND_CASTLE_Q => WideMoveKind::CastleQueenside,
            KIND_PROMOTION => WideMoveKind::Promotion {
                role: self.role_or_pawn(),
                capture: false,
            },
            KIND_PROMOTION_CAPTURE => WideMoveKind::Promotion {
                role: self.role_or_pawn(),
                capture: true,
            },
            // Any code at or above KIND_DROP is a drop; codes are dense, so this
            // only matches KIND_DROP for values this crate produces.
            _ => WideMoveKind::Drop {
                role: self.role_or_pawn(),
            },
        }
    }

    /// Decodes the role field, falling back to `Pawn` for an out-of-range index
    /// (which the encoders never produce).
    #[inline]
    fn role_or_pawn(self) -> WideRole {
        WideRole::from_index(self.role_index()).unwrap_or(WideRole::Pawn)
    }

    /// Returns `true` if this move captures a piece.
    #[must_use]
    #[inline]
    pub const fn is_capture(self) -> bool {
        matches!(
            self.kind_tag(),
            KIND_CAPTURE | KIND_EN_PASSANT | KIND_PROMOTION_CAPTURE
        )
    }

    /// Returns `true` if this move is castling.
    #[must_use]
    #[inline]
    pub const fn is_castle(self) -> bool {
        matches!(self.kind_tag(), KIND_CASTLE_K | KIND_CASTLE_Q)
    }

    /// Returns the promotion role if this move is a promotion, otherwise `None`.
    #[must_use]
    #[inline]
    pub fn promotion(self) -> Option<WideRole> {
        match self.kind_tag() {
            KIND_PROMOTION | KIND_PROMOTION_CAPTURE => Some(self.role_or_pawn()),
            _ => None,
        }
    }

    /// Returns `true` if this move is a drop.
    #[must_use]
    #[inline]
    pub const fn is_drop(self) -> bool {
        self.kind_tag() == KIND_DROP
    }

    /// Returns the dropped role if this move is a drop, otherwise `None`.
    #[must_use]
    #[inline]
    pub fn drop_role(self) -> Option<WideRole> {
        if self.is_drop() {
            Some(self.role_or_pawn())
        } else {
            None
        }
    }

    /// Renders a square index as a UCI-ish coordinate for a board of geometry
    /// `G`: the file as a letter (`a`, `b`, ...) and the rank as a 1-based
    /// number.
    ///
    /// Files past `z` (width > 26) wrap to a two-letter form is *not* attempted;
    /// the generic layer's variants are all `<= 12` files, so a single ASCII
    /// letter always suffices. The rank is a decimal number `1..=height`.
    fn render_square<G: Geometry>(out: &mut String, index: u8) {
        let file = index % G::WIDTH;
        let rank = index / G::WIDTH;
        out.push((b'a' + file) as char);
        // Ranks up to `height` need decimal digits (e.g. rank 10).
        let rank_no = rank as u32 + 1;
        if rank_no >= 10 {
            out.push((b'0' + (rank_no / 10) as u8) as char);
        }
        out.push((b'0' + (rank_no % 10) as u8) as char);
    }

    /// Formats this move in UCI long algebraic notation over the geometry `G`.
    ///
    /// Squares render as a file letter plus a 1-based rank number (so a
    /// ten-rank board reaches `a10`). A promotion appends the promotion role's
    /// lowercase letter; a drop uses the `{ROLE}@{square}` form.
    ///
    /// ```
    /// use mce::geometry::{Cap10x8, Chess8x8, Square, WideMove, WideMoveKind, WideRole};
    /// let m = WideMove::new(
    ///     Square::<Chess8x8>::new(12),
    ///     Square::new(28),
    ///     WideMoveKind::DoublePawnPush,
    /// );
    /// assert_eq!(m.to_uci::<Chess8x8>(), "e2e4");
    ///
    /// // A drop on the tenth file of a 10-wide board.
    /// let d = WideMove::drop(WideRole::Cannon, Square::<Cap10x8>::new(9));
    /// assert_eq!(d.to_uci::<Cap10x8>(), "C@j1");
    /// ```
    #[must_use]
    pub fn to_uci<G: Geometry>(self) -> String {
        if let Some(role) = self.drop_role() {
            let mut s = String::with_capacity(4);
            s.push(role.upper_char());
            s.push('@');
            Self::render_square::<G>(&mut s, self.to_index());
            return s;
        }
        let mut s = String::with_capacity(6);
        Self::render_square::<G>(&mut s, self.from_index());
        Self::render_square::<G>(&mut s, self.to_index());
        if let Some(role) = self.promotion() {
            s.push(role.char());
        }
        s
    }
}

impl fmt::Display for WideMove {
    /// Formats the raw move without a geometry, as `from->to` square indices
    /// (a geometry-free debug form). For algebraic notation use
    /// [`WideMove::to_uci`], which needs the board width and height.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}->{}", self.from_index(), self.to_index())?;
        if let Some(role) = self.promotion() {
            write!(f, "={}", role.char())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Cap10x8, Chess8x8};

    // Test-only geometries. The `unreachable_pub` allow lets the macro-generated
    // marker struct live inside this private test module without tripping the
    // lint (the generated item is `pub` for crate-level geometries).
    crate::geometry!(
        /// 11x11 = 121 squares, exercising the high square indices (`> 100`).
        #[allow(unreachable_pub)]
        Wide11x11,
        u128,
        11,
        11
    );
    crate::geometry!(
        /// 10x10, exercising ten ranks so a rank renders as two digits.
        #[allow(unreachable_pub)]
        Grand10x10,
        u128,
        10,
        10
    );

    #[test]
    fn wide_move_is_four_bytes() {
        assert_eq!(core::mem::size_of::<WideMove>(), 4);
    }

    #[test]
    fn round_trips_every_simple_kind() {
        let from = Square::<Cap10x8>::new(15);
        let to = Square::<Cap10x8>::new(25);
        let cases = [
            WideMoveKind::Quiet,
            WideMoveKind::Capture,
            WideMoveKind::DoublePawnPush,
            WideMoveKind::EnPassant,
            WideMoveKind::CastleKingside,
            WideMoveKind::CastleQueenside,
        ];
        for kind in cases {
            let m = WideMove::new(from, to, kind);
            assert_eq!(m.from_index(), 15);
            assert_eq!(m.to_index(), 25);
            assert_eq!(m.kind(), kind);
            assert_eq!(m.is_capture(), kind.is_capture());
            assert_eq!(m.is_castle(), kind.is_castle());
            assert_eq!(m.promotion(), None);
            assert!(!m.is_drop());
            assert_eq!(m.from::<Cap10x8>(), from);
            assert_eq!(m.to::<Cap10x8>(), to);
        }
    }

    #[test]
    fn round_trips_wide_promotion_roles_both_flavours() {
        let from = Square::<Cap10x8>::new(60);
        let to = Square::<Cap10x8>::new(70);
        // Including a wide (fairy) role index well past the standard six.
        for role in [
            WideRole::Knight,
            WideRole::Queen,
            WideRole::Hawk,
            WideRole::Elephant,
        ] {
            for capture in [false, true] {
                let kind = WideMoveKind::Promotion { role, capture };
                let m = WideMove::new(from, to, kind);
                assert_eq!(m.kind(), kind);
                assert_eq!(m.promotion(), Some(role));
                assert_eq!(m.is_capture(), capture);
                assert!(!m.is_drop());
                assert_eq!(m.from_index(), 60);
                assert_eq!(m.to_index(), 70);
            }
        }
    }

    #[test]
    fn round_trips_wide_drop_roles() {
        // Every droppable role, including the high fairy indices, round-trips.
        for role in WideRole::ALL {
            // Skip reserved roles for the kind check (they still encode fine).
            let sq = Square::<Cap10x8>::new(42);
            let m = WideMove::drop(role, sq);
            assert!(m.is_drop());
            assert_eq!(m.drop_role(), Some(role));
            assert_eq!(m.kind(), WideMoveKind::Drop { role });
            // A drop has from == to == target.
            assert_eq!(m.from_index(), 42);
            assert_eq!(m.to_index(), 42);
            assert!(!m.is_capture());
            assert!(!m.is_castle());
            assert_eq!(m.promotion(), None);

            // The same role via `new(Drop { .. })`.
            let m2 = WideMove::new(sq, sq, WideMoveKind::Drop { role });
            assert_eq!(m, m2);
        }
    }

    #[test]
    fn covers_high_square_indices() {
        // 11x11 = 121 squares; index 120 is past the 100-square mark.
        let s = Square::<Wide11x11>::new(120);
        let m = WideMove::new(s, Square::<Wide11x11>::new(0), WideMoveKind::Capture);
        assert_eq!(m.from_index(), 120);
        assert_eq!(m.to_index(), 0);
        assert_eq!(m.from::<Wide11x11>(), s);
    }

    #[test]
    fn uci_render_for_8x8_matches_concrete_style() {
        // e2e4: file e (4), rank 2 (index 12) -> file e, rank 4 (index 28).
        let m = WideMove::new(
            Square::<Chess8x8>::new(12),
            Square::<Chess8x8>::new(28),
            WideMoveKind::DoublePawnPush,
        );
        assert_eq!(m.to_uci::<Chess8x8>(), "e2e4");

        // A promotion appends the role letter.
        let p = WideMove::new(
            Square::<Chess8x8>::new(52), // e7
            Square::<Chess8x8>::new(60), // e8
            WideMoveKind::Promotion {
                role: WideRole::Queen,
                capture: false,
            },
        );
        assert_eq!(p.to_uci::<Chess8x8>(), "e7e8q");
    }

    #[test]
    fn uci_render_for_wide_board_files_and_ranks() {
        // Cap10x8: ten files a..j, eight ranks 1..8.
        // Index 9 = file 9 (j), rank 0 (1) -> "j1".
        // Index 79 = file 9 (j), rank 7 (8) -> "j8".
        let m = WideMove::new(
            Square::<Cap10x8>::new(9),
            Square::<Cap10x8>::new(79),
            WideMoveKind::Quiet,
        );
        assert_eq!(m.to_uci::<Cap10x8>(), "j1j8");

        // A drop on the j-file: C@j1.
        let d = WideMove::drop(WideRole::Cannon, Square::<Cap10x8>::new(9));
        assert_eq!(d.to_uci::<Cap10x8>(), "C@j1");
    }

    #[test]
    fn uci_render_two_digit_ranks() {
        // A ten-rank board: rank 10 renders as the digits "10".
        // file 0 (a), rank 9 -> "a10".
        let s = Square::<Grand10x10>::from_file_rank(0, 9).unwrap();
        let m = WideMove::new(s, Square::<Grand10x10>::new(0), WideMoveKind::Quiet);
        assert_eq!(m.to_uci::<Grand10x10>(), "a10a1");
    }

    #[test]
    fn display_is_geometry_free() {
        let m = WideMove::new(
            Square::<Cap10x8>::new(3),
            Square::<Cap10x8>::new(13),
            WideMoveKind::Quiet,
        );
        assert_eq!(alloc::format!("{m}"), "3->13");
        let p = WideMove::new(
            Square::<Cap10x8>::new(3),
            Square::<Cap10x8>::new(13),
            WideMoveKind::Promotion {
                role: WideRole::Hawk,
                capture: false,
            },
        );
        assert_eq!(alloc::format!("{p}"), "3->13=a");
    }
}
