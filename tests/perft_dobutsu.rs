//! Dobutsu (3x4 animal shogi / `u64`) perft validation on the generic engine
//! (issue #233) — a tiny educational shogi variant reusing the Shogi (#190) /
//! Minishogi (#195) persistent capture-fed **hand**, **drops**, and far-rank
//! **promotion** machinery on a three-by-four (12-square) board, with a
//! **non-royal Lion** (loss by extinction, no check) and a **try** (flag) win:
//! the Lion wins by reaching the far rank *and being safe there*.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Dobutsu::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant dobutsu`) running `go perft` on the byte-identical position. The
//! `compare-fairy/` harness re-runs that head-to-head on demand
//! (`compare-fairy/src/dobutsu.rs`); this test pins the FSF-confirmed numbers so a
//! regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `dobutsu` start (`position startpos`):
//!
//! ```text
//! gle/1c1/1C1/ELG[-] w 0 1
//! ```
//!
//! mcr renders the same position with an empty `[]` holdings bracket. Its piece
//! letters differ from FSF's — the Lion is a King (`k`), the Chick a Pawn (`p`),
//! the Elephant a Met (`m`), and the Giraffe the Wazir overflow role (`*j`) — so
//! the placement is `*jkm/1p1/1P1/MK*J`; the `compare-fairy/` harness translates
//! the letters when driving FSF. The FSF-confirmed startpos perft sequence is
//! `4, 17, 123, 976, 8122, 71677, 643982, 5866031`.
//!
//! ## Pieces and rules pinned here
//!
//! * **Forced Chick promotion** (`forced_promo`): a Chick reaching the far rank is
//!   forced to promote to a Hen (it would otherwise be immobile).
//! * **Drops** (`drops`, `multi_hand`): a captured piece is banked unpromoted and
//!   may be dropped on **any** empty square — a Chick may be dropped on the last
//!   rank (no dead-piece rule) and there is **no nifu**.
//! * **Try / flag win** (`try_advance`): a Lion advancing toward the far rank with
//!   the enemy Lion contesting it — the deep tree exercises the safe-try rule (a
//!   Lion on the far rank wins only when unattacked).
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_dobutsu -- --include-ignored`.

use mcr::geometry::{perft as gperft, Dobutsu, Dobutsu3x4};

/// The Dobutsu starting FEN, confirmed against Fairy-Stockfish's
/// `UCI_Variant dobutsu`. The hand is empty (`[]`). mcr's letters: Lion `k`,
/// Chick `p`, Elephant `m`, Giraffe `*j`.
const STARTPOS: &str = "*jkm/1p1/1P1/MK*J[] w - - 0 1";

/// The start with the Chicks exchanged off the board into the hand (FSF
/// `gle/3/1C1/ELG[c]`, black holding a Chick): drops appear immediately and the
/// branching factor jumps. FSF-confirmed.
const DROPS: &str = "*jkm/3/1P1/MK*J[p] w - - 0 1";

/// Bare Lions with **one of every droppable role in each hand** (Elephant `m`,
/// Giraffe `*j`, Chick `p`), white to move: drops *dominate* the move set (35
/// legal moves at depth 1), stressing the no-restriction drop rule and the
/// non-royal Lion across the whole board. FSF `1l1/3/3/1L1[EGCegc]`. FSF-confirmed.
const MULTI_HAND: &str = "1k1/3/3/1K1[M*JPm*jp] w - - 0 1";

/// A lone white Chick on b3, one step from the far rank, Lions clear, white to
/// move: the push to b4 is **forced** to promote to a Hen (it would otherwise have
/// no further move). FSF `1l1/1C1/3/1L1[]`. FSF-confirmed.
const FORCED_PROMO: &str = "1k1/1P1/3/1K1[] w - - 0 1";

/// A white Lion on b2 advancing toward its far rank with the black Lion on b4
/// contesting it, both sides holding an Elephant/Giraffe/Chick: the deep tree
/// exercises the **safe-try** flag-win rule (a Lion reaching the far rank wins
/// only when the opponent cannot capture it there). FSF `1l1/3/1L1/3[EGC]`.
/// FSF-confirmed.
const TRY_ADVANCE: &str = "1k1/3/1K1/3[M*JP] w - - 0 1";

/// Asserts the generic Dobutsu perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `dobutsu` `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Dobutsu::from_fen(fen).expect("valid Dobutsu FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Dobutsu3x4, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Dobutsu perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) ------------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 4), (2, 17), (3, 123), (4, 976)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(
        STARTPOS,
        &[(5, 8122), (6, 71677), (7, 643982), (8, 5866031)],
    );
}

// -- Drops from the hand (FSF-confirmed) -------------------------------------

#[test]
fn drops_cheap() {
    check(DROPS, &[(1, 4), (2, 40), (3, 218), (4, 1705)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn drops_deep() {
    check(DROPS, &[(5, 10764), (6, 91091)]);
}

// -- Multi-hand: drops dominate (FSF-confirmed) ------------------------------

#[test]
fn multi_hand_cheap() {
    check(MULTI_HAND, &[(1, 35), (2, 1135), (3, 27255)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn multi_hand_deep() {
    check(MULTI_HAND, &[(4, 612534), (5, 10521669)]);
}

// -- Forced Chick promotion (FSF-confirmed) ----------------------------------

#[test]
fn forced_promo() {
    check(FORCED_PROMO, &[(1, 6), (2, 25), (3, 128), (4, 774)]);
}

// -- Try / safe flag win (FSF-confirmed) -------------------------------------

#[test]
fn try_advance_cheap() {
    check(TRY_ADVANCE, &[(1, 38), (2, 190), (3, 5302)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn try_advance_deep() {
    check(TRY_ADVANCE, &[(4, 28376), (5, 530596), (6, 3533203)]);
}
