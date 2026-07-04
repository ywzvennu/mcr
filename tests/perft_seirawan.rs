//! Seirawan chess (S-Chess, 8x8) perft validation on the generic engine
//! (issue #173) — the first **gating** variant, validating the reserve / gating
//! mechanic end-to-end.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Seirawan::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant seirawan`) running `go perft` on the byte-identical position.
//! The startpos depth-4 count (`782599`) is the value pinned in FSF's own
//! `tests/perft.sh`. The `compare-fairy/` harness re-runs that head-to-head on
//! demand (`compare-fairy/src/seirawan.rs`); this test pins the FSF-confirmed
//! numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `UCI_Variant seirawan` start position:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[HEhe] w KQBCDFGkqbcdfg - 0 1
//! ```
//!
//! mcr uses the **same dialect** FSF does for S-Chess (`H`/`h` Hawk, `E`/`e`
//! Elephant), so the FEN is byte-identical. The FEN carries two extensions over
//! plain chess: the `[HEhe]` holdings (the reserves in hand) and the gating
//! rights folded into the castling field (`KQBCDFGkqbcdfg`), where the `KQkq`
//! letters double as the castling rights and the file letters mark the remaining
//! gating-eligible back-rank squares.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_seirawan -- --include-ignored`.

use mcr::geometry::{
    perft as gperft, Chess8x8, GateRole, GateSquare, Seirawan, Square, WideMoveKind, WidePiece,
    WideRole,
};
use mcr::Color;

/// The Seirawan starting FEN, confirmed against Fairy-Stockfish's
/// `UCI_Variant seirawan`.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[HEhe] w KQBCDFGkqbcdfg - 0 1";

/// A midgame position exercising gating: both knights are developed off their
/// back-rank squares (so their first moves are spent), with both reserves still
/// in hand and the remaining back-rank pieces gating-eligible. Pinned against FSF
/// `UCI_Variant seirawan` (its `go perft` divide matches mcr's move-for-move).
const MID_GATING: &str =
    "r1bqkb1r/pppppppp/2n2n2/8/8/2N2N2/PPPPPPPP/R1BQKB1R[HEhe] w KQBCDEFGkqbcdefg - 4 3";

/// A position with a clear white kingside castle, both reserves in hand: the
/// castle itself may gate a Hawk or Elephant onto the king's *or* the rook's
/// vacated square. Pinned against FSF.
const CASTLE_GATE: &str =
    "rnbqk2r/pppppppp/8/8/8/5N2/PPPPPPBP/RNBQK2R[HEhe] w KQkqABCDFGabcdfgh - 0 1";

/// A developed midgame where white has already gated its Elephant (the `E` on
/// a1) and black its Elephant (the `e` on a8); each side keeps only a Hawk in
/// hand (`[Hh]`). Exercises gating with a partial reserve and pieces already on
/// the board. Pinned against FSF.
const PARTIAL_RESERVE: &str =
    "reb1k2r/pppp1ppp/2nbqn2/4p3/4P3/2NBQN2/PPPP1PPP/R1B1K2R[Hh] w KQkqABCDFGabcdfg - 8 6";

/// Asserts the generic Seirawan perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF seirawan `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Seirawan::from_fen(fen).expect("valid Seirawan FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Seirawan perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed; depth 4 = FSF perft.sh) -----------------

#[test]
fn startpos_cheap() {
    // perft(4) = 782599 is the value pinned in FSF's own tests/perft.sh.
    check(STARTPOS, &[(1, 28), (2, 784), (3, 24830), (4, 782599)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 27639803)]);
}

// -- Midgame with gating live (FSF-confirmed) -------------------------------

#[test]
fn mid_gating_cheap() {
    check(MID_GATING, &[(1, 28), (2, 780), (3, 24723)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn mid_gating_deep() {
    check(MID_GATING, &[(4, 775112)]);
}

// -- Castle that may itself gate (FSF-confirmed) ----------------------------

#[test]
fn castle_gate_cheap() {
    check(CASTLE_GATE, &[(1, 39), (2, 1402), (3, 50893)]);
}

// -- Partial reserve, pieces already gated in (FSF-confirmed) ---------------

#[test]
fn partial_reserve_cheap() {
    check(PARTIAL_RESERVE, &[(1, 47), (2, 2151), (3, 95523)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn partial_reserve_deep() {
    check(PARTIAL_RESERVE, &[(4, 4187528)]);
}

// -- Rule-level self-checks (independent of FSF) ----------------------------

/// The starting array round-trips through FEN and matches the confirmed string,
/// with both reserves in hand for both sides and every back-rank square gating-
/// eligible.
#[test]
fn startpos_fen_round_trips() {
    let pos = Seirawan::startpos();
    assert_eq!(pos.turn(), Color::White);
    // 28 = FSF-confirmed perft(1): 20 standard opening moves + 8 gating moves
    // (the two knights, each able to gate either reserve onto its vacated
    // square).
    assert_eq!(pos.legal_move_count(), 28);

    // The placement is the standard chess array; the reserves are off-board.
    let board = pos.board();
    assert_eq!(board.occupied().count(), 32);
    assert_eq!(
        board.king_of(Color::White),
        Square::<Chess8x8>::from_file_rank(4, 0)
    );
    // No Hawk or Elephant is on the board at the start (they are in hand).
    assert_eq!(board.by_role(WideRole::Hawk).count(), 0);
    assert_eq!(board.by_role(WideRole::Elephant).count(), 0);

    // The FEN re-parses to the same position.
    let reparsed = Seirawan::from_fen(STARTPOS).expect("startpos FEN parses");
    assert_eq!(reparsed.legal_move_count(), 28);
    assert_eq!(reparsed.board(), board);
}

/// A knight's opening move may gate a Hawk or an Elephant onto its vacated
/// square; gating is optional, so the un-gated move is present too.
#[test]
fn knight_first_move_can_gate_either_reserve() {
    let pos = Seirawan::startpos();
    let b1 = Square::<Chess8x8>::from_file_rank(1, 0).unwrap(); // b1 knight
    let c3 = Square::<Chess8x8>::from_file_rank(2, 2).unwrap(); // Nc3 target
    let nc3: Vec<_> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| m.from::<Chess8x8>() == b1 && m.to::<Chess8x8>() == c3)
        .collect();
    // Three variants: plain Nc3, Nc3 gating a Hawk on b1, Nc3 gating an Elephant.
    assert_eq!(nc3.len(), 3, "Nc3 + gate-Hawk + gate-Elephant");
    let plain = nc3.iter().filter(|m| !m.is_gating()).count();
    let hawk = nc3
        .iter()
        .filter(|m| m.gate() == Some(GateRole::Hawk))
        .count();
    let eleph = nc3
        .iter()
        .filter(|m| m.gate() == Some(GateRole::Elephant))
        .count();
    assert_eq!((plain, hawk, eleph), (1, 1, 1));

    // Playing the gating move places the reserve on b1 and consumes it from hand.
    let gate_hawk = nc3
        .iter()
        .find(|m| m.gate() == Some(GateRole::Hawk))
        .expect("gate-Hawk move");
    let next = pos.play(gate_hawk);
    assert_eq!(
        next.board().piece_at(b1),
        Some(WidePiece::new(Color::White, WideRole::Hawk)),
    );
    // After gating its Hawk, white has only the Elephant left, and b1 can no
    // longer gate.
    let b1_gates = next
        .legal_moves()
        .into_iter()
        .filter(|m| m.is_gating() && m.gate() == Some(GateRole::Hawk) && m.from::<Chess8x8>() == b1)
        .count();
    assert_eq!(b1_gates, 0);
}

/// A pawn may promote to a Hawk or Elephant in addition to the standard four
/// roles — the Seirawan promotion set.
#[test]
fn pawn_promotes_to_six_roles_including_reserves() {
    // A lone white pawn one step from promotion; kings tucked away.
    let pos = Seirawan::from_fen("4k3/P7/8/8/8/8/8/4K3[HEhe] w - - 0 1").expect("valid");
    let mut roles: Vec<WideRole> = pos
        .legal_moves()
        .into_iter()
        .filter_map(|m| m.promotion())
        .collect();
    roles.sort();
    roles.dedup();
    let mut want = vec![
        WideRole::Knight,
        WideRole::Bishop,
        WideRole::Rook,
        WideRole::Queen,
        WideRole::Hawk,
        WideRole::Elephant,
    ];
    want.sort();
    assert_eq!(roles, want, "Q/R/B/N + Hawk + Elephant");
}

/// Castling may gate onto the king's *or* the rook's vacated square (never
/// both), and counts as a first move for both.
#[test]
fn castling_can_gate_king_or_rook_square() {
    // Clear path for a white kingside castle; reserves in hand.
    let pos = Seirawan::from_fen(
        "rnbqk2r/pppppppp/8/8/8/5N2/PPPPPPBP/RNBQK2R[HEhe] w KQkqABCDFGabcdfgh - 0 1",
    )
    .expect("valid");
    let castles: Vec<_> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| matches!(m.kind(), WideMoveKind::CastleKingside))
        .collect();
    // 1 plain + (Hawk, Elephant) x (king-square, rook-square) = 1 + 4 = 5.
    assert_eq!(castles.len(), 5, "plain castle + 4 gating castles");
    let on_origin = castles
        .iter()
        .filter(|m| m.is_gating() && matches!(m.gate_square(), GateSquare::Origin))
        .count();
    let on_rook = castles
        .iter()
        .filter(|m| m.is_gating() && matches!(m.gate_square(), GateSquare::RookOrigin))
        .count();
    assert_eq!((on_origin, on_rook), (2, 2));

    // Gating onto the king's origin (e1) after O-O places the reserve on e1.
    let gate_king = castles
        .iter()
        .find(|m| matches!(m.gate_square(), GateSquare::Origin) && m.gate() == Some(GateRole::Hawk))
        .expect("a king-square gating castle");
    let next = pos.play(gate_king);
    let e1 = Square::<Chess8x8>::from_file_rank(4, 0).unwrap();
    assert_eq!(
        next.board().piece_at(e1),
        Some(WidePiece::new(Color::White, WideRole::Hawk)),
    );
}
