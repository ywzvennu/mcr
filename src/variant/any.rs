//! Runtime variant dispatch: [`AnyVariant`], a type-erased enum wrapper over the
//! generic [`VariantPosition<V>`](super::VariantPosition).
//!
//! The generic [`VariantPosition<V>`](super::VariantPosition) is the zero-cost core: `V` is a zero-sized
//! rule layer and every hook is monomorphized. That is exactly what a consumer
//! that knows its variant at compile time wants, but a server or UCI front-end
//! often picks the variant from a string at runtime and cannot name `V`.
//!
//! [`AnyVariant`] closes that gap: it is a plain enum with one arm per variant,
//! each wrapping that variant's [`VariantPosition`](super::VariantPosition) alias, plus a runtime
//! dispatch (a single `match`) for the common surface. It adds no state beyond
//! the wrapped position and introduces no `unsafe` or trait-object indirection;
//! the per-variant fast paths inside [`VariantPosition`](super::VariantPosition) are untouched.

use core::str::FromStr;

use super::{
    perft_variant, Antichess, Atomic, Chess, Chess960, Crazyhouse, Horde, KingOfTheHill,
    RacingKings, ThreeCheck, VariantId,
};
use crate::position::{FenError, ParseUciError};
use crate::{Color, EndReason, Move, Outcome, Zobrist};

/// The error returned by [`VariantId::from_str`] when a name is not a recognized
/// variant or alias.
///
/// The wrapped [`String`] is the offending input, lowercased exactly as it was
/// matched, so callers can echo it back in a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownVariant(pub String);

impl core::fmt::Display for UnknownVariant {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "unknown variant: {:?}", self.0)
    }
}

impl std::error::Error for UnknownVariant {}

impl FromStr for VariantId {
    type Err = UnknownVariant;

    /// Parses a [`VariantId`] from its canonical name or a common alias.
    ///
    /// Matching is case-insensitive and ignores surrounding whitespace. The
    /// accepted names per variant are:
    ///
    /// - [`Standard`](VariantId::Standard): `chess`, `standard`
    /// - [`Chess960`](VariantId::Chess960): `chess960`, `fischerandom`,
    ///   `fischerrandom`, `960`
    /// - [`KingOfTheHill`](VariantId::KingOfTheHill): `kingofthehill`, `koth`
    /// - [`ThreeCheck`](VariantId::ThreeCheck): `threecheck`, `3check`
    /// - [`RacingKings`](VariantId::RacingKings): `racingkings`, `racing`
    /// - [`Horde`](VariantId::Horde): `horde`
    /// - [`Atomic`](VariantId::Atomic): `atomic`
    /// - [`Antichess`](VariantId::Antichess): `antichess`, `giveaway`
    /// - [`Crazyhouse`](VariantId::Crazyhouse): `crazyhouse`, `zh`, `house`
    ///
    /// # Errors
    ///
    /// Returns [`UnknownVariant`] (carrying the normalized input) when the name
    /// matches no variant.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.trim().to_ascii_lowercase();
        let id = match normalized.as_str() {
            "chess" | "standard" => VariantId::Standard,
            "chess960" | "fischerandom" | "fischerrandom" | "960" => VariantId::Chess960,
            "kingofthehill" | "koth" => VariantId::KingOfTheHill,
            "threecheck" | "3check" => VariantId::ThreeCheck,
            "racingkings" | "racing" => VariantId::RacingKings,
            "horde" => VariantId::Horde,
            "atomic" => VariantId::Atomic,
            "antichess" | "giveaway" => VariantId::Antichess,
            "crazyhouse" | "zh" | "house" => VariantId::Crazyhouse,
            _ => return Err(UnknownVariant(normalized)),
        };
        Ok(id)
    }
}

/// A chess position whose variant is chosen at runtime: a thin enum wrapper with
/// one arm per variant, each holding that variant's [`VariantPosition`](super::VariantPosition) alias.
///
/// Construct one with [`AnyVariant::startpos`] or [`AnyVariant::from_fen`] from a
/// [`VariantId`] (which a string can yield via [`VariantId::from_str`]), then use
/// the same surface as [`VariantPosition`](super::VariantPosition) — [`legal_moves`](AnyVariant::legal_moves),
/// [`play`](AnyVariant::play), [`outcome`](AnyVariant::outcome), and the rest —
/// without naming the variant type. Every method forwards through a single
/// `match` to the inner generic position, so it is exactly as correct as the
/// underlying [`VariantPosition<V>`](super::VariantPosition) and pays only one branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnyVariant {
    /// Standard chess.
    Chess(Chess),
    /// Chess960 (Fischer random).
    Chess960(Chess960),
    /// King of the Hill.
    KingOfTheHill(KingOfTheHill),
    /// Three-check.
    ThreeCheck(ThreeCheck),
    /// Racing Kings.
    RacingKings(RacingKings),
    /// Horde.
    Horde(Horde),
    /// Atomic chess.
    Atomic(Atomic),
    /// Antichess (losing chess).
    Antichess(Antichess),
    /// Crazyhouse.
    Crazyhouse(Crazyhouse),
}

/// Dispatches an expression over every [`AnyVariant`] arm, binding the inner
/// `VariantPosition` to `$inner`. Keeps the forwarding methods mechanical and
/// exhaustively checked by the compiler.
macro_rules! dispatch {
    ($self:expr, $inner:ident => $body:expr) => {
        match $self {
            AnyVariant::Chess($inner) => $body,
            AnyVariant::Chess960($inner) => $body,
            AnyVariant::KingOfTheHill($inner) => $body,
            AnyVariant::ThreeCheck($inner) => $body,
            AnyVariant::RacingKings($inner) => $body,
            AnyVariant::Horde($inner) => $body,
            AnyVariant::Atomic($inner) => $body,
            AnyVariant::Antichess($inner) => $body,
            AnyVariant::Crazyhouse($inner) => $body,
        }
    };
}

impl AnyVariant {
    /// The starting position of the variant named by `id`.
    #[must_use]
    pub fn startpos(id: VariantId) -> Self {
        match id {
            VariantId::Standard => AnyVariant::Chess(Chess::startpos()),
            VariantId::Chess960 => AnyVariant::Chess960(Chess960::startpos()),
            VariantId::KingOfTheHill => AnyVariant::KingOfTheHill(KingOfTheHill::startpos()),
            VariantId::ThreeCheck => AnyVariant::ThreeCheck(ThreeCheck::startpos()),
            VariantId::RacingKings => AnyVariant::RacingKings(RacingKings::startpos()),
            VariantId::Horde => AnyVariant::Horde(Horde::startpos()),
            VariantId::Atomic => AnyVariant::Atomic(Atomic::startpos()),
            VariantId::Antichess => AnyVariant::Antichess(Antichess::startpos()),
            VariantId::Crazyhouse => AnyVariant::Crazyhouse(Crazyhouse::startpos()),
        }
    }

    /// Parses a position of the variant named by `id` from `fen`.
    ///
    /// # Errors
    ///
    /// Returns [`FenError`] if `fen` is malformed or fails the variant's
    /// validation (the same errors [`VariantPosition::from_fen`](super::VariantPosition::from_fen) reports).
    pub fn from_fen(id: VariantId, fen: &str) -> Result<Self, FenError> {
        let pos = match id {
            VariantId::Standard => AnyVariant::Chess(Chess::from_fen(fen)?),
            VariantId::Chess960 => AnyVariant::Chess960(Chess960::from_fen(fen)?),
            VariantId::KingOfTheHill => AnyVariant::KingOfTheHill(KingOfTheHill::from_fen(fen)?),
            VariantId::ThreeCheck => AnyVariant::ThreeCheck(ThreeCheck::from_fen(fen)?),
            VariantId::RacingKings => AnyVariant::RacingKings(RacingKings::from_fen(fen)?),
            VariantId::Horde => AnyVariant::Horde(Horde::from_fen(fen)?),
            VariantId::Atomic => AnyVariant::Atomic(Atomic::from_fen(fen)?),
            VariantId::Antichess => AnyVariant::Antichess(Antichess::from_fen(fen)?),
            VariantId::Crazyhouse => AnyVariant::Crazyhouse(Crazyhouse::from_fen(fen)?),
        };
        Ok(pos)
    }

    /// The stable identifier of the wrapped variant.
    #[must_use]
    pub fn variant_id(&self) -> VariantId {
        dispatch!(self, p => p.variant_id())
    }

    /// The side to move.
    #[must_use]
    pub fn turn(&self) -> Color {
        dispatch!(self, p => p.turn())
    }

    /// The legal moves of the side to move under the wrapped variant.
    #[must_use]
    pub fn legal_moves(&self) -> Vec<Move> {
        dispatch!(self, p => p.legal_moves())
    }

    /// Applies `mv`, returning the successor position in the same variant arm.
    ///
    /// The move must be legal (as for [`VariantPosition::play`](super::VariantPosition::play)).
    #[must_use]
    pub fn play(&self, mv: &Move) -> Self {
        match self {
            AnyVariant::Chess(p) => AnyVariant::Chess(p.play(mv)),
            AnyVariant::Chess960(p) => AnyVariant::Chess960(p.play(mv)),
            AnyVariant::KingOfTheHill(p) => AnyVariant::KingOfTheHill(p.play(mv)),
            AnyVariant::ThreeCheck(p) => AnyVariant::ThreeCheck(p.play(mv)),
            AnyVariant::RacingKings(p) => AnyVariant::RacingKings(p.play(mv)),
            AnyVariant::Horde(p) => AnyVariant::Horde(p.play(mv)),
            AnyVariant::Atomic(p) => AnyVariant::Atomic(p.play(mv)),
            AnyVariant::Antichess(p) => AnyVariant::Antichess(p.play(mv)),
            AnyVariant::Crazyhouse(p) => AnyVariant::Crazyhouse(p.play(mv)),
        }
    }

    /// The variant-aware game result, or `None` if the game is not over.
    #[must_use]
    pub fn outcome(&self) -> Option<Outcome> {
        dispatch!(self, p => p.outcome())
    }

    /// The variant-aware [`EndReason`], or `None` if the game is not over.
    #[must_use]
    pub fn end_reason(&self) -> Option<EndReason> {
        dispatch!(self, p => p.end_reason())
    }

    /// Whether the side to move is in check (always `false` where the king is not
    /// royal).
    #[must_use]
    pub fn is_check(&self) -> bool {
        dispatch!(self, p => p.is_check())
    }

    /// Renders `mv` as UCI long algebraic notation.
    #[must_use]
    pub fn to_uci(&self, mv: &Move) -> String {
        dispatch!(self, p => p.to_uci(mv))
    }

    /// Parses a UCI move string against this position.
    ///
    /// # Errors
    ///
    /// Returns [`ParseUciError`] if the string is malformed or names no legal
    /// move in this position.
    pub fn parse_uci(&self, uci: &str) -> Result<Move, ParseUciError> {
        dispatch!(self, p => p.parse_uci(uci))
    }

    /// Serializes this position to FEN.
    #[must_use]
    pub fn to_fen(&self) -> String {
        dispatch!(self, p => p.to_fen())
    }

    /// The Zobrist key of this position, including any variant-state
    /// contribution.
    #[must_use]
    pub fn zobrist(&self) -> Zobrist {
        dispatch!(self, p => p.zobrist())
    }

    /// Counts the leaf nodes reachable in exactly `depth` plies, forwarding to
    /// [`perft_variant`].
    #[must_use]
    pub fn perft(&self, depth: u32) -> u64 {
        dispatch!(self, p => perft_variant(p, depth))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every [`VariantId`], paired with the type-level perft check below.
    const ALL_IDS: [VariantId; 9] = [
        VariantId::Standard,
        VariantId::Chess960,
        VariantId::KingOfTheHill,
        VariantId::ThreeCheck,
        VariantId::RacingKings,
        VariantId::Horde,
        VariantId::Atomic,
        VariantId::Antichess,
        VariantId::Crazyhouse,
    ];

    #[test]
    fn from_str_accepts_canonical_names() {
        for id in ALL_IDS {
            assert_eq!(id.as_str().parse::<VariantId>(), Ok(id));
        }
    }

    #[test]
    fn from_str_accepts_documented_aliases() {
        let cases: &[(&str, VariantId)] = &[
            ("chess", VariantId::Standard),
            ("standard", VariantId::Standard),
            ("STANDARD", VariantId::Standard),
            ("  Chess960  ", VariantId::Chess960),
            ("fischerandom", VariantId::Chess960),
            ("fischerrandom", VariantId::Chess960),
            ("960", VariantId::Chess960),
            ("koth", VariantId::KingOfTheHill),
            ("kingofthehill", VariantId::KingOfTheHill),
            ("3check", VariantId::ThreeCheck),
            ("threecheck", VariantId::ThreeCheck),
            ("racing", VariantId::RacingKings),
            ("racingkings", VariantId::RacingKings),
            ("horde", VariantId::Horde),
            ("atomic", VariantId::Atomic),
            ("giveaway", VariantId::Antichess),
            ("antichess", VariantId::Antichess),
            ("zh", VariantId::Crazyhouse),
            ("house", VariantId::Crazyhouse),
            ("crazyhouse", VariantId::Crazyhouse),
        ];
        for (name, id) in cases {
            assert_eq!(name.parse::<VariantId>(), Ok(*id), "parsing {name:?}");
        }
    }

    #[test]
    fn from_str_rejects_junk() {
        for junk in ["", "chess9600", "kingofthevalley", "xyzzy", "check"] {
            let err = junk.parse::<VariantId>().unwrap_err();
            assert_eq!(err.0, junk.trim().to_ascii_lowercase());
        }
    }

    #[test]
    fn startpos_fen_round_trips_for_every_id() {
        for id in ALL_IDS {
            let pos = AnyVariant::startpos(id);
            let fen = pos.to_fen();
            let reparsed = AnyVariant::from_fen(id, &fen).expect("startpos fen parses");
            assert_eq!(reparsed.to_fen(), fen, "round trip for {id}");
            assert_eq!(reparsed, pos, "round trip equals startpos for {id}");
            assert_eq!(pos.variant_id(), id);
        }
    }

    /// Forwards through [`AnyVariant`] must agree with the generic
    /// [`VariantPosition<V>`](super::VariantPosition) for the matching variant.
    macro_rules! agree_with_generic {
        ($id:expr, $alias:ty, $any:path) => {{
            let generic = <$alias>::startpos();
            let any = AnyVariant::startpos($id);
            assert!(matches!(any, $any(_)));

            // Legal moves and outcome agree.
            assert_eq!(any.legal_moves(), generic.legal_moves());
            assert_eq!(any.outcome(), generic.outcome());
            assert_eq!(any.turn(), generic.turn());
            assert_eq!(any.is_check(), generic.is_check());
            assert_eq!(any.zobrist(), generic.zobrist());
            assert_eq!(any.to_fen(), generic.to_fen());

            // Playing the first legal move keeps the two in lockstep.
            if let Some(mv) = generic.legal_moves().first() {
                let any_after = any.play(mv);
                let generic_after = generic.play(mv);
                assert_eq!(any_after.to_fen(), generic_after.to_fen());
                assert_eq!(any_after.legal_moves(), generic_after.legal_moves());
                assert_eq!(any.to_uci(mv), generic.to_uci(mv));
                let uci = generic.to_uci(mv);
                assert_eq!(
                    any.parse_uci(&uci).unwrap(),
                    generic.parse_uci(&uci).unwrap()
                );
            }

            // Shallow perft agrees with the generic node counter.
            for depth in 0..=2 {
                assert_eq!(
                    any.perft(depth),
                    perft_variant(&generic, depth),
                    "perft {depth}"
                );
            }
        }};
    }

    #[test]
    fn matches_generic_for_every_variant() {
        agree_with_generic!(VariantId::Standard, Chess, AnyVariant::Chess);
        agree_with_generic!(VariantId::Chess960, Chess960, AnyVariant::Chess960);
        agree_with_generic!(
            VariantId::KingOfTheHill,
            KingOfTheHill,
            AnyVariant::KingOfTheHill
        );
        agree_with_generic!(VariantId::ThreeCheck, ThreeCheck, AnyVariant::ThreeCheck);
        agree_with_generic!(VariantId::RacingKings, RacingKings, AnyVariant::RacingKings);
        agree_with_generic!(VariantId::Horde, Horde, AnyVariant::Horde);
        agree_with_generic!(VariantId::Atomic, Atomic, AnyVariant::Atomic);
        agree_with_generic!(VariantId::Antichess, Antichess, AnyVariant::Antichess);
        agree_with_generic!(VariantId::Crazyhouse, Crazyhouse, AnyVariant::Crazyhouse);
    }
}
