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

use crate::geometry::{
    Board as WideBoard, GenericPlacement, GenericPosition, Geometry, WideRole, WideVariant,
};
use crate::{
    AnyVariant, Board, CastleSide, CastlingRights, Color, File, Move, MoveKind, Position, Square,
    VariantId,
};
use alloc::string::String;
use alloc::vec::Vec;

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

// === geometry layer ========================================================
//
// The wide (large-board, fairy-variant) layer mirrors the concrete layer's
// choices: the generic [`WideBoard`] and [`GenericPosition`] serialize as their
// **FEN strings** (placement for the board; the full six-field FEN — pockets,
// gating, Duck, promoted markers, and all — for the position), which are compact,
// stable, and free of the private Zobrist hash and bitboard layout. Both are
// generic over the geometry `G` (and the variant marker `V`); each concrete
// `pub type` alias (`Xiangqi`, `Shogi`, `Seirawan`, …) is a distinct type, so its
// FEN dialect is fixed at the type level and the round-trip is lossless for every
// legal value. The placement-phase pocket [`GenericPlacement`] serializes through
// a small explicit shape — its per-role counts for each side — rather than its
// private fixed-size arrays. ([`WideMove`](crate::geometry::WideMove) and the wide
// scalar enums carry their own impls at their definition sites.)

// -- Board<G> ---------------------------------------------------------------

impl<G: Geometry> Serialize for WideBoard<G> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_fen_placement())
    }
}

impl<'de, G: Geometry> Deserialize<'de> for WideBoard<G> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let placement = <&str>::deserialize(deserializer)?;
        WideBoard::<G>::from_fen_placement(placement).map_err(DeError::custom)
    }
}

// -- GenericPosition<G, V> --------------------------------------------------

impl<G: Geometry, V: WideVariant<G>, const R: usize> Serialize for GenericPosition<G, V, R> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_fen())
    }
}

impl<'de, G: Geometry, V: WideVariant<G>, const R: usize> Deserialize<'de>
    for GenericPosition<G, V, R>
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fen = <&str>::deserialize(deserializer)?;
        GenericPosition::<G, V, R>::from_fen(fen).map_err(DeError::custom)
    }
}

// -- GenericPlacement -------------------------------------------------------

/// The wire shape of a [`GenericPlacement`] pocket: one undeployed-piece count per
/// [`WideRole`] for each side, in [`WideRole::index`] order. This is exactly what
/// the public [`GenericPlacement::count`] accessor reports, and it round-trips
/// every pocket without leaking the type's private fixed-size count arrays.
#[derive(Serialize, Deserialize)]
struct GenericPlacementRepr {
    white: Vec<u8>,
    black: Vec<u8>,
}

impl<const R: usize> Serialize for GenericPlacement<R> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let counts = |color: Color| -> Vec<u8> {
            (0..R)
                .map(|i| match WideRole::from_index(i) {
                    Some(role) => self.count(color, role),
                    None => 0,
                })
                .collect()
        };
        GenericPlacementRepr {
            white: counts(Color::White),
            black: counts(Color::Black),
        }
        .serialize(serializer)
    }
}

impl<'de, const R: usize> Deserialize<'de> for GenericPlacement<R> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let GenericPlacementRepr { white, black } =
            GenericPlacementRepr::deserialize(deserializer)?;
        if white.len() != R || black.len() != R {
            return Err(DeError::custom("pocket must carry one count per WideRole"));
        }
        let mut white_counts = [0u8; R];
        let mut black_counts = [0u8; R];
        white_counts.copy_from_slice(&white);
        black_counts.copy_from_slice(&black);
        Ok(GenericPlacement::new(white_counts, black_counts))
    }
}
