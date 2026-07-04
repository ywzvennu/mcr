//! Janus Chess (10x8 / `u128`) perft validation on the generic engine — a
//! Capablanca-board variant with two Januses (Bishop + Knight) per side and no
//! Chancellor.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Janus::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant janus`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position. The `compare-fairy/` differential fuzzer re-runs the
//! head-to-head on demand (`--difffuzz --variant janus`); this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! ```text
//! FSF dialect: rjnbkqbnjr/pppppppppp/10/10/10/10/PPPPPPPPPP/RJNBKQBNJR w KQkq - 0 1
//! mcr dialect: ranbkqbnar/pppppppppp/10/10/10/10/PPPPPPPPPP/RANBKQBNAR w KQkq - 0 1
//! ```
//!
//! The two differ only in the Janus's letter (`j` in FSF, `a` in mcr, mcr's
//! [`WideRole::Hawk`] bishop-knight compound). The deep layer is `#[ignore]`d so
//! `cargo test` stays fast — run it with
//! `cargo test --release --test perft_janus -- --include-ignored`.

use mcr::geometry::{perft as gperft, Cap10x8, Janus};

/// The Janus starting FEN (mcr dialect), confirmed against FSF's
/// `UCI_Variant janus`.
const STARTPOS: &str = "ranbkqbnar/pppppppppp/10/10/10/10/PPPPPPPPPP/RANBKQBNAR w KQkq - 0 1";

/// A castling-rich position: the back ranks cleared between the king (e-file) and
/// both rooks (a/j files), pinning the Janus castle geometry (king e -> i/b).
const CASTLE: &str = "r3k4r/pppppppppp/10/10/10/10/PPPPPPPPPP/R3K4R w KQkq - 0 1";

fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Janus::from_fen(fen).expect("valid Janus FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Cap10x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Janus perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 28), (2, 782), (3, 24747), (4, 772074)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 26869186)]);
}

#[test]
fn castle_cheap() {
    check(CASTLE, &[(1, 31), (2, 961), (3, 29272), (4, 891556)]);
}
