//! Ataxx perft validation for the standalone `mce::ataxx` module (issue #280).
//!
//! Ataxx is **not** a chess variant — it has no pieces, no king, and no attacks
//! — so it lives in its own self-contained module rather than on the chess
//! engine (see `src/ataxx.rs`). The node counts below were produced
//! **identically** by `mce::ataxx::Position::perft` and by Fairy-Stockfish
//! (`UCI_Variant ataxx`, `go perft`) on the byte-identical FEN; the live
//! head-to-head re-runs on demand via `compare-fairy/src/ataxx.rs`. This test
//! pins the FSF-confirmed numbers so a regression is caught even without FSF
//! present.
//!
//! ## Confirmed starting FEN
//!
//! ```text
//! P5p/7/7/7/7/7/p5P w 0 1
//! ```
//!
//! White starts on a7 and g1, Black on a1 and g7. The deep startpos layers
//! (depths 5 and 6) are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_ataxx -- --include-ignored`.

use mce::ataxx::{Color, Move, Outcome, Position};

const STARTPOS: &str = "P5p/7/7/7/7/7/p5P w 0 1";

/// White's lone a7 stone is walled in by Black; Black still has moves and the
/// board is not full, so White must **pass** — exercising pass parity with FSF.
const PASS_WALL: &str = "Ppp4/ppp4/ppp4/7/7/7/7 w 0 1";

/// Three stones a side, an extra central White stone — a flip- and jump-heavy
/// open middlegame.
const MID_EXTRA: &str = "P5p/7/7/3P3/7/7/p5P w 0 1";

/// A symmetric cross of single stones on the central file/rank ends.
const CROSS: &str = "3p3/7/7/3P3/7/7/3p3 w 0 1";

/// A dense checkered top two ranks plus a lone White stone — many flips.
const CHECKER: &str = "PpPpPpP/pPpPpPp/7/7/7/7/3P3 w 0 1";

fn perft_at(fen: &str, depth: u32) -> u64 {
    Position::from_fen(fen)
        .expect("valid Ataxx FEN")
        .perft(depth)
}

#[test]
fn startpos_round_trips() {
    let pos = Position::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    assert_eq!(Position::from_fen(STARTPOS).unwrap(), pos);
    assert_eq!(pos.side_to_move(), Color::White);
    assert_eq!(pos.white_count(), 2);
    assert_eq!(pos.black_count(), 2);
}

#[test]
fn startpos_perft() {
    assert_eq!(perft_at(STARTPOS, 1), 16);
    assert_eq!(perft_at(STARTPOS, 2), 256);
    assert_eq!(perft_at(STARTPOS, 3), 6460);
    assert_eq!(perft_at(STARTPOS, 4), 155888);
}

#[test]
#[ignore = "deep; run with --release --include-ignored"]
fn startpos_perft_deep() {
    assert_eq!(perft_at(STARTPOS, 5), 4752668);
    assert_eq!(perft_at(STARTPOS, 6), 141865520);
}

#[test]
fn pass_wall_perft() {
    // Depth 1 is the single forced pass; the game is not over.
    let pos = Position::from_fen(PASS_WALL).unwrap();
    assert_eq!(pos.legal_moves(), vec![Move::Pass]);
    assert!(!pos.is_terminal());
    assert_eq!(perft_at(PASS_WALL, 1), 1);
    assert_eq!(perft_at(PASS_WALL, 2), 55);
    assert_eq!(perft_at(PASS_WALL, 3), 55);
    assert_eq!(perft_at(PASS_WALL, 4), 1961);
}

#[test]
fn mid_extra_perft() {
    assert_eq!(perft_at(MID_EXTRA, 1), 40);
    assert_eq!(perft_at(MID_EXTRA, 2), 618);
    assert_eq!(perft_at(MID_EXTRA, 3), 25026);
}

#[test]
fn cross_perft() {
    assert_eq!(perft_at(CROSS, 1), 24);
    assert_eq!(perft_at(CROSS, 2), 574);
    assert_eq!(perft_at(CROSS, 3), 13590);
    assert_eq!(perft_at(CROSS, 4), 386878);
}

#[test]
fn checker_perft() {
    assert_eq!(perft_at(CHECKER, 1), 54);
    assert_eq!(perft_at(CHECKER, 2), 1943);
    assert_eq!(perft_at(CHECKER, 3), 104864);
}

#[test]
fn terminal_positions_have_no_moves_and_score_by_majority() {
    // Side to move eliminated -> opponent wins, no pass.
    let wiped = Position::from_fen("p6/7/7/7/7/7/7 w 0 1").unwrap();
    assert!(wiped.is_terminal());
    assert_eq!(wiped.outcome(), Some(Outcome::BlackWins));

    // Full board -> majority wins.
    let full = Position::from_fen("PPPPPPP/PPPPPPP/PPPPPPP/PPPpPPP/PPPPPPP/PPPPPPP/PPPPPPP w 0 1")
        .unwrap();
    assert!(full.is_terminal());
    assert_eq!(full.outcome(), Some(Outcome::WhiteWins));
}

#[test]
fn perft_divide_sums_to_perft() {
    let pos = Position::from_fen(MID_EXTRA).unwrap();
    let divide = pos.perft_divide(3);
    let total: u64 = divide.iter().map(|(_, n)| n).sum();
    assert_eq!(total, pos.perft(3));
    assert_eq!(divide.len() as u64, pos.perft(1));
}
