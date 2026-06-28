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
//! [`crate::Position::perft`] exactly (see `tests/perft_generic.rs`). The
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

use super::attacks::{between, line};
use super::role::WideRole;
use super::variant::{WideEndReason, WideVariant};
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
/// move generation and [`apply`](GenericPosition::apply). For every non-gating
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
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
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
            .finish()
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
    _variant: PhantomData<V>,
}

impl<G: Geometry, V: WideVariant<G>> core::fmt::Debug for GenericPosition<G, V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GenericPosition")
            .field("placement", &self.board.to_fen_placement())
            .field("state", &self.state)
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

    /// Returns the side to move.
    #[must_use]
    #[inline]
    pub fn turn(&self) -> Color {
        self.state.turn
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
            // pattern back from `sq`. Every other role's attack set is symmetric
            // (a attacks b iff b attacks a under the same occupancy).
            let from_sq = if role == WideRole::Pawn || role == WideRole::Hoplite {
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
    pub fn is_check(&self) -> bool {
        let us = self.state.turn;
        let them = us.opposite();
        let occ = self.board.occupied();
        let royals = V::royal_squares(&self.board, us);
        if royals.is_empty() {
            return false;
        }
        let attacked = |sq| !self.attackers_to(sq, them, occ).is_empty();
        if V::multi_royal() {
            // Duple check: in check only when no royal is left unattacked.
            royals.into_iter().all(attacked)
        } else {
            royals.into_iter().any(attacked)
        }
    }

    /// Returns the squares attacked by color `by` under `occupied` — the
    /// king-danger map (the squares the other king may not step onto). Pawns use
    /// their diagonal attack pattern.
    fn attacked_by(&self, by: Color, occupied: Bitboard<G>) -> Bitboard<G> {
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
                        pins.add(pinned, l);
                    }
                }
            }
        }
        pins
    }

    // -- Move generation ---------------------------------------------------

    /// Generates every legal move for the side to move.
    #[must_use]
    pub fn legal_moves(&self) -> Vec<WideMove> {
        let mut out = Vec::new();
        self.generate_into(&mut out);
        out
    }

    /// Returns the number of legal moves (perft depth-1 leaf count). Materializes
    /// the list; correctness-first.
    #[must_use]
    pub fn legal_move_count(&self) -> usize {
        self.legal_moves().len()
    }

    /// Pushes every legal move into `out`.
    fn generate_into(&self, out: &mut Vec<WideMove>) {
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
        let us = self.state.turn;
        let them = us.opposite();
        let board = &self.board;
        let occupied = board.occupied();
        let our_pieces = board.by_color(us);
        let their_pieces = board.by_color(them);

        let king_sq = match board.king_of(us) {
            Some(sq) => sq,
            None => return,
        };

        let checkers = self.attackers_to(king_sq, them, occupied);
        let num_checkers = checkers.count();

        // King-danger: squares attacked by the enemy with our king lifted out
        // of the occupancy, so it cannot shield itself along a slider ray.
        let occ_without_king = occupied.without(king_sq);
        let king_danger = self.attacked_by(them, occ_without_king);

        // King moves are always generated (the only legal moves under double
        // check).
        let king_targets =
            V::role_attacks(WideRole::King, us, king_sq, occupied) & !our_pieces & !king_danger;
        emit_targets(out, king_sq, king_targets, their_pieces);

        if num_checkers >= 2 {
            // Double check: only king moves are legal.
            self.append_drops(out);
            return;
        }

        // The mask of squares that resolve a single check: capture the checker
        // or block between it and the king. Full board when not in check.
        let check_mask = if num_checkers == 1 {
            let checker = checkers.lsb().expect("one checker");
            checkers | between(king_sq, checker)
        } else {
            Bitboard::FULL
        };

        let pins = self.compute_pins(king_sq, us, them, occupied);

        // Every non-king, non-pawn role: its attack set minus friendly pieces,
        // confined by the check mask and (if pinned) its pin line.
        for role in WideRole::ALL {
            if role == WideRole::King || role == WideRole::Pawn {
                continue;
            }
            for from in board.pieces(us, role) {
                let pin_line = pins.line_of(from);
                let targets =
                    V::role_attacks(role, us, from, occupied) & !our_pieces & check_mask & pin_line;
                emit_targets(out, from, targets, their_pieces);
            }
        }

        // Pawns: pushes, double pushes, captures, en passant, promotions.
        self.gen_pawn_moves(out, us, occupied, their_pieces, check_mask, &pins, king_sq);

        // Castling, only when not in check.
        if V::has_castling() && num_checkers == 0 {
            self.gen_castles(out, us, occupied, king_danger, king_sq);
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
    fn generate_duck_into(&self, out: &mut Vec<WideMove>) {
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
                let rank = back_rank::<G>(us);
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
                emit_targets(base, from, targets, their_pieces);
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
    /// the move," which the [`Self::royals_survive`] predicate tests on the
    /// applied position. This unifies both colours: white (one king) reduces to
    /// the standard "king not in check," black (two kings) to "not duple check."
    fn generate_multi_royal_into(&self, out: &mut Vec<WideMove>) {
        let us = self.state.turn;
        // A side with no kings has already lost (its last king was captured on the
        // previous ply): no moves.
        if self.board.kings_of(us).is_empty() {
            return;
        }
        let mut pseudo = Vec::new();
        self.gen_multi_royal_pseudo(&mut pseudo, us);
        for mv in pseudo {
            let mut next = self.clone();
            next.apply(&mv);
            // `apply` flipped the side to move; test our (now non-to-move) kings.
            if next.royals_survive(us) {
                out.push(mv);
            }
        }
    }

    /// Returns `true` if, in this position, color `who` keeps at least one
    /// unattacked king — i.e. `who` is not in (duple) check. A side with no king
    /// at all returns `false` (it has been eliminated).
    fn royals_survive(&self, who: Color) -> bool {
        let kings = self.board.kings_of(who);
        if kings.is_empty() {
            return false;
        }
        let occ = self.board.occupied();
        let them = who.opposite();
        kings
            .into_iter()
            .any(|k| self.attackers_to(k, them, occ).is_empty())
    }

    /// Pushes the pseudo-legal base moves for a multi-king side into `pseudo`
    /// (no self-check filtering — that is done by the caller per move). Kings,
    /// every other piece, the Berolina/standard pawns, and castling are all
    /// emitted with a full board mask and no pins.
    fn gen_multi_royal_pseudo(&self, pseudo: &mut Vec<WideMove>, us: Color) {
        let board = &self.board;
        let occupied = board.occupied();
        let our_pieces = board.by_color(us);
        let their_pieces = board.by_color(us.opposite());
        let full = Bitboard::FULL;

        // Every non-pawn role (including the king): its attack set minus friendly
        // pieces. No check mask, no pin lines — the per-move filter handles safety.
        for role in WideRole::ALL {
            if role == WideRole::Pawn || role == WideRole::Hoplite {
                continue;
            }
            for from in board.pieces(us, role) {
                let targets = V::role_attacks(role, us, from, occupied) & !our_pieces;
                emit_targets(pseudo, from, targets, their_pieces);
                // Quiet-only steps (the Spartan Lieutenant's sideways slide): a
                // move onto an empty square that can never capture. Default-empty,
                // so inert for every other role/variant.
                let quiet_only = V::quiet_only_targets(role, us, from, occupied) & !occupied;
                for to in quiet_only {
                    pseudo.push(WideMove::new(from, to, WideMoveKind::Quiet));
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
        self.gen_pawn_moves(pseudo, us, occupied, their_pieces, full, &pins, king_sq);
        if V::has_berolina_pawns() {
            self.gen_berolina_moves(pseudo, us, occupied, their_pieces, full, &pins);
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
    fn gen_berolina_moves(
        &self,
        out: &mut Vec<WideMove>,
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
    fn emit_pawn_dest(
        out: &mut Vec<WideMove>,
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

    /// Appends any variant drop moves (reserved; standard chess emits none).
    fn append_drops(&self, out: &mut Vec<WideMove>) {
        V::emit_drops(&self.board, &self.state, out);
    }

    /// Generates the side-to-move's **placement-phase** drops (Sittuyin): for
    /// each role still in hand, a [`WideMove::drop`] onto every square the variant
    /// permits ([`WideVariant::placement_targets`]). FSF applies no check filter
    /// during placement, so the drops are emitted directly. Only reached while
    /// [`WideVariant::has_placement`] is `true` and the side's pocket is
    /// non-empty.
    fn generate_placement_into(&self, out: &mut Vec<WideMove>) {
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
    fn append_gating_moves(&self, out: &mut Vec<WideMove>, us: Color) {
        let gating = self.state.gating;
        if !gating.any_reserve(us) {
            return;
        }
        let eligible = gating.eligible();
        if eligible.is_empty() {
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
            let mv = out[i];
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

    /// Generates the side-to-move's pawn moves: single and double pushes,
    /// diagonal captures, en passant, and promotions, under the check mask and
    /// pins.
    #[allow(clippy::too_many_arguments)]
    fn gen_pawn_moves(
        &self,
        out: &mut Vec<WideMove>,
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
    fn gen_castles(
        &self,
        out: &mut Vec<WideMove>,
        us: Color,
        occupied: Bitboard<G>,
        king_danger: Bitboard<G>,
        king_sq: Square<G>,
    ) {
        if !self.state.castling.has_any(us) {
            return;
        }
        let rank = back_rank::<G>(us);
        if king_sq.rank() != rank {
            return;
        }

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

            out.push(WideMove::new(king_sq, king_dest, kind));
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

    fn apply(&mut self, mv: &WideMove) {
        let us = self.state.turn;
        let them = us.opposite();
        let from = mv.from::<G>();
        let to = mv.to::<G>();
        let rank = back_rank::<G>(us);

        // A drop has no origin piece (the square it names is empty before the
        // drop), so it is handled before the `from`-piece lookup the board moves
        // require. It places a held piece, advances the side and fullmove number,
        // and — in the placement phase — consumes the piece from the pocket. The
        // setup phase never resets nor advances the halfmove clock in FSF's
        // counting (it stays 0 through deployment), so leave it untouched.
        if let WideMoveKind::Drop { role } = mv.kind() {
            self.board.set_piece(to, WidePiece::new(us, role));
            if V::has_placement() {
                self.state.placement.take(us, role);
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

        self.state.ep_square = None;

        // The castling rook's origin, captured for the gating update below (a
        // castle vacates both the king's and the rook's squares).
        let mut castle_rook_from: Option<Square<G>> = None;

        match mv.kind() {
            WideMoveKind::Quiet => {
                self.board.remove_piece(from);
                self.board.set_piece(to, moving);
            }
            WideMoveKind::Capture => {
                reset_clock = true;
                self.board.remove_piece(from);
                self.board.set_piece(to, moving);
            }
            WideMoveKind::DoublePawnPush => {
                self.board.remove_piece(from);
                self.board.set_piece(to, moving);
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
                self.board.remove_piece(from);
                self.board.set_piece(to, moving);
                if let Some(captured) = Square::<G>::from_file_rank(to.file(), from.rank()) {
                    self.board.remove_piece(captured);
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
                self.board.remove_piece(from);
                self.board.remove_piece(rook_from);
                self.board.set_piece(to, moving);
                self.board.set_piece(rook_to, rook);
                castle_rook_from = Some(rook_from);
            }
            WideMoveKind::Promotion { role, capture } => {
                reset_clock = capture || is_pawn_move;
                self.board.remove_piece(from);
                self.board.set_piece(to, WidePiece::new(us, role));
            }
            WideMoveKind::Drop { .. } => {
                // Drops are fully handled by the early return above (a drop has no
                // origin piece, so it cannot share the board-move path).
                unreachable!("drops are handled before the board-move match");
            }
        }

        // Castling-right updates.
        if moving.role == WideRole::King {
            self.state.castling.revoke_color(us);
        }
        self.revoke_rights_for_square(from, us);
        if mv.is_capture() && !matches!(mv.kind(), WideMoveKind::EnPassant) {
            self.revoke_rights_for_square(to, them);
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

        if reset_clock {
            self.state.halfmove_clock = 0;
        } else {
            self.state.halfmove_clock = self.state.halfmove_clock.saturating_add(1);
        }
        if us.is_black() {
            self.state.fullmove_number = self.state.fullmove_number.saturating_add(1);
        }
        self.state.turn = them;
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
    }

    /// If `square` is a castling rook's home square for `color`, revokes that
    /// right.
    fn revoke_rights_for_square(&mut self, square: Square<G>, color: Color) {
        if self.state.castling.is_empty() {
            return;
        }
        let rank = back_rank::<G>(color);
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
        if self.legal_moves().is_empty() {
            if self.is_check() {
                Some(WideEndReason::Checkmate)
            } else {
                Some(WideEndReason::Stalemate)
            }
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
            WideEndReason::VariantWin => WideOutcome::Decisive {
                winner: self.state.turn,
            },
            WideEndReason::Stalemate
            | WideEndReason::InsufficientMaterial
            | WideEndReason::VariantDraw => WideOutcome::Draw,
        })
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
        let board = Board::<G>::from_fen_placement(placement).map_err(WideFenError::Placement)?;

        // Sittuyin carries the setup-phase pocket in the same `[..]` holdings
        // bracket the gating variants use (the crazyhouse convention): uppercase
        // letters are white's undeployed pieces, lowercase black's. A non-
        // placement variant never reads the bracket here, so its pocket stays
        // `NONE`.
        let placement_pocket = if V::has_placement() {
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
            parse_castling_and_gating::<G>(castling_field, holdings, &board)?
        } else {
            (
                parse_castling::<G>(castling_field, &board)?,
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
        };
        Ok(Self::from_parts(board, state))
    }

    /// Serializes this position as a six-field FEN string over `G`.
    ///
    /// A gating variant appends the reserves in hand to the placement field as a
    /// `[..]` bracket and folds the gating-eligible squares into the castling
    /// field (`KQBCDFGkqbcdfg`-style), matching the Fairy-Stockfish S-Chess
    /// dialect. A non-gating variant produces the plain six-field FEN unchanged.
    #[must_use]
    pub fn to_fen(&self) -> String {
        let mut out = if V::has_duck() {
            placement_with_duck::<G>(&self.board, self.state.duck)
        } else {
            self.board.to_fen_placement()
        };
        if V::supports_gating() {
            write_holdings(self.state.gating, &mut out);
        }
        if V::has_placement() {
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
        } else {
            write_castling(self.state.castling, &mut out);
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

/// The pinned pieces of the side to move and, per pinned piece, the line it is
/// confined to. Recorded inline (no allocation per node beyond this small vec).
struct Pins<G: Geometry> {
    pinned: Bitboard<G>,
    lines: Vec<(Square<G>, Bitboard<G>)>,
    king_sq: Square<G>,
}

impl<G: Geometry> Pins<G> {
    fn empty(king_sq: Square<G>) -> Pins<G> {
        Pins {
            pinned: Bitboard::EMPTY,
            lines: Vec::new(),
            king_sq,
        }
    }

    fn add(&mut self, square: Square<G>, l: Bitboard<G>) {
        self.pinned.set(square);
        self.lines.push((square, l));
    }

    /// The line a piece is confined to: its pin line if pinned, else the full
    /// board (unconfined).
    fn line_of(&self, square: Square<G>) -> Bitboard<G> {
        if !self.pinned.contains(square) {
            return Bitboard::FULL;
        }
        for &(sq, l) in &self.lines {
            if sq == square {
                return l;
            }
        }
        // Should be unreachable: `pinned` and `lines` stay in sync.
        let _ = self.king_sq;
        Bitboard::FULL
    }
}

/// Emits one move per target square from `from`, tagging captures by whether the
/// target holds an enemy piece.
fn emit_targets<G: Geometry>(
    out: &mut Vec<WideMove>,
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
        out.push(WideMove::new(from, to, kind));
    }
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

// -- Free perft functions ---------------------------------------------------

/// Counts the leaf nodes of the legal-move game tree below `position` at the
/// given `depth` — the generic analogue of [`crate::perft`].
///
/// `perft(pos, 0) == 1`. Correctness-first: it materializes the move list at
/// every interior node and recurses; the concrete engine's bulk-count and
/// stack-buffer optimizations are deferred to a later perf pass.
#[must_use]
pub fn perft<G: Geometry, V: WideVariant<G>>(position: &GenericPosition<G, V>, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    let moves = position.legal_moves();
    if depth == 1 {
        return moves.len() as u64;
    }
    let mut nodes = 0;
    for mv in moves {
        nodes += perft(&position.play(&mv), depth - 1);
    }
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
    for mv in position.legal_moves() {
        let count = if depth == 1 {
            1
        } else {
            perft(&position.play(&mv), depth - 1)
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

/// Parses the castling field (`-` or a subset of `KQkq`-style letters) into
/// [`GenericCastling`], reading each named rook's start file from the board.
///
/// Standard chess uses `K`/`k` for the kingside (rightmost) rook and `Q`/`q` for
/// the queenside (leftmost) rook of white / black. The rook's start file is the
/// file of the matching rook on that side's back rank.
fn parse_castling<G: Geometry>(
    field: &str,
    board: &Board<G>,
) -> Result<GenericCastling, WideFenError> {
    let mut rights = GenericCastling::NONE;
    if field == "-" {
        return Ok(rights);
    }
    for ch in field.chars() {
        let (color, side) = match ch {
            'K' => (Color::White, KINGSIDE),
            'Q' => (Color::White, QUEENSIDE),
            'k' => (Color::Black, KINGSIDE),
            'q' => (Color::Black, QUEENSIDE),
            _ => return Err(WideFenError::BadCastling),
        };
        let rank = match color {
            Color::White => 0,
            Color::Black => G::HEIGHT - 1,
        };
        // Find the rook on the named side: the outermost friendly rook on the
        // back rank toward the kingside (last file) or queenside (file 0).
        let rooks = board.pieces(color, WideRole::Rook);
        let mut chosen: Option<u8> = None;
        for file in 0..G::WIDTH {
            if let Some(sq) = Square::<G>::from_file_rank(file, rank) {
                if rooks.contains(sq) {
                    match side {
                        // Kingside: take the rightmost rook (keep updating).
                        KINGSIDE => chosen = Some(file),
                        // Queenside: take the leftmost rook (first one found).
                        _ => {
                            if chosen.is_none() {
                                chosen = Some(file);
                            }
                        }
                    }
                }
            }
        }
        let file = chosen.ok_or(WideFenError::BadCastling)?;
        rights.set(color, side, Some(file));
    }
    Ok(rights)
}

/// Writes the castling field in `KQkq` order, or `-` if no rights remain.
fn write_castling(castling: GenericCastling, out: &mut String) {
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
    for ch in holdings.chars() {
        let role = WideRole::from_char(ch).ok_or(WideFenError::BadCastling)?;
        let counts = if ch.is_ascii_uppercase() {
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
    write_castling(castling, out);
    // `write_castling` writes `-` for no rights; strip it so we can append gating
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
}
