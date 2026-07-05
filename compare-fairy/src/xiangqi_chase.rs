//! Xiangqi perpetual-**chase** cross-check against Fairy-Stockfish (issue #475).
//!
//! mcr models the AXF perpetual-chase rule as a `GenericGame`-level adjudication
//! whose per-ply kernel — [`GenericGame::chased_squares`] — is a port of FSF's
//! `Position::chased()`. FSF exposes that same set node-for-node on its `d`
//! command (`Chased: …`), so this mode walks seeded random legal Xiangqi games and
//! asserts mcr's chased victim set equals FSF's after **every** move. It is the
//! machine oracle the issue calls for; a single divergent node prints its FEN +
//! move to reproduce.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no GPL
//! code is linked. mcr's own chased set comes from the library.

use mcr::geometry::{GenericGame, Square, Xiangqi, Xiangqi9x10};

use crate::uci::Engine;
use crate::xiangqi::fen_to_fsf;

/// A deterministic splitmix64 PRNG (matching `difffuzz`): dependency-free and
/// reproducible from its seed.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed)
    }
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

/// Seed positions (mcr dialect). The startpos plus deliberately sparse endings that
/// make Chariots / Horses / Cannons harry each other, so random walks hit chases
/// (rather than quiet opening shuffles) frequently.
const SEEDS: &[&str] = &[
    "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/9/RJOUKUOJR w - - 0 1",
    // Chariot vs chariot + horses, open board.
    "4k4/9/9/9/2r3J2/2R3j2/9/9/9/4K4 w - - 0 1",
    // Cannons + horses + a chariot, kings on the same file (flying-general geometry).
    "4k4/9/4c4/9/2j3J2/2C6/9/4C4/9/4K4 w - - 0 1",
    // Advisors (`u`) / elephants (`o`) active near the palace edges with chariots.
    "3uk4/4o4/9/9/r5J2/R5j2/9/9/4O4/3UK4 w - - 0 1",
    // Sparse chariot + cannon + horse melee, different king files.
    "5k3/9/9/2r6/9/9/2C6/9/3j5/4K4 w - - 0 1",
    // Two horses + two chariots, mutual harassment (kings off a shared file).
    "3k5/9/9/1jr6/9/9/5RJ2/9/9/4K4 b - - 0 1",
    // Elephants (`o`) + advisors (`u`) + horses near both palaces, chariots probing.
    "2ouk4/4o4/2j3j2/9/r7r/R7R/2J3J2/2OUK4/9/9 w - - 0 1",
    // Cannon batteries with a screen, plus horses (discovered-attack rich).
    "4k4/9/2c3c2/9/2j6/9/2C3C2/9/2J6/4K4 w - - 0 1",
];

/// mcr square -> FSF square name (`a1`..`i10`): file letter + one-based rank.
fn square_name(sq: Square<Xiangqi9x10>) -> String {
    let file = (b'a' + sq.file()) as char;
    format!("{file}{}", sq.rank() + 1)
}

/// A running tally of the cross-check.
struct Tally {
    nodes: u64,
    chase_nodes: u64,
    agree: u64,
    diverge: u64,
    shown: u64,
}

/// Play one seeded random game from `seed_fen`, comparing mcr's chased set to FSF's
/// after every move (up to `plies`). Accumulates into `t`.
fn run_game(engine: &mut Engine, seed_fen: &str, mut rng: Rng, plies: u32, t: &mut Tally) {
    let Ok(start) = Xiangqi::from_fen(seed_fen) else {
        eprintln!("skip: mcr rejected seed FEN {seed_fen}");
        return;
    };
    let mut game = GenericGame::new(start);
    for _ in 0..plies {
        // The FEN *before* the move, in the FSF dialect (needed so FSF sees the same
        // move onto the same board and computes `st->chased`).
        let mcr_prev_fen = game.position().to_fen();
        let prev_fen = fen_to_fsf(&mcr_prev_fen);
        let moves = game.legal_moves();
        if moves.is_empty() {
            break;
        }
        let mv = moves[rng.below(moves.len())];
        let uci = mv.to_uci::<Xiangqi9x10>();
        if game.play(&mv).is_err() {
            break;
        }

        // mcr's chased set for the move just played.
        let mut mcr_chased: Vec<String> = game
            .chased_squares()
            .iter()
            .map(|s| square_name(*s))
            .collect();
        mcr_chased.sort();

        // FSF's chased set for the same move.
        let fsf_chased = match engine.chased(&prev_fen, &uci) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("skip node ({prev_fen} moves {uci}): {e}");
                continue;
            }
        };

        t.nodes += 1;
        if !mcr_chased.is_empty() || !fsf_chased.is_empty() {
            t.chase_nodes += 1;
        }
        if mcr_chased == fsf_chased {
            t.agree += 1;
        } else {
            t.diverge += 1;
            if t.shown < 40 {
                t.shown += 1;
                eprintln!(
                    "DIVERGE  fen: {prev_fen}  move: {uci}  (from={} to={})\n         mcr-fen: {mcr_prev_fen}\n         mcr: {mcr_chased:?}\n         fsf: {fsf_chased:?}",
                    mv.from::<Xiangqi9x10>().index(),
                    mv.to::<Xiangqi9x10>().index(),
                );
            }
        }

        if game.is_over() {
            break;
        }
    }
}

/// Run the chase cross-check across all seeds and `games` random walks each of
/// `plies` plies. Returns the divergence count (0 = full node-for-node agreement).
pub fn run(engine: &mut Engine, seed: u64, games: u32, plies: u32) -> u64 {
    println!();
    println!("Xiangqi perpetual-chase cross-check vs FSF `chased()` (issue #475):");
    if !engine.has_variant("xiangqi") {
        println!("  SKIP: this FSF binary has no `xiangqi` variant (build it largeboards=yes).");
        return 0;
    }
    if let Err(e) = engine.set_variant("xiangqi", false) {
        println!("  SKIP: could not select xiangqi: {e}");
        return 0;
    }

    let mut t = Tally {
        nodes: 0,
        chase_nodes: 0,
        agree: 0,
        diverge: 0,
        shown: 0,
    };
    for (si, seed_fen) in SEEDS.iter().enumerate() {
        for g in 0..games {
            let s = seed
                .wrapping_add((si as u64).wrapping_mul(0xD1B5_4A32_D192_ED03))
                .wrapping_add((g as u64).wrapping_mul(0xCA45_57F8_5EBA_7C9B));
            run_game(
                engine,
                seed_fen,
                Rng::new(Rng::new(s).next_u64()),
                plies,
                &mut t,
            );
        }
    }

    println!(
        "  nodes: {}  (chase nodes: {})  agree: {}  diverge: {}",
        t.nodes, t.chase_nodes, t.agree, t.diverge
    );
    if t.diverge == 0 {
        println!(
            "OK: mcr chased() matched FSF on all {} nodes ({} with a live chase).",
            t.nodes, t.chase_nodes
        );
    } else {
        eprintln!("ERROR: {} chase divergence(s) vs FSF.", t.diverge);
    }
    t.diverge
}
