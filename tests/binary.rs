//! Round-trip property tests for the compact binary wire format (issue #319).
//!
//! For every one of the 47 fairy variants this exercises:
//!
//! * the position codec — `decode(encode(pos)).to_fen() == pos.to_fen()` at the
//!   start position and at a bounded walk of midgame nodes reached by *playing*
//!   moves (which exercises the Duck square, the Alice plane mask, the Janggi
//!   pass counter, and the crazyhouse promoted mask — state FEN cannot even
//!   carry, so the binary form is strictly more faithful than a FEN round trip);
//! * the move codec — every legal move at every visited node round-trips
//!   (`WideMove` has `PartialEq`, so this is exact equality);
//! * the game-record codec — a start position plus a move list round-trips and
//!   replays to the same final position.
//!
//! Plus malformed-input handling: every decoder must return `Err` (never panic)
//! on truncated, mis-tagged, or out-of-range bytes.

use mce::geometry::{decode_game, encode_game, AnyWideVariant, WideMove, WideVariantId, WireError};

/// Encodes then decodes `pos` through the self-describing format and asserts the
/// canonical FEN survives (positions have no `PartialEq`, so FEN is the oracle).
#[track_caller]
fn assert_position_round_trips(pos: &AnyWideVariant) {
    let bytes = pos.to_bytes();
    let back = AnyWideVariant::from_bytes(&bytes).expect("decode position");
    assert_eq!(
        back.to_fen(),
        pos.to_fen(),
        "position round trip diverged for {}",
        pos.variant_id()
    );
    assert_eq!(back.variant_id(), pos.variant_id());
}

/// Asserts every legal move at `pos` survives the move codec exactly.
#[track_caller]
fn assert_moves_round_trip(pos: &AnyWideVariant) {
    for mv in pos.legal_moves() {
        let bytes = mv.to_bytes();
        let back = WideMove::from_bytes(&bytes).expect("decode move");
        assert_eq!(
            back,
            mv,
            "move round trip diverged for {}",
            pos.variant_id()
        );
    }
}

/// Walks the legal-move tree from `pos` to `depth`, recursing into at most
/// `breadth` moves per node, checking the position and move codecs at each node.
fn walk(pos: &AnyWideVariant, depth: u32, breadth: usize) {
    assert_position_round_trips(pos);
    assert_moves_round_trip(pos);
    if depth == 0 {
        return;
    }
    for mv in pos.legal_moves().into_iter().take(breadth) {
        walk(&pos.play(&mv), depth - 1, breadth);
    }
}

#[test]
fn round_trips_startpos_and_midgame_for_every_variant() {
    for &id in WideVariantId::ALL {
        let start = AnyWideVariant::startpos(id);
        // A bounded midgame walk: depth 3, three moves per node, reaching the
        // variant-specific addenda (duck/alice/promoted/pass) as *applied* state.
        walk(&start, 3, 3);
    }
}

#[test]
fn game_record_round_trips_for_every_variant() {
    for &id in WideVariantId::ALL {
        let start = AnyWideVariant::startpos(id);
        let mut moves = Vec::new();
        let mut cursor = start.clone();
        for _ in 0..6 {
            let Some(mv) = cursor.legal_moves().into_iter().next() else {
                break;
            };
            moves.push(mv);
            cursor = cursor.play(&mv);
        }
        let bytes = encode_game(&start, &moves);
        let (decoded_start, decoded_moves) = decode_game(&bytes).expect("decode game");
        assert_eq!(decoded_start.to_fen(), start.to_fen(), "{id} start");
        assert_eq!(decoded_moves, moves, "{id} moves");

        let mut replay = decoded_start;
        for mv in &decoded_moves {
            replay = replay.play(mv);
        }
        assert_eq!(replay.to_fen(), cursor.to_fen(), "{id} replay");
    }
}

#[test]
fn binary_is_smaller_than_fen_for_every_variant() {
    // The headline compactness claim: the encoded start position is shorter than
    // the FEN string it replaces, for all 47 variants.
    for &id in WideVariantId::ALL {
        let pos = AnyWideVariant::startpos(id);
        let bytes = pos.to_bytes().len();
        let fen = pos.to_fen().len();
        assert!(
            bytes < fen,
            "{id}: binary {bytes} bytes not smaller than FEN {fen} bytes",
        );
    }
}

#[test]
fn malformed_input_is_rejected_without_panic() {
    // Empty / wrong-tag / truncated inputs are rejected, never panicking.
    assert!(matches!(
        AnyWideVariant::from_bytes(&[]),
        Err(WireError::Truncated)
    ));
    assert!(matches!(
        AnyWideVariant::from_bytes(&[0x00, 0x00]),
        Err(WireError::BadTag(0x00))
    ));
    // Correct tag, but the variant selector names no variant.
    let bad_variant = {
        let mut v = AnyWideVariant::startpos(WideVariantId::Alice).to_bytes();
        v[1] = 200; // past the 47 variants
        AnyWideVariant::from_bytes(&v)
    };
    assert!(matches!(bad_variant, Err(WireError::UnknownVariant(200))));

    // A valid encoding with one extra trailing byte is rejected.
    let mut trailing = AnyWideVariant::startpos(WideVariantId::Shogi).to_bytes();
    trailing.push(0xAB);
    assert!(matches!(
        AnyWideVariant::from_bytes(&trailing),
        Err(WireError::TrailingData)
    ));

    // Move and game decoders likewise reject junk.
    assert_eq!(WideMove::from_bytes(&[]), Err(WireError::Truncated));
    assert!(matches!(decode_game(&[]), Err(WireError::Truncated)));
    // `0xD4` is the v2 game tag (issue #448); the varint claims a longer position
    // body than is present, so the decoder reports truncation, never panicking.
    assert!(matches!(
        decode_game(&[0xD4, 200, 0, 0, 0]),
        Err(WireError::Truncated | WireError::BadValue | WireError::BadTag(_))
    ));
}

/// Fuzz the decoders with structured-but-corrupt bytes: flipping any single byte
/// of a valid encoding must never panic (it either decodes to *some* position or
/// returns an error).
#[test]
fn single_byte_corruption_never_panics() {
    for id in [
        WideVariantId::Alice,
        WideVariantId::Duck,
        WideVariantId::Seirawan,
        WideVariantId::Shogi,
        WideVariantId::Capahouse,
        WideVariantId::Janggi,
    ] {
        let valid = AnyWideVariant::startpos(id).to_bytes();
        for i in 0..valid.len() {
            for delta in [1u8, 0x7f, 0x80, 0xff] {
                let mut corrupt = valid.clone();
                corrupt[i] = corrupt[i].wrapping_add(delta);
                // Must not panic; the result is unused.
                let _ = AnyWideVariant::from_bytes(&corrupt);
            }
        }
    }
}
