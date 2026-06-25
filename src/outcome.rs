//! Game-end and draw detection: turning a [`Position`] (and, for the
//! history-dependent rules, a sequence of them) into a chess result.
//!
//! Two layers are provided:
//!
//! - [`Position::outcome`] computes the result that is derivable from a *single*
//!   position — checkmate, stalemate, insufficient material, and the automatic
//!   seventy-five-move rule. These need no history.
//! - [`Game`] wraps a [`Position`] together with the [`Zobrist`] keys of the
//!   positions that have occurred, so it can also detect repetition. It reports
//!   the *claimable* draws (the fifty-move rule and threefold repetition) through
//!   predicates, and folds the *automatic* draws (the seventy-five-move rule and
//!   fivefold repetition) into [`Game::outcome`].
//!
//! # Which rules end the game automatically
//!
//! Under the FIDE Laws of Chess the distinction matters:
//!
//! - **Automatic** (the game is over the moment the condition holds, with no
//!   claim required): checkmate, stalemate, insufficient material, the
//!   seventy-five-move rule, and fivefold repetition.
//! - **Claimable** (a player *may* claim the draw, but play may continue if
//!   nobody does): the fifty-move rule and threefold repetition.
//!
//! [`Position::outcome`] and [`Game::outcome`] therefore return a result only for
//! the automatic conditions; the claimable ones are exposed as predicates
//! ([`Game::is_fifty_move_rule`], [`Game::is_threefold_repetition`]).

use crate::{Color, Move, Position, Zobrist};

/// The result of a finished game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Outcome {
    /// One side won; `winner` is the victorious color.
    Decisive {
        /// The side that won.
        winner: Color,
    },
    /// The game was drawn.
    Draw,
}

/// The specific rule under which a game ended (or could be claimed to end).
///
/// Every variant except the two trailing repetition/move-clock claims is an
/// *automatic* termination; the last two are *claimable* and never end the game
/// on their own (see [`EndReason::is_automatic`]).
///
/// Standard chess only ever produces [`EndReason::Checkmate`], [`EndReason::Stalemate`],
/// [`EndReason::InsufficientMaterial`], [`EndReason::SeventyFiveMoveRule`],
/// [`EndReason::FivefoldRepetition`], and the two claimable reasons. The
/// remaining reasons are variant-specific terminations whose label records *how*
/// a variant game ended; each preserves the winner (or draw) the variant
/// produced when it formerly reused [`EndReason::Checkmate`] or
/// [`EndReason::VariantWin`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndReason {
    /// The side to move is in check and has no legal move. Decisive for the side
    /// *not* to move.
    Checkmate,
    /// A variant-specific decisive ending in which the side *to move* wins.
    /// Decisive.
    ///
    /// Standard chess never produces this — its only decisive single-position
    /// reason is [`EndReason::Checkmate`], where the side that just moved (the
    /// side *not* to move) wins. Antichess inverts the no-legal-move result: a
    /// side with no move, or with no pieces left, *wins*, which is exactly this
    /// reason. Automatic.
    VariantWin,
    /// King of the Hill: a king reached one of the four central squares
    /// (d4/e4/d5/e5). The king's owner — the side *not* to move, since reaching
    /// the hill passes the turn — wins. Automatic, decisive.
    KingInTheHill,
    /// Three-check: a side delivered its third check. The checking side — the
    /// side that just moved, i.e. the side *not* to move — wins. Automatic,
    /// decisive.
    ThreeChecks,
    /// Racing Kings: exactly one king reached the eighth rank. The racer — the
    /// side that just moved, i.e. the side *not* to move — wins. Automatic,
    /// decisive. The both-kings-home case is [`EndReason::RaceDraw`] instead.
    RaceFinished,
    /// Racing Kings: both kings finished on the eighth rank (Black having matched
    /// White's home rank on the very next move). Automatic draw.
    RaceDraw,
    /// Atomic: a royal king was destroyed by the blast of an adjacent capture.
    /// The surviving side — the side *not* to move, since the side to move is the
    /// one whose king was just exploded — wins. Automatic, decisive.
    KingExploded,
    /// Horde: the pawn horde (White) has no pieces left, so Black has eliminated
    /// it. Reached on a Black move, leaving White to move, so the winner is the
    /// side *not* to move. Automatic, decisive.
    HordeDefeated,
    /// The side to move is not in check but has no legal move. Draw.
    Stalemate,
    /// Neither side has the material to deliver checkmate. Draw.
    InsufficientMaterial,
    /// Seventy-five full moves (150 plies) have passed with no capture or pawn
    /// move. Automatic draw.
    SeventyFiveMoveRule,
    /// The same position has occurred five times. Automatic draw.
    FivefoldRepetition,
    /// Fifty full moves (100 plies) have passed with no capture or pawn move.
    /// Claimable draw.
    FiftyMoveRule,
    /// The same position has occurred three times. Claimable draw.
    ThreefoldRepetition,
}

impl EndReason {
    /// Returns `true` for the reasons that end the game automatically (checkmate,
    /// the variant win, every variant-specific termination, stalemate,
    /// insufficient material, the seventy-five-move rule, and fivefold
    /// repetition), and `false` for the two claimable ones.
    #[must_use]
    #[inline]
    pub const fn is_automatic(self) -> bool {
        matches!(
            self,
            EndReason::Checkmate
                | EndReason::VariantWin
                | EndReason::KingInTheHill
                | EndReason::ThreeChecks
                | EndReason::RaceFinished
                | EndReason::RaceDraw
                | EndReason::KingExploded
                | EndReason::HordeDefeated
                | EndReason::Stalemate
                | EndReason::InsufficientMaterial
                | EndReason::SeventyFiveMoveRule
                | EndReason::FivefoldRepetition
        )
    }

    /// The [`Outcome`] this reason produces, given the side to move when it
    /// applies.
    ///
    /// The decisive reasons split by which side wins:
    ///
    /// - the side *not* to move (the side that just moved) wins under
    ///   [`EndReason::Checkmate`], [`EndReason::KingInTheHill`],
    ///   [`EndReason::ThreeChecks`], [`EndReason::RaceFinished`],
    ///   [`EndReason::KingExploded`], and [`EndReason::HordeDefeated`];
    /// - the side *to* move wins under [`EndReason::VariantWin`] (antichess).
    ///
    /// Every other reason — including the [`EndReason::RaceDraw`] both-home case
    /// — is a draw.
    #[must_use]
    #[inline]
    pub const fn outcome(self, turn: Color) -> Outcome {
        match self {
            EndReason::Checkmate
            | EndReason::KingInTheHill
            | EndReason::ThreeChecks
            | EndReason::RaceFinished
            | EndReason::KingExploded
            | EndReason::HordeDefeated => Outcome::Decisive {
                winner: turn.opposite(),
            },
            EndReason::VariantWin => Outcome::Decisive { winner: turn },
            EndReason::RaceDraw
            | EndReason::Stalemate
            | EndReason::InsufficientMaterial
            | EndReason::SeventyFiveMoveRule
            | EndReason::FivefoldRepetition
            | EndReason::FiftyMoveRule
            | EndReason::ThreefoldRepetition => Outcome::Draw,
        }
    }
}

/// The halfmove-clock value (in plies) at which the fifty-move rule may be
/// claimed: fifty full moves by each side.
const FIFTY_MOVE_PLIES: u32 = 100;
/// The halfmove-clock value (in plies) at which the seventy-five-move rule ends
/// the game automatically.
const SEVENTY_FIVE_MOVE_PLIES: u32 = 150;
/// The number of occurrences of a position that makes threefold repetition
/// claimable.
const THREEFOLD: usize = 3;
/// The number of occurrences of a position that ends the game automatically.
const FIVEFOLD: usize = 5;

impl Position {
    /// The game result derivable from this position alone, or `None` if the game
    /// is not over by any single-position rule.
    ///
    /// This covers the automatic terminations that do not depend on move history:
    ///
    /// - **Checkmate** → [`Outcome::Decisive`] for the side *not* to move.
    /// - **Stalemate** → [`Outcome::Draw`].
    /// - **Insufficient material** → [`Outcome::Draw`].
    /// - **Seventy-five-move rule** (halfmove clock ≥ 150) → [`Outcome::Draw`].
    ///
    /// Repetition draws (threefold and fivefold) need the sequence of prior
    /// positions and are therefore *not* reported here; use [`Game::outcome`] for
    /// those. The claimable fifty-move rule is likewise omitted on purpose — it
    /// does not end the game on its own.
    ///
    /// ```
    /// use mce::{Outcome, Color, Position};
    /// // Fool's mate: 1.f3 e5 2.g4 Qh4#, white to move and mated.
    /// let pos =
    ///     Position::from_fen("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3")
    ///         .unwrap();
    /// assert_eq!(pos.outcome(), Some(Outcome::Decisive { winner: Color::Black }));
    /// ```
    #[must_use]
    pub fn outcome(&self) -> Option<Outcome> {
        self.end_reason().map(|reason| reason.outcome(self.turn()))
    }

    /// The [`EndReason`] derivable from this position alone, or `None`.
    ///
    /// Returns the automatic single-position reasons only (see
    /// [`Position::outcome`]). When the side to move has no legal move the result
    /// is checkmate or stalemate; otherwise insufficient material and then the
    /// seventy-five-move rule are checked, in that order.
    #[must_use]
    pub fn end_reason(&self) -> Option<EndReason> {
        if self.legal_move_count() == 0 {
            return Some(if self.is_check() {
                EndReason::Checkmate
            } else {
                EndReason::Stalemate
            });
        }
        if self.is_insufficient_material() {
            return Some(EndReason::InsufficientMaterial);
        }
        if self.halfmove_clock() >= SEVENTY_FIVE_MOVE_PLIES {
            return Some(EndReason::SeventyFiveMoveRule);
        }
        None
    }
}

/// The error returned when an illegal move is passed to [`Game::play`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IllegalMove(pub Move);

impl core::fmt::Display for IllegalMove {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "illegal move {} in this position", self.0.to_uci())
    }
}

impl std::error::Error for IllegalMove {}

/// Counts how many entries of `history` equal `key`.
///
/// This is the primitive behind repetition detection: a position has *occurred*
/// `count_repetitions(history, key)` times, where `history` holds the Zobrist
/// key of every position that has arisen (including the current one).
#[must_use]
pub fn count_repetitions(history: &[Zobrist], key: Zobrist) -> usize {
    history.iter().filter(|&&k| k == key).count()
}

/// Returns `true` if `key` appears at least `threshold` times in `history`.
#[must_use]
pub fn is_repetition(history: &[Zobrist], key: Zobrist, threshold: usize) -> bool {
    count_repetitions(history, key) >= threshold
}

/// A game in progress: a [`Position`] plus the [`Zobrist`] keys of every position
/// that has occurred, which is what repetition detection needs.
///
/// [`Position`] is kept deliberately history-free; `Game` is the lightweight
/// wrapper that records history. Advance the game with [`Game::play`], which
/// validates legality and records the key of the resulting position.
///
/// ```
/// use mce::{Game, Outcome};
/// let mut game = Game::from_startpos();
/// // Shuffle knights back and forth; the start position recurs.
/// for uci in ["g1f3", "g8f6", "f3g1", "f6g8"] {
///     let mv = game.position().parse_uci(uci).unwrap();
///     game.play(&mv).unwrap();
/// }
/// // The starting position has now occurred twice (start + once more).
/// assert!(!game.is_threefold_repetition());
/// assert_eq!(game.outcome(), None);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Game {
    position: Position,
    /// The Zobrist key of every position that has occurred, oldest first, with
    /// the current position's key last. Always non-empty.
    history: Vec<Zobrist>,
}

impl Game {
    /// Starts a game from `position`. Its key seeds the (otherwise empty)
    /// history, so it counts as its own first occurrence.
    #[must_use]
    pub fn new(position: Position) -> Game {
        let history = vec![position.zobrist()];
        Game { position, history }
    }

    /// Starts a game from the standard chess starting position.
    #[must_use]
    pub fn from_startpos() -> Game {
        Game::new(Position::startpos())
    }

    /// The current position.
    #[must_use]
    #[inline]
    pub fn position(&self) -> &Position {
        &self.position
    }

    /// The Zobrist keys of every position that has occurred, oldest first and the
    /// current one last.
    #[must_use]
    #[inline]
    pub fn history(&self) -> &[Zobrist] {
        &self.history
    }

    /// The legal moves in the current position.
    #[must_use]
    #[inline]
    pub fn legal_moves(&self) -> Vec<Move> {
        self.position.legal_moves()
    }

    /// Applies `mv`, advancing the game and recording the new position's Zobrist
    /// key.
    ///
    /// # Errors
    ///
    /// Returns [`IllegalMove`] if `mv` is not legal in the current position; the
    /// game is left unchanged.
    pub fn play(&mut self, mv: &Move) -> Result<(), IllegalMove> {
        if !self.position.is_legal(mv) {
            return Err(IllegalMove(*mv));
        }
        self.position = self.position.play(mv);
        self.history.push(self.position.zobrist());
        Ok(())
    }

    /// How many times the current position has occurred so far (always ≥ 1).
    #[must_use]
    pub fn repetition_count(&self) -> usize {
        count_repetitions(&self.history, self.position.zobrist())
    }

    /// Whether the current position has occurred three times (claimable
    /// threefold repetition).
    #[must_use]
    pub fn is_threefold_repetition(&self) -> bool {
        self.repetition_count() >= THREEFOLD
    }

    /// Whether the current position has occurred five times (automatic fivefold
    /// repetition).
    #[must_use]
    pub fn is_fivefold_repetition(&self) -> bool {
        self.repetition_count() >= FIVEFOLD
    }

    /// Whether the fifty-move rule may be claimed (halfmove clock ≥ 100 plies).
    #[must_use]
    pub fn is_fifty_move_rule(&self) -> bool {
        self.position.halfmove_clock() >= FIFTY_MOVE_PLIES
    }

    /// Whether the seventy-five-move rule applies (halfmove clock ≥ 150 plies),
    /// which ends the game automatically.
    #[must_use]
    pub fn is_seventy_five_move_rule(&self) -> bool {
        self.position.halfmove_clock() >= SEVENTY_FIVE_MOVE_PLIES
    }

    /// The reason the game has ended automatically, or `None` if it is still in
    /// progress.
    ///
    /// This is the union of the single-position reasons ([`Position::end_reason`])
    /// and automatic fivefold repetition. The claimable reasons (fifty-move rule,
    /// threefold repetition) are never returned here.
    #[must_use]
    pub fn end_reason(&self) -> Option<EndReason> {
        if let Some(reason) = self.position.end_reason() {
            return Some(reason);
        }
        if self.is_fivefold_repetition() {
            return Some(EndReason::FivefoldRepetition);
        }
        None
    }

    /// The automatic game result, or `None` if the game is still in progress.
    ///
    /// Combines the single-position outcome ([`Position::outcome`]) with automatic
    /// fivefold repetition. Claimable draws do not end the game and so never
    /// produce an outcome here.
    #[must_use]
    pub fn outcome(&self) -> Option<Outcome> {
        self.end_reason()
            .map(|reason| reason.outcome(self.position.turn()))
    }

    /// Whether the game is automatically drawn.
    #[must_use]
    pub fn is_draw(&self) -> bool {
        matches!(self.outcome(), Some(Outcome::Draw))
    }

    /// Whether the game has automatically ended (decisively or in a draw).
    #[must_use]
    pub fn is_over(&self) -> bool {
        self.outcome().is_some()
    }
}

impl From<Position> for Game {
    #[inline]
    fn from(position: Position) -> Game {
        Game::new(position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plays a sequence of UCI moves through a [`Game`], asserting each is legal.
    fn play_line(game: &mut Game, ucis: &[&str]) {
        for uci in ucis {
            let mv = game.position().parse_uci(uci).expect("legal uci move");
            game.play(&mv).expect("legal move");
        }
    }

    #[test]
    fn fools_mate_is_decisive_for_black() {
        // 1.f3 e5 2.g4 Qh4# — fastest mate; white is mated, so black wins.
        let mut game = Game::from_startpos();
        play_line(&mut game, &["f2f3", "e7e5", "g2g4", "d8h4"]);
        assert!(game.position().is_checkmate());
        assert_eq!(
            game.position().outcome(),
            Some(Outcome::Decisive {
                winner: Color::Black
            })
        );
        assert_eq!(
            game.outcome(),
            Some(Outcome::Decisive {
                winner: Color::Black
            })
        );
        assert_eq!(game.end_reason(), Some(EndReason::Checkmate));
    }

    #[test]
    fn checkmate_winner_is_side_not_to_move() {
        // Black to move and mated (back-rank style): white wins.
        let pos = Position::from_fen("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1").unwrap();
        // Set up an actual mate: rook delivers it from a8.
        let mated =
            Position::from_fen("R5k1/5ppp/8/8/8/8/8/6K1 b - - 0 1").unwrap_or_else(|_| pos.clone());
        assert!(mated.is_checkmate());
        assert_eq!(
            mated.outcome(),
            Some(Outcome::Decisive {
                winner: Color::White
            })
        );
    }

    #[test]
    fn stalemate_is_a_draw() {
        // Black king on h8 has no move and is not in check.
        let pos = Position::from_fen("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1").unwrap();
        assert!(pos.is_stalemate());
        assert_eq!(pos.outcome(), Some(Outcome::Draw));
        assert_eq!(pos.end_reason(), Some(EndReason::Stalemate));
    }

    #[test]
    fn insufficient_material_outcomes() {
        // K vs K.
        let kk = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        assert_eq!(kk.outcome(), Some(Outcome::Draw));
        assert_eq!(kk.end_reason(), Some(EndReason::InsufficientMaterial));

        // K+B vs K.
        let kbk = Position::from_fen("4k3/8/8/8/8/8/8/4KB2 w - - 0 1").unwrap();
        assert_eq!(kbk.outcome(), Some(Outcome::Draw));

        // K+N vs K.
        let knk = Position::from_fen("4k3/8/8/8/8/8/8/4KN2 w - - 0 1").unwrap();
        assert_eq!(knk.outcome(), Some(Outcome::Draw));

        // K+B vs K+B, both bishops on the light complex (c4 and d5): draw.
        let same = Position::from_fen("4k3/8/8/3b4/2B5/8/8/4K3 w - - 0 1").unwrap();
        assert!(same.is_insufficient_material());
        assert_eq!(same.outcome(), Some(Outcome::Draw));

        // K+B vs K+B on opposite colors (c5 dark, c4 light): NOT insufficient,
        // because a mate can be set up.
        let opp = Position::from_fen("4k3/8/8/2b5/2B5/8/8/4K3 w - - 0 1").unwrap();
        assert!(!opp.is_insufficient_material());
        assert_eq!(opp.outcome(), None);

        // K+R vs K and K+P vs K are sufficient.
        assert_eq!(
            Position::from_fen("4k3/8/8/8/8/8/8/4KR2 w - - 0 1")
                .unwrap()
                .outcome(),
            None
        );
        assert_eq!(
            Position::from_fen("4k3/8/8/8/8/4P3/8/4K3 w - - 0 1")
                .unwrap()
                .outcome(),
            None
        );
    }

    #[test]
    fn ongoing_game_has_no_outcome() {
        let game = Game::from_startpos();
        assert_eq!(game.outcome(), None);
        assert!(!game.is_over());
        assert!(!game.is_draw());
        assert_eq!(game.repetition_count(), 1);
    }

    #[test]
    fn illegal_move_is_rejected_and_leaves_game_unchanged() {
        let mut game = Game::from_startpos();
        // e2e5 is not a legal opening move.
        let bogus = Move::new(crate::Square::E2, crate::Square::E5, crate::MoveKind::Quiet);
        let before = game.clone();
        assert_eq!(game.play(&bogus), Err(IllegalMove(bogus)));
        assert_eq!(game, before);
    }

    #[test]
    fn threefold_then_fivefold_repetition() {
        let mut game = Game::from_startpos();
        // The starting position has occurred once.
        assert_eq!(game.repetition_count(), 1);
        assert!(!game.is_threefold_repetition());

        // One knight-shuffle cycle returns to the start: 2nd occurrence.
        play_line(&mut game, &["g1f3", "g8f6", "f3g1", "f6g8"]);
        assert_eq!(game.repetition_count(), 2);
        assert!(!game.is_threefold_repetition());
        assert_eq!(game.outcome(), None);

        // 2nd cycle: 3rd occurrence -> threefold claimable, still not automatic.
        play_line(&mut game, &["g1f3", "g8f6", "f3g1", "f6g8"]);
        assert_eq!(game.repetition_count(), 3);
        assert!(game.is_threefold_repetition());
        assert!(!game.is_fivefold_repetition());
        // Threefold is claimable, never automatic: outcome is still None.
        assert_eq!(game.outcome(), None);
        assert_eq!(game.end_reason(), None);

        // 3rd cycle: 4th occurrence, still not automatic.
        play_line(&mut game, &["g1f3", "g8f6", "f3g1", "f6g8"]);
        assert_eq!(game.repetition_count(), 4);
        assert_eq!(game.outcome(), None);

        // 4th cycle: 5th occurrence -> fivefold, automatic draw.
        play_line(&mut game, &["g1f3", "g8f6", "f3g1", "f6g8"]);
        assert_eq!(game.repetition_count(), 5);
        assert!(game.is_fivefold_repetition());
        assert_eq!(game.outcome(), Some(Outcome::Draw));
        assert_eq!(game.end_reason(), Some(EndReason::FivefoldRepetition));
        assert!(game.is_draw());
    }

    #[test]
    fn fifty_and_seventy_five_move_boundaries() {
        // Halfmove clock exactly at 100 plies: fifty-move rule claimable, but not
        // automatic (no outcome yet from the move rule).
        let fifty = Position::from_fen("4k3/8/8/8/8/8/8/Q3K3 w - - 100 80").unwrap();
        let game = Game::new(fifty);
        assert!(game.is_fifty_move_rule());
        assert!(!game.is_seventy_five_move_rule());
        // The fifty-move rule is claimable, so it does not end the game by itself.
        assert_eq!(game.outcome(), None);

        // Just below 150 plies: still not automatic.
        let almost = Position::from_fen("4k3/8/8/8/8/8/8/Q3K3 w - - 149 100").unwrap();
        let game = Game::new(almost);
        assert!(!game.is_seventy_five_move_rule());
        assert_eq!(game.outcome(), None);

        // At 150 plies: the seventy-five-move rule ends the game automatically.
        let seventy_five = Position::from_fen("4k3/8/8/8/8/8/8/Q3K3 w - - 150 100").unwrap();
        let game = Game::new(seventy_five.clone());
        assert!(game.is_seventy_five_move_rule());
        assert_eq!(game.outcome(), Some(Outcome::Draw));
        assert_eq!(
            seventy_five.end_reason(),
            Some(EndReason::SeventyFiveMoveRule)
        );
    }

    #[test]
    fn checkmate_takes_precedence_over_move_clock() {
        // A mate that also happens to have a high halfmove clock is reported as a
        // decisive checkmate, not a draw.
        let pos = Position::from_fen("R5k1/5ppp/8/8/8/8/8/6K1 b - - 120 90").unwrap();
        assert!(pos.is_checkmate());
        assert_eq!(
            pos.outcome(),
            Some(Outcome::Decisive {
                winner: Color::White
            })
        );
    }

    #[test]
    fn count_repetitions_helper() {
        let a = Position::startpos().zobrist();
        let b = Position::startpos()
            .play(&Position::startpos().parse_uci("e2e4").unwrap())
            .zobrist();
        let history = vec![a, b, a, a];
        assert_eq!(count_repetitions(&history, a), 3);
        assert_eq!(count_repetitions(&history, b), 1);
        assert!(is_repetition(&history, a, 3));
        assert!(!is_repetition(&history, a, 4));
    }

    #[test]
    fn end_reason_is_automatic_classification() {
        for r in [
            EndReason::Checkmate,
            EndReason::VariantWin,
            EndReason::KingInTheHill,
            EndReason::ThreeChecks,
            EndReason::RaceFinished,
            EndReason::RaceDraw,
            EndReason::KingExploded,
            EndReason::HordeDefeated,
            EndReason::Stalemate,
            EndReason::InsufficientMaterial,
            EndReason::SeventyFiveMoveRule,
            EndReason::FivefoldRepetition,
        ] {
            assert!(r.is_automatic(), "{r:?} should be automatic");
        }
        for r in [EndReason::FiftyMoveRule, EndReason::ThreefoldRepetition] {
            assert!(!r.is_automatic(), "{r:?} should be claimable");
        }
    }

    #[test]
    fn variant_reasons_map_to_the_right_outcome() {
        // The variant-specific decisive reasons all award the win to the side
        // *not* to move (the side that just moved), like checkmate.
        for r in [
            EndReason::KingInTheHill,
            EndReason::ThreeChecks,
            EndReason::RaceFinished,
            EndReason::KingExploded,
            EndReason::HordeDefeated,
        ] {
            assert_eq!(
                r.outcome(Color::White),
                Outcome::Decisive {
                    winner: Color::Black
                },
                "{r:?} should award the side not to move"
            );
            assert_eq!(
                r.outcome(Color::Black),
                Outcome::Decisive {
                    winner: Color::White
                },
                "{r:?} should award the side not to move"
            );
        }

        // Antichess's VariantWin awards the side *to* move.
        assert_eq!(
            EndReason::VariantWin.outcome(Color::White),
            Outcome::Decisive {
                winner: Color::White
            }
        );

        // The racing both-home reason is a draw regardless of side to move.
        assert_eq!(EndReason::RaceDraw.outcome(Color::White), Outcome::Draw);
        assert_eq!(EndReason::RaceDraw.outcome(Color::Black), Outcome::Draw);
    }
}
