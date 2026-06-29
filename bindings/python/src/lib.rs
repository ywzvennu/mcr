//! Python bindings for the `mce` chess engine, built with pyo3.
//!
//! This crate exposes a thin, Pythonic `mce` module: a single [`Position`]
//! class wrapping [`mce::AnyVariant`] (so every variant is reachable through one
//! type) plus a module-level [`perft`] function. The surface mirrors
//! python-chess where that is natural — `legal_moves()`, `push()`, `fen`,
//! `turn`, `is_check()`, … — while staying a direct, allocation-light forward
//! to the underlying Rust API.
//!
//! Error handling: every fallible Rust call (`from_fen`, `parse_uci`,
//! `parse_san`, …) maps its `Err` onto a Python `ValueError`. No Rust panic is
//! allowed to cross the FFI boundary; the binding validates or converts first.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyType;

use ::mce::geometry::{AnyWideVariant, WideEndReason, WideMove, WideOutcome, WideVariantId};
use ::mce::{
    AnyVariant, Board, Color, EndReason, Move, Outcome, Position as CorePosition, VariantId,
};

/// Reaches the standard-chess core [`mce::Position`] inside an [`AnyVariant`].
///
/// `AnyVariant` is an enum with one arm per variant and does not itself expose
/// the core, but every arm wraps a `VariantPosition<V>` whose `.core()` returns
/// the underlying `Position`. SAN and the ASCII board are defined on that core,
/// so the binding dispatches here to reach it. This is plain safe Rust — a
/// match over the public enum arms.
fn core(av: &AnyVariant) -> &CorePosition {
    match av {
        AnyVariant::Chess(p) => p.core(),
        AnyVariant::Chess960(p) => p.core(),
        AnyVariant::KingOfTheHill(p) => p.core(),
        AnyVariant::ThreeCheck(p) => p.core(),
        AnyVariant::RacingKings(p) => p.core(),
        AnyVariant::Horde(p) => p.core(),
        AnyVariant::Atomic(p) => p.core(),
        AnyVariant::Antichess(p) => p.core(),
        AnyVariant::Crazyhouse(p) => p.core(),
    }
}

/// Parses a variant name into a [`VariantId`], raising `ValueError` on an
/// unknown name (the same alias set the Rust `FromStr` accepts).
fn parse_variant(name: &str) -> PyResult<VariantId> {
    name.parse::<VariantId>()
        .map_err(|e| PyValueError::new_err(e.to_string()))
}

/// A textual label for an outcome, mirroring python-chess result strings.
fn outcome_str(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Decisive {
            winner: Color::White,
        } => "1-0",
        Outcome::Decisive {
            winner: Color::Black,
        } => "0-1",
        Outcome::Draw => "1/2-1/2",
    }
}

/// A chess position of any supported variant.
///
/// Construct from a FEN (defaulting to the variant's start position) and a
/// variant name, then generate and play moves through the same surface
/// regardless of variant.
#[pyclass(module = "mce", from_py_object)]
#[derive(Clone)]
struct Position {
    inner: AnyVariant,
}

impl Position {
    /// Resolves a UCI string to a legal [`Move`] in this position, raising
    /// `ValueError` for a malformed or illegal move.
    fn resolve_uci(&self, uci: &str) -> PyResult<Move> {
        self.inner
            .parse_uci(uci)
            .map_err(|e| PyValueError::new_err(format!("invalid UCI move {uci:?}: {e}")))
    }
}

#[pymethods]
impl Position {
    /// `Position(fen=None, variant="chess")`.
    ///
    /// With `fen` omitted (or `None`) the variant's start position is used.
    /// `variant` accepts any name the engine recognizes (`"chess"`,
    /// `"atomic"`, `"crazyhouse"`, `"koth"`, …). Raises `ValueError` on an
    /// unknown variant or an invalid FEN.
    #[new]
    #[pyo3(signature = (fen=None, variant="chess"))]
    fn new(fen: Option<&str>, variant: &str) -> PyResult<Self> {
        let id = parse_variant(variant)?;
        let inner = match fen {
            None => AnyVariant::startpos(id),
            Some(fen) => AnyVariant::from_fen(id, fen)
                .map_err(|e| PyValueError::new_err(format!("invalid FEN: {e}")))?,
        };
        Ok(Position { inner })
    }

    /// The start position of `variant` (default standard chess).
    #[classmethod]
    #[pyo3(signature = (variant="chess"))]
    fn startpos(_cls: &Bound<'_, PyType>, variant: &str) -> PyResult<Self> {
        let id = parse_variant(variant)?;
        Ok(Position {
            inner: AnyVariant::startpos(id),
        })
    }

    /// The legal moves as a list of UCI strings.
    fn legal_moves(&self) -> Vec<String> {
        self.inner
            .legal_moves()
            .iter()
            .map(|mv| self.inner.to_uci(mv))
            .collect()
    }

    /// The legal moves as a list of SAN strings.
    fn legal_moves_san(&self) -> Vec<String> {
        let core = core(&self.inner);
        self.inner
            .legal_moves()
            .iter()
            .map(|mv| core.san(mv))
            .collect()
    }

    /// Plays the move given in UCI in place, mutating this position.
    ///
    /// Raises `ValueError` if the move is malformed or illegal here.
    fn push(&mut self, uci: &str) -> PyResult<()> {
        let mv = self.resolve_uci(uci)?;
        self.inner = self.inner.play(&mv);
        Ok(())
    }

    /// Returns a new position with the UCI move played, leaving `self`
    /// unchanged. Raises `ValueError` on a malformed or illegal move.
    fn play(&self, uci: &str) -> PyResult<Self> {
        let mv = self.resolve_uci(uci)?;
        Ok(Position {
            inner: self.inner.play(&mv),
        })
    }

    /// The position in six-field FEN.
    #[getter]
    fn fen(&self) -> String {
        self.inner.to_fen()
    }

    /// The side to move: `"white"` or `"black"`.
    #[getter]
    fn turn(&self) -> &'static str {
        match self.inner.turn() {
            Color::White => "white",
            Color::Black => "black",
        }
    }

    /// The variant name (canonical lowercase), e.g. `"chess"` or `"atomic"`.
    #[getter]
    fn variant(&self) -> &'static str {
        self.inner.variant_id().as_str()
    }

    /// Whether the side to move is in check.
    fn is_check(&self) -> bool {
        self.inner.is_check()
    }

    /// Whether the position is checkmate (no legal moves while in check, or a
    /// decisive variant end such as an exploded king).
    fn is_checkmate(&self) -> bool {
        matches!(self.inner.end_reason(), Some(r) if is_decisive_end(r))
    }

    /// Whether the position is a stalemate (no legal moves, not in check).
    fn is_stalemate(&self) -> bool {
        self.inner.end_reason() == Some(EndReason::Stalemate)
    }

    /// The game result string (`"1-0"`, `"0-1"`, `"1/2-1/2"`) if the game has
    /// ended, else `None`.
    fn outcome(&self) -> Option<&'static str> {
        self.inner.outcome().map(outcome_str)
    }

    /// The reason the game ended (e.g. `"checkmate"`, `"stalemate"`,
    /// `"insufficient_material"`), or `None` if it is ongoing.
    fn end_reason(&self) -> Option<&'static str> {
        self.inner.end_reason().map(end_reason_str)
    }

    /// The SAN for a move given in UCI. Raises `ValueError` if the UCI move is
    /// malformed or illegal in this position.
    fn san(&self, uci: &str) -> PyResult<String> {
        let mv = self.resolve_uci(uci)?;
        Ok(core(&self.inner).san(&mv))
    }

    /// Parses a SAN move and returns it as UCI. Raises `ValueError` if the SAN
    /// is malformed, illegal, or ambiguous here.
    fn parse_san(&self, san: &str) -> PyResult<String> {
        let mv = core(&self.inner)
            .parse_san(san)
            .map_err(|e| PyValueError::new_err(format!("invalid SAN move {san:?}: {e}")))?;
        Ok(self.inner.to_uci(&mv))
    }

    /// The 64-bit Zobrist hash of the position.
    fn zobrist(&self) -> u64 {
        self.inner.zobrist().get()
    }

    /// An ASCII rendering of the board (rank 8 on top, `.` for empty squares).
    fn __str__(&self) -> PyResult<String> {
        let placement = self
            .inner
            .to_fen()
            .split(' ')
            .next()
            .map(str::to_owned)
            .unwrap_or_default();
        let board = Board::from_fen_placement(&placement)
            .map_err(|e| PyValueError::new_err(format!("could not render board: {e}")))?;
        Ok(board.to_string())
    }

    fn __repr__(&self) -> String {
        format!(
            "Position(fen={:?}, variant={:?})",
            self.inner.to_fen(),
            self.inner.variant_id().as_str()
        )
    }

    fn __eq__(&self, other: &Position) -> bool {
        self.inner == other.inner
    }
}

/// Whether an end reason corresponds to a decisive (won/lost) game rather than
/// a draw — used to answer `is_checkmate()` across all variants.
fn is_decisive_end(reason: EndReason) -> bool {
    matches!(
        reason,
        EndReason::Checkmate
            | EndReason::VariantWin
            | EndReason::KingInTheHill
            | EndReason::ThreeChecks
            | EndReason::RaceFinished
            | EndReason::KingExploded
            | EndReason::HordeDefeated
    )
}

/// A snake_case label for an [`EndReason`].
fn end_reason_str(reason: EndReason) -> &'static str {
    match reason {
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
        EndReason::SeventyFiveMoveRule => "seventyfive_move_rule",
        EndReason::FivefoldRepetition => "fivefold_repetition",
        EndReason::FiftyMoveRule => "fifty_move_rule",
        EndReason::ThreefoldRepetition => "threefold_repetition",
    }
}

/// Counts the leaf nodes of the move tree to `depth` (a perft node count).
///
/// `depth` 0 is 1; the standard chess start position gives 20 at depth 1 and
/// 400 at depth 2. Works for every variant.
#[pyfunction]
fn perft(position: &Position, depth: u32) -> u64 {
    position.inner.perft(depth)
}

/// A snake_case label for a [`WideEndReason`] (the geometry layer's analogue of
/// [`EndReason`]).
fn wide_end_reason_str(reason: WideEndReason) -> &'static str {
    match reason {
        WideEndReason::Checkmate => "checkmate",
        WideEndReason::Stalemate => "stalemate",
        WideEndReason::InsufficientMaterial => "insufficient_material",
        WideEndReason::VariantWin => "variant_win",
        WideEndReason::VariantDraw => "variant_draw",
        WideEndReason::Repetition => "repetition",
        WideEndReason::Sennichite => "sennichite",
        WideEndReason::PerpetualCheckLoss => "perpetual_check_loss",
        WideEndReason::Bikjang => "bikjang",
        WideEndReason::CountingDraw => "counting_draw",
        WideEndReason::MoveRule => "move_rule",
    }
}

/// A textual result label for a [`WideOutcome`], mirroring python-chess strings.
fn wide_outcome_str(outcome: WideOutcome) -> &'static str {
    match outcome {
        WideOutcome::Decisive {
            winner: Color::White,
        } => "1-0",
        WideOutcome::Decisive {
            winner: Color::Black,
        } => "0-1",
        WideOutcome::Draw => "1/2-1/2",
    }
}

/// A fairy-chess position on the geometry layer: xiangqi, shogi, janggi, orda,
/// and the rest of the wide variants, reached through mce's runtime
/// [`AnyWideVariant`] dispatch.
///
/// This mirrors [`Position`] (construct by name, FEN I/O, legal moves, perft,
/// play) for the variants whose board geometry differs from 8x8, so it is a
/// separate class. SAN, the Zobrist hash, and the 8x8 ASCII board do not apply
/// here and are not exposed. List the variant names with [`variants`].
#[pyclass(module = "mce", from_py_object)]
#[derive(Clone)]
struct FairyPosition {
    inner: AnyWideVariant,
}

impl FairyPosition {
    /// Resolves a UCI string to a legal [`WideMove`], raising `ValueError` for a
    /// malformed or illegal move.
    fn resolve_uci(&self, uci: &str) -> PyResult<WideMove> {
        self.inner
            .parse_uci(uci)
            .ok_or_else(|| PyValueError::new_err(format!("invalid UCI move {uci:?}")))
    }
}

#[pymethods]
impl FairyPosition {
    /// `FairyPosition(variant, fen=None)`.
    ///
    /// `variant` accepts any fairy name the engine recognizes (`"xiangqi"`,
    /// `"shogi"`, `"janggi"`, `"orda"`, … plus aliases such as `"cchess"`). With
    /// `fen` omitted (or `None`) the variant's start position is used. Raises
    /// `ValueError` on an unknown variant or an invalid FEN.
    #[new]
    #[pyo3(signature = (variant, fen=None))]
    fn new(variant: &str, fen: Option<&str>) -> PyResult<Self> {
        let id = parse_wide_variant(variant)?;
        let inner = match fen {
            None => AnyWideVariant::startpos(id),
            Some(fen) => AnyWideVariant::from_fen(id, fen)
                .map_err(|e| PyValueError::new_err(format!("invalid FEN: {e}")))?,
        };
        Ok(FairyPosition { inner })
    }

    /// The start position of the named fairy `variant`.
    #[classmethod]
    fn startpos(_cls: &Bound<'_, PyType>, variant: &str) -> PyResult<Self> {
        let id = parse_wide_variant(variant)?;
        Ok(FairyPosition {
            inner: AnyWideVariant::startpos(id),
        })
    }

    /// The legal moves as a list of UCI strings.
    fn legal_moves(&self) -> Vec<String> {
        self.inner
            .legal_moves()
            .iter()
            .map(|mv| self.inner.to_uci(mv))
            .collect()
    }

    /// Plays the move given in UCI in place, mutating this position. Raises
    /// `ValueError` if the move is malformed or illegal here.
    fn push(&mut self, uci: &str) -> PyResult<()> {
        let mv = self.resolve_uci(uci)?;
        self.inner = self.inner.play(&mv);
        Ok(())
    }

    /// Returns a new position with the UCI move played, leaving `self`
    /// unchanged. Raises `ValueError` on a malformed or illegal move.
    fn play(&self, uci: &str) -> PyResult<Self> {
        let mv = self.resolve_uci(uci)?;
        Ok(FairyPosition {
            inner: self.inner.play(&mv),
        })
    }

    /// Counts the leaf nodes of the move tree to `depth` (a perft node count).
    fn perft(&self, depth: u32) -> u64 {
        self.inner.perft(depth)
    }

    /// The position in FEN.
    #[getter]
    fn fen(&self) -> String {
        self.inner.to_fen()
    }

    /// The side to move: `"white"` or `"black"`.
    #[getter]
    fn turn(&self) -> &'static str {
        match self.inner.turn() {
            Color::White => "white",
            Color::Black => "black",
        }
    }

    /// The variant name (canonical lowercase), e.g. `"xiangqi"` or `"shogi"`.
    #[getter]
    fn variant(&self) -> &'static str {
        self.inner.variant_id().as_str()
    }

    /// Whether the side to move is in check (always `False` where the king is
    /// not royal).
    fn is_check(&self) -> bool {
        self.inner.is_check()
    }

    /// Whether the position is a decisive loss for the side to move (checkmate
    /// or a variant-specific win).
    fn is_checkmate(&self) -> bool {
        matches!(
            self.inner.end_reason(),
            Some(WideEndReason::Checkmate | WideEndReason::VariantWin)
        )
    }

    /// Whether the position is a stalemate (no legal moves, not in check).
    fn is_stalemate(&self) -> bool {
        self.inner.end_reason() == Some(WideEndReason::Stalemate)
    }

    /// The game result string (`"1-0"`, `"0-1"`, `"1/2-1/2"`) if the game has
    /// ended, else `None`.
    fn outcome(&self) -> Option<&'static str> {
        self.inner.outcome().map(wide_outcome_str)
    }

    /// The reason the game ended (e.g. `"checkmate"`, `"stalemate"`), or `None`
    /// if it is ongoing.
    fn end_reason(&self) -> Option<&'static str> {
        self.inner.end_reason().map(wide_end_reason_str)
    }

    fn __repr__(&self) -> String {
        format!(
            "FairyPosition(variant={:?}, fen={:?})",
            self.inner.variant_id().as_str(),
            self.inner.to_fen()
        )
    }

    fn __eq__(&self, other: &FairyPosition) -> bool {
        self.inner.variant_id() == other.inner.variant_id()
            && self.inner.to_fen() == other.inner.to_fen()
    }
}

/// Parses a fairy variant name into a [`WideVariantId`], raising `ValueError` on
/// an unknown name (the same alias set the Rust `FromStr` accepts).
fn parse_wide_variant(name: &str) -> PyResult<WideVariantId> {
    name.parse::<WideVariantId>()
        .map_err(|e| PyValueError::new_err(e.to_string()))
}

/// The supported fairy-variant names (canonical lowercase), in declaration
/// order — the catalogue for [`FairyPosition`].
#[pyfunction]
fn variants() -> Vec<&'static str> {
    WideVariantId::ALL.iter().map(|id| id.as_str()).collect()
}

/// The `mce` Python module.
#[pymodule]
fn mce(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Position>()?;
    m.add_class::<FairyPosition>()?;
    m.add_function(wrap_pyfunction!(perft, m)?)?;
    m.add_function(wrap_pyfunction!(variants, m)?)?;
    m.add(
        "__doc__",
        "Fast Rust-backed chess move generation and rules.",
    )?;
    Ok(())
}
