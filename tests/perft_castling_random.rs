//! Randomised-castling (Chess960 / Shredder-X-FEN) perft validation for the wide
//! variants that castle (issue #325).
//!
//! The wide engine already stores castling rights as the **rook's start file**
//! ([`GenericCastling`]) and generates / applies the castle from that file rather
//! than a fixed `e1`/`g1`/`c1` square, so a randomised back rank composes with the
//! existing castle geometry for free. This suite pins the FSF-confirmed node
//! counts for randomised starts of every wide castling variant that
//! Fairy-Stockfish can express, so a regression is caught even without FSF
//! present.
//!
//! ## How a randomised start is expressed
//!
//! It is a plain opt-in FEN — no flag or separate variant. A non-randomised start
//! is byte-identical because its rooks sit in the corners (a/h on 8×8, a/j on
//! Capablanca's 10×8), where the rook-file castle reduces exactly to the standard
//! one. Two FEN castling dialects are accepted, mirroring FSF:
//!
//! * **X-FEN** — `KQkq` names the outermost rook on each side (the only form a
//!   two-rook back rank ever needs).
//! * **Shredder-FEN** — an explicit rook-file letter (`JAja` for Capablanca's
//!   corner rooks), the dialect FSF writes for `caparandom`.
//!
//! Every `(depth, nodes)` pair below was produced identically by the wide engine
//! and by Fairy-Stockfish (`UCI_Chess960 true`) on the byte-identical position.
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_castling_random -- --include-ignored`.

use mcr::geometry::{
    perft as gperft, Cap10x8, Capablanca, Capahouse, Chess8x8, Seirawan, WideMoveKind,
};

// -- Capablanca (10x8), randomised back ranks (FSF: capablanca + Chess960) ----

/// Asserts the generic Capablanca perft equals each FSF-confirmed `(depth, nodes)`
/// for a randomised back rank named by its white array (chancellor `e`).
fn cap(back: &str, cases: &[(u32, u64)]) {
    let fen = format!(
        "{back}/pppppppppp/10/10/10/10/PPPPPPPPPP/{} w KQkq - 0 1",
        back.to_uppercase()
    );
    let pos = Capablanca::from_fen(&fen).expect("valid randomised Capablanca FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Cap10x8, _, _>(&pos, depth);
        assert_eq!(got, expected, "Capablanca-960 perft({depth}) for {fen}");
    }
}

#[test]
fn capablanca_random_baskets_cheap() {
    // Six randomised back ranks, each with the king between its two rooks. The
    // rook files (a/j or interior) drive the castle; the destinations stay
    // Capablanca's i/c (king) and h/d (rook).
    cap("rbnqkbaenr", &[(1, 28), (2, 784), (3, 25338), (4, 813823)]);
    cap("rnbeqkabnr", &[(3, 25227), (4, 805899)]);
    cap("anrbkqbenr", &[(3, 22522), (4, 688751)]);
    cap("rnabkqbenr", &[(3, 25228), (4, 806917)]);
    cap("nrbqkbenra", &[(3, 20358), (4, 607748)]);
    cap("rkrnbqaben", &[(3, 22917), (4, 714667)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn capablanca_random_deep() {
    cap("rbnqkbaenr", &[(5, 29192838)]);
}

/// A Shredder-FEN (`JAja`, naming Capablanca's corner rooks by file) parses to the
/// same position and perft as the X-FEN `KQkq` form. King on the e-file, rooks
/// a/j, the back rank cleared so both castles are available.
#[test]
fn capablanca_shredder_fen_parses() {
    let shredder = "r3k4r/pppppppppp/10/10/10/10/PPPPPPPPPP/R3K4R w JAja - 0 1";
    let xfen = "r3k4r/pppppppppp/10/10/10/10/PPPPPPPPPP/R3K4R w KQkq - 0 1";
    let a = Capablanca::from_fen(shredder).expect("Shredder FEN");
    let b = Capablanca::from_fen(xfen).expect("X-FEN");
    assert_eq!(a.castling(), b.castling(), "JAja == KQkq for corner rooks");
    for depth in 1..=4 {
        assert_eq!(
            gperft::<Cap10x8, _, _>(&a, depth),
            gperft::<Cap10x8, _, _>(&b, depth),
        );
    }
    // FSF-confirmed: this randomised castle position has perft(4) = 887784.
    assert_eq!(gperft::<Cap10x8, _, _>(&a, 4), 887784);
}

/// The randomised-castling discovered-check edge case: White may castle queenside
/// only if the departing b1 rook does not unmask the black a1 rook onto the king's
/// c1 landing square. It does, so the castle is illegal — FSF perft(1) = 19,
/// perft(2) = 225 (without the post-castle safety test mcr would count 20 / more).
#[test]
fn capablanca_castle_discovered_check_forbidden() {
    let fen = "4k5/10/10/10/10/10/10/rR2K4R w Q - 0 1";
    let pos = Capablanca::from_fen(fen).expect("valid");
    assert_eq!(gperft::<Cap10x8, _, _>(&pos, 1), 19);
    assert_eq!(gperft::<Cap10x8, _, _>(&pos, 2), 225);
    // No castling move is generated at the root.
    assert!(
        !pos.legal_moves().into_iter().any(|m| matches!(
            m.kind(),
            WideMoveKind::CastleKingside | WideMoveKind::CastleQueenside
        )),
        "the unsafe queenside castle must not be generated",
    );
}

// -- Capahouse (10x8, Capablanca + crazyhouse drops) --------------------------

/// A randomised Capahouse castle position (empty hand) castles on the same
/// Capablanca files; FSF-confirmed perft(2) = 961, perft(3) = 29210.
#[test]
fn capahouse_random_castle() {
    let fen = "r3k4r/pppppppppp/10/10/10/10/PPPPPPPPPP/R3K4R[] w KQkq - 0 1";
    let pos = Capahouse::from_fen(fen).expect("valid Capahouse FEN");
    assert_eq!(gperft::<Cap10x8, _, _>(&pos, 2), 961);
    assert_eq!(gperft::<Cap10x8, _, _>(&pos, 3), 29210);
}

// -- Seirawan (S-Chess, 8x8, gating) ------------------------------------------

/// A randomised Seirawan start (king e, rooks b/g) with every back-rank square
/// gating-eligible. FSF-confirmed (`seirawan` + Chess960) perft 22 / 484 / 14008.
#[test]
fn seirawan_random_start() {
    let fen = "nrbqkbrn/pppppppp/8/8/8/8/PPPPPPPP/NRBQKBRN[HEhe] w KQACDFHkqacdfh - 0 1";
    let pos = Seirawan::from_fen(fen).expect("valid randomised Seirawan FEN");
    assert_eq!(gperft::<Chess8x8, _, _>(&pos, 1), 22);
    assert_eq!(gperft::<Chess8x8, _, _>(&pos, 2), 484);
    assert_eq!(gperft::<Chess8x8, _, _>(&pos, 3), 14008);
}

/// A castling-rich Seirawan position (back rank cleared, full reserves and gating
/// rights) exercising the gated castle on the standard a/h-rook files.
/// FSF-confirmed perft 47 / 2209 / 80141.
#[test]
fn seirawan_castle_rich() {
    let fen = "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R[HEhe] w KQkq - 0 1";
    let pos = Seirawan::from_fen(fen).expect("valid Seirawan FEN");
    assert_eq!(gperft::<Chess8x8, _, _>(&pos, 1), 47);
    assert_eq!(gperft::<Chess8x8, _, _>(&pos, 2), 2209);
    assert_eq!(gperft::<Chess8x8, _, _>(&pos, 3), 80141);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn seirawan_castle_rich_deep() {
    let fen = "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R[HEhe] w KQkq - 0 1";
    let pos = Seirawan::from_fen(fen).expect("valid Seirawan FEN");
    assert_eq!(gperft::<Chess8x8, _, _>(&pos, 4), 2907399);
}
