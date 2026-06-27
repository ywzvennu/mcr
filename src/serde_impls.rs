//! Hand-written `serde` implementations, compiled only under the `serde`
//! feature.
//!
//! Most public value types derive `Serialize`/`Deserialize` directly at their
//! definition site via `#[cfg_attr(feature = "serde", derive(...))]`. The types
//! gathered here cannot use a plain derive because they hold *private* fields
//! whose in-memory encoding we do not want to leak onto the wire:
//!
//! - [`Square`] and [`Move`] pack their state into a private integer. They
//!   serialize through a small, explicit shape instead: a square as its `0..64`
//!   index, a move as a `{ from, to, kind }` record built back up with
//!   [`Move::new`].
//! - [`Board`] and [`Position`] serialize as their **FEN strings**, which are
//!   compact, stable across versions, and free of the private Zobrist hash and
//!   bitboard layout. The FEN round-trip is lossless for every legal value.
//! - [`CastlingRights`] serializes as the four rook files it records, the same
//!   information its public accessors expose.
//! - [`AnyVariant`] serializes as a `{ variant, fen }` pair: the [`VariantId`]
//!   selects the arm and the variant's own FEN (pockets, check counts, and all)
//!   carries the rest.
//!
//! Every representation here round-trips losslessly for legal inputs, and
//! deserialization validates its input (rejecting out-of-range indices and
//! malformed FEN) rather than trusting it.

use serde::de::{Error as DeError, Unexpected};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    AnyVariant, Board, CastleSide, CastlingRights, Color, File, Move, MoveKind, Position, Square,
    VariantId,
};

// -- Square -----------------------------------------------------------------

impl Serialize for Square {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.index())
    }
}

impl<'de> Deserialize<'de> for Square {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let index = u8::deserialize(deserializer)?;
        Square::try_new(index).ok_or_else(|| {
            DeError::invalid_value(
                Unexpected::Unsigned(u64::from(index)),
                &"a square index in 0..64",
            )
        })
    }
}

// -- Move -------------------------------------------------------------------

/// The wire shape of a [`Move`]: its public origin, destination, and kind. A
/// move rebuilds losslessly from these via [`Move::new`] — for a drop the
/// `from` field equals `to`, which `Move::new` ignores in favour of the kind's
/// role, and a promotion's capture flag is recovered geometrically from the
/// from/to files, exactly as the packed form does.
#[derive(Serialize, Deserialize)]
struct MoveRepr {
    from: Square,
    to: Square,
    kind: MoveKind,
}

impl Serialize for Move {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        MoveRepr {
            from: self.from(),
            to: self.to(),
            kind: self.kind(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Move {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let MoveRepr { from, to, kind } = MoveRepr::deserialize(deserializer)?;
        Ok(Move::new(from, to, kind))
    }
}

// -- Board ------------------------------------------------------------------

impl Serialize for Board {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_fen_placement())
    }
}

impl<'de> Deserialize<'de> for Board {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let placement = <&str>::deserialize(deserializer)?;
        Board::from_fen_placement(placement).map_err(DeError::custom)
    }
}

// -- CastlingRights ---------------------------------------------------------

/// The wire shape of [`CastlingRights`]: the rook file each of the four rights
/// is anchored to, or `None` when the right is absent. This is exactly what the
/// public [`CastlingRights::rook_file`] accessor reports, and it round-trips
/// arbitrary Chess960 rook files, not just the a-/h-file standard ones.
#[derive(Serialize, Deserialize)]
struct CastlingRightsRepr {
    white_king: Option<File>,
    white_queen: Option<File>,
    black_king: Option<File>,
    black_queen: Option<File>,
}

impl Serialize for CastlingRights {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        CastlingRightsRepr {
            white_king: self.rook_file(Color::White, CastleSide::King),
            white_queen: self.rook_file(Color::White, CastleSide::Queen),
            black_king: self.rook_file(Color::Black, CastleSide::King),
            black_queen: self.rook_file(Color::Black, CastleSide::Queen),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CastlingRights {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let CastlingRightsRepr {
            white_king,
            white_queen,
            black_king,
            black_queen,
        } = CastlingRightsRepr::deserialize(deserializer)?;
        Ok(CastlingRights::from_rook_files(
            white_king,
            white_queen,
            black_king,
            black_queen,
        ))
    }
}

// -- Position ---------------------------------------------------------------

impl Serialize for Position {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_fen())
    }
}

impl<'de> Deserialize<'de> for Position {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fen = <&str>::deserialize(deserializer)?;
        Position::from_fen(fen).map_err(DeError::custom)
    }
}

// -- AnyVariant -------------------------------------------------------------

/// The wire shape of [`AnyVariant`]: the [`VariantId`] selecting the arm and the
/// variant's FEN. The FEN carries the variant's full state (crazyhouse pockets,
/// three-check counters, and so on), so the pair round-trips every arm losslessly.
#[derive(Serialize, Deserialize)]
struct AnyVariantRepr {
    variant: VariantId,
    fen: String,
}

impl Serialize for AnyVariant {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        AnyVariantRepr {
            variant: self.variant_id(),
            fen: self.to_fen(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AnyVariant {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let AnyVariantRepr { variant, fen } = AnyVariantRepr::deserialize(deserializer)?;
        AnyVariant::from_fen(variant, &fen).map_err(DeError::custom)
    }
}
