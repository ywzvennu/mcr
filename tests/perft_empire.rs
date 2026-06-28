//! Empire (8x8) perft validation on the generic engine (issue #221) — the variant
//! exercising an **asymmetric Roman "Empire" army** of long-range "move-Queen /
//! capture-short" pieces against a standard chess Black army, plus the **flag-win
//! (campmate)** terminal rule and the broadened **flying-general** king faceoff.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mce::geometry::Empire` perft and by Fairy-Stockfish (FSF, `UCI_Variant empire`,
//! from its `variants.ini`) running `go perft` on the byte-identical position — the
//! FSF divide matches mce's move-for-move, including each Empire piece's
//! Queen-move / short-capture split (the Eagle captures like a knight, the Cardinal
//! like a bishop, the Tower like a rook, the Duke like a king, and all four *move*
//! like a queen onto empty squares), the forward/sideways Soldier, Queen-only pawn
//! promotion, the flag win (a king on its goal rank ends the game, terminating
//! perft descent), and the file+rank flying-general king faceoff. The
//! `compare-fairy/` harness re-runs that head-to-head on demand
//! (`compare-fairy/src/empire.rs`); this test pins the FSF-confirmed numbers so a
//! regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant empire`, `position startpos`) renders the start as
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/PPPSSPPP/8/TECDKCET w kq - 0 1
//! ```
//!
//! with FSF's Empire letters `T E C D K S` (Tower, Eagle, Cardinal, Duke, King,
//! Soldier). mce already names `e c t d` (Elephant / Cannon / Lieutenant / General)
//! and `s` (Silver), so the four Empire pieces take `*`-prefixed overflow tokens
//! (`*t *e *c *d`, recycling the FSF mnemonics) and the Soldier takes `z`; its
//! canonical start FEN is
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/PPPZZPPP/8/*T*E*C*DK*C*E*T w kq - 0 1
//! ```
//!
//! The two are the same position; `compare-fairy/` translates the tokens when
//! driving FSF (`*e → e`, `*c → c`, `*t → t`, `*d → d`, `z → s`). Note the
//! **asymmetry**: White's two Soldiers and six pawns sit on rank 3 (rank 2 is
//! empty), Black is ordinary chess on ranks 7-8, and only Black has castling rights.
//!
//! ## Confirmed semantics (all pinned move-for-move against FSF)
//!
//! * **Asymmetric armies.** Black = standard P/N/B/R/Q/K (the only side with
//!   castling). White = Empire: Tower, Eagle, Cardinal, Duke, two Soldiers, six
//!   pawns, and one King.
//! * **Piece movement.** Each Empire piece *moves* like a Queen onto an empty square
//!   but *captures* only on its short pattern — Eagle: knight, Cardinal: bishop,
//!   Tower: rook, Duke: king. Soldier: one step forward / sideways.
//! * **Promotion.** A pawn of either colour promotes to a Queen only.
//! * **Flag win.** White wins on reaching rank 8, Black on reaching rank 1; a node
//!   whose side to move's opponent already stands on its goal rank is terminal.
//! * **Flying general.** The two kings may not face each other on an open file or
//!   rank.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_empire -- --include-ignored`.

use mce::geometry::{perft as gperft, Chess8x8, Empire};

/// The Empire starting FEN in mce's dialect, confirmed against FSF's
/// `UCI_Variant empire` / `position startpos`.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/PPPZZPPP/8/*T*E*C*DK*C*E*T w kq - 0 1";

/// The start position with Black to move — the asymmetry seen from the standard
/// army's side (a full standard chess opening against the boxed-in Empire army).
const STARTPOS_BLACK: &str = "rnbqkbnr/pppppppp/8/8/8/PPPZZPPP/8/*T*E*C*DK*C*E*T b kq - 0 1";

/// A developed middlegame: White has pushed a centre pawn and brought an Eagle (`*E`)
/// out to d4 while a Cardinal / Duke / Tower remain home, Black has played `...c5`.
/// Exercises the Empire pieces' Queen-slide development against an intact Black army.
const MIDGAME: &str = "rnbqkbnr/pp1ppppp/8/2p5/3P*E3/2P2P2/PP2Z1PP/*T1*C*DK1*E*T w kq - 0 1";

/// A tactic exercising every Empire capture pattern at once: an Eagle (`*E`, knight
/// capture), Cardinal (`*C`, bishop capture), and Tower (`*T`, rook capture) abreast
/// on rank 4 with a ring of standard Black pieces (knights, rook, bishop, queen) in
/// short-capture range, plus a Duke-less White king and a lone pawn. Both kings safe.
const TACTIC: &str = "4k3/8/2n1n3/3rb3/3*E*C*T2/3q4/3P4/4K3 w - - 0 1";

/// A king-flag race: both kings a short walk from their goal ranks, so several lines
/// end by **flag win** (a king reaching its goal rank), terminating perft descent
/// exactly as FSF does.
const FLAG_RACE: &str = "4k3/8/8/8/8/8/4K3/8 w - - 0 1";

/// A bare-kings position on an open file, exercising the **flying-general** faceoff:
/// the two kings may not move onto a square where they see each other down an open
/// file or rank, so many otherwise-legal king steps are pruned.
const FLYING_GENERAL: &str = "8/8/3k4/8/8/8/3K4/8 w - - 0 1";

/// Asserts the generic Empire perft equals each pinned `(depth, nodes)` count. Every
/// number here also matched FSF empire `go perft` on the same FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Empire::from_fen(fen).expect("valid Empire FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Empire perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position, White to move (FSF-confirmed) --------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 30), (2, 600), (3, 20895), (4, 464633)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 17022705)]);
}

// -- Start position, Black to move (FSF-confirmed) --------------------------

#[test]
fn startpos_black_cheap() {
    check(
        STARTPOS_BLACK,
        &[(1, 20), (2, 600), (3, 13352), (4, 464840)],
    );
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_black_deep() {
    check(STARTPOS_BLACK, &[(5, 11495046)]);
}

// -- Developed middlegame (FSF-confirmed) -----------------------------------

#[test]
fn midgame_cheap() {
    check(MIDGAME, &[(1, 48), (2, 1095), (3, 51451), (4, 1277676)]);
}

// -- Every Empire capture pattern at once (FSF-confirmed) -------------------

#[test]
fn tactic_cheap() {
    check(TACTIC, &[(1, 40), (2, 1942), (3, 73871), (4, 3390039)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn tactic_deep() {
    check(TACTIC, &[(5, 135310667)]);
}

// -- Flag win / campmate terminal rule (FSF-confirmed) ----------------------

#[test]
fn flag_race_cheap() {
    check(FLAG_RACE, &[(1, 6), (2, 18), (3, 110), (4, 644), (5, 4218)]);
}

// -- Flying-general king faceoff (FSF-confirmed) ----------------------------

#[test]
fn flying_general_cheap() {
    check(
        FLYING_GENERAL,
        &[(1, 6), (2, 30), (3, 174), (4, 1162), (5, 7350)],
    );
}

// -- The starting FEN round-trips through mce's FEN I/O ----------------------

#[test]
fn startpos_fen_round_trips() {
    let pos = Empire::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    let reparsed = Empire::from_fen(STARTPOS).expect("startpos FEN parses");
    assert_eq!(reparsed.to_fen(), STARTPOS);
}
