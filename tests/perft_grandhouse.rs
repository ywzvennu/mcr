//! Grandhouse (10x10 / `u128`) perft validation on the generic engine (issue #265)
//! — **Grand chess plus crazyhouse drops**.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Grandhouse::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant grandhouse`, the `[grandhouse:grand]` `variants.ini` entry, built
//! `largeboards=yes`) running `go perft` on the byte-identical position. The
//! `compare-fairy/` harness re-runs that head-to-head on demand
//! (`compare-fairy/src/grandhouse.rs`); this test pins the FSF-confirmed numbers
//! so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `[grandhouse:grand]` `startFen`:
//!
//! ```text
//! FSF dialect: r8r/1nbqkcabn1/pppppppppp/10/10/10/10/PPPPPPPPPP/1NBQKCABN1/R8R[] w - - 0 1
//! mcr dialect: r8r/1nbqkeabn1/pppppppppp/10/10/10/10/PPPPPPPPPP/1NBQKEABN1/R8R[] w - - 0 1
//! ```
//!
//! The two differ only in the marshal's letter (`c` in FSF, `e` in mcr); the
//! trailing `[]` is the empty crazyhouse hand and there is no castling (`-`). The
//! deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_grandhouse -- --include-ignored`.

use mcr::geometry::{perft as gperft, Grand10x10, Grandhouse, WideMoveKind, WideRole};

/// The Grandhouse starting FEN (mcr dialect, marshal `e`), confirmed against FSF.
const STARTPOS: &str =
    "r8r/1nbqkeabn1/pppppppppp/10/10/10/10/PPPPPPPPPP/1NBQKEABN1/R8R[] w - - 0 1";

/// A drop-heavy position: both sides hold a queen and a pawn in hand, so move
/// generation must enumerate the crazyhouse drops — including the colour-aware
/// pawn drop region (white pawn drops only on ranks 2-7, black only on 4-9).
const HANDS: &str =
    "r8r/1nbqkeabn1/pppppppppp/10/10/10/10/PPPPPPPPPP/1NBQKEABN1/R8R[QPqp] w - - 0 1";

/// A promotion/demotion position: a lone white pawn one rank from promotion next
/// to a black rook it can capture-promote, with the black king poised to recapture
/// the promoted piece — exercising the promoted mask (a captured promoted piece
/// banks as a Pawn) within the perft tree.
const PROMO: &str = "1rk7/P9/10/10/10/10/10/10/10/5K4[] w - - 0 1";

/// A developed midgame with a knight in each hand and a contested centre.
const MID: &str =
    "r8r/2bqkeab2/pppp1ppppp/2n4n2/3Np5/3P6/7N2/PPP1PPPPPP/2BQKEAB2/R8R[Nn] w - - 1 4";

/// Asserts the generic Grandhouse perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF grandhouse `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Grandhouse::from_fen(fen).expect("valid Grandhouse FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Grand10x10, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Grandhouse perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 65), (2, 4225), (3, 259514)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(4, 15921643)]);
}

// -- Hands in pocket (FSF-confirmed) ----------------------------------------

#[test]
fn hands_cheap() {
    check(HANDS, &[(1, 167), (2, 27108)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn hands_deep() {
    check(HANDS, &[(3, 3642336)]);
}

// -- Promotion + demotion (FSF-confirmed) -----------------------------------

#[test]
fn promo_cheap() {
    check(PROMO, &[(1, 17), (2, 139), (3, 3496), (4, 64972)]);
}

// -- Midgame (FSF-confirmed) ------------------------------------------------

#[test]
fn mid_cheap() {
    check(MID, &[(1, 137), (2, 16941)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn mid_deep() {
    check(MID, &[(3, 1805660)]);
}

// -- Rule-level self-checks (independent of FSF) ----------------------------

/// The starting array round-trips through FEN — including the empty hand bracket
/// — and matches the confirmed string.
#[test]
fn startpos_fen_round_trips() {
    let pos = Grandhouse::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    assert_eq!(pos.legal_move_count(), 65);
}

/// A captured natural piece banks as its own role; a captured **promoted** piece
/// (one that reached the board through promotion) banks as a Pawn. This pins the
/// crazyhouse "promoted pieces demote" rule and the `~` FEN round-trip.
#[test]
fn promoted_piece_demotes_to_pawn_in_hand() {
    // White pawn a9 captures the black rook on b10 and promotes to a Queen; the
    // resulting white queen on b10 is a promoted piece (rendered `Q~`).
    let pos = Grandhouse::from_fen(PROMO).expect("valid");
    let promo = pos
        .legal_moves()
        .into_iter()
        .find(|m| {
            matches!(
                m.kind(),
                WideMoveKind::Promotion {
                    role: WideRole::Queen,
                    capture: true
                }
            )
        })
        .expect("a capture-promotion to queen exists");
    let after_promo = pos.play(&promo);
    assert!(
        after_promo.to_fen().contains("Q~"),
        "promoted queen carries a ~ marker: {}",
        after_promo.to_fen()
    );

    // Black's king on c10 recaptures the promoted queen on b10; because that queen
    // was promoted, Black banks a *Pawn*, not a Queen.
    let recapture = after_promo
        .legal_moves()
        .into_iter()
        .find(|m| {
            m.is_capture() && m.to::<Grand10x10>().index() == promo.to::<Grand10x10>().index()
        })
        .expect("the black king can recapture on b10");
    let after_recap = after_promo.play(&recapture);
    let fen = after_recap.to_fen();
    let hand = fen.split('[').nth(1).expect("a hand bracket");
    assert!(
        hand.contains('p') && !hand.to_ascii_lowercase().contains('q'),
        "the recaptured promoted queen demotes to a pawn in hand: {fen}"
    );
}

/// A white pawn may be dropped only on ranks 2-7 (not its back rank, rank 1, nor
/// the promotion zone, ranks 8-10); other pieces may drop anywhere empty.
#[test]
fn white_pawn_drops_avoid_back_rank_and_promotion_zone() {
    let pos = Grandhouse::from_fen("5k4/10/10/10/10/10/10/10/10/5K4[P] w - - 0 1").expect("valid");
    let pawn_drop_ranks: Vec<u8> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| {
            matches!(
                m.kind(),
                WideMoveKind::Drop {
                    role: WideRole::Pawn
                }
            )
        })
        .map(|m| m.to::<Grand10x10>().rank())
        .collect();
    // Only ranks 2-7 (indices 1..=6) are legal.
    assert!(
        pawn_drop_ranks.iter().all(|&r| (1..=6).contains(&r)),
        "white pawn drops confined to ranks 2-7: {pawn_drop_ranks:?}"
    );
    for r in 1u8..=6 {
        assert!(
            pawn_drop_ranks.contains(&r),
            "white pawn drop available on rank index {r}"
        );
    }
}

/// A black pawn may be dropped only on ranks 4-9 (not its back rank, rank 10, nor
/// the promotion zone, ranks 1-3) — the colour-mirrored region.
#[test]
fn black_pawn_drops_avoid_back_rank_and_promotion_zone() {
    let pos = Grandhouse::from_fen("5k4/10/10/10/10/10/10/10/10/5K4[p] b - - 0 1").expect("valid");
    let pawn_drop_ranks: Vec<u8> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| {
            matches!(
                m.kind(),
                WideMoveKind::Drop {
                    role: WideRole::Pawn
                }
            )
        })
        .map(|m| m.to::<Grand10x10>().rank())
        .collect();
    // Only ranks 4-9 (indices 3..=8) are legal.
    assert!(
        pawn_drop_ranks.iter().all(|&r| (3..=8).contains(&r)),
        "black pawn drops confined to ranks 4-9: {pawn_drop_ranks:?}"
    );
    for r in 3u8..=8 {
        assert!(
            pawn_drop_ranks.contains(&r),
            "black pawn drop available on rank index {r}"
        );
    }
}
