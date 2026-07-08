//! Atomar perft validation on the generic engine.
//!
//! Atomar is an 8x8 Fairy-Stockfish built-in (`UCI_Variant atomar`): nocheckatomic
//! with two Commoner immunities — Commoners are **blast-immune** (they survive an
//! adjacent explosion, and a capturing Commoner survives its own blast) and
//! **mutually immune** (a Commoner may never capture the enemy Commoner). Every
//! `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Atomar::perft` and by FSF running `go perft` on the byte-identical
//! position; the `compare-fairy/` harness re-runs that head-to-head on demand
//! (`compare-fairy/src/atomar.rs`), and this test pins the FSF-confirmed numbers so a
//! regression is caught even without FSF present.
//!
//! ## What the corpus exercises
//!
//! * [`STARTPOS`] — like nocheckatomic through depth 4 (the Commoners are far from
//!   any capture); the immunities begin to diverge the tree deeper.
//! * [`KIWI`] / [`ITAL`] — tactical middlegames whose blasts near the Commoners
//!   diverge from nocheckatomic (the blast-immune Commoner changes the resulting
//!   board, so the counts rise above nocheckatomic's from depth 2).
//! * [`BLAST`] — a symmetric piece cluster stressing the blast with immune Commoners
//!   in the mix.
//! * [`ADJ`] — the two Commoners standing adjacent: **mutual immunity** removes the
//!   Commoner-takes-Commoner move, so perft(1) is one lower than nocheckatomic (7 vs
//!   8).
//! * [`IMM`] — the white Commoner beside enemy rooks: **blast immunity** lets it
//!   survive captures that would blow up a nocheckatomic Commoner, so the counts
//!   diverge upward from depth 2.
//!
//! ## FEN dialect
//!
//! Atomar uses only **standard chess pieces** (`K Q R B N P` — the king is a Commoner
//! by rule, not by letter), identical in mcr and FSF, so the FEN is passed to FSF
//! unchanged.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_atomar -- --include-ignored`.

use mcr::geometry::{perft as gperft, Atomar, Chess8x8};

/// The Atomar starting FEN, confirmed against FSF — the standard array.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// The "Kiwipete" tactical middlegame — castling both sides and many captures, whose
/// blasts near the Commoners diverge from nocheckatomic.
const KIWI: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

/// An Italian-opening middlegame with live central captures.
const ITAL: &str = "r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4";

/// A symmetric piece cluster where captures repeatedly detonate 3x3 neighbourhoods.
const BLAST: &str = "r2qkb1r/ppp2ppp/2n2n2/3pp1B1/3PP1b1/2N2N2/PPP2PPP/R2QKB1R w KQkq - 0 1";

/// The two Commoners standing adjacent — **mutual immunity** forbids the
/// Commoner-takes-Commoner capture, so perft(1) = 7 (nocheckatomic = 8).
const ADJ: &str = "8/8/8/4k3/4K3/8/8/8 w - - 0 1";

/// The white Commoner beside enemy rooks — **blast immunity** lets it survive
/// captures that blow up a nocheckatomic Commoner, diverging upward from depth 2.
const IMM: &str = "4k3/8/3r4/2rK4/8/8/8/8 w - - 0 1";

/// `(depth, nodes)` rows confirmed identical between mcr and FSF.
struct Perft {
    fen: &'static str,
    rows: &'static [(u32, u64)],
}

const STARTPOS_PERFT: Perft = Perft {
    fen: STARTPOS,
    rows: &[(1, 20), (2, 400), (3, 8902), (4, 197_779), (5, 4_895_732)],
};

const KIWI_PERFT: Perft = Perft {
    fen: KIWI,
    rows: &[(1, 48), (2, 2019), (3, 93_221), (4, 3_813_425)],
};

const ITAL_PERFT: Perft = Perft {
    fen: ITAL,
    rows: &[(1, 33), (2, 1017), (3, 33_284), (4, 1_038_665)],
};

const BLAST_PERFT: Perft = Perft {
    fen: BLAST,
    rows: &[(1, 40), (2, 1613), (3, 64_182), (4, 2_563_458)],
};

const ADJ_PERFT: Perft = Perft {
    fen: ADJ,
    rows: &[(1, 7), (2, 52), (3, 397), (4, 3064)],
};

const IMM_PERFT: Perft = Perft {
    fen: IMM,
    rows: &[(1, 8), (2, 192), (3, 1482), (4, 39_924)],
};

fn check(p: &Perft, max_depth: u32) {
    let pos = Atomar::from_fen(p.fen).expect("Atomar FEN parses");
    for &(depth, nodes) in p.rows {
        if depth > max_depth {
            continue;
        }
        assert_eq!(
            gperft::<Chess8x8, _, _>(&pos, depth),
            nodes,
            "Atomar perft depth {depth} for FEN {}",
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
fn adjacent_commoners_matches_fsf() {
    check(&ADJ_PERFT, 4);
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
