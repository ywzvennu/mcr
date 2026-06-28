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
