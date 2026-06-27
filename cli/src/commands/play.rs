//! `mce play` — seeded random self-play.
//!
//! Picks uniformly random legal moves from the start position until the game
//! ends (or a ply cap is hit), printing the move list and the final result. The
//! RNG is a tiny deterministic `splitmix64`, so the same `--seed` always
//! reproduces the same game.
//!
//! Standard chess plays through the `Game` driver, which tracks repetition and
//! move-clock draws and renders SAN. Variants play through `AnyVariant` and
//! render UCI. `--pgn` additionally emits the game as PGN (built from the move
//! list via the library's `Pgn::from_moves`).

use clap::Args;

use mce::{AnyVariant, Game, Move, Outcome, Pgn, VariantId};

use crate::util::{self, CliError, CliResult, SplitMix64};

/// Default ply cap: random games end well within this; the cap only guards
/// against a pathological non-terminating line.
const DEFAULT_MAX_PLIES: usize = 1024;

/// Arguments for `mce play`.
#[derive(Debug, Args)]
pub struct PlayArgs {
    /// Variant rule set (default: standard chess).
    #[arg(long, value_name = "V")]
    variant: Option<String>,
    /// RNG seed; the same seed reproduces the same game.
    #[arg(long, default_value_t = 0x1234_5678_9abc_def0)]
    seed: u64,
    /// Maximum number of plies before giving up (safety cap).
    #[arg(long, value_name = "N", default_value_t = DEFAULT_MAX_PLIES)]
    max: usize,
    /// Also print the game as PGN.
    #[arg(long)]
    pgn: bool,
}

pub fn run(args: PlayArgs) -> CliResult {
    let id = util::parse_variant(args.variant.as_deref())?;
    println!("seed: {}", args.seed);
    if !util::is_standard(id) {
        println!("variant: {id}");
    }
    println!();

    if util::is_standard(id) {
        play_standard(&args)
    } else {
        play_variant(id, &args)
    }
}

/// Standard self-play through `Game` (SAN moves, full draw detection).
fn play_standard(args: &PlayArgs) -> CliResult {
    let mut rng = SplitMix64::new(args.seed);
    let mut game = Game::from_startpos();
    let mut moves: Vec<Move> = Vec::new();
    let mut plies = 0;

    while game.outcome().is_none() && plies < args.max {
        let legal = game.legal_moves();
        if legal.is_empty() {
            break;
        }
        let mv = legal[rng.below(legal.len() as u64) as usize];

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
        moves.push(mv);
        plies += 1;
    }

    println!();
    println!();
    println!("plies: {plies}");
    report_outcome(game.outcome(), game.end_reason(), plies, args.max);

    if args.pgn {
        emit_pgn(AnyVariant::startpos(VariantId::Standard), &moves)?;
    }
    Ok(())
}

/// Variant self-play through `AnyVariant` (UCI moves).
fn play_variant(id: VariantId, args: &PlayArgs) -> CliResult {
    let mut rng = SplitMix64::new(args.seed);
    let mut pos = AnyVariant::startpos(id);
    let mut moves: Vec<Move> = Vec::new();
    let mut plies = 0;

    while pos.outcome().is_none() && plies < args.max {
        let legal = pos.legal_moves();
        if legal.is_empty() {
            break;
        }
        let mv = legal[rng.below(legal.len() as u64) as usize];
        print!("{} ", pos.to_uci(&mv));
        pos = pos.play(&mv);
        moves.push(mv);
        plies += 1;
    }

    println!();
    println!();
    println!("plies: {plies}");
    report_outcome(pos.outcome(), pos.end_reason(), plies, args.max);

    if args.pgn {
        emit_pgn(AnyVariant::startpos(id), &moves)?;
    }
    Ok(())
}

/// Prints the final result line.
fn report_outcome(
    outcome: Option<Outcome>,
    end_reason: Option<mce::EndReason>,
    plies: usize,
    max: usize,
) {
    match outcome {
        Some(Outcome::Decisive { winner }) => {
            println!(
                "result: {} wins ({:?})",
                util::side_name(winner),
                end_reason
            );
        }
        Some(Outcome::Draw) => println!("result: draw ({end_reason:?})"),
        None => println!("result: unfinished (hit the {max}-ply cap at {plies} plies)"),
    }
}

/// Builds a PGN from the recorded move list and prints it.
fn emit_pgn(start: AnyVariant, moves: &[Move]) -> CliResult {
    let pgn = Pgn::from_moves(start, moves, Vec::new())
        .map_err(|err| CliError::new(format!("could not build PGN: {err}")))?;
    println!();
    println!("{}", pgn.to_pgn());
    Ok(())
}
