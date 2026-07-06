//! Pocket Knight chess (8x8) perft validation on the generic engine — standard
//! chess with **one extra Knight in hand per side, droppable at any move**, and
//! nothing else changed.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Pocketknight::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant pocketknight`, the built-in `pocketknight_variant()`) running
//! `go perft` on the byte-identical position. mcr and FSF spell Pocket Knight with
//! the same standard-chess letters and the Knight banked as `N`/`n` in the `[Nn]`
//! holdings bracket, so no dialect rewrite is needed. The `compare-fairy/`
//! differential fuzzer re-runs that head-to-head on demand; this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Where it diverges from standard chess
//!
//! From the opening the pocket adds a Knight drop onto every empty square, so the
//! startpos branching is far wider than standard chess (perft(1) = 52 = the 20
//! standard opening moves + 32 Knight drops, not 20). The pocket is a **one-shot**
//! reserve: captures do **not** bank into the hand (`capturesToHand = false`), so
//! once both pockets are empty the tree collapses back to plain standard chess —
//! the empty-pocket startpos counts (20 / 400 / 8902 / 197281) are exactly standard
//! chess.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `pocketknight_variant()` (`variant.cpp:688` — standard chess with
//! `pieceDrops = true`, `capturesToHand = false`, and a starting `[Nn]` pocket):
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[Nn] w KQkq - 0 1
//! ```
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_pocketknight -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Pocketknight, WideMoveKind, WideRole};
use mcr::Color;

/// The Pocket Knight starting FEN, confirmed against Fairy-Stockfish's
/// `UCI_Variant pocketknight`.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[Nn] w KQkq - 0 1";

/// Kiwipete with both pockets full: a castling-rich, tactically busy position where
/// the pocket Knight adds a drop onto every empty square on top of the standard
/// (already large) branching.
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R[Nn] w KQkq - 0 1";

/// One-sided pocket: only White still holds a Knight (Black's pocket is empty), so
/// White has the extra drops and Black plays plain standard chess — exercising the
/// asymmetric-hand FEN `[N]`.
const ONE_SIDED: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[N] w KQkq - 0 1";

/// Empty pockets: both Knights already spent. With no hand the variant collapses
/// exactly onto standard chess, so these counts are the classic startpos numbers.
const EMPTY_POCKET: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1";

/// A promotion position with both pockets full: exercises the standard four-role
/// promotion set alongside the Knight drops.
const PROMO: &str = "4k3/1P6/8/8/8/8/6p1/4K3[Nn] w - - 0 1";

/// Asserts the generic Pocket Knight perft equals each pinned `(depth, nodes)`
/// count. Every number here also matched FSF pocketknight `go perft` on the same
/// position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Pocketknight::from_fen(fen).expect("valid Pocket Knight FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "pocketknight perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    // 52 = 20 standard opening moves + 32 Knight drops (one per empty square).
    check(STARTPOS, &[(1, 52), (2, 2565), (3, 88617), (4, 3071267)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 99614985)]);
}

// -- Kiwipete with pockets (FSF-confirmed) ----------------------------------

#[test]
fn kiwipete_cheap() {
    check(KIWIPETE, &[(1, 80), (2, 5852), (3, 390853), (4, 23593092)]);
}

// -- One-sided pocket (FSF-confirmed) ---------------------------------------

#[test]
fn one_sided_pocket_cheap() {
    check(ONE_SIDED, &[(1, 52), (2, 995), (3, 35942), (4, 766389)]);
}

// -- Empty pockets: collapses to standard chess (FSF-confirmed) -------------

#[test]
fn empty_pocket_matches_standard_chess() {
    // No hand → plain standard chess: the classic startpos perft numbers.
    check(EMPTY_POCKET, &[(1, 20), (2, 400), (3, 8902), (4, 197281)]);
}

// -- Promotion with pockets (FSF-confirmed) ---------------------------------

#[test]
fn promo_with_pockets_cheap() {
    check(PROMO, &[(1, 68), (2, 4230), (3, 72683), (4, 1321988)]);
}

// -- Rule-level self-checks (independent of FSF) ----------------------------

/// The starting array round-trips through FEN with the `[Nn]` pocket, keeps full
/// castling rights, and offers the 20 standard opening moves plus a Knight drop on
/// each of the 32 empty squares.
#[test]
fn startpos_fen_round_trips_with_pocket() {
    let pos = Pocketknight::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    assert_eq!(pos.turn(), Color::White);
    assert!(pos.castling().has_any(Color::White));
    assert!(pos.castling().has_any(Color::Black));

    let drops = pos
        .legal_moves()
        .into_iter()
        .filter(|m| matches!(m.kind(), WideMoveKind::Drop { .. }))
        .count();
    assert_eq!(drops, 32, "a Knight may drop onto every empty square");
    assert_eq!(pos.legal_move_count(), 20 + 32);
}

/// The only droppable role is the Knight — the pocket holds nothing else, and
/// captures never add to it.
#[test]
fn only_the_knight_is_droppable() {
    let pos = Pocketknight::startpos();
    let mut drop_roles: Vec<WideRole> = pos
        .legal_moves()
        .into_iter()
        .filter_map(|m| match m.kind() {
            WideMoveKind::Drop { role } => Some(role),
            _ => None,
        })
        .collect();
    drop_roles.sort();
    drop_roles.dedup();
    assert_eq!(drop_roles, vec![WideRole::Knight], "only Knight drops");
}

/// Captures do not bank into the hand: after a capture the pocket still holds
/// exactly the one starting Knight per side (never two).
#[test]
fn capture_does_not_replenish_the_hand() {
    let pos =
        Pocketknight::from_fen("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR[Nn] w KQkq d6 0 3")
            .expect("valid FEN");
    let capture = pos
        .legal_moves()
        .into_iter()
        .find(|m| matches!(m.kind(), WideMoveKind::Capture))
        .expect("exd5 is available");
    let next = pos.play(&capture);
    assert!(
        next.to_fen().contains("[Nn]"),
        "capture must not add to the pocket: {}",
        next.to_fen()
    );
}
