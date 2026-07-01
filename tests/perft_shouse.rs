//! S-House / Seirawan-house (8x8) perft validation on the generic engine
//! (issue #264) — **Seirawan gating** (Hawk / Elephant) composed with
//! **Crazyhouse drops**, validating the unified gate/drop hand and the
//! crazyhouse promoted-square (`~`) revert end-to-end.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mce::geometry::Shouse::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant shouse`) running `go perft` on the byte-identical position. The
//! `compare-fairy/` harness re-runs that head-to-head on demand
//! (`compare-fairy/src/shouse.rs`); this test pins the FSF-confirmed numbers so a
//! regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `UCI_Variant shouse` start position:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[AEae] w KQBCDFGkqbcdfg - 0 1
//! ```
//!
//! The `[AEae]` is the crazyhouse hand (the starting Hawk/Elephant reserves) and
//! the `KQBCDFGkqbcdfg` castling field carries the castling rights plus the
//! gating-eligible back-rank files, exactly as in Seirawan. mce spells the Hawk
//! `a`/`A` (its census/Capablanca letter) where FSF's S-House uses `H`/`h`; the
//! Elephant is `e`/`E` in both, so the shown FEN is the **mce dialect** and the
//! `compare-fairy/` harness rewrites the one Hawk letter when driving FSF.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_shouse -- --include-ignored`.

use mce::geometry::{perft as gperft, Chess8x8, GateSquare, Shouse, WideRole};

/// The S-House starting FEN, confirmed against FSF `UCI_Variant shouse`.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[AEae] w KQBCDFGkqbcdfg - 0 1";

/// A developed midgame exercising **both** mechanics at once: each side has a
/// captured Knight in hand alongside the starting Hawk/Elephant (`[EANean]`), so
/// drops are live and a back-rank piece's first move may gate the Knight, the
/// Hawk, or the Elephant. Pinned against FSF (its normalised echo of the
/// position).
const DROPS_AND_GATES: &str =
    "r1bqk2r/ppp2ppp/2n5/3pp3/3PP3/2N5/PPP2PPP/R1BQK2R[EANean] w KQCDkqcd - 0 1";

/// A position with a **promoted** queen (`Q~` on a8) that Black can capture: the
/// crazyhouse rule banks it as a **Pawn**, not a Queen, so the perft diverges
/// from the same shape with an original queen. Validates the `~` mask and the
/// promoted-revert banking. Pinned against FSF.
const PROMOTED_REVERT: &str = "Q~r1k4/8/8/8/8/8/8/4K3[] b - - 0 1";

/// Discovered **double-check** gating (issue #363). White's `b5` Knight sits on
/// the `a4`-Bishop's `a4`-`e8` diagonal; moving it (`b5xc7` or `b5xd6`) delivers
/// a *double* check — the Knight directly plus the unmasked Bishop. Black's only
/// reply is the king flight `e8d8`, which may **still gate** the held Hawk onto
/// the vacated `e8` (`e8d8h`). mce previously returned early on double check
/// before the gating pass, dropping that gated escape and under-counting
/// `perft(2)` by 2. Both positions are pinned against FSF (`UCI_Variant shouse`).
const DOUBLE_CHECK_GATE_1: &str =
    "rnq1kbnr/p1p1pp1p/1p1p3p/1N1P1p1E/B7/8/P2PPPPP/1RBQKBNR[AEa] w KkqCDFGbfg - 0 15";
const DOUBLE_CHECK_GATE_2: &str =
    "rnq1kbnr/2p1pp1p/pp1p3p/1N1P1p1E/B7/7E/P2PPPPP/1RBQKBNR[Aa] w KkqCDFGbfg - 0 16";

/// `(depth, nodes)` rows confirmed identical between mce and FSF.
struct Perft {
    fen: &'static str,
    rows: &'static [(u32, u64)],
}

const STARTPOS_PERFT: Perft = Perft {
    fen: STARTPOS,
    rows: &[(1, 92), (2, 7944), (3, 546_694), (4, 36_857_065)],
};

const DROPS_AND_GATES_PERFT: Perft = Perft {
    fen: DROPS_AND_GATES,
    rows: &[(1, 200), (2, 36_942), (3, 5_165_468)],
};

const PROMOTED_REVERT_PERFT: Perft = Perft {
    fen: PROMOTED_REVERT,
    rows: &[(1, 7), (2, 126), (3, 1_795), (4, 37_401)],
};

fn check(p: &Perft, max_depth: u32) {
    let pos = Shouse::from_fen(p.fen).expect("S-House FEN parses");
    for &(depth, nodes) in p.rows {
        if depth > max_depth {
            continue;
        }
        assert_eq!(
            gperft::<Chess8x8, _>(&pos, depth),
            nodes,
            "S-House perft depth {depth} for FEN {}",
            p.fen,
        );
    }
}

#[test]
fn startpos_shallow_matches_fsf() {
    check(&STARTPOS_PERFT, 3);
}

#[test]
fn drops_and_gates_shallow_matches_fsf() {
    check(&DROPS_AND_GATES_PERFT, 2);
}

#[test]
fn promoted_revert_shallow_matches_fsf() {
    check(&PROMOTED_REVERT_PERFT, 3);
}

/// Regression for issue #363: a king fleeing a **discovered double check** off
/// its virgin gating square must still be able to gate a held piece onto the
/// vacated square. Both FENs' `perft(2)` are the FSF-confirmed counts (the
/// differing move is the gated king escape `e8d8h`, worth exactly one extra node
/// per position, so each total was short by 2 before the fix).
#[test]
fn double_check_gating_matches_fsf() {
    for (fen, want) in [(DOUBLE_CHECK_GATE_1, 5649), (DOUBLE_CHECK_GATE_2, 3813)] {
        let pos = Shouse::from_fen(fen).expect("S-House FEN parses");
        assert_eq!(
            gperft::<Chess8x8, _>(&pos, 2),
            want,
            "S-House double-check gating perft(2) for FEN {fen}",
        );
    }
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn startpos_deep_matches_fsf() {
    check(&STARTPOS_PERFT, 4);
    // FSF startpos depth 5 = 2_107_498_685.
    let pos = Shouse::from_fen(STARTPOS).expect("FEN parses");
    assert_eq!(gperft::<Chess8x8, _>(&pos, 5), 2_107_498_685);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn drops_and_gates_deep_matches_fsf() {
    check(&DROPS_AND_GATES_PERFT, 3);
    // FSF depth 4 = 707_042_638.
    let pos = Shouse::from_fen(DROPS_AND_GATES).expect("FEN parses");
    assert_eq!(gperft::<Chess8x8, _>(&pos, 4), 707_042_638);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn promoted_revert_deep_matches_fsf() {
    check(&PROMOTED_REVERT_PERFT, 4);
}

/// The starting hand makes both the Hawk and the Elephant droppable as well as
/// gateable: at the start FSF lists 16 pawn moves, 12 knight moves (4 plain plus
/// their gate-Hawk / gate-Elephant variants), and 64 drops (32 Hawk + 32
/// Elephant onto the empty middle ranks) — 92 in all.
#[test]
fn startpos_has_both_drops_and_gates() {
    let pos = Shouse::startpos();
    let moves = pos.legal_moves();
    assert_eq!(moves.len(), 92, "S-House startpos perft 1");

    let drops = moves.iter().filter(|m| m.is_drop()).count();
    assert_eq!(drops, 64, "32 Hawk + 32 Elephant drops");

    // A gate move carries a hand-gate role (the Hawk or Elephant) onto the moving
    // knight's vacated origin square.
    let gate_hawk = moves
        .iter()
        .filter(|m| m.hand_gate() == Some(WideRole::Hawk))
        .count();
    let gate_elephant = moves
        .iter()
        .filter(|m| m.hand_gate() == Some(WideRole::Elephant))
        .count();
    assert_eq!(gate_hawk, 4, "one gate-Hawk per knight move");
    assert_eq!(gate_elephant, 4, "one gate-Elephant per knight move");
    assert!(
        moves
            .iter()
            .all(|m| !m.is_gating() || m.hand_gate().is_none()),
        "S-House never uses the fixed Seirawan reserve encoding"
    );
    assert!(
        moves
            .iter()
            .filter(|m| m.hand_gate().is_some())
            .all(|m| matches!(m.hand_gate_square(), GateSquare::Origin)),
        "a non-castling gate lands on the origin"
    );
}

/// A captured Knight in hand may itself be gated by a back-rank piece's first
/// move — gating draws from the whole hand, not just the Hawk/Elephant.
#[test]
fn captured_piece_in_hand_is_gateable() {
    // White rook a1 (queenside gating right via `Q`), a Knight in hand.
    let pos = Shouse::from_fen("4k3/8/8/8/8/8/8/R3K3[N] w Q - 0 1").expect("FEN parses");
    let moves = pos.legal_moves();
    let knight_gates = moves
        .iter()
        .filter(|m| m.hand_gate() == Some(WideRole::Knight))
        .count();
    assert!(knight_gates > 0, "the held Knight is gateable");
    // A Pawn is never gated even when held.
    let pos2 = Shouse::from_fen("4k3/8/8/8/8/8/8/R3K3[P] w Q - 0 1").expect("FEN parses");
    assert!(
        pos2.legal_moves()
            .iter()
            .all(|m| m.hand_gate() != Some(WideRole::Pawn)),
        "a Pawn is never gated",
    );
}

/// The crazyhouse `~` promotion mark round-trips through FEN.
#[test]
fn promoted_mark_round_trips() {
    let pos = Shouse::from_fen(PROMOTED_REVERT).expect("FEN with ~ parses");
    let fen = pos.to_fen();
    assert!(
        fen.contains("Q~"),
        "FEN re-renders the promoted mark: {fen}"
    );
    let reparsed = Shouse::from_fen(&fen).expect("re-rendered FEN parses");
    assert_eq!(reparsed.to_fen(), fen, "FEN round-trips");
}
