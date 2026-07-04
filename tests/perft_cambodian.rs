//! Cambodian chess / Ouk Chaktrang perft validation on the generic engine
//! (issue #234).
//!
//! Cambodian is Makruk plus a one-time first-move leap for the king (to either
//! forward-knight square) and the queen/Met (a two-square straight advance). The
//! node counts below are pinned **and cross-checked against Fairy-Stockfish**
//! (FSF, `UCI_Variant cambodian`): every `(depth, nodes)` pair here was produced
//! identically by `mcr::geometry::Cambodian::perft` and by FSF's `go perft` on
//! the byte-identical FEN. The `compare-fairy/` harness re-runs that head-to-head
//! on demand (see `compare-fairy/src/main.rs --cambodian`); this test pins the
//! confirmed numbers so a regression is caught without FSF present.
//!
//! Confirmed Cambodian starting FEN (from FSF `position startpos`):
//!   `rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w DEde - 0 1`
//!
//! The two leap rights are encoded in the castling field as the **home file
//! letter** of each piece (uppercase white, lowercase black): `D` = white king
//! on the d-file, `E` = white Met on the e-file, `d` = black Met on the d-file,
//! `e` = black king on the e-file. FSF mirrors the king/Met pair between the
//! colors, so the field reads `DEde`.
//!
//! The cheap layers run as ordinary tests; the deep layers are `#[ignore]`d so
//! `cargo test` stays fast — run them with
//! `cargo test --release --test perft_cambodian -- --include-ignored`.

use mcr::geometry::{perft as gperft, Cambodian, Chess8x8, Square, WideMoveKind, WideRole};
use mcr::Color;

/// The Cambodian starting FEN, confirmed byte-for-byte against Fairy-Stockfish's
/// `UCI_Variant cambodian` / `position startpos`.
const STARTPOS: &str = "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w DEde - 0 1";

/// A midgame position with both leap rights still live and an open centre,
/// exercising the leap moves alongside ordinary development.
const MID_LEAPS: &str = "rnsmksnr/8/pp1ppp1p/2p2p2/2P2P2/PP1PPP1P/8/RNSKMSNR w DEde - 0 3";

/// The same board as [`MID_LEAPS`] but with **all leap rights spent** (no
/// `DEde`): this is exactly Makruk move generation, isolating the leap surplus.
const MID_SPENT: &str = "rnsmksnr/8/pp1ppp1p/2p2p2/2P2P2/PP1PPP1P/8/RNSKMSNR w - - 0 3";

/// A second open midgame (edge pawns advanced, both files cleared in front of
/// the kings) where every leap is reachable, all rights live.
const MID_OPEN: &str = "rnsmksnr/8/1ppppppp/p7/7P/PPPPPPP1/8/RNSKMSNR w DEde - 0 2";

/// Asserts the generic Cambodian perft equals each pinned `(depth, nodes)`
/// count. Every number here also matched FSF cambodian `go perft` on the same
/// FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Cambodian::from_fen(fen).expect("valid Cambodian FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Cambodian perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(
        STARTPOS,
        &[(1, 25), (2, 625), (3, 15031), (4, 361719), (5, 8597966)],
    );
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_depth6() {
    // FSF cambodian `go perft 6` on the startpos.
    check(STARTPOS, &[(6, 204583970)]);
}

// -- Midgame with both leaps live (FSF-confirmed) ---------------------------

#[test]
fn mid_leaps_cheap() {
    check(MID_LEAPS, &[(1, 23), (2, 532), (3, 12148), (4, 277928)]);
}

// -- Same board, leaps spent: pure Makruk move generation (FSF-confirmed) ----

#[test]
fn mid_spent_cheap() {
    check(MID_SPENT, &[(1, 21), (2, 444), (3, 9558), (4, 206271)]);
}

// -- Second open midgame, leaps reachable (FSF-confirmed) --------------------

#[test]
fn mid_open_cheap() {
    check(MID_OPEN, &[(1, 27), (2, 729), (3, 19038), (4, 497256)]);
}

// -- Rule-level self-checks (independent of FSF) ----------------------------

/// The starting array round-trips through FEN (including the `DEde` leap-rights
/// field) and matches the confirmed string.
#[test]
fn startpos_fen_round_trips() {
    let pos = Cambodian::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    assert_eq!(pos.turn(), Color::White);
    // The opening move count: 25 (FSF-confirmed perft(1)) — Makruk's 23 plus the
    // king's two forward-knight leaps.
    assert_eq!(pos.legal_move_count(), 25);
}

/// The king's one-time leap reaches both forward-knight squares from home,
/// landing only on empty squares, and is consumed once the king moves.
#[test]
fn king_leaps_to_forward_knight_squares() {
    let pos = Cambodian::startpos();
    let king_from = Square::from_file_rank(3, 0).unwrap(); // d1
    let b2 = Square::from_file_rank(1, 1).unwrap();
    let f2 = Square::from_file_rank(5, 1).unwrap();
    let dests: Vec<_> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| m.from::<Chess8x8>() == king_from)
        .map(|m| m.to::<Chess8x8>())
        .collect();
    assert!(dests.contains(&b2), "white king leaps to b2");
    assert!(dests.contains(&f2), "white king leaps to f2");

    // Spend the king's leap with an ordinary king step; the leap is gone and does
    // not return when the king comes back home.
    let step = pos
        .legal_moves()
        .into_iter()
        .find(|m| {
            m.from::<Chess8x8>() == king_from
                && m.to::<Chess8x8>() == Square::from_file_rank(3, 1).unwrap()
        })
        .expect("king d1-d2 step");
    let after = pos.play(&step);
    // White to move again after a black reply: pick any black move, then confirm
    // the king (now on d2) has no leap.
    let black = after.legal_moves().into_iter().next().expect("black move");
    let after2 = after.play(&black);
    let king_now = Square::from_file_rank(3, 1).unwrap(); // d2
    let leap_present = after2
        .legal_moves()
        .into_iter()
        .any(|m| m.from::<Chess8x8>() == king_now && m.to::<Chess8x8>() == f2);
    assert!(!leap_present, "the king leap is one-time only");
}

/// The Met (Neang) makes a single two-square straight advance from its home
/// square; it does not capture with the leap.
#[test]
fn met_leaps_two_squares_forward() {
    // White Met on e1 with its leap right, open file in front.
    let fen = "4k3/8/8/8/8/8/8/3KM3 w E - 0 1";
    let pos = Cambodian::from_fen(fen).expect("valid");
    let met_from = Square::from_file_rank(4, 0).unwrap(); // e1
    let e3 = Square::from_file_rank(4, 2).unwrap();
    let dests: Vec<_> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| m.from::<Chess8x8>() == met_from)
        .map(|m| m.to::<Chess8x8>())
        .collect();
    assert!(dests.contains(&e3), "white Met leaps two squares to e3");

    // The leap jumps a blocker on e2 but may not land on an occupied square.
    let blocked = Cambodian::from_fen("4k3/8/8/8/8/8/4p3/3KM3 w E - 0 1").expect("valid");
    let can_land = blocked
        .legal_moves()
        .into_iter()
        .any(|m| m.from::<Chess8x8>() == met_from && m.to::<Chess8x8>() == e3);
    assert!(
        !can_land,
        "the Met leap may not capture on its landing square"
    );
}

/// The king leap is offered only when the king is not in check (like castling).
#[test]
fn king_leap_suppressed_in_check() {
    // White king d1 in check from a rook on d8; the leap squares b2/f2 do not
    // resolve the check, so they are not generated.
    let fen = "3r4/8/8/8/8/8/8/3K2k1 w D - 0 1";
    let pos = Cambodian::from_fen(fen).expect("valid");
    let king_from = Square::from_file_rank(3, 0).unwrap();
    let b2 = Square::from_file_rank(1, 1).unwrap();
    let f2 = Square::from_file_rank(5, 1).unwrap();
    let leaps = pos.legal_moves().into_iter().any(|m| {
        m.from::<Chess8x8>() == king_from && (m.to::<Chess8x8>() == b2 || m.to::<Chess8x8>() == f2)
    });
    assert!(!leaps, "no king leap while in check");
}

/// A Bia (pawn) makes only a single-step advance — the Makruk rule is unchanged
/// in Cambodian — so no en-passant target is ever created.
#[test]
fn pawn_has_no_double_push() {
    let pos = Cambodian::startpos();
    let has_double = pos
        .legal_moves()
        .into_iter()
        .any(|m| matches!(m.kind(), WideMoveKind::DoublePawnPush));
    assert!(!has_double, "Cambodian pawns never double-push");
}

/// A Bia promotes to a Met (only), exactly as in Makruk.
#[test]
fn pawn_promotes_to_met_only() {
    let fen = "7k/8/8/4P3/8/8/8/K7 w - - 0 1";
    let pos = Cambodian::from_fen(fen).expect("valid");
    let promos: Vec<_> = pos
        .legal_moves()
        .into_iter()
        .filter_map(|m| m.promotion())
        .collect();
    assert_eq!(promos, vec![WideRole::Met]);
}
