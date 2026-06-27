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
use super::{Bitboard, Board, Geometry, Square, WideMove, WideMoveKind, WidePiece};
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
            let from_sq = if role == WideRole::Pawn {
                V::role_attacks(WideRole::Pawn, attacker.opposite(), sq, occupied)
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

    /// Returns `true` if the side to move is in check (any of its royal squares
    /// is attacked by the opponent).
    #[must_use]
    pub fn is_check(&self) -> bool {
        let us = self.state.turn;
        let them = us.opposite();
        let occ = self.board.occupied();
        let royals = V::royal_squares(&self.board, us);
        royals
            .into_iter()
            .any(|sq| !self.attackers_to(sq, them, occ).is_empty())
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
    }

    /// Appends any variant drop moves (reserved; standard chess emits none).
    fn append_drops(&self, out: &mut Vec<WideMove>) {
        V::emit_drops(&self.board, &self.state, out);
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
        let promo_rank = V::promotion_rank(us);
        let start_rank = V::double_push_rank(us);
        let promo_roles = V::promotion_config().roles;

        for from in pawns {
            let pin_line = pins.line_of(from);

            // Single (and double) push.
            if let Some(one) = from.offset(0, forward) {
                if !occupied.contains(one) {
                    if check_mask.contains(one) && pin_line.contains(one) {
                        if one.rank() == promo_rank {
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
                if to.rank() == promo_rank {
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
                } else {
                    out.push(WideMove::new(from, to, WideMoveKind::Capture));
                }
            }
        }

        // En passant.
        if let Some(ep) = self.state.ep_square {
            // A pawn that attacks the ep square may take. The captured pawn sits
            // on the ep file, on the capturing pawn's rank.
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

        let moving = self
            .board
            .piece_at(from)
            .expect("move originates from an occupied square");
        let is_pawn_move = moving.role == WideRole::Pawn;
        let mut reset_clock = is_pawn_move;

        self.state.ep_square = None;

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
            }
            WideMoveKind::Promotion { role, capture } => {
                reset_clock = capture || is_pawn_move;
                self.board.remove_piece(from);
                self.board.set_piece(to, WidePiece::new(us, role));
            }
            WideMoveKind::Drop { role } => {
                // Reserved variant path; standard chess never emits drops.
                self.board.set_piece(to, WidePiece::new(us, role));
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

        let placement = fields.next().ok_or(WideFenError::MissingField)?;
        let board = Board::<G>::from_fen_placement(placement).map_err(WideFenError::Placement)?;

        let turn = match fields.next().ok_or(WideFenError::MissingField)? {
            "w" => Color::White,
            "b" => Color::Black,
            _ => return Err(WideFenError::BadTurn),
        };

        let castling_field = fields.next().ok_or(WideFenError::MissingField)?;
        let castling = parse_castling::<G>(castling_field, &board)?;

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
            halfmove_clock,
            fullmove_number,
        };
        Ok(Self::from_parts(board, state))
    }

    /// Serializes this position as a six-field FEN string over `G`.
    #[must_use]
    pub fn to_fen(&self) -> String {
        let mut out = self.board.to_fen_placement();
        out.push(' ');
        out.push(if self.state.turn.is_white() { 'w' } else { 'b' });
        out.push(' ');
        write_castling(self.state.castling, &mut out);
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
