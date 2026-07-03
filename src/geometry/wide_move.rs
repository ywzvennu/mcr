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
//! # Bit layout (`u64`)
//!
//! The low 32 bits hold the base move exactly as a plain chess / fairy move; the
//! high 32 bits carry the Duck-chess addendum (`docs/fairy-variants-architecture.md`
//! §4.4), zero for every non-Duck move.
//!
//! ```text
//! bit:  31 30 | 29 | 28 27 | 26 25 24 23 | 22 21 20 19 18 17 16 | 15 ... 8 | 7 ... 0
//!       \unus/ \rk/ \gate-/ \---kind----/ \---------role--------/ \--from--/ \--to--/
//! ```
//!
//! * **`to`** (bits 0..8): destination square index, `0..128`.
//! * **`from`** (bits 8..16): origin square index, `0..128`. For a drop this is
//!   redundant (a drop has `from == to`), so the public [`WideMove::from`]
//!   reports the target square.
//! * **`role`** (bits 16..23): a [`WideRole`] index `0..=127`, used as the
//!   promotion role or the dropped role depending on the kind; unused (`0`)
//!   otherwise. Seven bits since the Cannon Shogi cannon army grew
//!   `WideRole::COUNT` past 64.
//! * **`kind`** (bits 23..27): the [`WideMoveKind`] tag. Four bits hold the nine
//!   kinds in use with headroom for future fairy kinds (gating, duck-placement,
//!   palace moves) beyond the codes used today.
//! * **`gate`** (bits 27..30): the Seirawan gating addendum. Bits 27..29 carry a
//!   gated-reserve code (`0` = no gate, `1` = Hawk, `2` = Elephant); bit 29
//!   selects, for a castling base move, whether the reserve gates onto the
//!   **rook's** vacated square (`1`) rather than the king's (`0`). For a
//!   non-castling base move the gated square is always the move's origin, so
//!   bit 29 is `0`. These bits are `0` for every non-gating move, so a variant
//!   without gating produces byte-identical words to before this field existed.
//!
//! ## Duck addendum (bits 32..40), Duck chess only
//!
//! * **`duck`** (bits 33..40): the square the neutral Duck is moved to as the
//!   second half of the ply, plus a presence bit. Bit 32 is the **has-duck**
//!   flag (`1` when this move carries a duck placement); bits 33..40 hold the
//!   8-bit duck destination index (`0..128`). The whole high word is `0` for
//!   every non-Duck move, so a variant without the duck mechanic produces words
//!   whose value is identical to the old `u32` layout (zero-extended) — its base
//!   logic, ordering, and equality are unchanged.
//!
//! The rest of the high word carries two further default-off addenda that are
//! mutually exclusive with the Duck fields (no variant sets more than one): the
//! S-House **hand-gate** (bits 41..49) and the Chu-Shogi **Lion** addendum (bits
//! 49..60) — an 8-bit intermediate/second-capture square plus two capture flags
//! and a presence bit, set only by a [`WideMoveKind::LionMove`]. Every other move
//! leaves all of these `0`.

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
    /// A Chu-Shogi **Lion** multi-step move: the Lion (or a lion-power promoted
    /// piece — Horned Falcon, Soaring Eagle) makes up to two King-steps in one
    /// turn, optionally capturing a piece on an **intermediate** square it steps
    /// through *and/or* on the destination. This is the only move kind that can
    /// remove **two** enemy pieces at once, and the only one whose origin may
    /// equal its destination (the *igui* stationary capture and the *jitto*
    /// pass).
    ///
    /// The intermediate square rides in a high-word addendum (see
    /// [`WideMove::lion`]); the two capture flags are carried here. Used only by
    /// Chu Shogi (default-off), so every other variant's packed words are
    /// byte-identical to before this kind existed.
    LionMove {
        /// Whether the move captures the piece on the intermediate square
        /// (the *igui* victim, or the first of a double capture).
        first_capture: bool,
        /// Whether the move captures the piece on the destination square
        /// (the second of a double capture, or a lone destination capture on a
        /// two-step area move).
        second_capture: bool,
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
                | WideMoveKind::LionMove {
                    first_capture: true,
                    ..
                }
                | WideMoveKind::LionMove {
                    second_capture: true,
                    ..
                }
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
// The Chu-Shogi Lion multi-step move (default-off). Its intermediate square and
// two capture flags live in a high-word addendum (bits 49..60), so every
// non-Lion move — for which those bits are `0` — keeps a byte-identical word.
const KIND_LION: u32 = 9;

const TO_SHIFT: u32 = 0;
const FROM_SHIFT: u32 = 8;
const ROLE_SHIFT: u32 = 16;
const KIND_SHIFT: u32 = 23;
const GATE_ROLE_SHIFT: u32 = 27;
const GATE_ON_ROOK_SHIFT: u32 = 29;

const SQ_MASK: u32 = 0xff;
// The role field is **7 bits** (bits 16..23), holding a `WideRole` index `0..=127`
// — widened from 6 bits when the Cannon Shogi cannon army pushed `WideRole::COUNT`
// past 64 (the promoted bishop-hopper is index 64, which a 6-bit field truncated to
// 0 = Pawn). The kind field stays **4 bits** (codes `0..16`, ample for the nine
// kinds) and the gate field each shift up one bit; both still fit below bit 30, so
// the whole high (Duck / hand-gate) word from bit 32 is unchanged.
const ROLE_MASK: u32 = 0x7f;
const KIND_MASK: u32 = 0xf;
const GATE_ROLE_MASK: u32 = 0x3;

// Gated-reserve codes occupying bits 27..29. `0` means no gate.
const GATE_NONE: u32 = 0;
const GATE_HAWK: u32 = 1;
const GATE_ELEPHANT: u32 = 2;

// Duck addendum, in the high 32 bits of the `u64` word. Bit 32 is the presence
// flag; bits 33..41 hold the 8-bit duck destination index.
const DUCK_PRESENT_SHIFT: u32 = 32;
const DUCK_SQUARE_SHIFT: u32 = 33;
const DUCK_PRESENT: u64 = 1 << DUCK_PRESENT_SHIFT;

// Hand-gate addendum (S-House), in free high-word bits above the Duck fields
// (which occupy 32..41). Unlike the 2-bit Seirawan `GATE_ROLE` field (which only
// encodes Hawk/Elephant), a hand-gate carries an arbitrary `WideRole` drawn from
// the crazyhouse hand, so it needs the full 6-bit role index. The two gate
// encodings are mutually exclusive (a variant gates either from the fixed
// reserve or from the hand, never both), and every non-hand-gating move leaves
// these bits `0`, so the words stay bit-identical for Seirawan and every other
// variant.
const HAND_GATE_ROLE_SHIFT: u32 = 41; // bits 41..47: 6-bit WideRole index
const HAND_GATE_ROLE_MASK: u64 = 0x3f;
const HAND_GATE_PRESENT: u64 = 1 << 47;
const HAND_GATE_ON_ROOK: u64 = 1 << 48;

// Chu Lion addendum, in the free high-word bits above the hand-gate fields
// (which end at bit 48). A Lion move (kind `KIND_LION`) carries the square it
// steps through / captures on as an 8-bit index, plus the two capture flags.
// Every non-Lion move leaves these bits `0`, so its word is bit-identical to the
// pre-Lion layout (and the Duck / hand-gate addenda, which occupy the lower half
// of the high word, are mutually exclusive with a Lion move — no variant fields
// both).
const LION_MID_SHIFT: u32 = 49; // bits 49..57: 8-bit intermediate square index
const LION_MID_MASK: u64 = 0xff;
const LION_CAP1: u64 = 1 << 57; // captured a piece on the intermediate square
const LION_CAP2: u64 = 1 << 58; // captured a piece on the destination square
const LION_PRESENT: u64 = 1 << 59; // this word carries a Lion addendum

/// The Seirawan reserve piece a [`WideMove`] gates in as the second half of a
/// back-rank piece's first move: a Hawk (Bishop + Knight) or an Elephant
/// (Rook + Knight). See [`WideMove::with_gate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GateRole {
    /// Gate the Hawk ([`WideRole::Hawk`]).
    Hawk,
    /// Gate the Elephant ([`WideRole::Elephant`]).
    Elephant,
}

impl GateRole {
    /// The [`WideRole`] this reserve places on the board.
    #[must_use]
    #[inline]
    pub const fn role(self) -> WideRole {
        match self {
            GateRole::Hawk => WideRole::Hawk,
            GateRole::Elephant => WideRole::Elephant,
        }
    }

    /// Builds a [`GateRole`] from a placed [`WideRole`], or `None` if the role is
    /// neither reserve piece.
    #[must_use]
    #[inline]
    pub const fn from_role(role: WideRole) -> Option<GateRole> {
        match role {
            WideRole::Hawk => Some(GateRole::Hawk),
            WideRole::Elephant => Some(GateRole::Elephant),
            _ => None,
        }
    }

    #[inline]
    const fn code(self) -> u32 {
        match self {
            GateRole::Hawk => GATE_HAWK,
            GateRole::Elephant => GATE_ELEPHANT,
        }
    }
}

/// Where a gating move places its reserve piece: on the moved piece's origin
/// square, or — for a castling base move — on the castling rook's vacated
/// square. A non-castling gate always targets the origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GateSquare {
    /// Gate onto the square the moving piece (or king) vacated — its origin.
    Origin,
    /// Gate onto the square the castling rook vacated (castling moves only).
    RookOrigin,
}

/// A wide chess move over the generic large-board layer: an 8-bit `from`, an
/// 8-bit `to` (covering `0..128`), and a [`WideMoveKind`] carrying a
/// [`WideRole`] index for promotions and drops.
///
/// Stored as a packed `u32` (see the module docs). The
/// concrete 8x8 [`crate::Move`] is untouched; this is the parallel wide move.
///
/// ```
/// use mce::geometry::{Chess8x8, Square, WideMove, WideMoveKind};
/// let m = WideMove::new(Square::<Chess8x8>::new(12), Square::new(28), WideMoveKind::DoublePawnPush);
/// assert_eq!(m.to_uci::<Chess8x8>(), "e2e4");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WideMove(u64);

impl WideMove {
    /// Packs `from`, `to`, a `role` index, and a `kind` tag into the word. The
    /// base move occupies the low 32 bits; the Duck addendum (high word) is `0`.
    #[inline]
    const fn pack(from: u8, to: u8, role: u32, kind: u32) -> WideMove {
        WideMove(
            ((kind << KIND_SHIFT)
                | (role << ROLE_SHIFT)
                | ((from as u32) << FROM_SHIFT)
                | ((to as u32) << TO_SHIFT)) as u64,
        )
    }

    /// A throwaway sentinel move (a quiet `0 -> 0`) used by the stack-backed
    /// [`WideMoveList`](super::position) to value-initialise its unused inline
    /// tail. It is never read: only the filled prefix is exposed, and each slot
    /// is overwritten by a real push before any read.
    #[must_use]
    #[inline]
    pub(crate) const fn null() -> WideMove {
        WideMove::pack(0, 0, 0, KIND_QUIET)
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
            // A Lion move carries its intermediate square in the addendum; `new`
            // has no square for it, so it packs the base move (with `mid == from`,
            // an inert default) and the capture flags. Use [`WideMove::lion`] to
            // set a real intermediate square.
            WideMoveKind::LionMove {
                first_capture,
                second_capture,
            } => {
                let base = WideMove::pack(from, to, 0, KIND_LION);
                let mut high = LION_PRESENT | ((from as u64 & LION_MID_MASK) << LION_MID_SHIFT);
                if first_capture {
                    high |= LION_CAP1;
                }
                if second_capture {
                    high |= LION_CAP2;
                }
                WideMove(base.0 | high)
            }
        }
    }

    /// Creates a Chu-Shogi **Lion** multi-step move: the Lion on `from` steps
    /// through the intermediate square `mid` to the destination `to`, capturing on
    /// `mid` when `first_capture` and on `to` when `second_capture`.
    ///
    /// `from == to` encodes the two net-zero Lion moves: an *igui* stationary
    /// capture (`first_capture`, the victim on `mid`) or a *jitto* pass (neither
    /// flag). The intermediate square and flags ride in the high-word addendum, so
    /// the low 32 bits stay a plain `from`/`to` move; every non-Lion move leaves
    /// the addendum `0` and is byte-identical.
    #[must_use]
    #[inline]
    pub fn lion<G: Geometry>(
        from: Square<G>,
        to: Square<G>,
        mid: Square<G>,
        first_capture: bool,
        second_capture: bool,
    ) -> WideMove {
        let base = WideMove::pack(from.index(), to.index(), 0, KIND_LION);
        let mut high = LION_PRESENT | ((mid.index() as u64 & LION_MID_MASK) << LION_MID_SHIFT);
        if first_capture {
            high |= LION_CAP1;
        }
        if second_capture {
            high |= LION_CAP2;
        }
        WideMove(base.0 | high)
    }

    /// The intermediate square index of a Lion move (the square it steps through /
    /// captures on), or `None` if this is not a Lion move.
    #[must_use]
    #[inline]
    pub const fn lion_mid_index(self) -> Option<u8> {
        if self.0 & LION_PRESENT != 0 {
            Some(((self.0 >> LION_MID_SHIFT) & LION_MID_MASK) as u8)
        } else {
            None
        }
    }

    /// The intermediate square of a Lion move for a board of geometry `G`, or
    /// `None` if this is not a Lion move.
    #[must_use]
    #[inline]
    pub fn lion_mid<G: Geometry>(self) -> Option<Square<G>> {
        self.lion_mid_index().map(Square::new)
    }

    /// Whether this Lion move captures on its intermediate square.
    #[must_use]
    #[inline]
    pub const fn lion_captures_mid(self) -> bool {
        self.0 & LION_PRESENT != 0 && self.0 & LION_CAP1 != 0
    }

    /// Whether this Lion move captures on its destination square.
    #[must_use]
    #[inline]
    pub const fn lion_captures_dest(self) -> bool {
        self.0 & LION_PRESENT != 0 && self.0 & LION_CAP2 != 0
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

    /// The raw packed `u64` word backing this move (the bit layout in the module
    /// docs). The stable representation the compact binary wire codec serializes;
    /// every accessor above derives from it, so it round-trips exactly.
    #[must_use]
    #[inline]
    pub(crate) const fn to_raw(self) -> u64 {
        self.0
    }

    /// Rebuilds a move from a raw packed `u64` word, the inverse of
    /// [`to_raw`](Self::to_raw). Any bit pattern is a representable move (an
    /// out-of-range role index decodes as `Pawn`), so this never fails.
    #[must_use]
    #[inline]
    pub(crate) const fn from_raw(bits: u64) -> WideMove {
        WideMove(bits)
    }

    /// The low 32 bits: the base move, with the Duck addendum stripped.
    #[inline]
    const fn base(self) -> u32 {
        self.0 as u32
    }

    /// The raw kind tag in bits 21..26.
    #[inline]
    const fn kind_tag(self) -> u32 {
        (self.base() >> KIND_SHIFT) & KIND_MASK
    }

    /// The raw role index in bits 16..21.
    #[inline]
    const fn role_index(self) -> usize {
        ((self.base() >> ROLE_SHIFT) & ROLE_MASK) as usize
    }

    /// Returns the origin square index, `0..128`.
    ///
    /// For a drop this equals [`WideMove::to_index`] (a drop has no distinct
    /// origin).
    #[must_use]
    #[inline]
    pub const fn from_index(self) -> u8 {
        ((self.base() >> FROM_SHIFT) & SQ_MASK) as u8
    }

    /// Returns the destination square index, `0..128`.
    #[must_use]
    #[inline]
    pub const fn to_index(self) -> u8 {
        ((self.base() >> TO_SHIFT) & SQ_MASK) as u8
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
            KIND_LION => WideMoveKind::LionMove {
                first_capture: self.0 & LION_CAP1 != 0,
                second_capture: self.0 & LION_CAP2 != 0,
            },
            // Any remaining code is a drop; codes are dense, so this only matches
            // KIND_DROP for values this crate produces.
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
        ) || (self.kind_tag() == KIND_LION && self.0 & (LION_CAP1 | LION_CAP2) != 0)
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

    /// Returns a copy of this move that, in addition to its base effect, gates a
    /// Seirawan reserve piece (`gate`) onto the square selected by `square`.
    ///
    /// The base move's kind, origin, destination, and any promotion role are
    /// preserved; only the gating addendum is added. For a non-castling base move
    /// `square` must be [`GateSquare::Origin`] (the gate lands on the vacated
    /// origin); [`GateSquare::RookOrigin`] is meaningful only for a castling base
    /// move, where it places the reserve on the castling rook's start square
    /// instead of the king's.
    #[must_use]
    #[inline]
    pub const fn with_gate(self, gate: GateRole, square: GateSquare) -> WideMove {
        let on_rook = match square {
            GateSquare::Origin => 0,
            GateSquare::RookOrigin => 1,
        };
        // The gate fields live in the low 32 bits; the high word (Duck addendum)
        // is preserved.
        let high = self.0 & 0xffff_ffff_0000_0000;
        let base = self.base() & !((GATE_ROLE_MASK << GATE_ROLE_SHIFT) | (1 << GATE_ON_ROOK_SHIFT));
        WideMove(
            high | ((base | (gate.code() << GATE_ROLE_SHIFT) | (on_rook << GATE_ON_ROOK_SHIFT))
                as u64),
        )
    }

    /// Returns the gated reserve piece if this move gates one, otherwise `None`.
    #[must_use]
    #[inline]
    pub fn gate(self) -> Option<GateRole> {
        match (self.base() >> GATE_ROLE_SHIFT) & GATE_ROLE_MASK {
            GATE_HAWK => Some(GateRole::Hawk),
            GATE_ELEPHANT => Some(GateRole::Elephant),
            _ => None,
        }
    }

    /// Returns which vacated square this move gates onto: the origin, or — for a
    /// castling base move — the rook's start square. Returns
    /// [`GateSquare::Origin`] for a move with no gate.
    #[must_use]
    #[inline]
    pub const fn gate_square(self) -> GateSquare {
        if (self.base() >> GATE_ON_ROOK_SHIFT) & 1 == 1 {
            GateSquare::RookOrigin
        } else {
            GateSquare::Origin
        }
    }

    /// Returns `true` if this move gates a Seirawan reserve piece.
    #[must_use]
    #[inline]
    pub const fn is_gating(self) -> bool {
        (self.base() >> GATE_ROLE_SHIFT) & GATE_ROLE_MASK != GATE_NONE
    }

    /// Returns a copy of this move that, in addition to its base effect, gates the
    /// arbitrary hand piece `role` (S-House) onto the square selected by `square`.
    ///
    /// The hand-gate counterpart of [`with_gate`](WideMove::with_gate): where that
    /// encodes a fixed Hawk/Elephant reserve in 2 bits, this carries the full
    /// [`WideRole`] index drawn from the crazyhouse hand. The base move (kind,
    /// squares, any promotion role) and the Duck addendum are preserved.
    #[must_use]
    #[inline]
    pub fn with_hand_gate<G: Geometry>(self, role: WideRole, square: GateSquare) -> WideMove {
        let _ = core::marker::PhantomData::<G>;
        let mut w = self.0 & !((HAND_GATE_ROLE_MASK << HAND_GATE_ROLE_SHIFT) | HAND_GATE_ON_ROOK);
        w |= HAND_GATE_PRESENT;
        w |= ((role.index() as u64) & HAND_GATE_ROLE_MASK) << HAND_GATE_ROLE_SHIFT;
        if matches!(square, GateSquare::RookOrigin) {
            w |= HAND_GATE_ON_ROOK;
        }
        WideMove(w)
    }

    /// Returns the hand-gated piece (S-House) if this move gates one from the hand,
    /// otherwise `None`.
    #[must_use]
    #[inline]
    pub fn hand_gate(self) -> Option<WideRole> {
        if self.0 & HAND_GATE_PRESENT == 0 {
            return None;
        }
        let idx = ((self.0 >> HAND_GATE_ROLE_SHIFT) & HAND_GATE_ROLE_MASK) as usize;
        WideRole::from_index(idx)
    }

    /// Returns which vacated square a hand-gate lands on: the origin, or — for a
    /// castling base move — the rook's start square.
    #[must_use]
    #[inline]
    pub const fn hand_gate_square(self) -> GateSquare {
        if self.0 & HAND_GATE_ON_ROOK != 0 {
            GateSquare::RookOrigin
        } else {
            GateSquare::Origin
        }
    }

    /// Returns a copy of this move carrying a Duck-chess placement: the second
    /// half of the ply moves the neutral Duck onto `square`. The base move is
    /// unchanged; only the high-word Duck addendum is set.
    ///
    /// Used only by Duck chess (`docs/fairy-variants-architecture.md` §4.4); every
    /// other variant leaves the addendum `0`, so its words are bit-identical to
    /// the zero-extended old `u32` layout.
    #[must_use]
    #[inline]
    pub fn with_duck<G: Geometry>(self, square: Square<G>) -> WideMove {
        let high = DUCK_PRESENT | ((square.index() as u64) << DUCK_SQUARE_SHIFT);
        WideMove((self.0 & 0x0000_0000_ffff_ffff) | high)
    }

    /// Returns the square the Duck is placed on this ply, or `None` if this move
    /// carries no Duck placement.
    #[must_use]
    #[inline]
    pub const fn duck_to_index(self) -> Option<u8> {
        if self.0 & DUCK_PRESENT != 0 {
            Some((self.0 >> DUCK_SQUARE_SHIFT) as u8)
        } else {
            None
        }
    }

    /// Returns the Duck destination square for a board of geometry `G`, or `None`
    /// if this move carries no Duck placement.
    #[must_use]
    #[inline]
    pub fn duck_to<G: Geometry>(self) -> Option<Square<G>> {
        self.duck_to_index().map(Square::new)
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

    /// Appends the UCI promotion suffix for `role`: the role's lowercase base
    /// letter, prefixed — for a role that has no bare letter of its own — with the
    /// same disambiguation SAN uses (`+` for a Shogi promoted role, `*` / `**` /
    /// `***` for the overflow tiers, `=` for the recycled third tier).
    ///
    /// A standard promotion (Queen, Rook, Bishop, Knight, and every other role
    /// with its own letter) keeps the bare lowercase suffix, so `e7e8q` and every
    /// existing UCI string is byte-identical. The prefix matters only for
    /// **overflow** promotion roles whose base letter collides with a plain role's:
    /// e.g. in Ordamirror a pawn may promote to either the Lancer (`f`) or the
    /// overflow Falcon (also base `f`). Without the prefix both render `...f` and
    /// the Falcon promotion is unrecoverable through [`parse_uci`]; with it they
    /// render `...f` vs. `...*f`, so board-move UCI stays injective and each
    /// promotion round-trips back to its exact [`WideMove`].
    ///
    /// [`parse_uci`]: super::GenericPosition::parse_uci
    fn push_promotion_suffix(out: &mut String, role: WideRole) {
        if role.is_promoted() {
            out.push('+');
        } else if role.is_overflow4() {
            out.push_str("***");
        } else if role.is_overflow2() {
            out.push_str("**");
        } else if role.is_overflow() {
            out.push('*');
        } else if role.is_overflow3() {
            out.push('=');
        }
        out.push(role.char());
    }

    /// Formats this move in UCI long algebraic notation over the geometry `G`.
    ///
    /// Squares render as a file letter plus a 1-based rank number (so a
    /// ten-rank board reaches `a10`). A promotion appends the promotion role's
    /// lowercase letter, carrying the same `+` / `*` / `**` / `***` / `=` prefix
    /// SAN uses when the role has no bare letter of its own, so two
    /// promotions to roles that share a base letter stay distinct; a drop uses the
    /// `{ROLE}@{square}` form.
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
        // A Chu Lion move renders as `<from><to>` plus its intermediate square:
        // `*<mid>` when the move captures on that square (an igui or the first leg
        // of a double capture), `-<mid>` when it only steps through. Spelling the
        // mid on **every** Lion move (not just capturing ones) makes the rendering
        // injective — the `(from, to, mid)` triple uniquely identifies a Lion move,
        // so two area paths to the same destination stay distinct (e.g. a
        // distance-two capture reached through either of two elbow squares). The
        // sole exception is the *jitto* pass (`from == to`, no capture on either
        // leg), which renders as the bare `<sq><sq>`.
        if self.kind_tag() == KIND_LION {
            let mut s = String::with_capacity(12);
            Self::render_square::<G>(&mut s, self.from_index());
            Self::render_square::<G>(&mut s, self.to_index());
            let is_pass = self.from_index() == self.to_index()
                && !self.lion_captures_mid()
                && !self.lion_captures_dest();
            if !is_pass {
                s.push(if self.lion_captures_mid() { '*' } else { '-' });
                if let Some(mid) = self.lion_mid_index() {
                    Self::render_square::<G>(&mut s, mid);
                }
            }
            return s;
        }
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
            Self::push_promotion_suffix(&mut s, role);
        }
        // A Seirawan gate is rendered FSF-style as `/<PIECE>` appended to the base
        // move (e.g. `b1c3/H`); a gate onto the castling rook's square instead of
        // the king's origin appends the rook square (e.g. `e1g1/H@h1`).
        if let Some(gate) = self.gate() {
            s.push('/');
            s.push(gate.role().upper_char());
            if matches!(self.gate_square(), GateSquare::RookOrigin) {
                // Distinguish a castling gate onto the rook's vacated square.
                s.push('@');
                s.push('r');
            }
        }
        // A hand-gate (S-House) renders the same `/<PIECE>` way, with the gated
        // piece drawn from the crazyhouse hand.
        if let Some(role) = self.hand_gate() {
            s.push('/');
            s.push(role.upper_char());
            if matches!(self.hand_gate_square(), GateSquare::RookOrigin) {
                s.push('@');
                s.push('r');
            }
        }
        // A Duck placement is rendered FSF-style as the base move, a comma, and the
        // duck sub-move `<from><to>`. FSF's duck `from` field is cosmetic (it
        // ignores it on input and prints the moved piece's destination), so we
        // render the base move's destination as the duck `from` to match its
        // divide strings exactly (e.g. `a2a3,a3a2`).
        if let Some(duck_to) = self.duck_to_index() {
            s.push(',');
            Self::render_square::<G>(&mut s, self.to_index());
            Self::render_square::<G>(&mut s, duck_to);
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

// -- serde ------------------------------------------------------------------
//
// `WideMove` packs its whole state into a private `u64`. Rather than leak that
// bit layout onto the wire — and mirroring the concrete `Move`, which serializes
// through a public `{ from, to, kind }` shape rather than its packed integer — it
// serializes through [`WideMoveData`]: the public origin and destination square
// *indices* (geometry-free `u8`s, `0..128`), the [`WideMoveKind`] (which already
// carries any promotion / drop role), and the three optional fairy addenda — the
// Seirawan gate, the S-House hand-gate, and the Duck placement. A move rebuilds
// losslessly from these, with no geometry needed (the squares are plain indices),
// so the non-generic `Deserialize` impl has everything it requires.
#[cfg(feature = "serde")]
#[derive(serde::Serialize, serde::Deserialize)]
struct WideMoveData {
    from: u8,
    to: u8,
    kind: WideMoveKind,
    /// The Seirawan reserve this move gates in, and onto which vacated square.
    gate: Option<(GateRole, GateSquare)>,
    /// The arbitrary hand piece this move gates in (S-House), and onto which
    /// vacated square.
    hand_gate: Option<(WideRole, GateSquare)>,
    /// The square the neutral Duck is placed on this ply (Duck chess), as an
    /// index.
    duck: Option<u8>,
    /// The intermediate square of a Chu Lion move (the square it steps through /
    /// captures on), as an index; `None` for every non-Lion move. The two Lion
    /// capture flags already ride in [`kind`](WideMoveData::kind).
    lion_mid: Option<u8>,
}

#[cfg(feature = "serde")]
impl WideMove {
    /// Builds the public wire shape from this move's accessors.
    fn to_data(self) -> WideMoveData {
        WideMoveData {
            from: self.from_index(),
            to: self.to_index(),
            kind: self.kind(),
            gate: self.gate().map(|g| (g, self.gate_square())),
            hand_gate: self.hand_gate().map(|r| (r, self.hand_gate_square())),
            duck: self.duck_to_index(),
            lion_mid: self.lion_mid_index(),
        }
    }

    /// Rebuilds a move from its public wire shape. Geometry-free: the squares are
    /// plain indices, so `pack` (not the typed `new`) reassembles the base word,
    /// and the gate / hand-gate / Duck addenda are reapplied through the same
    /// bit layout the typed builders use.
    fn from_data(data: WideMoveData) -> WideMove {
        let WideMoveData {
            from,
            to,
            kind,
            gate,
            hand_gate,
            duck,
            lion_mid,
        } = data;
        let mut mv = match kind {
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
            // A Lion move: its capture flags ride in the addendum; the
            // intermediate square is reapplied from `lion_mid` below.
            WideMoveKind::LionMove {
                first_capture,
                second_capture,
            } => {
                let base = WideMove::pack(from, to, 0, KIND_LION);
                let mut high = LION_PRESENT;
                if first_capture {
                    high |= LION_CAP1;
                }
                if second_capture {
                    high |= LION_CAP2;
                }
                WideMove(base.0 | high)
            }
        };
        if let Some(mid) = lion_mid {
            mv = WideMove(
                (mv.0 & !(LION_MID_MASK << LION_MID_SHIFT))
                    | LION_PRESENT
                    | ((mid as u64 & LION_MID_MASK) << LION_MID_SHIFT),
            );
        }
        if let Some((role, square)) = gate {
            mv = mv.with_gate(role, square);
        }
        if let Some((role, square)) = hand_gate {
            // The hand-gate fields are geometry-free bits in the high word; set
            // them straight (the typed `with_hand_gate` only ever touches these
            // same bits, and ignores its geometry parameter).
            let mut bits =
                mv.0 & !((HAND_GATE_ROLE_MASK << HAND_GATE_ROLE_SHIFT) | HAND_GATE_ON_ROOK);
            bits |= HAND_GATE_PRESENT;
            bits |= ((role.index() as u64) & HAND_GATE_ROLE_MASK) << HAND_GATE_ROLE_SHIFT;
            if matches!(square, GateSquare::RookOrigin) {
                bits |= HAND_GATE_ON_ROOK;
            }
            mv = WideMove(bits);
        }
        if let Some(index) = duck {
            // The Duck addendum is the high word; the base move is the low 32 bits.
            mv = WideMove(
                (mv.0 & 0x0000_0000_ffff_ffff)
                    | DUCK_PRESENT
                    | ((index as u64) << DUCK_SQUARE_SHIFT),
            );
        }
        mv
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for WideMove {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(&self.to_data(), serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for WideMove {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let data = <WideMoveData as serde::Deserialize>::deserialize(deserializer)?;
        Ok(WideMove::from_data(data))
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
    fn wide_move_is_eight_bytes() {
        // The base move occupies the low 32 bits; the high 32 carry the Duck
        // addendum (zero for every non-Duck move).
        assert_eq!(core::mem::size_of::<WideMove>(), 8);
    }

    #[test]
    fn duck_addendum_round_trips_and_is_default_off() {
        let m = WideMove::new(
            Square::<Chess8x8>::new(8),  // a2
            Square::<Chess8x8>::new(16), // a3
            WideMoveKind::Quiet,
        );
        // No duck by default.
        assert_eq!(m.duck_to_index(), None);
        // A non-duck move's word value equals the zero-extended old u32 layout.
        assert_eq!(m.0 & 0xffff_ffff_0000_0000, 0);

        let d = m.with_duck(Square::<Chess8x8>::new(8)); // duck to a2
        assert_eq!(d.duck_to_index(), Some(8));
        assert_eq!(d.duck_to::<Chess8x8>(), Some(Square::<Chess8x8>::new(8)));
        // The base move is untouched.
        assert_eq!(d.from_index(), m.from_index());
        assert_eq!(d.to_index(), m.to_index());
        assert_eq!(d.kind(), m.kind());
        // FSF-style rendering: base move, comma, duck `<piece_dest><duck_to>`.
        assert_eq!(d.to_uci::<Chess8x8>(), "a2a3,a3a2");
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
    fn lion_move_round_trips_and_is_default_off() {
        let from = Square::<Wide11x11>::new(50);
        let to = Square::<Wide11x11>::new(72);
        let mid = Square::<Wide11x11>::new(61);
        // A double capture: captures the intermediate and the destination.
        let d = WideMove::lion(from, to, mid, true, true);
        assert_eq!(d.from_index(), 50);
        assert_eq!(d.to_index(), 72);
        assert_eq!(d.lion_mid_index(), Some(61));
        assert!(d.lion_captures_mid());
        assert!(d.lion_captures_dest());
        assert!(d.is_capture());
        assert_eq!(
            d.kind(),
            WideMoveKind::LionMove {
                first_capture: true,
                second_capture: true,
            }
        );
        // An igui: from == to, captures only the adjacent (mid) square.
        let igui = WideMove::lion(from, from, mid, true, false);
        assert_eq!(igui.from_index(), igui.to_index());
        assert!(igui.lion_captures_mid());
        assert!(!igui.lion_captures_dest());
        assert!(igui.is_capture());
        // A jitto pass: from == to, no capture at all — but still a Lion move.
        let pass = WideMove::lion(from, from, mid, false, false);
        assert!(!pass.is_capture());
        assert_eq!(pass.lion_mid_index(), Some(61));
        assert_eq!(
            pass.kind(),
            WideMoveKind::LionMove {
                first_capture: false,
                second_capture: false,
            }
        );
        // A non-Lion move carries no Lion addendum (default-off byte-identity):
        // the whole high word is zero, exactly as before this kind existed.
        let plain = WideMove::new(from, to, WideMoveKind::Capture);
        assert_eq!(plain.lion_mid_index(), None);
        assert_eq!(plain.0 & 0xffff_ffff_0000_0000, 0);
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
