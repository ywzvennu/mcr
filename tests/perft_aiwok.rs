//! Ai-Wok perft validation on the generic engine (issue #404).
//!
//! Ai-Wok is Makruk with the Met (ferz) replaced by a single **Ai-Wok** — a Rook +
//! Knight + Ferz super-piece (FSF `AIWOK`, Betza `RNF`). The node counts below are
//! pinned **and cross-checked against Fairy-Stockfish** (FSF, `UCI_Variant
//! ai-wok`): every `(depth, nodes)` pair here was produced identically by
//! `mcr::geometry::Aiwok::perft` and by FSF's `go perft` on the byte-identical
//! FEN. The `compare-fairy/` harness re-runs that head-to-head on demand (its
//! `--difffuzz --variant ai-wok` mode); this test pins the confirmed numbers so a
//! regression is caught without FSF present.
//!
//! Confirmed Ai-Wok starting FEN (from FSF `position startpos`):
//!   `rnsaksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKASNR w - - 0 1`
//!
//! where FSF's `a` / `A` is the Ai-Wok. mcr has no dedicated Ai-Wok role (the wire
//! format caps a role index at 7 bits and the table is full), so it fields the
//! movement-identical Rook + Knight + Ferz [`mcr::geometry::WideRole::Ship`], whose
//! FEN token is the second-bank overflow `**s` / `**S`; the two describe the
//! byte-identical board.
//!
//! The cheap layers run as ordinary tests; the deep layers are `#[ignore]`d so
//! `cargo test` stays fast — run them with
//! `cargo test --release --test perft_aiwok -- --include-ignored`.

use mcr::geometry::{
    perft as gperft, Aiwok, Bitboard, Chess8x8, Square, WideMoveKind, WidePiece, WideRole,
};
use mcr::Color;

/// The Ai-Wok starting FEN in mcr's dialect (the Ai-Wok spelled `**s` / `**S`),
/// the byte-identical board to FSF's `rnsaksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKASNR
/// w - - 0 1`.
const STARTPOS: &str = "rns**sksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSK**SSNR w - - 0 1";

/// A midgame position (both sides symmetric, an open centre) exercising the
/// Ai-Wok's rook / knight / ferz movement in the open.
const MID: &str = "rns**sksnr/8/pp1ppp1p/2p2p2/2P2P2/PP1PPP1P/8/RNSK**SSNR w - - 0 3";

/// Asserts the generic Ai-Wok perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF ai-wok `go perft` on the byte-identical FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Aiwok::from_fen(fen).expect("valid Ai-Wok FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Aiwok perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(
        STARTPOS,
        &[(1, 26), (2, 676), (3, 18102), (4, 485045), (5, 13275068)],
    );
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_depth6() {
    // FSF ai-wok `go perft 6` on the startpos.
    check(STARTPOS, &[(6, 363187516)]);
}

// -- Midgame (FSF-confirmed) ------------------------------------------------

#[test]
fn mid_cheap() {
    check(MID, &[(1, 24), (2, 603), (3, 15549), (4, 413463)]);
}

// -- Rule-level self-checks (independent of FSF) ----------------------------

/// The starting array round-trips through FEN and matches the confirmed mcr-dialect
/// string; the opening move count is 26 (FSF-confirmed perft(1)).
#[test]
fn startpos_fen_round_trips() {
    let pos = Aiwok::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    assert_eq!(pos.turn(), Color::White);
    assert_eq!(pos.legal_move_count(), 26);
}

/// A lone Ai-Wok on d4 attacks exactly the union of a rook's rays, a knight's
/// leaps, and the four one-step diagonals (the Ferz) — the Rook + Knight + Ferz
/// pattern, confirmed against FSF's `RNF`.
#[test]
fn aiwok_moves_as_rook_knight_ferz() {
    // The Ai-Wok is the Ship (`**S`); place a white one on d4 with a bare enemy
    // king far away so the board is otherwise empty.
    let pos = Aiwok::from_fen("7k/8/8/8/3**S4/8/8/K7 w - - 0 1").expect("valid");
    let d4 = Square::from_file_rank(3, 3).unwrap();
    let attacks: Bitboard<Chess8x8> = pos.piece_attacks(d4).expect("a piece on d4");

    // Rook rays: whole d-file and 4th rank (minus d4 itself), spot-check the ends.
    for (f, r) in [(3u8, 0u8), (3, 7), (0, 3), (7, 3)] {
        let sq = Square::from_file_rank(f, r).unwrap();
        assert!(attacks.contains(sq), "rook ray reaches {f},{r}");
    }
    // Knight leaps from d4: e.g. c6, e6, b5, f5.
    for (f, r) in [(2u8, 5u8), (4, 5), (1, 4), (5, 4)] {
        let sq = Square::from_file_rank(f, r).unwrap();
        assert!(attacks.contains(sq), "knight leap reaches {f},{r}");
    }
    // Ferz steps from d4: c3, e3, c5, e5.
    for (f, r) in [(2u8, 2u8), (4, 2), (2, 4), (4, 4)] {
        let sq = Square::from_file_rank(f, r).unwrap();
        assert!(attacks.contains(sq), "ferz step reaches {f},{r}");
    }
    // A non-attacked square: the far corner h8 is not reachable (not on a ray,
    // knight leap, or ferz step from d4).
    let h1 = Square::from_file_rank(7, 0).unwrap();
    assert!(!attacks.contains(h1), "h1 is unreachable from d4");

    // The Ai-Wok really is fielded as the Ship role.
    let piece = pos.board().piece_at(d4).expect("a piece on d4");
    assert_eq!(piece, WidePiece::new(Color::White, WideRole::Ship));
}

/// A Bia (pawn) promotes only to an Ai-Wok (the Ship) — never a Met — and never
/// double-pushes.
#[test]
fn pawn_promotes_to_aiwok_only() {
    let pos = Aiwok::startpos();
    let has_double = pos
        .legal_moves()
        .into_iter()
        .any(|m| matches!(m.kind(), WideMoveKind::DoublePawnPush));
    assert!(!has_double, "Ai-Wok pawns never double-push");

    // A white pawn on the sixth rank (rank index 5) promotes on its next step —
    // to an Ai-Wok (Ship) only, exactly as Makruk promotes to a Met only.
    let ready = Aiwok::from_fen("7k/8/8/4P3/8/8/8/K7 w - - 0 1").expect("valid");
    let promos: Vec<_> = ready
        .legal_moves()
        .into_iter()
        .filter_map(|m| m.promotion())
        .collect();
    assert_eq!(promos, vec![WideRole::Ship]);
}
