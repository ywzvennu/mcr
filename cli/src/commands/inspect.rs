//! `mce inspect` — dump everything the library knows about a position.
//!
//! For standard chess this prints the ASCII board, side to move, castling
//! rights, en-passant target, move clocks, checkers, every legal move in both
//! SAN and UCI, and the outcome if the position is already terminal. For a
//! variant (`--variant`) the same surface is shown via the runtime-dispatch
//! `AnyVariant`, which renders moves in UCI (SAN is a standard-chess feature of
//! the library's `Position`).

use clap::Args;

use crate::util::{self, CliResult};

/// Arguments for `mce inspect <FEN>`.
#[derive(Debug, Args)]
pub struct InspectArgs {
    /// Position to inspect: a six-field FEN, or `startpos`.
    fen: String,
    /// Variant rule set (default: standard chess).
    #[arg(long, value_name = "V")]
    variant: Option<String>,
}

pub fn run(args: InspectArgs) -> CliResult {
    let fen = util::resolve_fen(&args.fen);
    let id = util::parse_variant(args.variant.as_deref())?;

    if util::is_standard(id) {
        inspect_standard(&util::load_position(fen)?);
    } else {
        inspect_variant(&util::load_variant(id, fen)?);
    }
    Ok(())
}

/// The rich standard-chess inspection (SAN + UCI moves, checkers, clocks).
fn inspect_standard(pos: &mce::Position) {
    util::print_board(pos.board());
    println!();
    println!("fen:        {}", pos.to_fen());
    println!("side:       {}", util::side_name(pos.turn()));
    println!("castling:   {}", util::castling_string(pos));
    println!(
        "en passant: {}",
        pos.ep_square().map_or("-".to_owned(), |sq| sq.to_string())
    );
    println!("halfmove:   {}", pos.halfmove_clock());
    println!("fullmove:   {}", pos.fullmove_number());
    println!("checkers:   {}", util::squares_string(pos.checkers()));

    println!();
    let moves = pos.legal_moves();
    println!("legal moves ({}):", moves.len());
    for mv in &moves {
        println!("  {:<7} {}", pos.san(mv), mv.to_uci());
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
}

/// The variant inspection via `AnyVariant`: UCI moves and the variant-aware
/// check / outcome surface. (SAN and the FEN side fields are read off the FEN.)
fn inspect_variant(pos: &mce::AnyVariant) {
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
}

/// Renders an outcome as a short phrase.
fn outcome_string(outcome: mce::Outcome) -> String {
    match outcome {
        mce::Outcome::Decisive { winner } => format!("{} wins", util::side_name(winner)),
        mce::Outcome::Draw => "draw".to_owned(),
    }
}
