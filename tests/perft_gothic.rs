//! Gothic Chess (10x8 / `u128`) perft validation on the generic engine — a
//! Capablanca-board variant with a different back-rank order.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mce::geometry::Gothic::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant gothic`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position. The `compare-fairy/` differential fuzzer re-runs the
//! head-to-head on demand (`--difffuzz --variant gothic`); this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! ```text
//! FSF dialect: rnbqckabnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNBQCKABNR w KQkq - 0 1
//! mce dialect: rnbqekabnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNBQEKABNR w KQkq - 0 1
//! ```
//!
//! The two differ only in the chancellor's letter (`c` in FSF, `e` in mce). The
//! deep layer is `#[ignore]`d so `cargo test` stays fast — run it with
//! `cargo test --release --test perft_gothic -- --include-ignored`.

use mce::geometry::{perft as gperft, Cap10x8, Gothic};

/// The Gothic starting FEN (mce dialect), confirmed against FSF's
/// `UCI_Variant gothic`.
const STARTPOS: &str = "rnbqekabnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNBQEKABNR w KQkq - 0 1";

/// A castling-rich position: the back ranks cleared between the king (f-file) and
/// both rooks (a/j files), so both sides may castle both ways.
const CASTLE: &str = "r4k3r/pppppppppp/10/10/10/10/PPPPPPPPPP/R4K3R w KQkq - 0 1";

fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Gothic::from_fen(fen).expect("valid Gothic FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Cap10x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Gothic perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 28), (2, 784), (3, 25283), (4, 808984)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 28946187)]);
}

#[test]
fn castle_cheap() {
    check(CASTLE, &[(1, 31), (2, 961), (3, 29210), (4, 887784)]);
}
