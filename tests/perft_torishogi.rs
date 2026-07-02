//! Tori Shogi (bird shogi, 7x7) perft, pinned node-for-node to Fairy-Stockfish
//! `UCI_Variant torishogi`.
//!
//! Every count below was produced by FSF `go perft <depth>` on the matching
//! position (the FEN translated to FSF's spelling — `*y → s`, `*g → +S`,
//! `*a → f`, `*i → +F`, `*k → c`, `*v → l`, `*r → r`, `*z → p`) and reproduced by
//! the `compare-fairy` harness. The cheap depths run in the default suite; the
//! deeper ones are `#[ignore]`d and run with `--include-ignored`.

use mce::geometry::{perft as gperft, Tori, Tori7x7};

/// The confirmed Tori Shogi starting position, in mce's overflow spelling of the
/// FSF start `rpckcpl/3f3/sssssss/2s1S2/SSSSSSS/3F3/LPCKCPR[-] w 0 1`.
const STARTPOS: &str =
    "*r*z*kk*k*z*v/3*a3/*y*y*y*y*y*y*y/2*y1*Y2/*Y*Y*Y*Y*Y*Y*Y/3*A3/*V*Z*KK*K*Z*R[] w - - 0 1";

/// A drop midgame: both back rows present, the centre swallows traded off, and
/// each side holding a Swallow in hand. (FSF `rpckcpl/3f3/sssssss/7/SSSSSSS/3F3/LPCKCPR[Ss]`.)
const DROPS: &str =
    "*r*z*kk*k*z*v/3*a3/*y*y*y*y*y*y*y/7/*Y*Y*Y*Y*Y*Y*Y/3*A3/*V*Z*KK*K*Z*R[*Y*y] w - - 0 1";

/// A promotion midgame: Swallows one step from the zone, an advanced Falcon, and
/// hands holding only Swallows and Falcons (the canonical hand contents). Every
/// zone move is a forced promotion. (FSF `2k4/1S5/7/3F3/7/5s1/4K2[SFsf]`.)
const PROMO: &str = "2k4/1*Y5/7/3*A3/7/5*y1/4K2[*Y*A*y*a] w - - 0 1";

/// A promoted-piece midgame: a Goose (`*G` / FSF `+S`) and an Eagle (`*I` / FSF
/// `+F`) of each colour on the board, each banking back to its base when captured.
/// (FSF `3k3/2+S4/7/3+F3/7/2+f4/3K3[Ss]`.)
const PROMOTED: &str = "3k3/2*G4/7/3*I3/7/2*i4/3K3[*Y*y] w - - 0 1";

/// A quail-active midgame: both (asymmetric) quails and a crane of each colour on
/// the board, with Swallows and Falcons in hand. (FSF
/// `3k3/7/2lr3/7/3LR2/7/3K3[SsFf]`.)
const QUAILS: &str = "3k3/7/2*v*r3/7/3*V*R2/7/3K3[*Y*y*A*a] w - - 0 1";

fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Tori::from_fen(fen).expect("valid Tori Shogi FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Tori7x7, _>(&pos, depth);
        assert_eq!(got, expected, "Tori perft({depth}) mismatch for {fen}");
    }
}

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 17), (2, 288), (3, 5430), (4, 103857)]);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 2179749)]);
}

#[test]
fn drops_cheap() {
    check(DROPS, &[(1, 36), (2, 1269), (3, 32496)]);
}

#[test]
fn promo_cheap() {
    check(PROMO, &[(1, 94), (2, 7588), (3, 422869)]);
}

#[test]
fn promoted_cheap() {
    check(PROMOTED, &[(1, 3), (2, 154), (3, 10490)]);
}

#[test]
fn quails_cheap() {
    check(QUAILS, &[(1, 92), (2, 7697), (3, 413805)]);
}

/// Regression for the Pheasant check-interposition drop bug (issue #239): the Tori
/// **Pheasant** leaps two squares straight forward (a Dabbaba jump), so a check it
/// delivers along a file **cannot be blocked** by interposing on the intervening
/// square. mce's hand-drop check mask used `between(king, checker)` unconditionally,
/// letting a held Swallow be (illegally) dropped onto that square to "block" the
/// jump — after which the Pheasant simply captured the king. The fix gates the drop
/// interposition on `role_is_slider`, mirroring the move generator's check mask.
///
/// Here Black's Pheasant on f3 checks the White king on f1 (the f3 -> f1 jump over
/// f2); the only legal replies are the two king steps and capturing the checker, so
/// perft(1) is 3 — not 4 (the bug also counted the bogus `S@f2` interposition drop).
/// This position is reached by the #239 differential fuzzer; FSF confirms 3/134.
#[test]
fn pheasant_check_cannot_be_blocked_by_a_drop() {
    const CHECK_BY_PHEASANT: &str =
        "*r*z1k2*v/2*a*k*k1*y/*y1*y*y*y1*Y/1*y*y1*Y*y1/*Y*Y*Y1*Y*z1/1*g1*k2*A/*V*Z2*KK*R[*Y*z] \
w - - 0 15";
    check(CHECK_BY_PHEASANT, &[(1, 3), (2, 134), (3, 4743)]);
}

/// Regression for the pinned-Pheasant jump bug (issue #416): a Pheasant pinned to
/// its own king along a file may **not** take its two-square forward *jump*, which
/// would vacate the shielding square and expose the king to the pinning slider.
///
/// Here the Black King is on f6, the Black Pheasant on f5, and the White Right
/// Quail on f4; the Quail slides forward up the file (f4 -> f5 -> f6), so the
/// Pheasant is the only shield. mce previously allowed the Pheasant's `f5f3` jump
/// (over the Quail, off the king-to-pinner segment), leaving the king in check —
/// perft(1) was 35 with the illegal `f5f3`. Confining a pinned piece to the
/// king-to-pinner segment drops it; the correct count is 34. FSF (`torishogi`,
/// same position) confirms 34 / 1048 / 30652.
#[test]
fn pinned_pheasant_cannot_take_its_forward_jump() {
    const PINNED_PHEASANT: &str =
        "*G*z1*y1*y*v/3*a1k1/*k*K*y2*z*Y/1*y*y*y*k*R*Y/1*Y1*Y*Y2/*V2*YK*Y*R/1*Z1*A*K*Z1[*Y*y] \
b - - 2 28";
    check(PINNED_PHEASANT, &[(1, 34), (2, 1048), (3, 30652)]);
}
