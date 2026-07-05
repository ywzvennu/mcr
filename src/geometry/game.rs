//! [`GenericGame`]: the opt-in history-recording wrapper that turns a sequence of
//! [`GenericPosition`] plies into a result under the **history-dependent** terminal
//! rules a single position cannot see — repetition (Xiangqi / Janggi),
//! Shogi **sennichite** and its **perpetual-check** exception, and the Makruk /
//! Cambodian **counting** countdown.
//!
//! # Why a separate wrapper
//!
//! [`GenericPosition`] is deliberately **history-free**: it answers
//! [`outcome`](GenericPosition::outcome) from the board and state alone, so perft
//! never allocates or maintains a position history and stays **byte-identical**.
//! The history-dependent rules therefore live here, exactly as the concrete
//! engine splits [`Position`](crate::Position) from [`Game`](crate::Game).
//!
//! A `GenericGame` records a [`repetition_key`](GenericPosition::repetition_key)
//! (and whether the side to move was in check) for every position that has
//! occurred — but **only** when the variant opts in through
//! [`WideVariant::tracks_repetition`] (and a counting state only when
//! [`WideVariant::counting_rule`]). For every other variant the history stays
//! empty, nothing is recorded, and the wrapper is a thin driver over
//! [`GenericPosition`] that just forwards [`outcome`](GenericPosition::outcome).
//!
//! # Perpetual check and chase
//!
//! When a repetition is found and the variant's
//! [`perpetual_check_loses`](WideVariant::perpetual_check_loses) is on, the
//! wrapper walks the repeated cycle: if one side delivered check on **every** one
//! of its moves through the cycle, that side is the perpetual checker and **loses**
//! (the win goes to the side that was being checked).
//!
//! Xiangqi additionally forbids perpetual **chase**
//! ([`perpetual_chase_loses`](WideVariant::perpetual_chase_loses), the AXF rule):
//! a side that, on **every** one of its moves through the repeated cycle, makes a
//! qualifying *chase* — a fresh attack on an enemy piece that is either left
//! **unprotected** or is **value-superior** to the attacker (a Horse / Cannon
//! attacking a Chariot, or an Elephant / Advisor attacking a Chariot / Cannon /
//! Horse) — loses, exactly as a perpetual checker does. Perpetual check is scored
//! first; a mutual perpetual (both sides) is a draw. Otherwise the repetition is
//! the ordinary [`repetition_draw_reason`](WideVariant::repetition_draw_reason)
//! draw (Sennichite for Shogi, Repetition elsewhere).
//!
//! The chase detector reproduces Fairy-Stockfish's **direct-attack** chase
//! (`Position::chased`): the moved piece's new attacks, the King / Soldier and
//! same-type-mutual exclusions, the Rook / Cannon "attack along the moved line is
//! not new" exclusion, the value-superior tiers, and the unprotected-victim test
//! (defenders recomputed with the attacker lifted off the board). It is a faithful
//! **subset**: it does not model FSF's discovered-attack chases, the impaired-horse
//! and flying-general-pin refinements, or the exact single-victim identity tracked
//! across the cycle (mcr instead requires a qualifying chase on **every** cycle
//! ply, the same structural rule as perpetual check). These cover the dominant
//! cases (a Chariot/Horse/Cannon harrying one piece) and agree with FSF on them.
//!
//! # Attack repetition (Chu / Dai large shogi)
//!
//! Chu and Dai Shogi layer a second chase model on sennichite
//! ([`attack_repetition_loses`](WideVariant::attack_repetition_loses)): at the
//! fourth occurrence, if through the repeated cycle **one side attacked** enemy
//! pieces (any threat on any non-royal, "however futile") **and the other side
//! attacked nothing**, the attacking side must break the pattern or **loses** — the
//! chessvariants Chu ruleset's asymmetric attack rule. Perpetual **check** (an
//! attack on the enemy royal) is a separate, higher-priority rule and is scored
//! first; the attack test excludes the enemy King and Crown Prince.
//!
//! Two points differ from that source's letter, resolved deliberately (there is
//! **no** machine oracle — HaChu only exercises captures at shallow depth and
//! segfaults on Tenjiku — so these are hand-derived choices, not validated):
//!
//! * **Attack strength.** The test applies no value/protection filter (unlike the
//!   Xiangqi chase above): the moved piece attacking *any* enemy non-royal counts.
//! * **Ambiguous sub-cases fall back to the draw.** When **both** sides attacked in
//!   the cycle, sources disagree (chessvariants forbids the repeating move outright;
//!   others draw); mcr draws. The "one side started passing" sub-rule (Chu's Lion
//!   `jitto` pass) is likewise **not** modelled here and draws. Only the
//!   well-characterized one-sided-attack core is decisive. Tenjiku, whose repetition
//!   convention is debated and unoracled, keeps only the base sennichite /
//!   perpetual-check of issue #471 and does **not** enable this rule.
//!
//! # Bikjang (Janggi)
//!
//! The Janggi **bikjang** draw fires when the two generals face each other down an
//! open line in **two consecutive** positions — Fairy-Stockfish's
//! `st->bikjang && st->previous->bikjang`. The wrapper records each position's
//! facing flag and draws ([`WideEndReason::Bikjang`]) when the current and previous
//! recorded positions both face, implementing the rule directly from its own
//! history rather than relying on the move generator.
//!
//! The two formulations coincide: the move generator forbids **any non-pass move
//! that leaves the generals facing** once they already face (FSF's legality at
//! `position.cpp` — a move whose post-occupancy keeps the rook-line between the
//! kings open is illegal unless it is a pass), so the only way to carry a facing
//! into the next ply is to **pass**. [`GenericPosition::end_reason`] already labels
//! that pass terminal (the facing side, after the opponent's pass, has no legal
//! move) a bikjang for a bare position; this wrapper additionally derives the same
//! verdict from the recorded facing flags.
//!
//! # Counting (Makruk / Cambodian / ASEAN)
//!
//! The counting endgame is reproduced **exactly** from Fairy-Stockfish's
//! `count_limit` / `do_move` / `is_optional_game_end` state machine (validated
//! against the FSF binary's echoed counting FEN field). The wrapper keeps the two
//! counters FSF does — a `limit` and a `ply`, both in plies — updating them on
//! every move and drawing ([`WideEndReason::CountingDraw`]) when `ply > limit`.
//! Both **board-honour** (the count while the losing side still has material) and
//! the material-scaled **pieces-honour** countdown (once it is a lone king) are
//! modelled, with the correct table per [`WideCountingRule`].

use alloc::vec::Vec;

use super::position::{GenericPosition, WideOutcome};
use super::variant::{WideCountingRule, WideEndReason, WideVariant};
use super::{Bitboard, Board, Geometry, Square, WideMove, WideRole};
use crate::Color;

/// The number of plies the Makruk / Cambodian **board-honour** count runs while the
/// losing side still has material: sixty-four full moves expressed in plies. The
/// pieces-honour countdown (a lone king) is shorter and material-scaled; see the
/// [module docs](self) and [`WideCountingRule`].
pub const COUNTING_LIMIT_PLIES: u16 = 128;

/// One recorded position in a [`GenericGame`]'s history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HistoryEntry {
    /// The position's [`repetition_key`](GenericPosition::repetition_key).
    key: u64,
    /// The side to move at this position.
    turn: Color,
    /// Whether the side to move is in check here (i.e. the move that produced this
    /// position delivered check). Read back by the perpetual-check adjudication.
    in_check: bool,
    /// Whether the move that produced this position was a qualifying
    /// repetition-**attack** for the variant's chase model — the Xiangqi
    /// value/protection [chase](WideVariant::perpetual_chase_loses), or the Chu/Dai
    /// large-shogi [attack](WideVariant::attack_repetition_loses) on any non-royal
    /// enemy piece. Read back by the perpetual-chase / attack-repetition
    /// adjudication. Always `false` for the seed entry and for variants with neither
    /// chase model.
    chase: bool,
    /// Whether the two generals **face** each other in this position (Janggi
    /// bikjang). Always `false` for variants without
    /// [`has_bikjang`](WideVariant::has_bikjang).
    facing: bool,
}

/// The Makruk / Cambodian / ASEAN counting state, mirroring Fairy-Stockfish's two
/// `StateInfo` counters (both in plies): the game is a [`CountingDraw`] once
/// `ply > limit`.
///
/// [`CountingDraw`]: WideEndReason::CountingDraw
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Counting {
    /// FSF `countingLimit`: twice the limit in full moves, or `0` when no count is
    /// active.
    limit: u16,
    /// FSF `countingPly`: how far the count has progressed.
    ply: u16,
}

/// The error returned when an illegal move is passed to [`GenericGame::play`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WideIllegalMove(pub WideMove);

impl core::fmt::Display for WideIllegalMove {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "illegal move in this position")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for WideIllegalMove {}

/// A generic game in progress: a [`GenericPosition`] plus the recorded history the
/// history-dependent terminal rules need (repetition / sennichite / perpetual
/// check / counting).
///
/// The history is recorded only for variants that opt in
/// ([`WideVariant::tracks_repetition`] / [`WideVariant::counting_rule`]); for every
/// other variant the wrapper merely forwards [`GenericPosition::outcome`], so it is
/// as cheap as the bare position and the perft path is untouched.
#[derive(Debug, Clone)]
pub struct GenericGame<G: Geometry, V: WideVariant<G>> {
    position: GenericPosition<G, V>,
    /// The recorded history (oldest first, current last); empty unless the variant
    /// tracks repetition.
    history: Vec<HistoryEntry>,
    /// The counting state (Makruk / Cambodian / ASEAN); inert (`limit == 0`) unless
    /// the variant counts and a triggering position has been reached.
    counting: Counting,
    /// The current position's incrementally-maintained Zobrist key
    /// ([`GenericPosition::zobrist`]). Maintained across [`play`](Self::play) only
    /// for variants that [track repetition](WideVariant::tracks_repetition) (the
    /// only ones that consult it); inert and unread otherwise. It always equals a
    /// from-scratch `self.position.zobrist()` recompute.
    key: u64,
}

impl<G: Geometry, V: WideVariant<G>> GenericGame<G, V> {
    /// Starts a game from `position`, seeding the history (when the variant tracks
    /// repetition) and the counting state (when it counts) from it.
    #[must_use]
    pub fn new(position: GenericPosition<G, V>) -> Self {
        // Seed the incremental Zobrist key from a from-scratch compute (a no-op cost
        // for the non-tracking variants that never read it again).
        let key = position.zobrist();
        let mut history = Vec::new();
        if V::tracks_repetition() {
            // The seed position has no preceding move, so it neither checks nor
            // chases; record only its facing flag (for bikjang).
            history.push(HistoryEntry {
                key,
                turn: position.turn(),
                in_check: position.is_check(),
                chase: false,
                facing: V::has_bikjang() && position.is_facing_generals(),
            });
        }
        GenericGame {
            position,
            history,
            // No count is active until a triggering move is played (FSF likewise
            // starts a fresh game with `countingLimit == 0`).
            counting: Counting::default(),
            key,
        }
    }

    /// Starts a game from the variant's starting position.
    #[must_use]
    pub fn startpos() -> Self {
        Self::new(GenericPosition::startpos())
    }

    /// The current position.
    #[must_use]
    #[inline]
    pub fn position(&self) -> &GenericPosition<G, V> {
        &self.position
    }

    /// The legal moves in the current position.
    #[must_use]
    #[inline]
    pub fn legal_moves(&self) -> Vec<WideMove> {
        self.position.legal_moves()
    }

    /// Applies `mv`, advancing the game and recording the new position in the
    /// history / counting state.
    ///
    /// # Errors
    ///
    /// Returns [`WideIllegalMove`] (and leaves the game unchanged) if `mv` is not
    /// legal in the current position.
    pub fn play(&mut self, mv: &WideMove) -> Result<(), WideIllegalMove> {
        if !self.position.legal_moves().iter().any(|m| m == mv) {
            return Err(WideIllegalMove(*mv));
        }
        // Snapshot the pre-move board for the counting material deltas (capture /
        // promotion detection), exactly as Fairy-Stockfish's `do_move` reads them.
        let before = *self.position.board();
        if V::tracks_repetition() {
            // Maintain the Zobrist key incrementally: XOR the old state half out,
            // apply the move in place (capturing the board edits in an `Undo`), then
            // XOR the board delta and the new state half in. This reproduces a
            // from-scratch `zobrist()` recompute without rescanning the board, and
            // touches only this repetition-tracking path — never the bare perft
            // make/unmake — so perft stays byte-identical.
            let old_state = self.position.zobrist_state_part();
            let undo = self.position.apply_with_undo(mv);
            self.key ^= self.position.zobrist_board_delta(&undo)
                ^ old_state
                ^ self.position.zobrist_state_part();
            // The `chase` flag records, per ply, the qualifying repetition-attack for
            // whichever chase model this variant uses (a variant enables at most one):
            // the Xiangqi value/protection chase, or the Chu/Dai large-shogi "attacks
            // any non-royal" test.
            let chase = if V::perpetual_chase_loses() {
                Self::chase_targets(&self.position, mv.from::<G>(), mv.to::<G>()).is_some()
            } else if V::attack_repetition_loses() {
                Self::attacks_nonroyal(&self.position, mv.to::<G>())
            } else {
                false
            };
            self.history.push(HistoryEntry {
                key: self.key,
                turn: self.position.turn(),
                in_check: self.position.is_check(),
                chase,
                facing: V::has_bikjang() && self.position.is_facing_generals(),
            });
        } else {
            self.position = self.position.play(mv);
        }
        if let Some(rule) = V::counting_rule() {
            self.counting = Self::update_counting(self.counting, &before, &self.position, rule);
        }
        Ok(())
    }

    /// How many times the current position has occurred (always ≥ 1 when the
    /// variant tracks repetition; `0` otherwise, since nothing is recorded).
    #[must_use]
    pub fn repetition_count(&self) -> usize {
        if !V::tracks_repetition() {
            return 0;
        }
        self.history.iter().filter(|e| e.key == self.key).count()
    }

    /// A stable 64-bit **Zobrist** position key for the current position — the same
    /// value as [`self.position().zobrist()`](GenericPosition::zobrist), suitable for
    /// opening books and position deduplication. For a
    /// [repetition-tracking](WideVariant::tracks_repetition) variant this is the
    /// incrementally-maintained key (no recompute); otherwise it is computed on
    /// demand.
    #[must_use]
    #[inline]
    pub fn position_key(&self) -> u64 {
        if V::tracks_repetition() {
            self.key
        } else {
            self.position.zobrist()
        }
    }

    /// The reason the game has ended, or `None` if it is still in progress.
    ///
    /// The union of the single-position reasons ([`GenericPosition::end_reason`])
    /// and the history-dependent ones: repetition / sennichite / perpetual-check /
    /// perpetual-chase, the two-ply bikjang draw, and the counting draw.
    #[must_use]
    pub fn end_reason(&self) -> Option<WideEndReason> {
        if let Some(reason) = self.position.end_reason() {
            return Some(reason);
        }
        if V::tracks_repetition() {
            if let Some((reason, _)) = self.repetition_adjudication() {
                return Some(reason);
            }
        }
        if self.bikjang_draw() {
            return Some(WideEndReason::Bikjang);
        }
        if V::counting_rule().is_some() && self.counting_elapsed() {
            return Some(WideEndReason::CountingDraw);
        }
        None
    }

    /// The game result, or `None` if the game is still in progress.
    #[must_use]
    pub fn outcome(&self) -> Option<WideOutcome> {
        if let Some(outcome) = self.position.outcome() {
            return Some(outcome);
        }
        if V::tracks_repetition() {
            if let Some((_, outcome)) = self.repetition_adjudication() {
                return Some(outcome);
            }
        }
        if self.bikjang_draw() {
            return Some(WideOutcome::Draw);
        }
        if V::counting_rule().is_some() && self.counting_elapsed() {
            return Some(WideOutcome::Draw);
        }
        None
    }

    /// Whether the game is a **bikjang** draw (Janggi): the two generals face each
    /// other in the current and the immediately preceding recorded positions —
    /// Fairy-Stockfish's `st->bikjang && st->previous->bikjang`.
    fn bikjang_draw(&self) -> bool {
        if !V::has_bikjang() {
            return false;
        }
        let n = self.history.len();
        n >= 2 && self.history[n - 1].facing && self.history[n - 2].facing
    }

    /// Whether the game has ended (decisively or drawn).
    #[must_use]
    pub fn is_over(&self) -> bool {
        self.outcome().is_some()
    }

    /// Whether the game has ended in a draw.
    #[must_use]
    pub fn is_draw(&self) -> bool {
        matches!(self.outcome(), Some(WideOutcome::Draw))
    }

    /// Whether the counting countdown has elapsed: FSF's `countingPly > countingLimit`
    /// with an active limit.
    fn counting_elapsed(&self) -> bool {
        self.counting.limit != 0 && self.counting.ply > self.counting.limit
    }

    /// Adjudicates the current position's repetition, if any: the
    /// `(reason, outcome)` pair when the position has recurred enough times under
    /// [`WideVariant::repetition_fold`], else `None`.
    ///
    /// A perpetual check (one side checking on every move through the repeated
    /// cycle) under [`WideVariant::perpetual_check_loses`] is a loss for the
    /// checker; a perpetual chase under
    /// [`WideVariant::perpetual_chase_loses`] is a loss for the chaser (perpetual
    /// check is scored first); otherwise the recurrence is the variant's repetition
    /// draw.
    fn repetition_adjudication(&self) -> Option<(WideEndReason, WideOutcome)> {
        let key = self.key;
        // Index of the earliest occurrence of the current key.
        let first = self.history.iter().position(|e| e.key == key)?;
        let count = self.history[first..]
            .iter()
            .filter(|e| e.key == key)
            .count();
        if count < V::repetition_fold() {
            return None;
        }
        if V::perpetual_check_loses() {
            if let Some(checker) = self.perpetual_checker(first) {
                // The perpetual checker loses; the side it was checking wins.
                return Some((
                    WideEndReason::PerpetualCheckLoss,
                    WideOutcome::Decisive {
                        winner: checker.opposite(),
                    },
                ));
            }
        }
        if V::perpetual_chase_loses() {
            if let Some(chaser) = self.perpetual_chaser(first) {
                // The perpetual chaser loses; the side it was chasing wins.
                return Some((
                    WideEndReason::PerpetualChaseLoss,
                    WideOutcome::Decisive {
                        winner: chaser.opposite(),
                    },
                ));
            }
        }
        if V::attack_repetition_loses() {
            if let Some(attacker) = self.attack_repetition_loser(first) {
                // The attacking side must break the pattern or lose; the side it was
                // attacking wins.
                return Some((
                    WideEndReason::AttackRepetitionLoss,
                    WideOutcome::Decisive {
                        winner: attacker.opposite(),
                    },
                ));
            }
        }
        Some((V::repetition_draw_reason(), WideOutcome::Draw))
    }

    /// The side that delivered a qualifying **chase** on **every** one of its moves
    /// through the repeated cycle starting at `first` (the perpetual chaser), if
    /// exactly one side did. The structural analogue of [`perpetual_checker`]: a
    /// move made at position `i` chased iff the position at `i + 1` carries the
    /// `chase` flag.
    ///
    /// [`perpetual_checker`]: Self::perpetual_checker
    fn perpetual_chaser(&self, first: usize) -> Option<Color> {
        let h = &self.history;
        let last = h.len() - 1;
        let mut white_moves = 0u32;
        let mut black_moves = 0u32;
        let mut white_all_chase = true;
        let mut black_all_chase = true;
        for i in first..last {
            let chased = h[i + 1].chase;
            match h[i].turn {
                Color::White => {
                    white_moves += 1;
                    white_all_chase &= chased;
                }
                Color::Black => {
                    black_moves += 1;
                    black_all_chase &= chased;
                }
            }
        }
        let white_perp = white_moves > 0 && white_all_chase;
        let black_perp = black_moves > 0 && black_all_chase;
        match (white_perp, black_perp) {
            (true, false) => Some(Color::White),
            (false, true) => Some(Color::Black),
            // Neither chased throughout, or (a mutual chase) both did: not a clean
            // perpetual chase — fall back to the ordinary repetition draw.
            _ => None,
        }
    }

    /// The side that must break the **large-shogi attack-repetition** (Chu / Dai
    /// "chase") or lose — the side that **attacked** an enemy non-royal piece on at
    /// least one of its moves through the repeated cycle starting at `first` while
    /// the other side attacked on **none** of its moves — or `None` if the cycle is
    /// not a clean one-sided attack.
    ///
    /// This is the chessvariants Chu ruleset's asymmetric test ("one side attacked
    /// pieces with any of his moves, and the other doesn't"): an **OR** over each
    /// side's moves, not the every-move **AND** of [`perpetual_chaser`] /
    /// [`perpetual_checker`]. When neither side attacked (a quiet repetition) the
    /// result is the ordinary sennichite draw; when **both** attacked — a case whose
    /// exact adjudication is genuinely ambiguous across sources (chessvariants would
    /// forbid the repeating move outright; other rule-sets differ) — mcr
    /// conservatively also falls back to the draw rather than guess, so only the
    /// well-characterized one-sided-attack core is decisive.
    ///
    /// [`perpetual_chaser`]: Self::perpetual_chaser
    /// [`perpetual_checker`]: Self::perpetual_checker
    fn attack_repetition_loser(&self, first: usize) -> Option<Color> {
        let h = &self.history;
        let last = h.len() - 1;
        let mut white_moves = 0u32;
        let mut black_moves = 0u32;
        let mut white_attacked = false;
        let mut black_attacked = false;
        for i in first..last {
            let attacked = h[i + 1].chase;
            match h[i].turn {
                Color::White => {
                    white_moves += 1;
                    white_attacked |= attacked;
                }
                Color::Black => {
                    black_moves += 1;
                    black_attacked |= attacked;
                }
            }
        }
        // A clean adjudication needs both sides to have moved in the cycle.
        if white_moves == 0 || black_moves == 0 {
            return None;
        }
        match (white_attacked, black_attacked) {
            (true, false) => Some(Color::White),
            (false, true) => Some(Color::Black),
            // Neither attacked (quiet repetition → draw) or both attacked (ambiguous
            // → draw): not a clean one-sided attack.
            _ => None,
        }
    }

    /// The side that delivered check on **every** one of its moves through the
    /// repeated cycle starting at `first` (the perpetual checker), if exactly one
    /// side did. A move made at position `i` gave check iff the position at `i + 1`
    /// has its side to move in check.
    fn perpetual_checker(&self, first: usize) -> Option<Color> {
        let h = &self.history;
        let last = h.len() - 1;
        let mut white_moves = 0u32;
        let mut black_moves = 0u32;
        let mut white_all_check = true;
        let mut black_all_check = true;
        for i in first..last {
            let gave_check = h[i + 1].in_check;
            match h[i].turn {
                Color::White => {
                    white_moves += 1;
                    white_all_check &= gave_check;
                }
                Color::Black => {
                    black_moves += 1;
                    black_all_check &= gave_check;
                }
            }
        }
        let white_perp = white_moves > 0 && white_all_check;
        let black_perp = black_moves > 0 && black_all_check;
        match (white_perp, black_perp) {
            (true, false) => Some(Color::White),
            (false, true) => Some(Color::Black),
            // Neither side checked throughout, or (degenerately) both did: not a
            // clean perpetual check — fall back to the ordinary repetition draw.
            _ => None,
        }
    }

    // -- Perpetual-chase detection (Xiangqi AXF; direct-attack subset) ----------

    /// The set of enemy squares the move `from -> to` (made in the position
    /// **before** `pos`, leaving `pos` with the chaser **not** to move) newly
    /// **chases**, or `None` if it chases nothing — a faithful subset of
    /// Fairy-Stockfish's `Position::chased` (direct attacks only).
    ///
    /// `pos` is the position **after** the move: its side to move
    /// ([`turn`](GenericPosition::turn)) is the chased side (FSF `sideToMove`); the
    /// chaser is the side that just moved. A chase is a *new* attack by the moved
    /// piece on an enemy piece that is not a General or Soldier, is not of the moved
    /// piece's own type, and is either **value-superior** to the attacker or left
    /// **unprotected**.
    fn chase_targets(
        pos: &GenericPosition<G, V>,
        from: Square<G>,
        to: Square<G>,
    ) -> Option<Bitboard<G>> {
        let board = pos.board();
        let victims = pos.turn();
        let mover = victims.opposite();
        let moved_role = board.role_at(to)?;
        // Only non-King, non-Soldier movers create a direct chase (FSF).
        if matches!(moved_role, WideRole::King | WideRole::Soldier) {
            return None;
        }
        let occ = board.occupied();
        let mut attacks = V::role_attacks(moved_role, mover, to, occ) & board.by_color(victims);
        // A Chariot / Cannon attack *along the moved line* is not a new attack.
        if matches!(moved_role, WideRole::Rook | WideRole::Cannon) {
            attacks &= !Self::move_line_mask(from, to);
        }
        // Exclude the enemy General and Soldiers (and so any check on the General).
        attacks &=
            !(board.pieces(victims, WideRole::King) | board.pieces(victims, WideRole::Soldier));
        // A piece attacking an enemy piece of its **own** type is a mutual /
        // symmetric attack, not a chase.
        attacks &= !board.pieces(victims, moved_role);

        let occ_without_attacker = occ ^ Bitboard::from_square(to);
        let mut result = Bitboard::EMPTY;
        for s in attacks {
            let Some(victim_role) = board.role_at(s) else {
                continue;
            };
            if Self::is_superior_chase(moved_role, victim_role)
                || pos
                    .attackers_to(s, victims, occ_without_attacker)
                    .is_empty()
            {
                result |= Bitboard::from_square(s);
            }
        }
        (!result.is_empty()).then_some(result)
    }

    // -- Attack-repetition detection (Chu / Dai large shogi) --------------------

    /// Whether the moved piece now standing on `to` (having just produced `pos`,
    /// whose side to move is the attacked side) **attacks at least one enemy
    /// non-royal piece** — the Chu / Dai large-shogi "attack" test used by the
    /// [attack-repetition rule](WideVariant::attack_repetition_loses).
    ///
    /// Unlike the Xiangqi [`chase_targets`](Self::chase_targets) test it applies
    /// **no** value-superiority, protection, same-type, or moved-line filter: any
    /// threat on any non-royal enemy piece counts ("however futile", per the
    /// chessvariants Chu ruleset). The enemy **royals** (King and Crown Prince) are
    /// excluded — an attack on a royal is a *check*, handled first and separately by
    /// the [perpetual-check](WideVariant::perpetual_check_loses) rule.
    fn attacks_nonroyal(pos: &GenericPosition<G, V>, to: Square<G>) -> bool {
        let board = pos.board();
        let victims = pos.turn();
        let mover = victims.opposite();
        let Some(moved_role) = board.role_at(to) else {
            return false;
        };
        let occ = board.occupied();
        let royals =
            board.pieces(victims, WideRole::King) | board.pieces(victims, WideRole::CrownPrince);
        let targets =
            V::role_attacks(moved_role, mover, to, occ) & board.by_color(victims) & !royals;
        !targets.is_empty()
    }

    /// Whether `attacker` attacking `victim` is a **value-superior** chase that
    /// counts regardless of protection (FSF's stronger-piece tiers): a Horse or
    /// Cannon attacking a Chariot, or an Elephant or Advisor attacking a Chariot,
    /// Cannon, or Horse.
    fn is_superior_chase(attacker: WideRole, victim: WideRole) -> bool {
        match attacker {
            WideRole::Horse | WideRole::Cannon => victim == WideRole::Rook,
            WideRole::XiangqiElephant | WideRole::Advisor => {
                matches!(victim, WideRole::Rook | WideRole::Cannon | WideRole::Horse)
            }
            _ => false,
        }
    }

    /// The rank or file the orthogonal move `from -> to` slides along — the squares
    /// a Chariot / Cannon was already covering before the move, so an attack on a
    /// piece there is not *new*.
    fn move_line_mask(from: Square<G>, to: Square<G>) -> Bitboard<G> {
        let mut mask = Bitboard::EMPTY;
        if from.file() == to.file() {
            let file = to.file();
            for rank in 0..G::HEIGHT {
                if let Some(sq) = Square::<G>::from_file_rank(file, rank) {
                    mask |= Bitboard::from_square(sq);
                }
            }
        } else if from.rank() == to.rank() {
            let rank = to.rank();
            for file in 0..G::WIDTH {
                if let Some(sq) = Square::<G>::from_file_rank(file, rank) {
                    mask |= Bitboard::from_square(sq);
                }
            }
        }
        mask
    }

    // -- Counting (Makruk / Cambodian / ASEAN; exact FSF replication) -----------

    /// Advances the counting state across a move, reproducing Fairy-Stockfish's
    /// `do_move` counting block: increment the live count, then (re)start it when a
    /// triggering material configuration is reached.
    fn update_counting(
        prev: Counting,
        before: &Board<G>,
        pos: &GenericPosition<G, V>,
        rule: WideCountingRule,
    ) -> Counting {
        let board = pos.board();
        let stm = pos.turn(); // FSF `sideToMove` (after the flip): the side to move now.
        let mover = stm.opposite();
        let mut c = prev;
        // Increment the existing count (FSF `if (countingLimit) ++countingPly`),
        // using the limit carried from before this move.
        if c.limit != 0 {
            c.ply = c.ply.saturating_add(1);
        }
        let total_all = board.occupied().count() as u16;
        let pawns = Self::pawns_total(board);
        let captured = (board.occupied().count() as u16) < (before.occupied().count() as u16);
        // The captured piece (if any) belonged to the side that did not move (`stm`).
        let captured_pawn = captured
            && Self::role_count(board, stm, WideRole::Pawn)
                < Self::role_count(before, stm, WideRole::Pawn);
        // A promotion lowers the mover's own pawn count.
        let promotion = Self::role_count(board, mover, WideRole::Pawn)
            < Self::role_count(before, mover, WideRole::Pawn);

        // Branch 1 (board-honour rules only): the mover's King captured the last
        // pawn and is now bare — start the count for the mover. Skipped by the
        // pieces-honour-only rules (ASEAN / Burmese), which count only a lone king.
        if !Self::is_pieces_honour_only(rule)
            && captured_pawn
            && Self::total(board, mover) == 1
            && pawns == 0
        {
            let limit = Self::count_limit(board, mover, rule);
            if limit != 0 {
                c.limit = 2 * limit;
                c.ply = 2 * total_all - 1;
            }
        }
        // Branch 2: start the count for the side to move when none is active, or
        // restart it when a capture / promotion has just bared that side.
        if c.limit == 0 || ((captured || promotion) && Self::total(board, stm) == 1) {
            let limit = Self::count_limit(board, stm, rule);
            if limit != 0 {
                c.limit = 2 * limit;
                c.ply = if Self::is_pieces_honour_only(rule) || Self::total(board, stm) > 1 {
                    0
                } else {
                    2 * total_all
                };
            }
        }
        c
    }

    /// The counting limit in **full moves** for the side `side` being counted under
    /// `rule`, or `0` for no count — a clean-room reproduction of Fairy-Stockfish's
    /// `Position::count_limit` (validated against the FSF binary's echoed counting
    /// FEN field). KHON is the Silver / Khon ([`WideRole::Silver`]).
    fn count_limit(board: &Board<G>, side: Color, rule: WideCountingRule) -> u16 {
        let opp = side.opposite();
        let pawns = Self::pawns_total(board);
        let rooks = Self::role_count(board, opp, WideRole::Rook);
        let khons = Self::role_count(board, opp, WideRole::Silver);
        let knights = Self::role_count(board, opp, WideRole::Knight);
        match rule {
            WideCountingRule::Makruk => {
                if pawns > 0 || Self::total(board, opp) == 1 {
                    return 0;
                }
                if Self::total(board, side) > 1 {
                    return 64; // board's honour
                }
                // Pieces' honour, scaled by the superior side's material.
                if rooks > 1 {
                    8
                } else if rooks == 1 {
                    16
                } else if khons > 1 {
                    22
                } else if knights > 1 {
                    32
                } else if khons == 1 {
                    44
                } else {
                    64
                }
            }
            WideCountingRule::Cambodian => {
                if Self::total(board, side) > 3 || Self::total(board, opp) == 1 {
                    return 0;
                }
                if Self::total(board, side) > 1 {
                    return 63; // board's honour
                }
                if pawns > 0 {
                    return 0;
                }
                if rooks > 1 {
                    7
                } else if rooks == 1 {
                    15
                } else if khons > 1 {
                    21
                } else if knights > 1 {
                    31
                } else if khons == 1 {
                    43
                } else {
                    63
                }
            }
            WideCountingRule::Asean => {
                // Pieces' honour only: the counted side must be a lone king.
                if pawns > 0 || Self::total(board, side) > 1 {
                    0
                } else if rooks > 0 {
                    16
                } else if khons > 0 {
                    44
                } else if knights > 0 {
                    64
                } else {
                    0
                }
            }
            WideCountingRule::Burmese => {
                // Sittuyin (published Burmese counting): the same pieces-honour
                // tiers as ASEAN (rook 16 / sin 44 / knight 64; the general/Met
                // alone cannot mate and draws at once), but a lone king caught on
                // one of the four centre squares (d4 / d5 / e4 / e5) is granted
                // five extra moves — the count starts only after its fifth move —
                // so the limits become 21 / 49 / 69. Fairy-Stockfish itself models
                // Sittuyin as plain ASEAN and omits this exception.
                if pawns > 0 || Self::total(board, side) > 1 {
                    return 0;
                }
                let base = if rooks > 0 {
                    16
                } else if khons > 0 {
                    44
                } else if knights > 0 {
                    64
                } else {
                    return 0;
                };
                if Self::king_on_centre(board, side) {
                    base + 5
                } else {
                    base
                }
            }
        }
    }

    /// Whether `rule` counts only a **lone king** (pieces' honour only, no
    /// board-honour phase and no board-honour count start) — ASEAN and Burmese.
    fn is_pieces_honour_only(rule: WideCountingRule) -> bool {
        matches!(rule, WideCountingRule::Asean | WideCountingRule::Burmese)
    }

    /// Whether `side`'s king stands on one of the four central squares (d4 / d5 /
    /// e4 / e5 on 8x8) — the Sittuyin centre-square counting exception. The four
    /// centre squares are the two middle files and the two middle ranks.
    fn king_on_centre(board: &Board<G>, side: Color) -> bool {
        let (cf0, cf1) = ((G::WIDTH - 1) / 2, G::WIDTH / 2);
        let (cr0, cr1) = ((G::HEIGHT - 1) / 2, G::HEIGHT / 2);
        board.pieces(side, WideRole::King).into_iter().any(|sq| {
            let (f, r) = (sq.file(), sq.rank());
            (f == cf0 || f == cf1) && (r == cr0 || r == cr1)
        })
    }

    /// The number of pieces of color `color` on `board`.
    fn total(board: &Board<G>, color: Color) -> u16 {
        board.by_color(color).count() as u16
    }

    /// The number of `role` pieces of `color` on `board`.
    fn role_count(board: &Board<G>, color: Color, role: WideRole) -> u16 {
        board.pieces(color, role).count() as u16
    }

    /// The total number of pawns (both colors) on `board`.
    fn pawns_total(board: &Board<G>) -> u16 {
        Self::role_count(board, Color::White, WideRole::Pawn)
            + Self::role_count(board, Color::Black, WideRole::Pawn)
    }
}

impl<G: Geometry, V: WideVariant<G>> From<GenericPosition<G, V>> for GenericGame<G, V> {
    #[inline]
    fn from(position: GenericPosition<G, V>) -> Self {
        Self::new(position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::variants::{
        Asean, Cambodian, CannonShogi, Capablanca, Chu, Dai, Janggi, Makpong, Makruk, Minishogi,
        Minixiangqi, ShoShogi, Shogi, Sittuyin, Xiangqi,
    };
    use crate::geometry::{GenericPosition, Geometry, WideEndReason, WideMove, WideVariant};

    /// Plays the cyclic move pattern `cycle` one ply at a time (up to `max` plies),
    /// returning the number of plies played when the game first becomes over, or
    /// `None` if it never does.
    fn play_until_over<G: Geometry, V: WideVariant<G>>(
        game: &mut GenericGame<G, V>,
        cycle: &[(u8, u8)],
        max: usize,
    ) -> Option<usize> {
        for i in 0..max {
            let (from, to) = cycle[i % cycle.len()];
            play(game, from, to);
            if game.is_over() {
                return Some(i + 1);
            }
        }
        None
    }

    /// Finds the legal move in `game`'s current position whose source and
    /// destination square indices are `from` and `to`.
    fn mv_by<G: Geometry, V: WideVariant<G>>(
        game: &GenericGame<G, V>,
        from: u8,
        to: u8,
    ) -> WideMove {
        game.legal_moves()
            .into_iter()
            .find(|m| m.from_index() == from && m.to::<G>().index() == to)
            .unwrap_or_else(|| panic!("expected a legal move {from}->{to}"))
    }

    /// Plays the move `from`->`to`, asserting it is legal.
    fn play<G: Geometry, V: WideVariant<G>>(game: &mut GenericGame<G, V>, from: u8, to: u8) {
        let mv = mv_by(game, from, to);
        game.play(&mv).expect("legal move");
    }

    // --- Shogi sennichite ------------------------------------------------

    #[test]
    fn shogi_sennichite_is_a_draw() {
        // Two lone kings shuffling in place: the position recurs without any
        // check, so the fourth occurrence is a plain sennichite draw.
        // 9x9: black king a9 = (0,8) = 72, white king e1 = (4,0) = 4.
        let pos = GenericPosition::<_, _>::from_fen("k8/9/9/9/9/9/9/9/4K4 w - - 0 1")
            .expect("valid shogi fen");
        let _: &Shogi = &pos;
        let mut game = GenericGame::new(pos);
        assert_eq!(game.repetition_count(), 1);
        // One cycle (white king e1<->e2, black king a9<->a8) returns to start.
        // e1=4, e2=13 (rank1*9+4); a9=72, a8=63 (rank7*9+0).
        for _ in 0..3 {
            play(&mut game, 4, 13); // white K e1->e2
            play(&mut game, 72, 63); // black K a9->a8
            play(&mut game, 13, 4); // white K e2->e1
            play(&mut game, 63, 72); // black K a8->a9
        }
        assert_eq!(game.repetition_count(), 4);
        assert_eq!(game.end_reason(), Some(WideEndReason::Sennichite));
        assert_eq!(game.outcome(), Some(WideOutcome::Draw));
        assert!(game.is_draw());
    }

    #[test]
    fn shogi_perpetual_check_loses_for_the_checker() {
        // White rook shuttles between i1/i2, checking the black king on the a-file
        // along ranks 0 and 1; the king is driven between a1 and a2. The rook stays
        // out of White's promotion zone (ranks 6..8) so it never promotes and the
        // position recurs exactly. Every white move is a check, so the repetition is
        // a perpetual check and White (the checker) loses.
        // i2 = (8,1) = 17, i1 = (8,0) = 8; a1 = (0,0) = 0, a2 = (0,1) = 9.
        let pos = GenericPosition::<_, _>::from_fen("9/9/9/9/4K4/9/9/8R/k8 w - - 0 1")
            .expect("valid shogi fen");
        let _: &Shogi = &pos;
        let mut game = GenericGame::new(pos);
        for _ in 0..3 {
            play(&mut game, 17, 8); // R i2->i1+, checks a1
            assert!(game.position().is_check());
            play(&mut game, 0, 9); // K a1->a2
            play(&mut game, 8, 17); // R i1->i2+, checks a2
            assert!(game.position().is_check());
            play(&mut game, 9, 0); // K a2->a1
        }
        assert_eq!(game.repetition_count(), 4);
        assert_eq!(game.end_reason(), Some(WideEndReason::PerpetualCheckLoss));
        // White perpetually checked, so White loses: Black wins.
        assert_eq!(
            game.outcome(),
            Some(WideOutcome::Decisive {
                winner: crate::Color::Black,
            })
        );
    }

    // --- Large / minor shogi sennichite (issue #471) ---------------------

    #[test]
    fn shoshogi_sennichite_is_a_draw() {
        // Sho Shogi shares Shogi's 9x9 geometry and sennichite rule (fold 4). Two
        // lone kings shuffling in place recur without check, so the fourth
        // occurrence is a plain sennichite draw.
        let pos = GenericPosition::<_, _>::from_fen("k8/9/9/9/9/9/9/9/4K4 w - - 0 1")
            .expect("valid sho shogi fen");
        let _: &ShoShogi = &pos;
        let mut game = GenericGame::new(pos);
        assert_eq!(game.repetition_count(), 1);
        for _ in 0..3 {
            play(&mut game, 4, 13); // white K e1->e2
            play(&mut game, 72, 63); // black K a9->a8
            play(&mut game, 13, 4); // white K e2->e1
            play(&mut game, 63, 72); // black K a8->a9
        }
        assert_eq!(game.repetition_count(), 4);
        assert_eq!(game.end_reason(), Some(WideEndReason::Sennichite));
        assert_eq!(game.outcome(), Some(WideOutcome::Draw));
        assert!(game.is_draw());
    }

    #[test]
    fn cannonshogi_perpetual_check_loses_for_the_checker() {
        // Cannon Shogi shares Shogi's 9x9 geometry; its Rook still moves as a Rook.
        // A White rook shuttling i1/i2 checks the lone black king along the a-file
        // on every move, so the repetition is a perpetual check and White (the
        // checker) loses. The rook stays out of the promotion zone so it never
        // promotes and the position recurs exactly.
        let pos = GenericPosition::<_, _>::from_fen("9/9/9/9/4K4/9/9/8R/k8 w - - 0 1")
            .expect("valid cannon shogi fen");
        let _: &CannonShogi = &pos;
        let mut game = GenericGame::new(pos);
        for _ in 0..3 {
            play(&mut game, 17, 8); // R i2->i1+, checks a1
            assert!(game.position().is_check());
            play(&mut game, 0, 9); // K a1->a2
            play(&mut game, 8, 17); // R i1->i2+, checks a2
            assert!(game.position().is_check());
            play(&mut game, 9, 0); // K a2->a1
        }
        assert_eq!(game.repetition_count(), 4);
        assert_eq!(game.end_reason(), Some(WideEndReason::PerpetualCheckLoss));
        // White perpetually checked, so White loses: Black wins.
        assert_eq!(
            game.outcome(),
            Some(WideOutcome::Decisive {
                winner: crate::Color::Black,
            })
        );
    }

    // --- Xiangqi / Janggi repetition -------------------------------------

    #[test]
    fn minixiangqi_threefold_repetition_is_a_draw() {
        // Two lone generals shuffling within their palaces on *different* files (so
        // they never face down the flying-general line): a quiet repetition, drawn
        // on the third occurrence. Minixiangqi is 7x7; the palace is the central
        // 3x3 (files 2..4). White general d1, black general c7.
        // d1 = (3,0) = 3, d2 = (3,1) = 10; c7 = (2,6) = 44, c6 = (2,5) = 37.
        let pos = GenericPosition::<_, _>::from_fen("2k4/7/7/7/7/7/3K3 w - - 0 1")
            .expect("valid minixiangqi fen");
        let _: &Minixiangqi = &pos;
        let mut game = GenericGame::new(pos);
        assert_eq!(game.repetition_count(), 1);
        for _ in 0..2 {
            play(&mut game, 3, 10); // K d1->d2
            play(&mut game, 44, 37); // k c7->c6
            play(&mut game, 10, 3); // K d2->d1
            play(&mut game, 37, 44); // k c6->c7
        }
        assert_eq!(game.repetition_count(), 3);
        assert_eq!(game.end_reason(), Some(WideEndReason::Repetition));
        assert_eq!(game.outcome(), Some(WideOutcome::Draw));
    }

    // --- Janggi bikjang --------------------------------------------------

    #[test]
    fn janggi_bikjang_facing_generals_draw() {
        // The two generals face down the open e-file. Bikjang is a draw when the
        // facing persists for two consecutive positions (FSF
        // `st->bikjang && st->previous->bikjang`): the start faces, and White
        // **passes** (always allowed under bikjang, and the only move that keeps the
        // generals faced — sliding the general along the contested file is illegal),
        // so the second position also faces — a bikjang draw.
        // e1 = (4,0) = 4 (a pass is e1->e1); e10 = (4,9).
        let pos = GenericPosition::<_, _>::from_fen("4k4/9/9/9/9/9/9/9/9/4K4 w - - 0 1")
            .expect("valid janggi fen");
        let pos: Janggi = pos;
        assert!(
            pos.is_facing_generals(),
            "generals should face on the e-file"
        );
        let mut game = GenericGame::new(pos);
        // A single facing position is not yet bikjang.
        assert_eq!(game.outcome(), None);
        play(&mut game, 4, 4); // White passes; the generals stay faced.
        assert!(game.position().is_facing_generals());
        assert_eq!(game.end_reason(), Some(WideEndReason::Bikjang));
        assert_eq!(game.outcome(), Some(WideOutcome::Draw));
    }

    #[test]
    fn janggi_facing_side_must_break_or_pass_and_breaking_avoids_bikjang() {
        // Fairy-Stockfish (and mcr's move generator) forbid **any non-pass move that
        // leaves the generals facing** once they already face — so a
        // two-consecutive-facing bikjang is only ever reachable through a pass, and
        // the wrapper's `st->bikjang && st->previous->bikjang` history check
        // coincides with the pass terminal it relabels. From the facing position
        // White's only legal moves are the two king steps that break the line
        // (e1->d1, e1->f1) and the pass (e1->e1); breaking it avoids bikjang.
        // e1=4, d1=3, f1=5.
        let pos = GenericPosition::<_, _>::from_fen("4k4/9/9/9/9/9/9/9/9/4K4 w - - 0 1")
            .expect("valid janggi fen");
        let pos: Janggi = pos;
        assert!(pos.is_facing_generals());
        let moves: alloc::vec::Vec<(u8, u8)> = pos
            .legal_moves()
            .into_iter()
            .map(|m| {
                (
                    m.from_index(),
                    m.to::<crate::geometry::Xiangqi9x10>().index(),
                )
            })
            .collect();
        assert_eq!(
            moves,
            alloc::vec![(4, 3), (4, 5), (4, 4)],
            "facing side may only break the line (d1/f1) or pass (e1e1)"
        );
        let mut game = GenericGame::new(pos);
        play(&mut game, 4, 3); // White Ke1->d1 breaks the facing
        assert!(!game.position().is_facing_generals());
        assert_eq!(
            game.end_reason(),
            None,
            "breaking the facing is not a bikjang"
        );
    }

    #[test]
    fn janggi_non_facing_generals_is_not_bikjang() {
        // Generals on different files do not face: no bikjang.
        let pos = GenericPosition::<_, _>::from_fen("3k5/9/9/9/9/9/9/9/9/4K4 w - - 0 1")
            .expect("valid janggi fen");
        let pos: Janggi = pos;
        assert!(!pos.is_facing_generals());
        let game = GenericGame::new(pos);
        assert_eq!(game.outcome(), None);
    }

    // --- Janggi perpetual check (issue #476) -----------------------------

    #[test]
    fn janggi_perpetual_check_loses_for_the_checker() {
        // A White Chariot (Rook) shuttles i9/i10, checking the lone Black general
        // along the rank on every White move; the general is driven between d10 and
        // d9. Every White move is a check, so the repetition is a perpetual check and
        // White (the checker) loses. The generals stand on different files
        // (White e2, Black d-file) so they never face — a facing is not an ordinary
        // check in Janggi (FSF `flyingGeneral = false`), so it could not be confused
        // with a perpetual check anyway.
        // Indices (9 wide): i9=80, i10=89 (Chariot); d10=84, d9=75 (general).
        let pos = GenericPosition::<_, _>::from_fen("3k5/8R/9/9/9/9/9/9/4K4/9 w - - 0 1")
            .expect("valid janggi fen");
        let _: &Janggi = &pos;
        let mut game = GenericGame::new(pos);
        // Two full 4-ply cycles bring the start position to its third occurrence.
        for _ in 0..2 {
            play(&mut game, 80, 89); // R i9->i10+, checks d10
            assert!(game.position().is_check());
            play(&mut game, 84, 75); // k d10->d9
            play(&mut game, 89, 80); // R i10->i9+, checks d9
            assert!(game.position().is_check());
            play(&mut game, 75, 84); // k d9->d10
        }
        assert_eq!(game.repetition_count(), 3);
        assert_eq!(game.end_reason(), Some(WideEndReason::PerpetualCheckLoss));
        // White perpetually checked, so White loses: Black wins.
        assert_eq!(
            game.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::Black,
            })
        );
    }

    #[test]
    fn janggi_quiet_repetition_is_a_plain_draw() {
        // Two lone generals shuffling within their palaces on *different* files (so
        // they never face and neither ever checks): an ordinary three-fold
        // repetition draws (FSF `nFoldValue = VALUE_DRAW`), not a perpetual-check
        // loss. White general e2<->e3, Black general d10<->d9.
        // e2=13, e3=22; d10=84, d9=75.
        let pos = GenericPosition::<_, _>::from_fen("3k5/9/9/9/9/9/9/9/4K4/9 w - - 0 1")
            .expect("valid janggi fen");
        let _: &Janggi = &pos;
        let mut game = GenericGame::new(pos);
        assert_eq!(game.repetition_count(), 1);
        for _ in 0..2 {
            play(&mut game, 13, 22); // K e2->e3
            play(&mut game, 84, 75); // k d10->d9
            play(&mut game, 22, 13); // K e3->e2
            play(&mut game, 75, 84); // k d9->d10
        }
        assert_eq!(game.repetition_count(), 3);
        assert_eq!(game.end_reason(), Some(WideEndReason::Repetition));
        assert_eq!(game.outcome(), Some(WideOutcome::Draw));
    }

    // --- Xiangqi perpetual chase -----------------------------------------

    #[test]
    fn xiangqi_perpetual_chase_loses_for_the_chaser() {
        // A White Horse perpetually chases an undefended Black Chariot (Rook): the
        // Horse re-attacks the fleeing Rook on every White move (a Horse attacking a
        // Chariot is a value-superior chase that counts regardless of protection),
        // forcing a repetition. White is the perpetual chaser and **loses**. The
        // kings sit on different files (White Kf1, Black Ke10) so they never face.
        // Validated against Fairy-Stockfish `UCI_Variant xiangqi` on the same line
        // (`go` returns a forced-mate loss for the chasing side).
        // Indices (9 wide): c4=29, e5=40 (Horse); d3=21, d6=48 (Chariot).
        let pos = GenericPosition::<_, _>::from_fen("4k4/9/9/9/9/9/2J6/3r5/9/5K3 w - - 0 1")
            .expect("valid xiangqi fen");
        let pos: Xiangqi = pos;
        let mut game = GenericGame::new(pos);
        // Two full 4-ply cycles bring the start position to its third occurrence.
        for _ in 0..2 {
            play(&mut game, 29, 40); // J c4->e5, chases the chariot on d3
            play(&mut game, 21, 48); // r d3->d6 (flees)
            play(&mut game, 40, 29); // J e5->c4, chases the chariot on d6
            play(&mut game, 48, 21); // r d6->d3 (flees)
        }
        assert_eq!(game.repetition_count(), 3);
        assert_eq!(game.end_reason(), Some(WideEndReason::PerpetualChaseLoss));
        // White perpetually chased, so White loses: Black wins.
        assert_eq!(
            game.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::Black,
            })
        );
    }

    #[test]
    fn xiangqi_quiet_repetition_is_a_plain_draw() {
        // The same two pieces shuffling **without** any chase (the Chariot moves
        // along its own file out of the Horse's reach and back; the Horse never
        // attacks it): an ordinary three-fold repetition draw, not a chase loss.
        // Horse a1 (idx 0) shuffles a1<->c2; Chariot i6 far away shuffles i6<->i5.
        // Kings on different files (Black Ke10, White Kf1) so the flying-general
        // file is never open between them.
        let pos = GenericPosition::<_, _>::from_fen("4k4/9/9/9/8r/9/9/9/9/J4K3 w - - 0 1")
            .expect("valid xiangqi fen");
        let pos: Xiangqi = pos;
        let mut game = GenericGame::new(pos);
        // Horse a1=0 -> c2=11 (a knight move) and back; Chariot i6=53 -> i5=44.
        for _ in 0..2 {
            play(&mut game, 0, 11); // J a1->c2 (does not attack the far chariot)
            play(&mut game, 53, 44); // r i6->i5
            play(&mut game, 11, 0); // J c2->a1
            play(&mut game, 44, 53); // r i5->i6
        }
        assert_eq!(game.repetition_count(), 3);
        assert_eq!(game.end_reason(), Some(WideEndReason::Repetition));
        assert_eq!(game.outcome(), Some(WideOutcome::Draw));
    }

    // --- Chu / Dai large-shogi attack repetition (issue #472) ------------
    //
    // Hand-derived cases only: there is no machine oracle for the chase rule
    // (HaChu exercises captures at shallow depth and segfaults on Tenjiku), so
    // these positions and verdicts are constructed and checked by hand against
    // the chessvariants Chu ruleset.

    #[test]
    fn chu_attack_repetition_loses_for_the_attacker() {
        // A White Rook (Chariot) on the d-file perpetually attacks a lone Black Gold
        // that shuffles up and down the same file staying in the Rook's line, while
        // the Gold's own moves attack nothing. Every White move re-attacks the Gold
        // and no Black move attacks anything, so the fourth occurrence is a one-sided
        // attack repetition and White (the attacker) LOSES rather than drawing by
        // sennichite. The Rook never reaches the enemy King (Black Kl12, on the
        // l-file) so it is not perpetual check; the kings are far apart so the Gold
        // never checks either. No captures/promotions occur, so the position recurs.
        // Chu is 12x12, index = rank*12 + file. Ra1-file: d1=3, d2=15. Gold: d5=51,
        // d6=63. Kings: white a1=0, black l12=143.
        let pos =
            GenericPosition::<_, _>::from_fen("11k/12/12/12/12/12/12/3g8/12/12/12/K2R8 w - - 0 1")
                .expect("valid chu fen");
        let _: &Chu = &pos;
        let mut game = GenericGame::new(pos);
        assert_eq!(game.repetition_count(), 1);
        // Three 4-ply cycles bring the start to its fourth occurrence (sennichite
        // fold). Rook d1<->d2 re-attacks the Gold; the Gold flees d5<->d6.
        for _ in 0..3 {
            play(&mut game, 3, 15); // R d1->d2, attacks the Gold
            assert!(!game.position().is_check());
            play(&mut game, 51, 63); // g d5->d6 (flees, attacks nothing)
            play(&mut game, 15, 3); // R d2->d1, attacks the Gold
            assert!(!game.position().is_check());
            play(&mut game, 63, 51); // g d6->d5 (flees, attacks nothing)
        }
        assert_eq!(game.repetition_count(), 4);
        assert_eq!(game.end_reason(), Some(WideEndReason::AttackRepetitionLoss));
        // White perpetually attacked, so White loses: Black wins.
        assert_eq!(
            game.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::Black,
            })
        );
    }

    #[test]
    fn chu_quiet_repetition_still_draws_by_sennichite() {
        // The control: two lone kings shuffling in place, neither attacking anything.
        // With no attacks on either side the attack-repetition rule does not fire and
        // the fourth occurrence is a plain sennichite draw — exactly as before #472.
        // White Ka1<->a2 (0<->12), Black Kl12<->l11 (143<->131).
        let pos =
            GenericPosition::<_, _>::from_fen("11k/12/12/12/12/12/12/12/12/12/12/K11 w - - 0 1")
                .expect("valid chu fen");
        let _: &Chu = &pos;
        let mut game = GenericGame::new(pos);
        assert_eq!(game.repetition_count(), 1);
        for _ in 0..3 {
            play(&mut game, 0, 12); // K a1->a2
            play(&mut game, 143, 131); // k l12->l11
            play(&mut game, 12, 0); // K a2->a1
            play(&mut game, 131, 143); // k l11->l12
        }
        assert_eq!(game.repetition_count(), 4);
        assert_eq!(game.end_reason(), Some(WideEndReason::Sennichite));
        assert_eq!(game.outcome(), Some(WideOutcome::Draw));
        assert!(game.is_draw());
    }

    #[test]
    fn dai_attack_repetition_loses_for_the_attacker() {
        // Dai (15x15) shares Chu's attack-repetition rule. The same construction: a
        // White Rook on the d-file perpetually attacks a lone Black Gold shuffling in
        // its line while the Gold attacks nothing, so White loses at the fourth
        // occurrence. index = rank*15 + file. d1=3, d2=18; Gold d8=108, d9=123;
        // kings white a1=0, black o15=224. The Gold sits outside both five-rank
        // promotion zones (ranks 6-10) so no promotion perturbs the cycle.
        let pos = GenericPosition::<_, _>::from_fen(
            "14k/15/15/15/15/15/15/3g11/15/15/15/15/15/15/K2R11 w - - 0 1",
        )
        .expect("valid dai fen");
        let _: &Dai = &pos;
        let mut game = GenericGame::new(pos);
        for _ in 0..3 {
            play(&mut game, 3, 18); // R d1->d2, attacks the Gold
            play(&mut game, 108, 123); // g d8->d9 (flees, attacks nothing)
            play(&mut game, 18, 3); // R d2->d1, attacks the Gold
            play(&mut game, 123, 108); // g d9->d8 (flees, attacks nothing)
        }
        assert_eq!(game.repetition_count(), 4);
        assert_eq!(game.end_reason(), Some(WideEndReason::AttackRepetitionLoss));
        assert_eq!(
            game.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::Black,
            })
        );
    }

    // --- Makruk / Cambodian / ASEAN counting -----------------------------

    #[test]
    fn makruk_pieces_honour_count_matches_fsf() {
        // Black is a lone king; White has K + one Chariot (Rook) and no pawns, so
        // the **pieces-honour** count applies: limit 16 full moves (FSF
        // `countingLimit = 32`), the count starting from the piece total (FSF
        // `countingPly = 6` after the first move). The draw fires when the ply
        // exceeds 32, i.e. on the 28th half-move — matching the FSF binary's echoed
        // counting field (`... 32 6` after one move; the rook K+R-vs-K limit is 16).
        let pos = GenericPosition::<_, _>::from_fen("k7/8/8/8/8/8/8/2R3K1 w - - 0 1")
            .expect("valid makruk fen");
        let pos: Makruk = pos;
        let mut game = GenericGame::new(pos);
        // c1=2, c2=10; a8=56, a7=48 (a quiet, non-mating shuffle).
        let plies = play_until_over(&mut game, &[(2, 10), (56, 48), (10, 2), (48, 56)], 60);
        assert_eq!(
            plies,
            Some(28),
            "FSF pieces-honour draws on the 28th half-move"
        );
        assert_eq!(game.end_reason(), Some(WideEndReason::CountingDraw));
        assert_eq!(game.outcome(), Some(WideOutcome::Draw));
    }

    #[test]
    fn makpong_pieces_honour_count_matches_makruk() {
        // Makpong's only rule change from Makruk is king-safety (the king may not
        // flee a check); the counting endgame must be inherited verbatim (issue
        // #469). The same lone-king K + Rook position as the Makruk pieces-honour
        // test, shuffled so no check ever arises, must draw on the identical 28th
        // half-move — proving Makpong now forwards Makruk's `counting_rule`.
        let pos = GenericPosition::<_, _>::from_fen("k7/8/8/8/8/8/8/2R3K1 w - - 0 1")
            .expect("valid makpong fen");
        let pos: Makpong = pos;
        let mut game = GenericGame::new(pos);
        // c1=2, c2=10; a8=56, a7=48 (a quiet, non-checking shuffle).
        let plies = play_until_over(&mut game, &[(2, 10), (56, 48), (10, 2), (48, 56)], 60);
        assert_eq!(
            plies,
            Some(28),
            "Makpong pieces-honour draws on the 28th half-move, exactly like Makruk"
        );
        assert_eq!(game.end_reason(), Some(WideEndReason::CountingDraw));
        assert_eq!(game.outcome(), Some(WideOutcome::Draw));
    }

    #[test]
    fn makruk_board_honour_count_matches_fsf() {
        // Both sides keep material (K + Rook each, no pawns), so the **board-honour**
        // count applies: limit 64 full moves (FSF `countingLimit = 128`), the count
        // starting from zero. The draw fires when the ply exceeds 128, i.e. on the
        // 130th half-move — matching the FSF echo (`... 128 0` after one move).
        let pos = GenericPosition::<_, _>::from_fen("k4r2/8/8/8/8/8/8/2R4K w - - 0 1")
            .expect("valid makruk fen");
        let pos: Makruk = pos;
        let mut game = GenericGame::new(pos);
        // White Rc1<->c2 (2<->10); Black Rf8<->f7 (61<->53). No captures or checks.
        let plies = play_until_over(&mut game, &[(2, 10), (61, 53), (10, 2), (53, 61)], 200);
        assert_eq!(
            plies,
            Some(130),
            "FSF board-honour draws on the 130th half-move"
        );
        assert_eq!(game.end_reason(), Some(WideEndReason::CountingDraw));
        assert_eq!(game.outcome(), Some(WideOutcome::Draw));
    }

    #[test]
    fn asean_pieces_honour_count_matches_fsf() {
        // ASEAN is pieces-honour only and starts the count from **zero** (FSF
        // `countingPly = 0`): K + Rook vs lone king gives limit 16 moves
        // (`countingLimit = 32`), so the draw fires when the ply exceeds 32 — the
        // 34th half-move (FSF echo `... 32 0` after one move).
        let pos = GenericPosition::<_, _>::from_fen("k7/8/8/8/8/8/8/2R3K1 w - - 0 1")
            .expect("valid asean fen");
        let pos: Asean = pos;
        let mut game = GenericGame::new(pos);
        let plies = play_until_over(&mut game, &[(2, 10), (56, 48), (10, 2), (48, 56)], 60);
        assert_eq!(plies, Some(34), "FSF ASEAN draws on the 34th half-move");
        assert_eq!(game.end_reason(), Some(WideEndReason::CountingDraw));
        assert_eq!(game.outcome(), Some(WideOutcome::Draw));
    }

    #[test]
    fn cambodian_pieces_honour_count_matches_fsf() {
        // Cambodian K + Rook vs lone king: pieces-honour limit 15 moves (FSF
        // `countingLimit = 30`), count starting from the piece total (`countingPly =
        // 6`). The draw fires when the ply exceeds 30 — the 26th half-move (FSF echo
        // `... 30 6` after one move). Cambodian shares the Makruk array but carries
        // the `DEde` leap-rights field.
        let pos = GenericPosition::<_, _>::from_fen("k7/8/8/8/8/8/8/2R3K1 w DEde - 0 1")
            .expect("valid cambodian fen");
        let pos: Cambodian = pos;
        let mut game = GenericGame::new(pos);
        let plies = play_until_over(&mut game, &[(2, 10), (56, 48), (10, 2), (48, 56)], 60);
        assert_eq!(plies, Some(26), "FSF Cambodian draws on the 26th half-move");
        assert_eq!(game.end_reason(), Some(WideEndReason::CountingDraw));
        assert_eq!(game.outcome(), Some(WideOutcome::Draw));
    }

    #[test]
    fn sittuyin_pieces_honour_count_matches_asean_base() {
        // Sittuyin (Burmese counting) shares ASEAN's pieces-honour tiers, so a
        // K + Rook vs lone king endgame in which the counted king is NOT on a
        // centre square behaves exactly like ASEAN: limit 16 moves
        // (`countingLimit = 32`), the count starting from zero, so the draw fires
        // when the ply exceeds 32 — the 34th half-move. (The black king shuffles
        // a8<->a7, never touching the four central squares.)
        let pos = GenericPosition::<_, _>::from_fen("k7/8/8/8/8/8/8/2R3K1 w - - 0 1")
            .expect("valid sittuyin fen");
        let pos: Sittuyin = pos;
        let mut game = GenericGame::new(pos);
        let plies = play_until_over(&mut game, &[(2, 10), (56, 48), (10, 2), (48, 56)], 60);
        assert_eq!(
            plies,
            Some(34),
            "Sittuyin base tier draws on the 34th half-move, exactly like ASEAN"
        );
        assert_eq!(game.end_reason(), Some(WideEndReason::CountingDraw));
        assert_eq!(game.outcome(), Some(WideOutcome::Draw));
    }

    #[test]
    fn sittuyin_centre_square_grants_five_extra_moves() {
        // Sittuyin's distinctive centre-square exception: a lone king caught on one
        // of the four central squares (d4 / d5 / e4 / e5) when the count starts is
        // granted five extra moves, so the K + Rook limit becomes 21 moves
        // (`countingLimit = 42`) instead of 16. Here the black king sits on e5 (a
        // centre square) at count-start — after White's first move — so the draw
        // fires when the ply exceeds 42: the 44th half-move, ten plies later than
        // the non-centre base case above. The rook shuffles c1<->c2 (never checking
        // the e-file king) and the king shuffles e5<->e6.
        let pos = GenericPosition::<_, _>::from_fen("8/8/8/4k3/8/8/8/2R3K1 w - - 0 1")
            .expect("valid sittuyin fen");
        let pos: Sittuyin = pos;
        let mut game = GenericGame::new(pos);
        // White Rc1<->c2 (2<->10); Black Ke5<->e6 (36<->44). No captures / checks.
        let plies = play_until_over(&mut game, &[(2, 10), (36, 44), (10, 2), (44, 36)], 80);
        assert_eq!(
            plies,
            Some(44),
            "Sittuyin centre-square exception draws on the 44th half-move (limit 21)"
        );
        assert_eq!(game.end_reason(), Some(WideEndReason::CountingDraw));
        assert_eq!(game.outcome(), Some(WideOutcome::Draw));
    }

    // --- Generic move-rule + insufficient material (opt-in test variant) --

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    struct DrawRulesChess;

    impl WideVariant<crate::geometry::Chess8x8> for DrawRulesChess {
        fn starting_position() -> (
            crate::geometry::Board<crate::geometry::Chess8x8>,
            crate::geometry::GenericState<crate::geometry::Chess8x8>,
        ) {
            <crate::geometry::StandardChess as WideVariant<crate::geometry::Chess8x8>>::starting_position()
        }

        fn move_rule_plies() -> Option<u16> {
            Some(100)
        }

        fn is_insufficient_material(
            board: &crate::geometry::Board<crate::geometry::Chess8x8>,
            _state: &crate::geometry::GenericState<crate::geometry::Chess8x8>,
        ) -> bool {
            board.occupied().count() == 2
        }
    }

    type DrawChess = GenericPosition<crate::geometry::Chess8x8, DrawRulesChess>;

    #[test]
    fn move_rule_draw_when_enabled() {
        // Clock at the 100-ply limit with a move available: a move-rule draw.
        let pos = DrawChess::from_fen("4k3/8/8/8/8/8/8/Q3K3 w - - 100 80").expect("valid fen");
        assert_eq!(pos.end_reason(), Some(WideEndReason::MoveRule));
        assert_eq!(pos.outcome(), Some(WideOutcome::Draw));
        // Below the limit: still playing.
        let pos = DrawChess::from_fen("4k3/8/8/8/8/8/8/Q3K3 w - - 99 80").expect("valid fen");
        assert_eq!(pos.end_reason(), None);
    }

    #[test]
    fn insufficient_material_draw_when_enabled() {
        let pos = DrawChess::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").expect("valid fen");
        assert_eq!(pos.end_reason(), Some(WideEndReason::InsufficientMaterial));
        assert_eq!(pos.outcome(), Some(WideOutcome::Draw));
    }

    // --- Western large boards: 50-move + threefold (#473) -----------------
    //
    // The standard-army large boards (Capablanca family) opt into both the
    // move-count rule (`move_rule_plies() == Some(100)`) and threefold
    // repetition (`tracks_repetition()`). Capablanca (10x8) stands in for the
    // whole set: the plumbing is shared, so one representative exercise of each
    // rule at the `GenericGame` level covers them all.

    #[test]
    fn capablanca_threefold_repetition_is_a_draw() {
        // Two kings shuffle in place with static rooks on the a-file (present so
        // the position is not an insufficient-material draw). The position recurs
        // without progress and is drawn on its third occurrence.
        // Cap10x8 index = rank*10 + file. Kf1 = 5, f2 = 15; kf8 = 75, f7 = 65.
        let pos = GenericPosition::<_, _>::from_fen("r4k4/10/10/10/10/10/10/R4K4 w - - 0 1")
            .expect("valid capablanca fen");
        let _: &Capablanca = &pos;
        let mut game = GenericGame::new(pos);
        assert_eq!(game.repetition_count(), 1);
        for _ in 0..2 {
            play(&mut game, 5, 15); // K f1->f2
            play(&mut game, 75, 65); // k f8->f7
            play(&mut game, 15, 5); // K f2->f1
            play(&mut game, 65, 75); // k f7->f8
        }
        assert_eq!(game.repetition_count(), 3);
        assert_eq!(game.end_reason(), Some(WideEndReason::Repetition));
        assert_eq!(game.outcome(), Some(WideOutcome::Draw));
        assert!(game.is_draw());
    }

    #[test]
    fn capablanca_fifty_move_rule_draws_at_the_game_level() {
        // Halfmove clock at the 100-ply limit with legal moves available (a lone
        // queen keeps the position out of the insufficient-material rule): the
        // GenericGame reports a move-rule draw.
        let pos = GenericPosition::<_, _>::from_fen("5k4/10/10/10/10/10/10/Q4K4 w - - 100 80")
            .expect("valid capablanca fen");
        let _: &Capablanca = &pos;
        let game = GenericGame::new(pos);
        assert_eq!(game.end_reason(), Some(WideEndReason::MoveRule));
        assert_eq!(game.outcome(), Some(WideOutcome::Draw));
        assert!(game.is_draw());
        // One ply below the limit the game is still live.
        let pos = GenericPosition::<_, _>::from_fen("5k4/10/10/10/10/10/10/Q4K4 w - - 99 80")
            .expect("valid capablanca fen");
        let _: &Capablanca = &pos;
        let game = GenericGame::new(pos);
        assert_eq!(game.end_reason(), None);
    }

    #[test]
    fn capablanca_move_clock_resets_on_pawn_move_and_captures() {
        // A pawn push resets the clock; a plain king move only advances it.
        // Pawn f5 = 45 -> f6 = 55; kings far apart so no check intervenes.
        let pos = GenericPosition::<_, _>::from_fen("5k4/10/10/5P4/10/10/10/5K4 w - - 98 60")
            .expect("valid capablanca fen");
        let _: &Capablanca = &pos;
        let mut game = GenericGame::new(pos);
        assert_eq!(game.position().halfmove_clock(), 98);
        play(&mut game, 45, 55); // pawn f5->f6: progress, clock resets
        assert_eq!(game.position().halfmove_clock(), 0);
        assert_eq!(game.end_reason(), None);

        // A capture also resets the clock. White rook on a1 takes a black rook on
        // a8 up the open a-file. Ra1 = 0, ra8 = 70.
        let pos = GenericPosition::<_, _>::from_fen("r4k4/10/10/10/10/10/10/R4K4 w - - 40 30")
            .expect("valid capablanca fen");
        let _: &Capablanca = &pos;
        let mut game = GenericGame::new(pos);
        assert_eq!(game.position().halfmove_clock(), 40);
        play(&mut game, 0, 70); // Rxa8: capture, clock resets
        assert_eq!(game.position().halfmove_clock(), 0);

        // A non-progress king move advances the clock instead of resetting it.
        let pos = GenericPosition::<_, _>::from_fen("5k4/10/10/10/10/10/10/Q4K4 w - - 50 40")
            .expect("valid capablanca fen");
        let _: &Capablanca = &pos;
        let mut game = GenericGame::new(pos);
        play(&mut game, 5, 6); // K f1->g1: quiet, clock advances
        assert_eq!(game.position().halfmove_clock(), 51);
    }

    // -- Incremental Zobrist key (issue #311) -----------------------------

    /// Walks the legal-move tree to `depth`, asserting at every node that the key
    /// [`GenericGame::play`] maintains **incrementally** equals a from-scratch
    /// recompute of the current position.
    fn walk_game_key<G: Geometry, V: WideVariant<G>>(game: &GenericGame<G, V>, depth: u32) {
        assert_eq!(
            game.position_key(),
            game.position().zobrist(),
            "maintained game key diverged from recompute at {}",
            game.position().to_fen(),
        );
        if depth == 0 || game.is_over() {
            return;
        }
        for mv in game.legal_moves() {
            let mut child = game.clone();
            child.play(&mv).expect("legal move");
            walk_game_key(&child, depth - 1);
        }
    }

    #[test]
    fn incremental_game_key_matches_recompute() {
        // The repetition-tracking variants are the ones that maintain the key
        // incrementally through `play`; walk each from its starting position.
        walk_game_key(&GenericGame::new(Shogi::startpos()), 2);
        walk_game_key(&GenericGame::new(Minishogi::startpos()), 3);
        walk_game_key(&GenericGame::new(Xiangqi::startpos()), 2);
        walk_game_key(&GenericGame::new(Minixiangqi::startpos()), 3);
        walk_game_key(&GenericGame::new(Janggi::startpos()), 2);
    }
}
