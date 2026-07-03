//! A generic chess position over an arbitrary [`Geometry`] and [`WideVariant`].
//!
//! This is the parallel generic analogue of the concrete [`crate::Position`]: a
//! [`Board<G>`] plus side-to-move, castling, en-passant, and clocks, with legal
//! move generation, make-move, FEN, and perft — all parametrised over a board
//! geometry `G` and a rule layer `V: WideVariant<G>`
//! (`docs/fairy-variants-architecture.md` §4, §5).
//!
//! [`GenericPosition<Chess8x8, StandardChess>`](GenericPosition) is the
//! reference instantiation; its perft equals the concrete
//! [`crate::perft`] exactly (see `tests/perft_generic.rs`). The
//! concrete 8x8 path is untouched — this is a separate, additive layer.
//!
//! ## Correctness over speed
//!
//! The generator mirrors the concrete engine's fast pin / check-mask discipline
//! (king-danger map, single-/double-check handling, pinned-piece line
//! confinement, the en-passant discovered-check filter) but drives every piece
//! through the variant's [`WideVariant::role_attacks`] hook with a simple
//! per-piece loop rather than the concrete engine's bulk pawn shifts. This keeps
//! the code general for fairy roles and easy to validate; a later phase tunes it.

use alloc::string::String;
use alloc::vec::Vec;
use core::marker::PhantomData;

use super::attacks::{
    between, bishop_attacks_masked, king_attack_lines, king_attack_lines_diag_capped, line,
    queen_attacks_masked, rook_attacks_masked, KingLineMasks,
};
use super::role::WideRole;
use super::variant::{RoyalSlider, WideEndReason, WideVariant};
use super::zobrist;
use super::{
    Bitboard, Board, GateRole, GateSquare, Geometry, Square, WideMove, WideMoveKind, WidePiece,
};
use crate::Color;

/// The castling rights of a generic position: per color and side, the file the
/// castling rook starts on (`None` if that right is gone).
///
/// The generic layer stores the rook's start file (`0..WIDTH`) rather than a
/// packed nibble so it works for any board width. Standard chess uses the
/// a-file (queenside) and h-file (kingside) rooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GenericCastling {
    /// `[color][side]`: side `0` is kingside, `1` is queenside. `255` means no
    /// right; any other value is the rook's start file.
    rook_file: [[u8; 2]; 2],
}

const KINGSIDE: usize = 0;
const QUEENSIDE: usize = 1;
const NO_RIGHT: u8 = 255;

#[inline]
const fn color_ix(c: Color) -> usize {
    match c {
        Color::White => 0,
        Color::Black => 1,
    }
}

impl GenericCastling {
    /// No castling rights at all.
    pub const NONE: GenericCastling = GenericCastling {
        rook_file: [[NO_RIGHT; 2]; 2],
    };

    /// The standard rights for an 8x8 board: kingside rook on the h-file
    /// (`WIDTH - 1`), queenside rook on the a-file (`0`), for both colors.
    #[must_use]
    pub fn standard<G: Geometry>() -> GenericCastling {
        let mut c = GenericCastling::NONE;
        for color in Color::ALL {
            c.set(color, KINGSIDE, Some(G::WIDTH - 1));
            c.set(color, QUEENSIDE, Some(0));
        }
        c
    }

    /// Returns the start file of `color`'s rook for the given `side`, or `None`
    /// if that right is gone.
    #[must_use]
    #[inline]
    pub fn rook_file(self, color: Color, side: usize) -> Option<u8> {
        let f = self.rook_file[color_ix(color)][side];
        if f == NO_RIGHT {
            None
        } else {
            Some(f)
        }
    }

    /// Sets (or clears, with `None`) the rook start file for `color`/`side`.
    #[inline]
    pub fn set(&mut self, color: Color, side: usize, file: Option<u8>) {
        self.rook_file[color_ix(color)][side] = file.unwrap_or(NO_RIGHT);
    }

    /// Revokes both of `color`'s rights (a king move).
    #[inline]
    pub fn revoke_color(&mut self, color: Color) {
        self.rook_file[color_ix(color)] = [NO_RIGHT; 2];
    }

    /// Returns `true` if `color` holds either right.
    #[must_use]
    #[inline]
    pub fn has_any(self, color: Color) -> bool {
        self.rook_file[color_ix(color)] != [NO_RIGHT; 2]
    }

    /// Returns `true` if no rights remain for either color.
    #[must_use]
    #[inline]
    pub fn is_empty(self) -> bool {
        self.rook_file == [[NO_RIGHT; 2]; 2]
    }
}

/// Bit `0` = Hawk available, bit `1` = Elephant available, in a per-color reserve
/// mask. Matches the [`GateRole`](super::GateRole) order.
const RESERVE_HAWK: u8 = 0b01;
const RESERVE_ELEPHANT: u8 = 0b10;

/// The Seirawan gating state of a generic position: the reserve pieces each side
/// still holds in hand and the squares from which a piece may still gate one in.
///
/// This is the one piece of variant state the generic engine threads through
/// move generation and `apply`. For every non-gating
/// variant it is [`GenericGating::NONE`] — an empty eligible set and no reserves —
/// so the gating code paths, all guarded behind
/// [`WideVariant::supports_gating`], never fire and the produced moves and state
/// stay byte-identical to a build without gating.
pub struct GenericGating<G: Geometry> {
    /// The squares a piece may still gate from: a back-rank square holding a
    /// piece that has not yet moved, for a side that still has a reserve.
    eligible: Bitboard<G>,
    /// Per-color reserve mask: `[white, black]`, each a combination of
    /// [`RESERVE_HAWK`] / [`RESERVE_ELEPHANT`].
    reserves: [u8; 2],
}

// Manual trait impls so the geometry marker `G` need not implement these; the
// bounds rest on `Bitboard<G>` (which carries them only over `G::Bits`), exactly
// as `Board<G>` does.
impl<G: Geometry> Clone for GenericGating<G> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<G: Geometry> Copy for GenericGating<G> {}

impl<G: Geometry> PartialEq for GenericGating<G> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.eligible == other.eligible && self.reserves == other.reserves
    }
}

impl<G: Geometry> Eq for GenericGating<G> {}

impl<G: Geometry> core::hash::Hash for GenericGating<G> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        // Hash the eligible set by its square indices so the impl is
        // unconditional in `G::Bits` (mirroring `Square`'s unconditional `Hash`),
        // keeping `GenericState`'s derived `Hash` free of a `G::Bits: Hash` bound.
        for sq in self.eligible {
            sq.index().hash(state);
        }
        self.reserves.hash(state);
    }
}

impl<G: Geometry> Default for GenericGating<G> {
    #[inline]
    fn default() -> Self {
        GenericGating::NONE
    }
}

impl<G: Geometry> core::fmt::Debug for GenericGating<G> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GenericGating")
            .field("eligible", &self.eligible.count())
            .field("reserves", &self.reserves)
            .finish()
    }
}

impl<G: Geometry> GenericGating<G> {
    /// No gating: an empty eligible set and no reserves. The value every
    /// non-Seirawan variant carries.
    pub const NONE: GenericGating<G> = GenericGating {
        eligible: Bitboard::EMPTY,
        reserves: [0, 0],
    };

    /// Builds a gating state from an eligible square set and per-color reserve
    /// availability for the Hawk and Elephant.
    #[must_use]
    #[inline]
    pub fn new(eligible: Bitboard<G>, white: [bool; 2], black: [bool; 2]) -> GenericGating<G> {
        GenericGating {
            eligible,
            reserves: [reserve_mask(white), reserve_mask(black)],
        }
    }

    /// The set of squares `color` may still gate from.
    #[must_use]
    #[inline]
    pub fn eligible(self) -> Bitboard<G> {
        self.eligible
    }

    /// Returns `true` if `color` still holds the given reserve piece.
    #[must_use]
    #[inline]
    pub fn has_reserve(self, color: Color, gate: super::GateRole) -> bool {
        self.reserves[color_ix(color)] & gate_bit(gate) != 0
    }

    /// Returns `true` if `color` holds at least one reserve piece.
    #[must_use]
    #[inline]
    pub fn any_reserve(self, color: Color) -> bool {
        self.reserves[color_ix(color)] != 0
    }

    /// Consumes `color`'s reserve `gate` (it has been gated in).
    #[inline]
    fn take_reserve(&mut self, color: Color, gate: super::GateRole) {
        self.reserves[color_ix(color)] &= !gate_bit(gate);
    }

    /// Clears `square` from the eligible set (its piece has moved or been
    /// captured), a no-op if it was not eligible.
    #[inline]
    fn vacate(&mut self, square: Square<G>) {
        self.eligible = self.eligible.without(square);
    }
}

/// Packs a `[hawk, elephant]` availability pair into a reserve mask.
#[inline]
fn reserve_mask(avail: [bool; 2]) -> u8 {
    (if avail[0] { RESERVE_HAWK } else { 0 }) | (if avail[1] { RESERVE_ELEPHANT } else { 0 })
}

/// The reserve-mask bit for a [`GateRole`](super::GateRole).
#[inline]
const fn gate_bit(gate: super::GateRole) -> u8 {
    match gate {
        super::GateRole::Hawk => RESERVE_HAWK,
        super::GateRole::Elephant => RESERVE_ELEPHANT,
    }
}

/// The placement-phase pocket of a generic position: the pieces each side has
/// **still to deploy** in a setup-phase variant (Sittuyin), held off-board until
/// dropped onto the player's own territory (`docs/fairy-variants-architecture.md`
/// §4.4).
///
/// It carries one count per [`WideRole`] per color. For every variant **without**
/// a placement phase the value is [`GenericPlacement::NONE`] (all zeros) and the
/// placement code paths — all guarded behind [`WideVariant::has_placement`] —
/// never fire, so produced moves, state, and FEN stay byte-identical to a build
/// without the placement mechanic. It carries no [`Geometry`] data (the pocket is
/// a piece-count tally, board-size-independent), so it is a plain `Copy` value.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct GenericPlacement {
    /// White's undeployed piece counts, indexed by [`WideRole::index`].
    white: [u8; WideRole::COUNT],
    /// Black's undeployed piece counts, indexed by [`WideRole::index`].
    black: [u8; WideRole::COUNT],
}

impl GenericPlacement {
    /// The empty pocket: no pieces in hand for either side. The value every
    /// non-placement variant carries, and the state of a placement variant once
    /// both sides are fully deployed.
    pub const NONE: GenericPlacement = GenericPlacement {
        white: [0; WideRole::COUNT],
        black: [0; WideRole::COUNT],
    };

    /// Builds a pocket from explicit per-color, per-role counts.
    #[must_use]
    pub const fn new(
        white: [u8; WideRole::COUNT],
        black: [u8; WideRole::COUNT],
    ) -> GenericPlacement {
        GenericPlacement { white, black }
    }

    /// Returns the number of `role` pieces `color` has still to deploy.
    #[must_use]
    #[inline]
    pub fn count(self, color: Color, role: WideRole) -> u8 {
        match color {
            Color::White => self.white[role.index()],
            Color::Black => self.black[role.index()],
        }
    }

    /// Returns `true` if `color` has any piece still to deploy.
    #[must_use]
    #[inline]
    pub fn any(self, color: Color) -> bool {
        let counts = match color {
            Color::White => &self.white,
            Color::Black => &self.black,
        };
        counts.iter().any(|&n| n != 0)
    }

    /// Removes one `role` piece from `color`'s pocket (it has just been dropped).
    #[inline]
    fn take(&mut self, color: Color, role: WideRole) {
        let slot = match color {
            Color::White => &mut self.white[role.index()],
            Color::Black => &mut self.black[role.index()],
        };
        *slot = slot.saturating_sub(1);
    }

    /// Adds one `role` piece to `color`'s hand (a Shogi capture banks the captured
    /// piece, flipped to the captor's side and reverted to its base role).
    #[inline]
    fn add(&mut self, color: Color, role: WideRole) {
        let slot = match color {
            Color::White => &mut self.white[role.index()],
            Color::Black => &mut self.black[role.index()],
        };
        *slot = slot.saturating_add(1);
    }
}

impl Default for GenericPlacement {
    fn default() -> Self {
        GenericPlacement::NONE
    }
}

impl core::fmt::Debug for GenericPlacement {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GenericPlacement")
            .field("white", &self.white)
            .field("black", &self.black)
            .finish()
    }
}

/// The non-board state of a generic position: side to move, castling rights,
/// en-passant target square, and the two move clocks.
#[derive(Clone, Copy)]
pub struct GenericState<G: Geometry> {
    /// The side to move.
    pub turn: Color,
    /// The castling rights.
    pub castling: GenericCastling,
    /// The en-passant target square (the square a pawn skipped), if any.
    pub ep_square: Option<Square<G>>,
    /// The Seirawan gating state (reserves in hand + gating-eligible squares).
    /// [`GenericGating::NONE`] for every non-gating variant.
    pub gating: GenericGating<G>,
    /// The square the neutral Duck occupies (Duck chess only). `None` for every
    /// other variant — including before the Duck enters the board on the first
    /// move — so the duck-blocker and two-part-move code paths, all guarded
    /// behind [`WideVariant::has_duck`], never fire and produced moves and state
    /// stay byte-identical to a build without the duck mechanic.
    pub duck: Option<Square<G>>,
    /// The setup-phase pocket: the pieces each side has yet to deploy
    /// (Sittuyin only). [`GenericPlacement::NONE`] for every other variant — so
    /// the placement code paths, all guarded behind
    /// [`WideVariant::has_placement`], never fire and produced moves, state, and
    /// FEN stay byte-identical to a build without the placement mechanic.
    pub placement: GenericPlacement,
    /// The halfmove clock (plies since the last capture or pawn move).
    pub halfmove_clock: u16,
    /// The fullmove number (incremented after a black move).
    pub fullmove_number: u16,
    /// The number of **consecutive passes** immediately preceding this position
    /// (Janggi only): `0` after any real move (and at the start of a game), `1`
    /// after a single pass, `2` after two passes in a row. Two consecutive passes
    /// **end the game** — a side to move with `consecutive_passes >= 2` has no legal
    /// move at all (Fairy-Stockfish returns zero). Passes are gated behind
    /// [`WideVariant::allows_pass`], so for every other variant this stays `0` and
    /// produced moves, state, and FEN are byte-identical to a build without it. It
    /// is transient (a freshly parsed FEN resets it to `0`).
    pub consecutive_passes: u8,
    /// **Alice chess** per-piece board membership: the set of squares whose
    /// occupant is on **plane B** (the second of the two mirror boards). A piece
    /// on a square *not* in this mask is on plane A. At most one piece occupies
    /// any square across both planes, so the [`Board`] holds every piece and this
    /// mask alone says which plane each is on.
    ///
    /// [`Bitboard::EMPTY`] for every non-Alice variant — and at the start of an
    /// Alice game (all pieces begin on plane A) — so the two-plane movement,
    /// transfer, and king-safety code paths, all guarded behind
    /// [`WideVariant::is_alice`], never fire and produced moves, state, and FEN
    /// stay byte-identical to a build without the Alice mechanic.
    pub board_b: Bitboard<G>,
}

impl<G: Geometry> core::fmt::Debug for GenericState<G> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GenericState")
            .field("turn", &self.turn)
            .field("castling", &self.castling)
            .field("ep_square", &self.ep_square.map(|s| s.index()))
            .field("gating", &self.gating)
            .field("duck", &self.duck.map(|s| s.index()))
            .field("placement", &self.placement)
            .field("halfmove_clock", &self.halfmove_clock)
            .field("fullmove_number", &self.fullmove_number)
            .field("consecutive_passes", &self.consecutive_passes)
            .field("board_b", &self.board_b.count())
            .finish()
    }
}

// Manual `PartialEq`/`Eq` (mirroring `GenericGating` and `Board`): every field is
// comparable under just `G: Geometry` — `Bitboard`, `Square`, and `GenericGating`
// all carry manual impls bounded on `G::Bits`, not on `G` — so writing the impl by
// hand keeps every generic user of `GenericState` free of the spurious `G:
// PartialEq` bound the derive would add.
impl<G: Geometry> PartialEq for GenericState<G> {
    fn eq(&self, other: &Self) -> bool {
        self.turn == other.turn
            && self.castling == other.castling
            && self.ep_square == other.ep_square
            && self.gating == other.gating
            && self.duck == other.duck
            && self.placement == other.placement
            && self.halfmove_clock == other.halfmove_clock
            && self.fullmove_number == other.fullmove_number
            && self.consecutive_passes == other.consecutive_passes
            && self.board_b == other.board_b
    }
}

impl<G: Geometry> Eq for GenericState<G> {}

// Manual `Hash` (mirroring `GenericGating`): the `board_b` plane mask is hashed
// by its square indices so the impl is unconditional in `G::Bits`, keeping every
// generic user of `GenericState` (and the `Board`-free state hashing) free of a
// `G::Bits: Hash` bound. Every other field hashes directly.
impl<G: Geometry> core::hash::Hash for GenericState<G> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.turn.hash(state);
        self.castling.hash(state);
        self.ep_square.hash(state);
        self.gating.hash(state);
        self.duck.hash(state);
        self.placement.hash(state);
        self.halfmove_clock.hash(state);
        self.fullmove_number.hash(state);
        self.consecutive_passes.hash(state);
        for sq in self.board_b {
            sq.index().hash(state);
        }
    }
}

/// A generic chess position: a [`Board<G>`] plus its [`GenericState<G>`], driven
/// by the rule layer `V`.
///
/// `V` is a zero-sized [`WideVariant`] marker, so this monomorphises with no
/// runtime dispatch. See the [module docs](self) for the design.
#[derive(Clone)]
pub struct GenericPosition<G: Geometry, V: WideVariant<G>> {
    board: Board<G>,
    state: GenericState<G>,
    /// The crazyhouse **promoted mask**: the squares whose occupant reached the
    /// board by promotion, so that capturing one banks a Pawn (the "promoted
    /// pieces demote" rule). It is always [`Bitboard::EMPTY`] — and never read —
    /// for every variant whose [`WideVariant::demotes_promoted_captures`] is
    /// `false` (all but Capahouse), keeping their moves, state, and FEN
    /// byte-identical to a build without it. It follows make/unmake via the
    /// position [`Clone`] and rides the FEN as a trailing `~` on a promoted
    /// piece's token.
    promoted: Bitboard<G>,
    _variant: PhantomData<V>,
}

impl<G: Geometry, V: WideVariant<G>> core::fmt::Debug for GenericPosition<G, V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut ds = f.debug_struct("GenericPosition");
        ds.field("placement", &self.board.to_fen_placement())
            .field("state", &self.state);
        // The promoted mask is only meaningful for a crazyhouse-style variant; it
        // stays out of every other variant's Debug so they remain byte-identical.
        // Rendered as its set squares' indices (the geometry's `Bits` need not be
        // `Debug`).
        if V::demotes_promoted_captures() {
            let squares: Vec<u8> = self.promoted.into_iter().map(|s| s.index()).collect();
            ds.field("promoted", &squares);
        }
        ds.finish()
    }
}

/// The maximum number of distinct [`WideRole`] masks a single
/// [`apply`](GenericPosition::apply) can touch: the moving piece's role, a
/// captured piece's role, a promotion role, a castled rook (`Rook`), a Kyoto flip
/// role, a Jieqi reveal role, and a gated reserve / hand role — at most eight.
/// Recording each touched role mask's prior value, plus the two color masks, is
/// enough to rebuild the board on [`undo`] by direct assignment, with no
/// per-square role scan.
///
/// [`undo`]: GenericPosition::undo
const MAX_UNDO_ROLES: usize = 8;

/// The minimal record needed to reverse a single `apply`
/// **in place** — the unmake half of the geometry layer's make/unmake, returned by
/// [`apply_with_undo`](GenericPosition::apply_with_undo) and consumed by
/// [`undo`](GenericPosition::undo).
///
/// Rather than clone the whole [`GenericPosition`] (a board of
/// [`WideRole::COUNT`]-plus-two bitboards — over a kilobyte on the wide
/// geometries — plus the state) per node, an `Undo` captures only what a move
/// changes:
///
/// * the **board** is restored by **direct mask assignment**: a move only ever
///   edits the two color masks and the role masks of a bounded set of roles (the
///   mover, a capture victim, a promotion, a castled rook, a Kyoto flip / Jieqi
///   reveal, a gated piece — at most `MAX_UNDO_ROLES`). Snapshotting those masks
///   by index before the move and assigning them straight back restores the board
///   byte-for-byte **without any per-square [`role_at`](super::Board) scan** — that
///   scan (a linear walk of all [`WideRole::COUNT`] masks) is what makes a
///   square-by-square restore lose to a flat board copy, so the mask-level restore
///   is what lets make/unmake actually beat copy-make on the wide boards.
/// * the **non-board state** ([`GenericState`]: side, castling, en passant,
///   Seirawan gating reserves, Duck square, placement / hand pockets, clocks, the
///   Janggi pass counter, the Alice plane mask) and the crazyhouse **promoted
///   mask** are each a small `Copy` value captured whole. Snapshotting them
///   wholesale reverses every variant hook's state effect (hand banking, gate
///   consumption, plane transfer, …) with no per-field bookkeeping and no risk of
///   a missed delta.
///
/// An `Undo` is opaque and only meaningful when paired with the exact position
/// [`apply_with_undo`](GenericPosition::apply_with_undo) produced from it; see
/// [`undo`](GenericPosition::undo) for the contract. It is designed to be
/// extensible: incremental attacker / Zobrist state (issues #310, #311) can hook
/// additional restore data alongside these fields.
pub struct Undo<G: Geometry> {
    /// The non-board state exactly as it stood before the move.
    state: GenericState<G>,
    /// The crazyhouse promoted mask before the move (empty and unused for every
    /// non-demoting variant).
    promoted: Bitboard<G>,
    /// Both color occupancy masks before the move.
    by_color: [Bitboard<G>; 2],
    /// The role masks the move touches, each as a `(WideRole index, prior mask)`
    /// pair. Only the first `role_count` entries are populated; duplicate indices
    /// are harmless (assigning the same prior mask twice is idempotent).
    roles: [(usize, Bitboard<G>); MAX_UNDO_ROLES],
    /// The number of populated entries in `roles`.
    role_count: usize,
}

impl<G: Geometry> Undo<G> {
    /// Records `role`'s current mask (read from `board`) as one the move touched,
    /// so [`undo`](GenericPosition::undo) can restore it. Called by `apply` at the
    /// point it has the pre-move board in hand, reusing its own piece lookups so no
    /// extra [`role_at`](super::Board) scan is needed. Duplicate roles are harmless
    /// (the restore assigns the same prior mask); `MAX_UNDO_ROLES` bounds the count.
    #[inline]
    fn touch(&mut self, role: WideRole, board: &Board<G>) {
        self.roles[self.role_count] = (role.index(), board.by_role(role));
        self.role_count += 1;
    }
}

impl<G: Geometry> core::fmt::Debug for Undo<G> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Render without a `G::Bits: Debug` bound (mirroring `GenericState`): the
        // masks by their set-bit counts, the touched roles by index.
        let roles: Vec<(usize, u32)> = self.roles[..self.role_count]
            .iter()
            .map(|&(idx, mask)| (idx, mask.count()))
            .collect();
        f.debug_struct("Undo")
            .field("state", &self.state)
            .field("promoted", &self.promoted.count())
            .field(
                "by_color",
                &[self.by_color[0].count(), self.by_color[1].count()],
            )
            .field("roles", &roles)
            .finish()
    }
}

impl<G: Geometry, V: WideVariant<G>> GenericPosition<G, V> {
    /// Builds a position from a board and state directly.
    #[must_use]
    #[inline]
    pub fn from_parts(board: Board<G>, state: GenericState<G>) -> Self {
        GenericPosition {
            board,
            state,
            promoted: Bitboard::EMPTY,
            _variant: PhantomData,
        }
    }

    /// The starting position of the variant `V`.
    #[must_use]
    pub fn startpos() -> Self {
        let (board, state) = V::starting_position();
        Self::from_parts(board, state)
    }

    /// Returns a reference to the board.
    #[must_use]
    #[inline]
    pub fn board(&self) -> &Board<G> {
        &self.board
    }

    /// Returns the board dimensions `(width, height)` in `(files, ranks)`, read
    /// straight from this position's [`Geometry`] (`G::WIDTH`, `G::HEIGHT`).
    ///
    /// The value is fixed by the geometry type, so it is the same for every
    /// position of a given variant; it is exposed here so a runtime
    /// [`AnyWideVariant`](super::AnyWideVariant) — which erases the geometry —
    /// can still report a variant's board size without naming its type.
    #[must_use]
    #[inline]
    pub fn dimensions(&self) -> (u8, u8) {
        (G::WIDTH, G::HEIGHT)
    }

    /// Returns the side to move.
    #[must_use]
    #[inline]
    pub fn turn(&self) -> Color {
        self.state.turn
    }

    /// Returns the **Alice** board-membership mask: the set of occupied squares
    /// whose piece is on plane B (the second mirror board); a piece on a square
    /// not in the mask is on plane A. [`Bitboard::EMPTY`] for every non-Alice
    /// variant. See [`GenericState::board_b`].
    #[must_use]
    #[inline]
    pub fn board_b(&self) -> Bitboard<G> {
        self.state.board_b
    }

    /// Returns the castling rights.
    #[must_use]
    #[inline]
    pub fn castling(&self) -> GenericCastling {
        self.state.castling
    }

    /// Returns the en-passant target square, if any.
    #[must_use]
    #[inline]
    pub fn ep_square(&self) -> Option<Square<G>> {
        self.state.ep_square
    }

    /// Returns the halfmove clock.
    #[must_use]
    #[inline]
    pub fn halfmove_clock(&self) -> u16 {
        self.state.halfmove_clock
    }

    /// Returns the fullmove number.
    #[must_use]
    #[inline]
    pub fn fullmove_number(&self) -> u16 {
        self.state.fullmove_number
    }

    /// Returns a reference to the non-board [`GenericState`]: side to move,
    /// castling, en passant, Seirawan gating, the Duck square, the hand / setup
    /// pocket, the two clocks, the Janggi pass counter, and the Alice plane mask.
    ///
    /// The board is [`board`](Self::board) and the crazyhouse promoted mask is
    /// [`promoted`](Self::promoted); together with this state they are exactly what
    /// the compact binary wire codec ([`to_bytes`](Self::to_bytes)) round-trips.
    #[must_use]
    #[inline]
    pub fn state(&self) -> &GenericState<G> {
        &self.state
    }

    /// Returns the crazyhouse **promoted mask**: the squares whose occupant
    /// reached the board by promotion (so capturing one banks a Pawn).
    /// [`Bitboard::EMPTY`] for every variant whose
    /// [`WideVariant::demotes_promoted_captures`] is `false` (all but Capahouse).
    #[must_use]
    #[inline]
    pub fn promoted(&self) -> Bitboard<G> {
        self.promoted
    }

    /// Overwrites the crazyhouse promoted mask. Used by the binary wire decoder
    /// to restore a position's full state after [`from_parts`](Self::from_parts),
    /// which always starts the mask empty.
    #[inline]
    pub(crate) fn set_promoted(&mut self, mask: Bitboard<G>) {
        self.promoted = mask;
    }

    /// Returns the number of `role` pieces `color` holds **in hand** (a Shogi
    /// hand or a Sittuyin placement pocket); `0` for a variant with neither.
    #[must_use]
    #[inline]
    pub fn hand_count(&self, color: Color, role: WideRole) -> u8 {
        self.state.placement.count(color, role)
    }

    /// Injects one `role` piece into `color`'s **hand**, where it becomes
    /// droppable — the **Bughouse cross-board transfer**.
    ///
    /// Bughouse is a 2-board team game: a piece captured on one board is delivered
    /// to the capturer's partner on the **other** board. This library models a
    /// single board only ([`Bughouse`](crate::geometry::Bughouse)); the partner
    /// linkage is a server (mcs) concern. A server holding the two boards calls
    /// this method on the partner board to deliver the captured piece — reverted
    /// to its base role and flipped to the receiving side's `color` (a captured
    /// **promoted** piece is delivered as a [`WideRole::Pawn`], the crazyhouse
    /// demotion applied at the transfer site). The injected piece then drops like
    /// any crazyhouse hand piece.
    ///
    /// This is the value-adding counterpart of [`remove_from_hand`](Self::remove_from_hand)
    /// and reuses the same per-color, per-role hand store
    /// ([`hand_count`](Self::hand_count) reads it back). It is only meaningful for
    /// a variant with a hand ([`WideVariant::has_hand`]); for any other variant the
    /// hand is never consulted by move generation, so an injected piece is inert.
    #[inline]
    pub fn inject_into_hand(&mut self, color: Color, role: WideRole) {
        self.state.placement.add(color, role);
    }

    /// Removes one `role` piece from `color`'s **hand**, returning `true` if one
    /// was present (and `false`, leaving the hand unchanged, if it was empty).
    ///
    /// The inverse of [`inject_into_hand`](Self::inject_into_hand): a server uses
    /// it to reclaim a piece from a board's hand (e.g. when undoing a cross-board
    /// transfer, or reconciling the two boards' hands). A normal **drop** already
    /// consumes from the hand through move generation; this is the out-of-band
    /// transfer hook, the mirror of the injection.
    #[inline]
    pub fn remove_from_hand(&mut self, color: Color, role: WideRole) -> bool {
        if self.state.placement.count(color, role) == 0 {
            return false;
        }
        self.state.placement.take(color, role);
        true
    }

    // -- Attack queries ----------------------------------------------------

    /// Returns the set of `attacker` pieces that attack `sq` under `occupied`.
    ///
    /// Pawns attack diagonally; sliders are blocked by the occupancy. The king
    /// on `sq`, if relevant, should be removed from `occupied` by the caller so
    /// it does not shield itself.
    #[must_use]
    pub fn attackers_to(
        &self,
        sq: Square<G>,
        attacker: Color,
        occupied: Bitboard<G>,
    ) -> Bitboard<G> {
        let b = &self.board;
        let mut result = Bitboard::EMPTY;
        for role in WideRole::ALL {
            let pieces = b.pieces(attacker, role);
            if pieces.is_empty() {
                continue;
            }
            // A piece of `role` standing on a square in `pieces` attacks `sq`
            // iff `sq` is in that piece's attack set. For a pawn, "attacks" is
            // the diagonal pattern, which is symmetric under color flip: a
            // `attacker` pawn attacks `sq` iff the *opposing*-color pawn pattern
            // from `sq` lands on it. Every other role's attack set is symmetric
            // (a attacks b iff b attacks a under the same occupancy), so we can
            // project from `sq` with the inverse pawn color and the role's own
            // pattern otherwise.
            // A pawn's (and a Berolina Hoplite's) "attack" pattern is
            // direction-dependent, so to find the squares *from which* such a
            // piece of `attacker` hits `sq`, project the *opposing*-colour
            // pattern back from `sq`. Every other *symmetric* role's attack set is
            // symmetric (a attacks b iff b attacks a under the same occupancy).
            //
            // The Xiangqi Horse is the exception: its hobbling leg is adjacent to
            // the *horse* and points toward the leap, so reverse-projecting from
            // `sq` would test the wrong leg and miss real attacks. For such a
            // leg-asymmetric role, detect attackers the way the generator does —
            // project forward from each candidate origin and keep those that hit
            // `sq`.
            if V::role_attack_is_leg_asymmetric(role) {
                for from in pieces {
                    // A board-aware attacker (the Janggi cannon) projects from its
                    // own origin against the whole board; the default-off hook
                    // returns `None` for every other variant, keeping the
                    // occupancy-only projection byte-identical. This is a *threat*
                    // query, so it takes the board-aware **threat** set
                    // (`role_threats_board`, default = `role_attacks_board`), which
                    // for Empire excludes the move-only quiet Queen slides that are
                    // not attacks (issue #359).
                    let att = if V::uses_board_attacks() {
                        V::role_threats_board(role, attacker, from, b)
                            .unwrap_or_else(|| V::role_attacks(role, attacker, from, occupied))
                    } else {
                        V::role_attacks(role, attacker, from, occupied)
                    };
                    if att.contains(sq) {
                        result |= Bitboard::from_square(from);
                    }
                }
                continue;
            }
            let from_sq = if V::role_attack_is_directional(role) {
                V::role_attacks(role, attacker.opposite(), sq, occupied)
            } else {
                V::role_attacks(role, attacker, sq, occupied)
            };
            result |= from_sq & pieces;
        }
        result
    }

    /// Returns `true` if `sq` is attacked by any piece of color `by`.
    #[must_use]
    #[inline]
    pub fn is_attacked(&self, sq: Square<G>, by: Color) -> bool {
        !self.attackers_to(sq, by, self.board.occupied()).is_empty()
    }

    /// Returns `true` if the royal square `sq` is attacked by color `by` under
    /// `occupied`, **including** any variant-specific extra royal attack.
    ///
    /// This is the per-role [`attackers_to`](Self::attackers_to) test ORed with
    /// the default-off [`WideVariant::extra_royal_attack`] hook — the Xiangqi
    /// flying-general confrontation (the two generals facing down an open file).
    /// For every variant without that hook (`has_flying_general() == false`) the
    /// extra term is skipped and this is exactly `attackers_to(...).is_empty()`
    /// negated, so those variants are byte-identical.
    #[must_use]
    #[inline]
    fn royal_attacked(&self, sq: Square<G>, by: Color, occupied: Bitboard<G>) -> bool {
        if !self.attackers_to(sq, by, occupied).is_empty() {
            return true;
        }
        V::has_flying_general() && V::extra_royal_attack(&self.board, sq, by, occupied)
    }

    /// Returns `true` if the side to move is in check.
    ///
    /// For a single-royal side (standard chess and every variant with one king)
    /// this is "the king's square is attacked." For a **multi-king** variant
    /// (Spartan, [`WideVariant::multi_royal`]) a side is in check only under
    /// **duple check** — when *every* royal king is attacked at once — since it
    /// may otherwise leave a king en prise and play on; this mirrors the
    /// multi-royal legality the move generator enforces. A side with no royal
    /// squares (Duck) is never in check.
    #[must_use]
    #[inline]
    pub fn is_check(&self) -> bool {
        self.is_in_check(self.state.turn)
    }

    /// Returns `true` if `color`'s king(s) are in check right now, regardless of
    /// whose turn it is.
    ///
    /// This is the color-parameterized form of [`is_check`](Self::is_check) (which
    /// is exactly `is_in_check(turn)`): it applies the identical, variant-correct
    /// royal-attack discipline to an arbitrary side. For a single-royal side it is
    /// "a king square is attacked"; for a multi-king duple-check variant (Spartan)
    /// it is "*every* royal is attacked"; a royalless side (Duck) is never in
    /// check. Because a legal position never leaves the side that just moved in
    /// check, `is_in_check(turn.opposite())` is always `false` after legal play.
    #[must_use]
    pub fn is_in_check(&self, color: Color) -> bool {
        let us = color;
        // Alice chess: a king is attacked only by enemy pieces on the **same
        // plane**, so check is plane-restricted. Default-off, so every other
        // variant takes the standard occupancy-wide test below.
        if V::is_alice() {
            return !self.alice_king_safe(us);
        }
        // Count-thresholded pseudo-royalty (Sho Shogi): while a side holds more
        // than one royal (King + Crown Prince) neither is royal, so it is never in
        // check. Default-on (`true`) for every other variant, so this is inert.
        if V::multi_royal() && !V::royal_constraint_active(&self.board, us) {
            return false;
        }
        let them = us.opposite();
        let occ = self.board.occupied();
        let royals = V::royal_squares(&self.board, us);
        if royals.is_empty() {
            return false;
        }
        let attacked = |sq| self.royal_attacked(sq, them, occ);
        if V::multi_royal() && !V::royals_all_must_survive() {
            // Spartan duple check: in check only when no royal is left unattacked.
            royals.into_iter().all(attacked)
        } else {
            // Single-royal, and Chak's all-royals-must-survive rule: in check when
            // any royal is attacked.
            royals.into_iter().any(attacked)
        }
    }

    /// Returns the squares attacked by color `by` under `occupied` — the
    /// king-danger map (the squares the other king may not step onto). Pawns use
    /// their diagonal attack pattern.
    ///
    /// `pub(crate)` so the Fog of War view helper
    /// ([`FogOfWar::visible_squares`](crate::geometry::FogOfWar::visible_squares))
    /// can reuse it to compute per-player visibility.
    pub(crate) fn attacked_by(&self, by: Color, occupied: Bitboard<G>) -> Bitboard<G> {
        let b = &self.board;
        let mut attacked = Bitboard::EMPTY;
        for role in WideRole::ALL {
            for from in b.pieces(by, role) {
                attacked |= V::role_attacks(role, by, from, occupied);
            }
        }
        attacked
    }

    // -- Pins --------------------------------------------------------------

    /// Computes the pinned friendly pieces of the side to move and, for each,
    /// the line through the king it is confined to.
    ///
    /// A pinned piece sits alone on a ray between its king and an enemy slider.
    /// Because fairy compounds (Hawk = B+N, Elephant = R+N) include a sliding
    /// component, any enemy slider whose ray family includes the king-line can
    /// pin. We treat every enemy slider's relevant ray as a candidate pinner.
    fn compute_pins(
        &self,
        king_sq: Square<G>,
        us: Color,
        them: Color,
        occupied: Bitboard<G>,
    ) -> Pins<G> {
        let b = &self.board;
        let mut pins = Pins::empty(king_sq);
        let our_pieces = b.by_color(us);

        // For each enemy sliding piece that lines up with the king on a rank,
        // file, or diagonal, test the in-between squares for exactly one
        // friendly blocker. The slider must actually attack along that line
        // (a rook does not pin on a diagonal, a bishop not on a file); we check
        // by asking whether the slider, on an *empty* board, would reach the
        // king square — i.e. the line is in its movement geometry.
        for role in WideRole::ALL {
            if !V::role_is_slider(role) {
                continue;
            }
            for slider in b.pieces(them, role) {
                let l = line(king_sq, slider);
                if l.is_empty() {
                    continue;
                }
                // The slider must be able to slide along this line (rook can't
                // pin on a diagonal). Ask its empty-board attack set.
                let empty_attacks = V::role_attacks(role, them, slider, Bitboard::EMPTY);
                if !empty_attacks.contains(king_sq) {
                    continue;
                }
                let blockers = between(king_sq, slider) & occupied;
                if blockers.count() == 1 {
                    let pinned = blockers.lsb().expect("one blocker");
                    if our_pieces.contains(pinned) {
                        // By default, confine the pinned piece to the full
                        // king–slider `line` (byte-identical to every existing
                        // variant). Variants that opt in via
                        // `confine_pins_to_segment` instead restrict it to the
                        // segment between the king and the pinner (inclusive of
                        // the pinner's square). The two are equivalent for a
                        // slider — the king blocks the far side and the pinner
                        // blocks beyond — but differ for a *leaper* (e.g. the
                        // Courier Alfil) that can jump over its own king onto a
                        // collinear square past it, which the full line would
                        // wrongly permit.
                        let mask = if V::confine_pins_to_segment() {
                            between(king_sq, slider) | Bitboard::from_square(slider)
                        } else {
                            l
                        };
                        pins.add(pinned, mask);
                    }
                }
            }
        }
        pins
    }

    /// The **absolutely pinned** friendly pieces of `color`: pieces that sit
    /// alone on a ray between one of `color`'s royal squares and an enemy slider,
    /// so moving them off that ray would expose the king.
    ///
    /// This is the exact pin relation the move generator itself uses (via the same
    /// `compute_pins`), unioned over every royal square of `color` — so it is
    /// variant-correct for the fairy compounds (a Hawk = B+N or Elephant = R+N
    /// pins along its sliding component) and honours the per-variant pin
    /// confinement (see [`WideVariant::confine_pins_to_segment`]). It reports no
    /// pins for a side with no royal square (Duck). A pinned piece may still have
    /// legal moves *along* its ray; use [`pin_ray_of`](Self::pin_ray_of) for that
    /// line and [`legal_moves_from`](Self::legal_moves_from) for the moves that
    /// survive the pin.
    #[must_use]
    pub fn pinned_pieces(&self, color: Color) -> Bitboard<G> {
        let them = color.opposite();
        let occ = self.board.occupied();
        let mut acc = Bitboard::EMPTY;
        for king in V::royal_squares(&self.board, color) {
            acc |= self.compute_pins(king, color, them, occ).pinned;
        }
        acc
    }

    /// The line a pinned piece of `color` on `square` is confined to — the ray
    /// through the pinning royal that the piece may move along without exposing
    /// the king — or `None` if `square` does not hold an absolutely pinned piece
    /// of `color`.
    ///
    /// The returned line is exactly the confinement mask the generator restricts
    /// the piece to (the full king–slider line, or, for a variant that opts into
    /// [`WideVariant::confine_pins_to_segment`], the king-to-pinner segment). When
    /// several royals pin the same piece the first pinning royal's line is
    /// reported.
    #[must_use]
    pub fn pin_ray_of(&self, color: Color, square: Square<G>) -> Option<Bitboard<G>> {
        if self.board.color_at(square) != Some(color) {
            return None;
        }
        let them = color.opposite();
        let occ = self.board.occupied();
        for king in V::royal_squares(&self.board, color) {
            let pins = self.compute_pins(king, color, them, occ);
            if pins.pinned.contains(square) {
                return Some(pins.line_of(square));
            }
        }
        None
    }

    // -- Move generation ---------------------------------------------------

    /// Generates every legal move for the side to move.
    #[must_use]
    pub fn legal_moves(&self) -> Vec<WideMove> {
        let mut out = Vec::new();
        self.generate_into(&mut out);
        out
    }

    /// Returns the number of legal moves (perft depth-1 leaf count).
    ///
    /// On the **standard single-king path** (every variant whose legality is the
    /// default king-safety discipline: standard chess, Makruk, Capablanca, Grand,
    /// …) this counts the legal moves *without materialising them* — the same bulk
    /// leaf-count the concrete engine uses at a perft leaf, where each per-target
    /// loop collapses to a population count. The result is exactly
    /// `legal_moves().len()` because the standard generator only ever pushes legal
    /// moves (no post-filter). The variant paths that filter pseudo-legal moves
    /// (multi-royal, cannon) or read the buffer back (Seirawan gating) cannot be
    /// bulk-counted soundly, so they materialise into a reusable list and return
    /// its length — byte-identical to before.
    #[must_use]
    pub fn legal_move_count(&self) -> usize {
        if self.uses_standard_path() {
            let mut sink = WideCountSink::default();
            self.generate_standard_into(&mut sink);
            sink.count() as usize
        } else {
            let mut list = WideMoveList::new();
            self.generate_special_into(&mut list);
            list.len()
        }
    }

    /// Returns `true` if this position takes the **standard single-king
    /// legality fast path** — the path on which every pushed move is already
    /// legal, so the move set may be bulk-counted without materialising it.
    ///
    /// This is the negation of every default-off variant hook that diverts
    /// [`generate_into`](Self::generate_into) to a special generator (duck,
    /// active placement phase, multi-royal, cannon) or that adds moves a count
    /// sink cannot tally by population count (Seirawan gating, variant drops).
    /// For standard chess and every standard-king-safety variant it is `true`.
    #[inline]
    fn uses_standard_path(&self) -> bool {
        let special = V::has_duck()
            || V::is_alice()
            || V::multi_royal()
            || V::has_cannons()
            || V::has_flying_general()
            || V::supports_gating()
            || V::has_hand()
            || (V::has_placement() && self.state.placement.any(self.state.turn));
        !special
    }

    /// Drives the generator into `out` for a variant **not** on the standard
    /// path. Mirrors [`generate_into`](Self::generate_into)'s dispatch but is
    /// generic over the destination buffer (`Vec<WideMove>` or [`WideMoveList`]),
    /// so the reusable-buffer perft recursion can reuse one allocation across
    /// sibling nodes for these variants too. Never reached on the standard path.
    fn generate_special_into<S: WideSink>(&self, out: &mut S) {
        if V::is_alice() {
            self.generate_alice_into(out);
        } else if V::has_duck() {
            self.generate_duck_into(out);
        } else if V::has_placement() && self.state.placement.any(self.state.turn) {
            self.generate_placement_into(out);
        } else if V::multi_royal() {
            self.generate_multi_royal_into(out);
        } else if V::has_cannons() || V::has_flying_general() {
            self.generate_cannon_verify_into(out);
        } else {
            // The standard path with the gating / drop addenda a count sink
            // cannot tally; materialise it through the same body the public
            // `generate_into` uses.
            self.generate_standard_into(out);
        }
    }

    /// Pushes every legal move into `out`.
    fn generate_into(&self, out: &mut Vec<WideMove>) {
        // Alice chess: two mirror boards with per-piece plane membership. Movement,
        // capture, blocking, and king-safety are all plane-restricted and every
        // move transfers the mover to the opposite plane, so it has its own
        // generator. Gated behind `is_alice()` (default-off), so every other
        // variant takes the standard path below unchanged.
        if V::is_alice() {
            self.generate_alice_into(out);
            return;
        }
        // Duck chess has its own generator: there is no check, so no pin /
        // king-danger filtering, and every base piece move is crossed with every
        // legal duck placement. Gated behind `has_duck()` (default off), so every
        // other variant takes the standard path below unchanged.
        if V::has_duck() {
            self.generate_duck_into(out);
            return;
        }
        // Setup / placement phase (Sittuyin): while the side to move still has
        // pieces in hand it makes a placement drop — never a board move — onto its
        // own territory. The phase is per-side (a side that has emptied its pocket
        // plays normally even while the opponent is still deploying), and FSF
        // applies no check filtering to placement drops. Gated behind
        // `has_placement()` (default-off), so every other variant takes the
        // standard path below unchanged.
        if V::has_placement() && self.state.placement.any(self.state.turn) {
            self.generate_placement_into(out);
            return;
        }
        // Multi-king variants (Spartan): a side can hold several royal kings, so
        // "in check" generalises to a set of royal squares and the single-king
        // legality fast path (one king, one check mask, one pin set) no longer
        // applies. Generate pseudo-legal moves and keep those that leave at least
        // one king unattacked. Gated behind `multi_royal()` (default-off), so
        // every other variant takes the standard single-king path below unchanged.
        if V::multi_royal() {
            self.generate_multi_royal_into(out);
            return;
        }
        // Cannon variants (Shako): a cannon's check and king-danger are
        // screen-dependent, so the mask-based single-king fast path (lifted-king
        // danger map, `between` interpose) is unsound — a king sliding along a
        // cannon ray, or a move that adds/removes a screen, can leave the king en
        // prise (or wrongly forbid a safe square). Generate pseudo-legal moves and
        // verify each against the actual post-move occupancy. Gated behind
        // `has_cannons()` (default-off), so every other variant takes the standard
        // path below unchanged.
        // Cannon variants and the flying-general / flag-win variants (Synochess)
        // share this per-move verify path: a flying-general confrontation is a
        // king-danger source the mask path never computes, and the flag-rank
        // ("campmate") rule forbids a king move that the standard generator would
        // otherwise emit. Gated behind the default-off hooks, so every other
        // variant takes the standard path below unchanged.
        if V::has_cannons() || V::has_flying_general() {
            self.generate_cannon_verify_into(out);
            return;
        }
        self.generate_standard_into(out);
    }

    /// The **standard single-king** move generator, generic over the destination
    /// [`WideSink`] (`Vec<WideMove>` / [`WideMoveList`] to materialise, or
    /// [`WideCountSink`] to bulk-count at a perft leaf).
    ///
    /// Every move this pushes is already legal — the king-danger map, check mask,
    /// and pin lines confine each piece's targets before they are emitted, so the
    /// list never needs a post-filter. That invariant is what makes the bulk count
    /// sound: through a [`WideCountSink`] each `emit_targets` collapses to a
    /// population count, yielding exactly `legal_moves().len()` without building a
    /// single move. It is reached only on the standard path (see
    /// [`uses_standard_path`](Self::uses_standard_path)); the gating / drop addenda
    /// at the tail are inert there and only fire on the materialising fallthrough.
    fn generate_standard_into<S: WideSink>(&self, out: &mut S) {
        let us = self.state.turn;
        let them = us.opposite();
        let board = &self.board;

        // Flag-win / campmate (Orda's "flag", Synochess's "campmate" on the
        // standard path): if the opponent's king has already reached its goal rank,
        // the side to move has lost and the node is terminal — no moves. Gated
        // behind `has_flag_win()` (default-off), so every other variant skips the
        // check and is byte-identical. This is the single chokepoint both the
        // materialising generator and the bulk-count leaf path funnel through, so a
        // flag win terminates perft descent exactly as Fairy-Stockfish does.
        // `flag_win_reached(opp)` ≡ "the opponent's king is on its goal rank."
        if V::has_flag_win() && self.flag_win_terminal(us) {
            return;
        }

        // Bare-king "Robado" draw (Shatar): if either side has been stripped to
        // its lone king, the game is already an immediate draw and the node is
        // terminal — no moves. Gated behind `has_bare_king_draw()` (default-off),
        // so every other variant skips the check and is byte-identical. This is
        // the single chokepoint both the materialising generator and the
        // bulk-count leaf path funnel through, so the draw truncates perft descent
        // exactly as Fairy-Stockfish's extinction rule does.
        if V::has_bare_king_draw() && self.bare_king_present() {
            return;
        }

        // Bare-king baring loss (Shatranj): a side reduced to its lone king (with
        // its bare-back chance spent or impossible) has lost, so the node is a
        // terminal perft leaf with no moves. Gated behind `has_bare_king_loss()`
        // (default-off), so every other variant skips the check and is
        // byte-identical. The single `bare_king_loss_loser` chokepoint truncates
        // perft descent exactly as Fairy-Stockfish's extinction claim does.
        if V::has_bare_king_loss() && self.bare_king_loss_loser().is_some() {
            return;
        }

        // Check-win (Checkshogi): if the side to move is in check, the opponent's
        // last move gave check and immediately won — the node is terminal with no
        // moves, a perft leaf exactly as Fairy-Stockfish truncates it. Gated behind
        // `wins_on_check()` (default-off), so every other variant skips the check
        // and is byte-identical. This is the single chokepoint both the
        // materialising generator and the bulk-count leaf path funnel through.
        if V::wins_on_check() && self.is_check() {
            return;
        }

        let occupied = board.occupied();
        let our_pieces = board.by_color(us);
        let their_pieces = board.by_color(them);

        let king_sq = match board.king_of(us) {
            Some(sq) => sq,
            None => return,
        };

        // A **non-royal** king (Dobutsu's Lion): there is no check, so the king
        // never has a check mask, pins, or king-danger filter — it may step into an
        // attacked square, and any piece may move freely (a side loses only by
        // extinction or the opponent's try, both handled as terminals). The "king"
        // here is the [`WideRole::King`]-role Lion; treat it as an ordinary piece
        // with no safety filtering. Gated behind `non_royal_king()` (default-off),
        // so every royal-king variant keeps the exact check/pin/king-danger path.
        let non_royal = V::non_royal_king();

        let checkers = if non_royal {
            Bitboard::EMPTY
        } else {
            self.attackers_to(king_sq, them, occupied)
        };
        let num_checkers = checkers.count();

        // King-danger: squares attacked by the enemy with our king lifted out
        // of the occupancy, so it cannot shield itself along a slider ray. A
        // non-royal king has no danger filter.
        let king_danger = if non_royal {
            Bitboard::EMPTY
        } else {
            let occ_without_king = occupied.without(king_sq);
            self.attacked_by(them, occ_without_king)
        };

        // King moves are always generated (the only legal moves under double
        // check).
        let mut king_targets =
            V::role_attacks(WideRole::King, us, king_sq, occupied) & !our_pieces & !king_danger;
        // Makpong: while in check, the king may move ONLY to capture the lone
        // checker — it may not flee to a safe square. Under double check there is
        // no single checker the king could capture, so it has no legal move; the
        // target set is emptied. Default-off, so every other variant is
        // byte-identical (the king-target set is left exactly as generated above).
        if V::king_may_only_capture_checker() && num_checkers > 0 {
            king_targets &= if num_checkers == 1 {
                checkers
            } else {
                Bitboard::EMPTY
            };
        }
        out.emit_targets(king_sq, king_targets, their_pieces);

        if num_checkers >= 2 {
            // Double check: only king moves are legal.
            self.append_drops(out);
            // A king that flees a double check off a gating-eligible square may
            // still gate a reserve onto the square it vacates: the gate rides the
            // (already-legal) king escape and can neither expose nor shield our
            // king, exactly as on the single-check / quiet tail below. Without
            // this the gated double-check escape is dropped (issue #363: S-House
            // discovered double checks under-counted the king's `+gate` reply).
            // Gated behind `supports_gating()` (default-off), so every non-gating
            // variant is byte-identical (its eligible set is empty regardless).
            if V::supports_gating() {
                self.append_gating_moves(out, us);
            }
            return;
        }

        // The mask of squares that resolve a single check: capture the checker
        // or block between it and the king. Full board when not in check.
        //
        // Interposition resolves the check **only when the checker is a slider** —
        // a leaper cannot be blocked. Most leapers (knight, ferz, ...) check from a
        // square not collinear with the king, so `between` is already empty for
        // them; but a leaper that jumps **along a line** — the Shatranj Alfil's
        // two-square diagonal jump (and the Shako Fers-Alfil, the Dabbaba, ...) —
        // checks from a collinear square with a real intervening square, and
        // `between` would offer that square as a (false) block. Gating on
        // [`role_is_slider`](WideVariant::role_is_slider) drops the bogus
        // interposition. This is byte-identical for every existing variant: the
        // pre-hook leaper checkers were never collinear (so `between` was empty
        // anyway), and the slider checkers are unchanged.
        let check_mask = if num_checkers == 1 {
            let checker = checkers.lsb().expect("one checker");
            let checker_is_slider = self.board.role_at(checker).is_some_and(V::role_is_slider);
            let interpose = if checker_is_slider {
                between(king_sq, checker)
            } else {
                Bitboard::EMPTY
            };
            checkers | interpose
        } else {
            Bitboard::FULL
        };

        let pins = if non_royal {
            Pins::empty(king_sq)
        } else {
            self.compute_pins(king_sq, us, them, occupied)
        };

        // Every non-king, non-pawn role: its attack set minus friendly pieces,
        // confined by the check mask and (if pinned) its pin line. Roles the
        // variant does not field have an empty piece mask, so their loop body
        // never runs — the `is_empty` guard skips the per-role `role_attacks`
        // dispatch and (for a count sink) keeps the inner loop branch-light.
        // A hand variant (Shogi) routes its Pawn through this generic piece loop
        // as a forward stepper — its `role_attacks` is the single forward square,
        // which serves as both its quiet push and its forward capture (a Shogi
        // pawn captures straight ahead, not diagonally) — and skips the diagonal-
        // capture `gen_pawn_moves` path below. Every other variant keeps the Pawn
        // on the dedicated pawn generator, byte-identically.
        let pawn_is_stepper = V::pawn_is_stepper();
        for role in WideRole::ALL {
            if role == WideRole::King || (role == WideRole::Pawn && !pawn_is_stepper) {
                continue;
            }
            let pieces = board.pieces(us, role);
            if pieces.is_empty() {
                continue;
            }
            // Whether this role expands into promote / non-promote variants per
            // target: a hand variant's promotable piece (Shogi), or a no-hand
            // piece-promotion variant's promotable mover (Khan's Chess's
            // KhanSoldier, which promotes to a Khan on the last rank). Both gates
            // are default-off, so every other role / variant is byte-identical.
            let promotable =
                (V::has_hand() || V::has_piece_promotion()) && V::role_can_promote(role);
            // Capture-only roles (Orda Lancer / Archer): their `role_attacks` set
            // (rook / bishop slide) may be reached only by capturing — never as a
            // quiet move. Their quiet moves come solely from `quiet_only_targets`
            // (the knight pattern). Default-off, so inert and byte-identical for
            // every other role / variant; the attack relation is unaffected.
            let capture_only = V::role_attacks_are_capture_only(role);
            for from in pieces {
                let pin_line = pins.line_of(from);
                let mut targets =
                    V::role_attacks(role, us, from, occupied) & !our_pieces & check_mask & pin_line;
                if capture_only {
                    targets &= their_pieces;
                }
                if promotable {
                    self.emit_promotable_targets(out, role, from, targets, their_pieces, us);
                    // A hand variant's promotable pieces draw every move from
                    // `role_attacks` (their `quiet_only_targets` is empty), so they
                    // skip the quiet-only pass exactly as before — byte-identical.
                    if V::has_hand() {
                        continue;
                    }
                    // A no-hand piece-promotion variant (Khan's KhanSoldier) draws
                    // its quiet leaps from `quiet_only_targets` and must emit them
                    // here, promoting those that end in the promotion zone (to
                    // `role_promoted_to`). Confined to empty squares and the same
                    // check / pin masks as a normal move.
                    let quiet_only = V::quiet_only_targets(role, us, from, occupied)
                        & !occupied
                        & check_mask
                        & pin_line;
                    let from_in_zone = V::in_promotion_zone(us, from.rank());
                    for to in quiet_only {
                        if from_in_zone || V::in_promotion_zone(us, to.rank()) {
                            Self::emit_piece_promotion_one(out, role, from, to, false, us);
                        } else {
                            out.push(WideMove::new(from, to, WideMoveKind::Quiet));
                        }
                    }
                    continue;
                }
                out.emit_targets(from, targets, their_pieces);
                // Quiet-only steps: squares a piece may move to but never capture
                // on — the cannon's empty rook-rays (its captures are the
                // over-screen `role_attacks` set above), and the Spartan
                // Lieutenant's sideways slide. Default-empty, so inert and
                // byte-identical for every other role and variant. Confined to
                // empty squares and the same check / pin masks as a normal move.
                let quiet_only = V::quiet_only_targets(role, us, from, occupied)
                    & !occupied
                    & check_mask
                    & pin_line;
                for to in quiet_only {
                    out.push(WideMove::new(from, to, WideMoveKind::Quiet));
                }
            }
        }

        // Pawns: pushes, double pushes, captures, en passant, promotions. A hand
        // variant (Shogi) handled its forward-stepping Pawn in the piece loop
        // above, so the diagonal-capture pawn generator is skipped for it.
        if !pawn_is_stepper {
            self.gen_pawn_moves(out, us, occupied, their_pieces, check_mask, &pins, king_sq);
        }

        // Castling, only when not in check.
        if V::has_castling() && num_checkers == 0 {
            self.gen_castles(out, us, occupied, king_danger, king_sq);
        }

        // One-time first-move leaps (Cambodian): the king's forward-knight leap
        // and the queen/Met's two-square advance, each offered only while its
        // home-square piece still holds its leap right. The king leap (like
        // castling) is offered only when not in check; the Met leap is an ordinary
        // piece move confined by the check mask and its pin line. Default-off, so
        // every other variant skips this entirely and is byte-identical.
        if V::has_first_move_leaps() {
            self.gen_first_move_leaps(
                out,
                us,
                occupied,
                our_pieces,
                king_danger,
                king_sq,
                num_checkers,
                check_mask,
                &pins,
            );
        }

        self.append_drops(out);

        // Seirawan gating: a back-rank piece's first move (and the king/rook of a
        // castle) may optionally gate a reserve onto the vacated square. Each base
        // move generated above is independently legal, so the gate — a friendly
        // piece dropped on an evacuated square — never changes legality (it can
        // neither expose nor shield our own king). Default-off: skipped entirely
        // unless the variant supports gating.
        if V::supports_gating() {
            self.append_gating_moves(out, us);
        }
    }

    /// Generates every legal Duck-chess move: each pseudo-legal base piece move
    /// crossed with every legal duck placement (`docs/fairy-variants-architecture.md`
    /// §4.4).
    ///
    /// Duck chess has **no check**: the king is not royal, so there is no pin,
    /// king-danger, or self-check filtering — every base move whose landing
    /// square is empty-of-friends-and-not-the-duck is legal (including capturing
    /// the enemy king, which is the win). After the base move, the **Duck** is
    /// moved to any square that is then empty and different from where it sits
    /// now, forming the second half of the one ply.
    fn generate_duck_into<S: WideSink>(&self, out: &mut S) {
        let us = self.state.turn;
        let board = &self.board;
        // If the side to move has no king, its king was captured on the previous
        // ply: the game is already over and there are no further moves. (FSF lists
        // a king capture as a legal move whose resulting node is terminal.)
        if board.king_of(us).is_none() {
            return;
        }
        let duck = self.state.duck;
        // The duck blocks every piece: it is part of the occupancy for sliders and
        // steppers (knights jump over it, since their attack set ignores
        // occupancy). It is neither side's piece, so it is never a capture target.
        let piece_occ = board.occupied();
        let occupied = match duck {
            Some(d) => piece_occ.with(d),
            None => piece_occ,
        };
        let our_pieces = board.by_color(us);
        let their_pieces = board.by_color(us.opposite());

        // Collect the pseudo-legal base moves first (no check filtering).
        let mut base = Vec::new();
        self.gen_duck_base_moves(&mut base, us, occupied, our_pieces, their_pieces);

        // Cross each base move with every legal duck placement.
        for mv in base {
            // The board occupancy after the base move: the piece leaves `from`
            // and lands on `to` (a capture removes the enemy already counted in
            // `piece_occ`). An en-passant also frees the captured pawn's square.
            let from = mv.from::<G>();
            let to = mv.to::<G>();
            let mut after = piece_occ.without(from).with(to);
            if matches!(mv.kind(), WideMoveKind::EnPassant) {
                if let Some(captured) = Square::<G>::from_file_rank(to.file(), from.rank()) {
                    after = after.without(captured);
                }
            }
            // Castling also moves the rook; reflect both squares so the duck never
            // lands on the rook's destination.
            if mv.is_castle() {
                let rank = V::castle_rank(us);
                let side = if matches!(mv.kind(), WideMoveKind::CastleKingside) {
                    KINGSIDE
                } else {
                    QUEENSIDE
                };
                if let Some(rook_file) = self.state.castling.rook_file(us, side) {
                    if let Some(rook_from) = Square::<G>::from_file_rank(rook_file, rank) {
                        after = after.without(rook_from);
                    }
                }
                let (_k, rook_dest_file) = V::castle_dest_files(side);
                if let Some(rook_to) = Square::<G>::from_file_rank(rook_dest_file, rank) {
                    after = after.with(rook_to);
                }
            }
            // Duck destinations: every square empty after the base move, except the
            // duck's current square (it must move to a *different* square).
            let mut duck_targets = !after;
            if let Some(d) = duck {
                duck_targets = duck_targets.without(d);
            }
            for dsq in duck_targets {
                out.push(mv.with_duck::<G>(dsq));
            }
        }
    }

    /// Pushes the pseudo-legal base piece moves for Duck chess (no check / pin
    /// filtering) into `base`. The duck already sits in `occupied`, blocking
    /// landings and slider rays.
    fn gen_duck_base_moves(
        &self,
        base: &mut Vec<WideMove>,
        us: Color,
        occupied: Bitboard<G>,
        our_pieces: Bitboard<G>,
        their_pieces: Bitboard<G>,
    ) {
        let board = &self.board;
        // No piece may land on the duck. The duck is in `occupied` (so it blocks
        // slider rays), but a stepper's attack set is occupancy-independent — a
        // knight or king reaching the duck's square must still be excluded
        // explicitly, since the duck is in neither color mask.
        let not_duck = match self.state.duck {
            Some(d) => !Bitboard::<G>::from_square(d),
            None => Bitboard::FULL,
        };
        // Non-pawn pieces (including the king): attack set minus friendly pieces
        // and minus the duck square. The duck is in neither color mask so it is
        // never a friendly piece nor a capture target.
        for role in WideRole::ALL {
            if role == WideRole::Pawn {
                continue;
            }
            for from in board.pieces(us, role) {
                let targets = V::role_attacks(role, us, from, occupied) & !our_pieces & not_duck;
                base.emit_targets(from, targets, their_pieces);
            }
        }

        // Pawns: pushes, double pushes, captures, en passant, promotions — with no
        // pin/check confinement (full board mask, no pin lines).
        let full = Bitboard::FULL;
        let pins = Pins::empty(board.king_of(us).unwrap_or_else(|| Square::new(0)));
        let king_sq = board.king_of(us).unwrap_or_else(|| Square::new(0));
        self.gen_pawn_moves(base, us, occupied, their_pieces, full, &pins, king_sq);

        // Castling: in duck chess there is no check, but FSF still forbids
        // castling through, out of, or into the duck's blocking squares and keeps
        // the empty-path requirement. King-safety (danger) is irrelevant with no
        // check, so pass an empty danger set.
        if V::has_castling() {
            self.gen_castles(base, us, occupied, Bitboard::EMPTY, king_sq);
        }
    }

    /// Generates every legal move for a **multi-king** side (Spartan): each
    /// pseudo-legal base move, kept only if it leaves at least one of the side's
    /// kings unattacked (`docs/fairy-variants-architecture.md` §4.4).
    ///
    /// A side with several kings is "in check" only when **every** king is
    /// attacked at once (duple check, for two kings); otherwise it is free to
    /// move — even to leave a king en prise, sacrificing it and continuing with
    /// the survivor. So legality is exactly "not all my kings are attacked after
    /// the move," which the [`Self::royals_survive_after`] predicate tests on the
    /// applied position. This unifies both colours: white (one king) reduces to
    /// the standard "king not in check," black (two kings) to "not duple check."
    ///
    /// Hot-path shape (issue #183), mirroring the cannon path (#194): the
    /// per-move verify uses **make/unmake** on a `Copy` board+state snapshot of
    /// one reused scratch position rather than cloning the whole
    /// [`GenericPosition`] per move, the king-attack test scans only the enemy
    /// roles **actually fielded** (precomputed once per node) instead of all
    /// [`WideRole::COUNT`] of them, and a **fast-accept geometry filter** skips
    /// the make/unmake+scan for a non-king move that provably cannot change any
    /// royal's safety. The produced move set is byte-identical.
    fn generate_multi_royal_into<S: WideSink>(&self, out: &mut S) {
        let us = self.state.turn;
        // The temple-win terminal (Chak): if the opponent's Divine Lord already
        // stands on its goal temple square, the opponent has won and the side to
        // move has no legal continuation — the node is a perft leaf, exactly as
        // Fairy-Stockfish truncates it. Gated behind `has_temple_win()`
        // (default-off), so every other multi-royal variant (Spartan) is
        // byte-identical.
        if V::has_temple_win() && self.temple_win_reached(us.opposite()) {
            return;
        }
        // The side's royal squares — its kings, plus any extra royal piece type the
        // variant declares (Chak's Divine Lord). For every existing variant
        // `royal_squares` is exactly `kings_of`, so this is byte-identical.
        let kings = V::royal_squares(&self.board, us);
        // A side with no royal pieces has normally already lost (its last royal was
        // captured on the previous ply): no moves. Xiang Fu's pseudo-royal extinction
        // (`royalless_generates()`) is the exception — FSF keeps generating the moves
        // of a side that has lost both Champions (with no pseudo-royal pieces left
        // there is no king-safety constraint, so every pseudo-legal move and drop is
        // legal), so perft must descend that node too.
        if kings.is_empty() {
            if V::royalless_generates() {
                let mut pseudo = WideMoveList::new();
                self.gen_multi_royal_pseudo(&mut pseudo, us);
                if V::has_hand() {
                    self.gen_hand_drops(&mut pseudo);
                }
                pseudo.for_each(|mv| out.push(mv));
            }
            return;
        }
        // Pseudo-legal moves into a stack-backed buffer (no per-node heap
        // allocation), then verified one at a time.
        let mut pseudo = WideMoveList::new();
        self.gen_multi_royal_pseudo(&mut pseudo, us);
        // Hand drops (Xiang Fu — the first multi-royal hand variant): captured
        // pieces bank into hand and are dropped onto the variant's drop region. The
        // full drop-target superset is generated here (Xiang Fu has no single KING,
        // so `gen_hand_drops` uses the whole board) and each drop is verified for
        // Champion safety by the per-move filter below, exactly like a board move.
        // Gated behind `has_hand()` (default-off), so Spartan / Chak / Sho Shogi —
        // the handless multi-royal variants — never generate drops and stay
        // byte-identical.
        if V::has_hand() {
            self.gen_hand_drops(&mut pseudo);
        }
        // Count-thresholded pseudo-royalty (Sho Shogi): when the side holds more
        // than one royal (King + Crown Prince) neither is royal — there is no
        // king-safety constraint, so every pseudo-legal move is legal and no
        // per-move verify is needed. Default-on for every other variant, so this
        // branch is never taken (Spartan / Chak byte-identical).
        if !V::royal_constraint_active(&self.board, us) {
            pseudo.for_each(|mv| out.push(mv));
            return;
        }
        let them = us.opposite();

        // The enemy roles in play, computed once for the whole node: the verify
        // test then projects only these from each royal square rather than
        // looping every `WideRole`. A scratch position drives make/unmake in
        // place.
        let attackers = EnemyAttackers::new(&self.board, them);

        // Fast-accept filter (issue #183). When the side is **not currently in
        // duple check** — at least one of its kings is unattacked now — a move
        // that moves no king and whose origin and destination both lie off every
        // line through every royal square cannot add or remove a blocker on any
        // royal's rank, file, or diagonal, so it changes no slider/leaper/pawn
        // attack on any king: every king's attacked-status is exactly what it was
        // before the move. A king that was unattacked therefore stays unattacked,
        // so the side keeps a surviving king and the move is provably legal — it
        // skips the make/unmake + scan entirely. Anything that *could* matter (a
        // king move, an en-passant's third-square shuffle, a move touching a
        // royal line) falls through to the full verify, so the result is
        // identical. (When already in duple check, no move is pre-accepted: every
        // move must be verified, since legality then requires the move to *create*
        // a surviving king.)
        let occ = self.board.occupied();
        // The fast-accept is disabled while the side is "in check": for Spartan's
        // duple-check rule that means **every** royal is attacked (a move off the
        // lines cannot create a surviving king); for Chak's all-must-survive rule it
        // means **any** royal is attacked (a move off the lines cannot resolve the
        // attack on the en-prise royal). In both cases an unsafe-now situation must
        // fall through to the full per-move verify.
        let no_fast_accept = if V::royals_all_must_survive() {
            kings.into_iter().any(|k| self.royal_attacked(k, them, occ))
        } else {
            !kings
                .into_iter()
                .any(|k| !self.royal_attacked(k, them, occ))
        };
        let royal_lines = multi_royal_attack_lines::<G>(kings);

        let mut scratch = self.clone();
        pseudo.for_each(|mv| {
            if !no_fast_accept && multi_royal_move_off_lines::<G>(&mv, kings, royal_lines) {
                // Provably safe: no apply/unmake, no scan.
                out.push(mv);
            } else if scratch.multi_royal_move_is_legal(&mv, us, &attackers) {
                out.push(mv);
            }
        });
    }

    /// Returns `true` if the pseudo-legal multi-royal move `mv` is legal — leaves
    /// at least one of our kings unattacked — testing it by **make/unmake** on
    /// `self` (a scratch position holding the pre-move position).
    ///
    /// `self` is mutated to the post-move position, the duple-check survival test
    /// runs on the true post-move occupancy via
    /// [`royals_survive_after`](Self::royals_survive_after), and then `self` is
    /// restored byte-identically by [`undo`](Self::undo) — so one scratch position
    /// serves every sibling move with no per-move heap work and no
    /// `GenericPosition` reconstruction. Identical in result to cloning, applying,
    /// and calling [`royals_survive`](Self::royals_survive).
    fn multi_royal_move_is_legal(
        &mut self,
        mv: &WideMove,
        us: Color,
        attackers: &EnemyAttackers,
    ) -> bool {
        let undo = self.apply_with_undo(mv);
        // `apply` flipped the side to move; test our (now non-to-move) kings.
        let legal = self.royals_survive_after(us, attackers);
        // The fielded-role survival test must agree with the authoritative
        // full-role predicate on the very same post-move position; assert it in
        // debug/test builds so any drift between them is caught at the perft
        // suites rather than shipped.
        debug_assert_eq!(
            legal,
            self.royals_survive(us),
            "multi-royal fielded-role survival diverged from the full-role predicate"
        );
        // Unmake: restore the position incrementally from the bounded undo record
        // (issue #309) — only the squares the move wrote and the prior state, no
        // whole-board copy.
        self.undo(undo);
        legal
    }

    /// Returns `true` if color `who` keeps at least one unattacked king under the
    /// current occupancy, scanning only the enemy roles `attackers` records as
    /// present. A side with no king at all returns `false` (it has been
    /// eliminated). This is [`royals_survive`](Self::royals_survive) restricted to
    /// the fielded enemy roles via [`king_safe_after`](Self::king_safe_after), and
    /// is identical to it in result.
    fn royals_survive_after(&self, who: Color, attackers: &EnemyAttackers) -> bool {
        // Count-thresholded pseudo-royalty (Sho Shogi): evaluated on the post-move
        // board, so a Drunk-Elephant → Crown-Prince promotion that lifts `who` to
        // two royals makes the constraint inactive and the move legal regardless of
        // king safety (matching FSF, where promoting into a second pseudo-royal
        // drops the pseudo-royalty). Default-on, so Spartan / Chak are unaffected.
        if !V::royal_constraint_active(&self.board, who) {
            return true;
        }
        let kings = V::royal_squares(&self.board, who);
        if kings.is_empty() {
            return false;
        }
        let them = who.opposite();
        // Spartan's duple-check rule keeps a side legal while **at least one** royal
        // survives. Chak's pseudo-royal rule (`royals_all_must_survive`) instead
        // requires **every** royal to be safe — a move may not leave any king or
        // Divine Lord en prise. Default is the at-least-one rule, so Spartan is
        // byte-identical.
        // Multi-royal variants (Spartan, Chak) are not cannon variants, so they opt
        // no role into the masked slider fast path (`royal_slider_kind` is `None`);
        // the masks are still built per king to satisfy the shared signature and are
        // otherwise inert here.
        if V::royals_all_must_survive() {
            kings.into_iter().all(|k| {
                self.king_safe_after(k, them, attackers, KingLineMasks::new(k), None, true)
            })
        } else {
            kings.into_iter().any(|k| {
                self.king_safe_after(k, them, attackers, KingLineMasks::new(k), None, true)
            })
        }
    }

    // ===================== Alice chess =====================

    /// Pushes every legal Alice move for the side to move into `out`.
    ///
    /// Alice is generated by **verify**: [`gen_alice_pseudo`](Self::gen_alice_pseudo)
    /// produces the pseudo-legal moves (every chess move on the mover's own plane
    /// whose destination is vacant on the opposite plane, plus Alice castling),
    /// and each is applied to a scratch position and kept only if it leaves the
    /// mover's king safe — on the plane the king **ends up on** after the transfer
    /// (so a discovered check on the plane it stayed on, or a king transferring
    /// into check on the plane it lands on, is rejected), and, for an ordinary
    /// king move, also on the plane it **leaves** (the king "may not transfer out
    /// of check"; a castle's transit safety on the leaving plane is checked at
    /// generation instead). See [`alice_move_is_legal`](Self::alice_move_is_legal).
    fn generate_alice_into<S: WideSink>(&self, out: &mut S) {
        let us = self.state.turn;
        // A side without a king has already lost; enumerate its pseudo-moves
        // unverified so perft still descends (mirrors the cannon kingless branch).
        if self.board.king_of(us).is_none() {
            self.gen_alice_pseudo(out, us);
            return;
        }
        let mut pseudo = WideMoveList::new();
        self.gen_alice_pseudo(&mut pseudo, us);
        let mut scratch = self.clone();
        pseudo.for_each(|mv| {
            if scratch.alice_move_is_legal(&mv, us) {
                out.push(mv);
            }
        });
    }

    /// Returns `true` if the pseudo-legal Alice move `mv` leaves `us`'s king safe,
    /// tested by **make/unmake** on `self` (a scratch position holding the pre-move
    /// position, restored by [`undo`](Self::undo)).
    fn alice_move_is_legal(&mut self, mv: &WideMove, us: Color) -> bool {
        let from = mv.from::<G>();
        let to = mv.to::<G>();
        let is_castle = matches!(
            mv.kind(),
            WideMoveKind::CastleKingside | WideMoveKind::CastleQueenside
        );
        // Condition X ("the king cannot transfer out of check"): an ordinary king
        // move must leave its destination square unattacked on the plane it is
        // **leaving** — evaluated on the pre-move board, before the transfer. A
        // castle's transit / destination safety on that plane is already enforced
        // by `gen_alice_castles`, so it is exempt here. `self` still holds the
        // pre-move position here (the make below is the first edit), so it is the
        // pre-move board.
        if !is_castle
            && self
                .board
                .piece_at(from)
                .is_some_and(|p| p.role == WideRole::King)
            && !self.alice_king_dest_safe_on_origin(from, to, us)
        {
            return false;
        }
        // Condition Y (post-transfer): after the full move + transfer the king must
        // be unattacked on the plane it ends up on.
        let undo = self.apply_with_undo(mv);
        let legal = self.alice_king_safe(us);
        // Unmake incrementally (issue #309) — only the written squares and prior
        // state, no whole-board copy.
        self.undo(undo);
        legal
    }

    /// Returns `true` if `us`'s king is **not** attacked by an enemy piece on the
    /// **same plane** (Alice king-safety in the current position). A side with no
    /// king is vacuously safe.
    fn alice_king_safe(&self, us: Color) -> bool {
        let Some(king) = self.board.king_of(us) else {
            return true;
        };
        let them = us.opposite();
        let plane_mask = self.alice_plane_mask(self.state.board_b.contains(king));
        let plane_occ = self.board.occupied() & plane_mask;
        // Attackers projected from the king under the king's-plane occupancy,
        // restricted to enemy pieces actually on that plane: a piece on the other
        // plane neither blocks the ray (it is absent from `plane_occ`) nor attacks
        // across boards (it is dropped by `& plane_mask`).
        (self.attackers_to(king, them, plane_occ) & plane_mask).is_empty()
    }

    /// Returns `true` if a **king** of `us` moving from `from` to `to` lands on a
    /// square unattacked on the plane it is **leaving** (Alice condition X),
    /// evaluated on the pre-move (`self`) board before the transfer.
    fn alice_king_dest_safe_on_origin(&self, from: Square<G>, to: Square<G>, us: Color) -> bool {
        let them = us.opposite();
        let plane_mask = self.alice_plane_mask(self.state.board_b.contains(from));
        // Pre-transfer board-O occupancy: the king has left `from` and stands at
        // `to` on the leaving plane; a captured enemy on `to` is gone.
        let plane_occ = (self.board.occupied() & plane_mask).without(from).with(to);
        // Enemy pieces on the leaving plane, excluding one captured on `to`.
        let enemy_plane = (self.board.by_color(them) & plane_mask).without(to);
        (self.attackers_to(to, them, plane_occ) & enemy_plane).is_empty()
    }

    /// The plane mask (the set of squares on plane B if `plane_b`, else plane A)
    /// used to restrict Alice movement and king-safety to a single board.
    #[inline]
    fn alice_plane_mask(&self, plane_b: bool) -> Bitboard<G> {
        if plane_b {
            self.state.board_b
        } else {
            !self.state.board_b
        }
    }

    /// The Alice king-danger map on one plane: the squares attacked by `by`'s
    /// pieces **on that plane** under the plane's own occupancy.
    fn alice_plane_danger(
        &self,
        by: Color,
        plane_mask: Bitboard<G>,
        plane_occ: Bitboard<G>,
    ) -> Bitboard<G> {
        let mut danger = Bitboard::EMPTY;
        for role in WideRole::ALL {
            for from in self.board.pieces(by, role) & plane_mask {
                danger |= V::role_attacks(role, by, from, plane_occ);
            }
        }
        danger
    }

    /// Pushes the pseudo-legal Alice moves for `us` (every chess move on the
    /// mover's own plane whose destination is vacant on the opposite plane, plus
    /// Alice castling) into `out`, without any king-safety filtering.
    fn gen_alice_pseudo<S: WideSink>(&self, out: &mut S, us: Color) {
        let board = &self.board;
        let them = us.opposite();
        let occ = board.occupied();
        let bb = self.state.board_b;
        let our = board.by_color(us);
        let promo_roles = V::promotion_config().roles;
        for from in our {
            let role = board
                .role_at(from)
                .expect("our piece on an occupied square");
            let plane_b = bb.contains(from);
            let plane_mask = self.alice_plane_mask(plane_b);
            let plane_occ = occ & plane_mask; // pieces sharing the mover's plane
            let other_occ = occ & !plane_mask; // pieces on the opposite (transfer) plane
            if role == WideRole::Pawn {
                self.gen_alice_pawn(
                    out,
                    us,
                    from,
                    plane_occ,
                    other_occ,
                    plane_mask,
                    &promo_roles,
                );
                continue;
            }
            // Chess attack set on the mover's own plane; sliders are blocked only
            // by same-plane pieces.
            let att = V::role_attacks(role, us, from, plane_occ);
            // Land only off our own same-plane pieces and on a square whose
            // opposite plane is vacant (so the transfer succeeds). A same-plane
            // enemy on the target is a capture (its opposite plane is empty by the
            // one-piece-per-square invariant, so it survives the `!other_occ`
            // filter); an empty same-plane square is a quiet move.
            let friendly_plane = our & plane_mask;
            let enemy_plane = board.by_color(them) & plane_mask;
            let targets = att & !friendly_plane & !other_occ;
            for to in targets {
                let kind = if enemy_plane.contains(to) {
                    WideMoveKind::Capture
                } else {
                    WideMoveKind::Quiet
                };
                out.push(WideMove::new(from, to, kind));
            }
        }
        if V::has_castling() {
            self.gen_alice_castles(out, us);
        }
    }

    /// Pushes the pseudo-legal Alice pawn moves from `from` into `out`. The pawn
    /// pushes and captures on its own plane and transfers to the opposite plane;
    /// a quiet push needs its landing square vacant on **both** planes, a capture
    /// needs a same-plane enemy (whose opposite plane is then empty). En passant
    /// is excluded from Alice (the standard ruleset normally omits it).
    #[allow(clippy::too_many_arguments)]
    fn gen_alice_pawn<S: WideSink>(
        &self,
        out: &mut S,
        us: Color,
        from: Square<G>,
        plane_occ: Bitboard<G>,
        other_occ: Bitboard<G>,
        plane_mask: Bitboard<G>,
        promo_roles: &[WideRole],
    ) {
        let board = &self.board;
        let them = us.opposite();
        let occ = board.occupied();
        let dir: i8 = if us.is_white() { 1 } else { -1 };
        let promo = V::promotion_rank(us);
        let dpr = V::double_push_rank(us);
        // Forward push: the square in front must be empty on the mover's own plane
        // (the slide is on that board).
        if let Some(one) = from.offset(0, dir) {
            if !plane_occ.contains(one) {
                // Single push lands on `one`; the transfer needs it vacant on the
                // opposite plane too — i.e. `one` totally empty.
                if !other_occ.contains(one) {
                    if one.rank() == promo {
                        for &r in promo_roles {
                            out.push(WideMove::new(
                                from,
                                one,
                                WideMoveKind::Promotion {
                                    role: r,
                                    capture: false,
                                },
                            ));
                        }
                    } else {
                        out.push(WideMove::new(from, one, WideMoveKind::Quiet));
                    }
                }
                // Double push: the intermediate `one` only needs to be clear on the
                // own plane (above); the landing `two` must be vacant on both planes.
                if from.rank() == dpr {
                    if let Some(two) = from.offset(0, 2 * dir) {
                        if !occ.contains(two) {
                            out.push(WideMove::new(from, two, WideMoveKind::DoublePawnPush));
                        }
                    }
                }
            }
        }
        // Diagonal captures: a same-plane enemy on the diagonal target. The target
        // then holds an enemy on this plane, so its opposite plane is empty and the
        // transfer always succeeds.
        let enemy_plane = board.by_color(them) & plane_mask;
        for df in [-1i8, 1] {
            if let Some(cap) = from.offset(df, dir) {
                if enemy_plane.contains(cap) {
                    if cap.rank() == promo {
                        for &r in promo_roles {
                            out.push(WideMove::new(
                                from,
                                cap,
                                WideMoveKind::Promotion {
                                    role: r,
                                    capture: true,
                                },
                            ));
                        }
                    } else {
                        out.push(WideMove::new(from, cap, WideMoveKind::Capture));
                    }
                }
            }
        }
    }

    /// Pushes the pseudo-legal Alice castling moves for `us` into `out`.
    ///
    /// Castling is a king move played on the king's own plane: the squares the
    /// king and rook traverse must be clear **on that plane** (other-plane pieces
    /// are invisible to the slide), the king must not be in check nor pass through
    /// or land on a square attacked **on that plane**, the rook must share the
    /// king's plane, and the king's and rook's destination squares must be vacant
    /// on the **opposite** plane (both transfer there). The king's safety on the
    /// plane it lands on is checked by the verify step.
    fn gen_alice_castles<S: WideSink>(&self, out: &mut S, us: Color) {
        if !self.state.castling.has_any(us) {
            return;
        }
        let Some(king_sq) = self.board.king_of(us) else {
            return;
        };
        let rank = V::castle_rank(us);
        if king_sq.rank() != rank {
            return;
        }
        let king_plane_b = self.state.board_b.contains(king_sq);
        let plane_mask = self.alice_plane_mask(king_plane_b);
        let occ = self.board.occupied();
        let plane_occ = occ & plane_mask;
        let other_occ = occ & !plane_mask;
        let them = us.opposite();
        let king_danger = self.alice_plane_danger(them, plane_mask, plane_occ);
        // The king may not castle out of check (attacked on its own plane).
        if king_danger.contains(king_sq) {
            return;
        }
        let (k_king, k_rook) = V::castle_dest_files(KINGSIDE);
        let (q_king, q_rook) = V::castle_dest_files(QUEENSIDE);
        for (side, king_dest_file, rook_dest_file, kind) in [
            (KINGSIDE, k_king, k_rook, WideMoveKind::CastleKingside),
            (QUEENSIDE, q_king, q_rook, WideMoveKind::CastleQueenside),
        ] {
            let Some(rook_file) = self.state.castling.rook_file(us, side) else {
                continue;
            };
            let Some(rook_from) = Square::<G>::from_file_rank(rook_file, rank) else {
                continue;
            };
            if self.board.piece_at(rook_from) != Some(WidePiece::new(us, WideRole::Rook)) {
                continue;
            }
            // The rook must be on the same plane as the king to castle on it.
            if self.state.board_b.contains(rook_from) != king_plane_b {
                continue;
            }
            let Some(king_dest) = Square::<G>::from_file_rank(king_dest_file, rank) else {
                continue;
            };
            let Some(rook_dest) = Square::<G>::from_file_rank(rook_dest_file, rank) else {
                continue;
            };
            let king_path = between(king_sq, king_dest).with(king_dest);
            let rook_path = between(rook_from, rook_dest).with(rook_dest);
            let must_be_empty = (king_path | rook_path).without(king_sq).without(rook_from);
            // Path clear on the king's plane (the board the move is played on).
            if !(must_be_empty & plane_occ).is_empty() {
                continue;
            }
            // King and rook destinations must be vacant on the opposite plane.
            if other_occ.contains(king_dest) || other_occ.contains(rook_dest) {
                continue;
            }
            // The king may not pass over or land on a square attacked on its plane.
            let king_walk = between(king_sq, king_dest).with(king_dest);
            if !(king_walk & king_danger).is_empty() {
                continue;
            }
            out.push(WideMove::new(king_sq, king_dest, kind));
        }
    }

    /// Generates every legal move for a **cannon** variant (Shako, Xiangqi) via
    /// pseudo-legal generation plus per-move verification.
    ///
    /// Reuses the multi-royal pseudo-move generator — which already emits every
    /// role (including the cannon's quiet rook-rays through `quiet_only_targets`
    /// and its over-screen captures through `role_attacks`), the standard pawns
    /// with en passant, and castling — then keeps each move whose resulting
    /// position leaves the (single) king unattacked, with attacks computed on the
    /// true post-move occupancy. This is the only sound way to handle a cannon's
    /// screen-dependent check and king-danger; gated behind `has_cannons()`, so it
    /// never runs for a non-cannon variant.
    ///
    /// Hot-path shape (issue #193): the per-move verify uses **make/unmake** on a
    /// `Copy` board+state snapshot rather than cloning the whole position into a
    /// fresh `GenericPosition` per move, and the king-attack test
    /// ([`king_safe_after`](Self::king_safe_after)) scans only the enemy roles
    /// **actually fielded** (precomputed once per node) instead of all
    /// [`WideRole::COUNT`] of them. The produced move set is byte-identical.
    fn generate_cannon_verify_into<S: WideSink>(&self, out: &mut S) {
        let us = self.state.turn;
        // Janggi pass terminals — the side to move then has **no legal move at all**
        // (Fairy-Stockfish returns zero), ending the game:
        //   * two consecutive passes (a pass right after the opponent passed), or
        //   * a pass made by the opponent **while the generals face** on an open
        //     line (the bikjang draw claim) — the facing side then has no move.
        // Gated behind `allows_pass()` (default-off), so inert for every other
        // variant.
        if V::allows_pass()
            && self.state.consecutive_passes >= 1
            && (self.state.consecutive_passes >= 2
                || (V::restricts_facing_general() && generals_face::<G>(&self.board)))
        {
            return;
        }
        // Flag-rank "campmate" (Synochess): if the opponent's king already stands
        // on its goal rank, the opponent has won and the side to move has no legal
        // continuation — Fairy-Stockfish returns zero here, so the node is a perft
        // leaf. Gated behind `has_flag_win()` (default-off).
        if V::has_flag_win() && self.flag_win_terminal(us) {
            return;
        }
        // Bare-king "Robado" draw (Shatar): a side reduced to its lone king ends
        // the game in an immediate draw, so the node is a terminal perft leaf with
        // no continuation. Gated behind `has_bare_king_draw()` (default-off), so
        // inert for every other variant. (Shatar takes the standard path; this
        // mirrors the standard-path chokepoint for any future bare-king variant
        // that rides the verify path.)
        if V::has_bare_king_draw() && self.bare_king_present() {
            return;
        }
        // Bare-king baring loss (Shatranj): a bared side that has lost ends the
        // game, so the node is a terminal perft leaf. Gated behind
        // `has_bare_king_loss()` (default-off), so inert for every other variant.
        // (This mirrors the standard-path chokepoint for any bare-king-loss variant
        // that rides the verify path.)
        if V::has_bare_king_loss() && self.bare_king_loss_loser().is_some() {
            return;
        }
        // A side whose king has been captured has no royal piece, so there is no
        // self-check to filter: every pseudo-legal move is "legal" (the side has
        // already lost, but perft still enumerates its continuations). Fairy-
        // Stockfish does this for Xiangqi — a cannon may capture the enemy general
        // over a screen, and the kingless side then enumerates its pseudo-moves —
        // so emit the pseudo-legal set unverified. (Unreachable in a Shako legal
        // tree, where standard check rules forbid ever leaving the king en prise.)
        if self.board.king_of(us).is_none() {
            self.gen_multi_royal_pseudo(out, us);
            return;
        }
        let king = self.board.king_of(us).expect("king present on this branch");
        let mut pseudo = WideMoveList::new();
        self.gen_multi_royal_pseudo(&mut pseudo, us);
        // The enemy roles in play, computed once for the whole node: the verify
        // test then projects only these from the king square rather than looping
        // every `WideRole`. A scratch position drives make/unmake in place.
        let attackers = EnemyAttackers::new(&self.board, us.opposite());

        // Fast-accept filter (issue #193). When our king is **not currently in
        // check**, a move that touches no line through the king — its origin and
        // destination both lie off the king's rank, file, and both diagonals — and
        // is not itself a king move can neither expose the king to a slider, change
        // a cannon's screen on it, nor open the generals' file: every attack on the
        // king travels one of those four lines, and a piece off all of them is
        // neither a blocker nor a screen for any of them. Such a move is provably
        // legal and skips the make/unmake + scan entirely. Anything that *could*
        // matter (a king move, an en-passant's three-square shuffle, a move on a
        // king line) falls through to the full verify, so the result is identical.
        // The king's line masks, precomputed once for the node: every sibling move
        // that falls through to the full verify re-tests king safety from this same
        // square (only a king move shifts it, handled in `cannon_move_is_legal`), so
        // the rank/file/diagonal masks the slider reverse-projection needs are
        // constant and built once here rather than per move.
        let king_masks = KingLineMasks::new(king);
        // The per-role reach supersets, also precomputed once for the node and
        // aligned to `attackers.roles()`: a forward-projected (leg-asymmetric) role
        // restricts the enemy pieces it tests to those on its king-reach superset.
        // `Bitboard::FULL` marks "no superset available, test every piece" (the
        // unchanged behaviour). Built per node and reused by every sibling move.
        let reach = attackers.reach_supersets::<G, V>(king);
        let reach_slice = reach.as_ref().map(|r| &r[..attackers.len()]);
        let in_check = !self.king_safe_after(
            king,
            us.opposite(),
            &attackers,
            king_masks,
            reach_slice,
            true,
        );
        // The fast-accept king-line mask. For a variant with no long-range diagonal
        // king attacker (no bishop/queen reaching the king down a board diagonal —
        // every diagonal threat is a hobbled leaper leg or a palace screen within a
        // few squares), the diagonal extent is capped, so a move on a far-diagonal
        // square is fast-accepted instead of needlessly verified. The authoritative
        // `king_safe_after` is unchanged, so the produced set stays byte-identical
        // (a debug_assert re-checks every capped fast-accept against the full
        // verify). Default `None` keeps the full board-length diagonals for every
        // diagonal-slider variant (Shako, Synochess, …).
        let king_lines = match V::king_diag_attack_radius() {
            Some(radius) => king_attack_lines_diag_capped::<G>(king, radius),
            None => king_attack_lines::<G>(king),
        };

        // Janggi bikjang: facing the enemy general on an open line is *also* a check
        // the side to move must resolve. It does not enter `king_safe_after` (it is
        // not a pinning ray), so test it separately. While it (or an ordinary check)
        // holds, the geometry fast-accept must be disabled — a move off the king
        // lines does not resolve a facing check, so it cannot be accepted without
        // the full per-move facing verify. Default-off elsewhere.
        let facing_check = V::restricts_facing_general() && generals_face::<G>(&self.board);
        let must_verify_all = in_check || facing_check;

        // Flag-rank "campmate" (Synochess): a king may not step **onto** its own
        // goal rank while the enemy king already occupies that rank (the flag is
        // contested) — the only flag-rank king move then allowed is to capture the
        // enemy king itself, which clears the contest. Precompute the contested
        // flag rank once; `None` when the rule is off or the flag is uncontested.
        // A move off the king lines can never be such a king move, so the
        // fast-accept path stays correct.
        let contested_flag_rank = self.contested_flag_rank(us);

        let mut scratch = self.clone();
        pseudo.for_each(|mv| {
            if let Some(rank) = contested_flag_rank {
                let to = mv.to::<G>();
                if mv.from::<G>() == king
                    && to.rank() == rank
                    && self
                        .board
                        .piece_at(to)
                        .is_none_or(|p| p.role != WideRole::King)
                {
                    // King stepping onto an empty/non-king square of the contested
                    // flag rank: forbidden.
                    return;
                }
            }
            if !must_verify_all && cannon_move_off_king_lines::<G>(&mv, king, king_lines) {
                // Provably safe: no apply/unmake, no scan. When the variant caps the
                // diagonal extent of the king lines (no long-range diagonal king
                // attacker), the smaller mask widens this fast-accept; the
                // make/unmake verify it skips would reach the same verdict, asserted
                // in debug builds.
                debug_assert!(
                    V::king_diag_attack_radius().is_none()
                        || scratch.cannon_move_is_legal(
                            &mv,
                            us,
                            &attackers,
                            king_masks,
                            reach_slice,
                            facing_check,
                        ),
                    "diagonal-capped fast-accept must agree with the full king-safety verify"
                );
                out.push(mv);
            } else if scratch.cannon_move_is_legal(
                &mv,
                us,
                &attackers,
                king_masks,
                reach_slice,
                facing_check,
            ) {
                out.push(mv);
            }
        });

        // Synochess soldier-reinforcement drops: the side to move places a pocketed
        // piece onto a permitted empty square. A drop only ever *adds* a friendly
        // blocker, so it cannot expose the king to a slider; the sole legality
        // concern is failing to resolve an existing check — including a
        // flying-general confrontation, which `gen_hand_drops`' `attackers_to`
        // mask does not see. So generate the pocket's drops, then keep each one
        // that leaves the king safe under the same post-move verify the board moves
        // use. Gated behind `has_hand()` (default-off), so no cannon-only variant
        // emits drops.
        if V::has_hand() {
            let mut drops = WideMoveList::new();
            self.gen_hand_drops(&mut drops);
            drops.for_each(|mv| {
                if scratch.cannon_move_is_legal(
                    &mv,
                    us,
                    &attackers,
                    king_masks,
                    reach_slice,
                    facing_check,
                ) {
                    out.push(mv);
                }
            });
        }

        // The Janggi pass: a legal null move that only flips the side to move.
        // Fairy-Stockfish counts it in perft and encodes it as the general "staying
        // put" (`from == to == the general's square`); it is forbidden while in an
        // **ordinary** check (but is a valid way to answer a bikjang facing check),
        // and **two consecutive passes end the game** (handled at the top). Gated
        // behind `allows_pass()` (default-off), so no other variant ever emits it.
        // Emitting it as a quiet `king -> king` move makes `apply` remove-then-
        // replace the general on its own square — a board no-op that advances the
        // turn and clocks.
        if V::allows_pass() && !in_check {
            out.push(WideMove::new(king, king, WideMoveKind::Quiet));
        }
    }

    /// Returns `true` if the pseudo-legal cannon-variant move `mv` is legal —
    /// leaves our king unattacked — testing it by **make/unmake** on `self`
    /// (a scratch position holding the pre-move position).
    ///
    /// `self` is mutated to the post-move position, the king-safety check runs on
    /// the true post-move occupancy (including the cannon over-screen captures and
    /// the flying-general file, via [`king_safe_after`](Self::king_safe_after)),
    /// and then `self` is restored byte-identically by [`undo`](Self::undo) — so
    /// one scratch position serves every sibling move with no per-move heap work
    /// and no `GenericPosition` reconstruction.
    fn cannon_move_is_legal(
        &mut self,
        mv: &WideMove,
        us: Color,
        attackers: &EnemyAttackers,
        king_masks: KingLineMasks<G>,
        reach: Option<&[Bitboard<G>]>,
        faced_before: bool,
    ) -> bool {
        // The bikjang facing rule needs whether the mover's general faced **before**
        // the move. That is a property of the (pre-move) node position, identical for
        // every sibling move verified against this scratch — `self` always holds the
        // restored node board here, the make below being the first edit — so the
        // caller computes it once per node (`facing_check`) and passes it in rather
        // than re-running `generals_face` for every move. Always `false` when the
        // variant has no facing rule (`restricts_facing_general() == false`).
        debug_assert_eq!(
            faced_before,
            V::restricts_facing_general() && generals_face::<G>(&self.board),
            "faced_before must equal the pre-move facing state of the scratch board"
        );
        // Fairy-Stockfish's special-cased en-passant legality re-checks only real
        // slider attacks through the two vacated pawn squares, never the
        // flying-general pseudo-attacker, so an en-passant that *reveals* a king
        // face-off is legal there (issue #359). Mirror that by skipping the facing
        // term of `king_safe_after` for an en-passant move; every other move (and the
        // ordinary horizontal-pin slider check) keeps the full test.
        let consider_facing = !matches!(mv.kind(), WideMoveKind::EnPassant);
        let undo = self.apply_with_undo(mv);
        // `apply` flipped the side to move; our king is now the non-mover's.
        let mut legal = match self.board.king_of(us) {
            // The node-level `king_masks` and reach supersets are taken through the
            // **pre-move** king square, which is correct for every sibling that does
            // not move the king. A king move shifts the royal square, so the cached
            // geometry no longer applies: rebuild the line masks for the new square
            // and disable the reach pre-filter (its supersets were keyed on the old
            // square), falling back to testing every piece. This is the only place
            // the cached node geometry can go stale.
            Some(king) => {
                if king == king_masks.square() {
                    self.king_safe_after(
                        king,
                        us.opposite(),
                        attackers,
                        king_masks,
                        reach,
                        consider_facing,
                    )
                } else {
                    let masks = KingLineMasks::new(king);
                    self.king_safe_after(
                        king,
                        us.opposite(),
                        attackers,
                        masks,
                        None,
                        consider_facing,
                    )
                }
            }
            // A move that captured our own king cannot arise from a legal pseudo
            // set here (our king is never a capture target of our own move), but
            // be defensive: no king means nothing to leave en prise.
            None => true,
        };
        // The Janggi bikjang general-facing rule (default-off). Facing the enemy
        // general on an open line is a **check the side to move must resolve**, but
        // — unlike Xiangqi's flying general — the facing ray does **not** pin a
        // blocker: a side may freely move a blocker off the line, *creating* a
        // facing against itself (the resulting check then falls on the opponent, who
        // is the next to move). So a non-pass move is illegal iff the mover's
        // general faces **after** it AND either it faced **before** (an existing
        // facing check it failed to resolve) OR the move was the **general's own**
        // move (the general may not step into / along a facing). The pass
        // (`from == to`) always escapes. `self` is the post-move position;
        // `faced_before` was captured from the pre-move board.
        if legal && V::restricts_facing_general() {
            let from = mv.from::<G>();
            let to = mv.to::<G>();
            if from != to && generals_face::<G>(&self.board) && faced_before {
                // Faced before and still faces after a non-pass move: an existing
                // bikjang check the move failed to resolve (or the general slid
                // along the contested line staying faced). Moving *into* a facing
                // from a non-facing position, and exposing one's own general, both
                // pass `faced_before == false` and stay legal.
                legal = false;
            }
        }
        // Unmake incrementally (issue #309) — only the written squares and prior
        // state, no whole-board copy.
        self.undo(undo);
        legal
    }

    /// Returns `true` if the royal square `king` is **not** attacked by color `by`
    /// under the current occupancy, scanning only the enemy roles `attackers`
    /// records as present (plus the default-off flying-general file).
    ///
    /// This is the cannon verify path's hot inner test. It is the negation of
    /// [`royal_attacked`](Self::royal_attacked) restricted to the fielded enemy
    /// roles: a role with no enemy piece can never attack the king, so projecting
    /// it from the king square is wasted work. The set of fielded roles is fixed
    /// for a node, so it is computed once in [`EnemyAttackers::new`] and reused for
    /// every sibling move. The result is identical to `!royal_attacked(...)`.
    ///
    /// `consider_facing` selects whether the default-off flying-general / king
    /// face-off ([`extra_royal_attack`](WideVariant::extra_royal_attack)) counts as
    /// an attack here. It is `true` everywhere except when verifying an **en-passant**
    /// capture on the cannon-verify path: Fairy-Stockfish's special-cased en-passant
    /// legality re-checks only real **slider** attacks through the vacated squares
    /// and never re-evaluates the flying-general pseudo-attacker, so an en-passant
    /// that *reveals* a king face-off is legal in FSF (issue #359). Passing `false`
    /// for en passant reproduces that exactly; every other move keeps the full check.
    #[inline]
    fn king_safe_after(
        &self,
        king: Square<G>,
        by: Color,
        attackers: &EnemyAttackers,
        king_masks: KingLineMasks<G>,
        reach: Option<&[Bitboard<G>]>,
        consider_facing: bool,
    ) -> bool {
        debug_assert_eq!(king_masks.square(), king);
        let board = &self.board;
        let occupied = board.occupied();
        for (idx, &role) in attackers.roles().iter().enumerate() {
            let mut pieces = board.pieces(by, role);
            if pieces.is_empty() {
                continue;
            }
            // Symmetric standard-slider roles (a plain rook / bishop / queen) reuse
            // the king's precomputed line masks: the reverse projection back from
            // the king is bit-for-bit the same slider ray, but the per-move mask
            // rebuild (the diagonal fill in particular) is skipped. A role opts in
            // only when its `role_attacks` is exactly the plain slider from the
            // king square, so the result is identical to the general path below.
            if let Some(kind) = V::royal_slider_kind(role) {
                let from_king = match kind {
                    RoyalSlider::Rook => rook_attacks_masked(king_masks, occupied),
                    RoyalSlider::Bishop => bishop_attacks_masked(king_masks, occupied),
                    RoyalSlider::Queen => queen_attacks_masked(king_masks, occupied),
                };
                if !(from_king & pieces).is_empty() {
                    return false;
                }
                continue;
            }
            // The Xiangqi Horse's leg is asymmetric, so reverse-projecting from the
            // king square tests the wrong leg; detect it forward from each horse,
            // exactly as `attackers_to` does.
            if V::role_attack_is_leg_asymmetric(role) {
                // Cheap superset pre-filter (precomputed once per node): a piece off
                // the king's reach superset for this role can never attack the king,
                // so it is dropped before the exact (and costlier) forward
                // projection. The mask is a superset — it ignores hobbling legs,
                // region confinement, and cannon screens, all of which the forward
                // projection still checks exactly — so no real attacker is excluded
                // and the result is identical to testing every piece. Absent a
                // superset (`None`) the full set is tested, as before.
                if let Some(masks) = reach {
                    if let Some(mask) = masks.get(idx).copied() {
                        pieces &= mask;
                        if pieces.is_empty() {
                            continue;
                        }
                    }
                }
                // A board-aware attacker (the Janggi cannon) is projected from each
                // origin against the whole board; the king sits on an occupied
                // square, so it can only fall in the cannon's capture portion. The
                // default-off hook leaves every other variant byte-identical.
                let hits = pieces.into_iter().any(|from| {
                    let att = if V::uses_board_attacks() {
                        V::role_threats_board(role, by, from, board)
                            .unwrap_or_else(|| V::role_attacks(role, by, from, occupied))
                    } else {
                        V::role_attacks(role, by, from, occupied)
                    };
                    att.contains(king)
                });
                if hits {
                    return false;
                }
                continue;
            }
            // Project the role's attack pattern back from the king square (the
            // opposite color for a directional role, e.g. a pawn), exactly as
            // `attackers_to` does, and see whether it reaches an enemy piece.
            let from_king = if V::role_attack_is_directional(role) {
                V::role_attacks(role, by.opposite(), king, occupied)
            } else {
                V::role_attacks(role, by, king, occupied)
            };
            if !(from_king & pieces).is_empty() {
                return false;
            }
        }
        // The Xiangqi flying-general file attack (default-off elsewhere). Skipped
        // when `consider_facing` is false — an en-passant capture, whose FSF legality
        // ignores this pseudo-attacker (issue #359).
        if consider_facing
            && V::has_flying_general()
            && V::extra_royal_attack(board, king, by, occupied)
        {
            return false;
        }
        true
    }

    /// Returns `true` if color `who` has a king standing on its flag
    /// ("campmate") goal rank — the Synochess win condition. Only meaningful while
    /// [`WideVariant::has_flag_win`] is `true`; the caller gates on it.
    fn flag_win_reached(&self, who: Color) -> bool {
        let rank = V::flag_rank(who);
        let on_rank = self
            .board
            .kings_of(who)
            .into_iter()
            .filter(|k| k.rank() == rank);
        if V::flag_win_requires_safe() {
            // Dobutsu's "try" rule: a king on its goal rank wins only if it is
            // **safe** — unattacked by the opponent, who would otherwise capture it.
            // The check fires on the opponent's turn, so the opponent is `who`'s
            // enemy; reuse the standard attacker scan on the live occupancy. Default
            // off, so every other flag variant keeps the purely positional rule.
            let them = who.opposite();
            let occ = self.board.occupied();
            on_rank
                .into_iter()
                .any(|k| self.attackers_to(k, them, occ).is_empty())
        } else {
            on_rank.into_iter().next().is_some()
        }
    }

    /// Returns `true` if a flag ("campmate" / "try") win has already been reached
    /// when `us` is to move, so the node is terminal (no legal continuation). The
    /// caller gates on [`WideVariant::has_flag_win`].
    ///
    /// For the purely positional flag (Orda / Synochess) only the **opponent** can
    /// already stand on its goal rank — the winner places its own king there on its
    /// own move, so the win is always adjudicated on the loser's turn.
    ///
    /// For the **safe** "try" rule (Dobutsu), the win can become true on **either**
    /// side's turn: the loser may have safely reached the far rank on its own move,
    /// or the *winner's* king may have become safe only because the loser's last
    /// move stopped attacking it — in which case the win is adjudicated on the
    /// winner's own (next) turn. So check both sides under the safe rule.
    fn flag_win_terminal(&self, us: Color) -> bool {
        self.flag_win_reached(us.opposite())
            || (V::flag_win_requires_safe() && self.flag_win_reached(us))
    }

    /// Returns `true` if **either** side has been reduced to a lone king (its
    /// only remaining piece) — the Shatar "Robado" terminal-draw condition. Only
    /// meaningful while [`WideVariant::has_bare_king_draw`] is `true`; the caller
    /// gates on it. A side is bare-king when its colour mask holds exactly one
    /// piece, which (every side always having a king on a legal board) is the
    /// king alone.
    #[must_use]
    pub fn bare_king_present(&self) -> bool {
        self.board.by_color(Color::White).count() == 1
            || self.board.by_color(Color::Black).count() == 1
    }

    /// Returns the side that has been **bared** (reduced to its lone king) and so
    /// **lost** under the Shatranj baring rule, if this node is terminal under that
    /// rule; else `None`. Only meaningful while
    /// [`WideVariant::has_bare_king_loss`] is `true`; the caller gates on it.
    ///
    /// A side is bared when its colour mask holds exactly one piece (its king).
    /// Baring is decisive — a loss for the bared side — but it mirrors
    /// Fairy-Stockfish's `extinctionClaim`, which grants the bared side one
    /// "bare-back" reply: the node is terminal when the opponent holds **three or
    /// more** pieces (no single capture could bare it back) **or** when it is the
    /// opponent's (the winner's) turn — the bare-back chance already spent. While
    /// it is the bared side's own turn and the opponent holds only two pieces (a
    /// king the bared side might capture next move, baring back into a
    /// King-vs-King draw), the node is **not** yet terminal, so this returns
    /// `None`. King-vs-King (neither side with two pieces) is likewise non-terminal.
    #[must_use]
    pub fn bare_king_loss_loser(&self) -> Option<Color> {
        let white = self.board.by_color(Color::White).count();
        let black = self.board.by_color(Color::Black).count();
        let (bared, opponent) = if white == 1 && black >= 2 {
            (Color::White, black)
        } else if black == 1 && white >= 2 {
            (Color::Black, white)
        } else {
            // Neither side bared, or King-vs-King (no opponent with ≥2 pieces).
            return None;
        };
        // Terminal once the bared side can no longer bare back: the opponent has
        // ≥3 pieces, or the bared side is not to move (its single reply is spent).
        if opponent >= 3 || self.state.turn != bared {
            Some(bared)
        } else {
            None
        }
    }

    /// Returns the flag goal rank of the side to move (`us`) when it is
    /// **contested** — i.e. the enemy king already stands on it, so `us`'s king may
    /// not step onto that rank (except to capture the enemy king there). Returns
    /// `None` when the flag rule is off or the rank is uncontested.
    fn contested_flag_rank(&self, us: Color) -> Option<u8> {
        if !V::has_flag_win() {
            return None;
        }
        let rank = V::flag_rank(us);
        let enemy_on_rank = self
            .board
            .kings_of(us.opposite())
            .into_iter()
            .any(|k| k.rank() == rank);
        enemy_on_rank.then_some(rank)
    }

    /// Returns `true` if, in this position, color `who` keeps at least one
    /// unattacked king — i.e. `who` is not in (duple) check. A side with no king
    /// at all returns `false` (it has been eliminated).
    fn royals_survive(&self, who: Color) -> bool {
        // Count-thresholded pseudo-royalty (Sho Shogi): while `who` holds more than
        // one royal the constraint is inactive — it cannot be in check, so it
        // always survives regardless of which royal is attacked. Default-on for
        // every other variant, so this is inert (Spartan / Chak byte-identical).
        if !V::royal_constraint_active(&self.board, who) {
            return true;
        }
        let kings = V::royal_squares(&self.board, who);
        if kings.is_empty() {
            return false;
        }
        let occ = self.board.occupied();
        let them = who.opposite();
        // See `royals_survive_after`: Chak requires every royal safe, Spartan only
        // one. Default (at-least-one) keeps Spartan byte-identical.
        if V::royals_all_must_survive() {
            kings
                .into_iter()
                .all(|k| self.attackers_to(k, them, occ).is_empty())
        } else {
            kings
                .into_iter()
                .any(|k| self.attackers_to(k, them, occ).is_empty())
        }
    }

    /// Pushes the pseudo-legal base moves for a multi-king side into `pseudo`
    /// (no self-check filtering — that is done by the caller per move). Kings,
    /// every other piece, the Berolina/standard pawns, and castling are all
    /// emitted with a full board mask and no pins.
    fn gen_multi_royal_pseudo<S: WideSink>(&self, pseudo: &mut S, us: Color) {
        let board = &self.board;
        let occupied = board.occupied();
        let our_pieces = board.by_color(us);
        let their_pieces = board.by_color(us.opposite());
        let full = Bitboard::FULL;

        // Every non-pawn role (including the king): its attack set minus friendly
        // pieces. No check mask, no pin lines — the per-move filter handles safety.
        // A hand variant whose Pawn is a forward stepper (Cannon Shogi's Soldier)
        // routes its Pawn through this generic piece loop, exactly as the standard
        // generator does — `gen_pawn_moves` (the diagonal-capture chess pawn) is
        // then skipped below. Every multi-royal variant without a hand keeps the
        // Pawn on the dedicated pawn generator, byte-identically.
        let pawn_is_stepper = V::pawn_is_stepper();
        for role in WideRole::ALL {
            // The Berolina Hoplite is always handled by its own emitter below. A
            // standard chess Pawn (double push, diagonal capture, en passant) is
            // handled by the straight-push pawn generator below; only a **forward
            // stepper** Pawn — the Shogi pawn (Sho Shogi: one square forward,
            // capturing straight ahead, promoting in the zone) or Cannon Shogi's
            // Soldier (forward *or sideways*) — flows through this role loop, where
            // its full move-and-attack set comes from `role_attacks` like any other
            // promotable stepper. `pawn_is_stepper()` is the sole discriminator:
            // every standard-pawn variant that reaches this generator (Synochess and
            // the crazyhouse family) sets it `false`, so they keep `gen_pawn_moves`
            // below and stay byte-identical, while Sho Shogi (no hand) and Cannon
            // Shogi (with a hand) set it `true` and are routed here.
            if role == WideRole::Hoplite {
                continue;
            }
            if role == WideRole::Pawn && !pawn_is_stepper {
                continue;
            }
            // Whether this role expands into a promotion on a move that ends in the
            // promotion zone: Chak's King → Divine Lord / Soldier → Shaman (no hand,
            // gated behind `has_piece_promotion()`), or a hand variant's promotable
            // piece (Cannon Shogi's Soldier / Rook / Bishop / cannons). Both are
            // default-off, so every other multi-royal variant skips this and is
            // byte-identical.
            let promotable =
                (V::has_piece_promotion() || V::has_hand()) && V::role_can_promote(role);
            for from in board.pieces(us, role) {
                // A board-aware role (the Janggi cannon, whose screen/target may
                // not be a cannon and which may jump the palace diagonal; Chak's
                // Quetzal cannon and move≠capture Soldier) computes its set from the
                // whole board; the default-off hook returns `None` for every other
                // variant/role, so they keep the occupancy-only path
                // byte-identically. The returned set already folds the cannon's
                // quiet jumps and over-screen captures together; `emit_targets`
                // splits them by enemy occupancy.
                let targets = if V::uses_board_attacks() {
                    V::role_attacks_board(role, us, from, board)
                        .unwrap_or_else(|| V::role_attacks(role, us, from, occupied))
                } else {
                    V::role_attacks(role, us, from, occupied)
                } & !our_pieces;
                if promotable {
                    // The full Shogi-aware promotion expansion: a move that starts
                    // or ends in the zone may promote (to `role_promoted_to`), and
                    // **must** where the piece would otherwise be immobile (a
                    // Pawn/Lance last rank, a Knight last two ranks — Sho Shogi's
                    // forced promotion, and Cannon Shogi's Lance/Knight). For Chak
                    // (mandatory promotion, no forced cases) and for Cannon Shogi's
                    // hand pieces this is the one emitter.
                    self.emit_promotable_targets(pseudo, role, from, targets, their_pieces, us);
                } else {
                    pseudo.emit_targets(from, targets, their_pieces);
                }
                // Quiet-only steps (the Spartan Lieutenant's sideways slide; Chak's
                // Soldier's forward/sideways move): a move onto an empty square that
                // can never capture. Default-empty, so inert for every other
                // role/variant.
                let quiet_only = if V::uses_board_attacks() {
                    V::quiet_targets_board(role, us, from, board)
                        .unwrap_or_else(|| V::quiet_only_targets(role, us, from, occupied))
                } else {
                    V::quiet_only_targets(role, us, from, occupied)
                } & !occupied;
                let from_in_zone = promotable && V::in_promotion_zone(us, from.rank());
                for to in quiet_only {
                    if promotable && (from_in_zone || V::in_promotion_zone(us, to.rank())) {
                        Self::emit_piece_promotion_one(pseudo, role, from, to, false, us);
                    } else {
                        pseudo.push(WideMove::new(from, to, WideMoveKind::Quiet));
                    }
                }
            }
        }

        // Pawns: the standard straight-push pawn (`WideRole::Pawn`) and, when the
        // variant fields them, the Berolina Hoplite (`WideRole::Hoplite`). A side
        // holds only one kind (Spartan: White has Pawns, Black has Hoplites), so
        // running both emitters yields exactly that side's pawn moves. A king
        // square is only needed by the standard generator for en passant; the
        // multi-king variant sets no ep target, so any king will do.
        let king_sq = board.king_of(us).unwrap_or_else(|| Square::new(0));
        let pins = Pins::empty(king_sq);
        // The straight-push chess pawn (double step, diagonal capture, en passant,
        // last-rank promotion). Skipped only when the variant's pawn is a forward
        // stepper (Sho Shogi's Shogi pawn, Cannon Shogi's Soldier) — it was already
        // produced via the role loop above. Every standard-pawn variant reaching
        // this generator (Synochess, the crazyhouse family) sets `pawn_is_stepper()`
        // false and keeps its pawns here, byte-identically.
        if !pawn_is_stepper {
            self.gen_pawn_moves(pseudo, us, occupied, their_pieces, full, &pins, king_sq);
        }
        if V::has_berolina_pawns() {
            self.gen_berolina_moves(pseudo, us, occupied, their_pieces, full, &pins);
        }
        // Chu-Shogi Lion powers (default-off): the two-step area move, igui
        // stationary capture, double capture, and jitto pass — the moves the plain
        // leaper `role_attacks` cannot express. Emitted only under `has_lion_moves`,
        // so every other variant skips this and is byte-identical.
        if V::has_lion_moves() {
            self.gen_lion_moves(pseudo, us, their_pieces, our_pieces);
        }

        // Castling: only the single-king side (white, in Spartan) ever has it, and
        // it must not be in check / pass through an attacked square. Compute the
        // enemy danger map with our king lifted, exactly as the standard path, and
        // require the side not currently in check.
        if V::has_castling() && self.state.castling.has_any(us) {
            if let Some(ksq) = board.king_of(us) {
                let occ_without_king = occupied.without(ksq);
                let king_danger = self.attacked_by(us.opposite(), occ_without_king);
                if self.attackers_to(ksq, us.opposite(), occupied).is_empty() {
                    self.gen_castles(pseudo, us, occupied, king_danger, ksq);
                }
            }
        }
    }

    /// Generates the side-to-move's **Chu-Shogi Lion** moves: the special
    /// [`WideMoveKind::LionMove`] moves that a plain leaper cannot express — the
    /// two-step area move (with an intermediate capture), the *igui* stationary
    /// capture, the double capture, and the *jitto* pass. The ordinary single steps
    /// and distance-two jumps are already produced by `role_attacks`
    /// (`Quiet`/`Capture` moves), so this pass adds *only* the moves that touch an
    /// intermediate square or return the piece to its origin.
    ///
    /// Runs only under [`WideVariant::has_lion_moves`]; a full lion
    /// ([`WideVariant::role_is_full_lion`]) turns freely across its two King-steps,
    /// while a line-lion ([`WideVariant::role_lion_lines`]: Horned Falcon, Soaring
    /// Eagle) may only step straight along each lion-power line.
    fn gen_lion_moves<S: WideSink>(
        &self,
        out: &mut S,
        us: Color,
        their_pieces: Bitboard<G>,
        our_pieces: Bitboard<G>,
    ) {
        // The eight King directions, the two-step alphabet of a full lion.
        const DIRS8: [(i8, i8); 8] = [
            (-1, -1),
            (0, -1),
            (1, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
        ];
        let board = &self.board;
        for role in WideRole::ALL {
            let full = V::role_is_full_lion(role);
            let lines = V::role_lion_lines(role);
            if !full && lines.is_empty() {
                continue;
            }
            for from in board.pieces(us, role) {
                let mut can_pass = false;
                if full {
                    // A full lion: the first King-step lands on any non-friendly
                    // adjacent square (empty or enemy), then it may stop, igui
                    // (return home), or take a second King-step in any direction.
                    // The plain single steps and distance-two jumps are already
                    // produced by `role_attacks`, so this pass emits only the moves
                    // that touch an intermediate square: the igui, and every
                    // two-step path that **captures** on either leg (a non-capturing
                    // two-step coincides with a jump and is not emitted again).
                    for &d1 in &DIRS8 {
                        let Some(s1) = from.offset(d1.0, d1.1) else {
                            continue;
                        };
                        if our_pieces.contains(s1) {
                            // Cannot step onto a friendly piece (only jump over it,
                            // which the leaper already covers).
                            continue;
                        }
                        let s1_enemy = their_pieces.contains(s1);
                        if s1_enemy {
                            // igui: capture the adjacent enemy and return home.
                            out.push(WideMove::lion(from, from, s1, true, false));
                        } else {
                            // An empty adjacent square: the lion can step there and
                            // back, so the jitto pass is available.
                            can_pass = true;
                        }
                        // The second King-step, from s1 to any non-friendly square
                        // (other than the origin — that is the igui above).
                        //
                        // * When the first step **captured** (`s1` enemy), the lion
                        //   may continue to any square, at distance one or two — the
                        //   capture makes the whole two-step move distinct from a
                        //   plain move to `s2`.
                        // * When the first step was onto an **empty** square, the
                        //   two-step move is only distinct if it captures on the
                        //   second step **and** lands at distance two: reaching an
                        //   adjacent (distance-one) square through an empty
                        //   intermediate is the same move as the plain step to it, so
                        //   it is not re-emitted. HaChu enumerates one such move per
                        //   intermediate path, matching this (a distance-two capture
                        //   through two elbow squares is two distinct area moves, plus
                        //   the leaper's direct jump).
                        for &d2 in &DIRS8 {
                            let Some(s2) = s1.offset(d2.0, d2.1) else {
                                continue;
                            };
                            if s2 == from || our_pieces.contains(s2) {
                                continue;
                            }
                            let s2_enemy = their_pieces.contains(s2);
                            let emit = if s1_enemy {
                                true
                            } else {
                                s2_enemy && chebyshev::<G>(from, s2) == 2
                            };
                            if emit {
                                out.push(WideMove::lion(from, s2, s1, s1_enemy, s2_enemy));
                            }
                        }
                    }
                } else {
                    // A line-lion (Horned Falcon / Soaring Eagle): lion power only
                    // straight along each of its lion-power lines, no turning.
                    for &line in lines {
                        let (df, dr) = if us.is_white() {
                            line
                        } else {
                            (line.0, -line.1)
                        };
                        let Some(s1) = from.offset(df, dr) else {
                            continue;
                        };
                        if our_pieces.contains(s1) {
                            // Cannot step onto a friendly first square (only the
                            // leaper's jump over it, already covered).
                            continue;
                        }
                        let s1_enemy = their_pieces.contains(s1);
                        if s1_enemy {
                            // igui: capture the enemy on the line and return home.
                            out.push(WideMove::lion(from, from, s1, true, false));
                        } else {
                            can_pass = true;
                        }
                        // The straight second step (no turning). Emit the two-step
                        // move only when it captures on a leg — a non-capturing
                        // straight double-step coincides with the leaper's jump.
                        if let Some(s2) = s1.offset(df, dr) {
                            let s2_enemy = their_pieces.contains(s2);
                            if !our_pieces.contains(s2) && (s1_enemy || s2_enemy) {
                                out.push(WideMove::lion(from, s2, s1, s1_enemy, s2_enemy));
                            }
                        }
                    }
                }
                if can_pass {
                    // The jitto pass: step to an empty adjacent square and back, a
                    // net-zero move that only passes the turn. One per piece, with
                    // an inert mid square (no capture).
                    out.push(WideMove::lion(from, from, from, false, false));
                }
            }
        }
    }

    /// Generates the side-to-move's **Berolina** pawn moves (the Spartan
    /// Hoplite): a diagonal-forward quiet advance (two squares from the start
    /// rank), a straight-forward capture, and last-rank promotion. There is no
    /// en passant (FSF's Spartan sets no ep target for the Hoplite double move).
    ///
    /// The `check_mask` / `pins` are accepted for symmetry with the standard
    /// generator but are full / empty under the multi-king path (the caller
    /// filters each move for self-check), so this stays a pure pseudo-legal
    /// emitter there.
    #[allow(clippy::too_many_arguments)]
    fn gen_berolina_moves<S: WideSink>(
        &self,
        out: &mut S,
        us: Color,
        occupied: Bitboard<G>,
        their_pieces: Bitboard<G>,
        check_mask: Bitboard<G>,
        pins: &Pins<G>,
    ) {
        let board = &self.board;
        let hoplites = board.pieces(us, WideRole::Hoplite);
        if hoplites.is_empty() {
            return;
        }
        let forward: i8 = if us.is_white() { 1 } else { -1 };
        let start_rank = V::double_push_rank(us);
        let promo_roles = V::promotion_targets(us, board);

        for from in hoplites {
            let pin_line = pins.line_of(from);

            // Quiet advance along each forward diagonal. The single step requires
            // its own landing square empty. The double step (only from the start
            // rank) is a **jump**: it requires only its landing square empty — the
            // intervening square may be occupied (FSF's Spartan Hoplite leaps it).
            for one in V::berolina_push_targets(us, from) {
                let df = (one.file() as i8) - (from.file() as i8);
                if !occupied.contains(one) && check_mask.contains(one) && pin_line.contains(one) {
                    Self::emit_pawn_dest(out, from, one, &promo_roles, false, us);
                }
                if from.rank() == start_rank {
                    if let Some(two) = from.offset(2 * df, 2 * forward) {
                        if !occupied.contains(two)
                            && check_mask.contains(two)
                            && pin_line.contains(two)
                        {
                            // A Hoplite double advance creates **no en-passant
                            // target** (FSF's Spartan sets none), so it is a plain
                            // quiet move, not a `DoublePawnPush` — `apply` then
                            // leaves `ep_square` clear.
                            out.push(WideMove::new(from, two, WideMoveKind::Quiet));
                        }
                    }
                }
            }

            // Capture: one square straight forward onto an enemy piece.
            if let Some(cap) = from.offset(0, forward) {
                if their_pieces.contains(cap) && check_mask.contains(cap) && pin_line.contains(cap)
                {
                    Self::emit_pawn_dest(out, from, cap, &promo_roles, true, us);
                }
            }
        }
    }

    /// Emits a pawn/Hoplite move to `to`, expanding to the promotion roles when
    /// `to` is in the promotion zone, otherwise a single quiet or capture move.
    fn emit_pawn_dest<S: WideSink>(
        out: &mut S,
        from: Square<G>,
        to: Square<G>,
        promo_roles: &[WideRole],
        capture: bool,
        us: Color,
    ) {
        if V::in_promotion_zone(us, to.rank()) {
            for &role in promo_roles {
                out.push(WideMove::new(
                    from,
                    to,
                    WideMoveKind::Promotion { role, capture },
                ));
            }
        } else if capture {
            out.push(WideMove::new(from, to, WideMoveKind::Capture));
        } else {
            out.push(WideMove::new(from, to, WideMoveKind::Quiet));
        }
    }

    /// Emits a single non-pawn piece move from `from` to `to` (Chak's King /
    /// Soldier), expanding it into the promotion form(s) when `to` is in the
    /// promotion zone. A capture/quiet split is given by `capture`. Used by the
    /// multi-royal pseudo generator under [`WideVariant::has_piece_promotion`].
    ///
    /// When the destination is in the zone the piece promotes to
    /// [`WideVariant::role_promoted_to`]; the non-promoting alternative is emitted
    /// too unless promotion is [`WideVariant::promotion_mandatory_in_zone`] there.
    fn emit_piece_promotion_one<S: WideSink>(
        out: &mut S,
        role: WideRole,
        from: Square<G>,
        to: Square<G>,
        capture: bool,
        us: Color,
    ) {
        if V::in_promotion_zone(us, from.rank()) || V::in_promotion_zone(us, to.rank()) {
            out.push(WideMove::new(
                from,
                to,
                WideMoveKind::Promotion {
                    role: V::role_promoted_to(role),
                    capture,
                },
            ));
            if !V::promotion_mandatory_in_zone() {
                let kind = if capture {
                    WideMoveKind::Capture
                } else {
                    WideMoveKind::Quiet
                };
                out.push(WideMove::new(from, to, kind));
            }
        } else {
            let kind = if capture {
                WideMoveKind::Capture
            } else {
                WideMoveKind::Quiet
            };
            out.push(WideMove::new(from, to, kind));
        }
    }

    /// Returns `true` if color `who` has a **Divine Lord** standing on its goal
    /// **temple square** — the Chak win condition (FSF `flagPiece = d`,
    /// `flagRegion…`). Only meaningful while [`WideVariant::has_temple_win`] is
    /// `true`; the caller gates on it.
    fn temple_win_reached(&self, who: Color) -> bool {
        !(self.board.pieces(who, WideRole::DivineLord) & V::temple_goal(who)).is_empty()
    }

    /// Emits the moves of a promotable hand-variant piece (Shogi) from `from` to
    /// each square in `targets`: a move that **starts or ends in the promotion
    /// zone** may promote, and on the squares where the piece would otherwise have
    /// no legal move (a Pawn/Lance last rank, a Knight last two ranks) it **must**.
    ///
    /// Each non-zone move stays a plain quiet / capture; each zone move emits the
    /// promotion (to the role's promoted form), plus the non-promoting variant
    /// when promotion is optional there. The capture / quiet split is read from
    /// `their_pieces`, exactly as [`WideSink::emit_targets`].
    fn emit_promotable_targets<S: WideSink>(
        &self,
        out: &mut S,
        role: WideRole,
        from: Square<G>,
        targets: Bitboard<G>,
        their_pieces: Bitboard<G>,
        us: Color,
    ) {
        let from_in_zone = V::in_promotion_zone(us, from.rank());
        let promoted = V::role_promoted_to(role);
        let mandatory = V::promotion_mandatory_in_zone();
        // Shogun's per-piece promotion limit (FSF `promotionLimit`): while the side
        // already holds the cap of this role's promoted form on the board, the
        // promoting move is suppressed and only the plain move is emitted. Inert
        // (default `false`) for Shogi / Shinobi, so they stay byte-identical. The
        // limit never bites a *forced* promotion in any variant that uses both: a
        // Shogun piece capped at its promoted form is never one that would be
        // immobile without promoting (only the uncapped-Commoner Pawn is forced).
        let limited = V::role_promotion_blocked_by_limit(role, us, &self.board);
        for to in targets {
            let capture = their_pieces.contains(to);
            let to_rank = to.rank();
            let to_in_zone = V::in_promotion_zone(us, to_rank);
            let forced = V::role_promotion_forced(role, us, to_rank);
            // Whether promotion is offered here, and whether it is compulsory.
            let (promo_allowed, must_promote) = if V::lion_style_promotion() {
                // The Chu-Shogi rule as HaChu applies it (its default
                // "Promote on entry"): a piece promotes exactly when it **enters**
                // the promotion zone from outside, and promotion is then
                // **mandatory** — there is no non-promoting alternative. A move that
                // stays within, leaves, or captures within the zone never promotes.
                // A Pawn / Lance that would otherwise be immobile on the furthest
                // rank (`forced`) also promotes, and must.
                let entering = to_in_zone && !from_in_zone;
                let may = entering || forced;
                (may, may)
            } else {
                // The standard "starts or ends in the zone" rule. Compulsory when
                // promotion is mandatory (Shinobi) or the piece would otherwise be
                // immobile (Shogi's forced promotion).
                let in_zone = from_in_zone || to_in_zone;
                (in_zone, in_zone && (mandatory || forced))
            };
            if promo_allowed {
                if !limited {
                    out.push(WideMove::new(
                        from,
                        to,
                        WideMoveKind::Promotion {
                            role: promoted,
                            capture,
                        },
                    ));
                }
                // The non-promoting alternative, unless promotion is compulsory
                // here. When the promotion is suppressed by the limit the plain
                // move is always available.
                if limited || !must_promote {
                    let kind = if capture {
                        WideMoveKind::Capture
                    } else {
                        WideMoveKind::Quiet
                    };
                    out.push(WideMove::new(from, to, kind));
                }
            } else {
                let kind = if capture {
                    WideMoveKind::Capture
                } else {
                    WideMoveKind::Quiet
                };
                out.push(WideMove::new(from, to, kind));
            }
        }
    }

    /// Generates the side-to-move's **hand drops** (Shogi): for each base role in
    /// hand, a [`WideMove::drop`] onto every square the variant permits
    /// ([`WideVariant::drop_targets`] — already excluding dead squares and nifu
    /// files), each filtered for self-check, and — for the pawn-drop role under
    /// [`WideVariant::pawn_drop_mate_forbidden`] — suppressed when the drop is
    /// checkmate (*uchifuzume*).
    ///
    /// A drop never exposes the dropping side's own king to a *new* discovered
    /// check (it adds a friendly blocker to an empty square and moves nothing), so
    /// the only self-check a drop must avoid is **failing to block an existing
    /// check** — handled by the check mask: while in check, a drop is legal only on
    /// a square that blocks the (single) checker. Out of check, every permitted
    /// target is self-check-safe. Reached only while [`WideVariant::has_hand`] is
    /// `true`.
    fn gen_hand_drops<S: WideSink>(&self, out: &mut S) {
        let us = self.state.turn;
        let board = &self.board;
        let them = us.opposite();
        let occupied = board.occupied();

        // The check mask: a drop must resolve a single check by interposing on a
        // square between the king and the (single) checker. Under double check no
        // drop helps (only a king move), and a drop can never capture the checker.
        // A **non-royal** king (Dobutsu's Lion) is never in check, so a drop is
        // never required to resolve one: the mask is the whole board. Gated behind
        // `non_royal_king()` (default-off), so every royal-king hand variant keeps
        // the byte-identical check-resolving drop mask.
        let drop_mask = if V::non_royal_king() {
            Bitboard::FULL
        } else if V::has_cannons() {
            // A cannon resolves a check differently from a slider, so the between /
            // double-check optimisation below is **unsound** for a cannon variant: a
            // single interposed piece both blocks a slider and becomes a cannon's new
            // over-screen target (shielding the king), so even a "double check" by a
            // rook and a cannon firing along one line is answered by a single drop on
            // that line. The cannon-verify path is the only caller while `has_cannons`
            // is set, and it re-checks every drop's king safety individually
            // ([`cannon_move_is_legal`]), so generating the full drop-target superset
            // here and letting the verify filter it is both correct and the simplest
            // sound rule. (Inert for every non-cannon hand variant, which keeps the
            // byte-identical between-squares mask below.)
            Bitboard::FULL
        } else {
            match board.king_of(us) {
                Some(king_sq) => {
                    let checkers = self.attackers_to(king_sq, them, occupied);
                    match checkers.count() {
                        0 => Bitboard::FULL,
                        // A drop cannot capture, so only an interposition resolves a
                        // single check — and only when the checker is a **slider**. A
                        // leaper that checks along a line (Tori's Pheasant jumping two
                        // squares straight to the king) cannot be blocked, so `between`
                        // must not offer the intervening square as a (false) drop
                        // block. Mirrors the move-generation check mask's
                        // [`role_is_slider`](WideVariant::role_is_slider) gate; for a
                        // non-collinear leaper `between` is empty either way, so this
                        // is byte-identical for every existing hand variant.
                        1 => {
                            let checker = checkers.lsb().expect("one checker");
                            if self.board.role_at(checker).is_some_and(V::role_is_slider) {
                                between(king_sq, checker)
                            } else {
                                Bitboard::EMPTY
                            }
                        }
                        // Double check: no drop is legal.
                        _ => return,
                    }
                }
                None => Bitboard::FULL,
            }
        };

        let pawn_role = V::pawn_drop_role();
        let check_uchifuzume = V::pawn_drop_mate_forbidden();
        // Kyoto Shogi: a held (base) piece may be deployed in either its base or
        // its promoted form (FSF `dropPromoted`). Default-off, so every other hand
        // variant emits a single base-form drop per square and stays byte-identical.
        let drops_can_promote = V::drops_can_promote();
        for role in WideRole::ALL {
            if self.state.placement.count(us, role) == 0 {
                continue;
            }
            let targets = V::drop_targets(role, us, board) & drop_mask;
            for sq in targets {
                let mv = WideMove::drop(role, sq);
                // Uchifuzume: a pawn drop that checkmates the opponent is illegal.
                // Only test pawn drops that actually give check, on the rare drop
                // that does — the mate test (a full child legal-move generation) is
                // skipped for every non-checking pawn drop and every other piece.
                if check_uchifuzume && role == pawn_role && self.pawn_drop_is_mate(sq) {
                    continue;
                }
                out.push(mv);
            }
            // The promoted-form deployment of the same held piece. It rides the
            // same drop targets (Kyoto imposes no per-form drop restriction —
            // `immobilityIllegal` is off, so a promoted Pawn may sit on the last
            // rank), drops nothing extra from hand (the base leaves the pocket on
            // apply), and is suppressed for every variant where the (base) held
            // role has no alternate form. The alternate form is the role's
            // `flips_on_move` target, the per-move flip mechanic that also defines
            // the dual-form drop.
            if drops_can_promote {
                // The alternate form is the role's per-move flip target (Kyoto) or,
                // where the variant flips only on capture (Micro Shogi), its
                // capture-flip target — both name the held base role's second form.
                if let Some(promoted) = V::flips_on_move(role).or_else(|| V::flips_on_capture(role))
                {
                    let targets = V::drop_targets(promoted, us, board) & drop_mask;
                    for sq in targets {
                        out.push(WideMove::drop(promoted, sq));
                    }
                }
            }
        }
    }

    /// Returns `true` if dropping a pawn of `us` on `sq` delivers immediate
    /// checkmate to the opponent (the *uchifuzume* condition). The drop is applied
    /// to a clone and the opponent is checkmate iff it is in check with no legal
    /// reply.
    fn pawn_drop_is_mate(&self, sq: Square<G>) -> bool {
        let mv = WideMove::drop(V::pawn_drop_role(), sq);
        let next = self.play(&mv);
        // `next` is the opponent to move. Checkmate = in check and no legal move.
        next.is_check() && next.legal_moves().is_empty()
    }

    /// Appends any variant drop moves (reserved; standard chess emits none).
    ///
    /// The [`WideVariant::emit_drops`] hook writes into a `Vec<WideMove>`; no
    /// variant overrides it yet, so the temporary is always empty and this is a
    /// no-op on every path (the standard generator's [`WideSink`] never sees a
    /// drop). When a drop variant lands, its moves are forwarded into the sink.
    fn append_drops<S: WideSink>(&self, out: &mut S) {
        // Shogi-style persistent hand drops, generated with full legality (dead
        // squares, nifu, check-blocking, uchifuzume). Gated behind `has_hand()`
        // (default-off), so every other variant skips it.
        if V::has_hand() {
            self.gen_hand_drops(out);
        }
        // The reserved `emit_drops` hook (no variant overrides it yet).
        let mut drops: Vec<WideMove> = Vec::new();
        V::emit_drops(&self.board, &self.state, &mut drops);
        for mv in drops {
            out.push(mv);
        }
    }

    /// Generates the side-to-move's **placement-phase** drops (Sittuyin): for
    /// each role still in hand, a [`WideMove::drop`] onto every square the variant
    /// permits ([`WideVariant::placement_targets`]). FSF applies no check filter
    /// during placement, so the drops are emitted directly. Only reached while
    /// [`WideVariant::has_placement`] is `true` and the side's pocket is
    /// non-empty.
    fn generate_placement_into<S: WideSink>(&self, out: &mut S) {
        let us = self.state.turn;
        for role in WideRole::ALL {
            if self.state.placement.count(us, role) == 0 {
                continue;
            }
            let targets = V::placement_targets(role, us, &self.board);
            for sq in targets {
                out.push(WideMove::drop(role, sq));
            }
        }
    }

    /// For every base move already in `out` that vacates a gating-eligible
    /// square, appends the gated variants (one per available reserve piece). A
    /// castling move vacates two eligible squares (the king's origin and the
    /// castling rook's origin); each may host a gate, but never both at once.
    ///
    /// The base moves stay in the list (gating is optional), so this only *adds*
    /// the gated alternatives. It reads only the eligible set and the reserves
    /// from the gating state.
    fn append_gating_moves<S: WideSink>(&self, out: &mut S, us: Color) {
        let gating = self.state.gating;
        let eligible = gating.eligible();
        if eligible.is_empty() {
            return;
        }
        // S-House draws the gated piece from the crazyhouse hand (any held non-pawn,
        // non-king role) and emits the wider hand-gate encoding; Seirawan draws it
        // from the fixed Hawk/Elephant reserve.
        if V::gates_from_hand() {
            self.append_hand_gating_moves(out, us, eligible);
            return;
        }
        if !gating.any_reserve(us) {
            return;
        }
        let reserves: Vec<GateRole> = [GateRole::Hawk, GateRole::Elephant]
            .into_iter()
            .filter(|&r| gating.has_reserve(us, r))
            .collect();

        // Snapshot the base-move count: only the moves present before this pass
        // are gating candidates (we must not gate an already-gated move).
        let base_len = out.len();
        for i in 0..base_len {
            let mv = out.get(i);
            if mv.is_castle() {
                // Castling vacates the king origin (mv.from) and the rook origin.
                let king_from = mv.from::<G>();
                let side = if matches!(mv.kind(), WideMoveKind::CastleKingside) {
                    KINGSIDE
                } else {
                    QUEENSIDE
                };
                let rook_from = self
                    .state
                    .castling
                    .rook_file(us, side)
                    .and_then(|f| Square::<G>::from_file_rank(f, back_rank::<G>(us)));
                for &r in &reserves {
                    if eligible.contains(king_from) {
                        out.push(mv.with_gate(r, GateSquare::Origin));
                    }
                    if let Some(rook_from) = rook_from {
                        if eligible.contains(rook_from) {
                            out.push(mv.with_gate(r, GateSquare::RookOrigin));
                        }
                    }
                }
            } else {
                let from = mv.from::<G>();
                if eligible.contains(from) {
                    for &r in &reserves {
                        out.push(mv.with_gate(r, GateSquare::Origin));
                    }
                }
            }
        }
    }

    /// The S-House counterpart of [`append_gating_moves`](Self::append_gating_moves):
    /// for every base move vacating a gating-eligible square, appends one hand-gate
    /// per **held** non-pawn, non-king role (drawn from the crazyhouse hand). Pawns
    /// and the king are never gated (FSF), and a role absent from the hand emits no
    /// gate.
    fn append_hand_gating_moves<S: WideSink>(&self, out: &mut S, us: Color, eligible: Bitboard<G>) {
        let roles: Vec<WideRole> = WideRole::ALL
            .into_iter()
            .filter(|&r| {
                r != WideRole::Pawn && r != WideRole::King && self.state.placement.count(us, r) > 0
            })
            .collect();
        if roles.is_empty() {
            return;
        }
        let base_len = out.len();
        for i in 0..base_len {
            let mv = out.get(i);
            if mv.is_castle() {
                let king_from = mv.from::<G>();
                let side = if matches!(mv.kind(), WideMoveKind::CastleKingside) {
                    KINGSIDE
                } else {
                    QUEENSIDE
                };
                let rook_from = self
                    .state
                    .castling
                    .rook_file(us, side)
                    .and_then(|f| Square::<G>::from_file_rank(f, back_rank::<G>(us)));
                for &r in &roles {
                    if eligible.contains(king_from) {
                        out.push(mv.with_hand_gate::<G>(r, GateSquare::Origin));
                    }
                    if let Some(rook_from) = rook_from {
                        if eligible.contains(rook_from) {
                            out.push(mv.with_hand_gate::<G>(r, GateSquare::RookOrigin));
                        }
                    }
                }
            } else {
                let from = mv.from::<G>();
                if eligible.contains(from) {
                    for &r in &roles {
                        out.push(mv.with_hand_gate::<G>(r, GateSquare::Origin));
                    }
                }
            }
        }
    }

    /// Generates the side-to-move's pawn moves: single and double pushes,
    /// diagonal captures, en passant, and promotions, under the check mask and
    /// pins.
    #[allow(clippy::too_many_arguments)]
    fn gen_pawn_moves<S: WideSink>(
        &self,
        out: &mut S,
        us: Color,
        occupied: Bitboard<G>,
        their_pieces: Bitboard<G>,
        check_mask: Bitboard<G>,
        pins: &Pins<G>,
        king_sq: Square<G>,
    ) {
        let board = &self.board;
        let pawns = board.pieces(us, WideRole::Pawn);
        if pawns.is_empty() {
            return;
        }
        let forward: i8 = if us.is_white() { 1 } else { -1 };
        let start_rank = V::double_push_rank(us);
        // The legal promotion targets. The default reads only the static
        // promotion config (every existing variant), so it is unchanged for them;
        // Grand chess overrides it to a board-dependent set, so it is recomputed
        // per generation from the live board.
        let promo_roles = V::promotion_targets(us, board);

        for from in pawns {
            let pin_line = pins.line_of(from);

            // Single (and double) push.
            if let Some(one) = from.offset(0, forward) {
                if !occupied.contains(one) {
                    if check_mask.contains(one) && pin_line.contains(one) {
                        if V::in_promotion_zone(us, one.rank()) {
                            for &role in &promo_roles {
                                out.push(WideMove::new(
                                    from,
                                    one,
                                    WideMoveKind::Promotion {
                                        role,
                                        capture: false,
                                    },
                                ));
                            }
                            // In a multi-rank promotion zone (Grand) a near-rank
                            // push may also stay a pawn; the last rank forces it.
                            if !V::promotion_is_forced(us, one.rank()) {
                                out.push(WideMove::new(from, one, WideMoveKind::Quiet));
                            }
                        } else {
                            out.push(WideMove::new(from, one, WideMoveKind::Quiet));
                        }
                    }
                    if from.rank() == start_rank {
                        if let Some(two) = from.offset(0, 2 * forward) {
                            if !occupied.contains(two)
                                && check_mask.contains(two)
                                && pin_line.contains(two)
                            {
                                out.push(WideMove::new(from, two, WideMoveKind::DoublePawnPush));
                            }
                        }
                    }
                }
            }

            // Diagonal captures (and capturing promotions).
            let caps = V::role_attacks(WideRole::Pawn, us, from, occupied) & their_pieces;
            for to in caps {
                if !check_mask.contains(to) || !pin_line.contains(to) {
                    continue;
                }
                if V::in_promotion_zone(us, to.rank()) {
                    for &role in &promo_roles {
                        out.push(WideMove::new(
                            from,
                            to,
                            WideMoveKind::Promotion {
                                role,
                                capture: true,
                            },
                        ));
                    }
                    // Optional-promotion zone (Grand): a capture onto a near zone
                    // rank may also be a plain capture; the last rank forces it.
                    if !V::promotion_is_forced(us, to.rank()) {
                        out.push(WideMove::new(from, to, WideMoveKind::Capture));
                    }
                } else {
                    out.push(WideMove::new(from, to, WideMoveKind::Capture));
                }
            }

            // Special promotion (Sittuyin): when the side has no Met on the
            // board, a pawn may become a Met in place or by a one-step ferz move
            // to an empty diagonal square. Gated behind `has_placement()`
            // (default-off), so every other variant skips this entirely and is
            // byte-identical. Each landing square is filtered by the same check
            // mask and pin line as every other move; an in-place promotion
            // (`to == from`) stays on the pin line and is legal only out of check
            // (the check mask never contains a friendly pawn's own square).
            if V::has_placement() {
                if let Some(targets) = V::special_promotion_targets(board, from, us) {
                    let met = promo_roles.first().copied().unwrap_or(WideRole::Met);
                    for to in targets {
                        if check_mask.contains(to) && pin_line.contains(to) {
                            out.push(WideMove::new(
                                from,
                                to,
                                WideMoveKind::Promotion {
                                    role: met,
                                    capture: false,
                                },
                            ));
                        }
                    }
                }
            }
        }

        // En passant.
        if let Some(ep) = self.state.ep_square {
            // The en-passant landing square is normally empty (the enemy pawn
            // skipped it), but in Duck chess the neutral Duck may sit on it; the
            // duck is part of `occupied`, so a blocked ep square forbids the
            // capture. (For non-duck variants `ep` is never occupied, so this is a
            // no-op.)
            if !occupied.contains(ep) {
                // A pawn that attacks the ep square may take. The captured pawn
                // sits on the ep file, on the capturing pawn's rank.
                let takers = V::role_attacks(WideRole::Pawn, us.opposite(), ep, occupied) & pawns;
                for from in takers {
                    let pin_line = pins.line_of(from);
                    let captured = match Square::<G>::from_file_rank(ep.file(), from.rank()) {
                        Some(sq) => sq,
                        None => continue,
                    };
                    let resolves_check = check_mask.contains(ep) || check_mask.contains(captured);
                    let ep_pin_ok = self.ep_is_legal(us, from, ep, captured, king_sq);
                    if resolves_check && pin_line.contains(ep) && ep_pin_ok {
                        out.push(WideMove::new(from, ep, WideMoveKind::EnPassant));
                    }
                }
            }
        }
    }

    /// Verifies an en-passant capture does not expose our king to a horizontal
    /// (or diagonal) slider once both the capturing and captured pawns leave.
    fn ep_is_legal(
        &self,
        us: Color,
        from: Square<G>,
        ep: Square<G>,
        captured: Square<G>,
        king_sq: Square<G>,
    ) -> bool {
        // Variants whose king is not royal (Duck) have no check, so an en-passant
        // capture can never be illegal for exposing the king. Skipping the test
        // keeps the byte-identical standard path for every royal-king variant.
        if V::royal_squares(&self.board, us).is_empty() {
            return true;
        }
        let them = us.opposite();
        let occ = self
            .board
            .occupied()
            .without(from)
            .without(captured)
            .with(ep);
        // Any enemy slider that now attacks the king through the freed squares
        // makes the capture illegal. We test every enemy sliding role.
        for role in WideRole::ALL {
            if !V::role_is_slider(role) {
                continue;
            }
            for slider in self.board.pieces(them, role) {
                if slider == captured || slider == from {
                    continue;
                }
                if V::role_attacks(role, them, slider, occ).contains(king_sq) {
                    return false;
                }
            }
        }
        true
    }

    /// Generates standard castling moves (king on the home e-file, rook on its
    /// recorded start file), gated by the not-in-check / clear-path / safe-walk
    /// conditions.
    fn gen_castles<S: WideSink>(
        &self,
        out: &mut S,
        us: Color,
        occupied: Bitboard<G>,
        king_danger: Bitboard<G>,
        king_sq: Square<G>,
    ) {
        if !self.state.castling.has_any(us) {
            return;
        }
        let rank = V::castle_rank(us);
        if king_sq.rank() != rank {
            return;
        }
        // A royal king gets the post-castle discovered-check test below; a non-royal
        // king (Duck) has no check and must not, matching its empty `king_danger`.
        let royal = !V::royal_squares(&self.board, us).is_empty();

        // Castle destinations come from the variant's `castle_dest_files` hook.
        // The default is the standard 8x8 geometry (kingside king to file 6 / g
        // and rook to 5 / f; queenside king to 2 / c and rook to 3 / d); a wide
        // variant like Capablanca overrides it for its own king/rook files.
        let (k_king, k_rook) = V::castle_dest_files(KINGSIDE);
        let (q_king, q_rook) = V::castle_dest_files(QUEENSIDE);
        for (side, king_dest_file, rook_dest_file, kind) in [
            (KINGSIDE, k_king, k_rook, WideMoveKind::CastleKingside),
            (QUEENSIDE, q_king, q_rook, WideMoveKind::CastleQueenside),
        ] {
            let Some(rook_file) = self.state.castling.rook_file(us, side) else {
                continue;
            };
            let rook_from = match Square::<G>::from_file_rank(rook_file, rank) {
                Some(sq) => sq,
                None => continue,
            };
            if self.board.piece_at(rook_from) != Some(WidePiece::new(us, WideRole::Rook)) {
                continue;
            }
            let Some(king_dest) = Square::<G>::from_file_rank(king_dest_file, rank) else {
                continue;
            };
            let Some(rook_dest) = Square::<G>::from_file_rank(rook_dest_file, rank) else {
                continue;
            };

            let king_path = between(king_sq, king_dest).with(king_dest);
            let rook_path = between(rook_from, rook_dest).with(rook_dest);
            let must_be_empty = (king_path | rook_path).without(king_sq).without(rook_from);
            if !(must_be_empty & occupied).is_empty() {
                continue;
            }

            let king_walk = between(king_sq, king_dest).with(king_dest);
            if !(king_walk & king_danger).is_empty() {
                continue;
            }

            // Cannon-aware transit safety (Shako and any cannon-castling variant).
            // `king_danger` is built by projecting each enemy piece *forward*
            // ([`attacked_by`](Self::attacked_by)), which for a cannon yields only
            // its current capture target — the first piece beyond its screen. An
            // **empty** transit square the king would step onto is skipped by that
            // projection (the cannon's ray runs through it to the next occupant), so
            // the map misses a cannon that *would* capture the king were it standing
            // there. The cannon is also `role_attack_is_leg_asymmetric`, so
            // [`attackers_to`](Self::attackers_to) forward-projects it too and shows
            // no attacker on an empty square — by design, to avoid a phantom on a
            // square a cannon cannot actually capture. So re-test each
            // strictly-between transit square with the **king placed on it** (and
            // lifted from its origin, matching the `king_danger` convention): the
            // cannon's forward projection then lands its over-screen capture on the
            // occupied transit square, exactly the danger the king walks into. A
            // plain slider's attack on the square is unaffected by the king sitting
            // on its endpoint, so this stays correct for non-cannon attackers too.
            // The destination square is covered by the post-castle `royal` test
            // below. Gated behind `has_cannons()` (default-off), so every non-cannon
            // castling variant (standard, Capablanca, Seirawan, …) is byte-identical,
            // and the loop body is reached only for an actual castle candidate
            // (castling rights present), so cannon variants without castling
            // (Xiangqi, Janggi) never run it.
            if V::has_cannons() {
                let mut transit_attacked = false;
                for s in between(king_sq, king_dest) {
                    let occ_at_s = occupied.without(king_sq).with(s);
                    if !self.attackers_to(s, us.opposite(), occ_at_s).is_empty() {
                        transit_attacked = true;
                        break;
                    }
                }
                if transit_attacked {
                    continue;
                }
            }

            // The king must also be safe on its destination under the *post*-castle
            // occupancy: king and rook off their start squares with the rook on its
            // destination. In a randomised (Chess960 / Shredder-X-FEN) back rank a
            // castling rook can shield the king's landing square, so its departure
            // may open a discovered check that `king_danger` — computed with the
            // rook still on its home square — misses. For the standard corner-rook
            // geometries (a/h on chess, a/j on Capablanca) no castling rook ever
            // stands between an enemy slider and an interior landing square, so this
            // test rejects nothing there and every non-randomised path stays
            // byte-identical. Skipped for a non-royal king (Duck), which has no
            // check at all and is called with an empty `king_danger`.
            if royal {
                let after = occupied.without(king_sq).without(rook_from).with(rook_dest);
                if !self
                    .attackers_to(king_dest, us.opposite(), after)
                    .is_empty()
                {
                    continue;
                }
            }

            out.push(WideMove::new(king_sq, king_dest, kind));
        }
    }

    /// Generates the Cambodian one-time first-move leaps (`has_first_move_leaps()`
    /// variants only).
    ///
    /// The two leap rights are carried in the [`GenericCastling`] field: the
    /// **kingside** slot holds the king's leap right and the **queenside** slot the
    /// queen/Met's, each keyed by its piece's home file on the back rank. A leap is
    /// offered only while its right is present *and* its piece still stands on that
    /// home square; both the right (revoked on the piece's first move) and the
    /// home-square check guard against re-use.
    ///
    /// * **King leap** — the forward-knight squares ([`king_leap_offsets`]). It
    ///   jumps any intervening piece, lands only on an empty square outside the
    ///   king-danger map, and (like castling) is offered only when not in check.
    /// * **Met leap** — the two-square straight advance ([`met_leap_offsets`]). An
    ///   ordinary quiet piece move: it jumps the square in front, lands only on an
    ///   empty square, and is confined by the check mask and the Met's pin line.
    ///
    /// [`king_leap_offsets`]: WideVariant::king_leap_offsets
    /// [`met_leap_offsets`]: WideVariant::met_leap_offsets
    #[allow(clippy::too_many_arguments)]
    fn gen_first_move_leaps<S: WideSink>(
        &self,
        out: &mut S,
        us: Color,
        occupied: Bitboard<G>,
        our_pieces: Bitboard<G>,
        king_danger: Bitboard<G>,
        king_sq: Square<G>,
        num_checkers: u32,
        check_mask: Bitboard<G>,
        pins: &Pins<G>,
    ) {
        let rank = V::castle_rank(us);

        // King leap: offered only when not in check and the king sits on its home
        // square (the kingside-slot file on the castle rank). Targets are empty
        // squares clear of the king-danger map.
        if num_checkers == 0 {
            if let Some(home_file) = self.state.castling.rook_file(us, KINGSIDE) {
                if Square::<G>::from_file_rank(home_file, rank) == Some(king_sq) {
                    for &(df, dr) in V::king_leap_offsets(us) {
                        if let Some(dest) = king_sq.offset(df, dr) {
                            if !occupied.contains(dest) && !king_danger.contains(dest) {
                                out.push(WideMove::new(king_sq, dest, WideMoveKind::Quiet));
                            }
                        }
                    }
                }
            }
        }

        // Met leap: an ordinary piece move from the Met's home square (the
        // queenside-slot file). Confined by the check mask and the Met's pin line,
        // and landing only on an empty square.
        if let Some(home_file) = self.state.castling.rook_file(us, QUEENSIDE) {
            if let Some(met_home) = Square::<G>::from_file_rank(home_file, rank) {
                if self.board.piece_at(met_home) == Some(WidePiece::new(us, WideRole::Met)) {
                    let pin_line = pins.line_of(met_home);
                    for &(df, dr) in V::met_leap_offsets(us) {
                        if let Some(dest) = met_home.offset(df, dr) {
                            if !occupied.contains(dest)
                                && !our_pieces.contains(dest)
                                && check_mask.contains(dest)
                                && pin_line.contains(dest)
                            {
                                out.push(WideMove::new(met_home, dest, WideMoveKind::Quiet));
                            }
                        }
                    }
                }
            }
        }
    }

    // -- Make move ---------------------------------------------------------

    /// Applies `mv` and returns the resulting position. The move must be legal.
    #[must_use]
    pub fn play(&self, mv: &WideMove) -> Self {
        let mut next = self.clone();
        next.apply(mv);
        next
    }

    /// Applies `mv` to this position **in place**. The move must be legal.
    pub fn play_unchecked(&mut self, mv: &WideMove) {
        self.apply(mv);
    }

    /// Applies `mv` **in place** and returns an [`Undo`] that reverses it exactly
    /// — the zero-copy make half of make/unmake (issue #309).
    ///
    /// This is [`play_unchecked`](Self::play_unchecked) plus a reversal record: it
    /// performs the identical in-place edit (so the resulting position is
    /// byte-for-byte what `play_unchecked` and [`play`](Self::play) produce) while
    /// snapshotting, *before* the edit, the prior occupant of every board square
    /// the move writes and the prior non-board state. Pass the returned token to
    /// [`undo`](Self::undo) to restore the position.
    ///
    /// # Contract
    ///
    /// The move **must be legal** for this position, exactly as for
    /// [`play_unchecked`](Self::play_unchecked); this does not re-validate it. The
    /// returned [`Undo`] is valid only for undoing *this* move from the position
    /// this produced — make/unmake pairs must nest (last made, first unmade).
    pub fn apply_with_undo(&mut self, mv: &WideMove) -> Undo<G> {
        // Snapshot the scalar state, promoted mask, and both color masks before any
        // edit; `apply` then fills in the touched role masks (reusing its own piece
        // lookups, so no extra `role_at` scan) and performs the move.
        let mut undo = Undo {
            state: self.state,
            promoted: self.promoted,
            by_color: self.board.color_masks(),
            roles: [(0usize, Bitboard::<G>::EMPTY); MAX_UNDO_ROLES],
            role_count: 0,
        };
        self.apply_inner(mv, Some(&mut undo));
        undo
    }

    /// Reverses an [`apply_with_undo`](Self::apply_with_undo) **in place**,
    /// restoring the position to exactly what it was before the move — board,
    /// side to move, castling rights, en passant, gating, Duck, placement / hand
    /// pockets, clocks, pass counter, Alice plane mask, and promoted mask all
    /// byte-identical.
    ///
    /// # Contract
    ///
    /// `undo` must be the token from the matching
    /// [`apply_with_undo`](Self::apply_with_undo) call on the position this is the
    /// successor of, and make/unmake pairs must nest (last made, first unmade).
    /// Misuse leaves the position in an unspecified state; there is no validation.
    pub fn undo(&mut self, undo: Undo<G>) {
        // Restore the non-board state and promoted mask wholesale.
        self.state = undo.state;
        self.promoted = undo.promoted;
        // Restore the board by direct mask assignment — no per-square role scan.
        // The two color masks and every role mask the move touched are put back to
        // their pre-move values; every other role mask was untouched by `apply`.
        self.board.set_color_masks(undo.by_color);
        for &(index, mask) in &undo.roles[..undo.role_count] {
            self.board.set_role_mask(index, mask);
        }
    }

    /// Test support (issue #309): walks the legal-move tree to `depth`, asserting
    /// at every node that [`apply_with_undo`](Self::apply_with_undo) reaches the
    /// same position as [`play`](Self::play) and that the matching
    /// [`undo`](Self::undo) restores the position **byte-for-byte** — board, state,
    /// and promoted mask. `self` is byte-identical on entry and exit.
    #[cfg(test)]
    pub(crate) fn assert_make_unmake_walk(&mut self, depth: u32) {
        if depth == 0 {
            return;
        }
        let before = self.clone();
        for mv in before.legal_moves() {
            let expected = before.play(&mv);
            let undo = self.apply_with_undo(&mv);
            assert!(
                self.board == expected.board
                    && self.state == expected.state
                    && self.promoted == expected.promoted,
                "apply_with_undo({}) diverged from play() at {}",
                mv.to_uci::<G>(),
                before.to_fen(),
            );
            self.assert_make_unmake_walk(depth - 1);
            self.undo(undo);
            assert!(
                self.board == before.board
                    && self.state == before.state
                    && self.promoted == before.promoted,
                "undo({}) failed to restore {} (got {})",
                mv.to_uci::<G>(),
                before.to_fen(),
                self.to_fen(),
            );
        }
    }

    /// Applies `mv` to this position **in place** (the copy-make / no-undo path).
    /// The move must be legal.
    fn apply(&mut self, mv: &WideMove) {
        self.apply_inner(mv, None);
    }

    /// Applies `mv` in place. The move must be legal.
    ///
    /// `play`/`play_unchecked` pass `undo == None`; `apply_with_undo` passes a
    /// fresh `Undo` (with its scalar state and color masks already snapshotted),
    /// and this records the role masks the move touches as it goes — reusing the
    /// `moving`/captured piece lookups the move already performs, so the make/unmake
    /// path costs no extra `role_at` scan over plain `apply` on the common path.
    fn apply_inner(&mut self, mv: &WideMove, mut undo: Option<&mut Undo<G>>) {
        let us = self.state.turn;
        let them = us.opposite();
        let from = mv.from::<G>();
        let to = mv.to::<G>();
        // The rank a castle's rook sits on (back rank by default; Shako uses
        // rank 2). Only consulted by the castle arm below.
        let rank = V::castle_rank(us);

        // A drop has no origin piece (the square it names is empty before the
        // drop), so it is handled before the `from`-piece lookup the board moves
        // require. It places a held piece, advances the side and fullmove number,
        // and — in the placement phase — consumes the piece from the pocket. The
        // setup phase never resets nor advances the halfmove clock in FSF's
        // counting (it stays 0 through deployment), so leave it untouched.
        if let WideMoveKind::Drop { role } = mv.kind() {
            // A drop places exactly its role on an empty square; that is the only
            // role mask it edits, so it is all the undo needs to record.
            if let Some(u) = undo.as_mut() {
                u.touch(role, &self.board);
            }
            self.board.set_piece(to, WidePiece::new(us, role));
            // A dropped piece is never promoted (its `to` was empty), so clear any
            // stale promoted bit there. Gated, so non-demoting variants skip it.
            if V::demotes_promoted_captures() {
                self.promoted.clear(to);
            }
            if V::has_placement() || V::has_hand() {
                // The hand stores the **base** role; under `drops_can_promote`
                // (Kyoto) a piece may be deployed in its promoted form, but it is
                // the base it was banked as that leaves the pocket. For every other
                // variant the dropped role is already its own base
                // (`role_hand_base` is the identity there), so this is
                // byte-identical.
                self.state.placement.take(us, V::role_hand_base(role));
            }
            // Placement (Pre-Chess): a deployment that puts the king on its
            // castling file with a corner rook confers standard castling rights,
            // assigned incrementally as the pieces reach their squares. Gated
            // behind `placement_castling_king_file()` (default `None`), so every
            // other variant — including the no-castling placement variant
            // (Sittuyin) — leaves the rights untouched and is byte-identical.
            if let Some(king_file) = V::placement_castling_king_file() {
                self.derive_placement_castling(us, king_file);
            }
            self.state.ep_square = None;
            if us.is_black() {
                self.state.fullmove_number = self.state.fullmove_number.saturating_add(1);
            }
            self.state.turn = them;
            return;
        }

        let moving = self
            .board
            .piece_at(from)
            .expect("move originates from an occupied square");
        let is_pawn_move = moving.role == WideRole::Pawn;
        let mut reset_clock = is_pawn_move;

        // Record, before any board edit, the role masks this move will touch, so
        // `undo` can restore the board by direct assignment with no per-square scan.
        // This reuses the `moving` lookup above (no extra `from` scan); the only
        // added read is a capture victim's role, and only on the make/unmake path.
        // The set mirrors `apply`'s board writes exactly: the mover's role (carried,
        // captured-away, promoted, flipped, or revealed at `to`), a capture victim
        // (the destination occupant; en passant's victim is a Pawn, the same role as
        // the moving pawn, already recorded), a promotion's new role, a castled
        // `Rook`, a Kyoto flip / Jieqi reveal role, and a Seirawan / S-House gate.
        if let Some(u) = undo.as_mut() {
            u.touch(moving.role, &self.board);
            if mv.is_capture() && !matches!(mv.kind(), WideMoveKind::EnPassant) {
                if let Some(captured) = self.board.role_at(to) {
                    u.touch(captured, &self.board);
                }
            }
            // A Lion double capture / igui also removes a piece on its
            // intermediate square; record that victim's role so undo restores it.
            if mv.lion_captures_mid() {
                if let Some(mid) = mv.lion_mid::<G>() {
                    if let Some(captured) = self.board.role_at(mid) {
                        u.touch(captured, &self.board);
                    }
                }
            }
            if let Some(promo) = mv.kind().promotion() {
                u.touch(promo, &self.board);
            }
            if mv.kind().is_castle() {
                u.touch(WideRole::Rook, &self.board);
            }
            if let Some(flipped) = V::flips_on_move(moving.role) {
                u.touch(flipped, &self.board);
            }
            if mv.is_capture() {
                if let Some(flipped) = V::flips_on_capture(moving.role) {
                    u.touch(flipped, &self.board);
                }
            }
            if let Some(revealed) = V::reveal_on_move(moving.role, from) {
                u.touch(revealed, &self.board);
            }
            if let Some(gate) = mv.gate() {
                u.touch(gate.role(), &self.board);
            }
            if let Some(role) = mv.hand_gate() {
                u.touch(role, &self.board);
            }
        }

        // Alice chess: the plane (A = false / B = true) the mover starts on, read
        // before any board-membership edit. After the board move below the piece
        // **transfers** to the opposite plane (and a castled rook with it). Read
        // only on the Alice path (default-off), so every other variant is inert.
        let alice_from_plane = V::is_alice() && self.state.board_b.contains(from);

        self.state.ep_square = None;

        // The castling rook's origin, captured for the gating update below (a
        // castle vacates both the king's and the rook's squares).
        let mut castle_rook_from: Option<Square<G>> = None;

        match mv.kind() {
            WideMoveKind::Quiet => {
                // `from` holds `moving` and `to` is empty (a quiet move never
                // lands on a piece), so the masks can be edited directly without
                // re-scanning either square for an occupant.
                self.board.remove_known(from, moving);
                self.board.set_empty(to, moving);
            }
            WideMoveKind::Capture => {
                reset_clock = true;
                // A hand variant (Shogi) banks the captured piece — flipped to the
                // captor's side and reverted to its base role — before it is
                // overwritten. Default-off, so inert for every other variant.
                // Synochess and Shinobi both have a hand but a fixed pocket
                // (`captures_to_hand()` is `false`), so their captures bank nothing.
                if V::has_hand() && V::captures_to_hand() {
                    if let Some(captured) = self.board.piece_at(to) {
                        self.state
                            .placement
                            .add(us, self.banked_role(captured.role, to));
                    }
                }
                // `to` holds the captured enemy, so `set_piece` clears it first;
                // `from`'s occupant is the known `moving` piece.
                self.board.remove_known(from, moving);
                self.board.set_piece(to, moving);
            }
            WideMoveKind::DoublePawnPush => {
                self.board.remove_known(from, moving);
                self.board.set_empty(to, moving);
                // The ep target is the square the pawn skipped.
                let mid = if us.is_white() {
                    from.offset(0, 1)
                } else {
                    from.offset(0, -1)
                };
                self.state.ep_square = mid;
            }
            WideMoveKind::EnPassant => {
                reset_clock = true;
                // The ep landing square is empty (the enemy pawn skipped it); the
                // captured pawn sits on `to`'s file at `from`'s rank and is a known
                // enemy pawn, so it too can be cleared without a scan.
                self.board.remove_known(from, moving);
                self.board.set_empty(to, moving);
                if let Some(captured) = Square::<G>::from_file_rank(to.file(), from.rank()) {
                    self.board
                        .remove_known(captured, WidePiece::new(them, WideRole::Pawn));
                }
            }
            WideMoveKind::CastleKingside | WideMoveKind::CastleQueenside => {
                let side = if matches!(mv.kind(), WideMoveKind::CastleKingside) {
                    KINGSIDE
                } else {
                    QUEENSIDE
                };
                // The rook destination file comes from the same hook the
                // generator used, so make-move and movegen stay in lockstep for
                // any board geometry.
                let (_king_dest_file, rook_dest_file) = V::castle_dest_files(side);
                let rook_file = self
                    .state
                    .castling
                    .rook_file(us, side)
                    .expect("castling right present for a castling move");
                let rook_from = Square::<G>::from_file_rank(rook_file, rank)
                    .expect("rook start file is on the board");
                let rook_to = Square::<G>::from_file_rank(rook_dest_file, rank)
                    .expect("rook dest file is on the board");
                let rook = WidePiece::new(us, WideRole::Rook);
                // Both origins hold their known pieces (the king `moving` and the
                // rook); clear them by mask. The destinations are set with the
                // scanning `set_piece` since on some castle geometries a dest may
                // coincide with the other piece's just-cleared origin.
                self.board.remove_known(from, moving);
                self.board.remove_known(rook_from, rook);
                self.board.set_piece(to, moving);
                self.board.set_piece(rook_to, rook);
                castle_rook_from = Some(rook_from);
            }
            WideMoveKind::Promotion { role, capture } => {
                reset_clock = capture || is_pawn_move;
                // A hand variant (Shogi) banks the captured piece on a capturing
                // promotion too. Default-off, so inert for every other variant
                // (and for Synochess / Shinobi, whose fixed pockets never
                // replenish — `captures_to_hand()` is `false`).
                if V::has_hand() && V::captures_to_hand() && capture {
                    if let Some(captured) = self.board.piece_at(to) {
                        self.state
                            .placement
                            .add(us, self.banked_role(captured.role, to));
                    }
                }
                // `from` holds the known promoting piece (`moving`); `to` may hold a
                // captured enemy (capturing promotion), so it keeps `set_piece`.
                self.board.remove_known(from, moving);
                self.board.set_piece(to, WidePiece::new(us, role));
            }
            WideMoveKind::Drop { .. } => {
                // Drops are fully handled by the early return above (a drop has no
                // origin piece, so it cannot share the board-move path).
                unreachable!("drops are handled before the board-move match");
            }
            WideMoveKind::LionMove {
                first_capture,
                second_capture,
            } => {
                // A Chu Lion multi-step move: lift the Lion, remove up to two
                // victims (the intermediate square and the destination), then land
                // the Lion on `to`. For an igui / jitto pass `from == to`, so the
                // Lion returns to its own square (a net-zero board move plus, for
                // igui, the removed adjacent victim).
                reset_clock = first_capture || second_capture;
                self.board.remove_known(from, moving);
                if first_capture {
                    if let Some(mid) = mv.lion_mid::<G>() {
                        if let Some(victim) = self.board.piece_at(mid) {
                            self.board.remove_known(mid, victim);
                        }
                    }
                }
                // `set_piece` clears any occupant of `to` first, covering the
                // destination capture (`second_capture`); for a pass / igui,
                // `to == from` is empty after the lift, so this simply lands the
                // Lion back home.
                self.board.set_piece(to, moving);
            }
        }

        // Kyoto Shogi per-move flip (default-off): a moved piece toggles to its
        // alternate form on the square it just reached. Applied after the move is
        // on the board, so it never affects the legality of the move itself — only
        // the next position sees the flipped role. The King (and every piece in a
        // non-flipping variant) returns `None` and is left untouched, keeping every
        // other variant byte-identical. A castled rook never flips (its move is the
        // king's `moving`, whose role decides the flip), and a flip is a board
        // rewrite only — it does not bank anything to hand.
        if let Some(flipped) = V::flips_on_move(moving.role) {
            self.board.set_piece(to, WidePiece::new(us, flipped));
        }

        // Micro Shogi capture flip (default-off): a piece toggles its form only
        // when it **captures** (FSF `piecePromotionOnCapture`) — a base piece
        // promotes, a promoted piece demotes — leaving a quiet move's role
        // untouched. Like the Kyoto per-move flip above it is a post-move board
        // rewrite at the destination, decided after legality, so the move's own
        // legality is unaffected; only the next position sees the flipped role.
        // Every non-capture and every non-flipping variant returns `None` / is not
        // a capture and is byte-identical.
        if mv.is_capture() {
            if let Some(flipped) = V::flips_on_capture(moving.role) {
                self.board.set_piece(to, WidePiece::new(us, flipped));
            }
        }

        // Jieqi reveal (default-off): a face-down dark piece reveals its identity
        // on its first board move. Keyed on the piece's *origin* (home) square
        // `from`, the deterministic baseline reveals it as the Xiangqi piece native
        // to that square (under which the Jieqi tree is exactly Xiangqi). Like the
        // Kyoto flip above it is a post-move board rewrite at the destination — it
        // never affects the legality of the move itself, only the next position
        // sees the revealed role. Every non-Jieqi variant returns `None` and is
        // byte-identical.
        if let Some(revealed) = V::reveal_on_move(moving.role, from) {
            self.board.set_piece(to, WidePiece::new(us, revealed));
        }

        // Castling-right updates. A king move clears *both* of its side's castling
        // rights — but only for a castling variant. A first-move-leap variant
        // (Cambodian) instead carries two independent per-piece leap rights in the
        // same field (king in the kingside slot, Met in the queenside slot), so a
        // king move must clear only the king's right; that is handled uniformly by
        // the home-square revocation below (the king leaving its home file clears
        // exactly its slot, leaving the Met's right intact).
        if moving.role == WideRole::King && V::has_castling() {
            self.state.castling.revoke_color(us);
        }
        self.revoke_rights_for_square(from, us);
        if mv.is_capture() && !matches!(mv.kind(), WideMoveKind::EnPassant) {
            self.revoke_rights_for_square(to, them);
        }

        // Cambodian king-leap revocation (default-off, so every other variant
        // skips this and is byte-identical). FSF models the king leap like a
        // castling right whose `castlingRightsMask` covers the king's entire home
        // rank and file, so an *enemy rook* arriving on any square that shares the
        // king's home rank or file clears that king's leap right — even though the
        // king itself never moved. A non-rook piece, or a friendly rook, is inert;
        // the mover `us` is the enemy of the opponent, so this revokes only the
        // opponent's king leap right.
        if V::has_first_move_leaps() && moving.role == WideRole::Rook {
            self.revoke_king_leap_for_square(to, them);
        }

        // Seirawan gating-state update (default-off). A piece leaving a
        // gating-eligible square (its origin, plus a castling rook's origin)
        // clears that square; a captured enemy piece on an eligible square clears
        // it too; and the gate itself places the reserve and consumes it.
        if V::supports_gating() {
            self.update_gating(mv, from, to, us, castle_rook_from);
        }

        // Duck chess: the second half of the ply moves the neutral Duck to its new
        // square (default-off — a non-duck move carries no duck addendum).
        if V::has_duck() {
            if let Some(dsq) = mv.duck_to::<G>() {
                self.state.duck = Some(dsq);
            }
        }

        // Alice chess: after the move is on the board, the moved piece **goes
        // through the looking-glass** — it transfers to the same square on the
        // opposite plane. The board-membership mask is updated to reflect it: the
        // origin is vacated, the destination takes the *opposite* plane, and a
        // castled rook transfers with the king. En passant is not played in Alice
        // (the standard ruleset normally excludes it), so the ep target is always
        // cleared. Default-off, so every other variant leaves `board_b` empty.
        if V::is_alice() {
            let to_plane = !alice_from_plane;
            // Vacate the origin(s).
            self.state.board_b.clear(from);
            if let Some(rook_from) = castle_rook_from {
                self.state.board_b.clear(rook_from);
            }
            if matches!(mv.kind(), WideMoveKind::EnPassant) {
                if let Some(captured) = Square::<G>::from_file_rank(to.file(), from.rank()) {
                    self.state.board_b.clear(captured);
                }
            }
            // Land the transferred piece(s) on the opposite plane (a capture's
            // victim shared the mover's plane, so overwriting its bit is correct).
            set_plane(&mut self.state.board_b, to, to_plane);
            if castle_rook_from.is_some() {
                let side = if matches!(mv.kind(), WideMoveKind::CastleKingside) {
                    KINGSIDE
                } else {
                    QUEENSIDE
                };
                let (_k, rook_dest_file) = V::castle_dest_files(side);
                if let Some(rook_to) = Square::<G>::from_file_rank(rook_dest_file, rank) {
                    set_plane(&mut self.state.board_b, rook_to, to_plane);
                }
            }
            self.state.ep_square = None;
        }

        if reset_clock {
            self.state.halfmove_clock = 0;
        } else {
            self.state.halfmove_clock = self.state.halfmove_clock.saturating_add(1);
        }
        if us.is_black() {
            self.state.fullmove_number = self.state.fullmove_number.saturating_add(1);
        }
        // Janggi pass tracking (default-off): a pass is the only move whose origin
        // equals its destination (a quiet `general -> general`); a pass increments
        // the consecutive-pass counter, any real move resets it to zero. Two
        // consecutive passes end the game (the generator returns no move at a count
        // of two). For every variant without `allows_pass()` no `from == to` move is
        // ever generated, so this stays `0` and is byte-identical.
        if V::allows_pass() {
            self.state.consecutive_passes = if from == to {
                self.state.consecutive_passes.saturating_add(1)
            } else {
                0
            };
        }
        // Crazyhouse promoted mask upkeep (default-off): carry a moving piece's
        // promoted bit to its destination, set it on a fresh promotion, and clear
        // any stale bit. The board-move path only (drops returned early above).
        if V::demotes_promoted_captures() {
            self.update_promoted_mask(mv.kind(), from, to);
        }
        self.state.turn = them;
    }

    /// The role a captured piece banks into the captor's hand: a Pawn when the
    /// captured square is in the crazyhouse [`promoted`](Self::promoted) mask (the
    /// piece reached the board by promotion), otherwise the role's own hand base
    /// ([`WideVariant::role_hand_base`]). For a non-demoting variant the mask is
    /// always empty, so this is exactly `role_hand_base`.
    #[inline]
    fn banked_role(&self, captured: WideRole, to: Square<G>) -> WideRole {
        if V::demotes_promoted_captures() && self.promoted.contains(to) {
            WideRole::Pawn
        } else {
            V::role_hand_base(captured)
        }
    }

    /// Maintains the crazyhouse [`promoted`](Self::promoted) mask after a board
    /// move (never a drop — those return early). A promotion marks its destination
    /// as a promoted piece; any other move carries the source's promoted bit, if
    /// any, to the destination (and clears a stale destination bit left by a
    /// captured promoted piece). The captured pawn of an en-passant move and the
    /// rook of a castle are never promoted, so the single source→destination carry
    /// of the moving piece covers every kind.
    fn update_promoted_mask(&mut self, kind: WideMoveKind, from: Square<G>, to: Square<G>) {
        match kind {
            WideMoveKind::Promotion { .. } => {
                // The newly promoted piece sits on `to`; its source pawn was never
                // promoted.
                self.promoted.clear(from);
                self.promoted.set(to);
            }
            WideMoveKind::Drop { .. } => {
                unreachable!("drops are handled before the board-move path");
            }
            WideMoveKind::Quiet
            | WideMoveKind::Capture
            | WideMoveKind::DoublePawnPush
            | WideMoveKind::EnPassant
            | WideMoveKind::CastleKingside
            | WideMoveKind::CastleQueenside
            // A Lion move carries the (never-promoted) Lion's bit from `from` to
            // `to` like any board move; the intermediate victim, if any, is cleared
            // with the destination bit. Chu Shogi has no crazyhouse promoted mask,
            // so this arm is inert there — it exists only for match exhaustiveness.
            | WideMoveKind::LionMove { .. } => {
                if self.promoted.contains(from) {
                    self.promoted.clear(from);
                    self.promoted.set(to);
                } else {
                    // The mover carried no promoted bit; clear `to` in case a
                    // captured promoted piece had stood there.
                    self.promoted.clear(from);
                    self.promoted.clear(to);
                }
            }
        }
    }

    /// Updates the Seirawan gating state after a move (gating variants only):
    /// vacated eligible squares are cleared, a captured enemy piece on an
    /// eligible square is cleared, and a gate places its reserve piece (consuming
    /// it from hand) on the chosen vacated square.
    fn update_gating(
        &mut self,
        mv: &WideMove,
        from: Square<G>,
        to: Square<G>,
        us: Color,
        castle_rook_from: Option<Square<G>>,
    ) {
        // A square stays gating-eligible only while it holds its *original*,
        // never-moved piece. Any piece moving off a square (`from`) or onto it
        // (`to`) ends that: the origin's piece has left, and a destination now
        // holds a foreign piece (or, on a capture, the original occupant is
        // gone). So both the origin and the destination are cleared. This keeps
        // the eligible set exactly "virgin back-rank squares", matching FSF's
        // per-square gating rights.
        self.state.gating.vacate(from);
        self.state.gating.vacate(to);
        // A castle also moves the rook: its origin and destination are no longer
        // virgin either.
        if let Some(rook_from) = castle_rook_from {
            self.state.gating.vacate(rook_from);
            // The rook's destination square is `to`-adjacent; clear it too so a
            // rook parked on a back-rank square never re-gates.
            let side = if matches!(mv.kind(), WideMoveKind::CastleKingside) {
                KINGSIDE
            } else {
                QUEENSIDE
            };
            let (_k, rook_dest_file) = V::castle_dest_files(side);
            if let Some(rook_to) = Square::<G>::from_file_rank(rook_dest_file, from.rank()) {
                self.state.gating.vacate(rook_to);
            }
        }

        // Apply the gate itself: drop the reserve on the chosen vacated square.
        if let Some(gate) = mv.gate() {
            let square = match mv.gate_square() {
                GateSquare::Origin => Some(from),
                GateSquare::RookOrigin => castle_rook_from,
            };
            if let Some(square) = square {
                self.board
                    .set_piece(square, WidePiece::new(us, gate.role()));
                self.state.gating.take_reserve(us, gate);
                // The freshly gated piece sits on a back-rank square but is a new
                // piece that has itself just "moved" in — it is not gating-
                // eligible, and `vacate(from)` above already cleared the square.
            }
        }
        // S-House hand-gate: the gated piece comes from the crazyhouse hand (any
        // held role), consumed from `placement` rather than the fixed reserve.
        if let Some(role) = mv.hand_gate() {
            let square = match mv.hand_gate_square() {
                GateSquare::Origin => Some(from),
                GateSquare::RookOrigin => castle_rook_from,
            };
            if let Some(square) = square {
                self.board.set_piece(square, WidePiece::new(us, role));
                self.state.placement.take(us, role);
            }
        }
    }

    /// If `square` is a castling rook's home square for `color`, revokes that
    /// right.
    fn revoke_rights_for_square(&mut self, square: Square<G>, color: Color) {
        if self.state.castling.is_empty() {
            return;
        }
        let rank = V::castle_rank(color);
        if square.rank() != rank {
            return;
        }
        for side in [KINGSIDE, QUEENSIDE] {
            if let Some(file) = self.state.castling.rook_file(color, side) {
                if Square::<G>::from_file_rank(file, rank) == Some(square) {
                    self.state.castling.set(color, side, None);
                }
            }
        }
    }

    /// Re-derives `color`'s standard castling rights from the board after a
    /// placement drop (Placement / Pre-Chess). With `color`'s king on
    /// `(king_file, castle_rank)`, a rook on the queenside corner (file `0`)
    /// confers the queenside right and a rook on the kingside corner
    /// (file `WIDTH - 1`) the kingside right — the a-/h-file rooks
    /// [`GenericCastling::standard`] uses. The rights only build up (the king and
    /// rooks never leave the board during deployment), so this matches FSF's
    /// incremental `KQkq` assignment.
    fn derive_placement_castling(&mut self, color: Color, king_file: u8) {
        let rank = V::castle_rank(color);
        let Some(king_sq) = Square::<G>::from_file_rank(king_file, rank) else {
            return;
        };
        if self.board.piece_at(king_sq) != Some(WidePiece::new(color, WideRole::King)) {
            return;
        }
        let rook = WidePiece::new(color, WideRole::Rook);
        for (side, file) in [(KINGSIDE, G::WIDTH - 1), (QUEENSIDE, 0)] {
            if let Some(rook_sq) = Square::<G>::from_file_rank(file, rank) {
                if self.board.piece_at(rook_sq) == Some(rook) {
                    self.state.castling.set(color, side, Some(file));
                }
            }
        }
    }

    /// Revokes `owner`'s king leap right when an enemy rook arrives on `square`
    /// and that square shares the king's home rank or file
    /// (`has_first_move_leaps()` variants only). A king's leap right lives in the
    /// kingside castling slot keyed by the king's home file; its home rank is the
    /// castle rank. This mirrors FSF's `castlingRightsMask` over the king's whole
    /// rank and file.
    fn revoke_king_leap_for_square(&mut self, square: Square<G>, owner: Color) {
        if self.state.castling.is_empty() {
            return;
        }
        let Some(home_file) = self.state.castling.rook_file(owner, KINGSIDE) else {
            return;
        };
        let home_rank = V::castle_rank(owner);
        if square.file() == home_file || square.rank() == home_rank {
            self.state.castling.set(owner, KINGSIDE, None);
        }
    }

    // -- Outcome -----------------------------------------------------------

    /// Returns the game-ending reason if the position is terminal, else `None`.
    ///
    /// Standard terminal rules: checkmate, stalemate, and the variant's
    /// [`WideVariant::extra_terminal`] hook (reserved). Material draws and the
    /// fifty-move rule are not modeled this phase.
    #[must_use]
    pub fn end_reason(&self) -> Option<WideEndReason> {
        if let Some(reason) = V::extra_terminal(&self.board, &self.state) {
            return Some(reason);
        }
        // Flag-rank "campmate" (Synochess): a king on its goal rank is a win for
        // that side, even though it is now the loser's turn. Gated behind
        // `has_flag_win()` (default-off). Reported as a variant win; `outcome`
        // resolves the winner from the board, since the campmate-reaching side is
        // the side *not* to move.
        if V::has_flag_win()
            && (self.flag_win_reached(Color::White) || self.flag_win_reached(Color::Black))
        {
            return Some(WideEndReason::VariantWin);
        }
        // Temple win (Chak): a Divine Lord standing on the enemy temple square is a
        // win for that side, even though it is now the loser's turn. Gated behind
        // `has_temple_win()` (default-off). Reported as a variant win; `outcome`
        // resolves the winner from the board (the temple-reaching side is the side
        // *not* to move).
        if V::has_temple_win()
            && (self.temple_win_reached(Color::White) || self.temple_win_reached(Color::Black))
        {
            return Some(WideEndReason::VariantWin);
        }
        // Check-win (Checkshogi): the side to move being in check means the
        // opponent's last move gave check and won the game outright. Gated behind
        // `wins_on_check()` (default-off). Reported *before* the checkmate/stalemate
        // test so a checking move is a variant win — not a (possibly non-mating)
        // check or a spurious checkmate. `outcome` credits the checker (the side
        // *not* to move).
        if V::wins_on_check() && self.is_check() {
            return Some(WideEndReason::VariantWin);
        }
        // Bare-king "Robado" draw (Shatar): a side reduced to its lone king draws
        // the game immediately. Gated behind `has_bare_king_draw()` (default-off).
        // Reported before the checkmate/stalemate test so a bare-king node — which
        // generates zero moves — is classified as the draw it is, not a (spurious)
        // checkmate or stalemate.
        if V::has_bare_king_draw() && self.bare_king_present() {
            return Some(WideEndReason::VariantDraw);
        }
        // Bare-king baring loss (Shatranj): a side bared of all but its king has
        // lost. Gated behind `has_bare_king_loss()` (default-off). Reported before
        // the checkmate/stalemate test so a bared node — which generates zero moves
        // — is classified as the baring win it is, not a spurious stalemate.
        // `outcome` resolves the winner as the side that is *not* bared.
        if V::has_bare_king_loss() && self.bare_king_loss_loser().is_some() {
            return Some(WideEndReason::VariantWin);
        }
        let no_moves = self.legal_moves().is_empty();
        // Checkmate takes precedence over every draw rule below: a side with no
        // move while in check has lost, whatever the clocks or material say.
        if no_moves && self.is_check() {
            return Some(WideEndReason::Checkmate);
        }
        // Bikjang (Janggi): when the opponent **passes while the two generals face**
        // down an open file, the side to move has no legal move at all (the move
        // generator already truncates to zero — a perft-counted terminal) and the
        // game is a **draw**, exactly as Fairy-Stockfish adjudicates
        // `st->bikjang && st->previous->bikjang`. Detected here from the single
        // position (the facing relation plus the pending pass recorded in
        // `consecutive_passes`) so the zero-move node is reported as the draw it is
        // rather than a spurious stalemate. Gated behind the default-off
        // [`WideVariant::has_bikjang`], so every other variant is byte-identical.
        if no_moves
            && V::has_bikjang()
            && self.state.consecutive_passes >= 1
            && self.is_facing_generals()
        {
            return Some(WideEndReason::Bikjang);
        }
        // The single-position draw rules, each behind a default-off hook so every
        // variant that does not opt in is unaffected (and none of this touches move
        // generation, so perft stays byte-identical). Repetition / sennichite /
        // perpetual-check / bikjang / counting need a move history and live in
        // [`GenericGame`](super::game::GenericGame); the two below are derivable
        // from the position alone.
        //
        // Insufficient material (opt-in per variant; default-off).
        if V::is_insufficient_material(&self.board, &self.state) {
            return Some(WideEndReason::InsufficientMaterial);
        }
        // Move-count rule — the generic analogue of the fifty-move rule (opt-in).
        if let Some(limit) = V::move_rule_plies() {
            if self.state.halfmove_clock >= limit {
                return Some(WideEndReason::MoveRule);
            }
        }
        if no_moves {
            // Not in check and no move: stalemate.
            Some(WideEndReason::Stalemate)
        } else {
            None
        }
    }

    /// Returns the outcome (decisive winner or draw) if the position is
    /// terminal, else `None`.
    #[must_use]
    pub fn outcome(&self) -> Option<WideOutcome> {
        let reason = self.end_reason()?;
        Some(match reason {
            WideEndReason::Checkmate => WideOutcome::Decisive {
                winner: self.state.turn.opposite(),
            },
            WideEndReason::VariantWin => {
                // Flag-rank "campmate" (Synochess) is won by the side whose king
                // stands on its goal rank — the side *not* to move. Other variant
                // wins (reserved) credit the side to move.
                let winner = if (V::has_flag_win()
                    && self.flag_win_reached(self.state.turn.opposite()))
                    || (V::has_temple_win() && self.temple_win_reached(self.state.turn.opposite()))
                    || (V::wins_on_check() && self.is_check())
                {
                    // Check-win (Checkshogi): the checked side to move has lost, so
                    // the checker — the side *not* to move — wins.
                    self.state.turn.opposite()
                } else if V::has_bare_king_loss() {
                    // Baring (Shatranj): the bared side has lost, so its opponent
                    // wins, whichever side is to move.
                    match self.bare_king_loss_loser() {
                        Some(loser) => loser.opposite(),
                        None => self.state.turn,
                    }
                } else {
                    self.state.turn
                };
                WideOutcome::Decisive { winner }
            }
            // Stalemate is a loss for the side to move in variants that say so
            // (Synochess); otherwise the usual draw.
            WideEndReason::Stalemate if V::stalemate_is_loss() => WideOutcome::Decisive {
                winner: self.state.turn.opposite(),
            },
            // The perpetual-check loss needs the move history to know which side
            // was the checker, so it is resolved by
            // [`GenericGame`](super::game::GenericGame), never produced here. A
            // bare position cannot reach this arm; treat it as a draw defensively.
            WideEndReason::PerpetualCheckLoss | WideEndReason::PerpetualChaseLoss => {
                WideOutcome::Draw
            }
            WideEndReason::Stalemate
            | WideEndReason::InsufficientMaterial
            | WideEndReason::VariantDraw
            | WideEndReason::Repetition
            | WideEndReason::Sennichite
            | WideEndReason::Bikjang
            | WideEndReason::CountingDraw
            | WideEndReason::MoveRule => WideOutcome::Draw,
        })
    }

    // -- Repetition / draw helpers -----------------------------------------

    /// A stable 64-bit **Zobrist** key identifying this position: the board
    /// placement, side to move, en-passant target, castling / gating rights, hands
    /// in pocket, the Duck and Alice planes, and the promoted mask — but **not** the
    /// move clocks nor the Janggi pass counter (which differ on most plies and must
    /// not break a repetition). Two positions share a key iff they are "the same
    /// position" for the repetition rules and for opening-book / deduplication use.
    ///
    /// This is the generic analogue of
    /// [`Position::zobrist`](crate::Position::zobrist): the XOR of one random
    /// constant per hashed feature (see the `zobrist` module). The value here
    /// is **computed from scratch**; [`GenericGame`](super::game::GenericGame)
    /// maintains the identical value **incrementally** across moves (XOR-ing
    /// features in and out) for variants that
    /// [track repetition](WideVariant::tracks_repetition), and
    /// [`GenericGame::position_key`](super::game::GenericGame::position_key) exposes
    /// it. The history-free [`GenericPosition`] (and therefore perft) never computes
    /// or maintains it, so the bare make/unmake path stays byte-identical.
    #[must_use]
    pub fn zobrist(&self) -> u64 {
        self.zobrist_board_part() ^ self.zobrist_state_part()
    }

    /// The previous name for [`zobrist`](Self::zobrist), kept for the repetition
    /// callers. Returns the same value.
    #[must_use]
    #[inline]
    pub fn repetition_key(&self) -> u64 {
        self.zobrist()
    }

    /// The board half of [`zobrist`](Self::zobrist): the XOR of every colored
    /// piece's square key.
    fn zobrist_board_part(&self) -> u64 {
        let mut hash = 0u64;
        for color in Color::ALL {
            for role in WideRole::ALL {
                for sq in self.board.pieces(color, role) {
                    hash ^= zobrist::piece_key(color, role, sq.index());
                }
            }
        }
        hash
    }

    /// The non-board half of [`zobrist`](Self::zobrist): side to move, castling /
    /// gating rights, en passant, the gating reserves, hands in pocket, Duck, Alice
    /// plane, and promoted mask. Recomputed from scratch (it is cheap — the state is
    /// tiny next to the board), so [`GenericGame`](super::game::GenericGame) can XOR
    /// the old part out and the new part in around a move.
    pub(crate) fn zobrist_state_part(&self) -> u64 {
        let s = &self.state;
        let mut hash = zobrist::side_key(s.turn);

        for color in Color::ALL {
            for side in [KINGSIDE, QUEENSIDE] {
                if let Some(file) = s.castling.rook_file(color, side) {
                    hash ^= zobrist::castling_key(color, side, file);
                }
            }
        }

        if let Some(sq) = s.ep_square {
            hash ^= zobrist::ep_key(sq.index());
        }

        for sq in s.gating.eligible() {
            hash ^= zobrist::gating_eligible_key(sq.index());
        }
        for color in Color::ALL {
            for gate in [GateRole::Hawk, GateRole::Elephant] {
                if s.gating.has_reserve(color, gate) {
                    hash ^= zobrist::gating_reserve_key(color, gate);
                }
            }
        }

        for color in Color::ALL {
            for role in WideRole::ALL {
                let count = s.placement.count(color, role);
                if count > 0 {
                    hash ^= zobrist::hand_key(color, role, count);
                }
            }
        }

        if let Some(sq) = s.duck {
            hash ^= zobrist::duck_key(sq.index());
        }
        for sq in s.board_b {
            hash ^= zobrist::alice_key(sq.index());
        }
        for sq in self.promoted {
            hash ^= zobrist::promoted_key(sq.index());
        }

        hash
    }

    /// The XOR delta to apply to a maintained [`zobrist`](Self::zobrist) key for the
    /// board edits a just-applied move made, given the [`Undo`] it produced.
    ///
    /// `self` is the position **after** the move; `undo` carries the pre-move color
    /// masks and the pre-move masks of every role the move touched. For each touched
    /// role and color, the squares whose membership flipped (the symmetric
    /// difference of the old and new piece sets) are exactly the piece-square keys
    /// to toggle — so XOR-ing this delta into the key reproduces a from-scratch
    /// [`zobrist_board_part`](Self::zobrist_board_part) without rescanning the whole
    /// board. The state half is handled separately by
    /// [`zobrist_state_part`](Self::zobrist_state_part).
    pub(crate) fn zobrist_board_delta(&self, undo: &Undo<G>) -> u64 {
        let mut delta = 0u64;
        // A role may be recorded more than once (the Undo tolerates duplicates); a
        // role's whole contribution must be folded exactly once, so skip repeats —
        // each stored mask is the same pre-move value, taken before any edit.
        let mut seen: u128 = 0;
        for &(idx, old_role_mask) in &undo.roles[..undo.role_count] {
            let bit = 1u128 << idx;
            if seen & bit != 0 {
                continue;
            }
            seen |= bit;
            let role = WideRole::ALL[idx];
            let new_role_mask = self.board.by_role(role);
            for (ci, color) in Color::ALL.into_iter().enumerate() {
                let old_set = undo.by_color[ci] & old_role_mask;
                let new_set = self.board.by_color(color) & new_role_mask;
                for sq in old_set ^ new_set {
                    delta ^= zobrist::piece_key(color, role, sq.index());
                }
            }
        }
        delta
    }

    /// Returns `true` if the two royal generals (kings) **face each other** down an
    /// open file with no piece between them. The geometric core of the Janggi
    /// **bikjang** rule (and the Xiangqi "flying general" relation). Single-royal
    /// only; uses each side's king square.
    ///
    /// Bikjang is a draw when this holds in two **consecutive** positions, exactly
    /// as Fairy-Stockfish models it (`st->bikjang && st->previous->bikjang`); the
    /// two-ply test lives in [`GenericGame`](super::game::GenericGame), which has
    /// the history. This predicate is the single-position half it consults.
    #[must_use]
    pub fn is_facing_generals(&self) -> bool {
        let (Some(w), Some(b)) = (
            self.board.king_of(Color::White),
            self.board.king_of(Color::Black),
        ) else {
            return false;
        };
        if w.file() != b.file() {
            return false;
        }
        let occ = self.board.occupied();
        let file = w.file();
        let lo = w.rank().min(b.rank());
        let hi = w.rank().max(b.rank());
        for r in (lo + 1)..hi {
            if let Some(sq) = Square::<G>::from_file_rank(file, r) {
                if occ.contains(sq) {
                    return false;
                }
            }
        }
        true
    }

    // -- FEN ---------------------------------------------------------------

    /// Parses a FEN string into a generic position over `G` and `V`.
    ///
    /// The grammar is the standard six fields: placement, side to move, castling
    /// (`KQkq` style, with the king on the e-file and rooks on the a/h files for
    /// standard chess), en passant, halfmove clock, fullmove number. The last two
    /// clock fields are optional and default to `0` / `1`.
    ///
    /// # Errors
    ///
    /// Returns a [`WideFenError`] for a missing or malformed field.
    pub fn from_fen(fen: &str) -> Result<Self, WideFenError> {
        let mut fields = fen.split_whitespace();

        let placement_field = fields.next().ok_or(WideFenError::MissingField)?;
        // Gating variants append the reserves in hand as a `[HEhe]`-style bracket
        // after the board placement (the crazyhouse holdings convention). Split it
        // off before parsing the board.
        let (placement, holdings) = split_holdings(placement_field);
        // Duck chess renders the neutral Duck as a `*` in the placement. Strip it
        // out (recording its square) before the board parser, which knows only
        // real pieces. Non-duck variants never see a `*`, so they keep the
        // borrowed placement and allocate nothing here.
        let mut duck = None;
        let stripped;
        let placement = if V::has_duck() {
            let (s, d) = split_duck::<G>(placement)?;
            duck = d;
            stripped = s;
            stripped.as_str()
        } else {
            placement
        };
        // A crazyhouse-style variant (Capahouse) marks a promoted piece with a
        // trailing `~` on its placement token (`Q~`). Strip the markers out
        // (recording their squares) before the board parser, which knows only bare
        // piece letters. Non-demoting variants never see a `~`, keep the borrowed
        // placement, and allocate nothing here.
        let mut promoted = Bitboard::<G>::EMPTY;
        let promoted_stripped;
        let placement = if V::demotes_promoted_captures() {
            let (s, mask) = split_promoted::<G>(placement)?;
            promoted = mask;
            promoted_stripped = s;
            promoted_stripped.as_str()
        } else {
            placement
        };
        let board = Board::<G>::from_fen_placement(placement).map_err(WideFenError::Placement)?;

        // Sittuyin carries the setup-phase pocket in the same `[..]` holdings
        // bracket the gating variants use (the crazyhouse convention): uppercase
        // letters are white's undeployed pieces, lowercase black's. A non-
        // placement variant never reads the bracket here, so its pocket stays
        // `NONE`.
        let placement_pocket = if V::has_placement() || V::has_hand() {
            parse_placement_holdings(holdings)?
        } else {
            GenericPlacement::NONE
        };

        let turn = match fields.next().ok_or(WideFenError::MissingField)? {
            "w" => Color::White,
            "b" => Color::Black,
            _ => return Err(WideFenError::BadTurn),
        };

        let castling_field = fields.next().ok_or(WideFenError::MissingField)?;
        // A gating variant folds gating-square rights into the castling field
        // (e.g. `KQBCDFGkqbcdfg`): the `KQkq` letters are castling rights and the
        // file letters mark gating-eligible squares. Non-gating variants reject
        // any non-`KQkq` letter, exactly as before.
        let (castling, gating) = if V::supports_gating() {
            // S-House keeps its reserves in the crazyhouse hand (parsed above into
            // `placement_pocket`), not in the fixed gating reserve, so the gating
            // parser sees empty holdings and only reads the eligible-square rights
            // from the castling field.
            let gating_holdings = if V::gates_from_hand() { "" } else { holdings };
            parse_castling_and_gating::<G>(castling_field, gating_holdings, &board)?
        } else if V::has_first_move_leaps() {
            // A first-move-leap variant (Cambodian) folds its two per-side leap
            // rights into the castling field as home-file letters (`DEde`),
            // delegating the dialect to the variant. Default-off, so every other
            // non-gating variant keeps the plain `KQkq` parser below.
            (
                V::parse_first_move_rights(castling_field).ok_or(WideFenError::BadCastling)?,
                GenericGating::NONE,
            )
        } else {
            (
                parse_castling::<G, V>(castling_field, &board)?,
                GenericGating::NONE,
            )
        };

        let ep_field = fields.next().ok_or(WideFenError::MissingField)?;
        let ep_square = parse_ep::<G>(ep_field)?;

        let halfmove_clock = match fields.next() {
            Some(s) => parse_clock(s)?,
            None => 0,
        };
        let fullmove_number = match fields.next() {
            Some(s) => parse_clock(s)?,
            None => 1,
        };

        if fields.next().is_some() {
            return Err(WideFenError::TrailingData);
        }

        let state = GenericState {
            turn,
            castling,
            ep_square,
            gating,
            duck,
            placement: placement_pocket,
            halfmove_clock,
            fullmove_number,
            consecutive_passes: 0,
            board_b: crate::geometry::Bitboard::EMPTY,
        };
        let mut pos = Self::from_parts(board, state);
        pos.promoted = promoted;
        Ok(pos)
    }

    /// Serializes this position as a six-field FEN string over `G`.
    ///
    /// A gating variant appends the reserves in hand to the placement field as a
    /// `[..]` bracket and folds the gating-eligible squares into the castling
    /// field (`KQBCDFGkqbcdfg`-style), matching the Fairy-Stockfish S-Chess
    /// dialect. A non-gating variant produces the plain six-field FEN unchanged.
    #[must_use]
    pub fn to_fen(&self) -> String {
        let mut out = if V::demotes_promoted_captures() {
            placement_with_promoted::<G>(&self.board, self.promoted)
        } else if V::has_duck() {
            placement_with_duck::<G>(&self.board, self.state.duck)
        } else {
            self.board.to_fen_placement()
        };
        if V::supports_gating() && !V::gates_from_hand() {
            write_holdings(self.state.gating, &mut out);
        }
        if V::has_placement() || V::has_hand() {
            write_placement_holdings(self.state.placement, &mut out);
        }
        out.push(' ');
        out.push(if self.state.turn.is_white() { 'w' } else { 'b' });
        out.push(' ');
        if V::supports_gating() {
            let kings = [
                self.board.king_of(Color::White),
                self.board.king_of(Color::Black),
            ];
            write_castling_and_gating::<G>(self.state.castling, self.state.gating, kings, &mut out);
        } else if V::has_first_move_leaps() {
            V::write_first_move_rights(self.state.castling, &mut out);
        } else {
            write_castling::<G, V>(self.state.castling, &mut out);
        }
        out.push(' ');
        match self.state.ep_square {
            Some(sq) => write_square::<G>(&mut out, sq),
            None => out.push('-'),
        }
        out.push(' ');
        push_decimal(&mut out, self.state.halfmove_clock as u32);
        out.push(' ');
        push_decimal(&mut out, self.state.fullmove_number as u32);
        out
    }
}

/// The terminal outcome of a generic game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WideOutcome {
    /// One side won.
    Decisive {
        /// The victorious color.
        winner: Color,
    },
    /// The game is drawn.
    Draw,
}

// -- Move sink (materialise vs bulk-count) ----------------------------------

/// The destination for the standard generator's emitted moves.
///
/// The generic analogue of the concrete [`MoveSink`](crate::position): the
/// standard single-king generator pushes each candidate move into a sink rather
/// than a fixed buffer, so one generator body serves both the *materialising*
/// callers (a `Vec<WideMove>` or a stack-backed [`WideMoveList`], which record
/// every move) and the *bulk leaf-counting* caller a perft leaf wants
/// ([`WideCountSink`], which only tallies how many moves there are). Because a
/// perft leaf needs the *number* of legal moves and never the moves themselves,
/// the counting sink replaces each per-target loop with a single population
/// count, skipping move construction entirely.
///
/// Every implementor must treat [`emit_targets`](WideSink::emit_targets)
/// identically in *count*: it is exactly `targets.count()` single moves, one per
/// set bit. The default body materialises; [`WideCountSink`] overrides it with
/// the population count. `len` / `get` are needed only by the Seirawan gating
/// pass, which reads the base moves back by index; the count sink never reaches
/// that path, so its `get` is `unreachable!`.
pub(crate) trait WideSink {
    /// Records a single fully-formed move.
    fn push(&mut self, mv: WideMove);

    /// Records one move from `from` to each square in `targets`, tagging each as
    /// a [`Capture`](WideMoveKind::Capture) when the target is in `their_pieces`
    /// and [`Quiet`](WideMoveKind::Quiet) otherwise.
    ///
    /// The materialising sinks iterate the targets and build a [`WideMove`] per
    /// bit; the counting sink replaces the whole loop with one population count
    /// (the capture/quiet split does not change *how many* moves there are) — the
    /// core of the perft bulk-count speedup.
    #[inline]
    fn emit_targets<G: Geometry>(
        &mut self,
        from: Square<G>,
        targets: Bitboard<G>,
        their_pieces: Bitboard<G>,
    ) {
        for to in targets {
            let kind = if their_pieces.contains(to) {
                WideMoveKind::Capture
            } else {
                WideMoveKind::Quiet
            };
            self.push(WideMove::new(from, to, kind));
        }
    }

    /// The number of moves recorded so far (the gating pass's base-move count).
    fn len(&self) -> usize;

    /// The move at `index` (the gating pass reads the base moves back by index).
    fn get(&self, index: usize) -> WideMove;
}

impl WideSink for Vec<WideMove> {
    #[inline]
    fn push(&mut self, mv: WideMove) {
        Vec::push(self, mv);
    }
    #[inline]
    fn len(&self) -> usize {
        Vec::len(self)
    }
    #[inline]
    fn get(&self, index: usize) -> WideMove {
        self[index]
    }
}

/// A [`WideSink`] that only counts the moves it is given, never materialising
/// any — the bulk leaf-counting destination for perft at depth 1 on the standard
/// single-king path.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct WideCountSink {
    count: u64,
}

impl WideCountSink {
    /// The number of moves recorded so far.
    #[inline]
    fn count(self) -> u64 {
        self.count
    }
}

impl WideSink for WideCountSink {
    #[inline]
    fn push(&mut self, _mv: WideMove) {
        self.count += 1;
    }
    #[inline]
    fn emit_targets<G: Geometry>(
        &mut self,
        _from: Square<G>,
        targets: Bitboard<G>,
        _their_pieces: Bitboard<G>,
    ) {
        // The whole per-target loop collapses to a single population count.
        self.count += u64::from(targets.count());
    }
    #[inline]
    fn len(&self) -> usize {
        self.count as usize
    }
    fn get(&self, _index: usize) -> WideMove {
        // Reached only by the Seirawan gating pass, which never runs on the
        // standard path a count sink drives.
        unreachable!("a counting sink never materialises moves to read back")
    }
}

/// A fixed-capacity, stack-backed list of [`WideMove`]s with heap spill on
/// overflow — the generic analogue of the concrete [`MoveList`](crate::position),
/// so the reusable-buffer perft recursion allocates no per-node `Vec`.
///
/// Move generation runs once per perft node; collecting each node's moves into a
/// fresh `Vec<WideMove>` is a heap allocation (and free) at every node. This
/// stores the first [`WideMoveList::INLINE`] moves in an inline `[WideMove; N]`
/// array with a length cursor and spills any overflow to a heap `Vec`, so the
/// common path is allocation-free. Standard chess has a proven 218-move maximum;
/// the large-board variants stay well under the inline capacity, and the spill
/// keeps the type total and safe for any adversarial position. A [`WideMove`] is
/// a `Copy` `u64`, so the buffer needs no `unsafe`, no `MaybeUninit`: the inline
/// tail is value-initialised with a sentinel that is never read (only the first
/// `inline_len` slots are exposed, each overwritten by a real push first).
#[derive(Clone)]
pub(crate) struct WideMoveList {
    inline: [WideMove; Self::INLINE],
    inline_len: usize,
    spill: Vec<WideMove>,
}

impl WideMoveList {
    /// The inline capacity. Covers the 218-move standard-chess maximum and the
    /// large-board variants' move counts with margin; rare overflow spills.
    pub(crate) const INLINE: usize = 256;

    /// A sentinel used to value-initialise the unused inline tail; never read.
    const NULL: WideMove = WideMove::null();

    /// An empty list.
    #[inline]
    pub(crate) fn new() -> WideMoveList {
        WideMoveList {
            inline: [Self::NULL; Self::INLINE],
            inline_len: 0,
            spill: Vec::new(),
        }
    }

    /// The number of moves in the list (inline plus spill).
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.inline_len + self.spill.len()
    }

    /// Removes all moves, keeping the spill allocation for reuse.
    #[inline]
    pub(crate) fn clear(&mut self) {
        self.inline_len = 0;
        self.spill.clear();
    }

    /// Calls `f` on each move in push order. On the common path (no spill) this
    /// is a tight loop over one contiguous slice — the shape the perft inner loop
    /// wants.
    #[inline]
    pub(crate) fn for_each(&self, mut f: impl FnMut(WideMove)) {
        for &mv in &self.inline[..self.inline_len] {
            f(mv);
        }
        for &mv in &self.spill {
            f(mv);
        }
    }
}

impl WideSink for WideMoveList {
    #[inline]
    fn push(&mut self, mv: WideMove) {
        if self.inline_len < Self::INLINE {
            self.inline[self.inline_len] = mv;
            self.inline_len += 1;
        } else {
            self.spill.push(mv);
        }
    }
    #[inline]
    fn len(&self) -> usize {
        WideMoveList::len(self)
    }
    #[inline]
    fn get(&self, index: usize) -> WideMove {
        if index < self.inline_len {
            self.inline[index]
        } else {
            self.spill[index - self.inline_len]
        }
    }
}

/// The inline capacity of [`Pins`]: at most one pin per king ray (≤ 8 rays), so
/// sixteen slots cover every position with margin and the line array never spills.
const PINS_INLINE: usize = 16;

/// The pinned pieces of the side to move and, per pinned piece, the line it is
/// confined to.
///
/// Recorded **inline** with no per-node heap allocation: a king has at most eight
/// ray directions and a ray can pin at most one piece, so a position has at most
/// eight pinned pieces; the [`INLINE`](Pins::INLINE) array of sixteen covers that
/// with margin. The `pinned` bitboard answers "is this piece pinned?" in one mask
/// test before the (tiny, bounded) linear scan for its line. The empty-pins case
/// (the common one) touches neither the array nor a scan.
struct Pins<G: Geometry> {
    pinned: Bitboard<G>,
    lines: [(Square<G>, Bitboard<G>); PINS_INLINE],
    len: usize,
    king_sq: Square<G>,
}

impl<G: Geometry> Pins<G> {
    fn empty(king_sq: Square<G>) -> Pins<G> {
        Pins {
            pinned: Bitboard::EMPTY,
            // A king square is a valid sentinel for the unused tail; never read
            // (only the first `len` entries, each written before any read).
            lines: [(king_sq, Bitboard::FULL); PINS_INLINE],
            len: 0,
            king_sq,
        }
    }

    fn add(&mut self, square: Square<G>, l: Bitboard<G>) {
        self.pinned.set(square);
        if self.len < PINS_INLINE {
            self.lines[self.len] = (square, l);
            self.len += 1;
        }
    }

    /// The line a piece is confined to: its pin line if pinned, else the full
    /// board (unconfined). The `pinned` mask short-circuits the unpinned common
    /// case before the bounded scan.
    #[inline]
    fn line_of(&self, square: Square<G>) -> Bitboard<G> {
        if !self.pinned.contains(square) {
            return Bitboard::FULL;
        }
        for &(sq, l) in &self.lines[..self.len] {
            if sq == square {
                return l;
            }
        }
        // Should be unreachable: `pinned` and `lines` stay in sync.
        let _ = self.king_sq;
        Bitboard::FULL
    }
}

/// The enemy roles **actually present** on a board, captured once per node so the
/// cannon verify path's king-attack test ([`king_safe_after`]) scans only the
/// roles that can attack rather than all [`WideRole::COUNT`] of them.
///
/// On the cannon path every sibling move re-tests "is our king attacked" on a
/// fresh post-move occupancy. A role with no enemy piece can never be that
/// attacker, so projecting its pattern from the king is wasted work — and the set
/// of fielded enemy roles does not change across the siblings of one node (a move
/// removes a captured enemy piece only on the post-move board the test reads, and
/// the `pieces(by, role)` mask re-checked there already drops an emptied role).
/// Recording the present roles once turns a 29-iteration loop into one over the
/// ~7–9 roles a cannon variant fields. No geometry data rides here (only role
/// indices), so it is a small `Copy` value built on the node's own stack.
///
/// [`king_safe_after`]: GenericPosition::king_safe_after
/// [`WideRole::COUNT`]: super::role::WideRole::COUNT
#[derive(Clone, Copy)]
struct EnemyAttackers {
    roles: [WideRole; WideRole::COUNT],
    len: usize,
}

impl EnemyAttackers {
    /// Records every role color `by` has at least one piece of on `board`.
    fn new<G: Geometry>(board: &Board<G>, by: Color) -> EnemyAttackers {
        let mut roles = [WideRole::King; WideRole::COUNT];
        let mut len = 0;
        for role in WideRole::ALL {
            if !board.pieces(by, role).is_empty() {
                roles[len] = role;
                len += 1;
            }
        }
        EnemyAttackers { roles, len }
    }

    /// The fielded enemy roles (the prefix actually written).
    #[inline]
    fn roles(&self) -> &[WideRole] {
        &self.roles[..self.len]
    }

    /// The number of fielded enemy roles.
    #[inline]
    fn len(&self) -> usize {
        self.len
    }

    /// The per-role king-reach superset masks, aligned to [`roles`](Self::roles),
    /// computed once per node for the cannon king-safety verify.
    ///
    /// Entry `i` is the variant's [`royal_reach_superset`] for role `roles()[i]`
    /// through `king`, or [`Bitboard::FULL`] when the variant offers no superset
    /// (the "test every piece" sentinel). Returns `None` — disabling the pre-filter
    /// entirely (correct, just unoptimised) — if the node fields more distinct roles
    /// than [`REACH_CAP`], which no cannon variant does. The fixed-width array is
    /// returned by value so it lives on the node's stack and is reused by every
    /// sibling move; capping it at `REACH_CAP` (rather than the full
    /// [`WideRole::COUNT`]) keeps that per-node initialisation small.
    ///
    /// [`royal_reach_superset`]: WideVariant::royal_reach_superset
    /// [`roles`]: Self::roles
    fn reach_supersets<G: Geometry, V: WideVariant<G>>(
        &self,
        king: Square<G>,
    ) -> Option<[Bitboard<G>; REACH_CAP]> {
        if self.len > REACH_CAP {
            // No cannon variant fields this many distinct roles; fall back to the
            // unfiltered path rather than truncate (which would be a correctness
            // bug). Cheap and never taken in practice.
            return None;
        }
        let mut masks = [Bitboard::FULL; REACH_CAP];
        for (i, &role) in self.roles().iter().enumerate() {
            if let Some(mask) = V::royal_reach_superset(role, king) {
                masks[i] = mask;
            }
        }
        Some(masks)
    }
}

/// The maximum number of distinct fielded roles the cannon king-safety reach
/// pre-filter precomputes per node. A cannon variant fields at most ~9 distinct
/// roles (e.g. Xiangqi: General, Advisor, Elephant, Horse, Chariot, Cannon,
/// Soldier), so this bound is never reached; a node that somehow exceeds it simply
/// disables the pre-filter (see [`EnemyAttackers::reach_supersets`]). Sized to keep
/// the per-node stack array small relative to the full [`WideRole::COUNT`].
const REACH_CAP: usize = 12;

/// Returns `true` if the cannon-variant move `mv` is **provably king-safe** by
/// geometry alone: it is an ordinary board move (not a king move, not an
/// en-passant) whose origin and destination both lie off [`king_attack_lines`].
///
/// Such a move leaves no blocker/screen change on any line through the king, so —
/// when the side is not currently in check — the king stays unattacked and the
/// move needs no make/unmake verification. Every other move returns `false` and
/// is routed to the full verify, keeping the produced set byte-identical.
#[inline]
fn cannon_move_off_king_lines<G: Geometry>(
    mv: &WideMove,
    king: Square<G>,
    king_lines: Bitboard<G>,
) -> bool {
    let from = mv.from::<G>();
    // A king move changes the king's own square, and an en-passant removes a third
    // (captured-pawn) square that may sit on a king line: both need full verify.
    if from == king || matches!(mv.kind(), WideMoveKind::EnPassant) {
        return false;
    }
    let to = mv.to::<G>();
    !king_lines.contains(from) && !king_lines.contains(to)
}

/// Sets or clears `sq` in the Alice board-membership mask `bb` to place its
/// occupant on plane B (`plane_b == true`) or plane A (`plane_b == false`).
#[inline]
fn set_plane<G: Geometry>(bb: &mut Bitboard<G>, sq: Square<G>, plane_b: bool) {
    if plane_b {
        bb.set(sq);
    } else {
        bb.clear(sq);
    }
}

/// Returns `true` if both sides have exactly-locatable generals (kings) that
/// **face each other on an open line**: they share a file or a rank with no piece
/// strictly between them. The generic test behind the Janggi bikjang rule
/// (default-off elsewhere).
#[inline]
fn generals_face<G: Geometry>(board: &Board<G>) -> bool {
    let (Some(w), Some(b)) = (board.king_of(Color::White), board.king_of(Color::Black)) else {
        return false;
    };
    if w.file() != b.file() && w.rank() != b.rank() {
        return false;
    }
    (between::<G>(w, b) & board.occupied()).is_empty()
}

/// The union of [`king_attack_lines`] over every royal square in `kings` — every
/// rank, file, and diagonal along which a slider (or the default-off
/// flying-general file) can attack *any* of the side's kings.
///
/// A non-king move whose origin and destination both lie *off* this set adds and
/// removes no blocker on any line through any royal, so it cannot change whether
/// any king is attacked. The multi-royal verify path uses it as a fast-accept
/// test. Costs the four short ray walks per king, paid once and shared by every
/// sibling move of the node.
#[inline]
fn multi_royal_attack_lines<G: Geometry>(kings: Bitboard<G>) -> Bitboard<G> {
    let mut lines = Bitboard::<G>::EMPTY;
    for king in kings {
        lines |= king_attack_lines::<G>(king);
    }
    lines
}

/// Returns `true` if the multi-royal move `mv` is **provably safe** by geometry
/// alone: it moves no king, is not an en-passant, and its origin and destination
/// both lie off [`multi_royal_attack_lines`].
///
/// Such a move leaves no blocker change on any line through any royal, so — when
/// the side is not currently in duple check — every king's attacked-status is
/// unchanged and at least one king stays unattacked: the move needs no
/// make/unmake verification. Every other move returns `false` and is routed to
/// the full verify, keeping the produced set byte-identical. A king move can
/// shift a king onto or off an attacked square, and an en-passant removes a
/// third (captured-pawn) square that may sit on a royal line, so both fall
/// through.
#[inline]
fn multi_royal_move_off_lines<G: Geometry>(
    mv: &WideMove,
    kings: Bitboard<G>,
    royal_lines: Bitboard<G>,
) -> bool {
    let from = mv.from::<G>();
    // A Lion move can remove a piece on its *intermediate* square (an igui / double
    // capture) — a third square, off the from/to pair, that may sit on a royal line
    // and unblock a check. Its from == to for an igui / pass also defeats the
    // origin/destination reasoning. So no Lion move is ever fast-accepted; each is
    // routed to the full make/unmake verify. (Gated behind `has_lion_moves`, so no
    // other variant ever produces one and this is inert for them.)
    if kings.contains(from)
        || matches!(
            mv.kind(),
            WideMoveKind::EnPassant | WideMoveKind::LionMove { .. }
        )
    {
        return false;
    }
    let to = mv.to::<G>();
    !royal_lines.contains(from) && !royal_lines.contains(to)
}

/// The Chebyshev (king-move) distance between two squares — `max(|Δfile|,
/// |Δrank|)`. Used by the Chu Lion generator to tell a genuine distance-two area
/// move from a redundant two-step to an adjacent square.
#[inline]
fn chebyshev<G: Geometry>(a: Square<G>, b: Square<G>) -> u8 {
    let df = (a.file() as i16 - b.file() as i16).unsigned_abs();
    let dr = (a.rank() as i16 - b.rank() as i16).unsigned_abs();
    df.max(dr) as u8
}

/// The back rank (0-based) of `color`: rank `0` for white, the top rank for
/// black.
#[inline]
fn back_rank<G: Geometry>(color: Color) -> u8 {
    match color {
        Color::White => 0,
        Color::Black => G::HEIGHT - 1,
    }
}

impl<G: Geometry, V: WideVariant<G>> GenericPosition<G, V> {
    /// Generates every legal move into a stack-backed [`WideMoveList`], dispatching
    /// to the standard or the special generator exactly as
    /// [`generate_into`](Self::generate_into) does. The reusable-buffer perft
    /// recursion fills one such list per ply and reuses it across sibling nodes,
    /// so it allocates no per-node `Vec`.
    #[inline]
    fn generate_list(&self, out: &mut WideMoveList) {
        if self.uses_standard_path() {
            self.generate_standard_into(out);
        } else {
            self.generate_special_into(out);
        }
    }
}

// -- Free perft functions ---------------------------------------------------

/// Counts the leaf nodes of the legal-move game tree below `position` at the
/// given `depth` — the generic analogue of [`crate::perft`].
///
/// `perft(pos, 0) == 1`. The recursion runs **allocation-free**: each interior
/// ply fills one caller-owned stack-backed `WideMoveList` and reuses it across
/// every sibling node it visits, and the leaf ply (`depth == 1`) is **bulk
/// leaf-counted** — the legal moves are tallied by population count through a
/// `WideCountSink` without ever being materialised (on the standard
/// single-king path; the variant filter paths fall back to counting a reused
/// list). The node counts are byte-identical to the correctness-first reference;
/// only the cost changes.
#[must_use]
pub fn perft<G: Geometry, V: WideVariant<G>>(position: &GenericPosition<G, V>, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    // One move buffer per ply, threaded down the recursion and reused across
    // sibling nodes; standard positions never spill, so this allocates nothing
    // below the root.
    let mut buf = WideMoveList::new();
    // The recursion walks a single mutable position by **make/unmake** (issue
    // #309) rather than cloning the whole position per node, so it works on one
    // owned copy made here at the root; the per-node cost is then a bounded undo
    // record, not a kilobyte-scale board copy.
    let mut scratch = position.clone();
    perft_inner(&mut scratch, depth, &mut buf)
}

/// Recursive core of [`perft`]. The caller owns `buf` (this ply's move buffer)
/// and reuses it across sibling nodes; each frame creates one child buffer on its
/// own stack and threads it down. The position is walked in place by make/unmake:
/// each child is reached with [`apply_with_undo`](GenericPosition::apply_with_undo)
/// and the move undone before the next sibling, so `position` is byte-identical on
/// entry and exit.
fn perft_inner<G: Geometry, V: WideVariant<G>>(
    position: &mut GenericPosition<G, V>,
    depth: u32,
    buf: &mut WideMoveList,
) -> u64 {
    // Bulk leaf counting: at the last ply perft wants only *how many* legal moves
    // there are, so count them directly (population counts over the generators'
    // target masks) instead of building each move and taking the length.
    // `legal_move_count()` drives the identical legal generator, so the count
    // equals `legal_moves().len()` exactly.
    if depth == 1 {
        return position.legal_move_count() as u64;
    }
    buf.clear();
    position.generate_list(buf);
    let mut nodes = 0;
    if depth == 2 {
        // Every child is a leaf: bulk-count it directly, no child buffer needed.
        buf.for_each(|mv| {
            let undo = position.apply_with_undo(&mv);
            nodes += position.legal_move_count() as u64;
            position.undo(undo);
        });
        return nodes;
    }
    // One child buffer on this frame's stack, reused for every child node.
    let mut child = WideMoveList::new();
    buf.for_each(|mv| {
        let undo = position.apply_with_undo(&mv);
        nodes += perft_inner(position, depth - 1, &mut child);
        position.undo(undo);
    });
    nodes
}

/// Like [`perft`], but returns the per-root-move leaf counts — the breakdown for
/// debugging a mismatching total against a reference.
#[must_use]
pub fn perft_divide<G: Geometry, V: WideVariant<G>>(
    position: &GenericPosition<G, V>,
    depth: u32,
) -> Vec<(WideMove, u64)> {
    let mut out = Vec::new();
    if depth == 0 {
        return out;
    }
    // Walk the root moves by make/unmake on one owned copy (issue #309), mirroring
    // `perft`; the per-move subtree count is byte-identical to cloning and playing.
    let moves = position.legal_moves();
    let mut scratch = position.clone();
    for mv in moves {
        let count = if depth == 1 {
            1
        } else {
            let undo = scratch.apply_with_undo(&mv);
            let count = perft(&scratch, depth - 1);
            scratch.undo(undo);
            count
        };
        out.push((mv, count));
    }
    out
}

// -- FEN sub-parsers --------------------------------------------------------

/// The error returned when a FEN string cannot be parsed into a
/// [`GenericPosition`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WideFenError {
    /// A required field was missing.
    MissingField,
    /// The placement field was invalid.
    Placement(super::ParseBoardError),
    /// The side-to-move field was neither `w` nor `b`.
    BadTurn,
    /// The castling field was malformed.
    BadCastling,
    /// The en-passant field was malformed.
    BadEnPassant,
    /// A clock field was not a non-negative integer.
    BadClock,
    /// Extra fields followed the six expected ones.
    TrailingData,
}

impl core::fmt::Display for WideFenError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            WideFenError::MissingField => f.write_str("FEN is missing a required field"),
            WideFenError::Placement(e) => write!(f, "FEN placement field is invalid: {e}"),
            WideFenError::BadTurn => f.write_str("FEN side-to-move field is not 'w' or 'b'"),
            WideFenError::BadCastling => f.write_str("FEN castling field is malformed"),
            WideFenError::BadEnPassant => f.write_str("FEN en-passant field is malformed"),
            WideFenError::BadClock => f.write_str("FEN clock field is not an integer"),
            WideFenError::TrailingData => f.write_str("FEN has trailing data after six fields"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for WideFenError {}

/// Parses the castling field into [`GenericCastling`], reading each named rook's
/// start file from the board.
///
/// Two dialects are accepted, mirroring Fairy-Stockfish:
///
/// * **X-FEN** — `K`/`k` name the kingside (rightmost) rook and `Q`/`q` the
///   queenside (leftmost) rook of white / black; the rook's start file is read off
///   that side's back rank. This is the only form a non-randomised start ever
///   uses, so every existing variant keeps the identical `KQkq` parse.
/// * **Shredder-FEN** — an explicit rook-file letter (uppercase = white,
///   lowercase = black) names the rook directly by its start file, with the side
///   inferred from the rook's file relative to its king. This is how FSF writes a
///   randomised (Chess960-style) back rank, e.g. Capablanca's `JAja`. A file
///   letter is unambiguous on the ≤10-wide castling boards because the X-FEN
///   markers `K`/`Q` fall outside the `a..j` file range. The arm only fires on
///   input the `KQkq` parser previously rejected, so it adds support without
///   changing any non-randomised result.
fn parse_castling<G: Geometry, V: WideVariant<G>>(
    field: &str,
    board: &Board<G>,
) -> Result<GenericCastling, WideFenError> {
    let mut rights = GenericCastling::NONE;
    if field == "-" {
        return Ok(rights);
    }
    for ch in field.chars() {
        match ch {
            'K' | 'Q' | 'k' | 'q' => {
                let (color, side) = match ch {
                    'K' => (Color::White, KINGSIDE),
                    'Q' => (Color::White, QUEENSIDE),
                    'k' => (Color::Black, KINGSIDE),
                    _ => (Color::Black, QUEENSIDE),
                };
                // The rank the king and rooks castle on — the back rank by default,
                // but a variant (Shako) may place them on a different rank.
                let rank = V::castle_rank(color);
                let file = outermost_rook_file::<G>(board, color, side, rank)
                    .ok_or(WideFenError::BadCastling)?;
                rights.set(color, side, Some(file));
            }
            // Shredder-FEN: an explicit rook-file letter.
            'A'..='Z' | 'a'..='z' => {
                let color = if ch.is_ascii_uppercase() {
                    Color::White
                } else {
                    Color::Black
                };
                let file = (ch.to_ascii_lowercase() as u8) - b'a';
                if file >= G::WIDTH {
                    return Err(WideFenError::BadCastling);
                }
                let rank = V::castle_rank(color);
                // The named square must actually hold a friendly rook, and the king
                // must share the castle rank so its file decides the side.
                let sq =
                    Square::<G>::from_file_rank(file, rank).ok_or(WideFenError::BadCastling)?;
                if board.piece_at(sq) != Some(WidePiece::new(color, WideRole::Rook)) {
                    return Err(WideFenError::BadCastling);
                }
                let king_file = board
                    .king_of(color)
                    .filter(|k| k.rank() == rank)
                    .map(Square::file)
                    .ok_or(WideFenError::BadCastling)?;
                let side = if file > king_file {
                    KINGSIDE
                } else {
                    QUEENSIDE
                };
                rights.set(color, side, Some(file));
            }
            _ => return Err(WideFenError::BadCastling),
        }
    }
    Ok(rights)
}

/// Writes the castling field, or `-` if no rights remain.
///
/// The default is `KQkq` order. A [`shredder_castling_fen`](WideVariant::shredder_castling_fen)
/// variant (Caparandom) instead writes explicit rook-**file** letters — uppercase
/// for White, lowercase for Black, in the same king-side-then-queen-side per-colour
/// order — so its arbitrary rook files round-trip and match Fairy-Stockfish (e.g.
/// the Capablanca-random startpos renders `JAja`).
fn write_castling<G: Geometry, V: WideVariant<G>>(castling: GenericCastling, out: &mut String) {
    if !V::shredder_castling_fen() {
        write_castling_kqkq(castling, out);
        return;
    }
    let before = out.len();
    for (color, side) in [
        (Color::White, KINGSIDE),
        (Color::White, QUEENSIDE),
        (Color::Black, KINGSIDE),
        (Color::Black, QUEENSIDE),
    ] {
        if let Some(file) = castling.rook_file(color, side) {
            let letter = (b'a' + file) as char;
            out.push(if color.is_white() {
                letter.to_ascii_uppercase()
            } else {
                letter
            });
        }
    }
    if out.len() == before {
        out.push('-');
    }
}

/// Writes the castling field in the standard `KQkq` order, or `-` if no rights
/// remain. The default form for every non-Shredder variant, and the castling part
/// of a gating variant's combined field.
fn write_castling_kqkq(castling: GenericCastling, out: &mut String) {
    let before = out.len();
    if castling.rook_file(Color::White, KINGSIDE).is_some() {
        out.push('K');
    }
    if castling.rook_file(Color::White, QUEENSIDE).is_some() {
        out.push('Q');
    }
    if castling.rook_file(Color::Black, KINGSIDE).is_some() {
        out.push('k');
    }
    if castling.rook_file(Color::Black, QUEENSIDE).is_some() {
        out.push('q');
    }
    if out.len() == before {
        out.push('-');
    }
}

// -- Gating (Seirawan) FEN helpers ------------------------------------------

/// Splits a `[..]`-bracketed holdings suffix off a placement field, returning the
/// bare placement and the holdings string (empty if there was no bracket).
///
/// Seirawan (and crazyhouse) append the reserves in hand after the board as
/// `rnbqkbnr/.../RNBQKBNR[HEhe]`. The board parser cannot see the bracket, so it
/// is split here.
fn split_holdings(placement_field: &str) -> (&str, &str) {
    match placement_field.split_once('[') {
        Some((board, rest)) => (board, rest.strip_suffix(']').unwrap_or(rest)),
        None => (placement_field, ""),
    }
}

/// Parses a combined castling-and-gating field for a gating variant, together
/// with the holdings string, into the castling rights and the gating state.
///
/// The field interleaves `KQkq` castling letters with gating-square file letters
/// (uppercase = white's back rank, lowercase = black's): e.g.
/// `KQBCDFGkqbcdfg`. A castling letter additionally makes its rook square (and,
/// since the king is then unmoved, the king square) gating-eligible — these
/// redundancies are not spelled out explicitly, matching the FSF dialect.
fn parse_castling_and_gating<G: Geometry>(
    field: &str,
    holdings: &str,
    board: &Board<G>,
) -> Result<(GenericCastling, GenericGating<G>), WideFenError> {
    let mut castling = GenericCastling::NONE;
    let mut eligible = Bitboard::<G>::EMPTY;

    if field != "-" {
        for ch in field.chars() {
            match ch {
                'K' | 'Q' | 'k' | 'q' => {
                    let (color, side) = match ch {
                        'K' => (Color::White, KINGSIDE),
                        'Q' => (Color::White, QUEENSIDE),
                        'k' => (Color::Black, KINGSIDE),
                        _ => (Color::Black, QUEENSIDE),
                    };
                    let rank = back_rank::<G>(color);
                    let rook_file = outermost_rook_file::<G>(board, color, side, rank)
                        .ok_or(WideFenError::BadCastling)?;
                    castling.set(color, side, Some(rook_file));
                    // The rook and (unmoved) king squares are gating-eligible.
                    if let Some(sq) = Square::<G>::from_file_rank(rook_file, rank) {
                        eligible.set(sq);
                    }
                    if let Some(king) = board.king_of(color) {
                        if king.rank() == rank {
                            eligible.set(king);
                        }
                    }
                }
                // An explicit file letter marks a gating-eligible back-rank
                // square: uppercase for white (rank 0), lowercase for black.
                'A'..='Z' => mark_gating_file::<G>(&mut eligible, ch, Color::White)?,
                'a'..='z' => mark_gating_file::<G>(&mut eligible, ch, Color::Black)?,
                _ => return Err(WideFenError::BadCastling),
            }
        }
    }

    let (white, black) = parse_holdings(holdings)?;
    Ok((castling, GenericGating::new(eligible, white, black)))
}

/// Marks the gating-eligible back-rank square named by a file letter (`a`..`z`,
/// case already classified to `color`) in `eligible`.
fn mark_gating_file<G: Geometry>(
    eligible: &mut Bitboard<G>,
    ch: char,
    color: Color,
) -> Result<(), WideFenError> {
    let file = (ch.to_ascii_lowercase() as u8).wrapping_sub(b'a');
    if file >= G::WIDTH {
        return Err(WideFenError::BadCastling);
    }
    let rank = back_rank::<G>(color);
    if let Some(sq) = Square::<G>::from_file_rank(file, rank) {
        eligible.set(sq);
    }
    Ok(())
}

/// The outermost rook file for a color/side on `rank`: the rightmost rook for the
/// kingside, the leftmost for the queenside.
fn outermost_rook_file<G: Geometry>(
    board: &Board<G>,
    color: Color,
    side: usize,
    rank: u8,
) -> Option<u8> {
    let rooks = board.pieces(color, WideRole::Rook);
    let mut chosen: Option<u8> = None;
    for file in 0..G::WIDTH {
        if let Some(sq) = Square::<G>::from_file_rank(file, rank) {
            if rooks.contains(sq) {
                match side {
                    KINGSIDE => chosen = Some(file),
                    _ => {
                        if chosen.is_none() {
                            chosen = Some(file);
                        }
                    }
                }
            }
        }
    }
    chosen
}

/// Parses the `[HEhe]`-style holdings string into per-color `[hawk, elephant]`
/// reserve availability. Uppercase letters are white's reserves, lowercase
/// black's; the Hawk is `H`/`h` and the Elephant `E`/`e` (the FSF S-Chess
/// dialect). Any other letter is rejected.
fn parse_holdings(holdings: &str) -> Result<([bool; 2], [bool; 2]), WideFenError> {
    let mut white = [false; 2];
    let mut black = [false; 2];
    for ch in holdings.chars() {
        match ch {
            'H' => white[0] = true,
            'E' => white[1] = true,
            'h' => black[0] = true,
            'e' => black[1] = true,
            _ => return Err(WideFenError::BadCastling),
        }
    }
    Ok((white, black))
}

/// Parses a placement-phase `[..]` holdings string into a [`GenericPlacement`]
/// pocket. Uppercase letters tally white's undeployed pieces, lowercase black's,
/// each letter the role's FEN character (mce dialect — the Met is `m`, the Silver
/// `s`). Any letter that is not a known role is rejected.
fn parse_placement_holdings(holdings: &str) -> Result<GenericPlacement, WideFenError> {
    let mut pocket = GenericPlacement::NONE;
    let mut chars = holdings.chars();
    while let Some(ch) = chars.next() {
        // An overflow role's token is `*` + a recycled base letter whose case
        // carries the colour (e.g. Shinobi's Shogi Knight `*N` / `*n`), mirroring
        // the board placement's overflow handling. A bare letter is an ordinary
        // role.
        let (role, white) = if ch == crate::geometry::role::OVERFLOW_PREFIX {
            let next = chars.next().ok_or(WideFenError::BadCastling)?;
            // A doubled prefix `**` marks a second-bank overflow role (e.g. the
            // Mansindam Angel `**a`); a single `*` an ordinary overflow role.
            if next == crate::geometry::role::OVERFLOW_PREFIX {
                let base = chars.next().ok_or(WideFenError::BadCastling)?;
                let role = WideRole::overflow2_from_base(base).ok_or(WideFenError::BadCastling)?;
                (role, base.is_ascii_uppercase())
            } else {
                let role = WideRole::overflow_from_base(next).ok_or(WideFenError::BadCastling)?;
                (role, next.is_ascii_uppercase())
            }
        } else if ch == crate::geometry::role::OVERFLOW_PREFIX_3 {
            // A held third-tier overflow role (the Cannon Shogi cannon army) is
            // `=` + a recycled base letter, mirroring the `*` overflow handling.
            let base = chars.next().ok_or(WideFenError::BadCastling)?;
            let role = WideRole::overflow3_from_base(base).ok_or(WideFenError::BadCastling)?;
            (role, base.is_ascii_uppercase())
        } else {
            let role = WideRole::from_char(ch).ok_or(WideFenError::BadCastling)?;
            (role, ch.is_ascii_uppercase())
        };
        let counts = if white {
            &mut pocket.white
        } else {
            &mut pocket.black
        };
        counts[role.index()] = counts[role.index()].saturating_add(1);
    }
    Ok(pocket)
}

/// Writes the placement-phase `[..]` holdings bracket: white's undeployed pieces
/// (uppercase) then black's (lowercase), each role in [`WideRole::ALL`] index
/// order, repeated by its count. An empty pocket (both sides fully deployed)
/// emits `[]`, matching FSF's rendering once the setup phase is over.
fn write_placement_holdings(placement: GenericPlacement, out: &mut String) {
    out.push('[');
    for (color, upper) in [(Color::White, true), (Color::Black, false)] {
        for role in WideRole::ALL {
            let n = placement.count(color, role);
            let ch = if upper {
                role.upper_char()
            } else {
                role.char()
            };
            for _ in 0..n {
                // An overflow role (e.g. Shinobi's Shogi Knight `*N`) has no bare
                // letter: its token is the `*` prefix plus the recycled base
                // letter, the case already encoded in `ch` above. A second-bank
                // overflow role (the Mansindam Angel `**A`) doubles the prefix.
                if role.is_overflow2() {
                    out.push(crate::geometry::role::OVERFLOW_PREFIX);
                    out.push(crate::geometry::role::OVERFLOW_PREFIX);
                } else if role.is_overflow() {
                    out.push(crate::geometry::role::OVERFLOW_PREFIX);
                } else if role.is_overflow3() {
                    // A held third-tier overflow role (the Cannon Shogi cannon
                    // army) has no bare letter: its token is the `=` prefix plus the
                    // recycled base letter, the case already encoded in `ch` above.
                    out.push(crate::geometry::role::OVERFLOW_PREFIX_3);
                }
                out.push(ch);
            }
        }
    }
    out.push(']');
}

/// Writes the `[..]` holdings bracket for a gating variant: the white reserves
/// (uppercase) then the black reserves (lowercase), Hawk before Elephant. An
/// empty hand emits `[]`.
fn write_holdings<G: Geometry>(gating: GenericGating<G>, out: &mut String) {
    out.push('[');
    for (color, hawk, eleph) in [(Color::White, 'H', 'E'), (Color::Black, 'h', 'e')] {
        if gating.has_reserve(color, GateRole::Hawk) {
            out.push(hawk);
        }
        if gating.has_reserve(color, GateRole::Elephant) {
            out.push(eleph);
        }
    }
    out.push(']');
}

/// Writes the combined castling-and-gating field for a gating variant: the
/// `KQkq` castling letters followed by the explicit gating-square file letters
/// not already implied by a castling right (the rook square it names, and the
/// king square — since a castling right means the king is unmoved). `kings` is
/// `[white_king, black_king]`. Emits `-` if there is neither a castling right nor
/// an eligible square.
fn write_castling_and_gating<G: Geometry>(
    castling: GenericCastling,
    gating: GenericGating<G>,
    kings: [Option<Square<G>>; 2],
    out: &mut String,
) {
    let before = out.len();
    write_castling_kqkq(castling, out);
    // `write_castling_kqkq` writes `-` for no rights; strip it so we can append gating
    // letters (and re-add `-` only if nothing at all is written below).
    if out.ends_with('-') {
        out.pop();
    }

    let eligible = gating.eligible();
    for (color, king, upper) in [
        (Color::White, kings[0], true),
        (Color::Black, kings[1], false),
    ] {
        let rank = back_rank::<G>(color);
        // Squares already implied by a castling right (rook squares named, and
        // the king square since the king is then unmoved) are not re-listed.
        let mut implied = Bitboard::<G>::EMPTY;
        for side in [KINGSIDE, QUEENSIDE] {
            if let Some(file) = castling.rook_file(color, side) {
                if let Some(sq) = Square::<G>::from_file_rank(file, rank) {
                    implied.set(sq);
                }
            }
        }
        if castling.has_any(color) {
            if let Some(king) = king {
                implied.set(king);
            }
        }
        for sq in eligible {
            if sq.rank() != rank || implied.contains(sq) {
                continue;
            }
            let ch = (b'a' + sq.file()) as char;
            out.push(if upper { ch.to_ascii_uppercase() } else { ch });
        }
    }

    if out.len() == before {
        out.push('-');
    }
}

// -- Duck (Duck chess) FEN helpers ------------------------------------------

/// Splits the neutral Duck's `*` cell out of a placement field, returning the
/// placement with the duck cell rewritten as one empty square and the duck's
/// square (`None` if the position has no duck on the board yet).
///
/// The `*` occupies one file like a piece letter; replacing it with a blank
/// lets the real-piece board parser consume the rest unchanged. At most one `*`
/// is allowed.
fn split_duck<G: Geometry>(placement: &str) -> Result<(String, Option<Square<G>>), WideFenError> {
    let width = G::WIDTH;
    let height = G::HEIGHT;
    let mut out = String::with_capacity(placement.len());
    let mut duck: Option<Square<G>> = None;

    let _ = width;
    for (rank_from_top, rank_str) in placement.split('/').enumerate() {
        if rank_from_top > 0 {
            out.push('/');
        }
        if rank_from_top as u8 >= height {
            // Let the board parser report the structural error; just copy through.
            out.push_str(rank_str);
            continue;
        }
        let rank = height - 1 - rank_from_top as u8;
        let mut file: u32 = 0;
        // Re-emit the rank with the duck cell turned into one empty square,
        // tracking a running empty-run so an inserted blank merges cleanly with
        // adjacent empty counts rather than concatenating into a larger digit run.
        let mut empty: u32 = 0;
        let bytes = rank_str.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if b == b'*' {
                if duck.is_some() {
                    return Err(WideFenError::Placement(
                        super::ParseBoardError::InvalidChar('*'),
                    ));
                }
                let sq = Square::<G>::from_file_rank(file as u8, rank)
                    .ok_or(WideFenError::BadEnPassant)?;
                duck = Some(sq);
                empty += 1; // the duck cell is empty for the board parser
                file += 1;
                i += 1;
            } else if b.is_ascii_digit() {
                let mut skip: u32 = 0;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    skip = skip
                        .saturating_mul(10)
                        .saturating_add((bytes[i] - b'0') as u32);
                    i += 1;
                }
                empty = empty.saturating_add(skip);
                file = file.saturating_add(skip);
            } else {
                flush_empty(&mut out, &mut empty);
                out.push(b as char);
                file = file.saturating_add(1);
                i += 1;
            }
        }
        flush_empty(&mut out, &mut empty);
    }
    Ok((out, duck))
}

/// Renders a placement field with the neutral Duck shown as a `*` on its square.
/// The inverse of [`split_duck`]; iterates per cell like
/// [`Board::to_fen_placement`] but emits `*` on the duck square.
fn placement_with_duck<G: Geometry>(board: &Board<G>, duck: Option<Square<G>>) -> String {
    let width = G::WIDTH;
    let height = G::HEIGHT;
    let mut fen = String::with_capacity(width as usize * height as usize + height as usize);
    for rank_from_top in 0..height {
        let rank = height - 1 - rank_from_top;
        let mut empty: u32 = 0;
        for file in 0..width {
            let square = Square::<G>::new(rank * width + file);
            let is_duck = duck == Some(square);
            match (board.piece_at(square), is_duck) {
                (Some(piece), _) => {
                    flush_empty(&mut fen, &mut empty);
                    fen.push(piece.char());
                }
                (None, true) => {
                    flush_empty(&mut fen, &mut empty);
                    fen.push('*');
                }
                (None, false) => empty += 1,
            }
        }
        flush_empty(&mut fen, &mut empty);
        if rank > 0 {
            fen.push('/');
        }
    }
    fen
}

/// Strips the crazyhouse promoted markers (`~`, a suffix on a piece token) out of
/// a placement field, returning the bare placement (which the board parser
/// understands) and the mask of squares whose occupant is promoted. A `~` not
/// preceded by a piece on its rank is rejected.
fn split_promoted<G: Geometry>(placement: &str) -> Result<(String, Bitboard<G>), WideFenError> {
    let height = G::HEIGHT;
    let mut out = String::with_capacity(placement.len());
    let mut promoted = Bitboard::<G>::EMPTY;

    for (rank_from_top, rank_str) in placement.split('/').enumerate() {
        if rank_from_top > 0 {
            out.push('/');
        }
        if rank_from_top as u8 >= height {
            // Let the board parser report the structural error; just copy through.
            out.push_str(rank_str);
            continue;
        }
        let rank = height - 1 - rank_from_top as u8;
        let mut file: u32 = 0;
        // The square the most recent piece letter on this rank occupies — the one a
        // `~` would mark as promoted.
        let mut last_sq: Option<Square<G>> = None;
        let bytes = rank_str.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if b == b'~' {
                let sq = last_sq.ok_or(WideFenError::Placement(
                    super::ParseBoardError::InvalidChar('~'),
                ))?;
                promoted.set(sq);
                // The marker is consumed and never re-emitted; it advances no file.
                last_sq = None;
                i += 1;
            } else if b.is_ascii_digit() {
                let mut skip: u32 = 0;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    skip = skip
                        .saturating_mul(10)
                        .saturating_add((bytes[i] - b'0') as u32);
                    out.push(bytes[i] as char);
                    i += 1;
                }
                file = file.saturating_add(skip);
                last_sq = None;
            } else {
                last_sq = Square::<G>::from_file_rank(file as u8, rank);
                out.push(b as char);
                file = file.saturating_add(1);
                i += 1;
            }
        }
    }
    Ok((out, promoted))
}

/// Renders a placement field with each promoted piece carrying a trailing `~`
/// (`Q~`). The inverse of [`split_promoted`]; iterates per cell like
/// [`Board::to_fen_placement`] but appends `~` after a piece on a promoted square.
fn placement_with_promoted<G: Geometry>(board: &Board<G>, promoted: Bitboard<G>) -> String {
    let width = G::WIDTH;
    let height = G::HEIGHT;
    let mut fen = String::with_capacity(width as usize * height as usize + height as usize);
    for rank_from_top in 0..height {
        let rank = height - 1 - rank_from_top;
        let mut empty: u32 = 0;
        for file in 0..width {
            let square = Square::<G>::new(rank * width + file);
            match board.piece_at(square) {
                Some(piece) => {
                    flush_empty(&mut fen, &mut empty);
                    fen.push(piece.char());
                    if promoted.contains(square) {
                        fen.push('~');
                    }
                }
                None => empty += 1,
            }
        }
        flush_empty(&mut fen, &mut empty);
        if rank > 0 {
            fen.push('/');
        }
    }
    fen
}

/// Flushes a pending empty-square run into a FEN rank as its decimal count.
fn flush_empty(out: &mut String, empty: &mut u32) {
    if *empty > 0 {
        push_decimal(out, *empty);
        *empty = 0;
    }
}

/// Parses the en-passant field (`-` or a `file`+`rank` coordinate) into a
/// square, mapping the file letter and 1-based rank number to an index.
fn parse_ep<G: Geometry>(field: &str) -> Result<Option<Square<G>>, WideFenError> {
    if field == "-" {
        return Ok(None);
    }
    let bytes = field.as_bytes();
    if bytes.is_empty() {
        return Err(WideFenError::BadEnPassant);
    }
    let file_ch = bytes[0];
    if !file_ch.is_ascii_lowercase() {
        return Err(WideFenError::BadEnPassant);
    }
    let file = file_ch - b'a';
    // The remaining bytes are the 1-based rank number (one or more digits).
    let rank_str = &field[1..];
    if rank_str.is_empty() {
        return Err(WideFenError::BadEnPassant);
    }
    let rank_no: u32 = rank_str.parse().map_err(|_| WideFenError::BadEnPassant)?;
    if rank_no == 0 {
        return Err(WideFenError::BadEnPassant);
    }
    let rank = (rank_no - 1) as u8;
    Square::<G>::from_file_rank(file, rank)
        .map(Some)
        .ok_or(WideFenError::BadEnPassant)
}

/// Writes a square as a `file`-letter + 1-based-rank coordinate.
fn write_square<G: Geometry>(out: &mut String, sq: Square<G>) {
    out.push((b'a' + sq.file()) as char);
    push_decimal(out, sq.rank() as u32 + 1);
}

/// Parses a non-negative decimal clock field.
fn parse_clock(field: &str) -> Result<u16, WideFenError> {
    let v: u32 = field.parse().map_err(|_| WideFenError::BadClock)?;
    Ok(v.min(u16::MAX as u32) as u16)
}

/// Appends a decimal integer to `out`.
fn push_decimal(out: &mut String, mut n: u32) {
    if n == 0 {
        out.push('0');
        return;
    }
    let mut digits = [0u8; 10];
    let mut i = 0;
    while n > 0 {
        digits[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        out.push(digits[i] as char);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Chess8x8, StandardChess};

    type Pos = GenericPosition<Chess8x8, StandardChess>;

    const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    #[test]
    fn startpos_round_trips_through_fen() {
        let pos = Pos::startpos();
        assert_eq!(pos.to_fen(), STARTPOS);
        let parsed = Pos::from_fen(STARTPOS).expect("valid");
        assert_eq!(parsed.to_fen(), STARTPOS);
        assert_eq!(pos.turn(), Color::White);
        assert_eq!(pos.legal_move_count(), 20);
        assert!(!pos.is_check());
        assert_eq!(pos.outcome(), None);
    }

    #[test]
    fn fen_round_trips_with_ep_and_clocks() {
        let fen = "rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq c6 0 2";
        let pos = Pos::from_fen(fen).expect("valid");
        assert_eq!(pos.to_fen(), fen);
        assert_eq!(pos.ep_square().map(|s| s.index()), Some(42)); // c6
    }

    #[test]
    fn detects_checkmate_and_stalemate() {
        // Fool's mate position: black has just mated white.
        let mate = "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3";
        let pos = Pos::from_fen(mate).expect("valid");
        assert!(pos.is_check());
        assert!(pos.legal_moves().is_empty());
        assert_eq!(pos.end_reason(), Some(WideEndReason::Checkmate));
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::Black
            })
        );

        // A classic stalemate: black to move, no legal move, not in check.
        let stale = "7k/5Q2/6K1/8/8/8/8/8 b - - 0 1";
        let pos = Pos::from_fen(stale).expect("valid");
        assert!(!pos.is_check());
        assert!(pos.legal_moves().is_empty());
        assert_eq!(pos.end_reason(), Some(WideEndReason::Stalemate));
        assert_eq!(pos.outcome(), Some(WideOutcome::Draw));
    }

    #[test]
    fn play_matches_fen_after_a_move() {
        let pos = Pos::startpos();
        // Find the e2e4 double push and play it.
        let mv = pos
            .legal_moves()
            .into_iter()
            .find(|m| m.to_uci::<Chess8x8>() == "e2e4")
            .expect("e2e4 is legal at the start");
        let next = pos.play(&mv);
        assert_eq!(
            next.to_fen(),
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1"
        );
    }

    #[test]
    fn castling_field_round_trips_partial_rights() {
        let fen = "r3k2r/8/8/8/8/8/8/R3K2R w Kq - 0 1";
        let pos = Pos::from_fen(fen).expect("valid");
        assert_eq!(pos.to_fen(), fen);
    }

    // -- Zobrist position key ---------------------------------------------

    #[test]
    fn zobrist_is_deterministic_and_splits_into_parts() {
        let pos = Pos::startpos();
        // Stable across repeated computation.
        assert_eq!(pos.zobrist(), pos.zobrist());
        assert_eq!(pos.repetition_key(), pos.zobrist());
        // The board and state halves XOR to the whole key.
        assert_eq!(
            pos.zobrist(),
            pos.zobrist_board_part() ^ pos.zobrist_state_part()
        );
        // Two FENs for the same position hash equal.
        let same = Pos::from_fen(STARTPOS).expect("valid");
        assert_eq!(same.zobrist(), pos.zobrist());
    }

    #[test]
    fn zobrist_distinguishes_one_ply() {
        let start = Pos::startpos();
        let mv = start
            .legal_moves()
            .into_iter()
            .find(|m| m.to_uci::<Chess8x8>() == "e2e4")
            .expect("e2e4 legal");
        let after = start.play(&mv);
        assert_ne!(after.zobrist(), start.zobrist());
    }

    /// Walks the legal-move tree to `depth`, asserting at every node that the
    /// **incremental** Zobrist update — the old state half XORed out, the board
    /// delta from the [`Undo`] XORed in, the new state half XORed in — reproduces a
    /// from-scratch [`zobrist`](GenericPosition::zobrist) recompute exactly, and that
    /// `undo` then restores the parent key. This exercises the same machinery
    /// [`GenericGame`](super::super::game::GenericGame) uses to maintain the key.
    fn walk_incremental_zobrist<G: Geometry, V: WideVariant<G>>(
        pos: &mut GenericPosition<G, V>,
        depth: u32,
    ) {
        let before = pos.zobrist();
        if depth == 0 {
            return;
        }
        for mv in pos.legal_moves() {
            let old_state = pos.zobrist_state_part();
            let undo = pos.apply_with_undo(&mv);
            let incremental =
                before ^ pos.zobrist_board_delta(&undo) ^ old_state ^ pos.zobrist_state_part();
            assert_eq!(
                incremental,
                pos.zobrist(),
                "incremental key diverged from recompute after {}",
                mv.to_uci::<G>(),
            );
            walk_incremental_zobrist(pos, depth - 1);
            pos.undo(undo);
            assert_eq!(pos.zobrist(), before, "undo did not restore the key");
        }
    }

    #[test]
    fn incremental_zobrist_matches_recompute_standard() {
        walk_incremental_zobrist(&mut Pos::startpos(), 3);
        // "Kiwipete": captures, castling, and en passant are all on offer right
        // away, exercising the board delta for every standard move kind at shallow
        // depth.
        let mut kiwipete =
            Pos::from_fen("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1")
                .expect("valid");
        walk_incremental_zobrist(&mut kiwipete, 3);
    }

    #[test]
    fn incremental_zobrist_matches_recompute_fairy_variants() {
        use crate::geometry::variants::{
            Alice, Bughouse, Capahouse, Duck, Janggi, Makruk, Minishogi, Minixiangqi, Seirawan,
            Shouse, Sittuyin, Xiangqi,
        };

        // One walk per variant, exercising hands (crazyhouse / S-House), gating
        // (Seirawan / S-House), Alice planes, the Duck, the placement pocket
        // (Sittuyin), the promoted mask (Capahouse), and the wide boards. Depth is
        // kept shallow on the heavy generic boards (drops and gating fan the tree
        // out fast) so the test stays quick; the deep board-delta coverage comes
        // from the depth-4 standard-chess walk above, and the small boards walk a
        // little deeper here. Hand / promoted-mask hashing is additionally covered
        // by the crazyhouse `pockets_affect_zobrist` / `promoted_mask_affects_zobrist`
        // unit tests.
        walk_incremental_zobrist(&mut Bughouse::startpos(), 2);
        walk_incremental_zobrist(&mut Capahouse::startpos(), 2);
        walk_incremental_zobrist(&mut Seirawan::startpos(), 2);
        walk_incremental_zobrist(&mut Shouse::startpos(), 2);
        walk_incremental_zobrist(&mut Alice::startpos(), 2);
        walk_incremental_zobrist(&mut Duck::startpos(), 2);
        walk_incremental_zobrist(&mut Sittuyin::startpos(), 2);
        walk_incremental_zobrist(&mut Makruk::startpos(), 2);
        walk_incremental_zobrist(&mut Minishogi::startpos(), 3);
        walk_incremental_zobrist(&mut Xiangqi::startpos(), 2);
        walk_incremental_zobrist(&mut Minixiangqi::startpos(), 3);
        walk_incremental_zobrist(&mut Janggi::startpos(), 2);
    }
}
