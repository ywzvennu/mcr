//! Sho Shogi (old 9x9 Shogi **without drops**, **with** the Drunk Elephant /
//! Crown Prince) perft validation on the generic engine (issue #267).
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::ShoShogi::perft` and by Fairy-Stockfish (FSF, `UCI_Variant
//! shoshogi`) running `go perft` on the byte-identical position — the FSF divide
//! matches mcr's move-for-move, including the full Shogi army and its
//! `+`-promotions, the Drunk Elephant's seven-direction step, the Drunk Elephant →
//! Crown Prince promotion (creating a **second royal**), and the
//! **count-thresholded** two-royal rule (while a side holds both a King and a Crown
//! Prince neither is royal — it may leave either en prise and is never in check;
//! reduced to one, that piece is an ordinary royal). The `compare-fairy/` harness
//! re-runs the head-to-head on demand (`compare-fairy/src/shoshogi.rs`); this test
//! pins the FSF-confirmed numbers so a regression is caught even without FSF.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant shoshogi`, `position startpos`) renders the start as
//!
//! ```text
//! lnsgkgsnl/1r2e2b1/ppppppppp/9/9/9/PPPPPPPPP/1B2E2R1/LNSGKGSNL w 0 1
//! ```
//!
//! with the Drunk Elephant as `e`/`E`. The single-`*` overflow alphabet being
//! exhausted, mcr spells the Drunk Elephant and Crown Prince with the **doubled**
//! overflow prefix `**` (`**E`/`**e` Drunk Elephant, `**C`/`**c` Crown Prince), so
//! mcr's start FEN is
//!
//! ```text
//! lnsgkgsnl/1r2**e2b1/ppppppppp/9/9/9/PPPPPPPPP/1B2**E2R1/LNSGKGSNL w - - 0 1
//! ```
//!
//! The two are the same position; `compare-fairy/` rewrites `**e → e`, `**c → +E`
//! when driving FSF. The deep layers are `#[ignore]`d so `cargo test` stays fast —
//! run them with `cargo test --release --test perft_shoshogi -- --include-ignored`.

use mcr::geometry::{perft as gperft, ShoShogi, Shogi9x9};

/// The Sho Shogi starting FEN in mcr's dialect, confirmed against FSF's
/// `UCI_Variant shoshogi` / `position startpos`.
const STARTPOS: &str =
    "lnsgkgsnl/1r2**e2b1/ppppppppp/9/9/9/PPPPPPPPP/1B2**E2R1/LNSGKGSNL w - - 0 1";

/// A developed middlegame (a few pawns advanced on both wings) — exercises the
/// full army's interactions away from the opening.
const MIDGAME: &str =
    "lnsgkgsnl/1r2**e2b1/p1pppp1pp/1p4p2/9/2P3P2/PP1PPP1PP/1B2**E2R1/LNSGKGSNL w - - 0 1";

/// **Two white royals**: a King (e1) and a Crown Prince (e3, a promoted Drunk
/// Elephant). While White holds both, **neither is royal** — White is never in
/// check and may move (or expose) either freely; every pseudo-legal move is legal.
const TWO_ROYALS: &str = "4k4/9/9/9/9/9/4**C4/9/4K4 w - - 0 1";

/// A white **Drunk Elephant in the promotion zone** (e7): each of its moves may
/// promote to a **Crown Prince**, giving White a second royal — a promotion that
/// is always legal (it drops the side's pseudo-royalty), exactly as in FSF.
const DE_PROMOTE: &str = "4k4/9/4**E4/9/9/9/9/9/4K4 w - - 0 1";

/// A **lone Crown Prince** (White's only royal, after the King was lost) standing
/// in check from a Rook on a2: it behaves as an ordinary royal — only the two
/// king-moves that escape the check are legal.
const LONE_CROWN_PRINCE: &str = "3k5/9/9/9/9/9/9/r8/4**C4 w - - 0 1";

/// Asserts the generic Sho Shogi perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `shoshogi` `go perft` on the same FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = ShoShogi::from_fen(fen).expect("valid Sho Shogi FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Shogi9x9, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Sho Shogi perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 26), (2, 676), (3, 17368), (4, 445372)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 11494746), (6, 296171901)]);
}

// -- Developed middlegame (FSF-confirmed) -----------------------------------

#[test]
fn midgame_cheap() {
    check(MIDGAME, &[(1, 36), (2, 1199), (3, 41031), (4, 1358571)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_deep() {
    check(MIDGAME, &[(5, 46281288)]);
}

// -- Two royals: count-thresholded pseudo-royalty (FSF-confirmed) -----------

#[test]
fn two_royals_cheap() {
    // With both a King and a Crown Prince, White is never in check.
    check(TWO_ROYALS, &[(1, 13), (2, 65), (3, 830), (4, 5644)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn two_royals_deep() {
    check(TWO_ROYALS, &[(5, 75616)]);
}

// -- Drunk Elephant → Crown Prince promotion (FSF-confirmed) ----------------

#[test]
fn de_promote_cheap() {
    check(DE_PROMOTE, &[(1, 19), (2, 56), (3, 838), (4, 3950)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn de_promote_deep() {
    check(DE_PROMOTE, &[(5, 57356)]);
}

// -- Lone Crown Prince behaves as a royal king (FSF-confirmed) --------------

#[test]
fn lone_crown_prince_cheap() {
    check(LONE_CROWN_PRINCE, &[(1, 2), (2, 74), (3, 232), (4, 5999)]);
}

// -- In-check crown-prince escape: promoting a Drunk Elephant drops the check --

/// A White-to-move node **in check** (a Black Pawn on h4 attacks the lone White
/// King on h3) where White still holds an **un-promoted** Drunk Elephant on d6.
/// Reached from issue #454's parent by Black `h5h4`. mcr finds **four** legal
/// evasions; FSF finds only three — the difference is the Drunk-Elephant → Crown-
/// Prince promotion, which is a genuine fourth evasion under Sho Shogi's rules
/// (gaining a second royal drops the whole notion of check) but which FSF's move
/// generator does not emit. mcr is correct; the `compare-fairy` fuzzer discounts
/// exactly this FSF-omitted move (see `shoshogi_fsf_visible_count`) so the
/// cross-check stays faithful.
const IN_CHECK_DE_ESCAPE: &str =
    "7k1/l3**er2l/1p+N1Ng2n/ps1**Esbp2/9/P2PPPPpp/LG4GK1/6S1R/2S5L w - - 0 50";

#[test]
fn in_check_de_promotion_is_a_legal_evasion() {
    let pos = ShoShogi::from_fen(IN_CHECK_DE_ESCAPE).expect("valid Sho Shogi FEN");

    // White's lone King (h3) is in check from the Black Pawn on h4.
    assert!(
        pos.is_check(),
        "White should be in check in {IN_CHECK_DE_ESCAPE}"
    );

    // Exactly four legal evasions, one of them the Drunk-Elephant → Crown-Prince
    // promotion (rendered with the `**c` overflow-promotion token).
    let moves = pos.legal_moves();
    let mut ucis: Vec<String> = moves.iter().map(|mv| mv.to_uci::<Shogi9x9>()).collect();
    ucis.sort();
    assert_eq!(
        ucis,
        vec!["d6d7**c", "g3h4", "h3h2", "h3i4"],
        "the four legal evasions, including the crown-prince promotion"
    );
    assert_eq!(
        gperft::<Shogi9x9, _, _>(&pos, 1),
        4,
        "perft(1) is the legal-move count"
    );

    // Playing the promotion gives White a second royal (King h3 + Crown Prince d7):
    // under Sho Shogi's count-thresholded pseudo-royalty neither is royal, so the
    // side is no longer in check even though the King is still physically attacked.
    let white = pos.turn();
    let promo = moves
        .iter()
        .find(|mv| mv.to_uci::<Shogi9x9>() == "d6d7**c")
        .expect("the crown-prince promotion is legal");
    let after = pos.play(promo);
    assert!(
        !after.is_in_check(white),
        "with two royals White is never in check, so the promotion escapes"
    );
}

/// A **not-in-check** node where White's Drunk Elephant (d7) is **pinned** in
/// front of its King (d1) by a Black Rook (d8). Moving the Elephant off the d-file
/// would expose the King — so mcr allows it **only by promoting**: the six off-file
/// Crown-Prince promotions are legal (each gains a second royal, and a two-royal
/// side is never in check, so leaving the King en prise is fine), while their
/// non-promoting twins are not. Only `d7d8`/`d7d8**c` (capturing the pinner) has a
/// legal non-promoting form. FSF omits the six promotions; mcr is correct.
const PINNED_DE_PROMOTES_OFF_PIN: &str =
    "l1+Ng1g3/3r1k3/1+S1**E**ep1pl/1pp6/2P2P1np/7G1/PP2+n1N2/L3G3L/3K2S2 w - - 1 46";

#[test]
fn pinned_drunk_elephant_may_break_pin_only_by_promoting() {
    let pos = ShoShogi::from_fen(PINNED_DE_PROMOTES_OFF_PIN).expect("valid Sho Shogi FEN");
    let white = pos.turn();

    // White holds a single royal (King d1) and is not in check — the Elephant
    // shields it from the Rook on d8.
    assert!(!pos.is_check());

    // Every legal Drunk-Elephant move from d7. Off the d-file, only the promoting
    // form is legal; on the d-file (d7d8, capturing the pinner) both forms are.
    let mut de_moves: Vec<String> = pos
        .legal_moves()
        .iter()
        .map(|mv| mv.to_uci::<Shogi9x9>())
        .filter(|u| u.starts_with("d7"))
        .collect();
    de_moves.sort();
    assert_eq!(
        de_moves,
        vec!["d7c6**c", "d7c7**c", "d7c8**c", "d7d8", "d7d8**c", "d7e6**c", "d7e7**c", "d7e8**c",],
        "off-file Elephant moves are legal only as Crown-Prince promotions"
    );

    // Stepping off the pin by promoting exposes the King to the Rook, yet with two
    // royals White is not in check, so the move is legal.
    let off_pin = pos
        .legal_moves()
        .into_iter()
        .find(|mv| mv.to_uci::<Shogi9x9>() == "d7c6**c")
        .expect("the off-pin promotion is legal");
    let after = pos.play(&off_pin);
    assert!(
        !after.is_in_check(white),
        "two royals ⇒ never in check, even with the King now exposed to the rook"
    );
}
