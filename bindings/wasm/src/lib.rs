//! WebAssembly bindings for the [`mce`] chess engine.
//!
//! This crate is a thin `wasm-bindgen` shim over the mce public API. It exposes
//! a single JS-facing [`Game`] class that drives any of the supported variants
//! (standard chess, Chess960, and the major variants) through mce's runtime
//! [`AnyVariant`] dispatch, plus the standard-chess SAN helpers.
//!
//! No mce call can panic across the JS boundary: every fallible entry point
//! returns a `Result<_, JsError>`, which `wasm-bindgen` turns into a thrown JS
//! exception, and the infallible queries borrow already-validated state.

// `wasm-bindgen` generates glue that this crate cannot annotate, so the parent
// crate's stricter pub/idiom lints would only produce noise here.
#![allow(clippy::needless_pass_by_value)]

use mce::geometry::{AnyWideVariant, WideEndReason, WideOutcome, WideVariantId};
use mce::{AnyVariant, Color, EndReason, Outcome, Position, VariantId};
use wasm_bindgen::prelude::*;

/// Map a value that implements [`core::fmt::Display`] (every mce error type does)
/// into a JS exception carrying its message.
fn js_err(e: impl core::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

/// Stable lowercase string for a colour, matching mce's own `Display`.
fn color_str(c: Color) -> String {
    match c {
        Color::White => "white",
        Color::Black => "black",
    }
    .to_owned()
}

/// A self-contained, JSON-friendly description of a finished game. `null` on the
/// JS side while the game is still in progress.
#[wasm_bindgen(getter_with_clone)]
#[derive(Debug, Clone)]
pub struct GameOutcome {
    /// `"decisive"` or `"draw"`.
    pub kind: String,
    /// Winning colour (`"white"`/`"black"`) for a decisive result, else `null`.
    pub winner: Option<String>,
    /// Machine reason label (e.g. `"checkmate"`, `"stalemate"`), when known.
    pub reason: Option<String>,
}

/// The lowercase machine label for an [`EndReason`] (mce does not expose a
/// `Display` for it, so spell it out here — kept in lockstep with the enum).
fn end_reason_str(r: EndReason) -> &'static str {
    match r {
        EndReason::Checkmate => "checkmate",
        EndReason::VariantWin => "variant_win",
        EndReason::KingInTheHill => "king_in_the_hill",
        EndReason::ThreeChecks => "three_checks",
        EndReason::RaceFinished => "race_finished",
        EndReason::RaceDraw => "race_draw",
        EndReason::KingExploded => "king_exploded",
        EndReason::HordeDefeated => "horde_defeated",
        EndReason::Stalemate => "stalemate",
        EndReason::InsufficientMaterial => "insufficient_material",
        EndReason::SeventyFiveMoveRule => "seventy_five_move_rule",
        EndReason::FivefoldRepetition => "fivefold_repetition",
        EndReason::FiftyMoveRule => "fifty_move_rule",
        EndReason::ThreefoldRepetition => "threefold_repetition",
    }
}

/// A chess game: a position plus the variant it is played under, with legal-move
/// generation, move application, and the usual queries.
///
/// Construct one with [`Game::startpos`] or [`Game::from_fen`]. The mutating
/// helpers ([`Game::push`]/[`Game::play`]) advance this object in place;
/// `wasm-bindgen` hands JS a handle, so the same object is reused move to move.
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct Game {
    inner: AnyVariant,
}

#[wasm_bindgen]
impl Game {
    /// Start position for a variant. With no argument (or `"standard"`/`"chess"`)
    /// this is the standard chess start position. Accepts any name mce's
    /// `VariantId` parser understands (`"atomic"`, `"koth"`, `"3check"`, `"zh"`,
    /// …); unknown names throw.
    #[wasm_bindgen(js_name = startpos)]
    pub fn startpos(variant: Option<String>) -> Result<Game, JsError> {
        let id = match variant {
            None => VariantId::Standard,
            Some(name) => name.parse::<VariantId>().map_err(js_err)?,
        };
        Ok(Game {
            inner: AnyVariant::startpos(id),
        })
    }

    /// Parse a six-field FEN under the given variant (defaults to standard).
    /// Throws on a malformed or illegal FEN, or an unknown variant name.
    #[wasm_bindgen(js_name = fromFen)]
    pub fn from_fen(fen: &str, variant: Option<String>) -> Result<Game, JsError> {
        let id = match variant {
            None => VariantId::Standard,
            Some(name) => name.parse::<VariantId>().map_err(js_err)?,
        };
        let inner = AnyVariant::from_fen(id, fen).map_err(js_err)?;
        Ok(Game { inner })
    }

    /// The variant identifier, lowercased (e.g. `"standard"`, `"atomic"`).
    #[wasm_bindgen(js_name = variant)]
    pub fn variant(&self) -> String {
        self.inner.variant_id().to_string()
    }

    /// The six-field FEN for the current position.
    #[wasm_bindgen(js_name = fen)]
    pub fn fen(&self) -> String {
        self.inner.to_fen()
    }

    /// Side to move: `"white"` or `"black"`.
    #[wasm_bindgen(js_name = turn)]
    pub fn turn(&self) -> String {
        color_str(self.inner.turn())
    }

    /// Legal moves in this position, as UCI strings (e.g. `"e2e4"`, `"e7e8q"`).
    #[wasm_bindgen(js_name = legalMoves)]
    pub fn legal_moves(&self) -> Vec<String> {
        self.inner
            .legal_moves()
            .iter()
            .map(|mv| self.inner.to_uci(mv))
            .collect()
    }

    /// Legal moves in this position rendered as SAN (e.g. `"Nf3"`, `"O-O"`,
    /// `"exd5"`). SAN is only defined for standard chess and Chess960; for other
    /// variants this throws.
    #[wasm_bindgen(js_name = legalMovesSan)]
    pub fn legal_moves_san(&self) -> Result<Vec<String>, JsError> {
        let pos = self.san_position()?;
        Ok(pos.legal_moves().iter().map(|mv| pos.san(mv)).collect())
    }

    /// Apply a move given in UCI. Returns the resulting position's FEN. Throws if
    /// the move is malformed or not legal in this position. Alias: [`Game::play`].
    #[wasm_bindgen(js_name = push)]
    pub fn push(&mut self, uci: &str) -> Result<String, JsError> {
        let mv = self.inner.parse_uci(uci).map_err(js_err)?;
        self.inner = self.inner.play(&mv);
        Ok(self.inner.to_fen())
    }

    /// Alias for [`Game::push`]: apply a UCI move in place, returning the new FEN.
    #[wasm_bindgen(js_name = play)]
    pub fn play(&mut self, uci: &str) -> Result<String, JsError> {
        self.push(uci)
    }

    /// True if the side to move is in check.
    #[wasm_bindgen(js_name = isCheck)]
    pub fn is_check(&self) -> bool {
        self.inner.is_check()
    }

    /// True if the position is checkmate (decisive, with the side to move losing
    /// by being mated). This is the standard-chess mate; variant-specific wins
    /// (king-in-the-hill, three-checks, …) surface through [`Game::outcome`].
    #[wasm_bindgen(js_name = isCheckmate)]
    pub fn is_checkmate(&self) -> bool {
        matches!(self.inner.end_reason(), Some(EndReason::Checkmate))
    }

    /// The game outcome, or `null` while play continues. See [`GameOutcome`].
    #[wasm_bindgen(js_name = outcome)]
    pub fn outcome(&self) -> Option<GameOutcome> {
        let reason = self
            .inner
            .end_reason()
            .map(end_reason_str)
            .map(str::to_owned);
        self.inner.outcome().map(|o| match o {
            Outcome::Decisive { winner } => GameOutcome {
                kind: "decisive".to_owned(),
                winner: Some(color_str(winner)),
                reason: reason.clone(),
            },
            Outcome::Draw => GameOutcome {
                kind: "draw".to_owned(),
                winner: None,
                reason: reason.clone(),
            },
        })
    }

    /// Render a UCI move as SAN in the current position. Standard chess /
    /// Chess960 only; throws otherwise, or if the move is illegal.
    #[wasm_bindgen(js_name = san)]
    pub fn san(&self, uci: &str) -> Result<String, JsError> {
        let pos = self.san_position()?;
        let mv = pos.parse_uci(uci).map_err(js_err)?;
        Ok(pos.san(&mv))
    }

    /// Parse a SAN move in the current position into its UCI string. Standard
    /// chess / Chess960 only; throws otherwise, or on a bad/ambiguous SAN.
    #[wasm_bindgen(js_name = parseSan)]
    pub fn parse_san(&self, san: &str) -> Result<String, JsError> {
        let pos = self.san_position()?;
        let mv = pos.parse_san(san).map_err(js_err)?;
        Ok(mv.to_uci())
    }

    /// The position's Zobrist hash as a 16-digit lowercase hex string. (Returned
    /// as a string because a `u64` exceeds JS's exact-integer range.)
    #[wasm_bindgen(js_name = zobrist)]
    pub fn zobrist(&self) -> String {
        format!("{:016x}", self.inner.zobrist().get())
    }

    /// Perft node count to `depth` from the current position. Returned as a
    /// string to preserve precision beyond JS's safe-integer range.
    #[wasm_bindgen(js_name = perft)]
    pub fn perft(&self, depth: u32) -> String {
        self.inner.perft(depth).to_string()
    }
}

impl Game {
    /// A standard-`Position` view of the current state, for the SAN helpers.
    ///
    /// SAN is only well-defined on the standard `Position` surface (and the
    /// Chess960 variant, which shares it). For every other variant we refuse
    /// rather than risk a wrong rendering.
    fn san_position(&self) -> Result<Position, JsError> {
        match self.inner.variant_id() {
            VariantId::Standard | VariantId::Chess960 => {
                Position::from_fen(&self.inner.to_fen()).map_err(js_err)
            }
            other => Err(JsError::new(&format!(
                "SAN is only available for standard chess and Chess960, not {other}"
            ))),
        }
    }
}

/// The lowercase machine label for a [`WideEndReason`] (the geometry layer's
/// analogue of [`EndReason`]; kept in lockstep with the enum).
fn wide_end_reason_str(r: WideEndReason) -> &'static str {
    match r {
        WideEndReason::Checkmate => "checkmate",
        WideEndReason::Stalemate => "stalemate",
        WideEndReason::InsufficientMaterial => "insufficient_material",
        WideEndReason::VariantWin => "variant_win",
        WideEndReason::VariantDraw => "variant_draw",
        WideEndReason::Repetition => "repetition",
        WideEndReason::Sennichite => "sennichite",
        WideEndReason::PerpetualCheckLoss => "perpetual_check_loss",
        WideEndReason::PerpetualChaseLoss => "perpetual_chase_loss",
        WideEndReason::Bikjang => "bikjang",
        WideEndReason::CountingDraw => "counting_draw",
        WideEndReason::MoveRule => "move_rule",
    }
}

/// A fairy-chess game on the geometry layer: xiangqi, shogi, janggi, orda, and
/// the rest of the wide variants, reached through mce's runtime
/// [`AnyWideVariant`] dispatch.
///
/// This mirrors [`Game`] (construct, FEN I/O, legal moves, perft, play) for the
/// variants whose board geometry differs from 8x8, so it is a separate class
/// rather than another arm of `Game`. SAN, the Zobrist hash, and the other 8x8
/// `Position` conveniences do not apply here and are not exposed. Enumerate the
/// variant names with [`FairyGame::variants`].
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct FairyGame {
    inner: AnyWideVariant,
}

#[wasm_bindgen]
impl FairyGame {
    /// Every supported fairy-variant name, as accepted by [`FairyGame::startpos`]
    /// / [`FairyGame::fromFen`].
    #[wasm_bindgen(js_name = variants)]
    pub fn variants() -> Vec<String> {
        WideVariantId::ALL
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect()
    }

    /// Start position of the named fairy variant (e.g. `"xiangqi"`, `"shogi"`,
    /// `"janggi"`; aliases such as `"cchess"` work too). Unknown names throw.
    #[wasm_bindgen(js_name = startpos)]
    pub fn startpos(variant: &str) -> Result<FairyGame, JsError> {
        let id = variant.parse::<WideVariantId>().map_err(js_err)?;
        Ok(FairyGame {
            inner: AnyWideVariant::startpos(id),
        })
    }

    /// Parse a FEN under the named fairy variant. Throws on a malformed or
    /// illegal FEN, or an unknown variant name.
    #[wasm_bindgen(js_name = fromFen)]
    pub fn from_fen(variant: &str, fen: &str) -> Result<FairyGame, JsError> {
        let id = variant.parse::<WideVariantId>().map_err(js_err)?;
        let inner = AnyWideVariant::from_fen(id, fen).map_err(js_err)?;
        Ok(FairyGame { inner })
    }

    /// The variant identifier, lowercased (e.g. `"xiangqi"`, `"shogi"`).
    #[wasm_bindgen(js_name = variant)]
    pub fn variant(&self) -> String {
        self.inner.variant_id().to_string()
    }

    /// The FEN for the current position.
    #[wasm_bindgen(js_name = fen)]
    pub fn fen(&self) -> String {
        self.inner.to_fen()
    }

    /// Side to move: `"white"` or `"black"`.
    #[wasm_bindgen(js_name = turn)]
    pub fn turn(&self) -> String {
        color_str(self.inner.turn())
    }

    /// Legal moves in this position, as UCI strings.
    #[wasm_bindgen(js_name = legalMoves)]
    pub fn legal_moves(&self) -> Vec<String> {
        self.inner
            .legal_moves()
            .iter()
            .map(|mv| self.inner.to_uci(mv))
            .collect()
    }

    /// Apply a move given in UCI. Returns the resulting position's FEN. Throws if
    /// the move is malformed or not legal here. Alias: [`FairyGame::play`].
    #[wasm_bindgen(js_name = push)]
    pub fn push(&mut self, uci: &str) -> Result<String, JsError> {
        let mv = self
            .inner
            .parse_uci(uci)
            .ok_or_else(|| JsError::new(&format!("illegal or malformed UCI move: {uci:?}")))?;
        self.inner = self.inner.play(&mv);
        Ok(self.inner.to_fen())
    }

    /// Alias for [`FairyGame::push`]: apply a UCI move in place, returning the
    /// new FEN.
    #[wasm_bindgen(js_name = play)]
    pub fn play(&mut self, uci: &str) -> Result<String, JsError> {
        self.push(uci)
    }

    /// True if the side to move is in check (always `false` where the king is not
    /// royal).
    #[wasm_bindgen(js_name = isCheck)]
    pub fn is_check(&self) -> bool {
        self.inner.is_check()
    }

    /// True if the position is a decisive loss for the side to move (checkmate or
    /// a variant-specific win). Drawn ends surface through [`FairyGame::outcome`].
    #[wasm_bindgen(js_name = isCheckmate)]
    pub fn is_checkmate(&self) -> bool {
        matches!(
            self.inner.end_reason(),
            Some(WideEndReason::Checkmate | WideEndReason::VariantWin)
        )
    }

    /// The game outcome, or `null` while play continues. See [`GameOutcome`].
    #[wasm_bindgen(js_name = outcome)]
    pub fn outcome(&self) -> Option<GameOutcome> {
        let reason = self
            .inner
            .end_reason()
            .map(wide_end_reason_str)
            .map(str::to_owned);
        self.inner.outcome().map(|o| match o {
            WideOutcome::Decisive { winner } => GameOutcome {
                kind: "decisive".to_owned(),
                winner: Some(color_str(winner)),
                reason: reason.clone(),
            },
            WideOutcome::Draw => GameOutcome {
                kind: "draw".to_owned(),
                winner: None,
                reason: reason.clone(),
            },
        })
    }

    /// Perft node count to `depth` from the current position. Returned as a
    /// string to preserve precision beyond JS's safe-integer range.
    #[wasm_bindgen(js_name = perft)]
    pub fn perft(&self, depth: u32) -> String {
        self.inner.perft(depth).to_string()
    }
}
