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
use shakmaty::{CastlingMode, Chess as ShChess};

use mce::{AnyVariant, VariantId};

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
        use shakmaty::Position;
        match self {
            ShakPos::Variant(p) => p.legal_moves().len(),
            ShakPos::Chess960(p) => p.legal_moves().len(),
        }
    }
}
