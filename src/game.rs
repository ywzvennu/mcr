//! The unified top-level game surface: [`Game`], one variant-agnostic handle over
//! **every** shipped variant — the concrete 8×8 engine and the generic-geometry
//! fairy layer alike.
//!
//! mcr ships two parallel runtime-dispatch position types (see the crate docs):
//! the concrete [`AnyVariant`] (standard chess and the eight classic 8×8 variants)
//! and the wider [`AnyWideVariant`](crate::geometry::AnyWideVariant) (Shogi,
//! Xiangqi, Chu Shogi, Capablanca, … — the ~90 non-8×8 variants). They share the
//! same feel but are distinct types keyed by distinct selectors, and the unified
//! catalog key [`VariantRef`] spans both. A consumer that wants to *play* any
//! variant — a bot, a server, a front-end — would otherwise have to branch on
//! which family a variant belongs to and juggle two move types
//! ([`Move`](crate::Move) vs [`WideMove`](crate::geometry::WideMove)) and two
//! outcome types.
//!
//! [`Game`] closes that gap: a thin enum wrapping whichever family a
//! [`VariantRef`] names, with one uniform surface — [`legal_moves`](Game::legal_moves),
//! [`play`](Game::play) / [`play_uci`](Game::play_uci), [`to_move`](Game::to_move),
//! [`fen`](Game::fen), [`outcome`](Game::outcome), [`rules`](Game::rules) — that a
//! caller uses without ever naming a concrete variant type or matching on the
//! [`Concrete`](Game::Concrete) / [`Wide`](Game::Wide) family. Moves are carried
//! by the unified [`GameMove`] and results by the unified [`GameOutcome`]. Every
//! method forwards through a single `match` to the inner [`AnyVariant`] /
//! [`AnyWideVariant`](crate::geometry::AnyWideVariant), so it reimplements no move
//! generation and is exactly as correct as the layer beneath it.
//!
//! This is the production surface. The lower [`AnyVariant`] /
//! [`AnyWideVariant`](crate::geometry::AnyWideVariant) types remain available for
//! callers that know their family and want the family-specific extras (the wide
//! layer's analysis queries, allocation-free move listing, the concrete layer's
//! staged move ordering).
//!
//! # Example
//!
//! ```
//! use mcr::{Game, GameOutcome, VariantRef};
//!
//! // Make a game of any variant from its catalog key — here standard chess.
//! let start = Game::new(VariantRef::from_name("chess").unwrap());
//! assert_eq!(start.legal_moves().len(), 20);
//!
//! // Play a move by UCI, uniformly, without knowing the variant family.
//! let after = start.play_uci("e2e4").unwrap();
//! assert!(after.outcome().is_none());
//! assert!(after.fen().starts_with("rnbqkbnr/pppppppp/8/8/4P3"));
//!
//! // The same surface drives a wide (non-8×8) variant.
//! let shogi = Game::new(VariantRef::from_name("shogi").unwrap());
//! assert!(!shogi.legal_moves().is_empty());
//! assert_eq!(shogi.rules().board.width, 9);
//! ```

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use crate::catalog::VariantRef;
use crate::geometry::rules::VariantRules;
use crate::geometry::{AnyWideVariant, PlayerView, WideFenError, WideMove, WideOutcome};
use crate::position::FenError;
use crate::variant::AnyVariant;
use crate::{Color, Move, Outcome, Square};

/// A move in a [`Game`], spanning both variant families: a concrete 8×8
/// [`Move`](crate::Move) or a wide-geometry [`WideMove`](crate::geometry::WideMove).
///
/// A `GameMove` is produced by [`Game::legal_moves`] /
/// [`Game::legal_moves_from`] / [`Game::parse_uci`] and consumed by
/// [`Game::play`] / [`Game::to_uci`]. Common flows — list moves, play one, render
/// it — never need to match on the family; do so only to reach into the concrete
/// [`Move`](crate::Move) or [`WideMove`](crate::geometry::WideMove) when a
/// caller wants family-specific detail.
///
/// A `GameMove` renders to UCI through its [`Game`]
/// ([`Game::to_uci`] / [`Game::legal_ucis`]) rather than on its own: a wide move's
/// UCI depends on its board geometry, which only the game (not a bare move) knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameMove {
    /// A move in a concrete 8×8 variant.
    Concrete(Move),
    /// A move in a wide-geometry fairy variant.
    Wide(WideMove),
}

impl GameMove {
    /// The wrapped concrete 8×8 [`Move`](crate::Move), or `None` for a wide move.
    #[must_use]
    #[inline]
    pub fn as_concrete(self) -> Option<Move> {
        match self {
            GameMove::Concrete(mv) => Some(mv),
            GameMove::Wide(_) => None,
        }
    }

    /// The wrapped wide-geometry [`WideMove`](crate::geometry::WideMove), or `None`
    /// for a concrete move.
    #[must_use]
    #[inline]
    pub fn as_wide(self) -> Option<WideMove> {
        match self {
            GameMove::Wide(mv) => Some(mv),
            GameMove::Concrete(_) => None,
        }
    }
}

/// The result of a finished [`Game`], spanning both variant families: the unified
/// counterpart of the concrete [`Outcome`](crate::Outcome) and the wide
/// [`WideOutcome`](crate::geometry::WideOutcome), which share this shape.
///
/// Obtain it from [`Game::outcome`]; it is `None` while the game is in progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GameOutcome {
    /// One side won; `winner` is the victorious color.
    Decisive {
        /// The side that won.
        winner: Color,
    },
    /// The game was drawn.
    Draw,
}

impl GameOutcome {
    /// The winning side, or `None` for a draw.
    #[must_use]
    #[inline]
    pub fn winner(self) -> Option<Color> {
        match self {
            GameOutcome::Decisive { winner } => Some(winner),
            GameOutcome::Draw => None,
        }
    }

    /// Whether the game was decided in favour of one side.
    #[must_use]
    #[inline]
    pub fn is_decisive(self) -> bool {
        matches!(self, GameOutcome::Decisive { .. })
    }

    /// Whether the game was drawn.
    #[must_use]
    #[inline]
    pub fn is_draw(self) -> bool {
        matches!(self, GameOutcome::Draw)
    }
}

impl From<Outcome> for GameOutcome {
    #[inline]
    fn from(outcome: Outcome) -> GameOutcome {
        match outcome {
            Outcome::Decisive { winner } => GameOutcome::Decisive { winner },
            Outcome::Draw => GameOutcome::Draw,
        }
    }
}

impl From<WideOutcome> for GameOutcome {
    #[inline]
    fn from(outcome: WideOutcome) -> GameOutcome {
        match outcome {
            WideOutcome::Decisive { winner } => GameOutcome::Decisive { winner },
            WideOutcome::Draw => GameOutcome::Draw,
        }
    }
}

/// The error [`Game::from_fen`] returns when a FEN fails to parse, dispatched to
/// the family the [`VariantRef`] named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameFenError {
    /// A concrete 8×8 variant's FEN error.
    Concrete(FenError),
    /// A wide-geometry variant's FEN error.
    Wide(WideFenError),
}

impl core::fmt::Display for GameFenError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            GameFenError::Concrete(err) => write!(f, "{err}"),
            GameFenError::Wide(err) => write!(f, "{err}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for GameFenError {}

/// A playable game of **any** shipped variant: a thin enum over the concrete 8×8
/// runtime dispatch ([`AnyVariant`]) and the wide-geometry runtime dispatch
/// ([`AnyWideVariant`](crate::geometry::AnyWideVariant)), presenting one uniform,
/// variant-agnostic surface.
///
/// Build one from a [`VariantRef`] — the unified catalog key that spans both
/// families — with [`Game::new`] (start position) or [`Game::from_fen`], then use
/// the same methods for every variant: [`legal_moves`](Game::legal_moves),
/// [`play`](Game::play) / [`play_uci`](Game::play_uci),
/// [`to_move`](Game::to_move), [`fen`](Game::fen), [`outcome`](Game::outcome), and
/// [`rules`](Game::rules). None of the common flows require matching on the
/// [`Concrete`](Game::Concrete) / [`Wide`](Game::Wide) family.
///
/// `Game` is a *single-position* handle: it answers the rules questions a position
/// can see on its own (legal moves, the automatic single-position terminations).
/// The history-dependent rules (threefold / fivefold repetition, sennichite,
/// perpetual check / chase, bikjang, counting) need a move-history wrapper and are
/// out of scope here — the concrete [`ChessGame`](crate::ChessGame) and the wide
/// [`GenericGame`](crate::geometry::GenericGame) provide them per family.
///
/// See [`Game::new`] and [`Game::play_uci`] for worked examples.
///
/// The wide arm is boxed: [`AnyWideVariant`](crate::geometry::AnyWideVariant) is a
/// wide facade (sized by its widest inline position, far larger than a concrete
/// 8×8 one), so storing it behind a [`Box`] keeps a `Game` small and cheap to move
/// for the common concrete case — the same footprint discipline the wide facade
/// applies to its own giant arms.
#[derive(Debug, Clone)]
pub enum Game {
    /// A concrete 8×8 variant (standard chess and the eight classic 8×8 variants).
    Concrete(AnyVariant),
    /// A wide-geometry fairy variant (Shogi, Xiangqi, Chu Shogi, Capablanca, …).
    Wide(Box<AnyWideVariant>),
}

impl Game {
    /// The starting position of the variant named by `variant`.
    ///
    /// ```
    /// use mcr::{Game, VariantRef};
    /// let g = Game::new(VariantRef::from_name("atomic").unwrap());
    /// assert_eq!(g.variant(), VariantRef::from_name("atomic").unwrap());
    /// assert_eq!(g.legal_moves().len(), 20);
    /// ```
    #[must_use]
    pub fn new(variant: VariantRef) -> Game {
        match variant {
            VariantRef::Concrete(id) => Game::Concrete(AnyVariant::startpos(id)),
            VariantRef::Wide(id) => Game::Wide(Box::new(AnyWideVariant::startpos(id))),
        }
    }

    /// Parses a game of the variant named by `variant` from `fen`, dispatching to
    /// the concrete or wide from-FEN parser.
    ///
    /// # Errors
    ///
    /// Returns [`GameFenError`] if `fen` is malformed or fails the variant's
    /// validation (wrapping the family-specific [`FenError`](crate::FenError) /
    /// [`WideFenError`](crate::geometry::WideFenError)).
    pub fn from_fen(variant: VariantRef, fen: &str) -> Result<Game, GameFenError> {
        match variant {
            VariantRef::Concrete(id) => AnyVariant::from_fen(id, fen)
                .map(Game::Concrete)
                .map_err(GameFenError::Concrete),
            VariantRef::Wide(id) => AnyWideVariant::from_fen(id, fen)
                .map(|p| Game::Wide(Box::new(p)))
                .map_err(GameFenError::Wide),
        }
    }

    /// The unified catalog key of this game's variant.
    #[must_use]
    pub fn variant(&self) -> VariantRef {
        match self {
            Game::Concrete(p) => VariantRef::Concrete(p.variant_id()),
            Game::Wide(p) => VariantRef::Wide(p.variant_id()),
        }
    }

    /// The side to move.
    #[must_use]
    pub fn to_move(&self) -> Color {
        match self {
            Game::Concrete(p) => p.turn(),
            Game::Wide(p) => p.turn(),
        }
    }

    /// Whether the side to move is in check (always `false` where the king is not
    /// royal).
    #[must_use]
    pub fn is_check(&self) -> bool {
        match self {
            Game::Concrete(p) => p.is_check(),
            Game::Wide(p) => p.is_check(),
        }
    }

    /// The legal moves of the side to move, as unified [`GameMove`]s.
    #[must_use]
    pub fn legal_moves(&self) -> Vec<GameMove> {
        match self {
            Game::Concrete(p) => p
                .legal_moves()
                .into_iter()
                .map(GameMove::Concrete)
                .collect(),
            Game::Wide(p) => p.legal_moves().into_iter().map(GameMove::Wide).collect(),
        }
    }

    /// The legal moves of the side to move whose origin is the square at index
    /// `square`, as unified [`GameMove`]s.
    ///
    /// `square` is the geometry-agnostic little-endian index
    /// (`rank * width + file`, `0..width * height`) — `0..64` for the concrete 8×8
    /// variants. An index off this variant's board yields an empty list. A drop is
    /// grouped under the square it drops onto (its packed origin equals its
    /// target).
    #[must_use]
    pub fn legal_moves_from(&self, square: u8) -> Vec<GameMove> {
        match self {
            Game::Concrete(p) => match Square::try_new(square) {
                Some(sq) => p
                    .legal_moves_from(sq)
                    .into_iter()
                    .map(GameMove::Concrete)
                    .collect(),
                None => Vec::new(),
            },
            Game::Wide(p) => p
                .legal_moves_from(square)
                .into_iter()
                .map(GameMove::Wide)
                .collect(),
        }
    }

    /// The legal moves of the side to move, rendered as UCI long-algebraic
    /// strings — the ergonomic list a UI or a bot shows or matches against,
    /// without handling a [`GameMove`] or knowing the variant's geometry.
    #[must_use]
    pub fn legal_ucis(&self) -> Vec<String> {
        match self {
            Game::Concrete(p) => p.legal_moves().iter().map(|mv| p.to_uci(mv)).collect(),
            Game::Wide(p) => p.legal_moves().iter().map(|mv| p.to_uci(mv)).collect(),
        }
    }

    /// Applies `mv`, returning the successor game.
    ///
    /// # Panics
    ///
    /// Panics if `mv` is from the other variant family than this game (a
    /// [`GameMove::Wide`] played on a [`Game::Concrete`] or vice versa). Pass a
    /// move obtained from this game's [`legal_moves`](Game::legal_moves) /
    /// [`parse_uci`](Game::parse_uci); as for the layers beneath, the move must be
    /// legal in this position. To play from an untrusted UCI string without this
    /// obligation, prefer [`play_uci`](Game::play_uci).
    #[must_use]
    pub fn play(&self, mv: &GameMove) -> Game {
        match (self, mv) {
            (Game::Concrete(p), GameMove::Concrete(m)) => Game::Concrete(p.play(m)),
            (Game::Wide(p), GameMove::Wide(m)) => Game::Wide(Box::new(p.play(m))),
            _ => panic!("GameMove family does not match this Game's variant family"),
        }
    }

    /// Parses `uci` against this position and applies it, returning the successor
    /// game, or `None` if the string names no legal move — the primary ergonomic
    /// play path, uniform across families.
    ///
    /// ```
    /// use mcr::{Game, VariantRef};
    /// let g = Game::new(VariantRef::from_name("shogi").unwrap());
    /// // A legal opening pawn push in Shogi.
    /// let after = g.play_uci("g3g4").unwrap();
    /// assert_eq!(after.to_move(), mcr::Color::Black);
    /// // An illegal string is rejected.
    /// assert!(g.play_uci("z9z9").is_none());
    /// ```
    #[must_use]
    pub fn play_uci(&self, uci: &str) -> Option<Game> {
        let mv = self.parse_uci(uci)?;
        Some(self.play(&mv))
    }

    /// Resolves a UCI move string to a legal [`GameMove`] in this position, or
    /// `None` if it names no legal move.
    #[must_use]
    pub fn parse_uci(&self, uci: &str) -> Option<GameMove> {
        match self {
            Game::Concrete(p) => p.parse_uci(uci).ok().map(GameMove::Concrete),
            Game::Wide(p) => p.parse_uci(uci).map(GameMove::Wide),
        }
    }

    /// Renders `mv` as UCI long-algebraic notation for this game's variant
    /// geometry.
    ///
    /// # Panics
    ///
    /// Panics if `mv` is from the other variant family than this game (see
    /// [`play`](Game::play)).
    #[must_use]
    pub fn to_uci(&self, mv: &GameMove) -> String {
        match (self, mv) {
            (Game::Concrete(p), GameMove::Concrete(m)) => p.to_uci(m),
            (Game::Wide(p), GameMove::Wide(m)) => p.to_uci(m),
            _ => panic!("GameMove family does not match this Game's variant family"),
        }
    }

    /// Serializes this position to FEN.
    #[must_use]
    pub fn fen(&self) -> String {
        match self {
            Game::Concrete(p) => p.to_fen(),
            Game::Wide(p) => p.to_fen(),
        }
    }

    /// The unified game result derivable from this position alone, or `None` if the
    /// game is not over by any single-position rule.
    ///
    /// This folds the concrete [`Outcome`](crate::Outcome) and the wide
    /// [`WideOutcome`](crate::geometry::WideOutcome) into one [`GameOutcome`]. As
    /// for the layers beneath, the history-dependent draws (repetition and its
    /// variant analogues) are *not* seen here — a bare game has no move history.
    #[must_use]
    pub fn outcome(&self) -> Option<GameOutcome> {
        match self {
            Game::Concrete(p) => p.outcome().map(GameOutcome::from),
            Game::Wide(p) => p.outcome().map(GameOutcome::from),
        }
    }

    /// Whether the game has ended by a single-position rule ([`outcome`](Game::outcome)
    /// is `Some`).
    #[must_use]
    pub fn is_over(&self) -> bool {
        self.outcome().is_some()
    }

    /// The structured, engine-derived [`VariantRules`](crate::geometry::VariantRules)
    /// for this game's variant — its board, army, and pawn / promotion / castling /
    /// draw / terminal / special-mechanic rules — delegating to the variant's
    /// catalog entry ([`VariantRef::rules`]).
    #[must_use]
    pub fn rules(&self) -> VariantRules {
        self.variant().rules()
    }

    /// This position **as `perspective` may see it** — the per-player redaction
    /// seam, uniform across both variant families.
    ///
    /// mcr is the single source of truth for every variant's rules, including
    /// hidden-information per-player redaction: a consumer asks the game "what
    /// does player `perspective` see?" and never computes redaction itself. For a
    /// perfect-information variant (standard chess, and every concrete 8×8
    /// variant) the returned [`PlayerView`] is the *full* position — its
    /// [`fen`](PlayerView::fen) equals [`fen`](Game::fen) and its
    /// [`legal_ucis`](PlayerView::legal_ucis) equals [`legal_ucis`](Game::legal_ucis),
    /// byte-identical, since there is nothing to hide. For a hidden-information
    /// wide variant (Fog of War) the FEN hides the pieces `perspective` may not
    /// see and the move list is limited to that perspective's own moves.
    ///
    /// ```
    /// use mcr::{Color, Game, VariantRef};
    /// // Perfect information: a perspective view is the full position.
    /// let g = Game::new(VariantRef::from_name("chess").unwrap());
    /// let view = g.view_for(Color::White);
    /// assert_eq!(view.fen, g.fen());
    /// assert_eq!(view.legal_ucis, g.legal_ucis());
    /// ```
    #[must_use]
    pub fn view_for(&self, perspective: Color) -> PlayerView {
        match self {
            // Every concrete 8×8 variant is perfect-information: the full view.
            Game::Concrete(_) => PlayerView {
                perspective: Some(perspective),
                fen: self.fen(),
                legal_ucis: self.legal_ucis(),
            },
            Game::Wide(p) => p.view_for(perspective),
        }
    }

    /// The perspective-less **spectator** view: the full position for a
    /// perfect-information variant, or a doubly redacted board with no move list
    /// for a hidden-information variant in progress (see
    /// [`AnyWideVariant::spectator_view`]).
    #[must_use]
    pub fn spectator_view(&self) -> PlayerView {
        match self {
            Game::Concrete(_) => PlayerView {
                perspective: None,
                fen: self.fen(),
                legal_ucis: self.legal_ucis(),
            },
            Game::Wide(p) => p.spectator_view(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(name: &str) -> VariantRef {
        VariantRef::from_name(name).expect("known variant name")
    }

    #[test]
    fn concrete_standard_play_and_fen_round_trip() {
        let g = Game::new(v("chess"));
        // Concrete family, standard start: 20 legal moves, game not over.
        assert_eq!(g.variant(), v("chess"));
        assert_eq!(g.legal_moves().len(), 20);
        assert_eq!(g.to_move(), Color::White);
        assert_eq!(g.outcome(), None);
        assert!(!g.is_over());

        // Play e2e4 by UCI, uniformly — no matching on Concrete/Wide.
        assert!(g.legal_ucis().contains(&"e2e4".to_string()));
        let after = g.play_uci("e2e4").unwrap();
        assert_eq!(after.to_move(), Color::Black);
        assert!(after.fen().starts_with("rnbqkbnr/pppppppp/8/8/4P3"));

        // FEN round-trips back through from_fen.
        let reparsed = Game::from_fen(v("chess"), &after.fen()).unwrap();
        assert_eq!(reparsed.fen(), after.fen());
        assert_eq!(reparsed.legal_moves().len(), after.legal_moves().len());
    }

    #[test]
    fn wide_shogi_play_and_fen() {
        let g = Game::new(v("shogi"));
        assert_eq!(g.variant(), v("shogi"));
        assert!(!g.legal_moves().is_empty());
        assert_eq!(g.outcome(), None);

        // Play a legal Shogi pawn push by UCI, uniformly.
        let after = g.play_uci("g3g4").unwrap();
        assert_ne!(after.fen(), g.fen());
        assert_eq!(after.to_move(), Color::Black);
    }

    #[test]
    fn play_via_game_move_without_matching_family() {
        // The unified move surface: list, play, and render a move without ever
        // matching on GameMove::Concrete / GameMove::Wide.
        for name in ["chess", "shogi", "xiangqi", "crazyhouse"] {
            let g = Game::new(v(name));
            let mv = g.legal_moves()[0];
            let uci = g.to_uci(&mv);
            // The rendered UCI resolves back to a legal move and plays.
            let played = g.play_uci(&uci).expect("rendered uci is legal");
            let direct = g.play(&mv);
            assert_eq!(
                played.fen(),
                direct.fen(),
                "{name}: play vs play_uci diverge"
            );
        }
    }

    #[test]
    fn terminal_outcome_is_unified() {
        // A concrete decisive terminal: fool's mate, White is mated, Black wins —
        // read through the unified GameOutcome without touching Outcome/WideOutcome.
        let mated = Game::from_fen(
            v("chess"),
            "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3",
        )
        .unwrap();
        assert!(mated.is_over());
        assert_eq!(
            mated.outcome(),
            Some(GameOutcome::Decisive {
                winner: Color::Black
            })
        );
        assert_eq!(mated.outcome().unwrap().winner(), Some(Color::Black));
        assert!(mated.legal_moves().is_empty());

        // A drawn terminal: bare kings, insufficient material.
        let draw = Game::from_fen(v("chess"), "4k3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        assert_eq!(draw.outcome(), Some(GameOutcome::Draw));
        assert!(draw.outcome().unwrap().is_draw());
    }

    #[test]
    fn from_fen_and_rules_for_one_of_each_family() {
        // Concrete: from_fen plus rules() delegate to the catalog entry.
        let concrete = Game::from_fen(v("chess"), "8/8/8/8/8/8/8/4K2k w - - 0 1").unwrap();
        assert_eq!(concrete.rules().board.width, 8);
        assert_eq!(concrete.rules().board.height, 8);

        // Wide: a Xiangqi position parsed from FEN, rules() gives its 9×10 board.
        let wide = Game::new(v("xiangqi"));
        let wide = Game::from_fen(v("xiangqi"), &wide.fen()).unwrap();
        assert_eq!(wide.rules().board.width, 9);
        assert_eq!(wide.rules().board.height, 10);

        // A bad FEN surfaces the unified error for both families.
        assert!(Game::from_fen(v("chess"), "not a fen").is_err());
        assert!(Game::from_fen(v("shogi"), "not a fen").is_err());
    }

    #[test]
    fn view_for_perfect_information_is_the_full_position() {
        // A perfect-information variant hides nothing: `view_for` for either
        // color is byte-identical to the full FEN and full legal-move list, for
        // both a concrete (chess) and a wide (shogi) variant.
        for name in ["chess", "shogi", "xiangqi"] {
            let g = Game::new(v(name));
            for color in [Color::White, Color::Black] {
                let view = g.view_for(color);
                assert_eq!(view.perspective, Some(color));
                assert_eq!(view.fen, g.fen(), "{name}: perfect-info fen redacted");
                assert_eq!(
                    view.legal_ucis,
                    g.legal_ucis(),
                    "{name}: perfect-info move list redacted"
                );
            }
            // The spectator view of a perfect-information game is the full
            // position with no perspective.
            let spectator = g.spectator_view();
            assert_eq!(spectator.perspective, None);
            assert_eq!(spectator.fen, g.fen());
            assert_eq!(spectator.legal_ucis, g.legal_ucis());
        }
    }

    #[test]
    fn view_for_fog_of_war_redacts_through_the_game_surface() {
        // The unified Game surface delegates redaction to the wide layer: a Fog
        // of War game hides the enemy king from White's view but shows a visible
        // enemy pawn, and never leaks the opponent's move list.
        let g = Game::from_fen(v("fogofwar"), "4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1").unwrap();
        let white = g.view_for(Color::White);
        let placement = white.fen.split(' ').next().unwrap();
        assert!(placement.contains('p'), "visible Black pawn must show");
        assert!(
            !placement.contains('k'),
            "hidden Black king leaked: {}",
            white.fen
        );
        // White is to move, so White sees its own moves; Black (non-mover) sees
        // no move list through the game surface.
        assert!(!white.legal_ucis.is_empty());
        assert!(g.view_for(Color::Black).legal_ucis.is_empty());
    }

    #[test]
    fn legal_moves_from_partitions_by_origin() {
        // The geometry-agnostic square-indexed origin filter, on a concrete board.
        let g = Game::new(v("chess"));
        let all = g.legal_moves().len();
        let mut summed = 0;
        for sq in 0..64u8 {
            summed += g.legal_moves_from(sq).len();
        }
        assert_eq!(summed, all);
        // An off-board index yields nothing.
        assert!(g.legal_moves_from(200).is_empty());
    }
}
