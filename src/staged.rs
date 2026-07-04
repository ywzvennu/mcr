//! Staged (lazy) move generation for move ordering.
//!
//! An alpha-beta search wants to try moves in roughly best-first order so that a
//! beta cutoff happens on the first move as often as possible — and, crucially,
//! it wants to *stop pulling moves* the instant a cutoff fires, without having
//! paid to generate the moves it never tried. [`MoveGenerator`] provides exactly
//! that: a pull-style iterator over a position's legal moves that yields them in
//! stages and only does the work for a stage when the caller first reaches it.
//!
//! # Stages
//!
//! Moves come out in this order:
//!
//! 1. **Transposition / hash move.** If the caller supplies a priority move
//!    (typically the best move found for this position at a shallower depth) and
//!    it is legal here, it is yielded first and then suppressed from the later
//!    stages so it is never repeated.
//! 2. **Captures and capturing promotions**, ordered most-valuable-victim first
//!    ([`Position::victim_value`] — an ordinary capture by the value of the piece
//!    on the destination, en passant as a pawn). This is a cheap MVV key;
//!    refining it to full MVV-LVA or SEE ([`Position::see`]) is a drop-in change
//!    to the sort and is left as a follow-up. En-passant and capturing promotions
//!    sort in with the rest.
//! 3. **Quiet moves**: non-capturing pushes, double pushes, non-capturing
//!    promotions, and castles, in generator order.
//!
//! The captures stage is generated lazily on the first capture pull and the
//! quiets stage on the first quiet pull, each into its own stack-backed
//! [`MoveList`]. A search that cuts off on a capture therefore never generates
//! the quiet moves at all.
//!
//! # The set invariant
//!
//! The moves yielded across all stages are exactly the moves of
//! [`Position::legal_moves`] — same set, no duplicates, no omissions — for
//! standard chess and every variant. Each stage runs the *existing* legal
//! generator (filtered to captures or quiets), and the TT move is yielded only
//! when it is itself legal and is then removed from the later stages, so nothing
//! is added or dropped relative to `legal_moves`.

use crate::movelist::MoveList;
use crate::{Move, Position};

/// Which phase of [`MoveGenerator`] is being produced next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// Yield the priority (TT) move, if one was supplied and is legal.
    Tt,
    /// Generate and yield the capturing moves, ordered by victim value.
    Captures,
    /// Generate and yield the quiet moves.
    Quiets,
    /// All stages exhausted.
    Done,
}

/// A staged, lazy iterator over a position's legal moves for move ordering.
///
/// Built by [`Position::staged_moves`] (and [`crate::VariantPosition::staged_moves`]).
/// Pull moves with [`MoveGenerator::next`] or via the [`Iterator`] impl; see the
/// module docs for the stage order and the guarantee that the yielded
/// set equals [`Position::legal_moves`].
///
/// ```
/// use mcr::Position;
/// use std::collections::BTreeSet;
///
/// let pos = Position::startpos();
/// let staged: BTreeSet<_> = pos.staged_moves(None).collect();
/// let legal: BTreeSet<_> = pos.legal_moves().into_iter().collect();
/// assert_eq!(staged, legal);
/// ```
#[derive(Debug)]
pub struct MoveGenerator<'a> {
    pos: &'a Position,
    /// The supplied priority move, taken (set to `None`) once handled so it is
    /// emitted at most once and skipped in the capture/quiet stages.
    tt_move: Option<Move>,
    /// Whether a TT move was actually yielded — it is then excluded from the
    /// later stages by equality.
    tt_yielded: Option<Move>,
    stage: Stage,
    /// The current stage's moves and the read cursor into them. Filled lazily
    /// when each stage is first entered.
    buffer: MoveList,
    cursor: usize,
}

impl<'a> MoveGenerator<'a> {
    /// Creates a staged generator over `pos`, optionally trying `tt_move` first.
    pub(crate) fn new(pos: &'a Position, tt_move: Option<Move>) -> MoveGenerator<'a> {
        MoveGenerator {
            pos,
            tt_move,
            tt_yielded: None,
            stage: Stage::Tt,
            buffer: MoveList::new(),
            cursor: 0,
        }
    }

    /// Whether `mv` was the TT move already yielded and so must be skipped now.
    #[inline]
    fn is_skipped(&self, mv: Move) -> bool {
        self.tt_yielded == Some(mv)
    }

    /// Fills `self.buffer` with the legal captures of `pos`, ordered
    /// most-valuable-victim first, and resets the cursor.
    fn load_captures(&mut self) {
        self.buffer.clear();
        self.pos.legal_captures_into(&mut self.buffer);
        sort_by_victim(self.pos, &mut self.buffer);
        self.cursor = 0;
    }

    /// Fills `self.buffer` with the legal quiet moves of `pos` and resets the
    /// cursor.
    fn load_quiets(&mut self) {
        self.buffer.clear();
        self.pos.legal_quiets_into(&mut self.buffer);
        self.cursor = 0;
    }

    /// Returns the next move in the current buffer that is not the
    /// already-yielded TT move, advancing the cursor.
    fn next_from_buffer(&mut self) -> Option<Move> {
        while self.cursor < self.buffer.len() {
            let mv = self.buffer[self.cursor];
            self.cursor += 1;
            if !self.is_skipped(mv) {
                return Some(mv);
            }
        }
        None
    }

    /// Pulls the next legal move in staged order, or `None` when exhausted. See
    /// the module docs for the stage order.
    pub fn next_move(&mut self) -> Option<Move> {
        loop {
            match self.stage {
                Stage::Tt => {
                    self.stage = Stage::Captures;
                    if let Some(mv) = self.tt_move.take() {
                        // Only emit it if it is actually legal here; an illegal or
                        // stale TT move is simply ignored and the captures stage
                        // (which re-derives legality) covers the real moves.
                        if self.pos.is_legal(&mv) {
                            self.tt_yielded = Some(mv);
                            return Some(mv);
                        }
                    }
                }
                Stage::Captures => {
                    if self.cursor == 0 && self.buffer.is_empty() {
                        self.load_captures();
                    }
                    if let Some(mv) = self.next_from_buffer() {
                        return Some(mv);
                    }
                    self.stage = Stage::Quiets;
                    self.buffer.clear();
                    self.cursor = 0;
                }
                Stage::Quiets => {
                    if self.cursor == 0 {
                        self.load_quiets();
                    }
                    if let Some(mv) = self.next_from_buffer() {
                        return Some(mv);
                    }
                    self.stage = Stage::Done;
                }
                Stage::Done => return None,
            }
        }
    }
}

impl Iterator for MoveGenerator<'_> {
    type Item = Move;

    #[inline]
    fn next(&mut self) -> Option<Move> {
        self.next_move()
    }
}

/// Sorts `moves` in place so the highest victim value comes first — the captures
/// stage's MVV ordering. A small insertion sort keeps it allocation-free over the
/// stack [`MoveList`] and is fast for the handful of captures a position has;
/// equal-victim ties keep generator order (a stable pass).
pub(crate) fn sort_by_victim(pos: &Position, moves: &mut MoveList) {
    let len = moves.len();
    for i in 1..len {
        let mv = moves[i];
        let key = pos.victim_value(&mv);
        let mut j = i;
        while j > 0 && pos.victim_value(&moves[j - 1]) < key {
            moves.set(j, moves[j - 1]);
            j -= 1;
        }
        moves.set(j, mv);
    }
}

impl Position {
    /// A staged, lazy iterator over this position's legal moves for move
    /// ordering: an optional priority (TT/hash) move first, then captures ordered
    /// by victim value, then quiets — each stage generated only when first
    /// reached. The yielded set equals [`Position::legal_moves`]. See
    /// [`MoveGenerator`] for the full contract.
    ///
    /// ```
    /// use mcr::Position;
    ///
    /// let pos = Position::startpos();
    /// // No legal captures from the start, so the first move is a quiet.
    /// let mut gen = pos.staged_moves(None);
    /// assert!(gen.next().is_some());
    /// ```
    #[must_use]
    pub fn staged_moves(&self, tt_move: Option<Move>) -> MoveGenerator<'_> {
        MoveGenerator::new(self, tt_move)
    }
}
