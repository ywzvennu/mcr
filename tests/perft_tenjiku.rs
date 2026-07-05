//! Tenjiku Shogi (16x16) start-position perft and movement cross-checks.
//!
//! ## Validation status (read `variants::tenjiku` module docs for the full picture)
//!
//! The reference oracle for the large-shogi variants is **HaChu** (H. G. Muller,
//! driven by `compare-fairy`); Tenjiku is **not** a Fairy-Stockfish variant.
//!
//! **HaChu cannot run Tenjiku in this build.** HaChu 0.23 (`ddugovic/hachu`)
//! **segfaults deterministically** on `variant tenjiku`: its 16x16 play area fills
//! the entire `BW*BH` board array, so there is no EDGE-sentinel border for its
//! 0x88-style neighbour scans (its own burn code even comments "assumes 32x16
//! board"), and a padded 32x32 rebuild crashes identically. This is a genuine HaChu
//! limitation, not an mcr difference — so a live node-by-node HaChu perft (as was
//! run for Chu/Dai) is unavailable here.
//!
//! Instead, mcr's Tenjiku start position is validated **directly against HaChu's own
//! source tables** — `variant.c`'s `tenjikuPieces` / `tenArray` / `tenIDs`, and the
//! `GenNonCapts` non-capture move semantics — reconciled move-for-move:
//!
//! * **perft(1) = 72** — every one of mcr's 72 start-position moves corresponds to a
//!   HaChu-table move node-for-node: the 14 unblocked Pawn pushes; the two advanced
//!   Dragon Kings' 17 moves each (34; each includes the file-4 / file-11 capture of
//!   the opposing Dragon King, the sole captures reachable at the start); the rank-4
//!   Horned Falcons' forward 2-jumps over the pawn wall and the Soaring Eagles'
//!   forward-diagonal 2-jumps (6); and the 18 short down/sideways moves of the
//!   back-three ranks into the four empty rank-2 squares (a HaChu array asymmetry
//!   faithfully reproduced). This pinned down the exact start placement.
//! * **perft(2) = 5663** and **perft(3) = 424582** are mcr regression pins (the
//!   jitto pass is emitted once per side, as HaChu tracks a single null move); they
//!   are **not** HaChu-cross-checked (HaChu crashes on the variant). Before the
//!   range-jumping Generals were modelled (issue #478) these read 5662 / 424195; the
//!   difference is a **single genuine jump-capture recapture that first becomes
//!   available at depth 2** (see below), plus its subtree. perft(1) — the
//!   HaChu-validated, all-non-capture start move set — is **unchanged at 72**, which
//!   is the true guard that the new generation never leaks into the non-capture
//!   path: no jump-capture exists in the start position itself (every General is
//!   walled by its own army and, jumping onto the empty rank beyond, has nothing to
//!   capture).
//!
//!   The lone new depth-2 node: White's free Dragon King e6→e11 (`(4,5)→(4,10)`,
//!   an ordinary Rook slide up the empty file) captures Black's free Dragon King;
//!   Black's Great General on `(7,13)` then **recaptures** by jumping over its own
//!   Rook General `(6,12)` and Pawn `(5,11)` — two consecutive lower-ranked friends —
//!   to land on the White Dragon at `(4,10)`. That single reply raises perft(2) by
//!   one (5662→5663); the extra depth-3 nodes (424195→424582) are its continuations
//!   plus the further recaptures that open once a capture clears a General's line.
//!   The precise jump-capture mechanics are hand-derived and pinned in the
//!   `Jump-capturing Generals` tests below.
//!
//! What is **not** modelled (see the module docs): a General's jump-*check* through a
//! screen is not in the attack model (king-safety uses the ordinary ride only), and a
//! Great General taken as a Lion double-capture's / Fire Demon burn's *secondary*
//! victim is not specially made immune.

use mcr::geometry::{perft, Square, Tenjiku, Tenjiku16x16};

/// The Tenjiku start position round-trips through mcr's FEN I/O (the `****`-dialect
/// fifth-tier tokens for the new pieces).
#[test]
fn startpos_round_trips() {
    let pos = Tenjiku::startpos();
    let fen = pos.to_fen();
    assert!(fen.starts_with(
        "l*n***l***u***csg**ekgs***c***u***l*nl/\
         ***r1****c****c1***t***pq***n***k***t1****c****c1***r/\
         ****s****lb+b+r****w****i****g****h****i****w+r+bb****l****s/\
         ***i***vr***h***e****b****r****v****g****r****b***e***hr***v***i/\
         pppppppppppppppp/4+r6+r4/16/16/16/16/4+R6+R4/PPPPPPPPPPPPPPPP/\
         ***I***VR***H***E****B****R****G****V****R****B***E***HR***V***I/\
         ****S****LB+B+R****W****I****H****E****I****W+R+BB****L****S/\
         ***R1****C****C***T***K***NQ***P***T1****C****C1***R1/\
         L*N***L***U***CSGK**EGS***C***U***L*NL w - - 0 1"
    ));
    // Full round-trip through parse.
    assert_eq!(Tenjiku::from_fen(&fen).expect("re-parse").to_fen(), fen);
}

/// Start-position perft. perft(1) = 72 is validated node-for-node against HaChu's
/// source tables (HaChu crashes on `variant tenjiku`, so a live oracle is
/// unavailable) and is **unchanged** by the range-jumping Generals (issue #478) —
/// no jump-capture exists in the start position, so the new generation provably does
/// not leak into the non-capture path. perft(2) / perft(3) are mcr regression pins;
/// they rose by exactly the one depth-2 jump-capture recapture (and its subtree)
/// documented in the module header (5662→5663, 424195→424582).
#[test]
fn startpos_perft_regression() {
    let pos = Tenjiku::startpos();
    assert_eq!(perft::<Tenjiku16x16, _>(&pos, 1), 72);
    assert_eq!(perft::<Tenjiku16x16, _>(&pos, 2), 5663);
    assert_eq!(perft::<Tenjiku16x16, _>(&pos, 3), 424582);
}

fn targets(fen: &str, file: u8, rank: u8) -> Vec<u8> {
    let pos = Tenjiku::from_fen(fen).expect("valid Tenjiku FEN");
    let from = Square::<Tenjiku16x16>::from_file_rank(file, rank).expect("on board");
    let mut v: Vec<u8> = pos
        .legal_moves()
        .iter()
        .filter(|m| m.from::<Tenjiku16x16>() == from)
        .map(|m| m.to::<Tenjiku16x16>().index())
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

fn idx(coords: &[(u8, u8)]) -> Vec<u8> {
    let mut v: Vec<u8> = coords
        .iter()
        .map(|&(f, r)| {
            Square::<Tenjiku16x16>::from_file_rank(f, r)
                .expect("on board")
                .index()
        })
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// An empty-ish 16x16 board with the two Kings placed **off every ray** from the
/// piece under test at h8 (file 7, rank 7) — White on b1, Black on o16 — so the
/// piece's ride is unobstructed to the board edges. `piece` is its FEN token.
fn lone(piece: &str) -> String {
    format!("14k1/16/16/16/16/16/16/16/7{piece}8/16/16/16/16/16/16/1K14 w - - 0 1")
}

/// A Water Buffalo: sideways and diagonal Rook/Bishop rides, plus one or two squares
/// straight forward/back.
#[test]
fn water_buffalo_moves() {
    let got = targets(&lone("****W"), 7, 7);
    let mut want: Vec<(u8, u8)> = Vec::new();
    // Horizontal Rook (blocked only by edges here).
    for f in (0..7).chain(8..16) {
        want.push((f, 7));
    }
    // Both full diagonals.
    for d in 1..16i16 {
        for (sf, sr) in [(1, 1), (-1, 1), (1, -1), (-1, -1)] {
            let (f, r) = (7 + sf * d, 7 + sr * d);
            if (0..16).contains(&f) && (0..16).contains(&r) {
                want.push((f as u8, r as u8));
            }
        }
    }
    // Range-2 vertical.
    for (f, r) in [(7, 8), (7, 9), (7, 6), (7, 5)] {
        want.push((f, r));
    }
    assert_eq!(got, idx(&want));
}

/// A Side Soldier: horizontal Rook, one step back, range-2 forward.
#[test]
fn side_soldier_moves() {
    let got = targets(&lone("****S"), 7, 7);
    let mut want: Vec<(u8, u8)> = Vec::new();
    for f in (0..7).chain(8..16) {
        want.push((f, 7));
    }
    want.push((7, 6)); // one step back
    want.push((7, 8)); // range-2 forward
    want.push((7, 9));
    assert_eq!(got, idx(&want));
}

/// The Horned Falcon's forward 2-square Lion-power reach jumps over an intervening
/// piece to the empty second square (the move that lets it clear a pawn wall at the
/// Tenjiku start).
#[test]
fn horned_falcon_jumps_two_forward_over_a_blocker() {
    // A friendly pawn one square ahead of the Horned Falcon (h9); the two-step
    // forward reach still lands on the empty h10. Kings off-ray (b1 / o16).
    let got = targets(
        "14k1/16/16/16/16/16/16/7P8/7***H8/16/16/16/16/16/16/1K14 w - - 0 1",
        7,
        7,
    );
    // h10 = (7, 9) must be reachable (the jump), even though h9 = (7, 8) is a
    // friendly blocker (so the single forward step is not available).
    let h10 = Square::<Tenjiku16x16>::from_file_rank(7, 9)
        .unwrap()
        .index();
    let h9 = Square::<Tenjiku16x16>::from_file_rank(7, 8)
        .unwrap()
        .index();
    assert!(got.contains(&h10), "Horned Falcon should jump to h10");
    assert!(!got.contains(&h9), "h9 is a friendly blocker");
}

/// The Soaring Eagle's forward-diagonal 2-square Lion reach jumps to the empty
/// second diagonal square over an intervening piece.
#[test]
fn soaring_eagle_jumps_two_forward_diagonally() {
    // Friendly pieces one diagonal step ahead (g9 and i9); the two-step diagonal
    // reach still lands on f10 and j10. Kings off-ray (b1 / o16).
    let got = targets(
        "14k1/16/16/16/16/16/16/6P1P7/7***E8/16/16/16/16/16/16/1K14 w - - 0 1",
        7,
        7,
    );
    let f10 = Square::<Tenjiku16x16>::from_file_rank(5, 9)
        .unwrap()
        .index();
    let j10 = Square::<Tenjiku16x16>::from_file_rank(9, 9)
        .unwrap()
        .index();
    assert!(got.contains(&f10) && got.contains(&j10));
}

/// Attacker / pin consistency on the 16x16 geometry: every legal move keeps the
/// moving side's King safe (a base sanity net on the new-piece attacker projection
/// and the make/unmake king-safety path, which Tenjiku rides as a multi-royal
/// variant).
#[test]
fn attackers_and_pins_consistency() {
    // Black Rook on h6 aimed down the h-file at the White King on h2, with a White
    // blocker Pawn between them on h4: the King is not in check, and no legal move
    // may expose it.
    let pos = Tenjiku::from_fen("15k/16/16/16/16/16/16/16/16/16/7r8/16/7P8/16/7K8/16 w - - 0 1")
        .expect("valid Tenjiku FEN");
    let king = Square::<Tenjiku16x16>::from_file_rank(7, 1).unwrap();
    assert!(!pos.is_attacked(king, mcr::Color::Black));
    for m in pos.legal_moves().iter() {
        let after = pos.play(m);
        let mut k = None;
        for f in 0..16 {
            for r in 0..16 {
                let sq = Square::<Tenjiku16x16>::from_file_rank(f, r).unwrap();
                if after.board().kings_of(mcr::Color::White).contains(sq) {
                    k = Some(sq);
                }
            }
        }
        if let Some(k) = k {
            assert!(
                !after.is_attacked(k, mcr::Color::Black),
                "move {} left the White King in check",
                m.to_uci::<Tenjiku16x16>()
            );
        }
    }
}

// ===========================================================================
// Fire Demon area-burn + igui (issue #477)
// ===========================================================================
//
// Tenjiku's Fire Demon moves as a Flying Ox and then **burns** (captures) every
// enemy on the up-to-eight squares adjacent to its destination, and may **igui**
// (burn in place without moving). There is **no machine oracle** for this: HaChu
// segfaults on `variant tenjiku` and, even where it runs, exercises captures only
// at shallow depth. So the burn is validated by **hand-derived perft** on small,
// fully-enumerable constructed positions, with the node counts derived below by
// hand and pinned here. The FEN dialect uses `****I` / `****i` for the Fire Demon,
// `P`/`p` for pawns, `K`/`k` for kings; rows run rank 16 (top) down to rank 1.

use mcr::geometry::WideRole;
use mcr::Color;

/// Applies the unique legal move with UCI string `uci` from `fen`, returning the
/// resulting position. Panics if no such legal move exists (so a test that names a
/// move the generator does not produce fails loudly).
fn after_move(fen: &str, uci: &str) -> Tenjiku {
    let pos = Tenjiku::from_fen(fen).expect("valid Tenjiku FEN");
    let mv = pos
        .parse_uci(uci)
        .unwrap_or_else(|| panic!("`{uci}` is not a legal move from {fen}"));
    pos.play(&mv)
}

/// `true` if `sq` (file, rank) holds an enemy (Black) piece in `pos`.
fn black_at(pos: &Tenjiku, file: u8, rank: u8) -> bool {
    let sq = Square::<Tenjiku16x16>::from_file_rank(file, rank).unwrap();
    matches!(pos.board().piece_at(sq), Some(p) if p.color == Color::Black)
}

/// `true` if `sq` (file, rank) holds a White piece of `role` in `pos`.
fn white_role_at(pos: &Tenjiku, file: u8, rank: u8, role: WideRole) -> bool {
    let sq = Square::<Tenjiku16x16>::from_file_rank(file, rank).unwrap();
    matches!(pos.board().piece_at(sq), Some(p) if p.color == Color::White && p.role == role)
}

/// **Hand-derived perft — igui multi-burn, displacement captures, friendly safety.**
///
/// Position (White to move):
/// ```text
///   White: Fire Demon h8=(7,7); King a1=(0,0); Pawns h7=(7,6), h9=(7,8).
///   Black: King a8=(0,7); Pawns g7=(6,6), i7=(8,6), g9=(6,8), i9=(8,8).
/// ```
/// The friendly Pawns on h7 / h9 wall the demon's vertical ride; the four Black
/// Pawns sit on its four diagonals. The Flying Ox rides vertically and diagonally
/// only (never sideways), so the demon's moves are exactly:
///
/// * the four diagonal **displacement captures** g7, i7, g9, i9 (each blocked
///   beyond the captured Pawn), and
/// * the **igui** h8→h8 (available because there is at least one adjacent enemy).
///
/// Vertical up/down are walled by the friendly Pawns; sideways is not a Flying Ox
/// direction. So the demon has **5** moves. The King has 3 (a2, b1, b2); the h9
/// Pawn has 1 (h10); the h7 Pawn is blocked by the demon (0). **perft(1) = 5 + 3 +
/// 1 = 9.**
///
/// For perft(2), Black always has its King's 5 corner-edge moves plus one move per
/// surviving Pawn (each Pawn's forward square is empty). A Black Pawn is removed
/// only when the demon's move burns / captures it:
///
/// * **igui h8h8** burns *all four* adjacent Black Pawns (the friendly h7/h9 Pawns
///   are **not** burned) → 0 Pawns survive → 5 + 0 = **5** replies.
/// * each **diagonal capture** (g7, i7, g9, i9) removes only the one displaced
///   Pawn — the burn around the landing square reaches no *other* Pawn (they are a
///   knight's-move apart) — so 3 survive → 5 + 3 = **8** replies, ×4 = 32.
/// * the 3 King moves and the h9→h10 Pawn move remove no Black Pawn → 4 survive →
///   5 + 4 = **9** replies, ×4 = 36.
///
/// **perft(2) = 5 + 32 + 36 = 73.** The igui's 5 vs. a quiet move's 9 is the burn's
/// fingerprint: the missing 4 replies are exactly the four burned Pawns' moves.
#[test]
fn fire_demon_igui_burns_all_adjacent_perft() {
    let fen = "16/16/16/16/16/16/16/6pPp7/k6****I8/6pPp7/16/16/16/16/16/K15 w - - 0 1";
    let pos = Tenjiku::from_fen(fen).expect("valid Tenjiku FEN");
    assert_eq!(perft::<Tenjiku16x16, _>(&pos, 1), 9);
    assert_eq!(perft::<Tenjiku16x16, _>(&pos, 2), 73);

    // The igui (from == to) burns all four diagonal Black Pawns but not the two
    // friendly Pawns walling the file.
    let after = after_move(fen, "h8h8");
    assert!(
        white_role_at(&after, 7, 7, WideRole::FireDemon),
        "demon stays on h8"
    );
    for (f, r) in [(6, 6), (8, 6), (6, 8), (8, 8)] {
        assert!(
            !black_at(&after, f, r),
            "adjacent enemy at ({f},{r}) must be burned"
        );
    }
    assert!(
        white_role_at(&after, 7, 6, WideRole::Pawn),
        "friendly h7 Pawn survives"
    );
    assert!(
        white_role_at(&after, 7, 8, WideRole::Pawn),
        "friendly h9 Pawn survives"
    );
    // A diagonal displacement capture removes only the displaced Pawn; the other
    // three (a knight's move from the landing square) are untouched.
    let after = after_move(fen, "h8g7");
    assert!(
        white_role_at(&after, 6, 6, WideRole::FireDemon),
        "demon lands on g7"
    );
    assert!(black_at(&after, 8, 6) && black_at(&after, 6, 8) && black_at(&after, 8, 8));
}

/// **Hand-derived perft — arrival burn onto an *empty* square.**
///
/// Position (White to move):
/// ```text
///   White: Fire Demon h8=(7,7); King a1=(0,0);
///          Pawns g7=(6,6), h7=(7,6), i7=(8,6) (wall the down/​down-diagonals),
///          Pawn h10=(7,9) (walls the file above h9).
///   Black: King a8=(0,7); Pawns g9=(6,8), i9=(8,8).
/// ```
/// The demon's moves are: vertical-up to the **empty** h9=(7,8) (walled beyond by
/// the friendly h10 Pawn); the two diagonal **captures** g9, i9; and the **igui**
/// h8→h8. That is **4** demon moves. Plus 3 King moves, and 3 Pawn moves (g7→g8,
/// i7→i8, h10→h11; the h7 Pawn is blocked by the demon). **perft(1) = 4 + 3 + 3 =
/// 10.**
///
/// The load-bearing move is **h8→h9**: the demon slides onto the *empty* h9 and its
/// arrival burn removes **both** g9 and i9 — neither of which is the landing square,
/// proving the burn captures non-displaced adjacent enemies. Black then has only its
/// King (5 replies). By contrast:
///
/// * **h8g9** captures g9 by displacement; i9 (a knight's move from g9) survives → 6.
/// * **h8i9** captures i9; g9 survives → 6.
/// * **igui h8h8** burns both g9 and i9 → 5.
/// * the 3 King and 3 Pawn moves remove no Black Pawn → both survive → 7 each = 42.
///
/// **perft(2) = 5 + 6 + 6 + 5 + 42 = 64.**
#[test]
fn fire_demon_arrival_burn_on_empty_square_perft() {
    let fen = "16/16/16/16/16/16/7P8/6p1p7/k6****I8/6PPP7/16/16/16/16/16/K15 w - - 0 1";
    let pos = Tenjiku::from_fen(fen).expect("valid Tenjiku FEN");
    assert_eq!(perft::<Tenjiku16x16, _>(&pos, 1), 10);
    assert_eq!(perft::<Tenjiku16x16, _>(&pos, 2), 64);

    // Sliding onto the empty h9 burns both g9 and i9 (arrival burn, no displacement).
    let after = after_move(fen, "h8h9");
    assert!(
        white_role_at(&after, 7, 8, WideRole::FireDemon),
        "demon lands on h9"
    );
    assert!(!black_at(&after, 6, 8), "g9 must be burned on arrival");
    assert!(!black_at(&after, 8, 8), "i9 must be burned on arrival");
}

/// The area burn truncates correctly at a board **corner**: a Fire Demon on a1 has
/// only three on-board neighbours (b1, a2, b2), so its igui burns exactly those
/// enemies and reaches no phantom off-board square.
#[test]
fn fire_demon_corner_igui_truncates_to_three_neighbours() {
    // White Fire Demon a1=(0,0), King p1=(15,0) off every ray. Black King p16, and
    // three Black Pawns on the demon's only neighbours b1=(1,0), a2=(0,1), b2=(1,1).
    let fen = "15k/16/16/16/16/16/16/16/16/16/16/16/16/16/pp14/****Ip13K w - - 0 1";
    let after = after_move(fen, "a1a1");
    assert!(
        white_role_at(&after, 0, 0, WideRole::FireDemon),
        "demon stays on a1"
    );
    for (f, r) in [(1, 0), (0, 1), (1, 1)] {
        assert!(
            !black_at(&after, f, r),
            "corner-adjacent enemy ({f},{r}) must be burned"
        );
    }
}

/// `size_of::<WideMove>() == 8` still holds after adding the `FireDemonMove` kind
/// (it reuses a spare 4-bit tag code and carries no new addendum).
#[test]
fn wide_move_is_eight_bytes_with_fire_demon() {
    assert_eq!(core::mem::size_of::<mcr::geometry::WideMove>(), 8);
}

// ---------------------------------------------------------------------------
// Jump-capturing Generals (issue #478)
// ---------------------------------------------------------------------------
//
// The four range-jumping Generals (Great / Vice / Rook / Bishop) slide as their
// base piece and, **when capturing**, may jump over any number of *consecutive*
// strictly-lower-ranked pieces (friend or foe) to capture an enemy beyond, stopped
// by the first equal-or-higher-ranked piece. The Great General is un-capturable
// except by another Great General. The rank hierarchy is King/Prince = 4, Great
// General = 3, Vice General = 2, Rook/Bishop General = 1, everything else = 0.
//
// There is **no machine oracle** (HaChu crashes on Tenjiku), so these are validated
// by hand-derived perft on constructed positions and by exact move-target sets. The
// FEN dialect writes the Generals `****R/G/V/B` (White) and `****r/g/v/b` (Black).

/// Convenience: the board index of square `(file, rank)`.
fn sq_index(file: u8, rank: u8) -> u8 {
    Square::<Tenjiku16x16>::from_file_rank(file, rank)
        .expect("on board")
        .index()
}

/// **Hand-derived perft — a Rook General jumping one lower-ranked piece.**
///
/// ```text
///   White: Rook General a1=(0,0); King p16=(15,15).
///   Black: King h16=(7,15); Pawn a2=(0,1); Rook a3=(0,2).
/// ```
/// The Rook General sits in the corner, so it has only two rays. **Up the a-file**:
/// the ordinary ride is blocked by the Black Pawn on a2, which it captures (1 move);
/// the Pawn (rank 0 < 1) is then **jumped**, landing on the Black Rook a3 — a
/// jump-capture (1 move). The Rook a3 is itself lower-ranked, but the square beyond
/// it (a4) is empty, so the consecutive run ends and the ride stops: a-file = 2
/// moves. **Along rank 1**: b1…p1 are 15 empty quiet moves. So the Rook General has
/// `2 + 15 = 17` moves. The King (p16 corner) has 3 (o16, o15, p15). Neither Black
/// slider bears on the White King, so every move is legal. **perft(1) = 17 + 3 = 20**
/// (exactly one more than the 19 of an ordinary blockable Rook, whose a-file stops at
/// a2 — the `+1` is the jump-capture of a3).
#[test]
fn jump_general_single_jump_perft() {
    let fen = "7k7K/16/16/16/16/16/16/16/16/16/16/16/16/r15/p15/****R15 w - - 0 1";
    let pos = Tenjiku::from_fen(fen).expect("valid Tenjiku FEN");
    assert_eq!(perft::<Tenjiku16x16, _>(&pos, 1), 20);

    // The a-file targets are exactly a2 (ordinary capture) and a3 (jump-capture).
    let got = targets(fen, 0, 0);
    assert!(
        got.contains(&sq_index(0, 1)),
        "captures the Pawn a2 (ordinary)"
    );
    assert!(
        got.contains(&sq_index(0, 2)),
        "jump-captures the Rook a3 over the Pawn"
    );
    // The jump-capture actually removes only its landing square: a2 (the jumped
    // Pawn) survives, a3 (the victim) is gone, the General stands on a3.
    let after = after_move(fen, "a1a3");
    assert!(white_role_at(&after, 0, 2, WideRole::RookGeneral));
    assert!(black_at(&after, 0, 1), "the jumped Pawn a2 is not captured");
}

/// **Hand-derived perft — a Rook General jumping a run of three lower-ranked
/// pieces.** Position as above but the a-file holds Pawns on a2, a3, a4 and a Rook on
/// a5, all Black.
///
/// Up the a-file the General captures the first blocker a2 (ordinary), then
/// jump-captures each further enemy across the consecutive lower-ranked run: a3
/// (over a2), a4 (over a2, a3) and a5 (over a2, a3, a4) — 4 captures. Rank 1 adds 15
/// quiets, so the General has 19 moves; the King has 3. **perft(1) = 22.**
#[test]
fn jump_general_over_three_pieces_perft() {
    let fen = "7k7K/16/16/16/16/16/16/16/16/16/16/r15/p15/p15/p15/****R15 w - - 0 1";
    let pos = Tenjiku::from_fen(fen).expect("valid Tenjiku FEN");
    assert_eq!(perft::<Tenjiku16x16, _>(&pos, 1), 22);

    let got = targets(fen, 0, 0);
    for r in 1..=4 {
        assert!(
            got.contains(&sq_index(0, r)),
            "the General reaches a{} across the run",
            r + 1
        );
    }
}

/// **The jump stops at — and may capture — the first equal-or-higher-ranked piece,
/// but goes no further.**
///
/// ```text
///   White: Rook General a2=(0,1); Pawn a3=(0,2) (friendly); King p1=(15,0).
///   Black: Vice General a4=(0,3); Rook a5=(0,4); King h16=(7,15).
/// ```
/// Up the a-file the Rook General jumps its own Pawn on a3 (rank 0), reaches the
/// Black **Vice General** on a4 (rank 2 ≥ the mover's 1): it may capture it by
/// landing on it — but the Vice General is an opaque wall, so the ride stops there.
/// The Rook a5 beyond it is **not** reachable.
#[test]
fn jump_general_captures_higher_rank_then_stops() {
    let fen = "7k7K/16/16/16/16/16/16/16/16/16/16/r15/****v15/P15/****R15/15K w - - 0 1";
    let got = targets(fen, 0, 1);
    assert!(
        got.contains(&sq_index(0, 3)),
        "jump-captures the higher-ranked Vice General a4"
    );
    assert!(
        !got.contains(&sq_index(0, 4)),
        "cannot pass the Vice General to reach the Rook a5"
    );
    assert!(
        !got.contains(&sq_index(0, 2)),
        "does not capture its own screening Pawn a3"
    );
}

/// **A range-jumping General cannot jump-capture the immune Great General.** As above
/// but the piece beyond the friendly Pawn screen is a Black **Great General**.
#[test]
fn range_jumper_cannot_capture_great_general() {
    let fen = "7k7K/16/16/16/16/16/16/16/16/16/16/16/****g15/P15/****R15/15K w - - 0 1";
    let got = targets(fen, 0, 1);
    assert!(
        !got.contains(&sq_index(0, 3)),
        "the Rook General may not take the immune Great General"
    );
}

/// **The Great General is immune to an ordinary slider too.** A White Rook on c3 has
/// a Black Great General on a3 directly along its rank; occupancy still blocks the
/// slide there, but the capture is forbidden — the Rook reaches b3 and stops.
#[test]
fn great_general_immune_to_ordinary_rook() {
    let fen = "7k7K/16/16/16/16/16/16/16/16/16/16/16/16/****g1R13/16/K15 w - - 0 1";
    let got = targets(fen, 2, 2);
    assert!(got.contains(&sq_index(1, 2)), "the Rook may step to b3");
    assert!(
        !got.contains(&sq_index(0, 2)),
        "the Rook may not capture the Great General a3"
    );
}

/// **Only a Great General captures a Great General** — both an adjacent ordinary
/// capture and a jump-capture across a friendly screen are permitted for the mover.
#[test]
fn great_general_captures_great_general() {
    // Adjacent: White Great General b3 next to Black Great General a3.
    let adj = "7k7K/16/16/16/16/16/16/16/16/16/16/16/16/****g****G14/16/15K w - - 0 1";
    assert!(
        targets(adj, 1, 2).contains(&sq_index(0, 2)),
        "a Great General captures the adjacent enemy Great General"
    );
    // Across a screen: White Great General a2, own Pawn a3, Black Great General a4.
    let jump = "7k7K/16/16/16/16/16/16/16/16/16/16/16/****g15/P15/****G15/15K w - - 0 1";
    assert!(
        targets(jump, 0, 1).contains(&sq_index(0, 3)),
        "a Great General jump-captures an enemy Great General over a screen"
    );
}
