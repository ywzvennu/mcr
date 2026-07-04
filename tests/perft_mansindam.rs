//! Mansindam (만신담, "Pantheon tale", 9x9) perft validation on the generic engine
//! (issue #271) — a shogi-chess hybrid on the [`Shogi9x9`](mcr::geometry::Shogi9x9)
//! geometry with the full crazyhouse **captures-to-hand and drops**, a
//! **mandatory** far-three-ranks promotion zone (a promotable piece whose move
//! starts *or* ends in the zone must upgrade), three new pieces (the Angel =
//! Queen-plus-Knight, the promoted Rhino = Bishop-Knight-Wazir, and the promoted
//! Ship = Rook-Knight-Ferz), and the **campmate** flag win (a King reaching the
//! opponent's back rank ends the game).
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Mansindam` perft and by Fairy-Stockfish (FSF,
//! `UCI_Variant mansindam`, from its `variants.ini`) running `go perft` on the
//! byte-identical position. The `compare-fairy/` harness re-runs that head-to-head
//! on demand (`compare-fairy/src/mansindam.rs`); this test pins the FSF-confirmed
//! numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant mansindam`, `position startpos`) renders the start as
//!
//! ```text
//! rnbakqcnm/9/ppppppppp/9/9/9/PPPPPPPPP/9/MNCQKABNR[] w - - 0 1
//! ```
//!
//! with FSF's letters `a` (Angel/amazon), `c` (Cardinal/archbishop) and `m`
//! (Marshal/chancellor). mcr reuses `a` (Hawk = Cardinal) and `e` (Elephant =
//! Marshal) and spells the Angel with the second-bank overflow token `**a`, so its
//! canonical start FEN (with the empty crazyhouse hand bracket) is
//!
//! ```text
//! rnb**akqane/9/ppppppppp/9/9/9/PPPPPPPPP/9/ENAQK**ABNR[] w - - 0 1
//! ```
//!
//! The two are the same position (the back ranks are a 180° rotation, not a
//! mirror); `compare-fairy/` translates the tokens when driving FSF.
//!
//! The deep startpos layer is `#[ignore]`d so `cargo test` stays fast — run it with
//! `cargo test --release --test perft_mansindam -- --include-ignored`.

use mcr::geometry::{perft as gperft, Mansindam, Shogi9x9};

/// The Mansindam starting FEN in mcr's dialect, confirmed against FSF's
/// `UCI_Variant mansindam` / `position startpos`.
const STARTPOS: &str = "rnb**akqane/9/ppppppppp/9/9/9/PPPPPPPPP/9/ENAQK**ABNR[] w - - 0 1";

/// After 1. e4 e5 (each side's king pawn one square forward): a quiet opening
/// position with the back ranks intact, exercising ordinary development.
const E4E5: &str = "rnb**akqane/9/pppp1pppp/4p4/9/4P4/PPPP1PPPP/9/ENAQK**ABNR[] w - - 2 2";

/// Both sides holding a Knight, Bishop and Rook **in hand** with bare kings — a
/// drop swarm exercising the full drop generator (every empty square is a target
/// for every held piece, no last-rank/nifu filter for the non-pawns).
const DROPS: &str = "4k4/9/9/9/9/9/9/9/4K4[NBRnbr] w - - 0 1";

/// A White Pawn on e5 with a Pawn **in hand** for each side: the *nifu* filter
/// (FSF `dropNoDoubled = p`) forbids dropping White's held Pawn onto the e-file,
/// and the last-rank ban forbids dropping it on rank 9.
const NIFU: &str = "4k4/9/9/9/4P4/9/9/9/4K4[Pp] w - - 0 1";

/// A White Cardinal (a7) and Marshal (i7) standing **in the promotion zone**: every
/// one of their moves starts in the zone, so each is a *mandatory* promotion (to a
/// Rhino / Ship), exercising the origin-in-zone forced upgrade.
const PROMO_ZONE: &str = "4k4/9/A7E/9/9/9/9/9/4K4[] w - - 0 1";

/// A clean flag race: White's King on d8 is one step from its goal rank 9, Black's
/// King on f2 one step from its goal rank 1 — each King move onto the far rank is a
/// campmate that **truncates** that subtree to a perft leaf.
const FLAG_RACE: &str = "9/3K5/9/9/9/9/9/5k3/9[] w - - 0 1";

/// A board full of **promoted** pieces — Archer (`+B`) on c7, Tiger (`+R`) on e7,
/// Rhino (`**I`) on g7, Ship (`**S`) on c5, Centaur (`W`) on e5 and Guard (`*U`) on
/// g5 — with a Knight in hand for each side, exercising every promoted mover's
/// geometry at once. Black's King is tucked on a8 (off all of those pieces' lines).
const PROMOTED: &str = "9/k8/2+B1+R1**I2/9/2**S1W1*U2/9/9/9/4K4[Nn] w - - 0 1";

/// A lone **Angel** (Queen + Knight) on e5 with Black's King tucked on b9 (off all
/// of the Angel's lines), exercising the new compound's full movement.
const ANGEL: &str = "1k7/9/9/9/4**A4/9/9/9/4K4[] w - - 0 1";

/// A real **capture-to-hand** midgame: after 1. e4 e5 2. e5 d6 3. exd6-style play
/// White has captured a Pawn into its hand (`[P]`), Black to move — the captured
/// piece is now droppable, exercising the bank-and-redrop loop down the tree.
const CAPTURE_SEQ: &str = "rnb**akqane/9/ppp2pppp/3pP4/9/9/PPPP1PPPP/9/ENAQK**ABNR[P] b - - 0 3";

/// Asserts the generic Mansindam perft equals each pinned `(depth, nodes)` count,
/// and that the FEN round-trips through mcr's overflow-token + hand I/O. Every
/// number here also matched FSF mansindam `go perft` on the same FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Mansindam::from_fen(fen).expect("valid Mansindam FEN");
    assert_eq!(pos.to_fen(), fen, "Mansindam FEN round-trips: {fen}");
    for &(depth, expected) in cases {
        let got = gperft::<Shogi9x9, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Mansindam perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 31), (2, 961), (3, 32238), (4, 1081374)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 38109695)]);
}

#[test]
fn e4e5_cheap() {
    check(E4E5, &[(1, 42), (2, 1758), (3, 75941), (4, 3278693)]);
}

#[test]
fn drops_cheap() {
    check(DROPS, &[(1, 242), (2, 51524), (3, 7839499)]);
}

#[test]
fn nifu_cheap() {
    check(NIFU, &[(1, 70), (2, 5187), (3, 63334)]);
}

#[test]
fn promo_zone_cheap() {
    check(PROMO_ZONE, &[(1, 36), (2, 132), (3, 5420), (4, 21410)]);
}

#[test]
fn flag_race_cheap() {
    check(FLAG_RACE, &[(1, 8), (2, 40), (3, 200), (4, 1360)]);
}

#[test]
fn promoted_cheap() {
    check(PROMOTED, &[(1, 161), (2, 10979), (3, 1331492)]);
}

#[test]
fn angel_cheap() {
    check(ANGEL, &[(1, 44), (2, 145), (3, 5648), (4, 22423)]);
}

#[test]
fn capture_seq_cheap() {
    check(CAPTURE_SEQ, &[(1, 42), (2, 1765), (3, 77346), (4, 3385850)]);
}

#[test]
fn startpos_fen_round_trips() {
    let pos = Mansindam::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
}
