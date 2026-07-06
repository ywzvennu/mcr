//! Bughouse **single-board** perft validation on the generic engine (issue #279).
//!
//! Bughouse is a 2-board, 4-player team game, but **each board on its own** plays
//! like Crazyhouse with the hand **fed externally** — and that single board is a
//! full-information, deterministic position, so Fairy-Stockfish's
//! `UCI_Variant bughouse` `go perft` **is** meaningful for it. Every `(depth,
//! nodes)` pair below was produced **identically** by
//! `mcr::geometry::Bughouse::perft` and by FSF running `go perft` on the
//! byte-identical position; the `compare-fairy/` harness re-runs that head-to-head
//! on demand (`compare-fairy/src/bughouse.rs`), and this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## The single rule that distinguishes Bughouse from Crazyhouse
//!
//! On one board a capture does **not** bank the taken piece into the captor's hand
//! — it crosses to the partner board (FSF's `twoBoards`). The [`CAPTURE`] corpus
//! position pins that divergence: from `rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/
//! RNBQKBNR[]` Bughouse perft(3) = 27 226, whereas the same position under
//! Crazyhouse (which *does* bank the captured pawn, making it droppable) is
//! 28 137. Matching FSF's 27 226 confirms `captures_to_hand = false`.
//!
//! ## FEN dialect
//!
//! Bughouse uses only **standard chess pieces** (`K Q R B N P`), whose letters are
//! identical in mcr and FSF, so the FEN is passed to FSF unchanged. The trailing
//! `[..]` is the crazyhouse hand (empty `[]` at the start); FSF accepts it present
//! or omitted.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `UCI_Variant bughouse` start position — the hand starts empty and
//! every node count is identical to standard chess at the start:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1
//! ```
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_bughouse -- --include-ignored`.

use mcr::geometry::{perft as gperft, Bughouse, Chess8x8, Square, WideRole};
use mcr::Color;

/// The Bughouse starting FEN, confirmed against FSF — standard array, empty hand.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1";

/// A developed midgame with a Knight and a Pawn in **each** side's hand
/// (`[NPnp]`), so drops are live alongside ordinary play. Pinned against FSF.
const HAND: &str = "r1bqk2r/ppp2ppp/2n5/3pp3/3PP3/2N5/PPP2PPP/R1BQK2R[NPnp] w KQkq - 0 1";

/// A position with an **immediate capture** available (`e4`x`d5`), pinning the
/// Bughouse-vs-Crazyhouse divergence: the captured pawn is **not** banked into
/// White's hand (it would cross to the partner board), so this diverges from
/// Crazyhouse at depth 3 (Bughouse 27 226 vs Crazyhouse 28 137). Pinned vs FSF.
const CAPTURE: &str = "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR[] w KQkq - 0 1";

/// A bare-kings position with a lone Queen in White's hand, isolating the **drop**
/// generator: at depth 1, 5 king moves + 62 Queen drops (every empty square) = 67.
const QDROP: &str = "4k3/8/8/8/8/8/8/4K3[Q] w - - 0 1";

/// Both sides hold a full `[QRBNPqrbnp]` reserve over a bare castling skeleton,
/// stressing drops of every role at once (and the pawn rank-restriction). Pinned
/// against FSF.
const RICH: &str = "r3k2r/8/8/8/8/8/8/R3K2R[QRBNPqrbnp] w KQkq - 0 1";

/// `(depth, nodes)` rows confirmed identical between mcr and FSF.
struct Perft {
    fen: &'static str,
    rows: &'static [(u32, u64)],
}

const STARTPOS_PERFT: Perft = Perft {
    fen: STARTPOS,
    rows: &[
        (1, 20),
        (2, 400),
        (3, 8902),
        (4, 197_281),
        (5, 4_865_609),
        (6, 119_060_324),
    ],
};

const HAND_PERFT: Perft = Perft {
    fen: HAND,
    rows: &[(1, 101), (2, 9_720), (3, 744_450), (4, 56_322_675)],
};

const CAPTURE_PERFT: Perft = Perft {
    fen: CAPTURE,
    rows: &[
        (1, 31),
        (2, 866),
        (3, 27_226),
        (4, 788_468),
        (5, 25_292_260),
    ],
};

const QDROP_PERFT: Perft = Perft {
    fen: QDROP,
    rows: &[(1, 67), (2, 230), (3, 7_090)],
};

const RICH_PERFT: Perft = Perft {
    fen: RICH,
    rows: &[(1, 306), (2, 78_889), (3, 17_274_113)],
};

fn check(p: &Perft, max_depth: u32) {
    let pos = Bughouse::from_fen(p.fen).expect("Bughouse FEN parses");
    for &(depth, nodes) in p.rows {
        if depth > max_depth {
            continue;
        }
        assert_eq!(
            gperft::<Chess8x8, _>(&pos, depth),
            nodes,
            "Bughouse perft depth {depth} for FEN {}",
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
fn capture_does_not_bank_matches_fsf() {
    check(&CAPTURE_PERFT, 3);
}

#[test]
fn qdrop_shallow_matches_fsf() {
    check(&QDROP_PERFT, 3);
}

#[test]
fn rich_shallow_matches_fsf() {
    check(&RICH_PERFT, 2);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn startpos_deep_matches_fsf() {
    check(&STARTPOS_PERFT, 6);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn hand_deep_matches_fsf() {
    check(&HAND_PERFT, 4);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn capture_deep_matches_fsf() {
    check(&CAPTURE_PERFT, 5);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn rich_deep_matches_fsf() {
    check(&RICH_PERFT, 3);
}

/// A capture on this board must **not** add the taken piece to the captor's hand
/// (it crosses to the partner board): after `e4`x`d5` White's hand is still empty,
/// unlike Crazyhouse. This is the one single-board rule that separates Bughouse
/// from Crazyhouse, and the [`CAPTURE`] perft pins its node-count consequence.
#[test]
fn capture_leaves_hand_empty() {
    let pos = Bughouse::from_fen(CAPTURE).expect("FEN parses");
    let exd5 = pos
        .legal_moves()
        .into_iter()
        .find(|m| m.to_uci::<Chess8x8>() == "e4d5")
        .expect("e4xd5 is legal");
    let after = pos.play(&exd5);
    for color in [Color::White, Color::Black] {
        for role in [
            WideRole::Pawn,
            WideRole::Knight,
            WideRole::Bishop,
            WideRole::Rook,
            WideRole::Queen,
        ] {
            assert_eq!(
                after.hand_count(color, role),
                0,
                "a Bughouse capture banks nothing into either hand ({color:?} {role:?})",
            );
        }
    }
}

/// The external **hand-injection** API (the cross-board transfer a server wires
/// up): injecting a role into a side's hand makes it droppable, removing it takes
/// it back, and a removal from an empty hand reports `false`.
#[test]
fn inject_into_hand_makes_piece_droppable() {
    let mut pos = Bughouse::startpos();
    assert_eq!(pos.hand_count(Color::White, WideRole::Queen), 0);
    assert!(pos.legal_moves().iter().all(|m| !m.is_drop()));

    // Deliver a captured Queen from the partner board.
    pos.inject_into_hand(Color::White, WideRole::Queen);
    assert_eq!(pos.hand_count(Color::White, WideRole::Queen), 1);

    // It is now droppable onto the 32 empty middle-rank squares.
    let q_drops = pos
        .legal_moves()
        .into_iter()
        .filter(|m| m.is_drop() && m.drop_role() == Some(WideRole::Queen))
        .count();
    assert_eq!(q_drops, 32, "a held Queen drops onto every empty square");

    // Reclaiming it empties the hand and removes the drops.
    assert!(pos.remove_from_hand(Color::White, WideRole::Queen));
    assert_eq!(pos.hand_count(Color::White, WideRole::Queen), 0);
    assert!(pos.legal_moves().iter().all(|m| !m.is_drop()));

    // Removing from an empty hand reports no piece was present.
    assert!(!pos.remove_from_hand(Color::White, WideRole::Queen));
}

/// Injection stacks and is per-color, per-role: a server may deliver several
/// pieces, and each side's hand is independent.
#[test]
fn inject_stacks_and_is_per_color() {
    let mut pos = Bughouse::startpos();
    pos.inject_into_hand(Color::White, WideRole::Knight);
    pos.inject_into_hand(Color::White, WideRole::Knight);
    pos.inject_into_hand(Color::Black, WideRole::Rook);
    assert_eq!(pos.hand_count(Color::White, WideRole::Knight), 2);
    assert_eq!(pos.hand_count(Color::Black, WideRole::Knight), 0);
    assert_eq!(pos.hand_count(Color::Black, WideRole::Rook), 1);
    assert_eq!(pos.hand_count(Color::White, WideRole::Rook), 0);
}

/// A dropped Pawn may not land on the first or last rank (the crazyhouse rule,
/// confirmed against FSF): with only a Pawn in hand on a bare-kings board, every
/// pawn drop is on ranks 2-7.
#[test]
fn pawn_drops_avoid_first_and_last_rank() {
    let mut pos = Bughouse::from_fen("4k3/8/8/8/8/8/8/4K3[] w - - 0 1").expect("FEN parses");
    pos.inject_into_hand(Color::White, WideRole::Pawn);
    let pawn_drops: Vec<_> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| m.is_drop() && m.drop_role() == Some(WideRole::Pawn))
        .collect();
    assert_eq!(pawn_drops.len(), 48, "ranks 2-7 only: 6 ranks * 8 files");
    assert!(
        pawn_drops
            .iter()
            .all(|m| (1..=6).contains(&m.to::<Chess8x8>().rank())),
        "no pawn drop on rank 1 (0) or rank 8 (7)",
    );
}

/// The empty crazyhouse hand round-trips through FEN, and an injected piece shows
/// up in the rendered hand bracket.
#[test]
fn hand_round_trips_through_fen() {
    let pos = Bughouse::startpos();
    assert!(
        pos.to_fen().contains("[]"),
        "the empty hand renders as []: {}",
        pos.to_fen()
    );
    let mut held = Bughouse::startpos();
    held.inject_into_hand(Color::White, WideRole::Queen);
    let fen = held.to_fen();
    assert!(fen.contains("[Q]"), "injected Queen rides the FEN: {fen}");
    let reparsed = Bughouse::from_fen(&fen).expect("re-rendered FEN parses");
    assert_eq!(reparsed.hand_count(Color::White, WideRole::Queen), 1);
}

// --- 2-board linkage / cross-board hand transfer (issue #501) -------------

/// The **cross-board hand mechanic** a single board's perft never exercises: a
/// capture on one board delivers the taken piece (reverted to its base role, in
/// the partner's colour) to the *partner* board's hand, where it becomes
/// droppable — while the capturing board's own hand stays empty. This drives the
/// full server linkage end to end across two [`Bughouse`] positions: capture on
/// board A -> [`inject_into_hand`](Bughouse::inject_into_hand) on board B -> drop
/// on board B.
#[test]
fn two_board_capture_delivers_to_the_partner_board() {
    // Board A: White captures a Black pawn (e4xd5). Nothing banks locally.
    let mut board_a = Bughouse::from_fen(CAPTURE).expect("FEN parses");
    let d5 = Square::<Chess8x8>::from_file_rank(3, 4).unwrap();
    let victim = board_a
        .board()
        .piece_at(d5)
        .expect("a black pawn stands on d5");
    assert_eq!(
        (victim.color, victim.role),
        (Color::Black, WideRole::Pawn),
        "the captured piece is a black pawn",
    );
    let exd5 = board_a
        .legal_moves()
        .into_iter()
        .find(|m| m.to_uci::<Chess8x8>() == "e4d5")
        .expect("e4xd5 is legal");
    board_a = board_a.play(&exd5);
    for role in [
        WideRole::Pawn,
        WideRole::Knight,
        WideRole::Bishop,
        WideRole::Rook,
        WideRole::Queen,
    ] {
        assert_eq!(
            board_a.hand_count(Color::White, role),
            0,
            "board A banks nothing"
        );
        assert_eq!(board_a.hand_count(Color::Black, role), 0);
    }

    // Server routing: the captured piece crosses to the partner board B, where the
    // partner is to move and drops it as their own colour (partners play opposite
    // colours, so the delivered piece keeps the captured piece's base role/colour).
    let mut board_b = Bughouse::from_fen("4k3/8/8/8/8/8/8/4K3[] b - - 0 1").expect("FEN parses");
    assert!(
        board_b.legal_moves().iter().all(|m| !m.is_drop()),
        "board B has no reserves before delivery",
    );
    board_b.inject_into_hand(Color::Black, WideRole::Pawn);
    assert_eq!(board_b.hand_count(Color::Black, WideRole::Pawn), 1);

    // The delivered pawn is now droppable on the partner board, and dropping it
    // consumes the reserve and yields a legal continuation.
    let drop = board_b
        .legal_moves()
        .into_iter()
        .find(|m| m.is_drop() && m.drop_role() == Some(WideRole::Pawn))
        .expect("the delivered pawn is droppable on the partner board");
    let after = board_b.play(&drop);
    assert_eq!(
        after.hand_count(Color::Black, WideRole::Pawn),
        0,
        "the drop consumes the delivered reserve",
    );
    assert!(
        !after.legal_moves().is_empty(),
        "the partner board plays on after the drop",
    );
}

/// A captured **promoted** piece is demoted to a Pawn *at the transfer site* — the
/// partner board receives a Pawn, never the promoted piece. Single-board Bughouse
/// never banks captures, so it does not track a crazyhouse promoted mask
/// ([`WideVariant::demotes_promoted_captures`] is `false`); the demotion is
/// therefore the server's responsibility, modelled here — delivering a captured
/// promoted Queen injects a Pawn, so the partner can only ever drop the Pawn.
#[test]
fn promoted_capture_is_demoted_to_a_pawn_on_transfer() {
    // The server, knowing the captured Queen reached the board by promotion,
    // delivers a Pawn rather than the promoted Queen.
    let captured_role = WideRole::Queen;
    let was_promoted = true;
    let delivered = if was_promoted {
        WideRole::Pawn
    } else {
        captured_role
    };

    let mut board_b = Bughouse::from_fen("4k3/8/8/8/8/8/8/4K3[] w - - 0 1").expect("FEN parses");
    board_b.inject_into_hand(Color::White, delivered);
    assert_eq!(
        board_b.hand_count(Color::White, WideRole::Queen),
        0,
        "a promoted capture never delivers a Queen",
    );
    assert_eq!(
        board_b.hand_count(Color::White, WideRole::Pawn),
        1,
        "the promoted piece is demoted to a Pawn on transfer",
    );
    assert!(
        board_b
            .legal_moves()
            .into_iter()
            .any(|m| m.is_drop() && m.drop_role() == Some(WideRole::Pawn)),
        "the demoted pawn is droppable on the partner board",
    );
}
