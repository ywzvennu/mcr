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

use alloc::borrow::ToOwned;
use alloc::{string::String, vec::Vec};
use core::fmt;
use core::str::FromStr;

use crate::attacks::{
    between, bishop_attacks, king_attacks, knight_attacks, pawn_attacks, rook_attacks,
};
use crate::movelist::MoveList;
use crate::{Bitboard, Board, Color, File, Move, MoveKind, Piece, Rank, Role, Square};

/// Castling rights: which rooks each side may still castle with.
///
/// Each of the four rights is the [`File`] of the rook involved, or `None` if
/// that castling is no longer available. In standard chess the king-side rook
/// is on the h-file and the queen-side rook on the a-file; storing the file
/// rather than a bare boolean leaves room for arbitrary Chess960 rook files.
///
/// The four `Option<File>` are packed into a single `u16`, one 4-bit nibble per
/// right: `0` means the right is absent, `1..=8` encode files `A..=H` (the file
/// index plus one). This is a lossless re-encoding of the previous four
/// `Option<File>` bytes — every file and the absent state still round-trips —
/// but it costs 2 bytes instead of 4, shrinking [`Position`] by exposing more
/// of its loose state to padding reuse. The public API is unchanged: every
/// accessor still speaks in `Option<File>`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CastlingRights {
    /// Four 4-bit nibbles, low to high: white-king, white-queen, black-king,
    /// black-queen. Each nibble is `0` (no right) or `file as u16 + 1`.
    bits: u16,
}

impl fmt::Debug for CastlingRights {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CastlingRights")
            .field(
                "white_king",
                &self.rook_file(Color::White, CastleSide::King),
            )
            .field(
                "white_queen",
                &self.rook_file(Color::White, CastleSide::Queen),
            )
            .field(
                "black_king",
                &self.rook_file(Color::Black, CastleSide::King),
            )
            .field(
                "black_queen",
                &self.rook_file(Color::Black, CastleSide::Queen),
            )
            .finish()
    }
}

/// The nibble shift for a given color/side in [`CastlingRights::bits`].
#[inline]
const fn castling_shift(color: Color, side: CastleSide) -> u32 {
    let idx = match (color, side) {
        (Color::White, CastleSide::King) => 0,
        (Color::White, CastleSide::Queen) => 1,
        (Color::Black, CastleSide::King) => 2,
        (Color::Black, CastleSide::Queen) => 3,
    };
    idx * 4
}

/// Encodes an optional rook file into its 4-bit nibble value (`0` for absent,
/// `file as u16 + 1` otherwise).
#[inline]
const fn encode_castle_nibble(file: Option<File>) -> u16 {
    match file {
        None => 0,
        Some(f) => f as u16 + 1,
    }
}

/// Decodes a 4-bit nibble value back into an optional rook file.
#[inline]
const fn decode_castle_nibble(nibble: u16) -> Option<File> {
    match nibble {
        // 1..=8 -> File::A..=File::H. `0` (no right) and any other value (never
        // produced by the constructors) map to `None`.
        n @ 1..=8 => Some(File::ALL[(n - 1) as usize]),
        _ => None,
    }
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
    pub const NONE: CastlingRights = CastlingRights { bits: 0 };

    /// The standard starting rights: both sides may castle both ways, with rooks
    /// on the a- and h-files.
    pub const STANDARD: CastlingRights =
        CastlingRights::from_rook_files(Some(File::H), Some(File::A), Some(File::H), Some(File::A));

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
        let bits = encode_castle_nibble(white_king)
            | (encode_castle_nibble(white_queen) << 4)
            | (encode_castle_nibble(black_king) << 8)
            | (encode_castle_nibble(black_queen) << 12);
        CastlingRights { bits }
    }

    /// Returns the rook file for `color` castling toward `side`, if that right is
    /// still held.
    #[must_use]
    #[inline]
    pub const fn rook_file(self, color: Color, side: CastleSide) -> Option<File> {
        let nibble = (self.bits >> castling_shift(color, side)) & 0xF;
        decode_castle_nibble(nibble)
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
        let shift = castling_shift(color, side);
        self.bits = (self.bits & !(0xF << shift)) | (encode_castle_nibble(file) << shift);
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

    /// Returns `true` if `color` holds *either* castling right. A cheap bit-test
    /// used by the Chess960 castle generator to skip all castle work — including
    /// the expensive king-danger map — on the dominant midgame node where the
    /// side to move has already forfeited castling.
    #[must_use]
    #[inline]
    pub(crate) const fn has_any(self, color: Color) -> bool {
        // Each color occupies one byte of `bits` (two nibbles): white in the low
        // byte, black in the high byte. A non-zero byte means at least one of
        // that color's rook files is set.
        match color {
            Color::White => self.bits & 0x00FF != 0,
            Color::Black => self.bits & 0xFF00 != 0,
        }
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

/// The pinned friendly pieces of the side to move: which squares are pinned,
/// and the king the pins radiate from.
///
/// A pinned piece is confined to the full line through the king and the pinning
/// slider. The representation is deliberately tiny — a single `pinned` bitboard
/// plus the king square — so it costs nothing to construct or copy on every
/// node (the previous design heap-allocated a `Vec` of per-piece lines). The
/// confining line is recovered on demand as `line(king, piece)` only for the
/// rare piece that is actually pinned, so no per-piece line table is built, and
/// the common unpinned piece is settled by a single bitboard membership test in
/// [`Pins::line_of`].
#[derive(Clone, Copy)]
struct Pins {
    /// The set of pinned friendly squares.
    pinned: Bitboard,
    /// The king the pins radiate from, used to recover a pinned piece's line.
    king_sq: Square,
}

impl Pins {
    /// No pinned pieces — the placeholder for the fully general pseudo-legal
    /// path, which applies no pin restriction. `line_of` short-circuits to
    /// [`Bitboard::FULL`] for every square because `pinned` is empty, so the
    /// king square is never consulted.
    const EMPTY: Pins = Pins {
        pinned: Bitboard::EMPTY,
        king_sq: Square::A1,
    };

    /// Records that `sq` is pinned.
    #[inline]
    fn add(&mut self, sq: Square) {
        self.pinned = self.pinned.with(sq);
    }

    /// The line restricting `sq`, or [`Bitboard::FULL`] if `sq` is unpinned.
    #[inline]
    fn line_of(&self, sq: Square) -> Bitboard {
        // The common case is an unpinned piece: a single bitboard test settles
        // it. When there are no pins at all this is taken for every piece.
        if !self.pinned.contains(sq) {
            return Bitboard::FULL;
        }
        crate::attacks::line(self.king_sq, sq)
    }
}

/// The destination for the generators' emitted moves.
///
/// The standard move-generation routines push each candidate into a sink rather
/// than directly into a [`MoveList`], so the same generator body serves two
/// callers: the materializing path (a [`MoveList`], which records every move),
/// and the *bulk leaf-counting* path used by perft at depth 1 ([`CountSink`],
/// which only tallies how many moves there are). Because a perft leaf needs the
/// *number* of legal moves and never the moves themselves, the counting sink can
/// replace each per-target loop with a single population count, skipping the move
/// construction entirely.
///
/// The two `emit` methods are the only two shapes the generators use. Every
/// implementor must treat them identically in count: `emit_targets(from, t, _)`
/// is exactly `t.count()` single moves, one per set bit of `t`.
pub(crate) trait MoveSink {
    /// Records a single move with explicit `from`, `to`, and `kind`.
    fn emit(&mut self, from: Square, to: Square, kind: MoveKind);

    /// Records one move from `from` to each square in `targets`, tagging each as
    /// a capture when the target square is in `their_pieces` and quiet otherwise.
    ///
    /// The materializing sink iterates the targets and constructs a [`Move`] per
    /// bit; the counting sink replaces the whole loop with two population counts
    /// (captures and quiets), which is the core of the perft bulk-count speedup.
    fn emit_targets(&mut self, from: Square, targets: Bitboard, their_pieces: Bitboard);
}

impl MoveSink for MoveList {
    #[inline]
    fn emit(&mut self, from: Square, to: Square, kind: MoveKind) {
        self.push(Move::new(from, to, kind));
    }

    #[inline]
    fn emit_targets(&mut self, from: Square, targets: Bitboard, their_pieces: Bitboard) {
        for to in targets {
            let kind = if their_pieces.contains(to) {
                MoveKind::Capture
            } else {
                MoveKind::Quiet
            };
            self.push(Move::new(from, to, kind));
        }
    }
}

/// A [`MoveSink`] that only counts the moves it is given, never materializing
/// any of them — the bulk leaf-counting destination for perft at depth 1.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct CountSink {
    count: u64,
}

impl CountSink {
    /// The number of moves recorded so far.
    #[inline]
    fn count(self) -> u64 {
        self.count
    }
}

impl MoveSink for CountSink {
    #[inline]
    fn emit(&mut self, _from: Square, _to: Square, _kind: MoveKind) {
        self.count += 1;
    }

    #[inline]
    fn emit_targets(&mut self, _from: Square, targets: Bitboard, _their_pieces: Bitboard) {
        // The whole per-target loop collapses to a single population count: the
        // capture/quiet split does not change *how many* moves there are.
        self.count += u64::from(targets.count());
    }
}

/// A [`MoveSink`] that forwards into a [`MoveList`] only those moves matching a
/// capture/quiet predicate, the per-stage destination of the staged
/// (lazy) move generator. Running the *same* legal generator
/// ([`Position::generate_into_with_castles`]) through this filter twice — once
/// keeping captures, once keeping non-captures — partitions the legal move set
/// into a captures stage and a quiets stage with no change to which moves are
/// produced. The split is purely an output filter: it never alters generation,
/// so the two stages together reproduce `generate_into(..)` exactly.
///
/// "Capture" here is [`Move::is_capture`] — ordinary captures, en passant, and
/// capturing promotions — which is the same predicate the staged generator uses
/// to order captures before quiets. Non-capturing promotions, double pushes, and
/// castles fall into the quiets stage.
pub(crate) struct FilterSink<'a> {
    out: &'a mut MoveList,
    /// When `true` keep captures (and capturing promotions); when `false` keep
    /// everything else.
    want_captures: bool,
}

impl<'a> FilterSink<'a> {
    /// A sink that keeps only capturing moves.
    #[inline]
    fn captures(out: &'a mut MoveList) -> FilterSink<'a> {
        FilterSink {
            out,
            want_captures: true,
        }
    }

    /// A sink that keeps only non-capturing moves.
    #[inline]
    fn quiets(out: &'a mut MoveList) -> FilterSink<'a> {
        FilterSink {
            out,
            want_captures: false,
        }
    }

    #[inline]
    fn keep(&mut self, mv: Move) {
        if mv.is_capture() == self.want_captures {
            self.out.push(mv);
        }
    }
}

impl MoveSink for FilterSink<'_> {
    #[inline]
    fn emit(&mut self, from: Square, to: Square, kind: MoveKind) {
        self.keep(Move::new(from, to, kind));
    }

    #[inline]
    fn emit_targets(&mut self, from: Square, targets: Bitboard, their_pieces: Bitboard) {
        // A target landing on an enemy piece is a capture, every other target a
        // quiet — the same split `MoveList::emit_targets` records. Pre-masking the
        // target bitboard keeps only the wanted half before the per-bit loop.
        let wanted = if self.want_captures {
            targets & their_pieces
        } else {
            targets & !their_pieces
        };
        for to in wanted {
            let kind = if their_pieces.contains(to) {
                MoveKind::Capture
            } else {
                MoveKind::Quiet
            };
            self.out.push(Move::new(from, to, kind));
        }
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
    /// Packed castling rights (2 bytes; see [`CastlingRights`]).
    castling: CastlingRights,
    /// The square a pawn could move *to* in an en-passant capture, set the move
    /// after a double pawn push.
    ep_square: Option<Square>,
    /// The fullmove number, starting at 1. Stored as `u16`: realistic games stay
    /// well under 1000 and the field saturates at [`u16::MAX`] on FEN parse, so
    /// it never wraps. Widened to `u32` by [`Position::fullmove_number`].
    fullmove_number: u16,
    /// The halfmove clock (plies since the last capture or pawn move). Stored as
    /// `u8`: the draw rules cap interest at 150 (< 255) and the field saturates
    /// at [`u8::MAX`] rather than wrapping, so the 50-/75-move comparisons are
    /// unaffected. Widened to `u32` by [`Position::halfmove_clock`].
    halfmove_clock: u8,
    turn: Color,
}

/// The minimal state needed to reverse a single [`Position::make`], the undo
/// token for zero-copy make/unmake search.
///
/// A search descends the tree by calling [`Position::make`] on each step and
/// pops back up with [`Position::unmake`], threading the [`Undo`] returned by
/// the former into the latter. Unlike [`Position::play`] this copies nothing:
/// `make` mutates the position in place and records only the handful of fields a
/// move cannot reconstruct from the board alone — the captured piece (with its
/// square, which differs from the move's destination for en passant), and the
/// prior castling rights, en-passant target, and move clocks, each of which a
/// move can clear or change irreversibly.
///
/// An `Undo` is opaque and only meaningful when paired with the exact move and
/// position it came from; see [`Position::unmake`] for the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Undo {
    /// The piece removed by the move and the square it stood on, or `None` for a
    /// non-capturing move. The square is the move's destination for an ordinary
    /// capture or capturing promotion, and the en-passant pawn's square (on the
    /// destination's file, the origin's rank) for en passant.
    captured: Option<(Piece, Square)>,
    /// Castling rights before the move (a king or rook move, or a capture of a
    /// rook on its home square, can revoke a right the board cannot recover).
    castling: CastlingRights,
    /// En-passant target before the move (cleared every move, set only by a
    /// double push).
    ep_square: Option<Square>,
    /// Halfmove clock before the move (reset by a capture or pawn move).
    halfmove_clock: u8,
    /// Fullmove number before the move (incremented after a black move).
    fullmove_number: u16,
}

impl Undo {
    /// The piece a capturing move removed, and the square it stood on, or `None`
    /// for a non-capturing move. Used by the variant layer to drive its
    /// capture-side-effect hook from the same record the core unmake reverses.
    #[must_use]
    #[inline]
    pub(crate) fn captured(self) -> Option<(Piece, Square)> {
        self.captured
    }
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
        Position {
            board: Board::standard(),
            turn: Color::White,
            castling: CastlingRights::STANDARD,
            ep_square: None,
            halfmove_clock: 0,
            fullmove_number: 1,
        }
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

    /// The halfmove clock (plies since the last capture or pawn move), used for
    /// the fifty-move rule.
    ///
    /// Stored internally as a `u8` (the draw rules never look past 150) and
    /// widened here, so the public type is unchanged. A pathological FEN with a
    /// larger clock is saturated to 255 on parse, which leaves every 50-/75-move
    /// comparison identical.
    #[must_use]
    #[inline]
    pub const fn halfmove_clock(&self) -> u32 {
        self.halfmove_clock as u32
    }

    /// The fullmove number, starting at 1 and incremented after each black move.
    ///
    /// Stored internally as a `u16` (saturating at 65535 on parse) and widened
    /// here, so the public type is unchanged.
    #[must_use]
    #[inline]
    pub const fn fullmove_number(&self) -> u32 {
        self.fullmove_number as u32
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

    /// Returns the absolutely-pinned pieces of `color`: friendly pieces standing
    /// on a line between their own king and an enemy slider, with no other piece
    /// between, so that moving them off that line would expose the king to check.
    ///
    /// A pinned piece may legally move only along the line through its king and
    /// the pinning slider (and only as far as capturing that pinner). If `color`
    /// has no king, the result is empty: with no king there is nothing to pin to.
    ///
    /// This is the standard *absolute* pin (pinned to the king); it does not
    /// report relative pins to other pieces.
    #[must_use]
    pub fn pinned(&self, color: Color) -> Bitboard {
        match self.board.king_of(color) {
            Some(king) => {
                let occupied = self.board.occupied();
                self.compute_pins(king, color, color.opposite(), occupied)
                    .pinned
            }
            None => Bitboard::EMPTY,
        }
    }

    /// Returns the pseudo-attacks of the piece currently standing on `sq`, given
    /// the current board occupancy, or the empty set if `sq` is empty.
    ///
    /// "Pseudo-attacks" are the squares the piece attacks by its movement
    /// geometry under the present occupancy: sliders are blocked by intervening
    /// pieces (the first blocker on each ray is included), and pawns attack only
    /// their two forward diagonals. The result is *not* masked by friendly
    /// occupancy — a square defended by a friendly piece is still reported as
    /// attacked — and it ignores pins and check, so it is a raw attack set rather
    /// than a legal-move set. A pawn on its last rank attacks nothing.
    #[must_use]
    pub fn attacks_from(&self, sq: Square) -> Bitboard {
        let piece = match self.board.piece_at(sq) {
            Some(p) => p,
            None => return Bitboard::EMPTY,
        };
        let occupied = self.board.occupied();
        match piece.role {
            Role::Pawn => pawn_attacks(piece.color, sq),
            Role::Knight => knight_attacks(sq),
            Role::King => king_attacks(sq),
            Role::Bishop => bishop_attacks(sq, occupied),
            Role::Rook => rook_attacks(sq, occupied),
            Role::Queen => bishop_attacks(sq, occupied) | rook_attacks(sq, occupied),
        }
    }

    /// Static Exchange Evaluation (SEE) of the capture sequence on `mv`'s
    /// destination square, in centipawns from the moving side's point of view.
    ///
    /// SEE answers "if I initiate a capture on `to` and both sides keep
    /// recapturing on that square with their least valuable available attacker,
    /// what is the net material result?" It uses the standard swap-off algorithm:
    /// the initiating piece captures the occupant of `to`, then the two sides
    /// alternately bring on their cheapest remaining attacker of `to`,
    /// recomputing the attacker set as each piece is removed — including x-ray
    /// attackers (a rook or queen behind the first slider) that are *uncovered*
    /// when a piece in front of them leaves the line. A standard min-max back-pass
    /// over the gain list yields the value, since either side will decline a
    /// continuation that loses material.
    ///
    /// # Value scale
    ///
    /// Material is scored on the conventional centipawn scale (a pawn = 100):
    ///
    /// | role   | value |
    /// |--------|-------|
    /// | pawn   | 100   |
    /// | knight | 320   |
    /// | bishop | 330   |
    /// | rook   | 500   |
    /// | queen  | 900   |
    /// | king   | 10000 |
    ///
    /// The king's value is a large sentinel: a king may recapture, but no side
    /// will ever allow its king to be captured in return, so the sentinel simply
    /// ensures a king recapture is never "answered". A positive result means the
    /// initiating side comes out ahead by that many centipawns, a negative one
    /// means it loses material, and zero is an even trade.
    ///
    /// # Non-captures
    ///
    /// For a move that does not capture, the destination has no occupant to win,
    /// so the score of *initiating* an exchange there is the value the moving
    /// piece stands to lose if the square is defended. This returns `0` when the
    /// destination is undefended (the piece sits safely) and the negative of the
    /// moving piece's value when it is defended (the piece can be taken for
    /// nothing in return at the head of the swap-off). Promotions are scored on
    /// the pawn that initiates; the promotion gain itself is not modeled.
    ///
    /// En-passant captures are scored as winning a pawn on the en-passant target.
    #[must_use]
    pub fn see(&self, mv: &Move) -> i32 {
        let to = mv.to();
        let from = mv.from();
        let us = self.turn;
        let them = us.opposite();
        let board = &self.board;

        // The piece initiating the exchange and its value (what is risked once it
        // sits on `to`). A move with no piece on `from` is meaningless for SEE.
        let mover = match board.role_at(from) {
            Some(role) => role,
            None => return 0,
        };

        // Material standing on `to` to be captured first. For en passant the
        // captured pawn is not on `to` but on the mover's rank; either way the
        // first gain is a pawn.
        let captured_value = if mv.kind() == MoveKind::EnPassant {
            see_value(Role::Pawn)
        } else {
            match board.role_at(to) {
                Some(role) => see_value(role),
                None => 0,
            }
        };

        // Occupancy with the initiating piece lifted off `from` (and, for en
        // passant, the captured pawn removed from its square). The captured piece
        // on `to`, if any, is simply overwritten by the mover and so need not be
        // cleared from `occ`: it is no longer an attacker once `to` is recomputed
        // from the surviving sets below.
        let mut occ = board.occupied().without(from);
        if mv.kind() == MoveKind::EnPassant {
            // The en-passant victim sits on `to`'s file, `from`'s rank.
            let victim = Square::from_file_rank(to.file(), from.rank());
            occ = occ.without(victim);
        }

        // The swap-off gain list. `gain[0]` is the value just won by the first
        // capture; each later entry is the value the side to move at that ply
        // stands to win if it recaptures.
        let mut gain = [0i32; 32];
        gain[0] = captured_value;
        let mut depth = 0usize;

        // The piece now standing on `to` (whose value is at stake for the *next*
        // recapture) and the side about to recapture.
        let mut on_square = mover;
        let mut side = them;

        // Attackers of `to` from both sides, recomputed lazily as pieces leave
        // `occ`. We seed it once and then subtract the chosen attacker each ply,
        // re-adding any slider x-rayed in behind it.
        let mut attackers = self.see_attackers(to, occ);

        loop {
            // Restrict to the side about to move and stop if it has none.
            let side_attackers = attackers & board.by_color(side) & occ;
            let attacker_sq = match see_least_valuable(board, side_attackers) {
                Some(sq) => sq,
                None => break,
            };

            depth += 1;
            // The value won at this ply is the piece currently on `to`, minus the
            // best the opponent could already secure (filled in by the back-pass).
            gain[depth] = see_value(on_square) - gain[depth - 1];

            // Pruning: if even by winning `on_square` this side cannot reach a
            // non-losing position, it will simply decline — but the standard
            // formulation defers that to the back-pass, so we keep collecting.

            // The recapturing piece moves onto `to`; it becomes the new stake.
            on_square = board
                .role_at(attacker_sq)
                .expect("attacker square is occupied");
            occ = occ.without(attacker_sq);
            attackers.clear(attacker_sq);

            // Uncover x-ray sliders behind the piece that just left: any rook- or
            // bishop-like slider now seeing `to` through the freed line is added.
            // Pieces already used were cleared from `attackers` and removed from
            // `occ`, so re-OR'ing the slider set adds only the newly uncovered
            // ones; the `& occ` keeps spent attackers from creeping back in.
            attackers |= self.see_xray_attackers(to, occ) & occ;

            side = side.opposite();

            if depth + 1 >= gain.len() {
                break;
            }
        }

        // Min-max back-pass: each side, in reverse, takes the better of standing
        // pat at the previous gain or forcing the capture chain to continue.
        while depth > 0 {
            gain[depth - 1] = -i32::max(-gain[depth - 1], gain[depth]);
            depth -= 1;
        }
        gain[0]
    }

    /// The combined attacker set of *both* colors bearing on `sq` under `occ`,
    /// the seed of the [`Position::see`] swap-off. Reuses [`Position::attackers_to`]
    /// for each side.
    fn see_attackers(&self, sq: Square, occ: Bitboard) -> Bitboard {
        self.attackers_to(sq, Color::White, occ) | self.attackers_to(sq, Color::Black, occ)
    }

    /// The slider attackers (rook-, bishop-, and queen-like) of `sq` under `occ`,
    /// of either color — used to re-add x-ray attackers uncovered as front pieces
    /// leave the line during the [`Position::see`] swap-off. Steppers (pawn,
    /// knight, king) are never x-rayed, so only sliders are recomputed.
    fn see_xray_attackers(&self, sq: Square, occ: Bitboard) -> Bitboard {
        let b = &self.board;
        let bishop_like = b.by_role(Role::Bishop) | b.by_role(Role::Queen);
        let rook_like = b.by_role(Role::Rook) | b.by_role(Role::Queen);
        (bishop_attacks(sq, occ) & bishop_like) | (rook_attacks(sq, occ) & rook_like)
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
    ///
    /// Counts via the bulk leaf-counting [`CountSink`] (population counts over
    /// the target masks) rather than materializing every move, so it is both
    /// allocation-free and cheaper than building and measuring a [`MoveList`].
    #[must_use]
    pub fn legal_move_count(&self) -> usize {
        self.count_legal() as usize
    }

    /// Tallies the legal moves of the side to move through the bulk-counting
    /// [`CountSink`], the standard chess perft leaf count (`perft(pos, 1)`). This
    /// drives the *same* legal generator as [`Position::generate_into`], so the
    /// count is exactly `generate_into(..).len()`, only without writing any move
    /// into a buffer.
    pub(crate) fn count_legal(&self) -> u64 {
        let mut sink = CountSink::default();
        self.generate_into_with_castles(&mut sink, true);
        sink.count()
    }

    /// Appends only the legal *capturing* moves (ordinary captures, en passant,
    /// and capturing promotions) of the side to move into `out`, the captures
    /// stage of the staged generator. Runs the fast legal generator through a
    /// capture-only [`FilterSink`], so the produced moves are exactly the
    /// capturing subset of [`Position::generate_into`] — same legality, same
    /// pins/check handling, nothing extra.
    pub(crate) fn legal_captures_into(&self, out: &mut MoveList) {
        let mut sink = FilterSink::captures(out);
        self.generate_into_with_castles(&mut sink, true);
    }

    /// Appends only the legal *non-capturing* moves (quiets, double pushes,
    /// non-capturing promotions, and castles) of the side to move into `out`, the
    /// quiets stage of the staged generator. The complement of
    /// [`Position::legal_captures_into`]: together they reproduce
    /// [`Position::generate_into`] exactly.
    pub(crate) fn legal_quiets_into(&self, out: &mut MoveList) {
        let mut sink = FilterSink::quiets(out);
        self.generate_into_with_castles(&mut sink, true);
    }

    /// The static victim value used to order this position's captures
    /// (most-valuable-victim first). For an ordinary capture or capturing
    /// promotion it is the value of the piece standing on the destination; for en
    /// passant it is a pawn. A non-capturing move scores `0`. This is the cheap
    /// MVV key the staged captures stage sorts by; richer SEE/MVV-LVA ordering is
    /// available via [`Position::see`] and noted as a refinement.
    #[must_use]
    pub(crate) fn victim_value(&self, mv: &Move) -> i32 {
        if mv.kind() == MoveKind::EnPassant {
            return see_value(Role::Pawn);
        }
        match self.board.role_at(mv.to()) {
            Some(role) => see_value(role),
            None => 0,
        }
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
        self.pseudo_into_with(out, standard_castles, false, false);
    }

    /// Pushes the pseudo-legal moves of the side to move into `out`, exactly like
    /// [`Position::pseudo_into`], but additionally treating white's first-rank
    /// pawns as double-push eligible for the horde variant. Standard chess and
    /// every other variant call the wrappers above with the flag `false`, leaving
    /// their move sets identical.
    pub(crate) fn pseudo_into_horde(&self, out: &mut MoveList) {
        self.pseudo_into_with(out, true, true, false);
    }

    /// Pushes the pseudo-legal moves of the side to move into `out`, exactly like
    /// [`Position::pseudo_into`], but emitting king steps for *every* king of the
    /// side to move rather than just one. Antichess allows promotion to a king, so
    /// a side may have several kings and each must generate its moves; standard
    /// chess (exactly one king) keeps the faster single-king [`Position::pseudo_into`].
    /// Castling is never emitted here (antichess has no castling).
    pub(crate) fn pseudo_into_all_kings(&self, out: &mut MoveList) {
        self.pseudo_into_with(out, false, false, true);
    }

    /// Shared body of the pseudo-legal generators. `standard_castles` controls
    /// whether the standard castle generator runs; `white_first_rank_double`
    /// admits horde's first-rank white double-pushes.
    fn pseudo_into_with(
        &self,
        out: &mut MoveList,
        standard_castles: bool,
        white_first_rank_double: bool,
        all_kings: bool,
    ) {
        let us = self.turn;
        let them = us.opposite();
        let board = &self.board;
        let occupied = board.occupied();
        let our_pieces = board.by_color(us);
        let their_pieces = board.by_color(them);
        let full = Bitboard::FULL;
        let no_pins = Pins::EMPTY;

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
        } else if white_first_rank_double && us == Color::White {
            // Horde's kingless white side: no king, no pins, all-allowing check
            // mask. Generate every white pawn move in bulk with bitboard shifts
            // rather than one pin-line lookup + per-pawn loop iteration each. This
            // is the horde perft hot path (white has thirty-six pawns); the
            // per-pawn `gen_pawn_moves` below stays the source of truth for every
            // other caller.
            self.gen_horde_white_pawn_moves(out, occupied, their_pieces);
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
        // caller's filter). In standard chess a side has exactly one king, so the
        // single `king_of` square suffices and stays the fast path. Antichess can
        // have several kings (pawns may promote to a king), so `all_kings` makes
        // every king of the side emit its steps by iterating the king bitboard.
        if all_kings {
            for king_sq in board.pieces(us, Role::King) {
                let king_targets = king_attacks(king_sq) & !our_pieces;
                for to in king_targets {
                    let kind = if their_pieces.contains(to) {
                        MoveKind::Capture
                    } else {
                        MoveKind::Quiet
                    };
                    out.push(Move::new(king_sq, to, kind));
                }
            }
            // No castling: `all_kings` is only used by antichess, which has no
            // castling, and "the king" for castling is ambiguous with several
            // kings anyway. (`standard_castles` is already `false` for this path.)
        } else if let Some(king_sq) = board.king_of(us) {
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

    /// Pushes all legal moves into `out`.
    pub(crate) fn generate_into(&self, out: &mut MoveList) {
        self.generate_into_with_castles(out, true);
    }

    /// Pushes all legal moves into `out` *except* castling, for variant layers
    /// that run on the fast pin/check-mask path but supply their own
    /// arbitrary-geometry castle generator (Chess960). Every non-castling move
    /// is already fully legal, so the variant appends its self-validating castles
    /// without any make-move filter.
    pub(crate) fn generate_no_castles_into(&self, out: &mut MoveList) {
        self.generate_into_with_castles(out, false);
    }

    /// Shared body of the fast legal generator; `standard_castles` controls
    /// whether the standard castle generator runs at the end. Generic over the
    /// [`MoveSink`] so the same generation drives both the materializing path (a
    /// [`MoveList`]) and the bulk leaf-counting path ([`CountSink`]).
    fn generate_into_with_castles<S: MoveSink>(&self, out: &mut S, standard_castles: bool) {
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
        out.emit_targets(king_sq, king_targets, their_pieces);

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
        let pins = self.compute_pins(king_sq, us, them, occupied);

        self.gen_pawn_moves(
            out,
            us,
            occupied,
            their_pieces,
            check_mask,
            &pins,
            king_sq,
            false,
            true,
        );
        self.gen_knight_moves(out, us, our_pieces, their_pieces, check_mask, &pins);
        self.gen_slider_moves(
            out,
            us,
            occupied,
            our_pieces,
            their_pieces,
            check_mask,
            &pins,
        );

        // Castling is only possible when not in check. A variant that supplies
        // its own arbitrary-geometry castles (Chess960) passes `false` here and
        // appends them afterward.
        if standard_castles && num_checkers == 0 {
            self.gen_castles(out, us, occupied, king_danger, king_sq);
        }
    }

    /// Fills `out` with every *non-capturing* legal move for the side to move
    /// under **atomic** king-safety, using the fast pin/check-mask path.
    ///
    /// A non-capturing atomic move triggers no explosion, so it has ordinary
    /// chess legality with a single twist: the enemy king never gives check (a
    /// king that captures detonates itself, so it can threaten nothing), and the
    /// two kings may stand adjacent. This routine therefore reproduces the fast
    /// standard generator but **excludes the enemy king** from both the checker
    /// set and the king-danger set, and emits only quiet moves and castles
    /// (captures are handled separately by the explosion-aware filter). The enemy
    /// king is still part of the occupancy, so it continues to block slider rays.
    ///
    /// The emitted set is exactly the non-capturing moves `mv` for which
    /// `attackers_to(my_king, them, occ_after) \ {enemy_king}` is empty after
    /// `mv` — i.e. precisely the non-capturing moves that atomic's make-move
    /// legality test accepts, so substituting this for that test on the
    /// non-capturing moves leaves the legal-move set unchanged.
    pub(crate) fn atomic_noncapture_legal_into(&self, out: &mut MoveList) {
        let us = self.turn;
        let them = us.opposite();
        let board = &self.board;
        let occupied = board.occupied();

        let king_sq = match board.king_of(us) {
            Some(sq) => sq,
            None => return,
        };

        // The enemy king is not a real attacker in atomic (it cannot execute a
        // capture without exploding itself), so exclude it from the checker and
        // king-danger sets. It remains in `occupied` and so still blocks rays.
        //
        // The atomic immunity goes further: our king can never be captured while
        // it stands **adjacent to the enemy king**, because any enemy capture
        // there detonates a blast that also catches the enemy's own king — an
        // illegal self-explosion the enemy can never play. So every square
        // adjacent to the enemy king is immune to *all* enemy attackers and is a
        // safe destination for our king. The set of such squares,
        // `king_attacks(enemy_king)`, is removed from the danger set, and if our
        // king already stands on one of them it is not in check and is unpinning.
        let enemy_king = board.king_of(them);
        let king_immunity = enemy_king.map_or(Bitboard::EMPTY, king_attacks);
        let king_adjacent_to_enemy = king_immunity.contains(king_sq);

        let mut checkers = self.attackers_to(king_sq, them, occupied);
        if let Some(ek) = enemy_king {
            checkers.clear(ek);
        }
        // While adjacent to the enemy king our king is immune, so it is never in
        // check and no piece is pinned to it: clear the checker set entirely.
        if king_adjacent_to_enemy {
            checkers = Bitboard::EMPTY;
        }
        let num_checkers = checkers.count();

        // Squares the king may not step onto: those attacked by a non-king enemy
        // piece with our king removed from the occupancy (so it cannot shield
        // itself), minus the enemy-king-adjacency immunity zone. The enemy king's
        // own attacks are also omitted, so our king may walk adjacent to it.
        let occ_without_king = occupied.without(king_sq);
        let king_danger = self.attacked_by_nonking(them, occ_without_king) & !king_immunity;

        // King moves: only to empty squares (captures detonate and are handled
        // elsewhere) that are not in the non-king danger set.
        let king_targets = king_attacks(king_sq) & !occupied & !king_danger;
        out.emit_targets(king_sq, king_targets, Bitboard::EMPTY);

        if num_checkers >= 2 {
            // Double check from two non-king pieces: only king moves are legal.
            return;
        }

        let check_mask = if num_checkers == 1 {
            let checker = checkers.lsb().expect("one checker");
            checkers | between(king_sq, checker)
        } else {
            Bitboard::FULL
        };

        // While the king is immune (adjacent to the enemy king) it cannot be
        // exposed by any move, so no piece is pinned to it.
        let pins = if king_adjacent_to_enemy {
            Pins::EMPTY
        } else {
            self.compute_pins(king_sq, us, them, occupied)
        };

        // Pawn pushes only (no captures, no en passant — those detonate).
        self.gen_pawn_pushes(out, us, occupied, check_mask, &pins);

        // Knight and slider quiet moves: restrict targets to empty squares so no
        // capture is ever emitted from this path.
        let empty = !occupied;
        for from in board.pieces(us, Role::Knight) {
            let pin_line = pins.line_of(from);
            let targets = knight_attacks(from) & empty & check_mask & pin_line;
            out.emit_targets(from, targets, Bitboard::EMPTY);
        }
        // Slider quiet moves. The role-split shape (see `gen_slider_moves`) wins
        // with the cheap magic lookup and is in the noise with the default
        // hyperbola lookup, so it is gated the same way; both emit byte-identical
        // sets.
        #[cfg(feature = "magic")]
        {
            let allowed = empty & check_mask;
            for from in board.pieces(us, Role::Bishop) {
                let pin_line = pins.line_of(from);
                let targets = bishop_attacks(from, occupied) & allowed & pin_line;
                out.emit_targets(from, targets, Bitboard::EMPTY);
            }
            for from in board.pieces(us, Role::Rook) {
                let pin_line = pins.line_of(from);
                let targets = rook_attacks(from, occupied) & allowed & pin_line;
                out.emit_targets(from, targets, Bitboard::EMPTY);
            }
            for from in board.pieces(us, Role::Queen) {
                let pin_line = pins.line_of(from);
                let attacks = bishop_attacks(from, occupied) | rook_attacks(from, occupied);
                let targets = attacks & allowed & pin_line;
                out.emit_targets(from, targets, Bitboard::EMPTY);
            }
        }
        #[cfg(not(feature = "magic"))]
        for (role, diagonal, straight) in [
            (Role::Bishop, true, false),
            (Role::Rook, false, true),
            (Role::Queen, true, true),
        ] {
            for from in board.pieces(us, role) {
                let pin_line = pins.line_of(from);
                let mut attacks = Bitboard::EMPTY;
                if diagonal {
                    attacks |= bishop_attacks(from, occupied);
                }
                if straight {
                    attacks |= rook_attacks(from, occupied);
                }
                let targets = attacks & empty & check_mask & pin_line;
                out.emit_targets(from, targets, Bitboard::EMPTY);
            }
        }

        // Castling (a non-capturing king move) when not in check by a non-king
        // piece, with the king-walk safety judged against the non-king danger.
        if num_checkers == 0 {
            self.gen_castles(out, us, occupied, king_danger, king_sq);
        }
    }

    /// Emits the side-to-move's pawn single and double *advances* (no captures,
    /// no en passant), subject to the check mask and pins — the non-capturing
    /// pawn moves the atomic fast path needs.
    fn gen_pawn_pushes(
        &self,
        out: &mut MoveList,
        us: Color,
        occupied: Bitboard,
        check_mask: Bitboard,
        pins: &Pins,
    ) {
        let board = &self.board;
        let promo_rank = match us {
            Color::White => Rank::Eighth,
            Color::Black => Rank::First,
        };
        let start_rank = match us {
            Color::White => Rank::Second,
            Color::Black => Rank::Seventh,
        };
        let forward: i8 = if us.is_white() { 1 } else { -1 };

        for from in board.pieces(us, Role::Pawn) {
            let pin_line = pins.line_of(from);
            if let Some(one) = from.offset(0, forward) {
                if !occupied.contains(one) {
                    self.push_pawn_advance(out, from, one, promo_rank, check_mask, pin_line);
                    if from.rank() == start_rank {
                        if let Some(two) = from.offset(0, 2 * forward) {
                            if !occupied.contains(two)
                                && check_mask.contains(two)
                                && pin_line.contains(two)
                            {
                                out.emit(from, two, MoveKind::DoublePawnPush);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Like [`Position::attacked_by`] but excluding color `by`'s king attacks —
    /// the danger set under atomic non-capture rules, where the enemy king
    /// threatens nothing (it cannot capture without detonating itself).
    fn attacked_by_nonking(&self, by: Color, occupied: Bitboard) -> Bitboard {
        let b = &self.board;

        // Same bulk shape as [`Position::attacked_by`] (pawns via shifts, sliders
        // folded by ray family), but without the king's own attacks.
        let pawns = b.pieces(by, Role::Pawn);
        let mut attacked = match by {
            Color::White => pawns.north_east() | pawns.north_west(),
            Color::Black => pawns.south_east() | pawns.south_west(),
        };

        for from in b.pieces(by, Role::Knight) {
            attacked |= knight_attacks(from);
        }

        let bishop_like = b.pieces(by, Role::Bishop) | b.pieces(by, Role::Queen);
        for from in bishop_like {
            attacked |= bishop_attacks(from, occupied);
        }
        let rook_like = b.pieces(by, Role::Rook) | b.pieces(by, Role::Queen);
        for from in rook_like {
            attacked |= rook_attacks(from, occupied);
        }

        attacked
    }

    /// Returns the set of squares attacked by color `by` under `occupied`,
    /// using pawn-attack patterns for pawns (the squares a king of the other
    /// color may not move to).
    fn attacked_by(&self, by: Color, occupied: Bitboard) -> Bitboard {
        let b = &self.board;

        // Pawn attacks in bulk: every pawn's two capture squares come from a
        // single pair of bitboard shifts of the whole pawn set, rather than a
        // per-pawn table lookup. (Profiling round 2: this is the dominant share
        // of the king-danger map, and the shift form measured faster in both the
        // magic and hyperbola builds.)
        let pawns = b.pieces(by, Role::Pawn);
        let mut attacked = match by {
            Color::White => pawns.north_east() | pawns.north_west(),
            Color::Black => pawns.south_east() | pawns.south_west(),
        };

        for from in b.pieces(by, Role::Knight) {
            attacked |= knight_attacks(from);
        }

        // Sliders by ray family rather than by piece role: a queen contributes to
        // both the diagonal and the orthogonal pass, so folding the queens into
        // the bishop and rook sets issues each slider its rays directly with no
        // separate per-queen branch.
        let bishop_like = b.pieces(by, Role::Bishop) | b.pieces(by, Role::Queen);
        for from in bishop_like {
            attacked |= bishop_attacks(from, occupied);
        }
        let rook_like = b.pieces(by, Role::Rook) | b.pieces(by, Role::Queen);
        for from in rook_like {
            attacked |= rook_attacks(from, occupied);
        }

        if let Some(king) = b.king_of(by) {
            attacked |= king_attacks(king);
        }
        attacked
    }

    /// The pinned friendly pieces of the side to move, as an inline [`Pins`]
    /// (no heap allocation per node). Each pinned piece's confining line is
    /// recovered on demand from the king square, so only the pinned *set* is
    /// recorded here.
    fn compute_pins(&self, king_sq: Square, us: Color, them: Color, occupied: Bitboard) -> Pins {
        let b = &self.board;
        let mut pins = Pins {
            pinned: Bitboard::EMPTY,
            king_sq,
        };
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
                    pins.add(pinned);
                }
            }
        }
        pins
    }

    /// Generates every pseudo-legal white pawn move for the kingless horde white
    /// side, in bulk, with bitboard shifts.
    ///
    /// This is the horde perft hot path: white is a thirty-six-pawn horde with no
    /// king, so there are no pins and no king-safety filter — every pseudo-legal
    /// white pawn move is already legal. Rather than loop over each pawn (one pin
    /// lookup, one offset computation, one attack lookup apiece), this computes
    /// each *class* of move for all pawns at once via a single shift and mask:
    /// single pushes, the two double-push source ranks (rank 2 with an
    /// en-passant target, and horde's rank 1 without one), the two capture
    /// directions, and en passant. Promotions on the eighth rank are expanded per
    /// target. The reconstructed `from` square is the target shifted back by the
    /// fixed delta of its class, which is always on the board because the target
    /// was produced by shifting a real pawn forward.
    ///
    /// Only horde's kingless white side reaches this; [`Position::gen_pawn_moves`]
    /// remains the source of truth for every other caller and every other side.
    fn gen_horde_white_pawn_moves<S: MoveSink>(
        &self,
        out: &mut S,
        occupied: Bitboard,
        their_pieces: Bitboard,
    ) {
        let pawns = self.board.pieces(Color::White, Role::Pawn);
        if pawns.is_empty() {
            return;
        }
        let empty = !occupied;

        // Single pushes (and the eighth-rank promotions they reach).
        let single = pawns.north() & empty;
        for to in single & !Bitboard::RANK_8 {
            let from = Square::new(to.index() - 8);
            out.emit(from, to, MoveKind::Quiet);
        }
        for to in single & Bitboard::RANK_8 {
            let from = Square::new(to.index() - 8);
            for role in PROMOTION_ROLES {
                out.emit(
                    from,
                    to,
                    MoveKind::Promotion {
                        role,
                        capture: false,
                    },
                );
            }
        }

        // Double pushes from the second rank create an en-passant target.
        let rank2_step = (pawns & Bitboard::RANK_2).north() & empty;
        for to in rank2_step.north() & empty {
            let from = Square::new(to.index() - 16);
            out.emit(from, to, MoveKind::DoublePawnPush);
        }

        // Horde's first-rank pawns may double-push too, but per the horde
        // convention this sets no en-passant target — a plain quiet two-square
        // advance over an empty intermediate square.
        let rank1_step = (pawns & Bitboard::RANK_1).north() & empty;
        for to in rank1_step.north() & empty {
            let from = Square::new(to.index() - 16);
            out.emit(from, to, MoveKind::Quiet);
        }

        // Captures, north-east then north-west; eighth-rank targets are capturing
        // promotions.
        let cap_ne = pawns.north_east() & their_pieces;
        self.emit_horde_white_captures(out, cap_ne, 9);
        let cap_nw = pawns.north_west() & their_pieces;
        self.emit_horde_white_captures(out, cap_nw, 7);

        // En passant: white is kingless, so the discovered-check ep pin can never
        // apply — any white pawn that attacks the en-passant square may take.
        if let Some(ep) = self.ep_square {
            let takers = pawn_attacks(Color::Black, ep) & pawns;
            for from in takers {
                out.emit(from, ep, MoveKind::EnPassant);
            }
        }
    }

    /// Emits the white horde capture moves whose targets are `targets`, each
    /// reached from the square `delta` indices lower (the fixed back-shift of the
    /// capture direction). Eighth-rank targets expand to the four capturing
    /// promotions.
    #[inline]
    fn emit_horde_white_captures<S: MoveSink>(&self, out: &mut S, targets: Bitboard, delta: u8) {
        for to in targets & !Bitboard::RANK_8 {
            let from = Square::new(to.index() - delta);
            out.emit(from, to, MoveKind::Capture);
        }
        for to in targets & Bitboard::RANK_8 {
            let from = Square::new(to.index() - delta);
            for role in PROMOTION_ROLES {
                out.emit(
                    from,
                    to,
                    MoveKind::Promotion {
                        role,
                        capture: true,
                    },
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn gen_pawn_moves<S: MoveSink>(
        &self,
        out: &mut S,
        us: Color,
        occupied: Bitboard,
        their_pieces: Bitboard,
        check_mask: Bitboard,
        pins: &Pins,
        king_sq: Square,
        white_first_rank_double: bool,
        filter_ep_pin: bool,
    ) {
        let board = &self.board;
        let pawns = board.pieces(us, Role::Pawn);
        if pawns.is_empty() {
            return;
        }
        let promo_rank = match us {
            Color::White => Rank::Eighth,
            Color::Black => Rank::First,
        };
        let start_rank = match us {
            Color::White => Rank::Second,
            Color::Black => Rank::Seventh,
        };
        let forward: i8 = if us.is_white() { 1 } else { -1 };

        // A pinned pawn is confined to the line through the king and its pinner;
        // a pin ray is rare, so the overwhelming majority of pawns are unpinned.
        // The unpinned set is generated in bulk via bitboard shifts (one shift +
        // mask per move class, reconstructing each `from` by the fixed back-shift
        // of its class), and the few pinned pawns are handled individually along
        // their pin line by the original per-pawn logic.
        let unpinned = pawns & !pins.pinned;
        let pinned = pawns & pins.pinned;
        let empty = !occupied;
        let promo_mask = if us.is_white() {
            Bitboard::RANK_8
        } else {
            Bitboard::RANK_1
        };
        // The rank a single push lands on for a double-push-eligible (start-rank)
        // pawn — the intermediate square of its potential double push.
        let double_step_rank = if us.is_white() {
            Bitboard::RANK_3
        } else {
            Bitboard::RANK_6
        };

        // The forward, north-east-equivalent, and north-west-equivalent shifts of
        // the unpinned set, plus the back-shift deltas that recover each target's
        // source square. White advances by +8 (north); black by -8 (south). The
        // "left"/"right" capture directions are named relative to the absolute
        // board, not the moving side, so the deltas match the shift used.
        let (forward_set, cap_a, cap_b, fwd_delta, cap_a_delta, cap_b_delta) = if us.is_white() {
            (
                unpinned.north(),
                unpinned.north_east(),
                unpinned.north_west(),
                8u8,
                9u8,
                7u8,
            )
        } else {
            (
                unpinned.south(),
                unpinned.south_east(),
                unpinned.south_west(),
                8u8,
                7u8,
                9u8,
            )
        };

        // Single pushes: forward onto an empty square, kept by the check mask.
        let single = forward_set & empty;
        Self::emit_pawn_pushes(out, single & check_mask, fwd_delta, promo_mask, us);

        // Double pushes: from the start rank, both the intermediate and the
        // target square empty. `single` (computed before the check mask) already
        // required the intermediate square empty, so a start-rank pawn's valid
        // first step lands on `double_step_rank`; advancing it once more onto an
        // empty square (kept by the check mask) gives the double-push targets.
        let start_step = single & double_step_rank;
        let double = if us.is_white() {
            start_step.north()
        } else {
            start_step.south()
        } & empty
            & check_mask;
        for to in double {
            let from = back_shift(to, fwd_delta + 8, us);
            out.emit(from, to, MoveKind::DoublePawnPush);
        }

        // Horde's first-rank white pawns may also advance two squares, but per the
        // horde convention this sets no en-passant target — a plain quiet
        // two-square move. The `white_first_rank_double` flag is false for every
        // standard caller, so this whole branch is skipped there.
        if white_first_rank_double && us == Color::White {
            let rank1_step = single & Bitboard::RANK_2;
            for to in rank1_step.north() & empty & check_mask {
                let from = back_shift(to, fwd_delta + 8, us);
                out.emit(from, to, MoveKind::Quiet);
            }
        }

        // Captures (including capturing promotions), one direction at a time.
        let caps_a = cap_a & their_pieces & check_mask;
        Self::emit_pawn_captures(out, caps_a, cap_a_delta, promo_mask, us);
        let caps_b = cap_b & their_pieces & check_mask;
        Self::emit_pawn_captures(out, caps_b, cap_b_delta, promo_mask, us);

        // The few pinned pawns, along their pin line, with the original per-pawn
        // logic. This loop is empty in the common (no-pin) node.
        for from in pinned {
            let pin_line = pins.line_of(from);

            if let Some(one) = from.offset(0, forward) {
                if !occupied.contains(one) {
                    self.push_pawn_advance(out, from, one, promo_rank, check_mask, pin_line);
                    if from.rank() == start_rank {
                        if let Some(two) = from.offset(0, 2 * forward) {
                            if !occupied.contains(two)
                                && check_mask.contains(two)
                                && pin_line.contains(two)
                            {
                                out.emit(from, two, MoveKind::DoublePawnPush);
                            }
                        }
                    }
                }
            }

            let caps = pawn_attacks(us, from) & their_pieces;
            for to in caps {
                if !check_mask.contains(to) || !pin_line.contains(to) {
                    continue;
                }
                if to.rank() == promo_rank {
                    for role in PROMOTION_ROLES {
                        out.emit(
                            from,
                            to,
                            MoveKind::Promotion {
                                role,
                                capture: true,
                            },
                        );
                    }
                } else {
                    out.emit(from, to, MoveKind::Capture);
                }
            }
        }

        // En passant: rare, and the discovered-check ep-pin legality depends on
        // the *specific* capturing pawn, so it is handled per taker (pinned or
        // not) with the original logic, preserving `filter_ep_pin` exactly.
        if let Some(ep) = self.ep_square {
            let takers = pawn_attacks(us.opposite(), ep) & pawns;
            for from in takers {
                let pin_line = pins.line_of(from);
                // The captured pawn sits on the ep square's file, on `from`'s rank.
                let captured = Square::from_file_rank(ep.file(), from.rank());
                // En passant resolves check only if it captures the checking pawn
                // or blocks on the ep target.
                let resolves_check = check_mask.contains(ep) || check_mask.contains(captured);
                // The standard discovered-check ep-pin filter is a king-safety
                // concern; a variant on the make-move filter path (e.g. atomic,
                // whose explosion may remove the would-be pinning slider) passes
                // `filter_ep_pin = false` and re-validates the move itself.
                let ep_pin_ok = !filter_ep_pin || self.ep_is_legal(us, from, ep, captured, king_sq);
                if resolves_check && pin_line.contains(ep) && ep_pin_ok {
                    out.emit(from, ep, MoveKind::EnPassant);
                }
            }
        }
    }

    /// Emits the unpinned single-push moves whose targets are `targets`, each
    /// reached from the square `delta` indices behind it (the fixed back-shift of
    /// the forward direction). Last-rank targets expand to the four promotions.
    #[inline]
    fn emit_pawn_pushes<S: MoveSink>(
        out: &mut S,
        targets: Bitboard,
        delta: u8,
        promo_mask: Bitboard,
        us: Color,
    ) {
        for to in targets & !promo_mask {
            out.emit(back_shift(to, delta, us), to, MoveKind::Quiet);
        }
        for to in targets & promo_mask {
            let from = back_shift(to, delta, us);
            for role in PROMOTION_ROLES {
                out.emit(
                    from,
                    to,
                    MoveKind::Promotion {
                        role,
                        capture: false,
                    },
                );
            }
        }
    }

    /// Emits the unpinned capture moves whose targets are `targets`, each reached
    /// from the square `delta` indices behind it. Last-rank targets expand to the
    /// four capturing promotions.
    #[inline]
    fn emit_pawn_captures<S: MoveSink>(
        out: &mut S,
        targets: Bitboard,
        delta: u8,
        promo_mask: Bitboard,
        us: Color,
    ) {
        for to in targets & !promo_mask {
            out.emit(back_shift(to, delta, us), to, MoveKind::Capture);
        }
        for to in targets & promo_mask {
            let from = back_shift(to, delta, us);
            for role in PROMOTION_ROLES {
                out.emit(
                    from,
                    to,
                    MoveKind::Promotion {
                        role,
                        capture: true,
                    },
                );
            }
        }
    }

    /// Pushes a pawn single-advance, expanding promotions, subject to the check
    /// mask and pin line.
    fn push_pawn_advance<S: MoveSink>(
        &self,
        out: &mut S,
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
                out.emit(
                    from,
                    to,
                    MoveKind::Promotion {
                        role,
                        capture: false,
                    },
                );
            }
        } else {
            out.emit(from, to, MoveKind::Quiet);
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

    fn gen_knight_moves<S: MoveSink>(
        &self,
        out: &mut S,
        us: Color,
        our_pieces: Bitboard,
        their_pieces: Bitboard,
        check_mask: Bitboard,
        pins: &Pins,
    ) {
        for from in self.board.pieces(us, Role::Knight) {
            // A pinned knight can never move (its line is a straight ray and a
            // knight cannot stay on it).
            let pin_line = pins.line_of(from);
            let targets = knight_attacks(from) & !our_pieces & check_mask & pin_line;
            out.emit_targets(from, targets, their_pieces);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn gen_slider_moves<S: MoveSink>(
        &self,
        out: &mut S,
        us: Color,
        occupied: Bitboard,
        our_pieces: Bitboard,
        their_pieces: Bitboard,
        check_mask: Bitboard,
        pins: &Pins,
    ) {
        let board = &self.board;

        // Split by role rather than branching on `diagonal`/`straight` per square:
        // a bishop needs only the diagonal lookup, a rook only the straight one,
        // and a queen both, so each loop issues exactly the lookups it uses with
        // no per-square boolean test and no dead `EMPTY` accumulator. Measured a
        // ~7% win on the whole leaf generator with the cheap magic slider lookup
        // (where the per-square branch is a relatively large share of the work);
        // with the heavier default hyperbola lookup the branch is in the noise and
        // the split measured ~1% *slower*, so the original single-loop shape is
        // kept there. Both shapes emit byte-identical move sets (parity-checked).
        #[cfg(feature = "magic")]
        {
            // Destination filter shared by every slider square: never land on a
            // friendly piece, always resolve the check, never leave a pin line.
            let allowed = !our_pieces & check_mask;
            for from in board.pieces(us, Role::Bishop) {
                let pin_line = pins.line_of(from);
                let targets = bishop_attacks(from, occupied) & allowed & pin_line;
                out.emit_targets(from, targets, their_pieces);
            }
            for from in board.pieces(us, Role::Rook) {
                let pin_line = pins.line_of(from);
                let targets = rook_attacks(from, occupied) & allowed & pin_line;
                out.emit_targets(from, targets, their_pieces);
            }
            for from in board.pieces(us, Role::Queen) {
                let pin_line = pins.line_of(from);
                let attacks = bishop_attacks(from, occupied) | rook_attacks(from, occupied);
                let targets = attacks & allowed & pin_line;
                out.emit_targets(from, targets, their_pieces);
            }
        }
        #[cfg(not(feature = "magic"))]
        for (role, diagonal, straight) in [
            (Role::Bishop, true, false),
            (Role::Rook, false, true),
            (Role::Queen, true, true),
        ] {
            for from in board.pieces(us, role) {
                let pin_line = pins.line_of(from);
                let mut attacks = Bitboard::EMPTY;
                if diagonal {
                    attacks |= bishop_attacks(from, occupied);
                }
                if straight {
                    attacks |= rook_attacks(from, occupied);
                }
                let targets = attacks & !our_pieces & check_mask & pin_line;
                out.emit_targets(from, targets, their_pieces);
            }
        }
    }

    fn gen_castles<S: MoveSink>(
        &self,
        out: &mut S,
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
            out.emit(king_sq, king_dest, kind);
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
    /// itself, matching [`Position::pseudo_into`].
    ///
    /// The emitted castles are *fully legal*, not merely pseudo-legal: in
    /// addition to the king-walk-not-attacked test (under the pre-castle
    /// occupancy, which forbids passing through check), the king's final square
    /// is re-tested under the *post-castle* occupancy — the king and the
    /// castling rook removed from their start squares and the rook added at its
    /// destination. This catches the Chess960-only case where the castling
    /// rook's own departure opens an enemy slider line onto the king's landing
    /// square (impossible in standard chess, where the rook starts in the corner
    /// outside the king's walk), so callers need no further king-safety filter.
    ///
    /// `geom(side)` returns `(king_dest_file, rook_dest_file)` for that side, or
    /// `None` if the variant does not offer it.
    pub(crate) fn gen_castles_960<S: MoveSink>(
        &self,
        out: &mut S,
        geom: impl Fn(CastleSide) -> Option<(File, File)>,
    ) {
        let us = self.turn;
        // Dominant midgame node: the side to move holds no castling right, so
        // there is nothing to generate. A single bit-test settles it before any
        // occupancy, check, or king-danger work — the expensive parts below.
        if !self.castling.has_any(us) {
            return;
        }
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

        // The king-danger map (enemy attacks with our king lifted out of the
        // occupancy) is the costly query here, so it is computed lazily: only
        // once a castle candidate has cleared the cheap rook-present and
        // empty-path gates is it needed. Most nodes that still hold rights are
        // blocked before that point, so they never pay for it.
        let occ_without_king = occupied.without(king_sq);
        let mut king_danger: Option<Bitboard> = None;

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

            // The king must not pass through or land on a square attacked under
            // the pre-castle occupancy (forbids castling through check). The
            // king-danger map is computed on first need and cached for the
            // second castle side.
            let king_walk = between(king_sq, king_dest).with(king_dest);
            let danger =
                *king_danger.get_or_insert_with(|| self.attacked_by(them, occ_without_king));
            if !(king_walk & danger).is_empty() {
                continue;
            }

            // The king must also be safe on its destination under the *post*-
            // castle occupancy: king and rook off their start squares, rook on
            // its destination. Unlike standard chess, a Chess960 castling rook
            // can shield the king's landing square, so its departure may open a
            // discovered check that `king_danger` (rook still home) misses.
            let after = occupied.without(king_sq).without(rook_from).with(rook_dest);
            if !self.attackers_to(king_dest, them, after).is_empty() {
                continue;
            }

            let kind = match side {
                CastleSide::King => MoveKind::CastleKingside,
                CastleSide::Queen => MoveKind::CastleQueenside,
            };
            out.emit(king_sq, king_dest, kind);
        }
    }

    /// Tallies the Chess960 legal-move count of the side to move without
    /// materializing any move — the bulk leaf-counting analogue of the fast
    /// `generate_no_castles_into` + [`Position::gen_castles_960`] pair that the
    /// Chess960 variant runs at every node.
    ///
    /// At a perft leaf the variant needs only the *number* of legal moves, so
    /// this drives the same generation as the materializing path through a
    /// [`CountSink`]: the non-castling fast generator counts its moves via
    /// population counts, then the 960 castle generator tallies its (zero, one,
    /// or two) self-validating castles. The result is exactly the length of the
    /// list `generate_no_castles_into` + `gen_castles_960` would build, so every
    /// Chess960 perft count stays byte-identical while skipping move
    /// construction at the leaves — the same speedup standard chess gets from
    /// [`Position::count_legal`].
    pub(crate) fn count_legal_960(&self, geom: impl Fn(CastleSide) -> Option<(File, File)>) -> u64 {
        let mut sink = CountSink::default();
        self.generate_into_with_castles(&mut sink, false);
        self.gen_castles_960(&mut sink, geom);
        sink.count()
    }

    // -- Make move ---------------------------------------------------------

    /// Applies `mv` to this position, returning the resulting position.
    ///
    /// The move must be legal for this position. `play` does not re-validate
    /// legality; pass only moves obtained from [`Position::legal_moves`] (or
    /// validate with [`Position::is_legal`] first).
    ///
    /// This is the ergonomic immutable make-move: it clones the position and
    /// applies the move to the clone. A search engine that makes and unmakes
    /// millions of moves should reach for the in-place [`Position::play_unchecked`]
    /// instead, which mutates the position directly and so avoids the per-move
    /// copy.
    #[must_use]
    pub fn play(&self, mv: &Move) -> Position {
        let mut next = self.clone();
        next.play_unchecked(mv);
        next
    }

    /// Applies `mv` to this position **in place**, mutating it into the successor
    /// position without copying.
    ///
    /// This is the make/unmake hot path: it edits the board, updates castling
    /// rights, the en-passant target, the move clocks, and the side to move
    /// directly on `self`, producing exactly the position [`Position::play`] would
    /// return but with no allocation or
    /// clone. A search engine that descends the tree should snapshot the small
    /// amount of state it needs to undo (or clone once before a subtree) and call
    /// this on each step.
    ///
    /// # Contract
    ///
    /// The move **must be legal** for this position. Like shakmaty's
    /// `play_unchecked`, this method does *not* validate legality: passing an
    /// illegal or ill-formed move (one whose `from` square is empty, or a move
    /// kind inconsistent with the board) leaves the position in an unspecified
    /// state. Pass only moves obtained from [`Position::legal_moves`] (or first
    /// checked with [`Position::is_legal`]). The safe, checked default remains
    /// [`Position::play`].
    pub fn play_unchecked(&mut self, mv: &Move) {
        self.apply(mv);
    }

    /// Applies `mv` to this position **in place**, returning the piece captured
    /// by the move (if any) — the in-place make-move used by the variant layer to
    /// run its capture side-effect hook against the post-move core.
    ///
    /// The captured piece is the one that stood on the square the move removed an
    /// enemy from — the destination for ordinary captures and capturing
    /// promotions, the en-passant pawn's square for en passant. Quiet moves,
    /// castling, and drops capture nothing. The piece is read off the board
    /// *before* the move edits it.
    pub(crate) fn play_unchecked_tracking_capture(&mut self, mv: &Move) -> Option<(Piece, Square)> {
        let captured = self.captured_piece(mv);
        self.play_unchecked(mv);
        captured
    }

    /// Applies `mv` **in place** and returns an [`Undo`] that reverses it
    /// exactly, the zero-copy make half of make/unmake search.
    ///
    /// This is [`Position::play_unchecked`] plus a tiny reversal record: it
    /// performs the identical in-place edit (so the resulting position is
    /// byte-for-byte what `play_unchecked` and `play` produce) while snapshotting
    /// the captured piece and the prior irreversible state into the returned
    /// token. Pass that token to [`Position::unmake`] with the *same* move to
    /// restore the position.
    ///
    /// # Contract
    ///
    /// The move **must be legal** for this position, exactly as for
    /// [`Position::play_unchecked`]; `make` does not re-validate it. The returned
    /// [`Undo`] is valid only for unmaking *this* move from the position `make`
    /// produced — do not reorder make/unmake pairs or pair an [`Undo`] with a
    /// different move.
    pub fn make(&mut self, mv: &Move) -> Undo {
        let undo = Undo {
            captured: self.captured_piece(mv),
            castling: self.castling,
            ep_square: self.ep_square,
            halfmove_clock: self.halfmove_clock,
            fullmove_number: self.fullmove_number,
        };
        self.apply(mv);
        undo
    }

    /// Reverses a [`Position::make`] **in place**, restoring the position to
    /// exactly what it was before `mv` was made — board, side to move, castling
    /// rights, en-passant target, and both move clocks all byte-identical.
    ///
    /// # Contract
    ///
    /// `mv` and `undo` must be the move and token from the matching
    /// [`Position::make`] call on the position this is the successor of, and
    /// make/unmake pairs must nest (last made, first unmade). Misuse leaves the
    /// position in an unspecified state. There is no validation.
    pub fn unmake(&mut self, mv: &Move, undo: Undo) {
        // Flip the side back first so the moving side is `us` again; every board
        // edit below is described from the mover's perspective.
        self.turn = self.turn.opposite();
        let us = self.turn;
        let from = mv.from();
        let to = mv.to();
        let rank = back_rank(us);

        match mv.kind() {
            MoveKind::Quiet | MoveKind::DoublePawnPush | MoveKind::Capture => {
                // The moved piece is whatever now sits on `to`; carry it home.
                let moving = self
                    .board
                    .piece_at(to)
                    .expect("unmake: destination is occupied by the moved piece");
                self.board.remove_piece(to);
                self.board.set_piece(from, moving);
            }
            MoveKind::EnPassant => {
                let moving = self
                    .board
                    .piece_at(to)
                    .expect("unmake: destination holds the capturing pawn");
                self.board.remove_piece(to);
                self.board.set_piece(from, moving);
                // The captured pawn is restored from `undo` (its square is on the
                // destination's file and the origin's rank).
            }
            MoveKind::CastleKingside | MoveKind::CastleQueenside => {
                let side = if matches!(mv.kind(), MoveKind::CastleKingside) {
                    CastleSide::King
                } else {
                    CastleSide::Queen
                };
                // The rook's home file comes from the *prior* castling rights in
                // `undo` (current rights have already been revoked by the castle).
                let rook_file = undo
                    .castling
                    .rook_file(us, side)
                    .expect("unmake: castling right present in the undo token");
                let rook_from = Square::from_file_rank(rook_file, rank);
                let rook_dest_file = match side {
                    CastleSide::King => File::F,
                    CastleSide::Queen => File::D,
                };
                let rook_to = Square::from_file_rank(rook_dest_file, rank);
                let king = Piece::new(us, Role::King);
                let rook = Piece::new(us, Role::Rook);
                // Remove both from their destinations, then restore both to their
                // origins (handles the case where one origin equals the other's
                // destination).
                self.board.remove_piece(to);
                self.board.remove_piece(rook_to);
                self.board.set_piece(from, king);
                self.board.set_piece(rook_from, rook);
            }
            MoveKind::Promotion { .. } => {
                // The pawn promoted; remove the promoted piece and restore a pawn.
                self.board.remove_piece(to);
                self.board.set_piece(from, Piece::new(us, Role::Pawn));
            }
            MoveKind::Drop { .. } => {
                unreachable!("drops are unmade via the variant layer");
            }
        }

        // Restore any captured piece on its original square.
        if let Some((piece, sq)) = undo.captured {
            self.board.set_piece(sq, piece);
        }

        // Restore the scalar state verbatim.
        self.castling = undo.castling;
        self.ep_square = undo.ep_square;
        self.halfmove_clock = undo.halfmove_clock;
        self.fullmove_number = undo.fullmove_number;
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
    /// flipping the side to move and advancing the clocks — the core edit behind a
    /// crazyhouse drop, exposed for the variant layer's `apply_extra` hook.
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

        self.ep_square = None;

        self.board.set_piece(to, Piece::new(us, role));

        // Saturating so the narrowed clocks never wrap; the draw rules only look
        // at small halfmove values and games never approach the fullmove cap.
        self.halfmove_clock = self.halfmove_clock.saturating_add(1);
        if us.is_black() {
            self.fullmove_number = self.fullmove_number.saturating_add(1);
        }
        self.turn = them;
    }

    /// Captures the current reversible scalar state into an [`Undo`] *without*
    /// applying any move, for the variant make/unmake path to wrap a hook-driven
    /// edit (a crazyhouse drop) it applies itself.
    ///
    /// The returned token records no captured piece and the prior castling /
    /// en-passant / clock state, exactly as a [`Position::make`] of a
    /// non-capturing move would for those fields.
    pub(crate) fn snapshot_undo(&self) -> Undo {
        Undo {
            captured: None,
            castling: self.castling,
            ep_square: self.ep_square,
            halfmove_clock: self.halfmove_clock,
            fullmove_number: self.fullmove_number,
        }
    }

    /// Reverses a crazyhouse drop wrapped by [`Position::snapshot_undo`]: removes
    /// the dropped piece from `to`, flips the side back, and restores the scalar
    /// state from `undo`.
    pub(crate) fn unmake_drop(&mut self, to: Square, undo: Undo) {
        self.board.remove_piece(to);
        self.turn = self.turn.opposite();
        self.castling = undo.castling;
        self.ep_square = undo.ep_square;
        self.halfmove_clock = undo.halfmove_clock;
        self.fullmove_number = undo.fullmove_number;
    }

    /// Restores a `piece` onto `square`, the board edit [`VariantPosition::unmake`]
    /// uses to put back a piece a variant hook removed (atomic's blast) before
    /// reversing the core move.
    pub(crate) fn restore_piece(&mut self, square: Square, piece: Piece) {
        self.board.set_piece(square, piece);
    }

    /// Removes whatever piece sits on `square` (if any), revoking any castling
    /// right anchored on a rook that is removed. Exposed for the atomic variant's
    /// explosion side effect.
    ///
    /// Returns the removed piece, or `None` if the square was empty.
    pub(crate) fn remove_piece_tracked(&mut self, square: Square) -> Option<Piece> {
        let piece = self.board.piece_at(square)?;
        self.board.remove_piece(square);
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

        // Clear any prior en-passant target; set below only for a double push.
        self.ep_square = None;

        match mv.kind() {
            MoveKind::Quiet => {
                self.board.remove_piece(from);
                self.board.set_piece(to, moving);
            }
            MoveKind::Capture => {
                reset_clock = true;
                self.board.remove_piece(from);
                self.board.set_piece(to, moving);
            }
            MoveKind::DoublePawnPush => {
                self.board.remove_piece(from);
                self.board.set_piece(to, moving);
                // The ep target is the square the pawn skipped over.
                let mid_rank = from.rank().offset(if us.is_white() { 1 } else { -1 });
                if let Some(mid_rank) = mid_rank {
                    self.ep_square = Some(Square::from_file_rank(from.file(), mid_rank));
                }
            }
            MoveKind::EnPassant => {
                reset_clock = true;
                self.board.remove_piece(from);
                self.board.set_piece(to, moving);
                // Remove the captured pawn, which is on `to`'s file and `from`'s
                // rank.
                let captured = Square::from_file_rank(to.file(), from.rank());
                self.board.remove_piece(captured);
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
                self.board.remove_piece(from);
                self.board.remove_piece(rook_from);
                self.board.set_piece(to, moving);
                self.board.set_piece(rook_to, rook);
            }
            MoveKind::Promotion { role, capture } => {
                reset_clock = capture || is_pawn_move;
                self.board.remove_piece(from);
                self.board.set_piece(to, Piece::new(us, role));
            }
            MoveKind::Drop { .. } => {
                // Drops are a variant-only move kind; the core never generates
                // them and applies them through `apply_drop_core` instead, so
                // they never reach the standard make-move path.
                unreachable!("drop moves are applied via apply_drop_core");
            }
        }

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
            // Saturating so the narrowed `u8` clock never wraps; the 50-/75-move
            // comparisons only care about values up to 150 (< 255).
            self.halfmove_clock = self.halfmove_clock.saturating_add(1);
        }
        if us.is_black() {
            // Saturating `u16`; realistic games stay far below the cap.
            self.fullmove_number = self.fullmove_number.saturating_add(1);
        }
        self.turn = them;
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

        let position = Position::from_fields(
            board,
            turn,
            castling,
            ep_square,
            halfmove_clock,
            fullmove_number,
        );
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
    ///
    /// The two move clocks are accepted as `u32` (the natural FEN-parse type) but
    /// stored narrowed: `halfmove_clock` saturates into a `u8` and
    /// `fullmove_number` into a `u16`. Saturation is semantically harmless — the
    /// halfmove clock only matters up to the 75-move limit (150 < 255) and a
    /// fullmove number never approaches 65535 in a real game — and lets a clone
    /// of [`Position`] copy fewer bytes.
    #[must_use]
    pub(crate) fn from_fields(
        board: Board,
        turn: Color,
        castling: CastlingRights,
        ep_square: Option<Square>,
        halfmove_clock: u32,
        fullmove_number: u32,
    ) -> Position {
        Position {
            board,
            turn,
            castling,
            ep_square,
            halfmove_clock: halfmove_clock.min(u32::from(u8::MAX)) as u8,
            fullmove_number: fullmove_number.min(u32::from(u16::MAX)) as u16,
        }
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
    ///
    /// The royal opposite-king check uses the **standard** notion of attack
    /// ([`Position::is_attacked`]). Variants whose king safety differs (atomic,
    /// where the enemy king gives no executable check and a king adjacent to the
    /// enemy king is immune) must not rely on this and instead use
    /// [`Position::validate_core_with`] with their own predicate.
    pub(crate) fn validate_core(
        &self,
        require_two_kings: bool,
        king_is_royal: bool,
    ) -> Result<(), FenError> {
        self.validate_core_with(require_two_kings, king_is_royal, |pos, their_king, them| {
            pos.is_attacked(their_king, them.opposite())
        })
    }

    /// Like [`Position::validate_core`], but with a variant-supplied predicate
    /// deciding whether the side *not* to move is in check.
    ///
    /// `opposite_king_in_check(self, their_king, them)` returns `true` when the
    /// king of `them` (the side not to move) on `their_king` is under an
    /// executable attack by the side to move. Standard chess passes
    /// `is_attacked`; atomic passes its own rule so that legal atomic positions
    /// with the two kings adjacent — which the standard `is_attacked` wrongly
    /// flags, because a king "attacks" its neighbour — round-trip through FEN.
    pub(crate) fn validate_core_with(
        &self,
        require_two_kings: bool,
        king_is_royal: bool,
        opposite_king_in_check: impl Fn(&Position, Square, Color) -> bool,
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
                if opposite_king_in_check(self, their_king, them) {
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
        // The standard path writes the placement and castling fields straight
        // into `out`, avoiding the two intermediate `String`s that
        // `write_core_fen_with`/`write_core_fen_with_placement` allocate for the
        // variant writers that need owned `&str` fields.
        self.board.write_fen_placement(out);
        out.push(' ');
        out.push(if self.turn.is_white() { 'w' } else { 'b' });
        out.push(' ');
        write_standard_castling_field(self.castling, out);
        out.push(' ');
        match self.ep_square {
            Some(sq) => write_square(sq, out),
            None => out.push('-'),
        }
        out.push(' ');
        write_u32(u32::from(self.halfmove_clock), out);
        out.push(' ');
        write_u32(u32::from(self.fullmove_number), out);
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
            Some(sq) => write_square(sq, out),
            None => out.push('-'),
        }
        out.push(' ');
        write_u32(u32::from(self.halfmove_clock), out);
        out.push(' ');
        write_u32(u32::from(self.fullmove_number), out);
    }

    /// Basic sanity checks: each side has exactly one king, and the side *not*
    /// to move is not in check (which would mean the previous move was illegal).
    fn validate(&self) -> Result<(), FenError> {
        self.validate_core(true, true)
    }

    /// Serializes this position as a full six-field FEN string.
    #[must_use]
    pub fn to_fen(&self) -> String {
        // A standard FEN fits comfortably here: 71-byte placement worst case,
        // plus the turn, castling, en-passant, and two clock fields with their
        // separating spaces. Pre-sizing avoids reallocation during the writes.
        let mut fen = String::with_capacity(90);
        self.write_core_fen(&mut fen);
        fen
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

/// The centipawn material value of a role for Static Exchange Evaluation
/// ([`Position::see`]). The scale is the conventional pawn = 100; the king is a
/// large sentinel so a king recapture is never answered (no side allows its king
/// to be taken in return).
#[inline]
const fn see_value(role: Role) -> i32 {
    match role {
        Role::Pawn => 100,
        Role::Knight => 320,
        Role::Bishop => 330,
        Role::Rook => 500,
        Role::Queen => 900,
        Role::King => 10_000,
    }
}

/// Picks the least valuable attacker in `attackers` (a single-color attacker set
/// already restricted to the side about to recapture), the piece the swap-off
/// brings on next. Returns `None` when there is no attacker. Roles are scanned
/// cheapest-first, so the first non-empty intersection is the answer.
fn see_least_valuable(board: &Board, attackers: Bitboard) -> Option<Square> {
    for role in Role::ALL {
        let of_role = attackers & board.by_role(role);
        if let Some(sq) = of_role.lsb() {
            return Some(sq);
        }
    }
    None
}

/// The mask of light squares (a1 is dark), used for bishop-color analysis.
const LIGHT_SQUARES: u64 = 0x55AA_55AA_55AA_55AA;

/// The roles a pawn may promote to, in a stable order.
const PROMOTION_ROLES: [Role; 4] = [Role::Knight, Role::Bishop, Role::Rook, Role::Queen];

/// Recovers a bulk-generated pawn move's source square from its target `to` and
/// the fixed index `delta` of the move class (the forward/diagonal shift that
/// produced `to`). White pawns advance toward higher indices, so the source is
/// `delta` below; black pawns advance toward lower indices, so it is `delta`
/// above. The result is always on the board because `to` was produced by
/// shifting a real pawn — the inverse shift lands back on that pawn's square.
#[inline]
fn back_shift(to: Square, delta: u8, us: Color) -> Square {
    if us.is_white() {
        Square::new(to.index() - delta)
    } else {
        Square::new(to.index() + delta)
    }
}

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

/// Appends a square's two-character algebraic name (e.g. `e3`) to `out` via
/// direct ASCII byte pushes, avoiding the intermediate `String` that
/// [`fmt::Display`] would allocate.
#[inline]
fn write_square(sq: Square, out: &mut String) {
    let idx = sq.index();
    out.push((b'a' + idx % 8) as char);
    out.push((b'1' + idx / 8) as char);
}

/// Appends the decimal digits of `value` to `out` without allocating an
/// intermediate `String`. FEN move clocks are small, so a fixed stack buffer
/// (10 bytes covers all of `u32`) holds the digits while they are reversed.
#[inline]
fn write_u32(value: u32, out: &mut String) {
    if value < 10 {
        out.push((b'0' + value as u8) as char);
        return;
    }
    let mut buf = [0u8; 10];
    let mut i = buf.len();
    let mut v = value;
    while v > 0 {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    // SAFETY-free: the buffer holds only ASCII digits, valid UTF-8.
    out.push_str(core::str::from_utf8(&buf[i..]).unwrap_or("0"));
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
    // The whole recursion runs allocation-free. Each interior ply's move buffer
    // lives on a *stack* frame: a frame fills its own caller-provided `buf`, then
    // creates exactly one child buffer on its stack and reuses it (clearing, not
    // re-zeroing) across every child node it visits. Perft therefore pays the
    // `MoveList` value-init once per ply per parent node rather than once per
    // node — and never allocates a heap `Vec<MoveList>` or per-node `MoveList`,
    // since the inline array stays on the stack and standard positions never
    // spill. The leaf ply (`depth == 1`) bulk-counts directly and the
    // second-to-last ply bulk-counts its leaf children, so no buffer is needed
    // below the third-from-last ply.
    let mut buf = MoveList::new();
    perft_inner(position, depth, &mut buf)
}

/// Recursive core of [`perft`]. The caller owns `buf` (this ply's move buffer)
/// and reuses it across sibling nodes; each frame creates one child buffer on its
/// own stack frame and threads it down. Every buffer lives on a stack frame and
/// standard positions never spill, so the recursion performs zero heap
/// allocations.
fn perft_inner(position: &Position, depth: u32, buf: &mut MoveList) -> u64 {
    // Bulk leaf counting: at the last ply the only thing perft wants is *how
    // many* legal moves there are, so count them directly (population counts over
    // the generators' target masks) instead of materializing each move into a
    // buffer and taking its length. `count_legal()` drives the identical legal
    // generator, so the count equals `generate_into(..).len()` exactly.
    if depth == 1 {
        return position.count_legal();
    }
    buf.clear();
    position.generate_into(buf);
    if depth == 2 {
        // Every child is a leaf: bulk-count it directly, no child buffer needed.
        let mut nodes = 0;
        buf.for_each(|mv| nodes += position.play(&mv).count_legal());
        return nodes;
    }
    // One child buffer on this frame's stack, reused for every child node.
    let mut child = MoveList::new();
    let mut nodes = 0;
    buf.for_each(|mv| {
        nodes += perft_inner(&position.play(&mv), depth - 1, &mut child);
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

/// Like [`perft`], but splits the root moves across a [`rayon`] thread pool —
/// the optional, parallel counterpart gated behind the `parallel` feature.
///
/// Perft is embarrassingly parallel: the subtrees below the root's depth-1
/// children are independent, so this generates the root moves, hands each child
/// subtree to the rayon work-stealing pool, and sums the per-child serial
/// [`perft`] results. The node count is **byte-identical** to serial [`perft`]
/// — only the order the subtrees are summed in differs, and `u64` addition is
/// associative for these counts. It exists for benchmarking and tooling, not as
/// the core hot path.
///
/// Depths 0 and 1 are handled trivially (no thread pool is touched): depth 0 is
/// `1`, depth 1 is the bulk leaf count.
///
/// ```
/// use mce::{perft, perft_parallel, Position};
/// let pos = Position::startpos();
/// assert_eq!(perft_parallel(&pos, 4), perft(&pos, 4));
/// ```
#[cfg(feature = "parallel")]
#[must_use]
pub fn perft_parallel(position: &Position, depth: u32) -> u64 {
    use rayon::prelude::*;

    if depth == 0 {
        return 1;
    }
    if depth == 1 {
        // A single ply: bulk-count the root's legal moves directly, matching
        // serial `perft` and avoiding any thread-pool overhead.
        return position.count_legal();
    }
    // Split the root moves across the pool; each child computes its own subtree
    // serially via the existing allocation-free `perft`. Collecting the root
    // moves up front lets rayon own the per-child work items.
    let root_moves = position.legal_moves();
    root_moves
        .par_iter()
        .map(|mv| perft(&position.play(mv), depth - 1))
        .sum()
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

#[cfg(feature = "std")]
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

#[cfg(feature = "std")]
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
    fn position_is_compact() {
        // The Position layout is Board (64) + a tightly packed loose state
        // (castling rights, ep square, both move clocks, side to move), for a
        // total of 72 bytes. Issue #115 dropped the 8-byte incrementally-stored
        // Zobrist key, which nothing read (`zobrist()` recomputes from scratch),
        // shrinking every clone by a `u64`. A regression above 72 means the loose
        // state grew and copy-make pays for it again.
        assert_eq!(
            core::mem::size_of::<Position>(),
            72,
            "Position size regressed: loose state grew past the compact 72-byte layout"
        );
        // Packed castling rights are two bytes (four 4-bit nibbles), not four.
        assert_eq!(core::mem::size_of::<CastlingRights>(), 2);
    }

    #[test]
    fn castling_rights_round_trip_every_file() {
        // Every Option<File> (including the arbitrary Chess960 rook files) must
        // survive the packed nibble encoding unchanged, for each color/side.
        for color in Color::ALL {
            for side in [CastleSide::King, CastleSide::Queen] {
                let mut rights = CastlingRights::NONE;
                rights.set(color, side, None);
                assert_eq!(rights.rook_file(color, side), None);
                for file in File::ALL {
                    rights.set(color, side, Some(file));
                    assert_eq!(rights.rook_file(color, side), Some(file));
                    // Setting one slot must not disturb the others.
                    for other_color in Color::ALL {
                        for other_side in [CastleSide::King, CastleSide::Queen] {
                            if (other_color, other_side) == (color, side) {
                                continue;
                            }
                            assert_eq!(rights.rook_file(other_color, other_side), None);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn castling_rights_standard_files() {
        let r = CastlingRights::STANDARD;
        assert_eq!(r.rook_file(Color::White, CastleSide::King), Some(File::H));
        assert_eq!(r.rook_file(Color::White, CastleSide::Queen), Some(File::A));
        assert_eq!(r.rook_file(Color::Black, CastleSide::King), Some(File::H));
        assert_eq!(r.rook_file(Color::Black, CastleSide::Queen), Some(File::A));
        assert!(r.has_any(Color::White) && r.has_any(Color::Black));
        assert!(!CastlingRights::NONE.has_any(Color::White));
        assert!(!CastlingRights::NONE.has_any(Color::Black));
    }

    #[test]
    fn halfmove_clock_saturates_on_parse() {
        // A pathological halfmove clock larger than the u8 store saturates to 255
        // rather than wrapping; this never changes a draw-rule decision because
        // the rules only test values up to 150.
        let pos = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 9999 1").unwrap();
        assert_eq!(pos.halfmove_clock(), 255);
        // A normal in-range clock round-trips exactly.
        let pos = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 137 1").unwrap();
        assert_eq!(pos.halfmove_clock(), 137);
    }

    #[test]
    fn fullmove_number_saturates_on_parse() {
        let pos = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 70000").unwrap();
        assert_eq!(pos.fullmove_number(), 65535);
        let pos = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 4321").unwrap();
        assert_eq!(pos.fullmove_number(), 4321);
        assert_eq!(pos.to_fen(), "4k3/8/8/8/8/8/8/4K3 w - - 0 4321");
    }

    #[test]
    fn halfmove_clock_saturating_increment_does_not_wrap() {
        // Drive the clock to its u8 ceiling via repeated quiet moves; it must
        // pin at 255 instead of panicking or wrapping to 0.
        let mut pos = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 250 1").unwrap();
        for _ in 0..20 {
            let mv = pos
                .legal_moves()
                .into_iter()
                .find(|m| m.kind() == MoveKind::Quiet)
                .expect("a quiet king move exists");
            pos.play_unchecked(&mv);
        }
        assert_eq!(pos.halfmove_clock(), 255);
    }

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
    fn bulk_count_equals_materialized_count() {
        // The bulk-counting `CountSink` (population counts over the target masks,
        // promotions expanded, ep / castling gated) must agree exactly with the
        // length of the fully materialized legal move list, on positions
        // exercising checks, double checks, pins, en passant, promotions, and
        // castling. This is the invariant perft's depth-1 bulk count relies on.
        for fen in [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
            "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
            // En-passant available, pin and check positions.
            "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1",
            "8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1",
            // Double check: only king moves are legal.
            "4k3/8/4r3/8/8/4R3/8/3RK3 b - - 0 1",
            // Promotions, including capturing promotions.
            "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N b - - 0 1",
        ] {
            let pos = Position::from_fen(fen).unwrap();
            let materialized = pos.legal_moves().len() as u64;
            assert_eq!(
                pos.count_legal(),
                materialized,
                "bulk count != materialized for {fen}"
            );
            // And the public count helper agrees.
            assert_eq!(pos.legal_move_count(), materialized as usize);
        }
    }

    #[test]
    fn apply_drop_core_places_piece_and_advances_state() {
        // Drop plumbing for crazyhouse: placing a pocketed piece on an empty
        // square flips the side and leaves clocks/ep correct.
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
        // The Zobrist key still reflects the dropped piece (recomputed on demand).
        assert_ne!(
            pos.zobrist(),
            Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 5 9")
                .unwrap()
                .zobrist()
        );
    }

    #[test]
    fn perft_divide_sums_to_total() {
        let pos = Position::startpos();
        let div = perft_divide(&pos, 3);
        let total: u64 = div.iter().map(|(_, n)| n).sum();
        assert_eq!(total, perft(&pos, 3));
        assert_eq!(div.len(), 20);
    }

    /// `perft_parallel` must return node counts byte-identical to serial
    /// `perft`, including the trivial depth-0 and depth-1 cases that bypass the
    /// thread pool, across the startpos and the Kiwipete tactical position.
    #[cfg(feature = "parallel")]
    #[test]
    fn perft_parallel_matches_serial() {
        let start = Position::startpos();
        for depth in 0..=5 {
            assert_eq!(
                perft_parallel(&start, depth),
                perft(&start, depth),
                "startpos depth {depth}"
            );
        }
        let kiwipete = Position::from_fen(KIWIPETE).unwrap();
        for depth in 0..=4 {
            assert_eq!(
                perft_parallel(&kiwipete, depth),
                perft(&kiwipete, depth),
                "kiwipete depth {depth}"
            );
        }
    }

    // -- Public attack/threat queries + SEE --------------------------------

    #[test]
    fn checkers_reports_single_checking_piece() {
        // Black rook on e8 checks the white king on e1 down the open e-file.
        let pos = Position::from_fen("4rk2/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        assert_eq!(pos.checkers(), Bitboard::from(Square::E8));
        assert!(pos.is_check());
    }

    #[test]
    fn checkers_reports_double_check() {
        // White king e1; black rook e8 (file) and black bishop a5 (diagonal)
        // both bear on e1 — a genuine double check.
        let pos = Position::from_fen("4rk2/8/8/b7/8/8/8/4K3 w - - 0 1").unwrap();
        let checkers = pos.checkers();
        assert_eq!(checkers.count(), 2);
        assert!(checkers.contains(Square::E8));
        assert!(checkers.contains(Square::A5));
    }

    #[test]
    fn pinned_reports_a_pinned_knight() {
        // White king e1, white knight e4, black rook e8: the knight is pinned to
        // the king down the e-file and may not move off it.
        let pos = Position::from_fen("4rk2/8/8/8/4N3/8/8/4K3 w - - 0 1").unwrap();
        let pinned = pos.pinned(Color::White);
        assert_eq!(pinned, Bitboard::from(Square::E4));
        // The enemy has nothing pinned.
        assert!(pos.pinned(Color::Black).is_empty());
        // A pinned knight has no legal moves here (every knight jump leaves the
        // e-file).
        assert!(!pos.legal_moves().iter().any(|m| m.from() == Square::E4));
    }

    #[test]
    fn pinned_is_empty_without_a_pinner() {
        // Same knight, but the rook is replaced by an empty file: nothing pins.
        let pos = Position::from_fen("6k1/8/8/8/4N3/8/8/4K3 w - - 0 1").unwrap();
        assert!(pos.pinned(Color::White).is_empty());
    }

    #[test]
    fn attackers_to_collects_every_attacker() {
        // d4 is attacked by: white pawn c3 (diagonal), white knight f3, white rook
        // d1 (up the file), and a white queen on a7 (along the a7-d4 diagonal).
        let pos = Position::from_fen("4k3/Q7/8/8/8/2P2N2/8/3RK3 w - - 0 1").unwrap();
        let occ = pos.board().occupied();
        let attackers = pos.attackers_to(Square::D4, Color::White, occ);
        assert!(attackers.contains(Square::C3));
        assert!(attackers.contains(Square::F3));
        assert!(attackers.contains(Square::D1));
        assert!(attackers.contains(Square::A7));
        assert_eq!(attackers.count(), 4);
        assert!(pos.is_attacked(Square::D4, Color::White));
        // Black attacks none of d4.
        assert!(!pos.is_attacked(Square::D4, Color::Black));
    }

    #[test]
    fn attacks_from_each_piece_type() {
        // Empty square attacks nothing.
        let empty = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        assert!(empty.attacks_from(Square::D4).is_empty());

        // Knight on d4 (center): all eight jumps.
        let knight = Position::from_fen("4k3/8/8/8/3N4/8/8/4K3 w - - 0 1").unwrap();
        assert_eq!(knight.attacks_from(Square::D4), knight_attacks(Square::D4));

        // White pawn on d4 attacks c5 and e5 only.
        let pawn = Position::from_fen("4k3/8/8/8/3P4/8/8/4K3 w - - 0 1").unwrap();
        assert_eq!(
            pawn.attacks_from(Square::D4),
            Bitboard::from(Square::C5) | Bitboard::from(Square::E5)
        );

        // King on e1 (corner-ish): king-step pattern.
        let king = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        assert_eq!(king.attacks_from(Square::E1), king_attacks(Square::E1));

        // Bishop on c1 blocked on the e3 square shows the cut ray.
        let bishop = Position::from_fen("4k3/8/8/8/8/4p3/8/2B1K3 w - - 0 1").unwrap();
        let occ = bishop.board().occupied();
        assert_eq!(
            bishop.attacks_from(Square::C1),
            bishop_attacks(Square::C1, occ)
        );
        assert!(bishop.attacks_from(Square::C1).contains(Square::E3));
        assert!(!bishop.attacks_from(Square::C1).contains(Square::F4));

        // Rook and queen mirror the slider helpers under occupancy.
        let rook = Position::from_fen("4k3/8/8/8/8/8/8/R3K3 w - - 0 1").unwrap();
        let occ = rook.board().occupied();
        assert_eq!(rook.attacks_from(Square::A1), rook_attacks(Square::A1, occ));

        let queen = Position::from_fen("4k3/8/8/8/8/8/8/Q3K3 w - - 0 1").unwrap();
        let occ = queen.board().occupied();
        assert_eq!(
            queen.attacks_from(Square::A1),
            bishop_attacks(Square::A1, occ) | rook_attacks(Square::A1, occ)
        );
    }

    #[test]
    fn see_free_piece_is_a_clear_win() {
        // White rook on d1 captures an undefended black knight on d8: the knight
        // is worth 320 and nothing recaptures, so SEE = +320.
        let pos = Position::from_fen("3n4/8/8/8/8/8/8/3RK1k1 w - - 0 1").unwrap();
        let cap = pos.parse_uci("d1d8").unwrap();
        assert!(cap.is_capture());
        assert_eq!(pos.see(&cap), see_value(Role::Knight));
    }

    #[test]
    fn see_defended_pawn_capture_loses_for_higher_value_initiator() {
        // White rook on e1 captures a black pawn on e5 that is defended by a black
        // pawn on d6. White wins a pawn (+100) but loses the rook (-500) in the
        // recapture: SEE = 100 - 500 = -400.
        let pos = Position::from_fen("4k3/8/3p4/4p3/8/8/8/4RK2 w - - 0 1").unwrap();
        let cap = pos.parse_uci("e1e5").unwrap();
        assert!(cap.is_capture());
        assert_eq!(pos.see(&cap), see_value(Role::Pawn) - see_value(Role::Rook));
    }

    #[test]
    fn see_equal_pawn_trade_is_zero() {
        // White pawn d4 captures black pawn e5, which is defended by black pawn
        // f6: pawn for pawn, an even trade, SEE = 0.
        let pos = Position::from_fen("4k3/8/5p2/4p3/3P4/8/8/4K3 w - - 0 1").unwrap();
        let cap = pos.parse_uci("d4e5").unwrap();
        assert!(cap.is_capture());
        assert_eq!(pos.see(&cap), 0);
    }

    #[test]
    fn see_xray_through_a_stacked_rook() {
        // The x-ray case: white rooks doubled on e1/e2 attack a black pawn on e5
        // that is defended by a doubled black rook battery on e7/e8. The front
        // white rook takes the pawn; black's front rook recaptures; the rear white
        // rook (x-rayed in through e2 once e1 has moved) recaptures; black's rear
        // rook recaptures. Net for white: +pawn (e5) - rook (e1 lost) + rook (e7
        // won) - rook (the rear white rook lost) = 100 - 500 + 500 - 500 = -400.
        let pos = Position::from_fen("4k3/4r3/4r3/4p3/8/8/4R3/4R1K1 w - - 0 1").unwrap();
        // Build the front-rook capture directly: SEE judges the swap-off on `to`
        // regardless of the move's full king-safety legality.
        let cap = Move::new(Square::E1, Square::E5, MoveKind::Capture);
        assert!(cap.is_capture());
        assert_eq!(
            pos.see(&cap),
            see_value(Role::Pawn) - see_value(Role::Rook) + see_value(Role::Rook)
                - see_value(Role::Rook)
        );
    }

    #[test]
    fn see_noncapture_is_zero_when_undefended() {
        // A quiet rook move to an undefended square risks nothing: SEE = 0.
        let pos = Position::from_fen("4k3/8/8/8/8/8/8/R3K3 w - - 0 1").unwrap();
        let quiet = pos.parse_uci("a1a4").unwrap();
        assert!(!quiet.is_capture());
        assert_eq!(pos.see(&quiet), 0);
    }

    #[test]
    fn see_noncapture_is_negative_when_defended() {
        // A quiet rook move onto a square defended by a black pawn: the rook can
        // be taken for nothing, so SEE = -value(rook). Black pawn on b5 defends a4
        // and c4; the rook slides a1->a4 onto the defended a4.
        let pos = Position::from_fen("4k3/8/8/1p6/8/8/8/R3K3 w - - 0 1").unwrap();
        let quiet = Move::new(Square::A1, Square::A4, MoveKind::Quiet);
        assert!(!quiet.is_capture());
        // Verify a4 is genuinely defended by the b5 pawn.
        assert!(pos.is_attacked(Square::A4, Color::Black));
        assert_eq!(pos.see(&quiet), -see_value(Role::Rook));
    }

    #[test]
    fn see_en_passant_wins_a_pawn() {
        // White pawn d5, black just played c7c5; white captures en passant. The
        // recovered pawn is undefended, so SEE = +100.
        let pos = Position::from_fen("4k3/8/8/2pP4/8/8/8/4K3 w - c6 0 1").unwrap();
        let ep = pos.parse_uci("d5c6").unwrap();
        assert_eq!(ep.kind(), MoveKind::EnPassant);
        assert_eq!(pos.see(&ep), see_value(Role::Pawn));
    }
}
