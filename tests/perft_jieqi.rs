//! Jieqi (揭棋, hidden Xiangqi, 9x10 / `u128`) perft validation on the generic
//! engine (issue #278).
//!
//! Jieqi is **not** a Fairy-Stockfish variant — its stochastic hidden-identity
//! reveal cannot be expressed in an FSF variant config, and `go perft` is only
//! meaningful for a full-information position, which is exactly standard Xiangqi.
//! Correctness therefore splits into a deterministic core (validated here + vs FSF
//! `UCI_Variant xiangqi`) and a stochastic reveal layer (validated by the seeded
//! unit/property tests in `mcr::geometry::variants::jieqi`).
//!
//! ## Why these numbers are FSF-confirmed
//!
//! The reveal model in the engine's make-move path is the **identity** baseline: a
//! face-down piece reveals as the Xiangqi piece native to its home square. A dark
//! piece on square *s* therefore both *moves as* and *reveals to* the Xiangqi piece
//! native to *s*, so the **entire Jieqi game tree from the all-dark startpos is
//! bit-identical to standard Xiangqi**. Every `(depth, nodes)` pair below is thus
//! the canonical Xiangqi count, produced identically by `mcr::geometry::Jieqi`,
//! `mcr::geometry::Xiangqi`, and Fairy-Stockfish `go perft` (see
//! `tests/perft_xiangqi.rs` and `compare-fairy/src/jieqi.rs`). This pins the dark
//! movement **and** the reveal transition against FSF without FSF present.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_jieqi -- --include-ignored`.

use mcr::geometry::{perft as gperft, Jieqi, Xiangqi, Xiangqi9x10};

/// The all-dark Jieqi starting FEN (mcr dialect): the two Generals face-up,
/// every other piece a face-down `=D`/`=d`.
const ALL_DARK_STARTPOS: &str =
    "=d=d=d=dk=d=d=d=d/9/1=d5=d1/=d1=d1=d1=d1=d/9/9/=D1=D1=D1=D1=D/1=D5=D1/9/=D=D=D=DK=D=D=D=D w - - 0 1";

/// The same position spelled as standard Xiangqi (the identity-reveal equivalent):
/// FSF's Xiangqi startpos in the mcr dialect.
const XIANGQI_STARTPOS: &str =
    "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/9/RJOUKUOJR w - - 0 1";

/// A developed middlegame, **fully revealed** (so a legal Xiangqi position the FSF
/// `xiangqi` perft confirms): both sides' b/h-file horses are out and cannons
/// centred. Identical to `tests/perft_xiangqi.rs`'s `MID`, parsed by the Jieqi
/// engine (every piece is an existing Xiangqi role, so it moves as in Xiangqi).
const MID_REVEALED: &str =
    "r1oukuo1r/9/1cj3jc1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1CJ3JC1/9/R1OUKUO1R w - - 0 1";

/// A deterministic [splitmix64] step for the seeded lockstep playout.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Asserts the generic Jieqi perft equals each pinned `(depth, nodes)` count.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Jieqi::from_fen(fen).expect("Jieqi FEN parses");
    for &(depth, expected) in cases {
        let got = gperft::<Xiangqi9x10, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Jieqi perft({depth}) for {fen}: expected {expected} (FSF xiangqi-confirmed), got {got}"
        );
    }
}

/// Asserts the Jieqi perft of `jieqi_fen` equals the **Xiangqi** perft of its
/// identity-reveal equivalent `xiangqi_fen` at every depth — the core invariant
/// "identity-reveal Jieqi == Xiangqi", checked live (so it holds even past the
/// pinned constants and on mixed dark/revealed positions).
fn assert_matches_xiangqi(jieqi_fen: &str, xiangqi_fen: &str, depths: &[u32]) {
    let jq = Jieqi::from_fen(jieqi_fen).expect("Jieqi FEN parses");
    let xq = Xiangqi::from_fen(xiangqi_fen).expect("Xiangqi FEN parses");
    for &depth in depths {
        let a = gperft::<Xiangqi9x10, _>(&jq, depth);
        let b = gperft::<Xiangqi9x10, _>(&xq, depth);
        assert_eq!(
            a, b,
            "Jieqi {jieqi_fen} vs Xiangqi {xiangqi_fen} perft({depth}): {a} != {b}"
        );
    }
}

// -- All-dark startpos: bit-identical to the Xiangqi startpos ---------------

#[test]
fn all_dark_startpos_cheap() {
    // The canonical Xiangqi startpos perft sequence (FSF-confirmed).
    check(ALL_DARK_STARTPOS, &[(1, 44), (2, 1920), (3, 79666)]);
}

// The all-dark startpos depth-4 (3.29M nodes, ~26s in debug) is too slow for the
// per-PR floor, so it stays `#[ignore]`d; the depth-≥4 floor for Jieqi is proved
// by the lighter `revealed_middlegame_depth4` (1.78M nodes) below.
#[test]
#[ignore = "deep perft (~3.3M nodes, ~26s in debug); run with --release --include-ignored"]
fn all_dark_startpos_deep() {
    check(ALL_DARK_STARTPOS, &[(4, 3290240), (5, 133312995)]);
}

#[test]
fn all_dark_startpos_matches_xiangqi_live() {
    assert_matches_xiangqi(ALL_DARK_STARTPOS, XIANGQI_STARTPOS, &[1, 2, 3, 4]);
}

// -- Fully-revealed middlegame: a plain Xiangqi position --------------------

#[test]
fn revealed_middlegame_cheap() {
    check(MID_REVEALED, &[(1, 36), (2, 1292), (3, 47994)]);
}

#[test]
fn revealed_middlegame_depth4() {
    check(MID_REVEALED, &[(4, 1777662)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn revealed_middlegame_deep() {
    check(MID_REVEALED, &[(5, 67407683)]);
}

// -- Mixed dark + revealed reached by play: lockstep with Xiangqi -----------

/// Playing the same seeded move sequence from the all-dark Jieqi startpos and the
/// standard Xiangqi startpos keeps the two positions **bit-identical** at every
/// ply: each dark piece reveals (under the identity baseline) to exactly the
/// Xiangqi piece it stood in for, so the legal-move **sets** coincide and the
/// perft from every reached (mixed dark/revealed) node matches Xiangqi. (The move
/// *order* differs — Jieqi enumerates every face-down piece in the single `Dark`
/// role group, Xiangqi enumerates per concrete role — but the same `WideMove`
/// values are legal in both, since a move encodes only its from/to/kind.) This
/// validates the dark movement and the reveal transition on **mid-reveal**
/// positions, not just the all-dark start.
#[test]
fn lockstep_with_xiangqi_through_reveals() {
    let sorted = |mut v: Vec<mcr::geometry::WideMove>| {
        v.sort();
        v
    };
    let mut jq = Jieqi::startpos();
    let mut xq = Xiangqi::startpos();
    let mut seed = 0xC0FF_EE00_1234_5678u64;
    for ply in 0..16 {
        let jm = jq.legal_moves();
        let xm = xq.legal_moves();
        // Identical boards => identical move *sets* (order may differ).
        assert_eq!(
            sorted(jm.clone()),
            sorted(xm.clone()),
            "ply {ply}: Jieqi and Xiangqi move sets diverge"
        );
        // Perft from this mixed-reveal node matches Xiangqi.
        assert_eq!(
            gperft::<Xiangqi9x10, _>(&jq, 2),
            gperft::<Xiangqi9x10, _>(&xq, 2),
            "ply {ply}: perft from reached node diverges",
        );
        if jm.is_empty() {
            break;
        }
        let mv = jm[(splitmix64(&mut seed) as usize) % jm.len()];
        jq = jq.play(&mv);
        xq = xq.play(&mv);
        // Same occupancy after the move: the reveal lands the home-role Xiangqi
        // piece on the destination (the unmoved face-down pieces keep the `Dark`
        // label but stand on their home squares, where their effective role is
        // exactly the Xiangqi piece — hence the matching move sets above).
        assert_eq!(
            jq.board().occupied(),
            xq.board().occupied(),
            "ply {ply}: occupancy diverges after the move",
        );
    }
}
