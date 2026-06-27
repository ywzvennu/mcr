//! Seeded random self-play: pick uniformly random legal moves until the game ends.
//!
//! The randomness is a tiny `splitmix64` generator implemented right here, so the
//! example has no dependencies and is fully deterministic: the same seed always
//! produces the same game. It plays from the standard starting position to a
//! terminal node (checkmate, stalemate, or one of the draw rules), printing the
//! move list in SAN and the final result.
//!
//! ```text
//! cargo run --example random_game            # default seed
//! cargo run --example random_game -- 42      # explicit seed
//! ```
//!
//! A hard ply cap guards against the rare game that neither side can finish; in
//! practice random play almost always reaches a real terminal position first.

use std::process::ExitCode;

use mce::{Game, Outcome};

/// Safety cap on plies. Random games end well within this; the cap only protects
/// against a pathological non-terminating line.
const MAX_PLIES: usize = 1024;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let seed: u64 = match args.first() {
        None => 0x1234_5678_9abc_def0,
        Some(s) => match s.parse() {
            Ok(seed) => seed,
            Err(_) => {
                eprintln!("error: seed must be a non-negative integer, got {s:?}");
                return ExitCode::FAILURE;
            }
        },
    };

    let mut rng = SplitMix64::new(seed);
    // `Game` validates moves and tracks repetition / move-clock draws, so its
    // `outcome` recognizes the claimable draws a bare `Position` would not.
    let mut game = Game::from_startpos();
    let mut plies = 0;

    println!("seed: {seed}");
    println!();

    while game.outcome().is_none() && plies < MAX_PLIES {
        let moves = game.legal_moves();
        if moves.is_empty() {
            break;
        }
        let pick = rng.below(moves.len() as u64) as usize;
        let mv = moves[pick];

        // Render SAN against the pre-move position, then apply the move.
        let san = game.position().san(&mv);
        let move_number = game.position().fullmove_number();
        let white_to_move = game.position().turn().is_white();

        if white_to_move {
            print!("{move_number}. {san} ");
        } else {
            print!("{san} ");
        }

        game.play(&mv)
            .expect("move came from legal_moves, so it is legal");
        plies += 1;
    }

    println!();
    println!();
    println!("plies: {plies}");
    match game.outcome() {
        Some(Outcome::Decisive { winner }) => {
            let side = if winner.is_white() { "white" } else { "black" };
            println!("result: {side} wins ({:?})", game.end_reason());
        }
        Some(Outcome::Draw) => println!("result: draw ({:?})", game.end_reason()),
        None => println!("result: unfinished (hit the {MAX_PLIES}-ply cap)"),
    }

    ExitCode::SUCCESS
}

/// A minimal `splitmix64` PRNG — a single 64-bit state advanced by a fixed
/// increment and run through a finalizing mix. Deterministic, fast, and good
/// enough for picking moves; not for cryptography.
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        SplitMix64 { state: seed }
    }

    /// Returns the next 64-bit output and advances the state.
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A uniform value in `0..bound`. Uses Lemire's multiply-high method, which
    /// avoids modulo bias without a rejection loop in the common case.
    fn below(&mut self, bound: u64) -> u64 {
        // (x * bound) >> 64 is uniform when x is; take the high half of the
        // 128-bit product.
        ((u128::from(self.next_u64()) * u128::from(bound)) >> 64) as u64
    }
}
