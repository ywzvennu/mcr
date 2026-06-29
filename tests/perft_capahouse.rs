//! Capahouse (10x8 / `u128`) perft validation on the generic engine (issue #263)
//! — **Capablanca chess plus crazyhouse drops**.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mce::geometry::Capahouse::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant capahouse`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position. The `compare-fairy/` harness re-runs that head-to-head
//! on demand (`compare-fairy/src/capahouse.rs`); this test pins the FSF-confirmed
//! numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `capahouse_variant()` `startFen`:
//!
//! ```text
//! FSF dialect: rnabqkbcnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBCNR[] w KQkq - 0 1
//! mce dialect: rnabqkbenr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBENR[] w KQkq - 0 1
//! ```
//!
//! The two differ only in the chancellor's letter (`c` in FSF, `e` in mce); the
//! trailing `[]` is the empty crazyhouse hand. The deep layers are `#[ignore]`d so
//! `cargo test` stays fast — run them with
//! `cargo test --release --test perft_capahouse -- --include-ignored`.

use mce::geometry::{perft as gperft, Cap10x8, Capahouse, WideMoveKind, WideRole};

/// The Capahouse starting FEN (mce dialect), confirmed against FSF.
const STARTPOS: &str = "rnabqkbenr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBENR[] w KQkq - 0 1";

/// A drop-heavy position: both sides hold a queen and a rook in hand, so move
/// generation must enumerate the crazyhouse drops onto every empty square (the
/// pawn drop restriction does not bite a queen/rook).
const HANDS: &str = "rnabqkbenr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBENR[QRqr] w KQkq - 0 1";

/// A promotion/demotion position: a lone white pawn one rank from promotion next
/// to a black rook it can capture-promote, with the black king poised to recapture
/// the promoted piece — exercising the promoted mask (a captured promoted piece
/// banks as a Pawn) within the perft tree.
const PROMO: &str = "1rk7/P9/10/10/10/10/10/5K4[] w - - 0 1";

/// A developed midgame with a knight in each hand and a contested centre.
const MID: &str = "r1abqkbenr/pp1ppppppp/2p7/10/2n7/2N7/PP1PPPPPPP/R1ABQKBENR[Nn] w KQkq - 0 1";

/// Asserts the generic Capahouse perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF capahouse `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Capahouse::from_fen(fen).expect("valid Capahouse FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Cap10x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Capahouse perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 28), (2, 784), (3, 25228), (4, 805128)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 29097768)]);
}

// -- Hands in pocket (FSF-confirmed) ----------------------------------------

#[test]
fn hands_cheap() {
    check(HANDS, &[(1, 108), (2, 11466)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn hands_deep() {
    check(HANDS, &[(3, 1009512)]);
}

// -- Promotion + demotion (FSF-confirmed) -----------------------------------

#[test]
fn promo_cheap() {
    check(PROMO, &[(1, 17), (2, 123), (3, 2864), (4, 50312)]);
}

// -- Midgame (FSF-confirmed) ------------------------------------------------

#[test]
fn mid_cheap() {
    check(MID, &[(1, 74), (2, 5567)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn mid_deep() {
    check(MID, &[(3, 291260)]);
}

// -- Rule-level self-checks (independent of FSF) ----------------------------

/// The starting array round-trips through FEN — including the empty hand bracket
/// — and matches the confirmed string.
#[test]
fn startpos_fen_round_trips() {
    let pos = Capahouse::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    assert_eq!(pos.legal_move_count(), 28);
}

/// A captured natural piece banks as its own role; a captured **promoted** piece
/// (one that reached the board through promotion) banks as a Pawn. This pins the
/// crazyhouse "promoted pieces demote" rule and the `~` FEN round-trip.
#[test]
fn promoted_piece_demotes_to_pawn_in_hand() {
    // White pawn a7 captures the black rook on b8 and promotes to a Queen; the
    // resulting white queen on b8 is a promoted piece (rendered `Q~`).
    let pos = Capahouse::from_fen("1rk7/P9/10/10/10/10/10/5K4[] w - - 0 1").expect("valid");
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
    // FEN marks the promoted queen with a trailing `~`, and the black king/rook
    // never banked anything for White's pawn (a quiet promotion-capture banks the
    // captured rook for White).
    assert!(
        after_promo.to_fen().contains("Q~"),
        "promoted queen carries a ~ marker: {}",
        after_promo.to_fen()
    );

    // Black's king on c8 recaptures the promoted queen on b8; because that queen
    // was promoted, Black banks a *Pawn*, not a Queen.
    let recapture = after_promo
        .legal_moves()
        .into_iter()
        .find(|m| m.is_capture() && m.to::<Cap10x8>().index() == promo.to::<Cap10x8>().index())
        .expect("the black king can recapture on b8");
    let after_recap = after_promo.play(&recapture);
    let fen = after_recap.to_fen();
    // The hand bracket holds White's captured rook (`R`, from the promotion-
    // capture) and Black's recaptured piece. Because that piece was promoted,
    // Black banks a *Pawn* (`p`), not a Queen (`q`).
    let hand = fen.split('[').nth(1).expect("a hand bracket");
    assert!(
        hand.contains('p') && !hand.to_ascii_lowercase().contains('q'),
        "the recaptured promoted queen demotes to a pawn in hand: {fen}"
    );
}

/// A pawn may not be dropped on the first or last rank, but every other empty
/// square (ranks 2-7) is legal; other pieces may drop anywhere empty.
#[test]
fn pawn_drops_avoid_first_and_last_rank() {
    // White holds a pawn; an otherwise empty board (two kings tucked away).
    let pos = Capahouse::from_fen("5k4/10/10/10/10/10/10/5K4[P] w - - 0 1").expect("valid");
    let pawn_drops: Vec<u8> = pos
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
        .map(|m| m.to::<Cap10x8>().rank())
        .collect();
    // No drop on rank 0 (rank 1) or rank 7 (rank 8).
    assert!(
        pawn_drops.iter().all(|&r| r != 0 && r != 7),
        "no pawn drop on the first or last rank"
    );
    // Ranks 2-7 (indices 1..=6) are all reachable.
    for r in 1u8..=6 {
        assert!(
            pawn_drops.contains(&r),
            "pawn drop available on rank index {r}"
        );
    }
}
