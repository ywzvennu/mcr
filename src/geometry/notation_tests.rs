// Tests for the geometry-layer notation module (UCI parse, SAN, PGN). Included
// from `notation.rs` as its `tests` submodule.

use super::*;
use crate::geometry::{AnyWideVariant, WideVariantId};
use alloc::vec::Vec;

// --- Role token round-trips ------------------------------------------------

#[test]
fn role_token_round_trips_every_role() {
    for role in WideRole::ALL {
        let mut tok = String::new();
        push_role_token(&mut tok, role);
        // A drop's full-token parser recovers the role.
        assert_eq!(
            parse_full_role_token(&tok),
            Some(role),
            "role token {tok:?} for {role}"
        );
        // The leading-role parser consumes the whole token and recovers it.
        let (parsed, consumed) = parse_leading_role(&tok).expect("leading role");
        assert_eq!(parsed, role, "leading role for {tok:?}");
        assert_eq!(consumed, tok.len());
    }
}

#[test]
fn role_token_prefixes_match_fen_convention() {
    let mut s = String::new();
    push_role_token(&mut s, WideRole::Pawn);
    assert_eq!(s, "P");
    s.clear();
    push_role_token(&mut s, WideRole::Commoner); // overflow
    assert_eq!(s, "*U");
    s.clear();
    push_role_token(&mut s, WideRole::DrunkElephant); // overflow2
    assert_eq!(s, "**E");
    s.clear();
    push_role_token(&mut s, WideRole::RookCannon); // overflow3
    assert_eq!(s, "=A");
    s.clear();
    push_role_token(&mut s, WideRole::Tokin); // shogi promoted
    assert_eq!(s, "+P");
}

// --- Helpers ---------------------------------------------------------------

/// Walks `depth` plies from `pos`, asserting at every node that every legal move
/// round-trips through both UCI (move -> UCI -> move) and SAN (move -> SAN ->
/// move).
fn assert_round_trips(pos: &AnyWideVariant, depth: u32) {
    for mv in pos.legal_moves() {
        // UCI round-trip: render, then resolve back against the legal moves.
        let uci = pos.to_uci(&mv);
        assert_eq!(
            pos.parse_uci(&uci),
            Some(mv),
            "UCI round-trip {uci:?} in {} ({})",
            pos.variant_id(),
            pos.to_fen()
        );
        // SAN round-trip: render, then parse back.
        let san = pos.san(&mv);
        assert_eq!(
            pos.parse_san(&san),
            Some(mv),
            "SAN round-trip {san:?} (uci {uci}) in {} ({})",
            pos.variant_id(),
            pos.to_fen()
        );
    }
    if depth == 0 {
        return;
    }
    for mv in pos.legal_moves() {
        assert_round_trips(&pos.play(&mv), depth - 1);
    }
}

/// Asserts UCI is *injective* over the legal moves of `pos`: no two distinct
/// legal moves share a UCI string (otherwise `parse_uci` could not be lossless).
fn assert_uci_injective(pos: &AnyWideVariant) {
    let mut seen: Vec<(String, WideMove)> = Vec::new();
    for mv in pos.legal_moves() {
        let uci = pos.to_uci(&mv);
        if let Some((_, other)) = seen.iter().find(|(u, _)| *u == uci) {
            assert_eq!(
                *other, mv,
                "two distinct legal moves share UCI {uci:?} in {} ({})",
                pos.variant_id(),
                pos.to_fen()
            );
        }
        seen.push((uci, mv));
    }
}

// --- UCI + SAN round-trip audit over pinned corpora ------------------------

#[test]
fn uci_and_san_round_trip_startpos_corpus() {
    // One representative per move-kind family, exercised from the start position
    // a couple of plies deep.
    for id in [
        WideVariantId::Capablanca, // wide board, promotion
        WideVariantId::Shogi,      // drops, `+` promotion
        WideVariantId::Seirawan,   // gating
        WideVariantId::Shouse,     // hand gating
        WideVariantId::Janggi,     // passes
        WideVariantId::Xiangqi,    // fairy pieces, royal check
        WideVariantId::Chak,       // 9x9 overflow army
        WideVariantId::CannonShogi, // overflow-3 army
    ] {
        let pos = AnyWideVariant::startpos(id);
        assert_uci_injective(&pos);
        assert_round_trips(&pos, 1);
    }
}

#[test]
fn uci_round_trip_pinned_fens() {
    // Pinned mid-game FENs spanning the special move kinds.
    let cases: &[(WideVariantId, &str)] = &[
        // Seirawan with both reserves still in hand: gating moves are legal.
        (
            WideVariantId::Seirawan,
            "r1bqkb1r/pppppppp/2n2n2/8/8/2N2N2/PPPPPPPP/R1BQKB1R[HEhe] w KQBCDEFGkqbcdefg - 4 3",
        ),
        // S-House mid-game with hand reserves.
        (
            WideVariantId::Shouse,
            "r1bqk2r/ppp2ppp/2n5/3pp3/3PP3/2N5/PPP2PPP/R1BQK2R[EANean] w KQCDkqcd - 0 1",
        ),
        // A Janggi position where passes are available.
        (
            WideVariantId::Janggi,
            "9/1k7/r1r3c2/9/9/9/J1C3J2/9/4K4/C1C3C2 w - - 0 1",
        ),
    ];
    for (id, fen) in cases {
        let pos = AnyWideVariant::from_fen(*id, fen).expect("pinned FEN parses");
        assert_uci_injective(&pos);
        for mv in pos.legal_moves() {
            let uci = pos.to_uci(&mv);
            assert_eq!(pos.parse_uci(&uci), Some(mv), "UCI {uci:?} in {id} {fen}");
            let san = pos.san(&mv);
            assert_eq!(pos.parse_san(&san), Some(mv), "SAN {san:?} in {id} {fen}");
        }
    }
}

#[test]
fn san_covers_drops_and_passes_explicitly() {
    // A Shogi drop renders with the `@` form and round-trips.
    let shogi = AnyWideVariant::startpos(WideVariantId::Shogi);
    // Play a couple of moves to get a piece into hand is involved; instead use a
    // FEN with a pawn in hand.
    // White has a pawn in hand and an open c-file, so a pawn drop is legal
    // (no `nifu` two-pawns-per-file violation).
    let drop_pos = AnyWideVariant::from_fen(
        WideVariantId::Shogi,
        "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PP1PPPPPP/1B5R1/LNSGKGSNL[P] w - - 0 1",
    )
    .expect("shogi drop FEN parses");
    let drop = drop_pos
        .legal_moves()
        .into_iter()
        .find(|m| m.is_drop())
        .expect("a pawn drop is legal");
    let san = drop_pos.san(&drop);
    assert!(san.starts_with("P@"), "pawn drop SAN is `P@..`, got {san:?}");
    assert_eq!(drop_pos.parse_san(&san), Some(drop));
    let _ = shogi;

    // A Janggi pass renders as `--` and round-trips.
    let janggi = AnyWideVariant::from_fen(
        WideVariantId::Janggi,
        "9/1k7/r1r3c2/9/9/9/J1C3J2/9/4K4/C1C3C2 w - - 0 1",
    )
    .expect("janggi FEN");
    let pass = janggi
        .legal_moves()
        .into_iter()
        .find(|m| m.from_index() == m.to_index() && !m.is_drop())
        .expect("a pass is legal in Janggi");
    assert_eq!(janggi.san(&pass), "--");
    assert_eq!(janggi.parse_san("--"), Some(pass));
}

// --- PGN export / import ----------------------------------------------------

/// Plays the first `n` legal moves greedily from `start`, returning the move
/// list (a deterministic short game).
fn short_game(start: &AnyWideVariant, n: usize) -> Vec<WideMove> {
    let mut pos = start.clone();
    let mut moves = Vec::new();
    for _ in 0..n {
        let legal = pos.legal_moves();
        let Some(mv) = legal.into_iter().next() else {
            break;
        };
        moves.push(mv);
        pos = pos.play(&mv);
    }
    moves
}

#[test]
fn pgn_round_trips_a_fairy_game() {
    for id in [
        WideVariantId::Capablanca,
        WideVariantId::Shogi,
        WideVariantId::Seirawan,
        WideVariantId::Xiangqi,
    ] {
        let start = AnyWideVariant::startpos(id);
        let moves = short_game(&start, 6);
        let pgn = WidePgn::from_game(&start, &moves, Vec::new()).expect("game is legal");
        let text = pgn.to_pgn();
        assert!(
            text.contains(&format!("[Variant \"{}\"]", id.as_str())),
            "variant tag present for {id}"
        );

        let reparsed = WidePgn::from_pgn(&text).expect("exported PGN parses");
        assert_eq!(reparsed.variant(), id);
        assert_eq!(reparsed.moves(), pgn.moves(), "moves round-trip for {id}");
        assert_eq!(
            reparsed.final_position().to_fen(),
            pgn.final_position().to_fen(),
            "final position round-trips for {id}"
        );
        // A second export is byte-identical (canonical form is stable).
        assert_eq!(reparsed.to_pgn(), text, "PGN re-export is stable for {id}");
    }
}

#[test]
fn pgn_round_trips_with_custom_fen_setup() {
    // A non-standard Shogi start (a pawn already in hand) is carried via the
    // SetUp/FEN tags and recovered on import.
    let fen = "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PP1PPPPPP/1B5R1/LNSGKGSNL[P] w - - 0 1";
    let start = AnyWideVariant::from_fen(WideVariantId::Shogi, fen).expect("setup FEN");
    let moves = short_game(&start, 4);
    let pgn = WidePgn::from_game(&start, &moves, Vec::new()).expect("legal game");
    let text = pgn.to_pgn();
    assert!(text.contains("[SetUp \"1\"]"));
    assert!(text.contains("[FEN \""));

    let reparsed = WidePgn::from_pgn(&text).expect("custom-setup PGN parses");
    assert_eq!(
        reparsed.start_position().to_fen(),
        start.to_fen(),
        "custom start FEN round-trips"
    );
    assert_eq!(reparsed.moves(), pgn.moves());
    assert_eq!(reparsed.to_pgn(), text);
}

#[test]
fn parse_san_is_lenient_on_glyphs_and_castling() {
    let pos = AnyWideVariant::startpos(WideVariantId::Capablanca);
    // A knight move with a trailing annotation glyph resolves.
    let nf3 = pos.parse_san("Nc3").expect("Nc3 is legal");
    assert_eq!(pos.parse_san("Nc3!?"), Some(nf3));
}

#[test]
fn parse_san_rejects_junk_without_panic() {
    let pos = AnyWideVariant::startpos(WideVariantId::Shogi);
    assert_eq!(pos.parse_san(""), None);
    assert_eq!(pos.parse_san("   "), None);
    assert_eq!(pos.parse_san("zzz9"), None);
    // Non-ASCII must be rejected, not panic.
    assert_eq!(pos.parse_san("\u{1f600}"), None);
    assert_eq!(pos.parse_san("N\u{e9}f3"), None);
}
