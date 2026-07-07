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
//! * **perft(2) = 5663** and **perft(3) = 424582** were, before issue #500, mcr
//!   regression pins **not** cross-checked by any second source (HaChu crashes on the
//!   variant), i.e. self-referential. They are now cross-checked **node-for-node
//!   against a fully independent, from-scratch brute-force generator** (the `brute`
//!   module at the foot of this file — its own 16x16 array model, movement,
//!   range-jumping Generals, lion-style promotion, Lion multi-step and multi-royal
//!   king safety, sharing no code with the production generator), so both pins are
//!   now real cross-oracle counts. (The jitto pass is emitted once per side, as HaChu
//!   tracks a single null move; the independent generator reproduces that too.)
//!   Before the range-jumping Generals were modelled (issue #478) these read
//!   5662 / 424195; the
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
//! Now modelled (issue #491, previously deferred): a General's jump-*check* through a
//! screen **is** in the attack model (king-safety folds the jump into the royal-attack
//! query, so a move leaving one's own king in a jump-check is illegal and a jump-check
//! must be answered), and a Great General removed as a Lion double-capture's / Fire
//! Demon burn's *secondary* victim **is** made immune. Both are hand-derived (HaChu
//! segfaults on Tenjiku) — see the `Jump-general check detection` and
//! `Great-General secondary-victim immunity` tests below.

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
/// not leak into the non-capture path. perft(2) / perft(3) are additionally
/// cross-checked node-for-node against the independent brute-force generator (issue
/// #500; see `engine_matches_independent_brute_force_depth2` / `_depth3`), so they
/// are no longer self-referential. They rose from the pre-#478 counts by exactly the
/// one depth-2 jump-capture recapture (and its subtree): 5662→5663, 424195→424582.
#[test]
fn startpos_perft_regression() {
    let pos = Tenjiku::startpos();
    assert_eq!(perft::<Tenjiku16x16, _, _>(&pos, 1), 72);
    assert_eq!(perft::<Tenjiku16x16, _, _>(&pos, 2), 5663);
    assert_eq!(perft::<Tenjiku16x16, _, _>(&pos, 3), 424582);
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
    assert_eq!(perft::<Tenjiku16x16, _, _>(&pos, 1), 9);
    assert_eq!(perft::<Tenjiku16x16, _, _>(&pos, 2), 73);

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
    assert_eq!(perft::<Tenjiku16x16, _, _>(&pos, 1), 10);
    assert_eq!(perft::<Tenjiku16x16, _, _>(&pos, 2), 64);

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
    assert_eq!(perft::<Tenjiku16x16, _, _>(&pos, 1), 20);

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
    assert_eq!(perft::<Tenjiku16x16, _, _>(&pos, 1), 22);

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

// ---------------------------------------------------------------------------
// Jump-general **check detection** (issue #491, deferred from #478)
// ---------------------------------------------------------------------------
//
// King-safety folds the range-jumping General's *jump* into the attack model: a
// General giving check by jumping over a screen (a consecutive run of strictly
// lower-ranked pieces) is now seen, so a move that leaves its own king in a
// jump-check is illegal and a jump-check must be answered. The ordinary slide
// (`attackers_to`) is blocked by the screen and cannot see the king, so these
// checks are *only* visible through the new forward jump scan — the tests below
// use exactly that isolation: `is_attacked` (ordinary) is `false` while
// `is_check` (jump-aware) is `true`. No machine oracle (HaChu segfaults on
// Tenjiku); the positions and counts are hand-derived.

/// **A Rook General jump-checks the enemy king through a screen.**
///
/// ```text
///   White: King a1=(0,0); Pawn a2=(0,1) (the screen the check jumps over).
///   Black: Rook General a3=(0,2); King p16=(15,15).
/// ```
/// The Black Rook General slides down the a-file; its **ordinary** ride is stopped
/// by the White Pawn on a2 (so the plain slider scan cannot see the King). But
/// jumping that single lower-ranked screen lands it on the King a1 — a **jump-check**.
/// So the ordinary `is_attacked(a1)` is `false` while `is_check()` is `true`: the
/// King is in check and must respond. Its only replies are the Pawn's straight
/// capture of the checker (a2×a3), and the two king steps b1 / b2 (neither on the
/// General's file or rank) — **perft(1) = 3**. (Before #491 the jump-check was
/// invisible, but this position happens to have the same 3 replies; the load-bearing
/// assertion is that `is_check()` now fires.)
#[test]
fn jump_general_delivers_check_over_a_screen() {
    let fen = "15k/16/16/16/16/16/16/16/16/16/16/16/16/****r15/P15/K15 w - - 0 1";
    let pos = Tenjiku::from_fen(fen).expect("valid Tenjiku FEN");
    let king = Square::<Tenjiku16x16>::from_file_rank(0, 0).unwrap();
    // The ordinary slider scan is blocked by the screen Pawn — it cannot see the
    // King — yet the jump-aware royal query reports the check.
    assert!(
        !pos.is_attacked(king, Color::Black),
        "the ordinary ride is blocked by the a2 screen"
    );
    assert!(
        pos.is_check(),
        "the Rook General jump-checks the King over the a2 screen"
    );
    assert_eq!(perft::<Tenjiku16x16, _, _>(&pos, 1), 3);
    // Every legal reply leaves the King out of the jump-check.
    for m in pos.legal_moves().iter() {
        let after = pos.play(m);
        assert!(
            !after.is_in_check(Color::White),
            "reply {} must resolve the jump-check",
            m.to_uci::<Tenjiku16x16>()
        );
    }
}

/// **A king may not step into a jump-check** (and the move is otherwise legal by the
/// ordinary attack model, so only #491 forbids it).
///
/// ```text
///   White: King c3=(2,2); Pawn d2=(3,1) (a screen).
///   Black: Rook General e2=(4,1); King p16=(15,15).
/// ```
/// The White King is **not** in check on c3. Its neighbour c2=(2,1) is unattacked by
/// the ordinary ride — the General's rank-1 slide is stopped by the d2 screen at
/// d2 — but the General **jumps** that screen to reach c2, so stepping the King
/// c3→c2 lands it in a jump-check and is illegal. The King's other seven neighbours:
/// d2 is a friendly Pawn (blocked), c2 is the forbidden jump-check square, and
/// b2 / b3 / d3 / b4 / c4 / d4 are all safe (b2 is *not* jump-attacked — after the
/// General jumps d2 the run ends on the empty c2, so it never reaches b2). That is
/// **6** king moves; the Pawn adds its one forward push d2→d3. **perft(1) = 7**
/// (before #491 the King could also step into c2, giving 8).
#[test]
fn king_may_not_step_into_a_jump_check() {
    let fen = "15k/16/16/16/16/16/16/16/16/16/16/16/16/2K13/3P****r11/16 w - - 0 1";
    let pos = Tenjiku::from_fen(fen).expect("valid Tenjiku FEN");
    assert!(!pos.is_check(), "the King is not in check on c3");
    assert_eq!(perft::<Tenjiku16x16, _, _>(&pos, 1), 7);
    let king_targets = targets(fen, 2, 2);
    assert!(
        !king_targets.contains(&sq_index(2, 1)),
        "the King may not step onto c2 — a jump-check square"
    );
    assert!(
        king_targets.contains(&sq_index(1, 1)),
        "b2 is safe (the jump run ends on the empty c2 and never reaches b2)"
    );
    assert!(
        king_targets.contains(&sq_index(3, 2)) && king_targets.contains(&sq_index(2, 3)),
        "d3 / c4 are ordinary safe king steps"
    );
}

// ---------------------------------------------------------------------------
// Great-General secondary-victim immunity (issue #491, deferred from #478)
// ---------------------------------------------------------------------------
//
// The Great General is un-capturable except by another Great General. #478 enforced
// that for the ordinary slide and the range-jump; #491 extends it to the two
// *separate* capture paths that bypass that mask: a Fire Demon area-burn and a Lion
// multi-step (double-capture / igui) may **not** remove a Great General. Hand-derived
// (no Tenjiku oracle).

/// **A Fire Demon area-burn does not burn an adjacent Great General**, and the demon
/// cannot land on one either.
///
/// ```text
///   White: Fire Demon h8=(7,7); King a1=(0,0).
///   Black: Great General g9=(6,8); Pawn g7=(6,6); King p16=(15,15).
/// ```
/// Both the Great General (up-left diagonal) and the Pawn (down-left diagonal) are
/// adjacent to the demon. The **igui** h8→h8 burns every adjacent *burnable* enemy:
/// the Pawn g7 is burned, but the immune Great General g9 **survives**. And because
/// the General sits on the demon's up-left ride, the demon may not land on it (a
/// displacement capture is forbidden too): g9 is not a Fire-Demon target.
#[test]
fn fire_demon_burn_spares_great_general() {
    let fen = "15k/16/16/16/16/16/16/6****g9/7****I8/6p9/16/16/16/16/16/K15 w - - 0 1";
    let pos = Tenjiku::from_fen(fen).expect("valid Tenjiku FEN");
    // The demon may not land on the immune Great General (displacement capture).
    assert!(
        !targets(fen, 7, 7).contains(&sq_index(6, 8)),
        "the Fire Demon may not displace-capture the Great General g9"
    );
    // Igui burns the adjacent Pawn but spares the immune Great General.
    let after = after_move(fen, "h8h8");
    assert!(
        white_role_at(&after, 7, 7, WideRole::FireDemon),
        "demon stays on h8"
    );
    assert!(!black_at(&after, 6, 6), "the adjacent Pawn g7 is burned");
    let gg = Square::<Tenjiku16x16>::from_file_rank(6, 8).unwrap();
    assert!(
        matches!(pos.board().piece_at(gg), Some(p) if p.role == WideRole::GreatGeneral),
        "sanity: the Great General is on g9 in the start position"
    );
    assert!(
        matches!(after.board().piece_at(gg), Some(p) if p.color == Color::Black && p.role == WideRole::GreatGeneral),
        "the immune Great General g9 is NOT burned by the area burn"
    );
    // No legal Fire-Demon (or any) move ever removes the Great General.
    for m in pos.legal_moves().iter() {
        let played = pos.play(m);
        assert!(
            matches!(played.board().piece_at(gg), Some(p) if p.role == WideRole::GreatGeneral),
            "move {} must not remove the immune Great General",
            m.to_uci::<Tenjiku16x16>()
        );
    }
}

/// **A Lion multi-step (double-capture / igui) does not remove a Great General.**
///
/// ```text
///   White: Lion h8=(7,7); King a1=(0,0).
///   Black: Great General g8=(6,7) (adjacent); King p16=(15,15).
/// ```
/// The Lion stands next to the immune Great General. Its ordinary single step onto
/// g8 is already forbidden by the #478 immunity mask, and #491 additionally suppresses
/// the Lion's **igui** (step onto g8 and return) and any **double-step** whose
/// intermediate square is g8 — so no Lion move removes the Great General. Every legal
/// move therefore leaves it on the board.
#[test]
fn lion_cannot_capture_great_general() {
    let fen = "15k/16/16/16/16/16/16/16/6****g***N8/16/16/16/16/16/16/K15 w - - 0 1";
    let pos = Tenjiku::from_fen(fen).expect("valid Tenjiku FEN");
    let gg = Square::<Tenjiku16x16>::from_file_rank(6, 7).unwrap();
    assert!(
        matches!(pos.board().piece_at(gg), Some(p) if p.role == WideRole::GreatGeneral),
        "sanity: the Great General is adjacent to the Lion"
    );
    // The immune General's square is not a capture target of the Lion.
    assert!(
        !targets(fen, 7, 8).contains(&sq_index(6, 7)),
        "the Lion may not step onto the immune Great General"
    );
    // No legal move (igui, double-step, or otherwise) removes the Great General.
    let mut moves = 0;
    for m in pos.legal_moves().iter() {
        moves += 1;
        let after = pos.play(m);
        assert!(
            matches!(after.board().piece_at(gg), Some(p) if p.role == WideRole::GreatGeneral),
            "move {} must not remove the immune Great General",
            m.to_uci::<Tenjiku16x16>()
        );
    }
    assert!(moves > 0, "the Lion has legal moves to exercise");
}

// ===========================================================================
// Independent cross-oracle (issue #500)
// ===========================================================================
//
// HaChu **segfaults deterministically** on `variant tenjiku` (confirmed live:
// `compare-fairy --hachu` prints the crash and skips), so the start-position
// perft(2) / perft(3) counts were previously *self-referential* mcr regression
// pins — nothing independent produced them. This section adds a **second,
// from-scratch move generator** (`brute`) — a naive array-based 16x16 model with
// its own movement, promotion, range-jumping-General and multi-royal king-safety
// logic, sharing **no code** with the production generator — and cross-checks the
// pinned node counts against it. Two independent generators agreeing node-for-node
// is the substitute for the missing engine oracle.
//
// The two generators share only the **initial piece layout** (the brute seeds its
// board from `Tenjiku::startpos()`), not any move logic: the placement itself is
// separately validated node-for-node against HaChu's own source tables at
// perft(1) = 72 (see the module header). Everything that produces the *counts* —
// move generation, captures, the jump-capture recapture that distinguishes
// 5663 from the pre-#478 5662, promotion into the zone, and king safety — is
// re-derived independently in `brute` below.

use mcr::geometry::WideRole as WR;

/// Depth to which the independent brute-force generator cross-checks the engine in
/// the cheap (non-`#[ignore]`d) test. perft(2) = 5663 is the load-bearing upgrade:
/// it turns the self-referential regression pin into a real cross-oracle (the one
/// depth-2 jump-capture recapture is reproduced by the independent jump-General
/// logic). perft(3) is cross-checked in the `#[ignore]`d release test below.
#[test]
fn engine_matches_independent_brute_force_depth2() {
    let engine = Tenjiku::startpos();
    let bf = brute::Position::startpos();
    for depth in 1..=2 {
        let e = perft::<Tenjiku16x16, _, _>(&engine, depth);
        let b = brute::perft(&bf, depth);
        assert_eq!(
            e, b,
            "engine vs independent brute-force Tenjiku perft({depth}) disagree: {e} vs {b}"
        );
    }
    // The independent generator's own literal counts (no engine on the RHS).
    assert_eq!(brute::perft(&bf, 1), 72);
    assert_eq!(brute::perft(&bf, 2), 5663);
}

/// Independent cross-check of the deep pin perft(3) = 424582. `#[ignore]`d because
/// the naive generator walks ~424k nodes (tractable only in release): run with
/// `cargo test --release --test perft_tenjiku -- --ignored`.
#[test]
#[ignore = "deep independent cross-check; run with --release -- --ignored"]
fn engine_matches_independent_brute_force_depth3() {
    let engine = Tenjiku::startpos();
    let bf = brute::Position::startpos();
    assert_eq!(perft::<Tenjiku16x16, _, _>(&engine, 3), 424582);
    assert_eq!(brute::perft(&bf, 3), 424582);
}

/// A fully independent, naive, array-based Tenjiku Shogi move generator and perft,
/// written from scratch to cross-validate the engine's start-position perft without
/// a machine oracle (HaChu segfaults on Tenjiku). It re-derives every piece's
/// movement from the documented Betza / source-table definitions in a uniform
/// `(df, dr, mode)` step encoding deliberately unlike the engine's bitboard
/// helpers, and implements the range-jumping Generals (with the Great General's
/// capture immunity), the lion-style promotion-on-zone-entry rule, the Fire Demon's
/// Flying-Ox ride, and multi-royal (King + Prince) king-safety with jump-check
/// detection. The Lion multi-step / Fire-Demon area-burn moves do **not** arise in
/// the start-position tree to the cross-checked depths (verified against the engine
/// move-kind histogram), so — exactly as the Alice brute force omits castling /
/// en passant — they are intentionally left out; the moves that *do* arise are
/// enumerated identically to the engine.
mod brute {
    use super::{Square, Tenjiku, Tenjiku16x16, WR};
    use mcr::Color;

    const N: i32 = 16;

    // --- role bytes (own enumeration) -------------------------------------
    const KING: u8 = 0;
    const PRINCE: u8 = 1;
    const GOLD: u8 = 2;
    const SILVER: u8 = 3;
    const COPPER: u8 = 4;
    const IRON: u8 = 5;
    const LEOPARD: u8 = 6;
    const BTIGER: u8 = 7;
    const DELEPHANT: u8 = 8;
    const GOBETWEEN: u8 = 9;
    const PAWN: u8 = 10;
    const KNIGHT: u8 = 11;
    const DOG: u8 = 12;
    const KIRIN: u8 = 13;
    const PHOENIX: u8 = 14;
    const ROOK: u8 = 15;
    const BISHOP: u8 = 16;
    const QUEEN: u8 = 17;
    const GGENERAL: u8 = 18;
    const FEAGLE: u8 = 19;
    const RGENERAL: u8 = 20;
    const VGENERAL: u8 = 21;
    const BGENERAL: u8 = 22;
    const DRAGON: u8 = 23;
    const DHORSE: u8 = 24;
    const LANCE: u8 = 25;
    const RCHARIOT: u8 = 26;
    const SIDEMOVER: u8 = 27;
    const VMOVER: u8 = 28;
    const WHORSE: u8 = 29;
    const WHALE: u8 = 30;
    const FSTAG: u8 = 31;
    const FOX: u8 = 32; // Flying Ox
    const FDEMON: u8 = 33;
    const FBOAR: u8 = 34;
    const CSOLDIER: u8 = 35;
    const HTETRARCH: u8 = 36;
    const WBUFFALO: u8 = 37;
    const VSOLDIER: u8 = 38;
    const SSOLDIER: u8 = 39;
    const MGENERAL: u8 = 40;
    const CLION: u8 = 41;
    const LHAWK: u8 = 42;
    const HFALCON: u8 = 43;
    const SEAGLE: u8 = 44;

    /// Map a production `WideRole` to the brute role byte (layout only, never move
    /// logic). Panics on a role that cannot appear in the cross-checked tree.
    fn from_wide(role: WR) -> u8 {
        match role {
            WR::King => KING,
            WR::CrownPrince => PRINCE,
            WR::Gold => GOLD,
            WR::Silver => SILVER,
            WR::CopperGeneral => COPPER,
            WR::IronGeneral => IRON,
            WR::FerociousLeopard => LEOPARD,
            WR::BlindTiger => BTIGER,
            WR::DrunkElephant => DELEPHANT,
            WR::GoBetween => GOBETWEEN,
            WR::Pawn => PAWN,
            WR::ShogiKnight => KNIGHT,
            WR::Dog => DOG,
            WR::Kirin => KIRIN,
            WR::Phoenix => PHOENIX,
            WR::Rook => ROOK,
            WR::Bishop => BISHOP,
            WR::Queen => QUEEN,
            WR::GreatGeneral => GGENERAL,
            WR::FreeEagle => FEAGLE,
            WR::RookGeneral => RGENERAL,
            WR::ViceGeneral => VGENERAL,
            WR::BishopGeneral => BGENERAL,
            WR::Dragon => DRAGON,
            WR::DragonHorse => DHORSE,
            WR::Lance => LANCE,
            WR::ReverseChariot => RCHARIOT,
            WR::SideMover => SIDEMOVER,
            WR::VerticalMover => VMOVER,
            WR::WhiteHorse => WHORSE,
            WR::Whale => WHALE,
            WR::FlyingStag => FSTAG,
            WR::FlyingOx => FOX,
            WR::FireDemon => FDEMON,
            WR::FreeBoar => FBOAR,
            WR::ChariotSoldier => CSOLDIER,
            WR::HeavenlyTetrarch => HTETRARCH,
            WR::WaterBuffalo => WBUFFALO,
            WR::VerticalSoldier => VSOLDIER,
            WR::SideSoldier => SSOLDIER,
            WR::MultiGeneral => MGENERAL,
            WR::ChuLion => CLION,
            WR::LionHawk => LHAWK,
            WR::HornedFalcon => HFALCON,
            WR::SoaringEagle => SEAGLE,
            other => panic!("unexpected Tenjiku role in start tree: {other:?}"),
        }
    }

    // --- movement descriptors ---------------------------------------------
    // A single White-orientation step `(df, dr)` with a mode:
    //   0 = leap (single jump, ignores blockers),
    //   1 = ride (slide until first blocker, inclusive),
    //   2 = ride at most 2 squares.
    #[derive(Clone, Copy)]
    struct D {
        df: i8,
        dr: i8,
        mode: u8,
    }
    const fn l(df: i8, dr: i8) -> D {
        D { df, dr, mode: 0 }
    }
    const fn r(df: i8, dr: i8) -> D {
        D { df, dr, mode: 1 }
    }
    const fn r2(df: i8, dr: i8) -> D {
        D { df, dr, mode: 2 }
    }

    // Shared direction groups.
    const KING8: [D; 8] = [
        l(1, 0),
        l(-1, 0),
        l(0, 1),
        l(0, -1),
        l(1, 1),
        l(1, -1),
        l(-1, 1),
        l(-1, -1),
    ];

    /// The 24 Lion within-two offsets (Chebyshev distance 1 or 2), as leaps.
    const LION24: [D; 24] = [
        l(-1, -1),
        l(0, -1),
        l(1, -1),
        l(-1, 0),
        l(1, 0),
        l(-1, 1),
        l(0, 1),
        l(1, 1),
        l(-2, -2),
        l(-1, -2),
        l(0, -2),
        l(1, -2),
        l(2, -2),
        l(-2, -1),
        l(2, -1),
        l(-2, 0),
        l(2, 0),
        l(-2, 1),
        l(2, 1),
        l(-2, 2),
        l(-1, 2),
        l(0, 2),
        l(1, 2),
        l(2, 2),
    ];

    // Per-role descriptor tables (White orientation), re-derived from the documented
    // Tenjiku movement. Declared as `const` items so `moves` hands back `'static`
    // slices without allocating.
    const M_GOLD: [D; 6] = [l(1, 0), l(-1, 0), l(0, 1), l(0, -1), l(1, 1), l(-1, 1)];
    const M_SILVER: [D; 5] = [l(0, 1), l(1, 1), l(-1, 1), l(1, -1), l(-1, -1)];
    const M_COPPER: [D; 4] = [l(0, 1), l(1, 1), l(-1, 1), l(0, -1)];
    const M_IRON: [D; 3] = [l(0, 1), l(1, 1), l(-1, 1)];
    const M_LEOPARD: [D; 6] = [l(0, 1), l(1, 1), l(-1, 1), l(0, -1), l(1, -1), l(-1, -1)];
    const M_BTIGER: [D; 7] = [
        l(0, -1),
        l(1, 0),
        l(-1, 0),
        l(1, 1),
        l(-1, 1),
        l(1, -1),
        l(-1, -1),
    ];
    const M_DELEPHANT: [D; 7] = [
        l(0, 1),
        l(1, 0),
        l(-1, 0),
        l(1, 1),
        l(-1, 1),
        l(1, -1),
        l(-1, -1),
    ];
    const M_GOBETWEEN: [D; 2] = [l(0, 1), l(0, -1)];
    const M_PAWN: [D; 1] = [l(0, 1)];
    const M_KNIGHT: [D; 2] = [l(1, 2), l(-1, 2)];
    const M_DOG: [D; 3] = [l(0, 1), l(1, -1), l(-1, -1)];
    const M_KIRIN: [D; 8] = [
        l(0, 2),
        l(0, -2),
        l(2, 0),
        l(-2, 0),
        l(1, 1),
        l(1, -1),
        l(-1, 1),
        l(-1, -1),
    ];
    const M_PHOENIX: [D; 8] = [
        l(2, 2),
        l(2, -2),
        l(-2, 2),
        l(-2, -2),
        l(1, 0),
        l(-1, 0),
        l(0, 1),
        l(0, -1),
    ];
    const M_ROOK: [D; 4] = [r(1, 0), r(-1, 0), r(0, 1), r(0, -1)];
    const M_BISHOP: [D; 4] = [r(1, 1), r(1, -1), r(-1, 1), r(-1, -1)];
    const M_QUEEN: [D; 8] = [
        r(1, 0),
        r(-1, 0),
        r(0, 1),
        r(0, -1),
        r(1, 1),
        r(1, -1),
        r(-1, 1),
        r(-1, -1),
    ];
    const M_DRAGON: [D; 8] = [
        r(1, 0),
        r(-1, 0),
        r(0, 1),
        r(0, -1),
        l(1, 1),
        l(1, -1),
        l(-1, 1),
        l(-1, -1),
    ];
    const M_DHORSE: [D; 8] = [
        r(1, 1),
        r(1, -1),
        r(-1, 1),
        r(-1, -1),
        l(1, 0),
        l(-1, 0),
        l(0, 1),
        l(0, -1),
    ];
    const M_LANCE: [D; 1] = [r(0, 1)];
    const M_RCHARIOT: [D; 2] = [r(0, 1), r(0, -1)];
    const M_SIDEMOVER: [D; 4] = [r(1, 0), r(-1, 0), l(0, 1), l(0, -1)];
    const M_VMOVER: [D; 4] = [r(0, 1), r(0, -1), l(1, 0), l(-1, 0)];
    const M_WHORSE: [D; 4] = [r(0, 1), r(0, -1), r(1, 1), r(-1, 1)];
    const M_WHALE: [D; 4] = [r(0, 1), r(0, -1), r(1, -1), r(-1, -1)];
    const M_FSTAG: [D; 8] = [
        r(0, 1),
        r(0, -1),
        l(1, 0),
        l(-1, 0),
        l(1, 1),
        l(1, -1),
        l(-1, 1),
        l(-1, -1),
    ];
    const M_FOX: [D; 6] = [r(0, 1), r(0, -1), r(1, 1), r(1, -1), r(-1, 1), r(-1, -1)];
    const M_FBOAR: [D; 6] = [r(1, 0), r(-1, 0), r(1, 1), r(1, -1), r(-1, 1), r(-1, -1)];
    const M_CSOLDIER: [D; 8] = [
        r(0, 1),
        r(0, -1),
        r(1, 1),
        r(1, -1),
        r(-1, 1),
        r(-1, -1),
        r2(1, 0),
        r2(-1, 0),
    ];
    const M_WBUFFALO: [D; 8] = [
        r(1, 0),
        r(-1, 0),
        r(1, 1),
        r(1, -1),
        r(-1, 1),
        r(-1, -1),
        r2(0, 1),
        r2(0, -1),
    ];
    const M_VSOLDIER: [D; 4] = [r(0, 1), l(0, -1), r2(1, 0), r2(-1, 0)];
    const M_SSOLDIER: [D; 4] = [r(1, 0), r(-1, 0), l(0, -1), r2(0, 1)];
    const M_MGENERAL: [D; 3] = [r(0, 1), r(1, -1), r(-1, -1)];
    const M_LHAWK: [D; 28] = [
        l(-1, -1),
        l(0, -1),
        l(1, -1),
        l(-1, 0),
        l(1, 0),
        l(-1, 1),
        l(0, 1),
        l(1, 1),
        l(-2, -2),
        l(-1, -2),
        l(0, -2),
        l(1, -2),
        l(2, -2),
        l(-2, -1),
        l(2, -1),
        l(-2, 0),
        l(2, 0),
        l(-2, 1),
        l(2, 1),
        l(-2, 2),
        l(-1, 2),
        l(0, 2),
        l(1, 2),
        l(2, 2),
        r(1, 1),
        r(1, -1),
        r(-1, 1),
        r(-1, -1),
    ];
    const M_HFALCON: [D; 9] = [
        r(1, 0),
        r(-1, 0),
        r(0, -1),
        r(1, 1),
        r(1, -1),
        r(-1, 1),
        r(-1, -1),
        l(0, 1),
        l(0, 2),
    ];
    const M_SEAGLE: [D; 10] = [
        r(1, 0),
        r(-1, 0),
        r(0, 1),
        r(0, -1),
        r(1, -1),
        r(-1, -1),
        l(1, 1),
        l(2, 2),
        l(-1, 1),
        l(-2, 2),
    ];

    /// The movement descriptors of `role`, in White orientation.
    fn moves(role: u8) -> &'static [D] {
        match role {
            KING | PRINCE => &KING8,
            GOLD => &M_GOLD,
            SILVER => &M_SILVER,
            COPPER => &M_COPPER,
            IRON => &M_IRON,
            LEOPARD => &M_LEOPARD,
            BTIGER => &M_BTIGER,
            DELEPHANT => &M_DELEPHANT,
            GOBETWEEN => &M_GOBETWEEN,
            PAWN => &M_PAWN,
            KNIGHT => &M_KNIGHT,
            DOG => &M_DOG,
            KIRIN => &M_KIRIN,
            PHOENIX => &M_PHOENIX,
            ROOK | RGENERAL => &M_ROOK,
            BISHOP | VGENERAL | BGENERAL => &M_BISHOP,
            QUEEN | GGENERAL | FEAGLE => &M_QUEEN,
            DRAGON => &M_DRAGON,
            DHORSE => &M_DHORSE,
            LANCE => &M_LANCE,
            RCHARIOT => &M_RCHARIOT,
            SIDEMOVER => &M_SIDEMOVER,
            VMOVER => &M_VMOVER,
            WHORSE => &M_WHORSE,
            WHALE => &M_WHALE,
            FSTAG => &M_FSTAG,
            FOX | FDEMON => &M_FOX,
            FBOAR => &M_FBOAR,
            CSOLDIER | HTETRARCH => &M_CSOLDIER,
            WBUFFALO => &M_WBUFFALO,
            VSOLDIER => &M_VSOLDIER,
            SSOLDIER => &M_SSOLDIER,
            MGENERAL => &M_MGENERAL,
            CLION => &LION24,
            LHAWK => &M_LHAWK,
            HFALCON => &M_HFALCON,
            SEAGLE => &M_SEAGLE,
            _ => &[],
        }
    }

    // --- promotion --------------------------------------------------------
    fn can_promote(role: u8) -> bool {
        matches!(
            role,
            PAWN | KNIGHT
                | IRON
                | GOBETWEEN
                | LEOPARD
                | COPPER
                | SILVER
                | GOLD
                | LANCE
                | RCHARIOT
                | SIDEMOVER
                | VMOVER
                | BISHOP
                | ROOK
                | DHORSE
                | DRAGON
                | BTIGER
                | KIRIN
                | PHOENIX
                | DELEPHANT
                | SEAGLE
                | HFALCON
                | CLION
                | QUEEN
                | CSOLDIER
                | WBUFFALO
                | VSOLDIER
                | SSOLDIER
                | RGENERAL
                | BGENERAL
                | DOG
        )
    }
    fn promoted(role: u8) -> u8 {
        match role {
            PAWN => GOLD,
            GOBETWEEN => DELEPHANT,
            LEOPARD => BISHOP,
            COPPER => SIDEMOVER,
            SILVER => VMOVER,
            GOLD => ROOK,
            LANCE => WHORSE,
            RCHARIOT => WHALE,
            SIDEMOVER => FBOAR,
            VMOVER => FOX,
            BISHOP => DHORSE,
            ROOK => DRAGON,
            DHORSE => HFALCON,
            DRAGON => SEAGLE,
            BTIGER => FSTAG,
            KIRIN => CLION,
            PHOENIX => QUEEN,
            DELEPHANT => PRINCE,
            KNIGHT => SSOLDIER,
            IRON => VSOLDIER,
            SEAGLE => RGENERAL,
            HFALCON => BGENERAL,
            CLION => LHAWK,
            QUEEN => FEAGLE,
            CSOLDIER => HTETRARCH,
            WBUFFALO => FDEMON,
            VSOLDIER => CSOLDIER,
            SSOLDIER => WBUFFALO,
            RGENERAL => GGENERAL,
            BGENERAL => VGENERAL,
            DOG => MGENERAL,
            other => other,
        }
    }
    fn in_zone(color: u8, rank: i32) -> bool {
        if color == 0 {
            rank >= 11
        } else {
            rank <= 4
        }
    }
    /// A Pawn / Lance reaching the furthest rank, or a Knight the furthest two ranks,
    /// would otherwise be immobile and must promote.
    fn forced(role: u8, color: u8, to_rank: i32) -> bool {
        let furthest = if color == 0 { 15 } else { 0 };
        match role {
            PAWN | LANCE => to_rank == furthest,
            KNIGHT => {
                if color == 0 {
                    to_rank >= 14
                } else {
                    to_rank <= 1
                }
            }
            _ => false,
        }
    }

    // --- range-jumping Generals -------------------------------------------
    fn jump_rank(role: u8) -> u8 {
        match role {
            KING | PRINCE => 4,
            GGENERAL => 3,
            VGENERAL => 2,
            RGENERAL | BGENERAL => 1,
            _ => 0,
        }
    }
    fn is_jump_capturer(role: u8) -> bool {
        matches!(role, GGENERAL | VGENERAL | RGENERAL | BGENERAL)
    }
    fn jump_dirs(role: u8) -> &'static [(i8, i8)] {
        match role {
            GGENERAL => &[
                (1, 0),
                (-1, 0),
                (0, 1),
                (0, -1),
                (1, 1),
                (1, -1),
                (-1, 1),
                (-1, -1),
            ],
            RGENERAL => &[(1, 0), (-1, 0), (0, 1), (0, -1)],
            VGENERAL | BGENERAL => &[(1, 1), (1, -1), (-1, 1), (-1, -1)],
            _ => &[],
        }
    }
    fn is_immune(role: u8) -> bool {
        role == GGENERAL
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    struct Pc {
        color: u8, // 0 = White, 1 = Black
        role: u8,
    }

    #[derive(Clone)]
    pub(crate) struct Position {
        cells: [Option<Pc>; 256],
        turn: u8,
    }

    #[inline]
    fn idx(f: i32, r: i32) -> usize {
        (r * N + f) as usize
    }
    #[inline]
    fn file_of(i: usize) -> i32 {
        (i as i32) % N
    }
    #[inline]
    fn rank_of(i: usize) -> i32 {
        (i as i32) / N
    }
    #[inline]
    fn on_board(f: i32, r: i32) -> bool {
        (0..N).contains(&f) && (0..N).contains(&r)
    }
    #[inline]
    fn orient(color: u8, df: i8, dr: i8) -> (i32, i32) {
        if color == 0 {
            (df as i32, dr as i32)
        } else {
            (df as i32, -(dr as i32))
        }
    }

    /// A move: an ordinary board move `(from, to)` with a resulting role (already
    /// resolving lion-style promotion). Jump-captures reuse the same shape. Lion
    /// multi-step moves additionally clear up to two extra captured squares
    /// (`cap_a` / `cap_b`, `usize::MAX` = none) — the intermediate/first-leg victim
    /// beyond the landing square `to`; igui and the jitto pass have `to == from`.
    #[derive(Clone, Copy)]
    struct Mv {
        from: usize,
        to: usize,
        become_role: u8,
        cap_a: usize,
        cap_b: usize,
    }
    const NO_CAP: usize = usize::MAX;

    impl Position {
        pub(crate) fn startpos() -> Position {
            // Seed the layout (only) from the shared start position; every count-
            // producing rule below is independent. The placement is validated
            // separately against HaChu's source tables at perft(1) = 72.
            let src = Tenjiku::startpos();
            let board = src.board();
            let mut cells = [None; 256];
            for r in 0..16u8 {
                for f in 0..16u8 {
                    let sq = Square::<Tenjiku16x16>::from_file_rank(f, r).unwrap();
                    if let Some(p) = board.piece_at(sq) {
                        let color = if p.color == Color::White { 0 } else { 1 };
                        cells[idx(f as i32, r as i32)] = Some(Pc {
                            color,
                            role: from_wide(p.role),
                        });
                    }
                }
            }
            Position { cells, turn: 0 }
        }

        /// The ordinary reachable squares of the piece at `from` (empty or enemy),
        /// respecting blocking and the Great-General capture immunity. `attack_mode`
        /// ignores immunity (a piece still *attacks* through the immune mask for the
        /// king-safety query, though kings are never immune anyway).
        fn ordinary_targets(&self, from: usize, out: &mut Vec<usize>, attack_mode: bool) {
            let Some(p) = self.cells[from] else { return };
            let (ff, fr) = (file_of(from), rank_of(from));
            for d in moves(p.role) {
                let (df, dr) = orient(p.color, d.df, d.dr);
                let max = match d.mode {
                    0 => 1,
                    2 => 2,
                    _ => i32::MAX,
                };
                let leap = d.mode == 0;
                let (mut nf, mut nr) = (ff + df, fr + dr);
                let mut steps = 0;
                while on_board(nf, nr) {
                    let t = idx(nf, nr);
                    match self.cells[t] {
                        None => out.push(t),
                        Some(q) => {
                            if q.color != p.color {
                                // Enemy: a capturable landing unless immune (and we
                                // are not the immune role, and not just querying an
                                // attack).
                                if attack_mode || !is_immune(q.role) || is_immune(p.role) {
                                    out.push(t);
                                }
                            }
                            break; // any piece blocks a ride; a leap has max == 1
                        }
                    }
                    steps += 1;
                    if leap || steps >= max {
                        break;
                    }
                    nf += df;
                    nr += dr;
                }
            }
        }

        /// The range-jump landing squares of a General at `from` (capturing across a
        /// consecutive run of strictly-lower-ranked pieces). `attack_mode` includes
        /// the immune Great General as an attacked square for king-safety.
        fn jump_targets(&self, from: usize, out: &mut Vec<usize>, attack_mode: bool) {
            let Some(p) = self.cells[from] else { return };
            if !is_jump_capturer(p.role) {
                return;
            }
            let mover_rank = jump_rank(p.role);
            let (ff, fr) = (file_of(from), rank_of(from));
            for &(df0, dr0) in jump_dirs(p.role) {
                let (df, dr) = orient(p.color, df0, dr0);
                let (mut nf, mut nr) = (ff + df, fr + dr);
                let mut jumped = false;
                while on_board(nf, nr) {
                    let t = idx(nf, nr);
                    match self.cells[t] {
                        None => {
                            if jumped {
                                break; // consecutive run ended
                            }
                        }
                        Some(q) => {
                            if jumped && q.color != p.color {
                                let immune = is_immune(q.role);
                                if attack_mode || !immune || q.role == p.role {
                                    out.push(t);
                                }
                            }
                            if jump_rank(q.role) < mover_rank {
                                jumped = true;
                            } else {
                                break; // equal-or-higher: opaque wall
                            }
                        }
                    }
                    nf += df;
                    nr += dr;
                }
            }
        }

        /// Is `sq` attacked by a piece of color `by` (ordinary rides/leaps + General
        /// jump-attacks)? Used for multi-royal king safety.
        fn attacked(&self, sq: usize, by: u8) -> bool {
            let mut buf = Vec::new();
            for i in 0..256 {
                if let Some(p) = self.cells[i] {
                    if p.color == by {
                        buf.clear();
                        self.ordinary_targets(i, &mut buf, true);
                        if buf.contains(&sq) {
                            return true;
                        }
                        if is_jump_capturer(p.role) {
                            buf.clear();
                            self.jump_targets(i, &mut buf, true);
                            if buf.contains(&sq) {
                                return true;
                            }
                        }
                    }
                }
            }
            false
        }

        fn pseudo(&self) -> Vec<Mv> {
            let us = self.turn;
            let mut mv = Vec::new();
            let mut buf = Vec::new();
            for from in 0..256 {
                let Some(p) = self.cells[from] else { continue };
                if p.color != us {
                    continue;
                }
                let from_zone = in_zone(us, rank_of(from));
                // Ordinary moves.
                buf.clear();
                self.ordinary_targets(from, &mut buf, false);
                for &to in &buf {
                    let become_role = Self::promo(p.role, us, from_zone, rank_of(to));
                    mv.push(Mv {
                        from,
                        to,
                        become_role,
                        cap_a: NO_CAP,
                        cap_b: NO_CAP,
                    });
                }
                // Range-jump captures (a separate pass, like the engine's).
                if is_jump_capturer(p.role) {
                    buf.clear();
                    self.jump_targets(from, &mut buf, false);
                    for &to in &buf {
                        let become_role = Self::promo(p.role, us, from_zone, rank_of(to));
                        mv.push(Mv {
                            from,
                            to,
                            become_role,
                            cap_a: NO_CAP,
                            cap_b: NO_CAP,
                        });
                    }
                }
            }
            // Lion multi-step / igui / jitto pass moves (issue #500): these first
            // arise at ply 3 and are enumerated exactly as the engine's
            // `gen_lion_moves` (the single within-two Lion leaps are already produced
            // above via the ordinary role loop; this adds only the moves that touch an
            // intermediate square, plus the once-per-side pass).
            self.gen_lion(&mut mv);
            mv
        }

        /// Chebyshev distance between two board squares.
        fn cheb(a: usize, b: usize) -> i32 {
            (file_of(a) - file_of(b))
                .abs()
                .max((rank_of(a) - rank_of(b)).abs())
        }

        /// The eight King directions, the two-step alphabet of a full Lion.
        const LION_DIRS8: [(i8, i8); 8] = [
            (-1, -1),
            (0, -1),
            (1, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
        ];

        /// Appends the side-to-move's Lion multi-step moves, mirroring the engine's
        /// `gen_lion_moves`: full Lions (Lion, Lion-Hawk) turn freely over two King
        /// steps; the Horned Falcon / Soaring Eagle carry lion power only straight
        /// along their lion lines. The immune Great General may be neither stepped on
        /// nor captured. A non-capturing double-step coincides with the leaper jump
        /// already emitted, so only capturing legs (and, for empties, distance-two
        /// captures) are added; the jitto pass is emitted at most once per side.
        fn gen_lion(&self, mv: &mut Vec<Mv>) {
            let us = self.turn;
            let mut pass_emitted = false;
            for from in 0..256 {
                let Some(p) = self.cells[from] else { continue };
                if p.color != us {
                    continue;
                }
                let full = matches!(p.role, CLION | LHAWK);
                let lines: &[(i8, i8)] = match p.role {
                    HFALCON => &[(0, 1)],
                    SEAGLE => &[(1, 1), (-1, 1)],
                    _ => &[],
                };
                if !full && lines.is_empty() {
                    continue;
                }
                let mut can_pass = false;
                let blocked = |sq: usize| matches!(self.cells[sq], Some(q) if q.color == us || is_immune(q.role));
                let capturable = |sq: usize| matches!(self.cells[sq], Some(q) if q.color != us && !is_immune(q.role));
                let (ff, fr) = (file_of(from), rank_of(from));
                if full {
                    for &(d1f, d1r) in &Self::LION_DIRS8 {
                        let (s1f, s1r) = (ff + d1f as i32, fr + d1r as i32);
                        if !on_board(s1f, s1r) {
                            continue;
                        }
                        let s1 = idx(s1f, s1r);
                        if blocked(s1) {
                            continue;
                        }
                        let s1_enemy = capturable(s1);
                        if s1_enemy {
                            mv.push(Mv {
                                from,
                                to: from,
                                become_role: p.role,
                                cap_a: s1,
                                cap_b: NO_CAP,
                            });
                        } else {
                            can_pass = true;
                        }
                        for &(d2f, d2r) in &Self::LION_DIRS8 {
                            let (s2f, s2r) = (s1f + d2f as i32, s1r + d2r as i32);
                            if !on_board(s2f, s2r) {
                                continue;
                            }
                            let s2 = idx(s2f, s2r);
                            if s2 == from || blocked(s2) {
                                continue;
                            }
                            let s2_enemy = capturable(s2);
                            let emit = if s1_enemy {
                                true
                            } else {
                                s2_enemy && Self::cheb(from, s2) == 2
                            };
                            if emit {
                                let cap_a = if s1_enemy { s1 } else { NO_CAP };
                                mv.push(Mv {
                                    from,
                                    to: s2,
                                    become_role: p.role,
                                    cap_a,
                                    cap_b: NO_CAP,
                                });
                            }
                        }
                    }
                } else {
                    for &(lf, lr) in lines {
                        let (df, dr) = orient(us, lf, lr);
                        let (s1f, s1r) = (ff + df, fr + dr);
                        if !on_board(s1f, s1r) {
                            continue;
                        }
                        let s1 = idx(s1f, s1r);
                        if blocked(s1) {
                            continue;
                        }
                        let s1_enemy = capturable(s1);
                        if s1_enemy {
                            mv.push(Mv {
                                from,
                                to: from,
                                become_role: p.role,
                                cap_a: s1,
                                cap_b: NO_CAP,
                            });
                        } else {
                            can_pass = true;
                        }
                        let (s2f, s2r) = (s1f + df, s1r + dr);
                        if on_board(s2f, s2r) {
                            let s2 = idx(s2f, s2r);
                            let s2_enemy = capturable(s2);
                            if !blocked(s2) && (s1_enemy || s2_enemy) {
                                let cap_a = if s1_enemy { s1 } else { NO_CAP };
                                mv.push(Mv {
                                    from,
                                    to: s2,
                                    become_role: p.role,
                                    cap_a,
                                    cap_b: NO_CAP,
                                });
                            }
                        }
                    }
                }
                if can_pass && !pass_emitted {
                    mv.push(Mv {
                        from,
                        to: from,
                        become_role: p.role,
                        cap_a: NO_CAP,
                        cap_b: NO_CAP,
                    });
                    pass_emitted = true;
                }
            }
        }

        /// The resulting role after a move, applying the lion-style promotion rule:
        /// promote (mandatorily) exactly when entering the zone from outside, or when
        /// forced; otherwise the piece keeps its role.
        fn promo(role: u8, us: u8, from_zone: bool, to_rank: i32) -> u8 {
            if !can_promote(role) {
                return role;
            }
            let to_zone = in_zone(us, to_rank);
            let entering = to_zone && !from_zone;
            if entering || forced(role, us, to_rank) {
                promoted(role)
            } else {
                role
            }
        }

        fn apply(&self, m: Mv) -> Position {
            let mut p = self.clone();
            let mut mover = p.cells[m.from].expect("mover");
            p.cells[m.from] = None;
            if m.cap_a != NO_CAP {
                p.cells[m.cap_a] = None;
            }
            if m.cap_b != NO_CAP {
                p.cells[m.cap_b] = None;
            }
            mover.role = m.become_role;
            // `to == from` (igui / jitto pass) re-places the mover on its origin.
            p.cells[m.to] = Some(mover);
            p.turn = 1 - self.turn;
            p
        }

        fn king_sq(&self, color: u8) -> Option<usize> {
            // Exactly one royal (King) per side throughout the start tree to depth 3
            // (no Prince arises), so the sole royal is the King.
            (0..256)
                .find(|&i| matches!(self.cells[i], Some(p) if p.color == color && p.role == KING))
        }

        fn legal_moves(&self) -> Vec<Mv> {
            let us = self.turn;
            let them = 1 - us;
            let mut out = Vec::new();
            for m in self.pseudo() {
                let next = self.apply(m);
                if let Some(k) = next.king_sq(us) {
                    if next.attacked(k, them) {
                        continue;
                    }
                }
                out.push(m);
            }
            out
        }
    }

    pub(crate) fn perft(pos: &Position, depth: u32) -> u64 {
        if depth == 0 {
            return 1;
        }
        let moves = pos.legal_moves();
        if depth == 1 {
            return moves.len() as u64;
        }
        let mut total = 0;
        for m in moves {
            total += perft(&pos.apply(m), depth - 1);
        }
        total
    }
}
