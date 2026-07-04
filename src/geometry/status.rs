//! [`GameStatus`]: one consolidated, cross-variant view of a position's terminal
//! state.
//!
//! The generic engine already computes *why* a game ended ([`WideEndReason`], the
//! labelled rule) and *who* won ([`WideOutcome`], the winner-or-draw), through two
//! parallel entry points:
//!
//! - [`GenericPosition::end_reason`] / [`GenericPosition::outcome`] — the rules a
//!   single position can see (checkmate, stalemate, a variant flag/temple/baring
//!   win, insufficient material, the move-count rule);
//! - [`GenericGame::end_reason`] / [`GenericGame::outcome`] — those plus the
//!   history-dependent rules (repetition / sennichite / perpetual check / perpetual
//!   chase / bikjang / counting).
//!
//! [`GameStatus`] folds those two `Option` returns into **one** total enum a
//! downstream consumer can match on without knowing the variant: it says at once
//! whether the game is [`Ongoing`](GameStatus::Ongoing), a
//! [`Checkmate`](GameStatus::Checkmate) (with the winner), a
//! [`Stalemate`](GameStatus::Stalemate) scored as a draw, a
//! [`VariantWin`](GameStatus::VariantWin) (with the winner *and* the named
//! condition), or a [`Draw`](GameStatus::Draw) (with the named rule).
//!
//! It **delegates**: [`GenericPosition::status`] and [`GenericGame::status`] read
//! the existing `end_reason` / `outcome` hooks and re-shape the pair — no
//! termination rule is duplicated or changed here, so every variant's perft and
//! game result stay byte-identical.
//!
//! # Whether a stalemate is a draw or a loss
//!
//! Most variants score a stalemate (no legal move, not in check) as a **draw**;
//! a few ([`WideVariant::stalemate_is_loss`]) score it as a **loss** for the side
//! to move. This distinction is already resolved by [`GenericPosition::outcome`],
//! so it flows through faithfully: a stalemate-is-draw variant yields
//! [`GameStatus::Stalemate`], while a stalemate-is-loss variant yields
//! [`GameStatus::VariantWin`] labelled [`WideEndReason::Stalemate`].
//!
//! # Which draw / win rules apply per variant
//!
//! Every variant ends by **checkmate** (decisive) and, when the side to move has
//! no move and is not in check, by **stalemate** (a draw unless the variant scores
//! it a loss). Beyond that baseline, the special rules are opt-in per variant:
//!
//! | Rule | [`GameStatus`] arm ([`WideEndReason`]) | Variants |
//! |---|---|---|
//! | **Flag / campmate win** — a king on its goal rank wins | `VariantWin` ([`WideEndReason::VariantWin`]) | Khans, Dobutsu, Mansindam, Empire, Orda Mirror, Shinobi, Synochess, Orda |
//! | **Temple win** — a Divine Lord on the enemy temple wins | `VariantWin` ([`WideEndReason::VariantWin`]) | Chak |
//! | **Baring loss** — a side bared to its lone king loses | `VariantWin` ([`WideEndReason::VariantWin`]) | Shatranj |
//! | **Stalemate is a loss** — stalemate scores as a loss | `VariantWin` ([`WideEndReason::Stalemate`]) | Chennis, Chak, Khans, Empire, Shatranj, Synochess |
//! | **Perpetual-check loss** — the perpetual checker loses | `VariantWin` ([`WideEndReason::PerpetualCheckLoss`]) | Xiangqi, Minixiangqi, Shogi, Minishogi |
//! | **Perpetual-chase loss** — the perpetual chaser loses | `VariantWin` ([`WideEndReason::PerpetualChaseLoss`]) | Xiangqi |
//! | **Bare-king (Robado) draw** — a lone king draws immediately | `Draw` ([`WideEndReason::VariantDraw`]) | Shatar |
//! | **Insufficient material** — no mating material | `Draw` ([`WideEndReason::InsufficientMaterial`]) | Alice, Embassy, Janus, Capablanca, Knightmate, Chigorin, Gothic, Seirawan, Almost, Grand, Amazon |
//! | **Repetition draw** — the position recurs enough times | `Draw` ([`WideEndReason::Repetition`]) | Xiangqi, Minixiangqi, Janggi |
//! | **Sennichite** — a fourfold repetition draw | `Draw` ([`WideEndReason::Sennichite`]) | Shogi, Minishogi |
//! | **Bikjang** — the generals face for two consecutive plies | `Draw` ([`WideEndReason::Bikjang`]) | Janggi |
//! | **Counting draw** — the mating countdown expires | `Draw` ([`WideEndReason::CountingDraw`]) | Makruk, Cambodian, ASEAN |
//!
//! The **move-count rule** ([`WideEndReason::MoveRule`], the generic analogue of
//! the fifty-move rule) is wired up but not enabled by any shipped variant; a
//! variant opts in through [`WideVariant::move_rule_plies`].

use super::game::GenericGame;
use super::position::{GenericPosition, WideOutcome};
use super::variant::{WideEndReason, WideVariant};
use super::Geometry;
use crate::Color;

/// A position's terminal state, folding the *reason* a game ended together with
/// *who* won into one total enum.
///
/// This is the consolidated, variant-agnostic surface: obtain it with
/// [`GenericPosition::status`] (single-position rules) or [`GenericGame::status`]
/// (those plus the history-dependent rules). See the [module docs](self) for the
/// per-variant table of which rules apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameStatus {
    /// The game is still in progress: no terminal rule applies.
    Ongoing,
    /// The side to move is in check and has no legal move. `winner` is the side
    /// that delivered the mate (the side *not* to move). Extinction / multi-royal
    /// losses reach this arm too, since a side that cannot save its royal has no
    /// legal reply to the attack on it.
    Checkmate {
        /// The victorious side.
        winner: Color,
    },
    /// The side to move has no legal move and is *not* in check, and the variant
    /// scores this as a **draw** (the usual rule). Variants that score a stalemate
    /// as a *loss* report [`GameStatus::VariantWin`] labelled
    /// [`WideEndReason::Stalemate`] instead.
    Stalemate,
    /// A variant-specific **decisive** ending: a flag / temple / baring win, a
    /// perpetual-check or perpetual-chase loss, or a stalemate scored as a loss.
    /// `reason` names the condition and `winner` is the victor.
    VariantWin {
        /// The victorious side.
        winner: Color,
        /// The rule under which the game was won.
        reason: WideEndReason,
    },
    /// The game is **drawn**. `reason` names the rule (stalemate is reported as
    /// [`GameStatus::Stalemate`] rather than here; every other drawing rule —
    /// insufficient material, repetition, sennichite, bikjang, counting, the
    /// move-count rule, or a variant-specific draw — lands here).
    Draw {
        /// The rule under which the game was drawn.
        reason: WideEndReason,
    },
}

impl GameStatus {
    /// Folds an `(end_reason, outcome)` pair — the two existing terminal hooks —
    /// into a single [`GameStatus`]. Total: when the game is ongoing both inputs
    /// are `None`; otherwise a reason is always accompanied by an outcome, so the
    /// mismatched cases cannot arise and are treated as ongoing.
    pub(crate) fn from_parts(
        reason: Option<WideEndReason>,
        outcome: Option<WideOutcome>,
    ) -> GameStatus {
        let (Some(reason), Some(outcome)) = (reason, outcome) else {
            return GameStatus::Ongoing;
        };
        match (reason, outcome) {
            (WideEndReason::Checkmate, WideOutcome::Decisive { winner }) => {
                GameStatus::Checkmate { winner }
            }
            (WideEndReason::Stalemate, WideOutcome::Draw) => GameStatus::Stalemate,
            (reason, WideOutcome::Decisive { winner }) => GameStatus::VariantWin { winner, reason },
            (reason, WideOutcome::Draw) => GameStatus::Draw { reason },
        }
    }

    /// Whether the game is still in progress.
    #[must_use]
    #[inline]
    pub fn is_ongoing(self) -> bool {
        matches!(self, GameStatus::Ongoing)
    }

    /// Whether the game has ended (decisively or in a draw).
    #[must_use]
    #[inline]
    pub fn is_over(self) -> bool {
        !self.is_ongoing()
    }

    /// Whether the game ended with a winner (checkmate or a variant win).
    #[must_use]
    #[inline]
    pub fn is_decisive(self) -> bool {
        matches!(
            self,
            GameStatus::Checkmate { .. } | GameStatus::VariantWin { .. }
        )
    }

    /// Whether the game ended in a draw (stalemate scored as a draw, or any
    /// drawing rule).
    #[must_use]
    #[inline]
    pub fn is_draw(self) -> bool {
        matches!(self, GameStatus::Stalemate | GameStatus::Draw { .. })
    }

    /// The winning side, or `None` if the game is ongoing or drawn.
    #[must_use]
    #[inline]
    pub fn winner(self) -> Option<Color> {
        match self {
            GameStatus::Checkmate { winner } | GameStatus::VariantWin { winner, .. } => {
                Some(winner)
            }
            GameStatus::Ongoing | GameStatus::Stalemate | GameStatus::Draw { .. } => None,
        }
    }

    /// The [`WideEndReason`] that ended the game, or `None` if it is ongoing.
    ///
    /// [`GameStatus::Checkmate`] reports [`WideEndReason::Checkmate`] and
    /// [`GameStatus::Stalemate`] reports [`WideEndReason::Stalemate`]; the other
    /// terminal arms carry their reason directly.
    #[must_use]
    #[inline]
    pub fn reason(self) -> Option<WideEndReason> {
        match self {
            GameStatus::Ongoing => None,
            GameStatus::Checkmate { .. } => Some(WideEndReason::Checkmate),
            GameStatus::Stalemate => Some(WideEndReason::Stalemate),
            GameStatus::VariantWin { reason, .. } | GameStatus::Draw { reason } => Some(reason),
        }
    }

    /// The [`WideOutcome`] (decisive winner or draw), or `None` if the game is
    /// ongoing — the winner/draw half of the status.
    #[must_use]
    #[inline]
    pub fn outcome(self) -> Option<WideOutcome> {
        match self {
            GameStatus::Ongoing => None,
            GameStatus::Checkmate { winner } | GameStatus::VariantWin { winner, .. } => {
                Some(WideOutcome::Decisive { winner })
            }
            GameStatus::Stalemate | GameStatus::Draw { .. } => Some(WideOutcome::Draw),
        }
    }
}

impl<G: Geometry, V: WideVariant<G>> GenericPosition<G, V> {
    /// The consolidated [`GameStatus`] derivable from this position alone.
    ///
    /// Delegates to [`end_reason`](GenericPosition::end_reason) and
    /// [`outcome`](GenericPosition::outcome) and folds them into one enum. This
    /// covers the single-position rules only (checkmate, stalemate, a variant
    /// flag / temple / baring win, insufficient material, the move-count rule);
    /// the history-dependent rules need [`GenericGame::status`].
    ///
    /// ```
    /// use mcr::geometry::{Chess8x8, GameStatus, GenericPosition, StandardChess};
    /// use mcr::Color;
    /// // Fool's mate: white is checkmated, so Black wins.
    /// let pos = GenericPosition::<Chess8x8, StandardChess>::from_fen(
    ///     "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3",
    /// )
    /// .unwrap();
    /// assert_eq!(pos.status(), GameStatus::Checkmate { winner: Color::Black });
    /// ```
    #[must_use]
    pub fn status(&self) -> GameStatus {
        GameStatus::from_parts(self.end_reason(), self.outcome())
    }
}

impl<G: Geometry, V: WideVariant<G>> GenericGame<G, V> {
    /// The consolidated [`GameStatus`] of the current position, including the
    /// history-dependent rules.
    ///
    /// Delegates to [`end_reason`](GenericGame::end_reason) and
    /// [`outcome`](GenericGame::outcome) — so it sees repetition, sennichite,
    /// perpetual check / chase, bikjang, and the counting draw in addition to
    /// every single-position rule [`GenericPosition::status`] reports.
    #[must_use]
    pub fn status(&self) -> GameStatus {
        GameStatus::from_parts(self.end_reason(), self.outcome())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::variants::{
        Capablanca, Chak, Janggi, Makruk, Minixiangqi, Shatar, Shatranj, Shogi, Synochess, Xiangqi,
    };
    use crate::geometry::{Chess8x8, GenericGame, GenericPosition, StandardChess, WideMove};
    use crate::geometry::{Geometry, WideVariant};

    /// Plays the move `from`->`to` (by square index) through a [`GenericGame`],
    /// asserting it is legal.
    fn play<G: Geometry, V: WideVariant<G>>(game: &mut GenericGame<G, V>, from: u8, to: u8) {
        let mv: WideMove = game
            .legal_moves()
            .into_iter()
            .find(|m| m.from_index() == from && m.to::<G>().index() == to)
            .unwrap_or_else(|| panic!("expected a legal move {from}->{to}"));
        game.play(&mv).expect("legal move");
    }

    // --- Ongoing ---------------------------------------------------------

    #[test]
    fn startpos_is_ongoing() {
        let status = GenericPosition::<Chess8x8, StandardChess>::startpos().status();
        assert_eq!(status, GameStatus::Ongoing);
        assert!(status.is_ongoing());
        assert!(!status.is_over());
        assert_eq!(status.winner(), None);
        assert_eq!(status.reason(), None);
    }

    // --- Checkmate (standard) --------------------------------------------

    #[test]
    fn fools_mate_is_checkmate_for_black() {
        let pos = GenericPosition::<Chess8x8, StandardChess>::from_fen(
            "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3",
        )
        .expect("valid fen");
        let status = pos.status();
        assert_eq!(
            status,
            GameStatus::Checkmate {
                winner: Color::Black
            }
        );
        assert!(status.is_decisive());
        assert_eq!(status.winner(), Some(Color::Black));
        assert_eq!(status.reason(), Some(WideEndReason::Checkmate));
        // The game wrapper agrees with the bare position.
        assert_eq!(GenericGame::new(pos).status(), status);
    }

    // --- Stalemate scored as a draw (standard) ---------------------------

    #[test]
    fn stalemate_is_a_draw_in_standard_chess() {
        // Black king h8 has no move and is not in check.
        let pos =
            GenericPosition::<Chess8x8, StandardChess>::from_fen("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1")
                .expect("valid fen");
        let status = pos.status();
        assert_eq!(status, GameStatus::Stalemate);
        assert!(status.is_draw());
        assert_eq!(status.winner(), None);
        assert_eq!(status.reason(), Some(WideEndReason::Stalemate));
    }

    // --- Stalemate scored as a loss (Synochess) --------------------------

    #[test]
    fn stalemate_is_a_loss_in_synochess() {
        // Black king a8 stalemated: not in check, no move, and Synochess scores it
        // a loss — a variant win for White, labelled with the stalemate reason.
        let pos =
            Synochess::from_fen("k7/2K5/1Q6/8/8/8/8/8 b - - 0 1").expect("valid synochess fen");
        assert!(pos.legal_moves().is_empty());
        assert!(!pos.is_check());
        let status = pos.status();
        assert_eq!(
            status,
            GameStatus::VariantWin {
                winner: Color::White,
                reason: WideEndReason::Stalemate,
            }
        );
        assert!(status.is_decisive());
        assert_eq!(status.winner(), Some(Color::White));
    }

    // --- Variant win: flag / campmate (Synochess) ------------------------

    #[test]
    fn synochess_campmate_is_a_variant_win() {
        // A Black king on rank 1 has reached its goal: a flag win for Black.
        let pos =
            Synochess::from_fen("8/8/8/8/8/8/4K3/3k4 w - - 0 1").expect("valid synochess fen");
        let status = pos.status();
        assert_eq!(
            status,
            GameStatus::VariantWin {
                winner: Color::Black,
                reason: WideEndReason::VariantWin,
            }
        );
    }

    // --- Variant win: baring (Shatranj) ----------------------------------

    #[test]
    fn shatranj_baring_is_a_variant_win() {
        // White is bared to its lone king while Black holds three pieces: Black wins.
        let pos =
            Shatranj::from_fen("4k3/8/8/8/8/8/3mm3/4K3 w - - 0 1").expect("valid shatranj fen");
        let status = pos.status();
        assert_eq!(
            status,
            GameStatus::VariantWin {
                winner: Color::Black,
                reason: WideEndReason::VariantWin,
            }
        );
    }

    // --- Variant win: temple (Chak) --------------------------------------

    #[test]
    fn chak_temple_win_is_a_variant_win() {
        // A White Divine Lord (`*L`) stands on the enemy temple square e8 (Black to
        // move): the temple-reaching side, White, has won.
        let pos = Chak::from_fen("4k4/4*L4/9/9/9/9/9/9/4K4 b - - 0 1").expect("valid chak fen");
        let status = pos.status();
        assert_eq!(
            status,
            GameStatus::VariantWin {
                winner: Color::White,
                reason: WideEndReason::VariantWin,
            }
        );
    }

    // --- Variant win: perpetual check (Shogi) ----------------------------

    #[test]
    fn shogi_perpetual_check_is_a_variant_win() {
        let pos = Shogi::from_fen("9/9/9/9/4K4/9/9/8R/k8 w - - 0 1").expect("valid shogi fen");
        let mut game = GenericGame::new(pos);
        for _ in 0..3 {
            play(&mut game, 17, 8); // R i2->i1+, checks a1
            play(&mut game, 0, 9); // K a1->a2
            play(&mut game, 8, 17); // R i1->i2+, checks a2
            play(&mut game, 9, 0); // K a2->a1
        }
        // White checked on every move, so White loses: Black wins.
        assert_eq!(
            game.status(),
            GameStatus::VariantWin {
                winner: Color::Black,
                reason: WideEndReason::PerpetualCheckLoss,
            }
        );
    }

    // --- Variant win: perpetual chase (Xiangqi) --------------------------

    #[test]
    fn xiangqi_perpetual_chase_is_a_variant_win() {
        let pos =
            Xiangqi::from_fen("4k4/9/9/9/9/9/2J6/3r5/9/5K3 w - - 0 1").expect("valid xiangqi fen");
        let mut game = GenericGame::new(pos);
        for _ in 0..2 {
            play(&mut game, 29, 40); // J c4->e5, chases the chariot
            play(&mut game, 21, 48); // r d3->d6 flees
            play(&mut game, 40, 29); // J e5->c4, chases again
            play(&mut game, 48, 21); // r d6->d3 flees
        }
        assert_eq!(
            game.status(),
            GameStatus::VariantWin {
                winner: Color::Black,
                reason: WideEndReason::PerpetualChaseLoss,
            }
        );
    }

    // --- Draw: insufficient material (Capablanca) ------------------------

    #[test]
    fn capablanca_king_vs_king_is_an_insufficient_material_draw() {
        let pos =
            Capablanca::from_fen("5k4/10/10/10/10/10/10/5K4 w - - 0 1").expect("valid capa fen");
        let status = pos.status();
        assert_eq!(
            status,
            GameStatus::Draw {
                reason: WideEndReason::InsufficientMaterial,
            }
        );
        assert!(status.is_draw());
    }

    // --- Draw: bare-king Robado (Shatar) ---------------------------------

    #[test]
    fn shatar_bare_king_is_a_variant_draw() {
        // White is reduced to a lone king (Black keeps two rooks): an immediate
        // Robado draw.
        let pos = Shatar::from_fen("4k3/8/8/r6r/8/8/8/4K3 w - - 0 1").expect("valid shatar fen");
        let status = pos.status();
        assert_eq!(
            status,
            GameStatus::Draw {
                reason: WideEndReason::VariantDraw,
            }
        );
    }

    // --- Draw: plain repetition (Minixiangqi) ----------------------------

    #[test]
    fn minixiangqi_repetition_is_a_draw() {
        let pos =
            Minixiangqi::from_fen("2k4/7/7/7/7/7/3K3 w - - 0 1").expect("valid minixiangqi fen");
        let mut game = GenericGame::new(pos);
        for _ in 0..2 {
            play(&mut game, 3, 10);
            play(&mut game, 44, 37);
            play(&mut game, 10, 3);
            play(&mut game, 37, 44);
        }
        assert_eq!(
            game.status(),
            GameStatus::Draw {
                reason: WideEndReason::Repetition,
            }
        );
    }

    // --- Draw: sennichite (Shogi) ----------------------------------------

    #[test]
    fn shogi_sennichite_is_a_draw() {
        let pos = Shogi::from_fen("k8/9/9/9/9/9/9/9/4K4 w - - 0 1").expect("valid shogi fen");
        let mut game = GenericGame::new(pos);
        for _ in 0..3 {
            play(&mut game, 4, 13);
            play(&mut game, 72, 63);
            play(&mut game, 13, 4);
            play(&mut game, 63, 72);
        }
        assert_eq!(
            game.status(),
            GameStatus::Draw {
                reason: WideEndReason::Sennichite,
            }
        );
    }

    // --- Draw: bikjang (Janggi) ------------------------------------------

    #[test]
    fn janggi_bikjang_is_a_draw() {
        let pos = Janggi::from_fen("4k4/9/9/9/9/9/9/9/9/4K4 w - - 0 1").expect("valid janggi fen");
        let mut game = GenericGame::new(pos);
        play(&mut game, 4, 4); // White passes; the generals stay faced.
        assert_eq!(
            game.status(),
            GameStatus::Draw {
                reason: WideEndReason::Bikjang,
            }
        );
    }

    // --- Draw: counting (Makruk) -----------------------------------------

    #[test]
    fn makruk_counting_is_a_draw() {
        let pos = Makruk::from_fen("k7/8/8/8/8/8/8/2R3K1 w - - 0 1").expect("valid makruk fen");
        let mut game = GenericGame::new(pos);
        let cycle = [(2u8, 10u8), (56, 48), (10, 2), (48, 56)];
        for i in 0..60 {
            let (from, to) = cycle[i % cycle.len()];
            play(&mut game, from, to);
            if game.status().is_over() {
                break;
            }
        }
        assert_eq!(
            game.status(),
            GameStatus::Draw {
                reason: WideEndReason::CountingDraw,
            }
        );
    }

    // --- Accessor coverage -----------------------------------------------

    #[test]
    fn accessors_agree_with_underlying_hooks() {
        // A decisive status exposes its winner and reason; ongoing exposes neither.
        let mate = GenericPosition::<Chess8x8, StandardChess>::from_fen(
            "R5k1/5ppp/8/8/8/8/8/6K1 b - - 0 1",
        )
        .expect("valid fen");
        let s = mate.status();
        assert_eq!(s.outcome(), mate.outcome());
        assert_eq!(s.reason(), mate.end_reason());
        assert!(s.is_over() && s.is_decisive() && !s.is_draw());
        assert_eq!(s.winner(), Some(Color::White));
    }
}
