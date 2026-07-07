//! Shatranj (medieval chess) perft validation on the generic engine (issue #262).
//!
//! The node counts below are pinned **and cross-checked against Fairy-Stockfish**
//! (FSF, `UCI_Variant shatranj`, a built-in): every `(depth, nodes)` pair here
//! was produced identically by `mcr::geometry::Shatranj::perft` and by FSF's `go
//! perft` on the byte-identical position. The `compare-fairy/` harness re-runs
//! that head-to-head on demand (see `compare-fairy/src/shatranj.rs`); this test
//! pins the confirmed numbers so a regression is caught without FSF present.
//!
//! ## FEN dialect
//!
//! mcr and FSF render the same position with different piece letters. FSF's
//! `shatranj` uses `b` for the Alfil (elephant) and `q` for the Ferz (counselor);
//! mcr reuses `b`/`q` for its Bishop/Queen, so the Ferz takes the Makruk Met `m`
//! and the Alfil — past the exhausted single-letter alphabet — the `*`-prefixed
//! overflow token `*x`. The harness rewrites these (`*x → b`, `m → q`) when
//! driving FSF.
//!
//! Confirmed Shatranj starting FEN (from FSF `position startpos`, mcr dialect):
//!   `rn*xkm*xnr/pppppppp/8/8/8/8/PPPPPPPP/RN*XKM*XNR w - - 0 1`
//!
//! The cheap layers run as ordinary tests; the deep layers are `#[ignore]`d so
//! `cargo test` stays fast — run them with
//! `cargo test --release --test perft_shatranj -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Shatranj};

/// The Shatranj starting FEN (mcr dialect), confirmed byte-for-byte against
/// Fairy-Stockfish's `UCI_Variant shatranj` / `position startpos`.
const STARTPOS: &str = "rn*xkm*xnr/pppppppp/8/8/8/8/PPPPPPPP/RN*XKM*XNR w - - 0 1";

/// A midgame with all four Alfils developed off the back rank (white on d3/e3,
/// black on d6/e6), full material — exercises the two-diagonal Alfil jump.
const MID1: &str = "rn1km1nr/pppppppp/3*x*x3/8/8/3*X*X3/PPPPPPPP/RN1KM1NR w - - 4 3";

/// A midgame with both Ferzes advanced (white d2, black d7), a developed knight
/// and Alfil each, and an asymmetric centre — exercises the Ferz one-diagonal
/// step alongside the standard pieces.
const MID2: &str = "r1*xk1*x1r/pppmpppp/2np1n2/8/8/2NPP3/PPPM1PPP/R1*XK1*XNR w - - 3 5";

/// A bared-king endgame: Black is reduced to its lone king while White keeps six
/// pieces, so the node is a baring loss — terminal, zero continuations — exactly
/// as FSF truncates it (`go perft` returns 0 at any depth).
const BARED: &str = "4k3/8/8/2P1P3/3*X4/2P1P3/8/4K3 w - - 0 1";

/// Asserts the generic Shatranj perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF shatranj `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Shatranj::from_fen(fen).expect("valid Shatranj FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Shatranj perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    // No double pawn push: depth 1 is 8 single pawn pushes + 4 knight moves + 4
    // Alfil jumps (c1→a3/e3, f1→d3/h3) = 16.
    check(
        STARTPOS,
        &[(1, 16), (2, 256), (3, 4176), (4, 68122), (5, 1164248)],
    );
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    // FSF shatranj `go perft` on the startpos.
    check(STARTPOS, &[(6, 19864709), (7, 357218656)]);
}

// -- Midgames (FSF-confirmed) -----------------------------------------------

#[test]
fn mid1_alfils_cheap() {
    check(MID1, &[(1, 17), (2, 289), (3, 5221), (4, 94476)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn mid1_alfils_deep() {
    check(MID1, &[(5, 1784516)]);
}

#[test]
fn mid2_ferzes_cheap() {
    check(MID2, &[(1, 22), (2, 549), (3, 12124), (4, 304365)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn mid2_ferzes_deep() {
    check(MID2, &[(5, 6747793)]);
}

// -- Baring truncation (FSF-confirmed) --------------------------------------

#[test]
fn bared_king_truncates_to_zero() {
    // The bared side has lost: FSF reports the node terminal, so `go perft N` is 0
    // at every depth. mcr's baring-loss truncation matches exactly.
    check(BARED, &[(1, 0), (2, 0), (3, 0), (4, 0)]);
}
