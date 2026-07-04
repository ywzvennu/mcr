//! Embassy Chess (10x8 / `u128`) perft validation on the generic engine — a
//! Capablanca-board variant with the king on the e-file and its own castle files.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Embassy::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant embassy`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position. The `compare-fairy/` differential fuzzer re-runs the
//! head-to-head on demand (`--difffuzz --variant embassy`); this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! ```text
//! FSF dialect: rnbqkcabnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNBQKCABNR w KQkq - 0 1
//! mcr dialect: rnbqkeabnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNBQKEABNR w KQkq - 0 1
//! ```
//!
//! The two differ only in the chancellor's letter (`c` in FSF, `e` in mcr). The
//! deep layer is `#[ignore]`d so `cargo test` stays fast — run it with
//! `cargo test --release --test perft_embassy -- --include-ignored`.

use mcr::geometry::{perft as gperft, Cap10x8, Embassy};

/// The Embassy starting FEN (mcr dialect), confirmed against FSF's
/// `UCI_Variant embassy`.
const STARTPOS: &str = "rnbqkeabnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNBQKEABNR w KQkq - 0 1";

/// A castling-rich position: the back ranks cleared between the king (e-file) and
/// both rooks (a/j files), pinning the Embassy castle geometry (king e -> h/b).
const CASTLE: &str = "r3k4r/pppppppppp/10/10/10/10/PPPPPPPPPP/R3K4R w KQkq - 0 1";

fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Embassy::from_fen(fen).expect("valid Embassy FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Cap10x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Embassy perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 28), (2, 784), (3, 25281), (4, 809539)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 28937546)]);
}

#[test]
fn castle_cheap() {
    check(CASTLE, &[(1, 31), (2, 961), (3, 29210), (4, 887784)]);
}
