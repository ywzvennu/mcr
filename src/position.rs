//! A full standard-chess position: piece placement plus all the state needed to
//! generate legal moves and to apply them.
//!
//! A [`Position`] bundles a [`Board`] with the side to move, castling rights,
//! the en-passant target square, and the two move clocks. It can be parsed from
//! and serialized to [Forsyth–Edwards Notation][fen] (all six fields), it
//! generates the *legal* moves of the side to move, and it applies a move to
//! produce the successor position.
//!
//! # Move generation
//!
//! Generation is pseudo-legal first, then filtered for king safety: a candidate
//! move is legal exactly when it does not leave the moving side's king in check.
//! For most moves this is checked with a lightweight pin/check analysis rather
//! than a full make-move, but the rare cases that change two squares on a shared
//! line — en-passant captures and castling — are validated directly.
//!
//! # Castling rights
//!
//! Rights are stored as the file of each side's castling rook (king-side and
//! queen-side). For standard chess these are always the a- and h-files, but
//! storing the file rather than a bare boolean leaves room for a Chess960
//! position type to reuse this representation later.
//!
//! [fen]: https://en.wikipedia.org/wiki/Forsyth%E2%80%93Edwards_Notation

use core::fmt;
use core::str::FromStr;

use crate::attacks::{
    between, bishop_attacks, king_attacks, knight_attacks, pawn_attacks, rook_attacks,
};
use crate::movelist::MoveList;
use crate::{Bitboard, Board, Color, File, Move, MoveKind, Piece, Rank, Role, Square};

/// Castling rights: which rooks each side may still castle with.
///
/// Each entry is the [`File`] of the rook involved, or `None` if that castling
/// is no longer available. In standard chess the king-side rook is on the
/// h-file and the queen-side rook on the a-file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CastlingRights {
    /// White king-side rook file, if white may still castle king-side.
    white_king: Option<File>,
    /// White queen-side rook file, if white may still castle queen-side.
    white_queen: Option<File>,
    /// Black king-side rook file, if black may still castle king-side.
    black_king: Option<File>,
    /// Black queen-side rook file, if black may still castle queen-side.
    black_queen: Option<File>,
}

/// The two sides a castling move can be toward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CastleSide {
    /// Toward the h-file (short castling).
    King,
    /// Toward the a-file (long castling).
    Queen,
}

impl CastlingRights {
    /// No castling rights for either side.
    pub const NONE: CastlingRights = CastlingRights {
        white_king: None,
        white_queen: None,
        black_king: None,
        black_queen: None,
    };

    /// The standard starting rights: both sides may castle both ways, with rooks
    /// on the a- and h-files.
    pub const STANDARD: CastlingRights = CastlingRights {
        white_king: Some(File::H),
        white_queen: Some(File::A),
        black_king: Some(File::H),
        black_queen: Some(File::A),
    };

    /// Builds castling rights directly from each side's rook file (or `None`),
    /// for variants whose castling rooks are not on the a-/h-files (Chess960).
    #[must_use]
    #[inline]
    pub(crate) const fn from_rook_files(
        white_king: Option<File>,
        white_queen: Option<File>,
        black_king: Option<File>,
        black_queen: Option<File>,
    ) -> CastlingRights {
        CastlingRights {
            white_king,
            white_queen,
            black_king,
            black_queen,
        }
    }

    /// Returns the rook file for `color` castling toward `side`, if that right is
    /// still held.
    #[must_use]
    #[inline]
    pub const fn rook_file(self, color: Color, side: CastleSide) -> Option<File> {
        match (color, side) {
            (Color::White, CastleSide::King) => self.white_king,
            (Color::White, CastleSide::Queen) => self.white_queen,
            (Color::Black, CastleSide::King) => self.black_king,
            (Color::Black, CastleSide::Queen) => self.black_queen,
        }
    }

    /// Returns `true` if `color` may still castle toward `side`.
    #[must_use]
    #[inline]
    pub const fn has(self, color: Color, side: CastleSide) -> bool {
        self.rook_file(color, side).is_some()
    }

    /// Sets or clears the rook file for `color`/`side`.
    #[inline]
    fn set(&mut self, color: Color, side: CastleSide, file: Option<File>) {
        match (color, side) {
            (Color::White, CastleSide::King) => self.white_king = file,
            (Color::White, CastleSide::Queen) => self.white_queen = file,
            (Color::Black, CastleSide::King) => self.black_king = file,
            (Color::Black, CastleSide::Queen) => self.black_queen = file,
        }
    }

    /// Removes both of `color`'s castling rights (used when its king moves).
    #[inline]
    fn revoke_color(&mut self, color: Color) {
        self.set(color, CastleSide::King, None);
        self.set(color, CastleSide::Queen, None);
    }

    /// Returns `true` if no side holds any castling right.
    #[must_use]
    #[inline]
    fn is_empty(self) -> bool {
        self == CastlingRights::NONE
    }
}

/// The home rank of `color` (rank 1 for white, rank 8 for black).
#[inline]
const fn back_rank(color: Color) -> Rank {
    match color {
        Color::White => Rank::First,
        Color::Black => Rank::Eighth,
    }
}

/// A full standard-chess game position.
///
/// ```
/// use mce::Position;
/// let pos = Position::startpos();
/// assert_eq!(pos.legal_moves().len(), 20);
/// assert_eq!(pos.to_fen(), "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Position {
    board: Board,
    turn: Color,
    castling: CastlingRights,
    /// The square a pawn could move *to* in an en-passant capture, set the move
    /// after a double pawn push.
    ep_square: Option<Square>,
    halfmove_clock: u32,
    fullmove_number: u32,
    /// The incrementally maintained Zobrist key (see [`crate::zobrist`]).
    hash: u64,
}

impl Default for Position {
    #[inline]
    fn default() -> Position {
        Position::startpos()
    }
}

impl Position {
    /// The standard chess starting position.
    #[must_use]
    pub fn startpos() -> Position {
        let mut pos = Position {
            board: Board::standard(),
            turn: Color::White,
            castling: CastlingRights::STANDARD,
            ep_square: None,
            halfmove_clock: 0,
            fullmove_number: 1,
            hash: 0,
        };
        pos.hash = pos.compute_zobrist();
        pos
    }

    /// The piece placement of this position.
    #[must_use]
    #[inline]
    pub const fn board(&self) -> &Board {
        &self.board
    }

    /// The side to move.
    #[must_use]
    #[inline]
    pub const fn turn(&self) -> Color {
        self.turn
    }

    /// Alias for [`Position::turn`].
    #[must_use]
    #[inline]
    pub const fn side_to_move(&self) -> Color {
        self.turn
    }

    /// The current castling rights.
    #[must_use]
    #[inline]
    pub const fn castling_rights(&self) -> CastlingRights {
        self.castling
    }

    /// The en-passant target square, if a pawn double-pushed last move.
    #[must_use]
    #[inline]
    pub const fn ep_square(&self) -> Option<Square> {
        self.ep_square
    }

    /// The incrementally maintained Zobrist key, as kept up to date by
    /// [`Position::play`]. Always equal to [`Position::compute_zobrist`]; exposed
    /// for the equality test between the two paths.
    #[cfg(test)]
    #[must_use]
    #[inline]
    pub(crate) fn incremental_zobrist(&self) -> u64 {
        self.hash
    }

    /// The halfmove clock (plies since the last capture or pawn move), used for
    /// the fifty-move rule.
    #[must_use]
    #[inline]
    pub const fn halfmove_clock(&self) -> u32 {
        self.halfmove_clock
    }

    /// The fullmove number, starting at 1 and incremented after each black move.
    #[must_use]
    #[inline]
    pub const fn fullmove_number(&self) -> u32 {
        self.fullmove_number
    }

    // -- Attack queries ----------------------------------------------------

    /// Returns the set of `attacker` pieces that attack `sq`, given the current
    /// occupancy.
    ///
    /// A piece "attacks" a square if it could capture an enemy piece there;
    /// pawns attack only diagonally. The king on `sq` itself does not block
    /// these rays (callers checking king safety should pass an occupancy with
    /// the king removed when relevant).
    #[must_use]
    pub fn attackers_to(&self, sq: Square, attacker: Color, occupied: Bitboard) -> Bitboard {
        let b = &self.board;
        let mut result = Bitboard::EMPTY;

        // Pawns: a pawn of `attacker` attacks `sq` iff `sq` is attacked-from one
        // of its squares, i.e. the *opposing*-color pawn-attack pattern from
        // `sq` lands on an `attacker` pawn.
        result |= pawn_attacks(attacker.opposite(), sq) & b.pieces(attacker, Role::Pawn);
        result |= knight_attacks(sq) & b.pieces(attacker, Role::Knight);
        result |= king_attacks(sq) & b.pieces(attacker, Role::King);

        let bishops = b.pieces(attacker, Role::Bishop) | b.pieces(attacker, Role::Queen);
        result |= bishop_attacks(sq, occupied) & bishops;
        let rooks = b.pieces(attacker, Role::Rook) | b.pieces(attacker, Role::Queen);
        result |= rook_attacks(sq, occupied) & rooks;

        result
    }

    /// Returns `true` if `sq` is attacked by any piece of color `by`.
    #[must_use]
    #[inline]
    pub fn is_attacked(&self, sq: Square, by: Color) -> bool {
        !self.attackers_to(sq, by, self.board.occupied()).is_empty()
    }

    /// Returns the pieces of the side *not* to move that currently give check
    /// (attack the side-to-move's king).
    #[must_use]
    pub fn checkers(&self) -> Bitboard {
        match self.board.king_of(self.turn) {
            Some(king) => self.attackers_to(king, self.turn.opposite(), self.board.occupied()),
            None => Bitboard::EMPTY,
        }
    }

    /// Returns `true` if the side to move is in check.
    #[must_use]
    #[inline]
    pub fn is_check(&self) -> bool {
        !self.checkers().is_empty()
    }

    // -- Move generation ---------------------------------------------------

    /// Generates every legal move for the side to move.
    #[must_use]
    pub fn legal_moves(&self) -> Vec<Move> {
        let mut moves = MoveList::new();
        self.generate_into(&mut moves);
        moves.into_vec()
    }

    /// Returns `true` if `mv` is among this position's legal moves.
    #[must_use]
    pub fn is_legal(&self, mv: &Move) -> bool {
        self.legal_moves().contains(mv)
    }

    /// Number of legal moves without allocating the full list — used by
    /// checkmate / stalemate queries and perft at depth 1.
    #[must_use]
    pub fn legal_move_count(&self) -> usize {
        let mut count = 0usize;
        self.for_each_legal(|_| count += 1);
        count
    }

    /// Whether the side to move has been checkmated (in check with no legal
    /// move).
    #[must_use]
    pub fn is_checkmate(&self) -> bool {
        self.is_check() && self.legal_move_count() == 0
    }

    /// Whether the position is a stalemate (not in check, but no legal move).
    #[must_use]
    pub fn is_stalemate(&self) -> bool {
        !self.is_check() && self.legal_move_count() == 0
    }

    /// Whether neither side has the material to deliver checkmate by any
    /// sequence of legal moves (FIDE "insufficient material").
    ///
    /// The exact rule set treated as a draw is:
    ///
    /// - **King vs king.**
    /// - **King and a single minor** (one bishop or one knight, either side) **vs
    ///   king.**
    /// - **Bishops only, all on one color complex:** any number of bishops on
    ///   either side, provided every bishop on the board stands on the same color
    ///   square (so none can ever guard a square of the other color, and mate is
    ///   impossible).
    ///
    /// Everything else is treated as *sufficient* — notably any pawn, rook, or
    /// queen, bishops on both colors, and any position containing a knight
    /// alongside another minor. In particular **K+N+N vs K is reported as
    /// sufficient**: although it cannot be *forced*, it is not insufficient
    /// material under FIDE (a helpmate exists), so it is not an automatic draw.
    #[must_use]
    pub fn is_insufficient_material(&self) -> bool {
        let b = &self.board;
        // Any pawn, rook, or queen can force mate.
        if !(b.by_role(Role::Pawn) | b.by_role(Role::Rook) | b.by_role(Role::Queen)).is_empty() {
            return false;
        }
        let knights = b.by_role(Role::Knight);
        let bishops = b.by_role(Role::Bishop);
        let minors = knights | bishops;
        match minors.count() {
            0 | 1 => true,
            _ => {
                // Knights present alongside other minors can mate (or it is K+2N
                // which we conservatively treat as sufficient); only the pure
                // same-colored-bishops case is a guaranteed draw.
                if !knights.is_empty() {
                    return false;
                }
                let light = Bitboard(LIGHT_SQUARES);
                bishops & light == bishops || (bishops & light).is_empty()
            }
        }
    }

    /// Pushes the *pseudo-legal* moves of the side to move into `out`, without
    /// the king-safety filter.
    ///
    /// A pseudo-legal move follows the moving piece's geometry and the
    /// capture/occupancy rules, but may leave the moving side's king in check.
    /// Standard chess uses the fast pin/check-aware [`Position::legal_moves`];
    /// this slower, fully general generator exists so variant rule layers that
    /// need a different king-safety rule (or none at all) can start from the raw
    /// candidate set and filter it themselves. It does not depend on a king being
    /// present.
    ///
    /// Castling moves are emitted only when the side to move is not in check and
    /// the king does not pass through an attacked square (these are intrinsic to
    /// the castling rule, not king-safety of the destination); the caller's
    /// king-safety filter still validates the resulting position.
    pub(crate) fn pseudo_into(&self, out: &mut MoveList) {
        self.pseudo_into_with_castles(out, true);
    }

    /// Pushes the pseudo-legal moves of the side to move into `out` *excluding*
    /// castling, for variant layers that generate castling themselves (Chess960
    /// supplies its own arbitrary-geometry castle generator).
    pub(crate) fn pseudo_no_castles_into(&self, out: &mut MoveList) {
        self.pseudo_into_with_castles(out, false);
    }

    /// Shared body of [`Position::pseudo_into`]; `standard_castles` controls
    /// whether the standard castle generator runs.
    fn pseudo_into_with_castles(&self, out: &mut MoveList, standard_castles: bool) {
        self.pseudo_into_with(out, standard_castles, false);
    }

    /// Pushes the pseudo-legal moves of the side to move into `out`, exactly like
    /// [`Position::pseudo_into`], but additionally treating white's first-rank
    /// pawns as double-push eligible for the horde variant. Standard chess and
    /// every other variant call the wrappers above with the flag `false`, leaving
    /// their move sets identical.
    pub(crate) fn pseudo_into_horde(&self, out: &mut MoveList) {
        self.pseudo_into_with(out, true, true);
    }

    /// Shared body of the pseudo-legal generators. `standard_castles` controls
    /// whether the standard castle generator runs; `white_first_rank_double`
    /// admits horde's first-rank white double-pushes.
    fn pseudo_into_with(
        &self,
        out: &mut MoveList,
        standard_castles: bool,
        white_first_rank_double: bool,
    ) {
        let us = self.turn;
        let them = us.opposite();
        let board = &self.board;
        let occupied = board.occupied();
        let our_pieces = board.by_color(us);
        let their_pieces = board.by_color(them);
        let full = Bitboard::FULL;
        let no_pins: [(Square, Bitboard); 0] = [];

        // Pawns, knights, sliders: reuse the standard generators with an
        // all-allowing check mask and no pins, so every geometric move is kept.
        if let Some(king_sq) = board.king_of(us) {
            self.gen_pawn_moves(
                out,
                us,
                occupied,
                their_pieces,
                full,
                &no_pins,
                king_sq,
                white_first_rank_double,
                false,
            );
        } else {
            // Without a king there is no en-passant legality king to consult; the
            // generator only reads `king_sq` for that rare check, and a kingless
            // side cannot be in an ep pin, so any square is a safe placeholder.
            self.gen_pawn_moves(
                out,
                us,
                occupied,
                their_pieces,
                full,
                &no_pins,
                Square::A1,
                white_first_rank_double,
                false,
            );
        }
        self.gen_knight_moves(out, us, our_pieces, their_pieces, full, &no_pins);
        self.gen_slider_moves(out, us, occupied, our_pieces, their_pieces, full, &no_pins);

        // King steps to any non-friendly square (king-safety is left to the
        // caller's filter).
        if let Some(king_sq) = board.king_of(us) {
            let king_targets = king_attacks(king_sq) & !our_pieces;
            for to in king_targets {
                let kind = if their_pieces.contains(to) {
                    MoveKind::Capture
                } else {
                    MoveKind::Quiet
                };
                out.push(Move::new(king_sq, to, kind));
            }

            // Castling, gated only by the not-in-check / clear-path / safe-walk
            // conditions intrinsic to the castling rule (the standard generator
            // enforces the same).
            if standard_castles && self.attackers_to(king_sq, them, occupied).is_empty() {
                let occ_without_king = occupied.without(king_sq);
                let king_danger = self.attacked_by(them, occ_without_king);
                self.gen_castles(out, us, occupied, king_danger, king_sq);
            }
        }
    }

    /// Returns `true` if `mv`, applied to this position via [`Position::play`],
    /// leaves the moving side's king un-attacked — the standard king-safety
    /// predicate, expressed as a make-move filter for the pseudo-legal path.
    ///
    /// For positions with no king of the moving side this is vacuously `true`.
    pub(crate) fn move_keeps_king_safe(&self, mv: &Move) -> bool {
        let us = self.turn;
        let child = self.play(mv);
        match child.board.king_of(us) {
            Some(king) => !child.is_attacked(king, us.opposite()),
            None => true,
        }
    }

    /// Invokes `f` once for each legal move, in generation order, without
    /// collecting them.
    fn for_each_legal(&self, mut f: impl FnMut(Move)) {
        let mut buf = MoveList::new();
        self.generate_into(&mut buf);
        for &mv in buf.iter() {
            f(mv);
        }
    }

    /// Pushes all legal moves into `out`.
    pub(crate) fn generate_into(&self, out: &mut MoveList) {
        let us = self.turn;
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

        // Squares the king may not step onto: those attacked by the enemy with
        // the king removed from the occupancy (so it cannot "shield itself").
        let occ_without_king = occupied.without(king_sq);
        let king_danger = self.attacked_by(them, occ_without_king);

        // King moves are always generated (the only legal moves under double
        // check).
        let king_targets = king_attacks(king_sq) & !our_pieces & !king_danger;
        for to in king_targets {
            let kind = if their_pieces.contains(to) {
                MoveKind::Capture
            } else {
                MoveKind::Quiet
            };
            out.push(Move::new(king_sq, to, kind));
        }

        if num_checkers >= 2 {
            // Double check: only king moves are legal.
            return;
        }

        // The mask of destination squares that resolve a single check: capture
        // the checker or block the ray between it and the king. With no check,
        // every square is allowed.
        let check_mask = if num_checkers == 1 {
            let checker = checkers.lsb().expect("one checker");
            checkers | between(king_sq, checker)
        } else {
            Bitboard::FULL
        };

        // Pinned pieces: friendly pieces on a line between the king and an enemy
        // slider, with no other piece between. They may move only along that
        // line. We compute, per pinned piece, the line it is restricted to.
        let pin_lines = self.pin_lines(king_sq, us, them, occupied);

        self.gen_pawn_moves(
            out,
            us,
            occupied,
            their_pieces,
            check_mask,
            &pin_lines,
            king_sq,
            false,
            true,
        );
        self.gen_knight_moves(out, us, our_pieces, their_pieces, check_mask, &pin_lines);
        self.gen_slider_moves(
            out,
            us,
            occupied,
            our_pieces,
            their_pieces,
            check_mask,
            &pin_lines,
        );

        // Castling is only possible when not in check.
        if num_checkers == 0 {
            self.gen_castles(out, us, occupied, king_danger, king_sq);
        }
    }

    /// Returns the set of squares attacked by color `by` under `occupied`,
    /// using pawn-attack patterns for pawns (the squares a king of the other
    /// color may not move to).
    fn attacked_by(&self, by: Color, occupied: Bitboard) -> Bitboard {
        let b = &self.board;
        let mut attacked = Bitboard::EMPTY;

        for from in b.pieces(by, Role::Pawn) {
            attacked |= pawn_attacks(by, from);
        }
        for from in b.pieces(by, Role::Knight) {
            attacked |= knight_attacks(from);
        }
        for from in b.pieces(by, Role::Bishop) {
            attacked |= bishop_attacks(from, occupied);
        }
        for from in b.pieces(by, Role::Rook) {
            attacked |= rook_attacks(from, occupied);
        }
        for from in b.pieces(by, Role::Queen) {
            attacked |= bishop_attacks(from, occupied) | rook_attacks(from, occupied);
        }
        if let Some(king) = b.king_of(by) {
            attacked |= king_attacks(king);
        }
        attacked
    }

    /// For each pinned friendly piece, the full line (through the king and the
    /// pinning slider) it is confined to. Returned as `(square, line)` pairs.
    fn pin_lines(
        &self,
        king_sq: Square,
        us: Color,
        them: Color,
        occupied: Bitboard,
    ) -> Vec<(Square, Bitboard)> {
        let b = &self.board;
        let mut pins = Vec::new();
        let our_pieces = b.by_color(us);

        // Enemy sliders that could pin along a rank/file (rooks, queens) or a
        // diagonal (bishops, queens). Look from the king outward.
        let rook_like = b.pieces(them, Role::Rook) | b.pieces(them, Role::Queen);
        let bishop_like = b.pieces(them, Role::Bishop) | b.pieces(them, Role::Queen);

        // Candidate pinners are sliders that attack the king's square as if the
        // board were empty (so the only thing between them is potential pins).
        let rook_pinners = rook_attacks(king_sq, Bitboard::EMPTY) & rook_like;
        let bishop_pinners = bishop_attacks(king_sq, Bitboard::EMPTY) & bishop_like;

        for slider in rook_pinners | bishop_pinners {
            let blockers = between(king_sq, slider) & occupied;
            // Exactly one friendly piece between the king and the slider => pin.
            if blockers.count() == 1 {
                let pinned = blockers.lsb().expect("one blocker");
                if our_pieces.contains(pinned) {
                    let l = crate::attacks::line(king_sq, slider);
                    pins.push((pinned, l));
                }
            }
        }
        pins
    }

    /// Returns the pin line restricting `sq`, or `Bitboard::FULL` if unpinned.
    fn pin_line_of(pins: &[(Square, Bitboard)], sq: Square) -> Bitboard {
        for &(p, line) in pins {
            if p == sq {
                return line;
            }
        }
        Bitboard::FULL
    }

    #[allow(clippy::too_many_arguments)]
    fn gen_pawn_moves(
        &self,
        out: &mut MoveList,
        us: Color,
        occupied: Bitboard,
        their_pieces: Bitboard,
        check_mask: Bitboard,
        pins: &[(Square, Bitboard)],
        king_sq: Square,
        white_first_rank_double: bool,
        filter_ep_pin: bool,
    ) {
        let board = &self.board;
        let pawns = board.pieces(us, Role::Pawn);
        let promo_rank = match us {
            Color::White => Rank::Eighth,
            Color::Black => Rank::First,
        };
        let start_rank = match us {
            Color::White => Rank::Second,
            Color::Black => Rank::Seventh,
        };
        let forward: i8 = if us.is_white() { 1 } else { -1 };

        for from in pawns {
            let pin_line = Self::pin_line_of(pins, from);

            // Single and double pushes.
            if let Some(one) = from.offset(0, forward) {
                if !occupied.contains(one) {
                    self.push_pawn_advance(out, from, one, promo_rank, check_mask, pin_line);
                    // A standard double push from the start rank creates an
                    // en-passant target (`MoveKind::DoublePawnPush`). In horde,
                    // white's first-rank pawns may *also* advance two squares, but
                    // per the horde convention such a first-rank double push does
                    // *not* create an en-passant target — so it is emitted as a
                    // plain quiet two-square move. The `white_first_rank_double`
                    // flag (false for standard chess and every other caller) gates
                    // that extra, ep-less source rank.
                    if from.rank() == start_rank {
                        if let Some(two) = from.offset(0, 2 * forward) {
                            if !occupied.contains(two)
                                && check_mask.contains(two)
                                && pin_line.contains(two)
                            {
                                out.push(Move::new(from, two, MoveKind::DoublePawnPush));
                            }
                        }
                    } else if white_first_rank_double
                        && us == Color::White
                        && from.rank() == Rank::First
                    {
                        if let Some(two) = from.offset(0, 2 * forward) {
                            if !occupied.contains(two)
                                && check_mask.contains(two)
                                && pin_line.contains(two)
                            {
                                // No en-passant target for a first-rank double
                                // push: a quiet two-square advance.
                                out.push(Move::new(from, two, MoveKind::Quiet));
                            }
                        }
                    }
                }
            }

            // Captures (including capturing promotions).
            let caps = pawn_attacks(us, from) & their_pieces;
            for to in caps {
                if !check_mask.contains(to) || !pin_line.contains(to) {
                    continue;
                }
                if to.rank() == promo_rank {
                    for role in PROMOTION_ROLES {
                        out.push(Move::new(
                            from,
                            to,
                            MoveKind::Promotion {
                                role,
                                capture: true,
                            },
                        ));
                    }
                } else {
                    out.push(Move::new(from, to, MoveKind::Capture));
                }
            }

            // En passant.
            if let Some(ep) = self.ep_square {
                if pawn_attacks(us, from).contains(ep) {
                    // The captured pawn sits on the ep square's file, on `from`'s
                    // rank.
                    let captured = Square::from_file_rank(ep.file(), from.rank());
                    // En passant resolves check only if it captures the checking
                    // pawn or blocks on the ep target.
                    let resolves_check = check_mask.contains(ep) || check_mask.contains(captured);
                    // The standard discovered-check ep-pin filter is a king-safety
                    // concern; a variant on the make-move filter path (e.g. atomic,
                    // whose explosion may remove the would-be pinning slider) passes
                    // `filter_ep_pin = false` and re-validates the move itself.
                    let ep_pin_ok =
                        !filter_ep_pin || self.ep_is_legal(us, from, ep, captured, king_sq);
                    if resolves_check && pin_line.contains(ep) && ep_pin_ok {
                        out.push(Move::new(from, ep, MoveKind::EnPassant));
                    }
                }
            }
        }
    }

    /// Pushes a pawn single-advance, expanding promotions, subject to the check
    /// mask and pin line.
    fn push_pawn_advance(
        &self,
        out: &mut MoveList,
        from: Square,
        to: Square,
        promo_rank: Rank,
        check_mask: Bitboard,
        pin_line: Bitboard,
    ) {
        if !check_mask.contains(to) || !pin_line.contains(to) {
            return;
        }
        if to.rank() == promo_rank {
            for role in PROMOTION_ROLES {
                out.push(Move::new(
                    from,
                    to,
                    MoveKind::Promotion {
                        role,
                        capture: false,
                    },
                ));
            }
        } else {
            out.push(Move::new(from, to, MoveKind::Quiet));
        }
    }

    /// Verifies that an en-passant capture does not leave our king in check,
    /// covering the rare case where removing both the capturing and captured
    /// pawns exposes a horizontal slider check.
    fn ep_is_legal(
        &self,
        us: Color,
        from: Square,
        ep: Square,
        captured: Square,
        king_sq: Square,
    ) -> bool {
        let them = us.opposite();
        // Simulate the occupancy after the en-passant capture.
        let occ = self
            .board
            .occupied()
            .without(from)
            .without(captured)
            .with(ep);
        // Our king must not be attacked by enemy sliders through the now-empty
        // squares.
        let rook_like = self.board.pieces(them, Role::Rook) | self.board.pieces(them, Role::Queen);
        if !(rook_attacks(king_sq, occ) & rook_like).is_empty() {
            return false;
        }
        let bishop_like =
            self.board.pieces(them, Role::Bishop) | self.board.pieces(them, Role::Queen);
        (bishop_attacks(king_sq, occ) & bishop_like).is_empty()
    }

    fn gen_knight_moves(
        &self,
        out: &mut MoveList,
        us: Color,
        our_pieces: Bitboard,
        their_pieces: Bitboard,
        check_mask: Bitboard,
        pins: &[(Square, Bitboard)],
    ) {
        for from in self.board.pieces(us, Role::Knight) {
            // A pinned knight can never move (its line is a straight ray and a
            // knight cannot stay on it).
            let pin_line = Self::pin_line_of(pins, from);
            let targets = knight_attacks(from) & !our_pieces & check_mask & pin_line;
            for to in targets {
                let kind = if their_pieces.contains(to) {
                    MoveKind::Capture
                } else {
                    MoveKind::Quiet
                };
                out.push(Move::new(from, to, kind));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn gen_slider_moves(
        &self,
        out: &mut MoveList,
        us: Color,
        occupied: Bitboard,
        our_pieces: Bitboard,
        their_pieces: Bitboard,
        check_mask: Bitboard,
        pins: &[(Square, Bitboard)],
    ) {
        let board = &self.board;
        for (role, diagonal, straight) in [
            (Role::Bishop, true, false),
            (Role::Rook, false, true),
            (Role::Queen, true, true),
        ] {
            for from in board.pieces(us, role) {
                let pin_line = Self::pin_line_of(pins, from);
                let mut attacks = Bitboard::EMPTY;
                if diagonal {
                    attacks |= bishop_attacks(from, occupied);
                }
                if straight {
                    attacks |= rook_attacks(from, occupied);
                }
                let targets = attacks & !our_pieces & check_mask & pin_line;
                for to in targets {
                    let kind = if their_pieces.contains(to) {
                        MoveKind::Capture
                    } else {
                        MoveKind::Quiet
                    };
                    out.push(Move::new(from, to, kind));
                }
            }
        }
    }

    fn gen_castles(
        &self,
        out: &mut MoveList,
        us: Color,
        occupied: Bitboard,
        king_danger: Bitboard,
        king_sq: Square,
    ) {
        let rank = back_rank(us);
        // The king must be on its home square (e-file) for standard castling.
        if king_sq != Square::from_file_rank(File::E, rank) {
            return;
        }

        for (side, king_dest_file, rook_dest_file) in [
            (CastleSide::King, File::G, File::F),
            (CastleSide::Queen, File::C, File::D),
        ] {
            let Some(rook_file) = self.castling.rook_file(us, side) else {
                continue;
            };
            let rook_from = Square::from_file_rank(rook_file, rank);
            // The rook must actually be present.
            if self.board.piece_at(rook_from) != Some(Piece::new(us, Role::Rook)) {
                continue;
            }
            let king_dest = Square::from_file_rank(king_dest_file, rank);
            let rook_dest = Square::from_file_rank(rook_dest_file, rank);

            // All squares the king passes through (inclusive of start and end)
            // and the rook's path must be clear (excepting the king and rook
            // themselves), and the king's path must not be attacked.
            let king_path = between(king_sq, king_dest).with(king_dest);
            let rook_path = between(rook_from, rook_dest).with(rook_dest);

            // Squares that must be empty: everything between/at king and rook
            // destinations, excluding the king's and rook's own squares.
            let must_be_empty = (king_path | rook_path).without(king_sq).without(rook_from);
            if !(must_be_empty & occupied).is_empty() {
                continue;
            }

            // The king must not pass through or land on an attacked square.
            let king_walk = between(king_sq, king_dest).with(king_dest);
            if !(king_walk & king_danger).is_empty() {
                continue;
            }

            let kind = match side {
                CastleSide::King => MoveKind::CastleKingside,
                CastleSide::Queen => MoveKind::CastleQueenside,
            };
            out.push(Move::new(king_sq, king_dest, kind));
        }
    }

    /// Pushes the legal castling moves for an *arbitrary* king/rook placement
    /// (Chess960) into `out`, using each side's destination files from `geom`
    /// and the rook start files already stored in the castling rights.
    ///
    /// Generalizes [`Position::gen_castles`]: the king may sit on any file (its
    /// actual square is read from the board), and the king or the castling rook
    /// may already stand on its destination, or be adjacent. The path-must-be-
    /// empty and king-walk-must-be-safe conditions are computed from the real
    /// squares, excepting the castling king and rook from the empty test so a
    /// rook the king passes over (or vice versa) does not block. The king is
    /// removed from the occupancy when computing danger so it cannot shield
    /// itself, matching [`Position::pseudo_into`]. King safety of the final
    /// position is still left to the caller's filter.
    ///
    /// `geom(side)` returns `(king_dest_file, rook_dest_file)` for that side, or
    /// `None` if the variant does not offer it.
    pub(crate) fn gen_castles_960(
        &self,
        out: &mut MoveList,
        geom: impl Fn(CastleSide) -> Option<(File, File)>,
    ) {
        let us = self.turn;
        let them = us.opposite();
        let Some(king_sq) = self.board.king_of(us) else {
            return;
        };
        let rank = back_rank(us);
        if king_sq.rank() != rank {
            return;
        }
        let occupied = self.board.occupied();
        // Castling is illegal out of check.
        if !self.attackers_to(king_sq, them, occupied).is_empty() {
            return;
        }
        let occ_without_king = occupied.without(king_sq);
        let king_danger = self.attacked_by(them, occ_without_king);

        for side in [CastleSide::King, CastleSide::Queen] {
            let Some(rook_file) = self.castling.rook_file(us, side) else {
                continue;
            };
            let Some((king_dest_file, rook_dest_file)) = geom(side) else {
                continue;
            };
            let rook_from = Square::from_file_rank(rook_file, rank);
            // The rook named by the rights must actually be present.
            if self.board.piece_at(rook_from) != Some(Piece::new(us, Role::Rook)) {
                continue;
            }
            let king_dest = Square::from_file_rank(king_dest_file, rank);
            let rook_dest = Square::from_file_rank(rook_dest_file, rank);

            // Squares the king travels through (inclusive of start and end) and
            // the rook travels through (inclusive of end) must be empty, except
            // for the castling king and rook themselves.
            let king_path = between(king_sq, king_dest).with(king_dest).with(king_sq);
            let rook_path = between(rook_from, rook_dest)
                .with(rook_dest)
                .with(rook_from);
            let must_be_empty = (king_path | rook_path).without(king_sq).without(rook_from);
            if !(must_be_empty & occupied).is_empty() {
                continue;
            }

            // The king must not pass through or land on an attacked square.
            let king_walk = between(king_sq, king_dest).with(king_dest);
            if !(king_walk & king_danger).is_empty() {
                continue;
            }

            let kind = match side {
                CastleSide::King => MoveKind::CastleKingside,
                CastleSide::Queen => MoveKind::CastleQueenside,
            };
            out.push(Move::new(king_sq, king_dest, kind));
        }
    }

    // -- Make move ---------------------------------------------------------

    /// Applies `mv` to this position, returning the resulting position.
    ///
    /// The move must be legal for this position. `play` does not re-validate
    /// legality; pass only moves obtained from [`Position::legal_moves`] (or
    /// validate with [`Position::is_legal`] first).
    #[must_use]
    pub fn play(&self, mv: &Move) -> Position {
        let mut next = self.clone();
        next.apply(mv);
        next
    }

    /// Applies `mv` to a clone of this position, returning the successor and the
    /// piece captured by the move (if any), for variant capture side-effects.
    ///
    /// The captured piece is the one that stood on the square the move removed an
    /// enemy from — the destination for ordinary captures and capturing
    /// promotions, the en-passant pawn's square for en passant. Quiet moves,
    /// castling, and drops capture nothing.
    #[must_use]
    pub(crate) fn play_tracking_capture(&self, mv: &Move) -> (Position, Option<(Piece, Square)>) {
        let captured = self.captured_piece(mv);
        (self.play(mv), captured)
    }

    /// The enemy piece (and its square) a move removes from the board, if any.
    fn captured_piece(&self, mv: &Move) -> Option<(Piece, Square)> {
        match mv.kind() {
            MoveKind::Capture | MoveKind::Promotion { capture: true, .. } => {
                self.board.piece_at(mv.to()).map(|p| (p, mv.to()))
            }
            MoveKind::EnPassant => {
                let captured = Square::from_file_rank(mv.to().file(), mv.from().rank());
                self.board.piece_at(captured).map(|p| (p, captured))
            }
            _ => None,
        }
    }

    /// Drops a `role` piece of the side to move onto the empty square `to`,
    /// flipping the side to move and maintaining the incremental Zobrist key and
    /// clocks — the core edit behind a crazyhouse drop, exposed for the variant
    /// layer's `apply_extra` hook.
    ///
    /// A drop clears any en-passant target, never resets the halfmove clock by
    /// itself (it is neither a capture nor a pawn *move*), and increments the
    /// fullmove number after a black move, exactly like a quiet move.
    // Forward-looking plumbing for the crazyhouse variant's drop apply path; no
    // variant in this crate emits drops yet, so it is unused outside its test.
    #[allow(dead_code)]
    pub(crate) fn apply_drop_core(&mut self, role: Role, to: Square) {
        let us = self.turn;
        let them = us.opposite();

        if let Some(file) = self.zobrist_ep_file() {
            self.hash ^= crate::zobrist::ep_file_key(file);
        }
        self.ep_square = None;

        self.hash_set(to, Piece::new(us, role));

        self.halfmove_clock += 1;
        if us.is_black() {
            self.fullmove_number += 1;
        }
        self.turn = them;
        self.hash ^= crate::zobrist::side_key(us);
        self.hash ^= crate::zobrist::side_key(them);
    }

    /// Folds an opaque extra-state contribution into the incremental Zobrist key,
    /// for variants that hash pocket / counter state. Idempotent under XOR, so a
    /// variant toggles its old contribution out and the new one in.
    pub(crate) fn xor_hash(&mut self, key: u64) {
        self.hash ^= key;
    }

    /// Removes whatever piece sits on `square` (if any), keeping the incremental
    /// Zobrist key consistent and revoking any castling right anchored on a rook
    /// that is removed. Exposed for the atomic variant's explosion side effect.
    ///
    /// Returns the removed piece, or `None` if the square was empty.
    pub(crate) fn remove_piece_tracked(&mut self, square: Square) -> Option<Piece> {
        let piece = self.board.piece_at(square)?;
        self.hash_remove(square, piece);
        if piece.role == Role::Rook {
            self.revoke_rights_for_square(square, piece.color);
        }
        Some(piece)
    }

    /// In-place application of a move (see [`Position::play`]).
    fn apply(&mut self, mv: &Move) {
        let us = self.turn;
        let them = us.opposite();
        let from = mv.from();
        let to = mv.to();
        let rank = back_rank(us);

        let moving = self
            .board
            .piece_at(from)
            .expect("move originates from an occupied square");

        let is_pawn_move = moving.role == Role::Pawn;
        let mut reset_clock = is_pawn_move;

        // Incremental Zobrist: XOR out the parent's en-passant and castling
        // features now; piece moves are folded in as the board is edited, and the
        // new ep/castling/side features are folded back in once they are settled
        // at the end.
        if let Some(file) = self.zobrist_ep_file() {
            self.hash ^= crate::zobrist::ep_file_key(file);
        }
        self.hash ^= self.castling_hash();

        // Clear any prior en-passant target; set below only for a double push.
        let prev_ep = self.ep_square.take();

        match mv.kind() {
            MoveKind::Quiet => {
                self.hash_remove(from, moving);
                self.hash_set(to, moving);
            }
            MoveKind::Capture => {
                reset_clock = true;
                if let Some(captured) = self.board.piece_at(to) {
                    self.hash_remove(to, captured);
                }
                self.hash_remove(from, moving);
                self.hash_set(to, moving);
            }
            MoveKind::DoublePawnPush => {
                self.hash_remove(from, moving);
                self.hash_set(to, moving);
                // The ep target is the square the pawn skipped over.
                let mid_rank = from.rank().offset(if us.is_white() { 1 } else { -1 });
                if let Some(mid_rank) = mid_rank {
                    self.ep_square = Some(Square::from_file_rank(from.file(), mid_rank));
                }
            }
            MoveKind::EnPassant => {
                reset_clock = true;
                self.hash_remove(from, moving);
                self.hash_set(to, moving);
                // Remove the captured pawn, which is on `to`'s file and `from`'s
                // rank.
                let captured = Square::from_file_rank(to.file(), from.rank());
                let captured_pawn = Piece::new(them, Role::Pawn);
                self.hash_remove(captured, captured_pawn);
            }
            MoveKind::CastleKingside | MoveKind::CastleQueenside => {
                let side = if matches!(mv.kind(), MoveKind::CastleKingside) {
                    CastleSide::King
                } else {
                    CastleSide::Queen
                };
                let rook_file = self
                    .castling
                    .rook_file(us, side)
                    .expect("castling right present for a castling move");
                let rook_from = Square::from_file_rank(rook_file, rank);
                let rook_dest_file = match side {
                    CastleSide::King => File::F,
                    CastleSide::Queen => File::D,
                };
                let rook_to = Square::from_file_rank(rook_dest_file, rank);
                let rook = Piece::new(us, Role::Rook);
                // Move king and rook. Remove both first to handle the case where
                // a destination coincides with the other's origin.
                self.hash_remove(from, moving);
                self.hash_remove(rook_from, rook);
                self.hash_set(to, moving);
                self.hash_set(rook_to, rook);
            }
            MoveKind::Promotion { role, capture } => {
                reset_clock = capture || is_pawn_move;
                if capture {
                    if let Some(captured) = self.board.piece_at(to) {
                        self.hash_remove(to, captured);
                    }
                }
                self.hash_remove(from, moving);
                self.hash_set(to, Piece::new(us, role));
            }
            MoveKind::Drop { .. } => {
                // Drops are a variant-only move kind; the core never generates
                // them and applies them through `apply_drop_core` instead, so
                // they never reach the standard make-move path.
                unreachable!("drop moves are applied via apply_drop_core");
            }
        }
        let _ = prev_ep;

        // Update castling rights: a king move revokes both, a rook move from its
        // home square revokes that side, capturing a rook on its home square
        // revokes the opponent's side.
        if moving.role == Role::King {
            self.castling.revoke_color(us);
        }
        self.revoke_rights_for_square(from, us);
        // A capture (or capturing promotion) on a rook home square removes the
        // opponent's matching right.
        if mv.is_capture() && !matches!(mv.kind(), MoveKind::EnPassant) {
            self.revoke_rights_for_square(to, them);
        }

        if reset_clock {
            self.halfmove_clock = 0;
        } else {
            self.halfmove_clock += 1;
        }
        if us.is_black() {
            self.fullmove_number += 1;
        }
        self.turn = them;

        // Fold the settled castling and (capture-available) en-passant features,
        // plus the new side-to-move toggle, back into the key.
        self.hash ^= self.castling_hash();
        if let Some(file) = self.zobrist_ep_file() {
            self.hash ^= crate::zobrist::ep_file_key(file);
        }
        // Side to move flipped from `us` to `them`; toggle that feature.
        self.hash ^= crate::zobrist::side_key(us);
        self.hash ^= crate::zobrist::side_key(them);
    }

    /// Removes a known `piece` from `square`, keeping the Zobrist key in step.
    #[inline]
    fn hash_remove(&mut self, square: Square, piece: Piece) {
        self.board.remove_piece(square);
        self.hash ^= crate::zobrist::piece_square_key(piece, square);
    }

    /// Places a known `piece` on `square`, keeping the Zobrist key in step.
    #[inline]
    fn hash_set(&mut self, square: Square, piece: Piece) {
        self.board.set_piece(square, piece);
        self.hash ^= crate::zobrist::piece_square_key(piece, square);
    }

    /// The XOR of the keys for all castling rights currently held.
    #[inline]
    fn castling_hash(&self) -> u64 {
        let mut h = 0;
        for color in Color::ALL {
            for side in [CastleSide::King, CastleSide::Queen] {
                if self.castling.has(color, side) {
                    h ^= crate::zobrist::castling_key(color, side);
                }
            }
        }
        h
    }

    /// If `square` is the home square of a castling rook of `color`, revoke that
    /// castling right.
    fn revoke_rights_for_square(&mut self, square: Square, color: Color) {
        if self.castling.is_empty() {
            return;
        }
        let rank = back_rank(color);
        if square.rank() != rank {
            return;
        }
        for side in [CastleSide::King, CastleSide::Queen] {
            if let Some(file) = self.castling.rook_file(color, side) {
                if Square::from_file_rank(file, rank) == square {
                    self.castling.set(color, side, None);
                }
            }
        }
    }

    // -- UCI ---------------------------------------------------------------

    /// Parses a UCI move string against this position, resolving the
    /// context-sensitive move kind (capture, double push, en passant, castling,
    /// promotion).
    ///
    /// The returned move is guaranteed to be one of this position's legal moves.
    ///
    /// # Errors
    ///
    /// Returns [`ParseUciError`] if the string is malformed or does not name a
    /// legal move in this position.
    pub fn parse_uci(&self, uci: &str) -> Result<Move, ParseUciError> {
        let bytes = uci.as_bytes();
        // UCI is an ASCII-only grammar; reject non-ASCII up front so the
        // byte-indexed slicing below can never split a multi-byte UTF-8 char.
        if !uci.is_ascii() {
            return Err(ParseUciError::Malformed);
        }
        if bytes.len() != 4 && bytes.len() != 5 {
            return Err(ParseUciError::Malformed);
        }
        let from = uci[0..2]
            .parse::<Square>()
            .map_err(|_| ParseUciError::Malformed)?;
        let to = uci[2..4]
            .parse::<Square>()
            .map_err(|_| ParseUciError::Malformed)?;
        let promo = if bytes.len() == 5 {
            let role =
                Role::from_char(uci.as_bytes()[4] as char).ok_or(ParseUciError::Malformed)?;
            if matches!(role, Role::Pawn | Role::King) {
                return Err(ParseUciError::Malformed);
            }
            Some(role)
        } else {
            None
        };

        for mv in self.legal_moves() {
            if mv.from() == from && mv.to() == to && mv.promotion() == promo {
                return Ok(mv);
            }
        }
        Err(ParseUciError::Illegal)
    }

    // -- FEN ---------------------------------------------------------------

    /// Parses a position from a full six-field FEN string.
    ///
    /// # Errors
    ///
    /// Returns [`FenError`] if any field is missing, malformed, or describes an
    /// impossible position.
    pub fn from_fen(fen: &str) -> Result<Position, FenError> {
        let mut fields = fen.split_whitespace();

        let placement = fields.next().ok_or(FenError::MissingField)?;
        let board = Board::from_fen_placement(placement).map_err(FenError::Placement)?;

        let turn = match fields.next().ok_or(FenError::MissingField)? {
            "w" => Color::White,
            "b" => Color::Black,
            other => return Err(FenError::BadTurn(other.to_owned())),
        };

        let castling_field = fields.next().ok_or(FenError::MissingField)?;
        let castling = parse_castling(castling_field, &board)?;

        let ep_field = fields.next().ok_or(FenError::MissingField)?;
        let ep_square = parse_ep_field(ep_field)?;

        let halfmove_clock = match fields.next() {
            Some(s) => parse_clock(s)?,
            None => 0,
        };
        let fullmove_number = match fields.next() {
            Some(s) => parse_clock(s)?,
            None => 1,
        };

        if fields.next().is_some() {
            return Err(FenError::TrailingData);
        }

        let mut position = Position {
            board,
            turn,
            castling,
            ep_square,
            halfmove_clock,
            fullmove_number,
            hash: 0,
        };
        position.hash = position.compute_zobrist();
        position.validate()?;
        Ok(position)
    }

    /// Assembles a [`Position`] from already-parsed component fields and computes
    /// its Zobrist key, *without* the standard two-kings / opposite-king-in-check
    /// validation.
    ///
    /// Variant FEN parsers reuse the [`Position`] FEN sub-parsers
    /// ([`parse_castling_field`], [`parse_ep_field`], [`parse_clock`]) and then
    /// build the core through this constructor, applying their own validation via
    /// [`Position::validate_core`].
    #[must_use]
    pub(crate) fn from_fields(
        board: Board,
        turn: Color,
        castling: CastlingRights,
        ep_square: Option<Square>,
        halfmove_clock: u32,
        fullmove_number: u32,
    ) -> Position {
        let mut position = Position {
            board,
            turn,
            castling,
            ep_square,
            halfmove_clock,
            fullmove_number,
            hash: 0,
        };
        position.hash = position.compute_zobrist();
        position
    }

    /// Validates the core position with relaxed king requirements, for variant
    /// reuse.
    ///
    /// With `require_two_kings` and `king_is_royal` both true this is exactly the
    /// standard check: one king per side, and the side not to move not left in
    /// check. Kingless variants (horde) pass `require_two_kings = false` to skip
    /// the king-count requirement. Non-royal-king variants (antichess) pass
    /// `king_is_royal = false` as well, which also drops the
    /// opposite-king-in-check rule: a king may be left under attack ("en prise")
    /// when there is no concept of check.
    pub(crate) fn validate_core(
        &self,
        require_two_kings: bool,
        king_is_royal: bool,
    ) -> Result<(), FenError> {
        if require_two_kings {
            for color in Color::ALL {
                if self.board.pieces(color, Role::King).count() != 1 {
                    return Err(FenError::BadKings);
                }
            }
        }
        if king_is_royal {
            let them = self.turn.opposite();
            if let Some(their_king) = self.board.king_of(them) {
                if self.is_attacked(their_king, self.turn) {
                    return Err(FenError::OppositeKingInCheck);
                }
            }
        }
        Ok(())
    }

    /// Serializes the six standard FEN fields into `out` (no trailing space),
    /// shared by [`Position::to_fen`] and variant FEN writers that append extra
    /// fields afterward.
    pub(crate) fn write_core_fen(&self, out: &mut String) {
        self.write_core_fen_with(&self.castling_field(), out);
    }

    /// Serializes the six standard FEN fields into `out`, but with a
    /// caller-supplied castling field, so variant FEN writers can substitute a
    /// 960 (X-FEN / Shredder) castling field while reusing every other field.
    pub(crate) fn write_core_fen_with(&self, castling_field: &str, out: &mut String) {
        self.write_core_fen_with_placement(&self.board.to_fen_placement(), castling_field, out);
    }

    /// Serializes the six standard FEN fields into `out` with both a
    /// caller-supplied placement field and castling field, so a variant whose
    /// placement carries extra markers (crazyhouse pockets / `~`) can substitute
    /// it while reusing every other field.
    pub(crate) fn write_core_fen_with_placement(
        &self,
        placement_field: &str,
        castling_field: &str,
        out: &mut String,
    ) {
        out.push_str(placement_field);
        out.push(' ');
        out.push(if self.turn.is_white() { 'w' } else { 'b' });
        out.push(' ');
        out.push_str(castling_field);
        out.push(' ');
        match self.ep_square {
            Some(sq) => out.push_str(&sq.to_string()),
            None => out.push('-'),
        }
        out.push(' ');
        out.push_str(&self.halfmove_clock.to_string());
        out.push(' ');
        out.push_str(&self.fullmove_number.to_string());
    }

    /// Basic sanity checks: each side has exactly one king, and the side *not*
    /// to move is not in check (which would mean the previous move was illegal).
    fn validate(&self) -> Result<(), FenError> {
        self.validate_core(true, true)
    }

    /// Serializes this position as a full six-field FEN string.
    #[must_use]
    pub fn to_fen(&self) -> String {
        let mut fen = String::new();
        self.write_core_fen(&mut fen);
        fen
    }

    /// Renders the castling-rights FEN field (`KQkq`, a subset, or `-`).
    fn castling_field(&self) -> String {
        let mut s = String::new();
        write_standard_castling_field(self.castling, &mut s);
        s
    }
}

impl FromStr for Position {
    type Err = FenError;

    fn from_str(s: &str) -> Result<Position, FenError> {
        Position::from_fen(s)
    }
}

impl fmt::Display for Position {
    /// Formats the position as FEN.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_fen())
    }
}

/// The mask of light squares (a1 is dark), used for bishop-color analysis.
const LIGHT_SQUARES: u64 = 0x55AA_55AA_55AA_55AA;

/// The roles a pawn may promote to, in a stable order.
const PROMOTION_ROLES: [Role; 4] = [Role::Knight, Role::Bishop, Role::Rook, Role::Queen];

/// Parses the en-passant FEN field (`-` or a target square on the 3rd/6th rank)
/// into an optional target square. Shared by [`Position::from_fen`] and variant
/// FEN parsers.
pub(crate) fn parse_ep_field(field: &str) -> Result<Option<Square>, FenError> {
    if field == "-" {
        return Ok(None);
    }
    let sq = field
        .parse::<Square>()
        .map_err(|_| FenError::BadEnPassant(field.to_owned()))?;
    // En-passant target must be on the 3rd or 6th rank.
    match sq.rank() {
        Rank::Third | Rank::Sixth => Ok(Some(sq)),
        _ => Err(FenError::BadEnPassant(field.to_owned())),
    }
}

/// Parses a non-negative move-clock FEN field. Shared by [`Position::from_fen`]
/// and variant FEN parsers.
pub(crate) fn parse_clock(field: &str) -> Result<u32, FenError> {
    field
        .parse::<u32>()
        .map_err(|_| FenError::BadNumber(field.to_owned()))
}

/// Parses the castling-rights FEN field into [`CastlingRights`], validating the
/// rook squares against the placement so impossible fields are rejected. Shared
/// by [`Position::from_fen`] and variant FEN parsers.
pub(crate) fn parse_castling_field(field: &str, board: &Board) -> Result<CastlingRights, FenError> {
    parse_castling(field, board)
}

/// Renders the standard (`KQkq`) castling field for the given rights, shared by
/// [`Position::to_fen`] and variant FEN writers that fall back to the standard
/// form when both rooks sit on the a-/h-files.
pub(crate) fn write_standard_castling_field(rights: CastlingRights, out: &mut String) {
    let start = out.len();
    if rights.has(Color::White, CastleSide::King) {
        out.push('K');
    }
    if rights.has(Color::White, CastleSide::Queen) {
        out.push('Q');
    }
    if rights.has(Color::Black, CastleSide::King) {
        out.push('k');
    }
    if rights.has(Color::Black, CastleSide::Queen) {
        out.push('q');
    }
    if out.len() == start {
        out.push('-');
    }
}

/// Parses the castling-rights FEN field into [`CastlingRights`], validating the
/// rook squares against the placement so impossible fields are rejected.
fn parse_castling(field: &str, board: &Board) -> Result<CastlingRights, FenError> {
    let mut rights = CastlingRights::NONE;
    if field == "-" {
        return Ok(rights);
    }
    for ch in field.chars() {
        let (color, side) = match ch {
            'K' => (Color::White, CastleSide::King),
            'Q' => (Color::White, CastleSide::Queen),
            'k' => (Color::Black, CastleSide::King),
            'q' => (Color::Black, CastleSide::Queen),
            _ => return Err(FenError::BadCastling(field.to_owned())),
        };
        // Standard castling rooks: h-file for king-side, a-file for queen-side.
        let file = match side {
            CastleSide::King => File::H,
            CastleSide::Queen => File::A,
        };
        let rank = back_rank(color);
        // The named rook must be present, or the field is inconsistent.
        if board.piece_at(Square::from_file_rank(file, rank)) != Some(Piece::new(color, Role::Rook))
        {
            return Err(FenError::BadCastling(field.to_owned()));
        }
        rights.set(color, side, Some(file));
    }
    Ok(rights)
}

/// Counts the number of leaf nodes reachable in exactly `depth` plies from
/// `position`, the standard *perft* (performance test) used to validate move
/// generation against known reference counts.
///
/// ```
/// use mce::{perft, Position};
/// assert_eq!(perft(&Position::startpos(), 1), 20);
/// assert_eq!(perft(&Position::startpos(), 2), 400);
/// ```
#[must_use]
pub fn perft(position: &Position, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    // Allocate one reusable move buffer per interior ply once, up front, and
    // thread them through the recursion. Each buffer is `clear`ed (not freshly
    // value-initialized) before reuse at its ply, so perft touches the stack
    // buffers exactly `depth - 1` times total rather than allocating a fresh
    // `MoveList` per node. The leaf ply counts moves directly without filling a
    // buffer.
    let mut buffers: Vec<MoveList> = (0..depth).map(|_| MoveList::new()).collect();
    perft_with(position, depth, &mut buffers)
}

/// Recursive core of [`perft`], reusing the caller-owned per-ply `buffers` so no
/// move buffer is allocated per node. `buffers[0]` belongs to the current ply;
/// each buffer is `clear`ed (not value-initialized) before reuse, so the buffers
/// are filled in place rather than reallocated per node.
fn perft_with(position: &Position, depth: u32, buffers: &mut [MoveList]) -> u64 {
    let (here, rest) = buffers.split_first_mut().expect("a buffer per ply");
    here.clear();
    position.generate_into(here);
    if depth == 1 {
        return here.len() as u64;
    }
    let mut nodes = 0;
    here.for_each(|mv| {
        nodes += perft_with(&position.play(&mv), depth - 1, rest);
    });
    nodes
}

/// Like [`perft`], but returns the per-move leaf counts at the root, the
/// breakdown used to debug a mismatching total against a reference engine.
#[must_use]
pub fn perft_divide(position: &Position, depth: u32) -> Vec<(Move, u64)> {
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

/// The error returned when a six-field FEN string cannot be parsed into a
/// [`Position`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FenError {
    /// A required FEN field was missing.
    MissingField,
    /// The piece-placement field was invalid; wraps the placement error.
    Placement(crate::ParseBoardError),
    /// The side-to-move field was neither `w` nor `b`.
    BadTurn(String),
    /// The castling field was malformed or inconsistent with the placement.
    BadCastling(String),
    /// The en-passant field was not `-` or a valid target square.
    BadEnPassant(String),
    /// A move-clock field was not a non-negative integer.
    BadNumber(String),
    /// The position does not have exactly one king per side.
    BadKings,
    /// The side not to move is in check, so the position is unreachable.
    OppositeKingInCheck,
    /// Extra data followed the six FEN fields.
    TrailingData,
}

impl fmt::Display for FenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FenError::MissingField => f.write_str("FEN is missing a required field"),
            FenError::Placement(e) => write!(f, "invalid FEN piece placement: {e}"),
            FenError::BadTurn(s) => write!(f, "invalid side-to-move field {s:?}, expected w or b"),
            FenError::BadCastling(s) => write!(f, "invalid or inconsistent castling field {s:?}"),
            FenError::BadEnPassant(s) => write!(f, "invalid en-passant field {s:?}"),
            FenError::BadNumber(s) => write!(f, "invalid move-clock number {s:?}"),
            FenError::BadKings => f.write_str("position must have exactly one king per side"),
            FenError::OppositeKingInCheck => {
                f.write_str("the side not to move is in check (unreachable position)")
            }
            FenError::TrailingData => f.write_str("unexpected trailing data after FEN fields"),
        }
    }
}

impl std::error::Error for FenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FenError::Placement(e) => Some(e),
            _ => None,
        }
    }
}

/// The error returned when a UCI move string cannot be resolved against a
/// position by [`Position::parse_uci`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ParseUciError {
    /// The string was not a valid UCI move (wrong length, bad squares, or an
    /// invalid promotion letter).
    Malformed,
    /// The string was well-formed but names no legal move in the position.
    Illegal,
}

impl fmt::Display for ParseUciError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseUciError::Malformed => f.write_str("malformed UCI move string"),
            ParseUciError::Illegal => f.write_str("UCI move is not legal in this position"),
        }
    }
}

impl std::error::Error for ParseUciError {}

#[cfg(test)]
mod tests {
    use super::*;

    const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
    const POS3: &str = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1";
    const POS4: &str = "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1";
    const POS5: &str = "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8";
    const POS6: &str = "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10";

    #[test]
    fn startpos_fen_round_trip() {
        let pos = Position::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
        assert_eq!(Position::default(), pos);
        let parsed = Position::from_fen(&pos.to_fen()).unwrap();
        assert_eq!(parsed, pos);
    }

    #[test]
    fn fen_round_trips_reference_positions() {
        for fen in [KIWIPETE, POS3, POS4, POS5, POS6] {
            let pos = Position::from_fen(fen).unwrap();
            assert_eq!(pos.to_fen(), fen, "round-trip failed for {fen}");
        }
    }

    #[test]
    fn startpos_has_twenty_moves() {
        let pos = Position::startpos();
        assert_eq!(pos.legal_moves().len(), 20);
        assert_eq!(pos.legal_move_count(), 20);
        assert!(!pos.is_check());
        assert!(!pos.is_checkmate());
        assert!(!pos.is_stalemate());
    }

    #[test]
    fn rejects_invalid_fens() {
        // Missing fields.
        assert!(Position::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR").is_err());
        // Bad side to move.
        assert_eq!(
            Position::from_fen("8/8/8/8/8/8/8/K6k x - - 0 1").unwrap_err(),
            FenError::BadTurn("x".to_owned())
        );
        // Two white kings.
        assert_eq!(
            Position::from_fen("8/8/8/8/8/8/8/KK5k w - - 0 1").unwrap_err(),
            FenError::BadKings
        );
        // Side not to move is in check: black to move, but white's king (the
        // side that just moved) is attacked by a black rook on e8.
        assert_eq!(
            Position::from_fen("4r2k/8/8/8/8/8/8/4K3 b - - 0 1")
                .map(|_| ())
                .unwrap_err(),
            FenError::OppositeKingInCheck
        );
        // Castling field names a rook that is not there.
        assert!(matches!(
            Position::from_fen("8/8/8/8/8/8/8/4K2k w K - 0 1").unwrap_err(),
            FenError::BadCastling(_)
        ));
        // En-passant on the wrong rank.
        assert!(matches!(
            Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - e4 0 1").unwrap_err(),
            FenError::BadEnPassant(_)
        ));
    }

    #[test]
    fn checkmate_and_stalemate() {
        // Fool's mate position (black just delivered mate).
        let fen = "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3";
        let pos = Position::from_fen(fen).unwrap();
        assert!(pos.is_check());
        assert!(pos.is_checkmate());
        assert!(!pos.is_stalemate());

        // Classic stalemate: black king on a8, white king c6, white queen c7? No
        // -- use a standard one: black to move, king h8 boxed.
        let sm = "7k/5Q2/6K1/8/8/8/8/8 b - - 0 1";
        let pos = Position::from_fen(sm).unwrap();
        assert!(!pos.is_check());
        assert!(pos.is_stalemate());
        assert!(!pos.is_checkmate());
    }

    #[test]
    fn play_updates_state() {
        let pos = Position::startpos();
        let e4 = pos.parse_uci("e2e4").unwrap();
        assert_eq!(e4.kind(), MoveKind::DoublePawnPush);
        let after = pos.play(&e4);
        assert_eq!(after.turn(), Color::Black);
        assert_eq!(after.ep_square(), Some(Square::E3));
        assert_eq!(after.halfmove_clock(), 0);
        assert_eq!(after.fullmove_number(), 1);
        assert_eq!(
            after.to_fen(),
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1"
        );

        // A black reply increments the fullmove number.
        let c5 = after.parse_uci("c7c5").unwrap();
        let after2 = after.play(&c5);
        assert_eq!(after2.fullmove_number(), 2);
        assert_eq!(after2.turn(), Color::White);
        assert_eq!(after2.ep_square(), Some(Square::C6));
    }

    #[test]
    fn castling_moves_rook_and_revokes_rights() {
        // White to move, can castle king-side.
        let fen = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let oo = pos.parse_uci("e1g1").unwrap();
        assert_eq!(oo.kind(), MoveKind::CastleKingside);
        let after = pos.play(&oo);
        assert_eq!(
            after.board().piece_at(Square::G1),
            Some(Piece::new(Color::White, Role::King))
        );
        assert_eq!(
            after.board().piece_at(Square::F1),
            Some(Piece::new(Color::White, Role::Rook))
        );
        // White lost both castling rights.
        assert!(!after.castling_rights().has(Color::White, CastleSide::King));
        assert!(!after.castling_rights().has(Color::White, CastleSide::Queen));
        // Black retains its rights.
        assert!(after.castling_rights().has(Color::Black, CastleSide::King));

        // Queen-side.
        let ooo = pos.parse_uci("e1c1").unwrap();
        let after = pos.play(&ooo);
        assert_eq!(
            after.board().piece_at(Square::C1),
            Some(Piece::new(Color::White, Role::King))
        );
        assert_eq!(
            after.board().piece_at(Square::D1),
            Some(Piece::new(Color::White, Role::Rook))
        );
    }

    #[test]
    fn capturing_a_rook_revokes_opponent_right() {
        // White rook on a1 captures black rook on a8 -> black loses queenside.
        let fen = "r3k3/8/8/8/8/8/8/R3K3 w Qq - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        // Need a path: rook a1 to a8 is clear here.
        let cap = pos.parse_uci("a1a8").unwrap();
        assert!(cap.is_capture());
        let after = pos.play(&cap);
        assert!(!after.castling_rights().has(Color::Black, CastleSide::Queen));
    }

    #[test]
    fn en_passant_capture_removes_pawn() {
        // White pawn d5, black plays c7c5 -> white can capture en passant d5xc6.
        let pos = Position::from_fen("4k3/2p5/8/3P4/8/8/8/4K3 b - - 0 1").unwrap();
        let c5 = pos.parse_uci("c7c5").unwrap();
        let after = pos.play(&c5);
        assert_eq!(after.ep_square(), Some(Square::C6));
        let ep = after.parse_uci("d5c6").unwrap();
        assert_eq!(ep.kind(), MoveKind::EnPassant);
        let done = after.play(&ep);
        // Captured pawn on c5 is gone; white pawn now on c6.
        assert_eq!(done.board().piece_at(Square::C5), None);
        assert_eq!(
            done.board().piece_at(Square::C6),
            Some(Piece::new(Color::White, Role::Pawn))
        );
    }

    #[test]
    fn promotion_moves_generated_and_applied() {
        let pos = Position::from_fen("4k3/P7/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        let promos: Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.from() == Square::A7)
            .collect();
        assert_eq!(promos.len(), 4);
        let q = pos.parse_uci("a7a8q").unwrap();
        let after = pos.play(&q);
        assert_eq!(
            after.board().piece_at(Square::A8),
            Some(Piece::new(Color::White, Role::Queen))
        );
    }

    #[test]
    fn parse_uci_rejects_garbage() {
        let pos = Position::startpos();
        assert_eq!(pos.parse_uci("xyz").unwrap_err(), ParseUciError::Malformed);
        // Wrong length.
        assert_eq!(pos.parse_uci("e2e").unwrap_err(), ParseUciError::Malformed);
        // 'k' is not a valid promotion letter.
        assert_eq!(
            pos.parse_uci("e7e8k").unwrap_err(),
            ParseUciError::Malformed
        );
        // e2e4 with a promotion suffix is well-formed but not legal.
        assert_eq!(pos.parse_uci("e2e4q").unwrap_err(), ParseUciError::Illegal);
        // e2e5 is not legal.
        assert_eq!(pos.parse_uci("e2e5").unwrap_err(), ParseUciError::Illegal);
    }

    #[test]
    fn parse_uci_rejects_non_ascii_without_panic() {
        let pos = Position::startpos();
        // Multi-byte UTF-8 chars at various offsets must not split a slice.
        for s in [
            "\u{e9}e2e4",     // leading multi-byte char (2 bytes)
            "e\u{e9}e2e4",    // multi-byte char at byte offset 1
            "e2\u{e9}e4",     // multi-byte char straddling the from/to boundary
            "e2e\u{e9}4",     // multi-byte char at offset 3
            "e2e4\u{e9}",     // multi-byte promotion suffix
            "\u{1f600}e2e4",  // emoji (4 bytes)
            "e2e4\u{301}",    // combining acute accent
            "\u{301}\u{301}", // combining marks only
        ] {
            assert_eq!(
                pos.parse_uci(s).unwrap_err(),
                ParseUciError::Malformed,
                "{s:?} should be rejected as malformed"
            );
        }
        // Short / odd byte lengths must also be rejected, not panic.
        for s in ["", "e", "e2", "e2e", "e2e4e5"] {
            assert_eq!(pos.parse_uci(s).unwrap_err(), ParseUciError::Malformed);
        }
    }

    #[test]
    fn insufficient_material() {
        assert!(Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1")
            .unwrap()
            .is_insufficient_material());
        assert!(Position::from_fen("4k3/8/8/8/8/8/8/4KB2 w - - 0 1")
            .unwrap()
            .is_insufficient_material());
        assert!(Position::from_fen("4k3/8/8/8/8/8/8/4KN2 w - - 0 1")
            .unwrap()
            .is_insufficient_material());
        // King + rook is sufficient.
        assert!(!Position::from_fen("4k3/8/8/8/8/8/8/4KR2 w - - 0 1")
            .unwrap()
            .is_insufficient_material());
        // King + pawn is sufficient.
        assert!(!Position::from_fen("4k3/8/8/8/8/4P3/8/4K3 w - - 0 1")
            .unwrap()
            .is_insufficient_material());
    }

    #[test]
    fn perft_startpos_shallow() {
        let pos = Position::startpos();
        assert_eq!(perft(&pos, 1), 20);
        assert_eq!(perft(&pos, 2), 400);
        assert_eq!(perft(&pos, 3), 8902);
    }

    #[test]
    fn apply_drop_core_places_piece_and_keeps_hash() {
        // Drop plumbing for crazyhouse: placing a pocketed piece on an empty
        // square flips the side, leaves clocks/ep correct, and keeps the
        // incremental Zobrist key in step with a from-scratch computation.
        let mut pos = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 5 9").unwrap();
        pos.apply_drop_core(Role::Knight, Square::E4);
        assert_eq!(
            pos.board().piece_at(Square::E4),
            Some(Piece::new(Color::White, Role::Knight))
        );
        assert_eq!(pos.turn(), Color::Black);
        // A drop is neither a capture nor a pawn move, so the clock advances.
        assert_eq!(pos.halfmove_clock(), 6);
        assert_eq!(pos.fullmove_number(), 9);
        assert_eq!(pos.ep_square(), None);
        assert_eq!(pos.incremental_zobrist(), pos.compute_zobrist());
    }

    #[test]
    fn perft_divide_sums_to_total() {
        let pos = Position::startpos();
        let div = perft_divide(&pos, 3);
        let total: u64 = div.iter().map(|(_, n)| n).sum();
        assert_eq!(total, perft(&pos, 3));
        assert_eq!(div.len(), 20);
    }
}
