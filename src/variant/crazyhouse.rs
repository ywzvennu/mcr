//! Crazyhouse as a [`Variant`]: standard chess plus captured pieces that go into
//! the capturer's *pocket* and may later be *dropped* back onto the board as a
//! move.
//!
//! # Rules (standard Lichess crazyhouse)
//!
//! Every capture sends the captured piece — flipped to the capturer's color —
//! into that side's pocket. On a later turn a player may, instead of moving a
//! piece, drop a pocketed piece onto any empty square. A dropped pawn may not be
//! placed on rank 1 or rank 8; every other drop is unrestricted, and a drop may
//! give check or even deliver checkmate (pawn-drop-mate is allowed here).
//!
//! A piece that reached the board by promotion is tracked in a *promoted* mask:
//! when such a piece is later captured it reverts to a **pawn** in the
//! capturer's pocket (you only ever pocket the original material, never a queen
//! you made from a pawn). The promoted bit follows the piece while it moves and
//! is cleared when the piece leaves the board.
//!
//! Movement, king safety, castling, and the ordinary checkmate / stalemate / draw
//! terminations are all inherited unchanged from standard chess.
//!
//! # FEN convention
//!
//! Crazyhouse FEN rides two extra markers on the placement field (the first of
//! the six standard fields) rather than adding a trailing field:
//!
//! - the pocket is a bracketed suffix on the placement, uppercase letters for
//!   White's pocket and lowercase for Black's, e.g.
//!   `rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1` (empty pocket)
//!   or `...[PPnn]` (White holds two pawns, Black two knights);
//! - a promoted piece on the board carries a trailing `~`, e.g. `Q~`.
//!
//! Because these markers live on the placement field — which the generic
//! [`VariantPosition`] FEN path otherwise feeds straight to the core board parser
//! — this variant overrides the [`Variant::read_placement`] /
//! [`Variant::write_placement`] hooks to strip and re-emit the markers around the
//! bare core placement, leaving [`crate::Board`] untouched. The standard
//! [`VariantPosition::from_fen`] / [`VariantPosition::to_fen`] paths then handle
//! crazyhouse FEN with no special casing at the call site.

use super::{Variant, VariantId, VariantPosition, VariantState};
use crate::movelist::MoveList;
use crate::position::{FenError, Position};
use crate::{Bitboard, Board, Color, Move, MoveKind, Piece, Role, SanError, Square};
use alloc::format;
use alloc::{string::String, string::ToString, vec::Vec};

/// The number of droppable roles (pawn, knight, bishop, rook, queen — never the
/// king). Indexed by [`pocket_index`].
const POCKET_ROLES: usize = 5;

/// The droppable roles in pocket order: pawn, knight, bishop, rook, queen.
const DROP_ROLES: [Role; POCKET_ROLES] = [
    Role::Pawn,
    Role::Knight,
    Role::Bishop,
    Role::Rook,
    Role::Queen,
];

/// The pocket slot index for a droppable role, or `None` for a king (which is
/// never pocketed).
#[inline]
const fn pocket_index(role: Role) -> Option<usize> {
    match role {
        Role::Pawn => Some(0),
        Role::Knight => Some(1),
        Role::Bishop => Some(2),
        Role::Rook => Some(3),
        Role::Queen => Some(4),
        Role::King => None,
    }
}

/// The pocket slot index for a color.
#[inline]
const fn color_index(color: Color) -> usize {
    match color {
        Color::White => 0,
        Color::Black => 1,
    }
}

/// The crazyhouse per-position state: each side's pocket and the set of squares
/// holding a promoted piece.
///
/// `pockets[color][role]` is the count of pieces of that `role` the `color` holds
/// in hand, indexed by `pocket_index` over the five droppable roles
/// (pawn, knight, bishop, rook, queen). `promoted` marks every square whose
/// occupant arrived there by promotion, so that capturing it returns a pawn —
/// not the promoted role — to the pocket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CrazyhouseState {
    /// Per-color, per-role counts of pocketed pieces, indexed
    /// `[color][role]` via `color_index` / `pocket_index`.
    pub pockets: [[u8; POCKET_ROLES]; 2],
    /// The squares occupied by a piece that arrived there through promotion.
    pub promoted: Bitboard,
}

impl CrazyhouseState {
    /// The number of pieces of `role` that `color` holds in pocket.
    #[must_use]
    #[inline]
    pub fn pocket(&self, color: Color, role: Role) -> u8 {
        match pocket_index(role) {
            Some(i) => self.pockets[color_index(color)][i],
            None => 0,
        }
    }

    /// Adds one piece of `role` to `color`'s pocket. A king is never pocketed.
    #[inline]
    fn push(&mut self, color: Color, role: Role) {
        if let Some(i) = pocket_index(role) {
            self.pockets[color_index(color)][i] =
                self.pockets[color_index(color)][i].saturating_add(1);
        }
    }

    /// Removes one piece of `role` from `color`'s pocket.
    #[inline]
    fn pop(&mut self, color: Color, role: Role) {
        if let Some(i) = pocket_index(role) {
            let slot = &mut self.pockets[color_index(color)][i];
            *slot = slot.saturating_sub(1);
        }
    }
}

impl VariantState for CrazyhouseState {}

// -- Zobrist constants for the pocket and promoted state --------------------

/// The largest pocket count any single `(color, role)` slot contributes a
/// distinct Zobrist key for. Counts at or above this share the top key, which is
/// sufficient: a crazyhouse pocket never legitimately exceeds the sixteen pieces
/// of one side, and the cap only affects hashing, never play.
const MAX_POCKET_KEYS: usize = 17;

/// The fixed seed for the crazyhouse key generator, distinct from the core and
/// three-check seeds so these keys occupy their own feature space.
const CRAZYHOUSE_SEED: u64 = 0x6A2B_19F5_3C7D_8E41;

/// One step of splitmix64 (a tiny deterministic mixing function, not an RNG),
/// matching the core Zobrist table generator so the keys are reproducible across
/// builds and runs with no `rand` dependency.
#[inline]
const fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The full set of crazyhouse Zobrist constants, generated deterministically at
/// compile time.
struct CrazyhouseKeys {
    /// One key per `[color][role][count]` pocket slot; `count == 0` holds zero so
    /// an empty pocket contributes nothing and the startpos key matches the core.
    pockets: [[[u64; MAX_POCKET_KEYS]; POCKET_ROLES]; 2],
    /// One key per square marked promoted.
    promoted: [u64; 64],
}

impl CrazyhouseKeys {
    const fn generate() -> CrazyhouseKeys {
        let mut state = CRAZYHOUSE_SEED;

        let mut pockets = [[[0u64; MAX_POCKET_KEYS]; POCKET_ROLES]; 2];
        let mut c = 0;
        while c < 2 {
            let mut r = 0;
            while r < POCKET_ROLES {
                // Slot 0 (an empty count) is left at zero deliberately.
                let mut n = 1;
                while n < MAX_POCKET_KEYS {
                    pockets[c][r][n] = splitmix64(&mut state);
                    n += 1;
                }
                r += 1;
            }
            c += 1;
        }

        let mut promoted = [0u64; 64];
        let mut s = 0;
        while s < 64 {
            promoted[s] = splitmix64(&mut state);
            s += 1;
        }

        CrazyhouseKeys { pockets, promoted }
    }
}

/// The crazyhouse Zobrist constant tables, computed once at compile time.
static CRAZYHOUSE_KEYS: CrazyhouseKeys = CrazyhouseKeys::generate();

/// The Zobrist contribution of holding `count` pieces of role index `role` in
/// `color`'s pocket. The empty count contributes zero.
#[inline]
fn pocket_key(color: usize, role: usize, count: u8) -> u64 {
    let n = (count as usize).min(MAX_POCKET_KEYS - 1);
    CRAZYHOUSE_KEYS.pockets[color][role][n]
}

/// The Zobrist contribution of a promoted piece standing on `square`.
#[inline]
fn promoted_key(square: Square) -> u64 {
    CRAZYHOUSE_KEYS.promoted[square.index() as usize]
}

/// The crazyhouse rule layer: standard chess plus pocket drops and promoted-piece
/// tracking. A zero-sized marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CrazyhouseRules;

impl Variant for CrazyhouseRules {
    type State = CrazyhouseState;
    const ID: VariantId = VariantId::Crazyhouse;

    /// H4: a capture sends the captured piece — flipped to the capturer's color —
    /// into the capturer's pocket, as a **pawn** if it was a promoted piece and
    /// by its own role otherwise; the captured square's promoted bit is cleared.
    ///
    /// The captured square's promoted state is read from the *parent*'s mask via
    /// `state`, which still reflects the pre-move board at this point (the
    /// promoted mask is only updated in [`CrazyhouseRules::post_apply`], which
    /// runs after this hook).
    fn capture_side_effects(
        core: &mut Position,
        state: &mut Self::State,
        mv: &Move,
        captured: (Piece, Square),
        _removed: &mut super::heapless_removals::Removals,
    ) {
        let _ = core;
        let (piece, square) = captured;
        // The capturer is the side that just moved.
        let capturer = piece.color.opposite();
        let pocketed = if state.promoted.contains(square) {
            Role::Pawn
        } else {
            piece.role
        };
        state.push(capturer, pocketed);
        // The captured piece left the board; its promoted bit (if any) goes with
        // it. `post_apply` will carry the *mover's* promoted bit to `mv.to()`
        // afterwards, so clearing the captured square here is safe even when the
        // capture lands on it.
        state.promoted.clear(square);
        let _ = mv;
    }

    /// H5: for every role the side to move holds in pocket, emit a drop onto each
    /// *legal* empty square; pawns may not be dropped on rank 1 or rank 8.
    ///
    /// A drop only ever *adds* a friendly piece, so it can never expose the
    /// mover's own king — it can only block an existing check. King safety
    /// therefore reduces to: when the mover is in check, a drop is legal only if
    /// it interposes on the checking line. A double check (two checkers) cannot be
    /// blocked by a single drop, and a check from a knight or a pawn (no squares
    /// between it and the king) cannot be blocked at all; in both cases no drop is
    /// legal. When the mover is not in check, every empty square is a legal target.
    fn extra_moves(core: &Position, state: &Self::State, out: &mut MoveList) {
        let us = core.turn();
        let empty = !core.board().occupied();

        // Restrict to the squares that resolve any current check.
        let droppable = if core.is_check() {
            let checkers = core.checkers();
            if checkers.count() != 1 {
                // Double (or more) check: the king must move; no drop helps.
                return;
            }
            let checker = checkers.lsb().expect("exactly one checker");
            let king = match core.board().king_of(us) {
                Some(k) => k,
                None => return,
            };
            // The empty squares strictly between the king and the slider checker.
            // For a knight/pawn checker `between` is empty, so no drop is legal.
            empty & crate::attacks::between(king, checker)
        } else {
            empty
        };

        if droppable.is_empty() {
            return;
        }

        let pawn_droppable = droppable & !(Bitboard::RANK_1 | Bitboard::RANK_8);
        for role in DROP_ROLES {
            if state.pocket(us, role) == 0 {
                continue;
            }
            let targets = if role == Role::Pawn {
                pawn_droppable
            } else {
                droppable
            };
            for square in targets {
                out.push(Move::drop(role, square));
            }
        }
    }

    /// H6: apply a drop to the core board and decrement the pocket. A dropped
    /// piece is never promoted, so the promoted mask is untouched here.
    fn apply_extra(core: &mut Position, state: &mut Self::State, mv: &Move) {
        let role = mv
            .drop_role()
            .expect("apply_extra is only called for drop moves");
        let us = core.turn();
        core.apply_drop_core(role, mv.to());
        state.pop(us, role);
    }

    /// H14: after any move, maintain the promoted mask — carry a moving promoted
    /// piece's bit to its destination, and set the bit when a pawn promotes.
    ///
    /// This runs once `core` is the finished child and the capture hook has
    /// already cleared any captured square's bit, so the source/destination
    /// bookkeeping below never collides with a capture.
    fn post_apply(core: &mut Position, state: &mut Self::State, mv: &Move) {
        let _ = core;
        if mv.is_drop() {
            // Drops never produce a promoted piece, and from == to, so there is
            // nothing to carry.
            return;
        }
        let from = mv.from();
        let to = mv.to();
        match mv.kind() {
            MoveKind::Promotion { .. } => {
                // A pawn promoted: the new piece on `to` is promoted. Its source
                // square (a pawn) was never promoted, so only `to` is set.
                state.promoted.clear(from);
                state.promoted.set(to);
            }
            _ => {
                // A non-promoting move carries any promoted bit from the source to
                // the destination.
                if state.promoted.contains(from) {
                    state.promoted.clear(from);
                    state.promoted.set(to);
                } else {
                    // The source had no promoted bit; clear `to` in case a
                    // (now-overwritten) promoted piece had stood there. Captures
                    // already cleared `to` in the capture hook, but a quiet move
                    // onto an empty square cannot land on a promoted square, so
                    // this is a defensive no-op in practice.
                    state.promoted.clear(to);
                }
            }
        }
    }

    /// H12: fold every pocket count and every promoted square into the key, so
    /// that two positions differing only in pockets or promoted state hash apart.
    fn hash_state(state: &Self::State, hash: &mut u64) {
        for c in 0..2 {
            for r in 0..POCKET_ROLES {
                *hash ^= pocket_key(c, r, state.pockets[c][r]);
            }
        }
        for square in state.promoted {
            *hash ^= promoted_key(square);
        }
    }

    /// H13b read: parse the crazyhouse placement, recovering the bracketed pocket
    /// suffix and the `~` promotion markers before the bare placement reaches the
    /// core [`Board`] parser. The trailing-field reader has nothing to do.
    fn read_placement(token: &str) -> Result<(Board, Self::State), FenError> {
        let (clean, state) = split_placement(token)?;
        let board = Board::from_fen_placement(&clean).map_err(FenError::Placement)?;
        Ok((board, state))
    }

    /// H13b write: emit the placement with the `[...]` pocket suffix (empty as
    /// `[]`) and a `~` after every promoted piece.
    fn write_placement(board: &Board, state: &Self::State, out: &mut String) {
        write_placement_with_markers(board, state, out);
    }
}

/// Crazyhouse as a [`VariantPosition`].
///
/// Standard movement and king safety, plus pocket drops ([`CrazyhouseState`]).
/// FEN carries the pocket as a bracketed placement suffix (empty as `[]`) and
/// promoted pieces with a trailing `~`; round-trips through
/// [`VariantPosition::from_fen`] / [`VariantPosition::to_fen`].
pub type Crazyhouse = VariantPosition<CrazyhouseRules>;

/// Writes the crazyhouse placement field: the rank-by-rank board with a `~` after
/// each promoted piece, followed by the `[...]` pocket suffix.
fn write_placement_with_markers(board: &Board, state: &CrazyhouseState, out: &mut String) {
    for rank_from_top in 0..8u8 {
        if rank_from_top > 0 {
            out.push('/');
        }
        let rank = crate::Rank::new(7 - rank_from_top).expect("rank in range");
        let mut empty = 0u8;
        for file_idx in 0..8u8 {
            let file = crate::File::new(file_idx).expect("file in range");
            let square = Square::from_file_rank(file, rank);
            match board.piece_at(square) {
                Some(piece) => {
                    if empty > 0 {
                        out.push((b'0' + empty) as char);
                        empty = 0;
                    }
                    out.push(piece.char());
                    if state.promoted.contains(square) {
                        out.push('~');
                    }
                }
                None => empty += 1,
            }
        }
        if empty > 0 {
            out.push((b'0' + empty) as char);
        }
    }

    // The bracketed pocket: White's pieces uppercase, Black's lowercase, in role
    // order (pawn, knight, bishop, rook, queen), repeated by count.
    out.push('[');
    for color in Color::ALL {
        for role in DROP_ROLES {
            for _ in 0..state.pocket(color, role) {
                out.push(Piece::new(color, role).char());
            }
        }
    }
    out.push(']');
}

/// Splits a crazyhouse placement token into the bare core placement, the pocket
/// contents, and the set of promoted squares.
///
/// The token is the standard rank-by-rank placement with two crazyhouse
/// additions: a trailing `[...]` pocket suffix (uppercase = White, lowercase =
/// Black) and a `~` immediately after any promoted piece letter. This strips both
/// markers, returning the clean placement for [`Board::from_fen_placement`]
/// alongside the recovered [`CrazyhouseState`].
fn split_placement(token: &str) -> Result<(String, CrazyhouseState), FenError> {
    let mut state = CrazyhouseState::default();

    // Separate the optional `[...]` pocket suffix from the board placement.
    let (board_part, pocket_part) = match token.split_once('[') {
        Some((board, rest)) => {
            let pocket = rest
                .strip_suffix(']')
                .ok_or_else(|| FenError::Placement(parse_placement_err(token)))?;
            (board, Some(pocket))
        }
        None => (token, None),
    };

    if let Some(pocket) = pocket_part {
        for ch in pocket.chars() {
            let piece =
                Piece::from_char(ch).ok_or_else(|| FenError::Placement(parse_placement_err(ch)))?;
            if piece.role == Role::King {
                return Err(FenError::Placement(parse_placement_err(ch)));
            }
            state.push(piece.color, piece.role);
        }
    }

    // Walk the board placement, recording `~` markers as promoted squares and
    // emitting a clean placement with the markers removed. Some published FENs
    // carry a trailing `/` after the eighth rank; tolerate it by dropping an
    // empty final segment.
    let mut clean = String::with_capacity(board_part.len());
    let mut rank_from_top = 0u8;
    let segments: Vec<&str> = board_part.split('/').collect();
    let segments: &[&str] = match segments.split_last() {
        Some((last, init)) if last.is_empty() && init.len() == 8 => init,
        _ => &segments,
    };

    for (idx, rank_str) in segments.iter().enumerate() {
        if idx > 0 {
            clean.push('/');
        }
        let rank = 7u8.checked_sub(rank_from_top);
        // Accumulate in `usize` and saturate so an adversarial digit run cannot
        // overflow before the cleaned placement reaches the core board parser,
        // which performs the authoritative width validation.
        let mut file: usize = 0;
        let mut chars = rank_str.chars().peekable();
        while let Some(ch) = chars.next() {
            if let Some(skip) = ch.to_digit(10) {
                clean.push(ch);
                file = file.saturating_add(skip as usize);
                continue;
            }
            // A piece letter. A following `~` marks it as promoted.
            clean.push(ch);
            let promoted = matches!(chars.peek(), Some('~'));
            if promoted {
                chars.next();
                if let (Some(rank), true) = (rank, file < 8) {
                    if let (Some(rank_obj), Some(file_obj)) =
                        (crate::Rank::new(rank), crate::File::new(file as u8))
                    {
                        state
                            .promoted
                            .set(Square::from_file_rank(file_obj, rank_obj));
                    }
                }
            }
            file = file.saturating_add(1);
        }
        // Saturate the rank counter too: a placement with hundreds of `/`
        // separators must not overflow this `u8`. Once it pins at 7 the core
        // parser rejects the malformed rank count, so the exact value past the
        // eighth rank is immaterial.
        rank_from_top = rank_from_top.saturating_add(1);
    }

    Ok((clean, state))
}

/// Builds a placement-field parse error tagged with the offending text, reusing
/// the core board parser to obtain a real [`crate::ParseBoardError`].
fn parse_placement_err(bad: impl core::fmt::Display) -> crate::ParseBoardError {
    Board::from_fen_placement(&format!("{bad}"))
        .err()
        .unwrap_or(crate::ParseBoardError::TooFewRanks)
}

impl Crazyhouse {
    /// Renders the legal move `mv` as crazyhouse SAN.
    ///
    /// Drops render as `P@e4` / `N@f3` (with a `+`/`#` check suffix when the drop
    /// gives check or mate); every other move defers to the core
    /// [`Position::san`].
    #[must_use]
    pub fn san(&self, mv: &Move) -> String {
        if let Some(role) = mv.drop_role() {
            let mut s = String::with_capacity(5);
            // A pawn drop omits the role letter (`@e4`); other roles lead with it.
            if role != Role::Pawn {
                s.push(role.upper_char());
            }
            s.push('@');
            s.push_str(&mv.to().to_string());
            let after = self.play(mv);
            if after.is_check() {
                s.push(if after.legal_move_count() == 0 {
                    '#'
                } else {
                    '+'
                });
            }
            s
        } else {
            self.core().san(mv)
        }
    }

    /// Resolves a crazyhouse SAN string to the concrete legal [`Move`].
    ///
    /// Accepts the drop forms `@e4` (pawn) and `N@f3` (other roles), with any
    /// trailing `+`/`#`/`!`/`?` glyphs; non-drop SAN defers to the core
    /// [`Position::parse_san`].
    ///
    /// # Errors
    ///
    /// Returns [`SanError`] if the string is empty, malformed, names no legal
    /// move, or is ambiguous.
    pub fn parse_san(&self, s: &str) -> Result<Move, SanError> {
        let trimmed = s.trim_end_matches(['+', '#', '!', '?']);
        if let Some(at) = trimmed.find('@') {
            let (role_part, square_part) = trimmed.split_at(at);
            let square_part = &square_part[1..]; // drop the '@'
            let role = match role_part {
                "" => Role::Pawn,
                other => {
                    let mut chars = other.chars();
                    let (Some(ch), None) = (chars.next(), chars.next()) else {
                        return Err(SanError::Malformed);
                    };
                    Role::from_char(ch).ok_or(SanError::Malformed)?
                }
            };
            let square: Square = square_part.parse().map_err(|_| SanError::Malformed)?;
            let want = Move::drop(role, square);
            return if self.is_legal(&want) {
                Ok(want)
            } else {
                Err(SanError::Illegal)
            };
        }
        self.core().parse_san(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variant::perft_variant;
    use crate::{Color, Outcome, ParseUciError, Role};

    fn play_line(mut pos: Crazyhouse, ucis: &[&str]) -> Crazyhouse {
        for uci in ucis {
            let mv = pos.parse_uci(uci).expect("legal uci move");
            pos = pos.play(&mv);
        }
        pos
    }

    fn sq(s: &str) -> Square {
        s.parse().unwrap()
    }

    #[test]
    fn startpos_matches_standard_movegen() {
        let pos = Crazyhouse::startpos();
        assert_eq!(pos.variant_id(), VariantId::Crazyhouse);
        assert_eq!(pos.legal_moves().len(), 20);
        assert!(pos.outcome().is_none());
        assert_eq!(pos.state(), &CrazyhouseState::default());
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1"
        );
    }

    #[test]
    fn capture_fills_pocket_with_role() {
        // White rook on d1 captures the black knight on a1; White's pocket gains a
        // knight (by role, since the knight was never promoted).
        let pos: Crazyhouse = "4k3/8/8/8/8/8/8/n2RK3[] w - - 0 1".parse().unwrap();
        let after = play_line(pos, &["d1a1"]);
        assert_eq!(after.state().pocket(Color::White, Role::Knight), 1);
        assert_eq!(after.state().pocket(Color::White, Role::Pawn), 0);
    }

    #[test]
    fn drops_target_only_empty_squares() {
        // White holds a knight; it may drop on any empty square (62 of them: 64
        // minus the two kings), and no drop lands on an occupied square.
        let pos: Crazyhouse = "4k3/8/8/8/8/8/8/4K3[N] w - - 0 1".parse().unwrap();
        let drops: Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.is_drop())
            .collect();
        assert_eq!(drops.len(), 62);
        for d in &drops {
            assert!(pos.core().board().piece_at(d.to()).is_none());
        }
    }

    #[test]
    fn pawn_drops_excluded_from_back_ranks() {
        let pos: Crazyhouse = "4k3/8/8/8/8/8/8/4K3[P] w - - 0 1".parse().unwrap();
        let drops: Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.is_drop())
            .collect();
        // 62 empty squares minus the 8+8-? back-rank squares that are empty. Both
        // back ranks have a king, so 7 + 7 = 14 empty back-rank squares are
        // forbidden for a pawn; 62 - 14 = 48.
        assert_eq!(drops.len(), 48);
        for d in &drops {
            assert_ne!(d.to().rank(), crate::Rank::First);
            assert_ne!(d.to().rank(), crate::Rank::Eighth);
        }
    }

    #[test]
    fn drop_applies_and_decrements_pocket() {
        let pos: Crazyhouse = "4k3/8/8/8/8/8/8/4K3[N] w - - 0 1".parse().unwrap();
        let after = play_line(pos, &["N@e4"]);
        assert_eq!(
            after.core().board().piece_at(sq("e4")).map(|p| p.role),
            Some(Role::Knight)
        );
        assert_eq!(after.state().pocket(Color::White, Role::Knight), 0);
    }

    #[test]
    fn promoted_piece_reverts_to_pawn_when_captured() {
        // White pawn on b7 promotes to a queen on b8; Black's rook on a8 captures
        // it. Black's pocket should gain a *pawn*, not a queen.
        let pos: Crazyhouse = "r3k3/1P6/8/8/8/8/8/4K3[] w - - 0 1".parse().unwrap();
        let promoted = play_line(pos, &["b7b8q"]);
        assert!(promoted.state().promoted.contains(sq("b8")));
        assert_eq!(
            promoted.core().board().piece_at(sq("b8")).map(|p| p.role),
            Some(Role::Queen)
        );
        let captured = play_line(promoted, &["a8b8"]);
        assert_eq!(captured.state().pocket(Color::Black, Role::Pawn), 1);
        assert_eq!(captured.state().pocket(Color::Black, Role::Queen), 0);
        // The promoted bit is cleared once the piece is captured.
        assert!(!captured.state().promoted.contains(sq("b8")));
    }

    #[test]
    fn promoted_bit_follows_moving_piece() {
        // Promote a knight on b8 (no check), Black plays a waiting move, then the
        // promoted knight moves to c6; the promoted bit moves with it.
        let pos: Crazyhouse = "4k3/1P6/8/8/8/8/8/4K3[] w - - 0 1".parse().unwrap();
        let p = play_line(pos, &["b7b8n"]);
        assert!(p.state().promoted.contains(sq("b8")));
        let waited = play_line(p, &["e8f7"]);
        let moved = play_line(waited, &["b8c6"]);
        assert!(!moved.state().promoted.contains(sq("b8")));
        assert!(moved.state().promoted.contains(sq("c6")));
    }

    #[test]
    fn drop_can_deliver_checkmate() {
        // A back-rank smother: black king on h8 boxed in by its own pawns on g7,
        // h7; White drops a knight on g6 ... actually deliver a rook-drop mate.
        // White king on a1, black king on h8 with pawns g7/h7; White holds a
        // queen and drops Q@g8#? g8 adjacent to king. Use a clean rook-drop mate:
        // black king a8, white rook a-file control via drop on a-file with the
        // white king guarding. Simplest: K on c6, black K on a8, white holds a
        // rook; R@a-file? Need a real mate. Construct: black king a8, white king
        // a6, white holds a rook -> R@a... no. Use queen drop next to king with
        // king support.
        // Black Kh8, pawns g7 h7 (self-block), White Kf1, holds Q. Q@g8 is not
        // adjacent-safe. Instead drop knight to f7 forking? Keep it simple and
        // verify a check at least, plus a known mate below.
        let pos: Crazyhouse = "7k/5K2/8/8/8/8/8/8[Q] w - - 0 1".parse().unwrap();
        // Q@g7 mates: king on h8, queen g7 guarded by Kf7? f7 to g7 not adjacent
        // (f7-g7 are adjacent), so Qg7 is protected and covers g8,h7,h8 escape.
        let mv = pos.parse_san("Q@g7").expect("legal queen drop");
        let after = pos.play(&mv);
        assert!(after.is_check());
        assert_eq!(after.legal_move_count(), 0, "queen drop should mate");
        assert_eq!(
            after.outcome(),
            Some(Outcome::Decisive {
                winner: Color::White
            })
        );
    }

    #[test]
    fn drop_san_round_trips() {
        let pos: Crazyhouse = "4k3/8/8/8/8/8/8/4K3[NP] w - - 0 1".parse().unwrap();
        let knight = pos.parse_san("N@e4").unwrap();
        assert_eq!(knight, Move::drop(Role::Knight, sq("e4")));
        assert_eq!(pos.san(&knight), "N@e4");
        let pawn = pos.parse_san("@e4").unwrap();
        assert_eq!(pawn, Move::drop(Role::Pawn, sq("e4")));
        assert_eq!(pos.san(&pawn), "@e4");
        // UCI form is also accepted.
        assert_eq!(pos.parse_uci("N@e4").unwrap(), knight);
    }

    #[test]
    fn parse_rejects_non_ascii_without_panic() {
        let pos: Crazyhouse = "4k3/8/8/8/8/8/8/4K3[NP] w - - 0 1".parse().unwrap();
        // Non-ASCII in the UCI drop form and the standard UCI form must be
        // rejected, never panic on a split UTF-8 boundary.
        for s in [
            "N@e\u{e9}",    // multi-byte char in the drop square
            "\u{e9}@e4",    // multi-byte role marker
            "e2e\u{e9}",    // multi-byte char straddling a square boundary
            "\u{1f600}@e4", // emoji role
            "N@\u{301}e4",  // combining mark
        ] {
            assert_eq!(pos.parse_uci(s).unwrap_err(), ParseUciError::Malformed);
            // SAN drop path must likewise not panic.
            assert!(pos.parse_san(s).is_err(), "{s:?} should be an error");
        }
    }

    #[test]
    fn fen_round_trips_pockets_and_promoted() {
        for fen in [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1",
            "4k3/8/8/8/8/8/8/4K3[PPnn] w - - 0 1",
            "r1bqk2r/pppp1ppp/2n1p3/4P3/1b1Pn3/2NB1N2/PPP2PPP/R1BQK2R[] b KQkq - 0 1",
        ] {
            let pos: Crazyhouse = fen.parse().unwrap();
            assert_eq!(pos.to_fen(), fen, "round trip for {fen}");
        }
    }

    #[test]
    fn fen_reads_promoted_marker() {
        // A promoted queen on b7, marked with `~`.
        let pos: Crazyhouse = "4k3/1Q~6/8/8/8/8/8/4K3[] w - - 0 1".parse().unwrap();
        assert!(pos.state().promoted.contains(sq("b7")));
        assert_eq!(pos.to_fen(), "4k3/1Q~6/8/8/8/8/8/4K3[] w - - 0 1");
    }

    #[test]
    fn rejects_oversized_skip_run_without_panicking() {
        // Regression for issue #47: the crazyhouse placement reader strips the
        // `[...]`/`~` markers and accumulates a file counter before delegating
        // to the core board parser. An adversarial digit run must saturate
        // rather than overflow the counter and panic.
        let long = format!("{}/8/8/8/8/8/8/8[] w - - 0 1", "9".repeat(10_000));
        let parsed: Result<Crazyhouse, _> = long.parse();
        assert!(parsed.is_err());

        // The same on the bare placement field, plus a `~` after the run.
        let mut split = String::from("9");
        split.push_str(&"9".repeat(300));
        split.push_str("Q~/8/8/8/8/8/8/8[]");
        let parsed: Result<Crazyhouse, _> = format!("{split} w - - 0 1").parse();
        assert!(parsed.is_err());

        // Hundreds of `/` separators must not overflow the per-rank counter
        // (a `u8`) as the reader walks every segment. Surfaced by the #39
        // fen_roundtrip fuzz target.
        let many_ranks = format!("{}[] w - - 0 1", "8/".repeat(400));
        let parsed: Result<Crazyhouse, _> = many_ranks.parse();
        assert!(parsed.is_err());
    }

    #[test]
    fn pockets_affect_zobrist() {
        let empty: Crazyhouse = "4k3/8/8/8/8/8/8/4K3[] w - - 0 1".parse().unwrap();
        let one_pawn: Crazyhouse = "4k3/8/8/8/8/8/8/4K3[P] w - - 0 1".parse().unwrap();
        let one_black: Crazyhouse = "4k3/8/8/8/8/8/8/4K3[p] w - - 0 1".parse().unwrap();
        let two_pawn: Crazyhouse = "4k3/8/8/8/8/8/8/4K3[PP] w - - 0 1".parse().unwrap();
        assert_ne!(empty.zobrist(), one_pawn.zobrist());
        assert_ne!(one_pawn.zobrist(), one_black.zobrist());
        assert_ne!(one_pawn.zobrist(), two_pawn.zobrist());
        // Empty pocket matches the plain core key (no contribution).
        assert_eq!(empty.zobrist(), empty.core().zobrist());
        // Equal pockets hash equal.
        let one_pawn2: Crazyhouse = "4k3/8/8/8/8/8/8/4K3[P] w - - 0 1".parse().unwrap();
        assert_eq!(one_pawn.zobrist(), one_pawn2.zobrist());
    }

    #[test]
    fn promoted_mask_affects_zobrist() {
        let plain: Crazyhouse = "4k3/1Q6/8/8/8/8/8/4K3[] w - - 0 1".parse().unwrap();
        let promoted: Crazyhouse = "4k3/1Q~6/8/8/8/8/8/4K3[] w - - 0 1".parse().unwrap();
        assert_ne!(plain.zobrist(), promoted.zobrist());
    }

    #[test]
    fn hash_matches_after_drop_and_capture() {
        // Capture (fills pocket), then drop: reparsing the FEN must reproduce the
        // same key (recomputed from scratch, including the pocket hash) and state.
        let pos: Crazyhouse = "4k3/8/8/8/8/8/8/n2RK3[] w - - 0 1".parse().unwrap();
        let after = play_line(pos, &["d1a1", "e8d8", "N@e4"]);
        let reparsed: Crazyhouse = after.to_fen().parse().unwrap();
        assert_eq!(after.zobrist(), reparsed.zobrist());
        assert_eq!(after.state(), reparsed.state());
    }

    #[test]
    fn perft_startpos_shallow() {
        // Standard start: with empty pockets and no drops yet, crazyhouse perft
        // matches standard chess at shallow depth (the shakmaty `zh-middlegame`
        // fixture exercises drops at depth; see tests/perft_crazyhouse.rs).
        let pos = Crazyhouse::startpos();
        assert_eq!(perft_variant(&pos, 1), 20);
        assert_eq!(perft_variant(&pos, 2), 400);
        assert_eq!(perft_variant(&pos, 3), 8902);
    }
}
