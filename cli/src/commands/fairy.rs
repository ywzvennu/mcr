//! `mce fairy` — surface the geometry-layer fairy variants (xiangqi, shogi,
//! janggi, orda, …) from the shell.
//!
//! These variants live on the generic geometry engine and are reached at runtime
//! through [`mce::geometry::AnyWideVariant`] / [`mce::geometry::WideVariantId`],
//! a separate dispatch from the concrete engine's `AnyVariant` (which the other
//! subcommands use). The operations mirror what the rest of the CLI already does
//! for standard chess — list the variants, count perft nodes, inspect a position
//! (FEN, side, legal moves in UCI), and play UCI moves — only over the fairy
//! board geometries, where SAN and the 8x8 `Position` helpers do not apply.

use clap::{Args, Subcommand};

use mce::geometry::WideVariantId;

use crate::util::{self, CliResult};

/// Arguments for `mce fairy <SUBCOMMAND>`.
#[derive(Debug, Args)]
pub struct FairyArgs {
    #[command(subcommand)]
    command: FairyCommand,
}

/// The fairy-variant operations, each mirroring a standard-engine counterpart.
#[derive(Debug, Subcommand)]
enum FairyCommand {
    /// List every supported fairy variant name.
    List,
    /// Count the leaf nodes of the move tree to a fixed depth (perft).
    Perft(PerftArgs),
    /// Dump a position: board, side to move, and the legal moves in UCI.
    Inspect(PositionArgs),
    /// Play one or more UCI moves and print the resulting FEN.
    Play(PlayArgs),
}

/// A fairy variant plus a start position, shared by the subcommands.
#[derive(Debug, Args)]
struct PositionArgs {
    /// Fairy variant name, e.g. `xiangqi`, `shogi`, `janggi` (see `fairy list`).
    variant: String,
    /// Start position: a variant FEN, or `startpos` for the variant's opening.
    fen: String,
}

/// `mce fairy perft <VARIANT> <FEN|startpos> <DEPTH>`.
#[derive(Debug, Args)]
struct PerftArgs {
    #[command(flatten)]
    pos: PositionArgs,
    /// Search depth (number of plies); depth 0 counts the position itself (1).
    depth: u32,
    /// Print the per-root-move node breakdown as well as the total.
    #[arg(long)]
    divide: bool,
}

/// `mce fairy play <VARIANT> <FEN|startpos> <UCI>...`.
#[derive(Debug, Args)]
struct PlayArgs {
    #[command(flatten)]
    pos: PositionArgs,
    /// One or more UCI moves to apply in order.
    #[arg(required = true)]
    moves: Vec<String>,
}

pub fn run(args: FairyArgs) -> CliResult {
    match args.command {
        FairyCommand::List => list(),
        FairyCommand::Perft(args) => perft(args),
        FairyCommand::Inspect(args) => inspect(args),
        FairyCommand::Play(args) => play(args),
    }
}

/// Prints every fairy variant's canonical name, one per line.
fn list() -> CliResult {
    for id in WideVariantId::ALL {
        println!("{id}");
    }
    Ok(())
}

/// Perft over a fairy variant. `AnyWideVariant` exposes a whole-tree `perft` but
/// no divide, so the per-root-move breakdown is built by playing each root move
/// and counting one ply shallower (the same shape `mce perft --variant` uses).
fn perft(args: PerftArgs) -> CliResult {
    let id = util::parse_wide_variant(&args.pos.variant)?;
    let pos = util::load_wide_variant(id, &args.pos.fen)?;
    println!("variant: {}", pos.variant_id());

    if !args.divide || args.depth == 0 {
        println!("depth {}: {} nodes", args.depth, pos.perft(args.depth));
        return Ok(());
    }

    let mut total = 0;
    for mv in pos.legal_moves() {
        let count = pos.play(&mv).perft(args.depth - 1);
        println!("{}: {count}", pos.to_uci(&mv));
        total += count;
    }
    println!();
    println!("depth {}: {total} nodes", args.depth);
    Ok(())
}

/// The fairy inspection: the board, side to move, check flag, the legal moves in
/// UCI, and the outcome if the position is terminal.
fn inspect(args: PositionArgs) -> CliResult {
    let id = util::parse_wide_variant(&args.variant)?;
    let pos = util::load_wide_variant(id, &args.fen)?;

    let fen = pos.to_fen();
    util::print_board_fen(&fen);
    println!();
    println!("variant:    {}", pos.variant_id());
    println!("fen:        {fen}");
    println!("side:       {}", util::side_name(pos.turn()));
    println!("in check:   {}", pos.is_check());

    println!();
    let moves = pos.legal_moves();
    println!(
        "legal moves ({}):  [UCI; SAN is standard-chess only]",
        moves.len()
    );
    for mv in &moves {
        println!("  {}", pos.to_uci(mv));
    }

    println!();
    match pos.outcome() {
        None => println!("outcome:    in progress"),
        Some(outcome) => println!(
            "outcome:    {} ({:?})",
            outcome_string(outcome),
            pos.end_reason()
        ),
    }
    Ok(())
}

/// Plays the given UCI moves in order and prints the resulting FEN.
fn play(args: PlayArgs) -> CliResult {
    let id = util::parse_wide_variant(&args.pos.variant)?;
    let mut pos = util::load_wide_variant(id, &args.pos.fen)?;
    for uci in &args.moves {
        pos = pos
            .play_uci(uci)
            .ok_or_else(|| util::CliError::new(format!("illegal or malformed move: {uci}")))?;
    }
    println!("{}", pos.to_fen());
    Ok(())
}

/// Renders a fairy outcome as a short phrase.
fn outcome_string(outcome: mce::geometry::WideOutcome) -> String {
    match outcome {
        mce::geometry::WideOutcome::Decisive { winner } => {
            format!("{} wins", util::side_name(winner))
        }
        mce::geometry::WideOutcome::Draw => "draw".to_owned(),
    }
}
