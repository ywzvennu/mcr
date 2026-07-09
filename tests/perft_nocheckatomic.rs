//! Nocheckatomic perft validation on the generic engine.
//!
//! Nocheckatomic is an 8x8 Fairy-Stockfish built-in (`UCI_Variant nocheckatomic`):
//! atomic chess with a non-royal Commoner king — every capture detonates, and a
//! side loses by having its Commoner captured or blown up (there is no check). Every
//! `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Nocheckatomic::perft` and by FSF running `go perft` on the
//! byte-identical position; the `compare-fairy/` harness re-runs that head-to-head on
//! demand (`compare-fairy/src/nocheckatomic.rs`), and this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## What the corpus exercises
//!
//! * [`STARTPOS`] — the no-check Commoner king lifts the tree above standard chess
//!   from depth 4 (perft(4) = `197779`, chess = `197281`), while the first
//!   capture-blasts begin to truncate it at depth 5.
//! * [`KIWI`] — the "Kiwipete" tactical middlegame (castling both sides, many
//!   captures) drives the blast on a dense board.
//! * [`ITAL`] — an Italian-opening middlegame with live central captures.
//! * [`BLAST`] — a symmetric piece cluster where captures repeatedly blow up 3x3
//!   neighbourhoods of pieces.
//! * [`IMM`] — the white Commoner sitting next to enemy rooks it can capture (and be
//!   blasted with): the position where nocheckatomic and atomar diverge, since here
//!   the Commoner is *not* blast-immune.
//!
//! ## FEN dialect
//!
//! Nocheckatomic uses only **standard chess pieces** (`K Q R B N P` — the king is a
//! Commoner by rule, not by letter), identical in mcr and FSF, so the FEN is passed
//! to FSF unchanged.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_nocheckatomic -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Nocheckatomic};

/// The Nocheckatomic starting FEN, confirmed against FSF — the standard array.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// The "Kiwipete" tactical middlegame — castling both sides and many captures.
const KIWI: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

/// An Italian-opening middlegame with live central captures.
const ITAL: &str = "r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4";

/// A symmetric piece cluster where captures repeatedly detonate 3x3 neighbourhoods.
const BLAST: &str = "r2qkb1r/ppp2ppp/2n2n2/3pp1B1/3PP1b1/2N2N2/PPP2PPP/R2QKB1R w KQkq - 0 1";

/// The white Commoner beside enemy rooks it may capture — where nocheckatomic and
/// atomar diverge (the nocheckatomic Commoner is not blast-immune).
const IMM: &str = "4k3/8/3r4/2rK4/8/8/8/8 w - - 0 1";

/// `(depth, nodes)` rows confirmed identical between mcr and FSF.
struct Perft {
    fen: &'static str,
    rows: &'static [(u32, u64)],
}

const STARTPOS_PERFT: Perft = Perft {
    fen: STARTPOS,
    rows: &[(1, 20), (2, 400), (3, 8902), (4, 197_779), (5, 4_895_665)],
};

const KIWI_PERFT: Perft = Perft {
    fen: KIWI,
    rows: &[(1, 48), (2, 1942), (3, 88_692), (4, 3_525_246)],
};

const ITAL_PERFT: Perft = Perft {
    fen: ITAL,
    rows: &[(1, 33), (2, 991), (3, 32_636), (4, 997_426)],
};

const BLAST_PERFT: Perft = Perft {
    fen: BLAST,
    rows: &[(1, 40), (2, 1613), (3, 63_989), (4, 2_547_301)],
};

const IMM_PERFT: Perft = Perft {
    fen: IMM,
    rows: &[(1, 8), (2, 182), (3, 1408), (4, 37_959)],
};

fn check(p: &Perft, max_depth: u32) {
    let pos = Nocheckatomic::from_fen(p.fen).expect("Nocheckatomic FEN parses");
    for &(depth, nodes) in p.rows {
        if depth > max_depth {
            continue;
        }
        assert_eq!(
            gperft::<Chess8x8, _, _>(&pos, depth),
            nodes,
            "Nocheckatomic perft depth {depth} for FEN {}",
            p.fen,
        );
    }
}

#[test]
fn startpos_shallow_matches_fsf() {
    check(&STARTPOS_PERFT, 4);
}

#[test]
fn kiwi_shallow_matches_fsf() {
    check(&KIWI_PERFT, 3);
}

#[test]
fn italian_shallow_matches_fsf() {
    check(&ITAL_PERFT, 3);
}

#[test]
fn blast_shallow_matches_fsf() {
    check(&BLAST_PERFT, 3);
}

#[test]
fn immune_divergence_shallow_matches_fsf() {
    check(&IMM_PERFT, 4);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn startpos_deep_matches_fsf() {
    check(&STARTPOS_PERFT, 5);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn kiwi_deep_matches_fsf() {
    check(&KIWI_PERFT, 4);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn italian_deep_matches_fsf() {
    check(&ITAL_PERFT, 4);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn blast_deep_matches_fsf() {
    check(&BLAST_PERFT, 4);
}
