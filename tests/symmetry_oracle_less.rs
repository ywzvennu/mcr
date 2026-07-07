//! Colour-symmetry structural check for the oracle-less variants (issue #558).
//!
//! The thinnest-validated variants in the tree — the HaChu-oracle large shogi
//! (Chu, Dai, Tenjiku) and the fully independent ones (Alice, Jieqi, Wa Shogi) —
//! carry no external engine oracle, or only a shallow one. This test adds an
//! oracle-free *structural* invariant that needs no second implementation:
//! **colour symmetry of move generation** at the pinned start position.
//!
//! Every one of these variants — **except Tenjiku** — has a start array that is
//! symmetric under a 180° rotation of the board combined with a colour swap
//! (White's camp maps onto Black's). Tenjiku is deliberately excluded: its start
//! position faithfully reproduces the HaChu oracle's hand-written **asymmetry**
//! (White's second rank is one file short of Black's; see
//! `src/geometry/variants/tenjiku.rs`), so White-to-move perft (72 at depth 1) and
//! Black-to-move perft (79) differ *by design* — matching HaChu node-for-node is
//! the validation goal there, and colour symmetry would contradict it. This test
//! confirmed that asymmetry rather than papering over it.
//!
//! For the six symmetric starts (Chu, Dai, Alice, Jieqi, Wa Shogi, and the 10x10
//! Okisaki Shogi), that symmetry `σ` is a rules isomorphism, so for
//! the start position
//! `A = (board, White to move)` and the same board with the turn handed to Black,
//! `B = (board, Black to move) = σ(A)`, perft must agree at every depth:
//!
//! ```text
//! perft(A, d) == perft(σ(A), d) == perft(B, d)   for all d.
//! ```
//!
//! `B` is built by flipping **only** the side-to-move field of the start FEN — no
//! board surgery, so the test cannot be fooled by a buggy board transform. It is
//! non-vacuous (`B` is a genuinely different, Black-to-move position that drives
//! the Black half of the colour-generic generator) and it catches any colour
//! asymmetry in movegen, which no perft node count against a *single* reference
//! would reveal on its own. It is the oracle-free counterpart of the
//! FSF-differential sweep the oracle-backed variants enjoy.

use mcr::geometry::{
    perft as gperft, Alice, Chess8x8, Chu, Chu12x12, Dai, Dai15x15, Grand10x10, Jieqi,
    OkisakiShogi, Washogi, Washogi11x11, Xiangqi9x10, Yari, YariShogi7x9,
};

/// Returns `fen` with its side-to-move field flipped (`w` <-> `b`), leaving every
/// other field byte-identical. Splitting on whitespace guarantees the board field
/// (digits + piece letters, no bare `w`/`b` token) is never touched.
fn flip_turn(fen: &str) -> String {
    let mut fields: Vec<&str> = fen.split(' ').collect();
    // The side-to-move is the second space-separated field in every dialect here.
    let turn = fields.get(1).copied().unwrap_or("");
    let flipped = match turn {
        "w" => "b",
        "b" => "w",
        other => panic!("unexpected side-to-move field {other:?} in FEN {fen:?}"),
    };
    fields[1] = flipped;
    fields.join(" ")
}

/// Asserts, for a 180°-colour-symmetric start position, that handing the first
/// move to Black yields the identical perft at every depth in `depths`. The
/// turn-flipped FEN is re-parsed back into the concrete position type `$pos`, and
/// `gperft` is the generic node counter over that variant's geometry `$geom`.
macro_rules! symmetric_start {
    ($name:ident, $pos:ty, $geom:ty, $startpos:expr, $depths:expr) => {
        #[test]
        fn $name() {
            let white = $startpos;
            let white_fen = white.to_fen();
            let black_fen = flip_turn(&white_fen);
            assert_ne!(
                white_fen, black_fen,
                "turn flip must produce a distinct FEN (else the check is vacuous)"
            );
            let black = <$pos>::from_fen(&black_fen)
                .expect("the colour-mirrored start position must be a legal, parseable FEN");
            for &depth in $depths {
                let w = gperft::<$geom, _, _>(&white, depth);
                let b = gperft::<$geom, _, _>(&black, depth);
                assert_eq!(
                    w,
                    b,
                    "colour-symmetry broken for {} at depth {depth}: \
                     White-to-move perft {w} != Black-to-move perft {b}",
                    stringify!($name),
                );
            }
        }
    };
}

symmetric_start!(
    alice_colour_symmetric,
    Alice,
    Chess8x8,
    Alice::startpos(),
    &[1, 2, 3]
);
symmetric_start!(
    jieqi_colour_symmetric,
    Jieqi,
    Xiangqi9x10,
    Jieqi::startpos(),
    &[1, 2, 3]
);
symmetric_start!(
    chu_colour_symmetric,
    Chu,
    Chu12x12,
    Chu::startpos(),
    &[1, 2]
);
symmetric_start!(
    dai_colour_symmetric,
    Dai,
    Dai15x15,
    Dai::startpos(),
    &[1, 2]
);
symmetric_start!(
    washogi_colour_symmetric,
    Washogi,
    Washogi11x11,
    Washogi::startpos(),
    &[1, 2]
);
symmetric_start!(
    okisakishogi_colour_symmetric,
    OkisakiShogi,
    Grand10x10,
    OkisakiShogi::startpos(),
    &[1, 2, 3]
);
symmetric_start!(
    yari_colour_symmetric,
    Yari,
    YariShogi7x9,
    Yari::startpos(),
    &[1, 2, 3]
);

// Tenjiku is intentionally NOT here: its start array reproduces HaChu's documented
// asymmetry (perft(1) = 72 for White, 79 for Black), so colour symmetry does not
// hold and asserting it would be wrong. See the module doc above and
// `src/geometry/variants/tenjiku.rs`.
