//! Runtime (string-keyed) dispatch over both engines for arbitrary FENs.
//!
//! The original `lib.rs` basket dispatches on `&'static str` variant keys with a
//! macro per static [`Case`]. The expanded suite generates *thousands* of FENs at
//! runtime (the EPD suite and the seeded per-variant baskets), so it needs a
//! dispatch surface that takes an owned FEN string and a variant key and runs
//! either engine. This module provides exactly that, parsing once into each
//! engine's native runtime type:
//!
//! * mce via [`mce::AnyVariant`] (chess960 is its own [`mce::VariantId`]);
//! * shakmaty via [`shakmaty::variant::VariantPosition`] for the seven non-960
//!   variants, and `Chess` with [`CastlingMode::Chess960`] for chess960
//!   (shakmaty folds 960 into `Chess` + a castling mode rather than a variant).
//!
//! Both engines expose `perft`; we cross-check the counts elsewhere.

use shakmaty::fen::Fen;
use shakmaty::variant::{Variant as ShVariant, VariantPosition as ShVariantPos};
use shakmaty::{CastlingMode, Chess as ShChess, EnPassantMode, Position as _};

use mce::{AnyVariant, VariantId};

/// A side-agnostic, engine-agnostic game result for the differential check.
///
/// Both engines model an over game as either a decisive result (with a winner)
/// or a draw; an ongoing game is `None`. We compare the two engines' verdicts
/// on this small, shared shape so the difftest does not depend on either
/// engine's richer `EndReason`/`variant_outcome` taxonomy. `White`/`Black` are
/// encoded as a `bool` (`true` = white won) to avoid leaking either crate's
/// `Color` type across the comparison boundary.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TermStatus {
    /// The game is ongoing (no single-position termination).
    Ongoing,
    /// The game is over and decisive; `white_won` is the winner.
    Decisive { white_won: bool },
    /// The game is over and drawn.
    Draw,
}

/// The nine variant keys used throughout the suite, matching [`crate::VARIANTS`].
///
/// Returns the mce [`VariantId`] and the shakmaty [`ShVariant`] plus the
/// castling mode. shakmaty has no chess960 *variant*; we represent it as
/// `(Chess, Chess960)` and special-case parsing below.
fn keys(variant: &str) -> Option<VariantId> {
    Some(match variant {
        "standard" => VariantId::Standard,
        "chess960" => VariantId::Chess960,
        "king-of-the-hill" => VariantId::KingOfTheHill,
        "three-check" => VariantId::ThreeCheck,
        "racing-kings" => VariantId::RacingKings,
        "atomic" => VariantId::Atomic,
        "antichess" => VariantId::Antichess,
        "horde" => VariantId::Horde,
        "crazyhouse" => VariantId::Crazyhouse,
        _ => return None,
    })
}

/// The shakmaty variant + castling mode for a key. chess960 maps to
/// `(Chess, Chess960)`; everything else uses [`CastlingMode::Standard`].
fn shak_keys(variant: &str) -> Option<(Option<ShVariant>, CastlingMode)> {
    Some(match variant {
        "standard" => (Some(ShVariant::Chess), CastlingMode::Standard),
        "chess960" => (None, CastlingMode::Chess960), // None = plain Chess + 960 mode
        "king-of-the-hill" => (Some(ShVariant::KingOfTheHill), CastlingMode::Standard),
        "three-check" => (Some(ShVariant::ThreeCheck), CastlingMode::Standard),
        "racing-kings" => (Some(ShVariant::RacingKings), CastlingMode::Standard),
        "atomic" => (Some(ShVariant::Atomic), CastlingMode::Standard),
        "antichess" => (Some(ShVariant::Antichess), CastlingMode::Standard),
        "horde" => (Some(ShVariant::Horde), CastlingMode::Standard),
        "crazyhouse" => (Some(ShVariant::Crazyhouse), CastlingMode::Standard),
        _ => return None,
    })
}

/// An mce position parsed for a known variant key, ready for perft / move-gen.
pub struct McePos {
    inner: AnyVariant,
}

impl McePos {
    /// Parse `fen` for `variant`. Returns `None` if the key is unknown or the
    /// FEN does not parse in mce.
    pub fn parse(variant: &str, fen: &str) -> Option<Self> {
        let id = keys(variant)?;
        let inner = AnyVariant::from_fen(id, fen).ok()?;
        Some(Self { inner })
    }

    /// Perft node count to `depth`.
    pub fn perft(&self, depth: u32) -> u64 {
        self.inner.perft(depth)
    }

    /// Number of legal moves in this position.
    pub fn legal_move_count(&self) -> usize {
        self.inner.legal_moves().len()
    }

    /// Borrow the underlying [`AnyVariant`] (for micro-benchmarks).
    pub fn any(&self) -> &AnyVariant {
        &self.inner
    }

    /// The legal-move set as sorted UCI strings.
    ///
    /// mce renders castling as king→king-destination-square (the lichess/UCI
    /// "standard" form, e.g. `e1g1`), so to compare against shakmaty the
    /// shakmaty side must render with [`CastlingMode::Standard`] too (see
    /// [`ShakPos::legal_ucis`]).
    pub fn legal_ucis(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .inner
            .legal_moves()
            .iter()
            .map(|m| self.inner.to_uci(m))
            .collect();
        v.sort();
        v
    }

    /// Whether the side to move is in check.
    pub fn is_check(&self) -> bool {
        self.inner.is_check()
    }

    /// The single-position termination verdict (no repetition / move-clock
    /// claims, to match shakmaty's history-free `outcome()`).
    pub fn term_status(&self) -> TermStatus {
        match self.inner.outcome() {
            None => TermStatus::Ongoing,
            Some(mce::Outcome::Decisive { winner }) => TermStatus::Decisive {
                white_won: winner.is_white(),
            },
            Some(mce::Outcome::Draw) => TermStatus::Draw,
        }
    }

    /// This position serialized to FEN.
    pub fn to_fen(&self) -> String {
        self.inner.to_fen()
    }

    /// FEN round-trip: parse `self.to_fen()` back in the same variant and return
    /// the re-serialized FEN. A correct round-trip yields the original FEN.
    /// Returns `None` if the emitted FEN fails to re-parse.
    pub fn fen_roundtrip(&self, variant: &str) -> Option<String> {
        let id = keys(variant)?;
        Some(
            AnyVariant::from_fen(id, &self.inner.to_fen())
                .ok()?
                .to_fen(),
        )
    }

    /// Whether any node within `depth` plies of this position is variant-
    /// terminal (an mce-reported [`mce::Outcome`]). Used to confirm that a
    /// mismatch against shakmaty is the documented terminal-divergence case
    /// (shakmaty prunes at the terminal; mce keeps expanding) rather than a bug.
    pub fn any_reaches_terminal(&self, depth: u32) -> bool {
        reaches(&self.inner, depth)
    }
}

/// Recursive helper for [`McePos::any_reaches_terminal`].
fn reaches(pos: &AnyVariant, depth: u32) -> bool {
    if pos.outcome().is_some() {
        return true;
    }
    if depth == 0 {
        return false;
    }
    for mv in pos.legal_moves() {
        if reaches(&pos.play(&mv), depth - 1) {
            return true;
        }
    }
    false
}

/// Shakmaty's runtime position for a variant key. chess960 is a `Chess` with the
/// 960 castling mode; everything else is a [`ShVariantPos`].
pub enum ShakPos {
    /// A `shakmaty::variant::VariantPosition` (the seven non-960 variants +
    /// standard).
    Variant(ShVariantPos),
    /// Chess960: plain `Chess` parsed with [`CastlingMode::Chess960`].
    Chess960(ShChess),
}

impl ShakPos {
    /// Parse `fen` for `variant`. Returns `None` if the key is unknown or the
    /// FEN does not parse / is rejected by shakmaty (e.g. an over-material
    /// crazyhouse pocket, or a variant-terminal position shakmaty refuses).
    pub fn parse(variant: &str, fen: &str) -> Option<Self> {
        let (shvar, mode) = shak_keys(variant)?;
        let fen = Fen::from_ascii(fen.as_bytes()).ok()?;
        match shvar {
            None => {
                let pos: ShChess = fen.into_position(mode).ok()?;
                Some(ShakPos::Chess960(pos))
            }
            Some(v) => {
                let pos = ShVariantPos::from_setup(v, fen.into_setup(), mode).ok()?;
                Some(ShakPos::Variant(pos))
            }
        }
    }

    /// Perft node count to `depth`.
    pub fn perft(&self, depth: u32) -> u64 {
        match self {
            ShakPos::Variant(p) => shakmaty::perft(p, depth),
            ShakPos::Chess960(p) => shakmaty::perft(p, depth),
        }
    }

    /// Number of legal moves in this position.
    pub fn legal_move_count(&self) -> usize {
        match self {
            ShakPos::Variant(p) => p.legal_moves().len(),
            ShakPos::Chess960(p) => p.legal_moves().len(),
        }
    }

    /// The legal-move set as sorted UCI strings.
    ///
    /// Rendered with [`CastlingMode::Standard`] for **all** variants — including
    /// chess960 — so castling is emitted as king→king-destination-square
    /// (`e1g1`), matching mce. (`CastlingMode::Chess960` would emit the
    /// king→rook form `e1h1` and spuriously diverge.)
    pub fn legal_ucis(&self) -> Vec<String> {
        let render = |m: &shakmaty::Move| m.to_uci(CastlingMode::Standard).to_string();
        let mut v: Vec<String> = match self {
            ShakPos::Variant(p) => p.legal_moves().iter().map(render).collect(),
            ShakPos::Chess960(p) => p.legal_moves().iter().map(render).collect(),
        };
        v.sort();
        v
    }

    /// Whether the side to move is in check.
    pub fn is_check(&self) -> bool {
        match self {
            ShakPos::Variant(p) => p.is_check(),
            ShakPos::Chess960(p) => p.is_check(),
        }
    }

    /// The single-position termination verdict (shakmaty's history-free
    /// `outcome()`: variant terminal, checkmate, stalemate, or insufficient
    /// material).
    pub fn term_status(&self) -> TermStatus {
        let oc = match self {
            ShakPos::Variant(p) => p.outcome(),
            ShakPos::Chess960(p) => p.outcome(),
        };
        match oc {
            None => TermStatus::Ongoing,
            Some(shakmaty::Outcome::Decisive { winner }) => TermStatus::Decisive {
                white_won: winner.is_white(),
            },
            Some(shakmaty::Outcome::Draw) => TermStatus::Draw,
        }
    }

    /// This position serialized to FEN (Shredder/X-FEN castling for chess960).
    pub fn to_fen(&self) -> String {
        match self {
            ShakPos::Variant(p) => Fen::from_position(p.clone(), EnPassantMode::Legal).to_string(),
            ShakPos::Chess960(p) => Fen::from_position(p.clone(), EnPassantMode::Legal).to_string(),
        }
    }
}
