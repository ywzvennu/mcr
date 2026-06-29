//! Xiang Fu (9x9 Xiangqi-themed drop variant) perft validation on the generic
//! engine (issue #274) — the first variant to combine the **multi-royal
//! pseudo-royal** path with a **crazyhouse-style hand**: a side fields two
//! ring-confined royal **Champions** under FSF's combined `extinctionPseudoRoyal` +
//! `dupleCheck` rule (a move may not leave any Champion en prise unless it captures
//! an enemy Champion, which relaxes the rule to "not *both* attacked"), captured
//! pieces bank **to hand** and drop onto the first two ranks of their own side, and
//! a side that has lost **both** Champions keeps generating moves (FSF never
//! truncates that node).
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mce::geometry::Xiangfu` perft and by Fairy-Stockfish (FSF, `UCI_Variant
//! xiangfu`, loaded from a `variants.ini` defining the pychess `[xiangfu]` variant)
//! running `go perft` on the byte-identical position. The `compare-fairy/` harness
//! re-runs that head-to-head on demand (`compare-fairy/src/xiangfu.rs`); this test
//! pins the FSF-confirmed numbers so a regression is caught even without FSF
//! present.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant xiangfu`, `position startpos`) renders the start as
//!
//! ```text
//! 2rbm4/2cwn4/2+g1+g4/9/9/9/4+G1+G2/4NWC2/4MBR2[] w - 0 1
//! ```
//!
//! with FSF's letters `r b m c w n` (Chariot, Bishop, Mahout, Cannon, Crossbow,
//! Horse) and the promoted commoner `+g` (Champion). mce reuses `r`/`b`/`c`
//! (Rook / Bishop / Cannon) and spells the Horse `j`, the Crossbow `=c`
//! (BishopCannon), the Mahout `=m`, and the Champion `=k`; its canonical start FEN
//! is
//!
//! ```text
//! 2rb=m4/2c=cj4/2=k1=k4/9/9/9/4=K1=K2/4J=CC2/4=MBR2[] w - - 0 1
//! ```
//!
//! `compare-fairy/` translates the tokens when driving FSF (`=k → +g`, `=m → m`,
//! `j → n`, `=c → w`, and the hand Pupil `*u → g`).
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_xiangfu -- --include-ignored`.

use mce::geometry::{perft as gperft, Shogi9x9, Xiangfu};

/// The Xiang Fu starting FEN in mce's dialect, confirmed against FSF's
/// `UCI_Variant xiangfu` / `position startpos`.
const STARTPOS: &str = "2rb=m4/2c=cj4/2=k1=k4/9/9/9/4=K1=K2/4J=CC2/4=MBR2[] w - - 0 1";

/// A developed middlegame (Black to move): White's e2 Horse has leapt to c3 and its
/// g2 Cannon to c2, Black's c8 Cannon has slid to c4 and its d8 Crossbow to b6 —
/// exercising the Horse leap, both cannon families' over-screen geometry, and an
/// asymmetric tempo. (FSF `2rbm4/2cwn4/2+g1+g4/9/9/9/2N1+G1+G2/5WC2/4MBR2 b`.)
const MIDGAME_A: &str = "2rb=m4/2c=cj4/2=k1=k4/9/9/9/2J1=K1=K2/5=CC2/4=MBR2[] b - - 2 1";

/// A drops position: each side holds a captured **Pupil** in hand (mce `*U`/`*u`,
/// FSF `G`/`g`), exercising the captures-to-hand drop generation onto the first two
/// ranks under the pseudo-royal legality filter. (FSF
/// `2rbm4/2cwn4/2+g1+g4/9/9/9/4+G1+G2/4NWC2/4MBR2[Gg] w`.)
const DROPS: &str = "2rb=m4/2c=cj4/2=k1=k4/9/9/9/4=K1=K2/4J=CC2/4=MBR2[*U*u] w - - 0 1";

/// A **duple-check** position: each side's Champions have advanced so a White
/// Champion (e4) and a Black Champion (e6) stand one square apart, and a White
/// Champion may legally step to a square the enemy Champion attacks (e4→{d5,e5,f5})
/// because its *other* Champion stays safe — exercising the at-least-one
/// (duple-check) legality. (FSF
/// `2rbm4/2cwn4/2+g6/4+g4/9/4+G4/6+G2/4NWC2/4MBR2 w`.)
const DUPLE: &str = "2rb=m4/2c=cj4/2=k6/4=k4/9/4=K4/6=K2/4J=CC2/4=MBR2[] w - - 3 2";

/// Asserts the generic Xiang Fu perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `xiangfu` `go perft` on the same FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Xiangfu::from_fen(fen).expect("valid Xiang Fu FEN");
    // The FEN round-trips through mce's overflow-token + hand I/O.
    assert_eq!(pos.to_fen(), fen, "Xiang Fu FEN round-trips: {fen}");
    for &(depth, expected) in cases {
        let got = gperft::<Shogi9x9, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Xiang Fu perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 16), (2, 260), (3, 6276), (4, 152394)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 4449493), (6, 130537876)]);
}

// -- Middlegame A (Horse + cannon geometry) ---------------------------------

#[test]
fn midgame_a_cheap() {
    check(MIDGAME_A, &[(1, 17), (2, 439), (3, 10727), (4, 314847)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_a_deep() {
    check(MIDGAME_A, &[(5, 9293321)]);
}

// -- Drops (captures-to-hand) -----------------------------------------------

#[test]
fn drops_cheap() {
    check(DROPS, &[(1, 28), (2, 790), (3, 23168)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn drops_deep() {
    check(DROPS, &[(4, 683150)]);
}

// -- Duple check (Champions adjacent) ---------------------------------------

#[test]
fn duple_cheap() {
    check(DUPLE, &[(1, 26), (2, 678), (3, 20013)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn duple_deep() {
    check(DUPLE, &[(4, 589879)]);
}
