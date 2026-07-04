//! Shako (10x10 / `u128`) perft validation on the generic engine (issue #184) —
//! standard chess on a ten-by-ten board plus the Xiangqi-style **Cannon** and a
//! Fers-Alfil **Elephant**. This is the variant that introduces the reusable
//! cannon-attack primitive future Xiangqi/Janggi inherit.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Shako::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant shako`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position. The `compare-fairy/` harness re-runs that
//! head-to-head on demand (`compare-fairy/src/shako.rs`); this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `shako_variant()` (`startFen`):
//!
//! ```text
//! FSF dialect: c8c/ernbqkbnre/pppppppppp/10/10/10/10/PPPPPPPPPP/ERNBQKBNRE/C8C w KQkq - 0 1
//! mcr dialect: c8c/vrnbqkbnrv/pppppppppp/10/10/10/10/PPPPPPPPPP/VRNBQKBNRV/C8C w KQkq - 0 1
//! ```
//!
//! The two differ only in the elephant's letter: FSF spells the Fers-Alfil
//! elephant `e`, but mcr already uses `e` for the Rook+Knight Elephant (the
//! Capablanca/Grand marshal), so the Shako elephant takes the free letter `v`
//! ([`WideRole::FersAlfil`](mcr::geometry::WideRole::FersAlfil)). The cannon is
//! `c`/`C` ([`WideRole::Cannon`](mcr::geometry::WideRole::Cannon)) in both. The
//! cannons sit in the four corners; the king is on the f-file (file 5) with the
//! castling rooks on the b/i files, all on rank 2 (white) / rank 9 (black).
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_shako -- --include-ignored`.

use mcr::geometry::{perft as gperft, Grand10x10, Shako};

/// The Shako starting FEN (mcr dialect), confirmed against Fairy-Stockfish's
/// `UCI_Variant shako`.
const STARTPOS: &str =
    "c8c/vrnbqkbnrv/pppppppppp/10/10/10/10/PPPPPPPPPP/VRNBQKBNRV/C8C w KQkq - 0 1";

/// A developed midgame, white to move: both cannons advanced off the back rank,
/// pawns pushed to create screens, and captures (including a cannon capture over
/// a pawn screen) available at depth. Confirmed move-for-move by FSF.
const MID1: &str = "c8c/vr1bqkbnrv/pp1ppppppp/2p7/2C4c2/3P6/10/PP2PPPPPP/VRNBQKBNRV/C8C w - - 0 1";

/// White **in check from a black cannon** over a pawn screen (only two legal
/// replies at depth 1). Exercises the screen-dependent cannon check, the
/// check-evasion mask, and the cannon king-danger that the standard mask-based
/// fast path cannot model — the position that drove the `has_cannons()`
/// pseudo-legal + verify king-safety path.
const CHECKED: &str =
    "c8c/vrnbqkbnrv/pp2pppppp/2pp6/10/2C2c4/3P6/PP2PPPPPP/VRNBQKBNRV/C8C w - - 0 1";

/// Black **in check from a white cannon** down the f-file over a pawn screen.
/// The legal replies are the king's two off-file escapes plus three
/// interpositions onto the ray between the screen and the king (which add a
/// second screen and break the capture) — but **not** the king sliding *along*
/// the ray, which the lifted-king danger map would wrongly allow. FSF-confirmed.
const CANNON_CHECK_FILE: &str =
    "c8c/vrnbqkbnrv/ppppp1pppp/10/10/5p4/5C4/PPPPP1PPPP/VRNBQKBNRV/C8C b KQkq - 0 1";

/// A castling-legal position, white to move: the rank-2 squares between the king
/// (f-file) and both castling rooks (b/i files) are clear, so both the kingside
/// (king f2 -> h2, rook i2 -> g2) and queenside (king f2 -> d2, rook b2 -> e2)
/// castles are legal. Exercises the rank-2 [`castle_rank`] / [`castle_dest_files`]
/// geometry. FSF-confirmed.
///
/// [`castle_rank`]: mcr::geometry::WideVariant::castle_rank
/// [`castle_dest_files`]: mcr::geometry::WideVariant::castle_dest_files
const CASTLING: &str = "c8c/vr3k2rv/pppppppppp/10/10/10/10/PPPPPPPPPP/VR3K2RV/C8C w KQkq - 0 1";

/// A promotion position, white to move: a white pawn on b9 promotes on b10 to any
/// of the six Shako promotion types — Queen, Rook, Bishop, Knight, **Cannon, or
/// Elephant**. Exercises the wider promotion set (and a fresh cannon/elephant on
/// the board). FSF-confirmed.
const PROMO: &str = "5k4/1P8/10/10/10/10/10/10/10/5K3C w - - 0 1";

/// Cannon-aware castling king-walk regression (issue #335), white to move. The
/// white king is home on f2 and the queenside castle f2 -> d2 must walk across
/// e2, which is attacked by the black cannon on e5 **over the white knight on e3
/// as its screen**. FSF forbids that castle; mcr used to allow it because its
/// king-walk danger map was built by *forward*-projecting each enemy piece, and a
/// cannon's forward projection lands its over-screen capture on the first
/// *occupied* square beyond the screen (here e1, not the empty transit square e2).
/// The fix re-tests each transit square with the king placed on it, so the
/// cannon's forward projection sees e2 as its target. This position is the one the
/// #239 differential fuzzer surfaced (reached from the pinned mid-game FEN after
/// `e10e5`); the depth-1 count is `67` (it was `68` with the spurious `f2d2`).
const CANNON_CASTLE_335: &str =
    "c1q2k4/vrn6v/2pp2p1r1/4p1n2p/pp3p1ppb/1Q2cP4/PN1P3P2/1PP1N1P1Pb/1R3KB2R/1V1CB3VC w Q - 0 18";

/// Asserts the generic Shako perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF shako `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Shako::from_fen(fen).expect("valid Shako FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Grand10x10, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Shako perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 58), (2, 3364), (3, 185938)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    // FSF shako `go perft` on the startpos.
    check(STARTPOS, &[(4, 10273158), (5, 559582321)]);
}

// -- Midgame with cannon captures over a screen (FSF-confirmed) -------------

#[test]
fn midgame_cheap() {
    check(MID1, &[(1, 71), (2, 4555), (3, 303607)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_deep() {
    check(MID1, &[(4, 18713984)]);
}

// -- Cannon checks (the screen-dependent king-safety path, FSF-confirmed) ---

#[test]
fn cannon_check_white_cheap() {
    check(CHECKED, &[(1, 2), (2, 134), (3, 7632)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn cannon_check_white_deep() {
    check(CHECKED, &[(4, 490402)]);
}

#[test]
fn cannon_check_along_file() {
    check(CANNON_CHECK_FILE, &[(1, 5), (2, 398), (3, 22443)]);
}

// -- Castling on rank 2 (FSF-confirmed) -------------------------------------

#[test]
fn castling_cheap() {
    check(CASTLING, &[(1, 54), (2, 2916), (3, 151262)]);
}

// -- Promotion to the full Shako set incl. cannon/elephant (FSF-confirmed) --

#[test]
fn promotion_cheap() {
    check(PROMO, &[(1, 23), (2, 111), (3, 3050), (4, 19737)]);
}

// -- Cannon-aware castling king-walk (issue #335, FSF-confirmed) ------------

#[test]
fn cannon_castle_king_walk_335() {
    // FSF shako `go perft`: 67 / 4160 / 268485. mcr previously reported 68 at
    // depth 1 (a spurious `f2d2` castle across the cannon-attacked e2 transit
    // square).
    check(CANNON_CASTLE_335, &[(1, 67), (2, 4160), (3, 268485)]);
}
