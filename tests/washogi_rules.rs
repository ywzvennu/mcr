//! Wa Shogi rule unit tests (issue #324): piece-move correctness, the promotion
//! zone and forced promotion, drop legality, and FEN round-trip. These document the
//! rules-validated (no perft oracle) behaviour piece by piece, complementing the
//! independent brute-force perft cross-check in `tests/perft_washogi.rs` and the
//! attacker-consistency playouts in `tests/attackers_consistency.rs`.

use mcr::geometry::{Washogi, Washogi11x11, WideRole};
use mcr::Color;

const STARTPOS: &str = "**f**j**h**l**nk**o**k**g**m**d/1**v3**q3**t1/\
**b**b**b**r**b**b**b**u**b**b**b/11/11/11/11/11/\
**B**B**B**U**B**B**B**R**B**B**B/1**T3**Q3**V1/\
**D**M**G**K**OK**N**L**H**J**F[] w - - 0 1";

/// Square index for `(file, rank)` (0-based, a1 = 0).
fn sq(file: u8, rank: u8) -> u8 {
    rank * 11 + file
}

/// The sorted set of `(file, rank)` destinations of every legal move from the
/// `(file, rank)` origin in `fen`.
fn dests_from(fen: &str, file: u8, rank: u8) -> Vec<(u8, u8)> {
    let pos = Washogi::from_fen(fen).expect("valid Wa FEN");
    let from = sq(file, rank);
    let mut out: Vec<(u8, u8)> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| m.from::<Washogi11x11>().index() == from)
        .map(|m| {
            let t = m.to::<Washogi11x11>().index();
            (t % 11, t / 11)
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

#[test]
fn startpos_fen_round_trips() {
    let pos = Washogi::from_fen(STARTPOS).expect("valid Wa FEN");
    assert_eq!(pos.to_fen(), STARTPOS);
    assert_eq!(pos.turn(), Color::White);
}

#[test]
fn startpos_is_the_documented_placement() {
    // The royal Crane King reuses `WideRole::King`; the back rank reads (a..k)
    // Oxcart, Blind Dog, Strutting Crow, Flying Goose, Violent Wolf, Crane King,
    // Violent Stag, Flying Cock, Swooping Owl, Climbing Monkey, Liberated Horse.
    let pos = Washogi::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
}

#[test]
fn heavenly_horse_jumps_as_a_vertical_knight() {
    // A lone (already-promoted) Heavenly Horse `=H` on f6 jumps to the four
    // `(±1, ±2)` squares, forward and backward, ignoring blockers.
    let fen = "k10/11/11/11/11/5=H5/11/11/11/11/K10 w - - 0 1";
    let d = dests_from(fen, 5, 5);
    assert_eq!(d, vec![(4, 3), (4, 7), (6, 3), (6, 7)]);
}

#[test]
fn cloud_eagle_moves_as_documented() {
    // A lone Cloud Eagle `**V` on f6 (file 5, rank 5): a vertical rook (five up,
    // five down), one sideways step, a one-to-three forward-diagonal slide, and one
    // backward-diagonal step. It never promotes, so no extra promoting moves arise.
    let fen = "k10/11/11/11/11/5**V5/11/11/11/11/K10 w - - 0 1";
    let d = dests_from(fen, 5, 5);
    let mut expected = vec![
        // Vertical rook up f7..f11 and down f5..f1.
        (5, 6),
        (5, 7),
        (5, 8),
        (5, 9),
        (5, 10),
        (5, 4),
        (5, 3),
        (5, 2),
        (5, 1),
        (5, 0),
        // Sideways one step.
        (4, 5),
        (6, 5),
        // Forward diagonals up to three.
        (6, 6),
        (7, 7),
        (8, 8),
        (4, 6),
        (3, 7),
        (2, 8),
        // Backward diagonals one step.
        (6, 4),
        (4, 4),
    ];
    expected.sort_unstable();
    assert_eq!(d, expected);
}

#[test]
fn oxcart_is_a_forward_lance() {
    // A lone Oxcart `**D` on f6 slides straight up only (f7..f11). The squares in
    // the promotion zone (ranks 9-11, 0-based 8-10) offer an optional promotion, so
    // each of those is reached by two moves; here we only check the destination set.
    let fen = "k10/11/11/11/11/5**D5/11/11/11/11/K10 w - - 0 1";
    let d = dests_from(fen, 5, 5);
    assert_eq!(d, vec![(5, 6), (5, 7), (5, 8), (5, 9), (5, 10)]);
}

#[test]
fn sparrow_promotion_is_forced_on_the_last_rank_optional_otherwise() {
    // A Sparrow on f10 (rank 9, in the zone) moving to f11 (the last rank) has no
    // further move there, so promotion is forced: exactly one move, a promotion to
    // a Golden Bird.
    let forced_fen = "5k5/5**B5/11/11/11/11/11/11/11/11/5K5 w - - 0 1";
    let pos = Washogi::from_fen(forced_fen).expect("valid");
    let from = sq(5, 9);
    let sparrow_moves: Vec<_> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| m.from::<Washogi11x11>().index() == from)
        .collect();
    assert_eq!(sparrow_moves.len(), 1, "forced single promoting move");
    assert_eq!(
        sparrow_moves[0].promotion(),
        Some(WideRole::GoldenBird),
        "Sparrow promotes to a Golden Bird"
    );

    // A Sparrow on f9 (rank 8, in the zone) moving to f10 (still in the zone, not
    // the last rank) promotes optionally: two moves, one promoting, one not.
    let optional_fen = "5k5/11/5**B5/11/11/11/11/11/11/11/5K5 w - - 0 1";
    let pos = Washogi::from_fen(optional_fen).expect("valid");
    let from = sq(5, 8);
    let sparrow_moves: Vec<_> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| m.from::<Washogi11x11>().index() == from)
        .collect();
    assert_eq!(sparrow_moves.len(), 2, "optional promotion: promote or not");
    assert!(sparrow_moves.iter().any(|m| m.promotion().is_some()));
    assert!(sparrow_moves.iter().any(|m| m.promotion().is_none()));
}

#[test]
fn sparrow_may_not_be_dropped_on_the_last_rank() {
    // White holds a Sparrow in hand; a dropped Sparrow would be immobile on the last
    // rank (rank 11, 0-based 10), so no such drop is legal.
    let fen = "5k5/11/11/11/11/11/11/11/11/11/5K5[**B] w - - 0 1";
    let pos = Washogi::from_fen(fen).expect("valid");
    let last_rank_drops = pos
        .legal_moves()
        .into_iter()
        .filter(|m| m.is_drop() && m.to::<Washogi11x11>().index() / 11 == 10)
        .count();
    assert_eq!(last_rank_drops, 0, "no Sparrow drop on the last rank");
    // It may be dropped elsewhere (e.g. rank 10), so drops do exist.
    let any_drop = pos.legal_moves().into_iter().any(|m| m.is_drop());
    assert!(any_drop, "Sparrow drops are otherwise available");
}

#[test]
fn captured_promoted_piece_reverts_to_its_base_in_hand() {
    // A White Crane King on f1 captures a Black promoted Gliding Swallow `=x` on g1;
    // the bank should be its base, a Swallow's Wings, droppable as `**Q`.
    let fen = "5k5/11/11/11/11/11/11/11/11/11/5K=x4 w - - 0 1";
    let pos = Washogi::from_fen(fen).expect("valid");
    let capture = pos
        .legal_moves()
        .into_iter()
        .find(|m| {
            m.from::<Washogi11x11>().index() == sq(5, 0)
                && m.to::<Washogi11x11>().index() == sq(6, 0)
        })
        .expect("king captures the gliding swallow");
    let next = pos.play(&capture);
    // The captured Gliding Swallow banks as a Swallow's Wings for White.
    let fen_after = next.to_fen();
    assert!(
        fen_after.contains("**Q"),
        "hand holds a Swallow's Wings (`**Q`): {fen_after}"
    );
}
