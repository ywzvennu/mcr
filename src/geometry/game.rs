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
//! # Perpetual check
//!
//! When a repetition is found and the variant's
//! [`perpetual_check_loses`](WideVariant::perpetual_check_loses) is on, the
//! wrapper walks the repeated cycle: if one side delivered check on **every** one
//! of its moves through the cycle, that side is the perpetual checker and **loses**
//! (the win goes to the side that was being checked). Otherwise the repetition is
//! the ordinary [`repetition_draw_reason`](WideVariant::repetition_draw_reason)
//! draw (Sennichite for Shogi, Repetition elsewhere).
//!
//! # Counting (simplified)
//!
//! The Makruk / Cambodian counting endgame is modelled in a **board-honour-only,
//! simplified** form: from the ply a side is reduced to a lone king, the superior
//! side is given [`COUNTING_LIMIT_PLIES`] plies to deliver mate; if the count
//! elapses the game is a [`WideEndReason::CountingDraw`]. The full Fairy-Stockfish
//! rule (piece-honour counts that begin earlier and whose limit scales with the
//! stronger side's material) is **not** reproduced; see the per-variant notes.

use alloc::vec::Vec;

use super::position::{GenericPosition, WideOutcome};
use super::variant::{WideEndReason, WideVariant};
use super::{Geometry, WideMove};
use crate::Color;

/// The number of plies, counted from the appearance of a lone king, after which
/// the Makruk / Cambodian **board-honour** count elapses into a draw. Sixty-four
/// full moves — the board-honour limit — expressed in plies. This is the
/// simplified model (see the [module docs](self)).
pub const COUNTING_LIMIT_PLIES: u16 = 128;

/// One recorded position in a [`GenericGame`]'s history: its repetition key, the
/// side to move there, and whether that side was in check (i.e. whether the move
/// that produced the position delivered check). The check flag is what the
/// perpetual-check adjudication reads back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HistoryEntry {
    /// The position's [`repetition_key`](GenericPosition::repetition_key).
    key: u64,
    /// The side to move at this position.
    turn: Color,
    /// Whether the side to move is in check here.
    in_check: bool,
}

/// The Makruk / Cambodian board-honour counting state (simplified): how many
/// plies have elapsed since a lone king appeared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Counting {
    /// The side reduced to a lone king (the side the count protects).
    lone: Color,
    /// Plies elapsed since the lone king appeared (`1` on the ply it is first
    /// seen).
    plies: u16,
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
    /// The board-honour counting state; `None` unless the variant uses counting
    /// and a lone king is on the board.
    counting: Option<Counting>,
}

impl<G: Geometry, V: WideVariant<G>> GenericGame<G, V> {
    /// Starts a game from `position`, seeding the history (when the variant tracks
    /// repetition) and the counting state (when it counts) from it.
    #[must_use]
    pub fn new(position: GenericPosition<G, V>) -> Self {
        let mut history = Vec::new();
        if V::tracks_repetition() {
            history.push(Self::entry_for(&position));
        }
        let counting = if V::counting_rule() {
            Self::counting_for(&position, None)
        } else {
            None
        };
        GenericGame {
            position,
            history,
            counting,
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
        self.position = self.position.play(mv);
        if V::tracks_repetition() {
            self.history.push(Self::entry_for(&self.position));
        }
        if V::counting_rule() {
            self.counting = Self::counting_for(&self.position, self.counting);
        }
        Ok(())
    }

    /// Builds the history entry for `position`.
    fn entry_for(position: &GenericPosition<G, V>) -> HistoryEntry {
        HistoryEntry {
            key: position.repetition_key(),
            turn: position.turn(),
            in_check: position.is_check(),
        }
    }

    /// The side reduced to a lone king (the only piece of its colour), if exactly
    /// one side is, and the other still has material.
    fn lone_king_side(position: &GenericPosition<G, V>) -> Option<Color> {
        let board = position.board();
        let white = board.by_color(Color::White).count();
        let black = board.by_color(Color::Black).count();
        if white == 1 && black > 1 {
            Some(Color::White)
        } else if black == 1 && white > 1 {
            Some(Color::Black)
        } else {
            None
        }
    }

    /// Advances the counting state for `position` given the prior state.
    fn counting_for(position: &GenericPosition<G, V>, prev: Option<Counting>) -> Option<Counting> {
        let lone = Self::lone_king_side(position)?;
        match prev {
            // Same lone king as before: keep counting.
            Some(c) if c.lone == lone => Some(Counting {
                lone,
                plies: c.plies.saturating_add(1),
            }),
            // A lone king has just appeared (or switched sides): (re)start.
            _ => Some(Counting { lone, plies: 1 }),
        }
    }

    /// How many times the current position has occurred (always ≥ 1 when the
    /// variant tracks repetition; `0` otherwise, since nothing is recorded).
    #[must_use]
    pub fn repetition_count(&self) -> usize {
        if !V::tracks_repetition() {
            return 0;
        }
        let key = self.position.repetition_key();
        self.history.iter().filter(|e| e.key == key).count()
    }

    /// The reason the game has ended, or `None` if it is still in progress.
    ///
    /// The union of the single-position reasons ([`GenericPosition::end_reason`])
    /// and the history-dependent ones: repetition / sennichite / perpetual-check
    /// and the counting draw.
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
        if V::counting_rule() && self.counting_elapsed() {
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
        if V::counting_rule() && self.counting_elapsed() {
            return Some(WideOutcome::Draw);
        }
        None
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

    /// Whether the board-honour count has elapsed.
    fn counting_elapsed(&self) -> bool {
        matches!(self.counting, Some(c) if c.plies >= COUNTING_LIMIT_PLIES)
    }

    /// Adjudicates the current position's repetition, if any: the
    /// `(reason, outcome)` pair when the position has recurred enough times under
    /// [`WideVariant::repetition_fold`], else `None`.
    ///
    /// A perpetual check (one side checking on every move through the repeated
    /// cycle) under [`WideVariant::perpetual_check_loses`] is a loss for the
    /// checker; otherwise the recurrence is the variant's repetition draw.
    fn repetition_adjudication(&self) -> Option<(WideEndReason, WideOutcome)> {
        let key = self.position.repetition_key();
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
        Some((V::repetition_draw_reason(), WideOutcome::Draw))
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
    use crate::geometry::variants::{Janggi, Makruk, Minixiangqi, Shogi};
    use crate::geometry::{GenericPosition, Geometry, WideEndReason, WideMove, WideVariant};

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
    fn janggi_non_facing_generals_is_not_bikjang() {
        // Generals on different files do not face: no bikjang.
        let pos = GenericPosition::<_, _>::from_fen("3k5/9/9/9/9/9/9/9/9/4K4 w - - 0 1")
            .expect("valid janggi fen");
        let pos: Janggi = pos;
        assert!(!pos.is_facing_generals());
        let game = GenericGame::new(pos);
        assert_eq!(game.outcome(), None);
    }

    // --- Makruk counting -------------------------------------------------

    #[test]
    fn makruk_board_honour_count_elapses_into_a_draw() {
        // Black is reduced to a lone king; White shuffles a rook without mating.
        // After the board-honour count elapses the game is a counting draw.
        // White king g1=(6,0); white rook c1=(2,0); black king a8=(0,7).
        let pos = GenericPosition::<_, _>::from_fen("k7/8/8/8/8/8/8/2R3K1 w - - 0 1")
            .expect("valid makruk fen");
        let pos: Makruk = pos;
        let mut game = GenericGame::new(pos);
        // c1=2, c2=10; a8=56, a7=48.
        let mut elapsed = false;
        for _ in 0..40 {
            play(&mut game, 2, 10); // R c1->c2
            play(&mut game, 56, 48); // k a8->a7
            play(&mut game, 10, 2); // R c2->c1
            play(&mut game, 48, 56); // k a7->a8
            if game.outcome().is_some() {
                elapsed = true;
                break;
            }
        }
        assert!(elapsed, "the counting draw should have been reached");
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
}
