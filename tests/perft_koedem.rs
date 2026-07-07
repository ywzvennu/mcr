//! Koedem perft validation on the generic engine (issue #585).
//!
//! Koedem ("King of the dead") is an 8x8 Fairy-Stockfish built-in
//! (`UCI_Variant koedem`): Crazyhouse-style chess where the king is a non-royal
//! Commoner you must re-drop from hand, and a side wins by owning *every* king on
//! the board. Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Koedem::perft` and by FSF running `go perft` on the byte-identical
//! position; the `compare-fairy/` harness re-runs that head-to-head on demand
//! (`compare-fairy/src/koedem.rs`), and this test pins the FSF-confirmed numbers so a
//! regression is caught even without FSF present.
//!
//! ## What the corpus exercises
//!
//! * [`STARTPOS`] — the no-check Commoner king already lifts the tree above standard
//!   chess (perft(4) = `197742`, chess = `197281`), like Extinction chess.
//! * [`HAND`] — a developed middlegame with a Knight and a Pawn in each hand
//!   (`[PNpn]`), so crazyhouse-style drops run alongside ordinary play.
//! * [`KINGDROP`] — an extra king in White's hand: `mustDrop` forces White to drop
//!   the Commoner before anything else, so perft(1) is exactly the empty-square count
//!   (32).
//! * [`ENDGAME`] — a single king per side with pawns, where a king can be captured
//!   **without** ending the game (the opponent then owns only one king, below the
//!   `opponent_min = 2` "own all" threshold): the tree plays on, and matching FSF
//!   confirms mcr does not wrongly truncate.
//! * [`TWOKING`] — two kings per side, so king-captures and king-adjacency appear
//!   throughout the tree.
//! * [`RICH`] — a full `[PNBRQpnbrq]` reserve over a castling skeleton, stressing
//!   drops of every role at once (and the pawn rank restriction).
//!
//! ## FEN dialect
//!
//! Koedem uses only **standard chess pieces** (`K Q R B N P` — the king is a
//! Commoner by rule, not by letter), identical in mcr and FSF, so the FEN — hand
//! bracket `[..]` included — is passed to FSF unchanged.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_koedem -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Koedem};

/// The Koedem starting FEN, confirmed against FSF — standard array, empty hand.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1";

/// A developed middlegame with a Knight and a Pawn in **each** side's hand
/// (`[PNpn]`), so drops are live alongside ordinary play. Pinned against FSF.
const HAND: &str = "r1bqk2r/ppp2ppp/2n5/3pp3/3PP3/2N5/PPP2PPP/R1BQK2R[PNpn] w KQkq - 0 1";

/// An extra king in White's hand over the standard array: `mustDrop` forces White
/// to drop the Commoner first, so perft(1) = 32 (one king drop per empty square).
const KINGDROP: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[K] w KQkq - 0 1";

/// A single king per side with pawns: a king may be captured **without** ending the
/// game (the captor then owns only one king, below the `opponent_min = 2` "own all"
/// threshold), so the tree plays on. Matching FSF confirms mcr does not truncate.
const ENDGAME: &str = "4k3/pp6/8/8/8/8/6PP/4K3[] w - - 0 1";

/// Two kings per side over pawns, so king-captures and king-adjacency appear
/// throughout the tree (no check ever restricts them). Pinned against FSF.
const TWOKING: &str = "3k1k2/pp4pp/8/8/8/8/PP4PP/3K1K2[] w - - 0 1";

/// A full `[PNBRQpnbrq]` reserve over a bare castling skeleton, stressing drops of
/// every role at once (and the pawn rank restriction). Pinned against FSF.
const RICH: &str = "r3k2r/8/8/8/8/8/8/R3K2R[PNBRQpnbrq] w KQkq - 0 1";

/// `(depth, nodes)` rows confirmed identical between mcr and FSF.
struct Perft {
    fen: &'static str,
    rows: &'static [(u32, u64)],
}

const STARTPOS_PERFT: Perft = Perft {
    fen: STARTPOS,
    rows: &[(1, 20), (2, 400), (3, 8902), (4, 197_742), (5, 4_897_256)],
};

const HAND_PERFT: Perft = Perft {
    fen: HAND,
    rows: &[(1, 101), (2, 10_021), (3, 790_351), (4, 61_610_397)],
};

const KINGDROP_PERFT: Perft = Perft {
    fen: KINGDROP,
    rows: &[(1, 32), (2, 630), (3, 16_040), (4, 350_483)],
};

const ENDGAME_PERFT: Perft = Perft {
    fen: ENDGAME,
    rows: &[(1, 9), (2, 81), (3, 756), (4, 7_224), (5, 68_888)],
};

const TWOKING_PERFT: Perft = Perft {
    fen: TWOKING,
    rows: &[(1, 17), (2, 289), (3, 4_913), (4, 83_457)],
};

const RICH_PERFT: Perft = Perft {
    fen: RICH,
    rows: &[(1, 306), (2, 91_960), (3, 23_417_495)],
};

fn check(p: &Perft, max_depth: u32) {
    let pos = Koedem::from_fen(p.fen).expect("Koedem FEN parses");
    for &(depth, nodes) in p.rows {
        if depth > max_depth {
            continue;
        }
        assert_eq!(
            gperft::<Chess8x8, _, _>(&pos, depth),
            nodes,
            "Koedem perft depth {depth} for FEN {}",
            p.fen,
        );
    }
}

#[test]
fn startpos_shallow_matches_fsf() {
    check(&STARTPOS_PERFT, 4);
}

#[test]
fn hand_shallow_matches_fsf() {
    check(&HAND_PERFT, 3);
}

#[test]
fn kingdrop_shallow_matches_fsf() {
    check(&KINGDROP_PERFT, 3);
}

#[test]
fn endgame_shallow_matches_fsf() {
    check(&ENDGAME_PERFT, 4);
}

#[test]
fn twoking_shallow_matches_fsf() {
    check(&TWOKING_PERFT, 3);
}

#[test]
fn rich_shallow_matches_fsf() {
    check(&RICH_PERFT, 2);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn startpos_deep_matches_fsf() {
    check(&STARTPOS_PERFT, 5);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn hand_deep_matches_fsf() {
    check(&HAND_PERFT, 4);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn kingdrop_deep_matches_fsf() {
    check(&KINGDROP_PERFT, 4);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn endgame_deep_matches_fsf() {
    check(&ENDGAME_PERFT, 5);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn twoking_deep_matches_fsf() {
    check(&TWOKING_PERFT, 4);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn rich_deep_matches_fsf() {
    check(&RICH_PERFT, 3);
}
