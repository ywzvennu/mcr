//! Perft (performance test) node counter.
//!
//! Perft walks the legal-move tree to a fixed depth and counts the leaf nodes.
//! It is the standard correctness and speed check for a move generator: the node
//! count for a given position and depth is a fixed number that every correct
//! engine must reproduce. The per-root-move breakdown ("divide") is the usual
//! way to bisect a discrepancy down to the move that misbehaves.
//!
//! Run it with a FEN, a depth, and an optional variant name:
//!
//! ```text
//! cargo run --example perft -- "<FEN>" <depth> [variant]
//! cargo run --example perft -- startpos 5
//! cargo run --example perft -- startpos 5 atomic
//! ```
//!
//! `startpos` is accepted as a shorthand for the standard starting FEN. The
//! variant name is anything [`VariantId`] accepts (`chess`, `atomic`, `koth`,
//! `3check`, `crazyhouse`, ...); omit it for standard chess.

use std::process::ExitCode;

use mce::{AnyVariant, Position, VariantId};

const STARTPOS_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

fn main() -> ExitCode {
    // args[0] is the program name; the user arguments follow.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 || args.len() > 3 {
        eprintln!("usage: perft <FEN|startpos> <depth> [variant]");
        eprintln!("  e.g. perft startpos 5");
        eprintln!("       perft \"r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1\" 4");
        eprintln!("       perft startpos 4 atomic");
        return ExitCode::FAILURE;
    }

    let fen = if args[0] == "startpos" {
        STARTPOS_FEN
    } else {
        &args[0]
    };

    let depth: u32 = match args[1].parse() {
        Ok(d) => d,
        Err(_) => {
            eprintln!(
                "error: depth must be a non-negative integer, got {:?}",
                args[1]
            );
            return ExitCode::FAILURE;
        }
    };

    // With three arguments the last names a variant; route through the runtime
    // dispatch enum so a single code path handles every rule set.
    if let Some(name) = args.get(2) {
        let id: VariantId = match name.parse() {
            Ok(id) => id,
            Err(err) => {
                eprintln!("error: {err}");
                return ExitCode::FAILURE;
            }
        };
        let pos = match AnyVariant::from_fen(id, fen) {
            Ok(pos) => pos,
            Err(err) => {
                eprintln!("error: bad FEN for {id}: {err}");
                return ExitCode::FAILURE;
            }
        };
        run_variant(&pos, depth);
    } else {
        let pos = match Position::from_fen(fen) {
            Ok(pos) => pos,
            Err(err) => {
                eprintln!("error: bad FEN: {err}");
                return ExitCode::FAILURE;
            }
        };
        run_standard(&pos, depth);
    }

    ExitCode::SUCCESS
}

/// Prints the per-root-move breakdown and total for a standard position.
fn run_standard(pos: &Position, depth: u32) {
    let mut total = 0;
    // perft_divide gives the leaf count under each legal root move, the usual
    // way to localize a movegen bug to a single move.
    for (mv, count) in mce::perft_divide(pos, depth) {
        println!("{}: {count}", mv.to_uci());
        total += count;
    }
    println!();
    println!("depth {depth}: {total} nodes");
}

/// Prints the per-root-move breakdown and total for a runtime-chosen variant.
///
/// [`AnyVariant`] exposes a whole-tree [`AnyVariant::perft`] but no divide, so we
/// build the breakdown ourselves: play each legal root move and run a perft one
/// ply shallower from the resulting position.
fn run_variant(pos: &AnyVariant, depth: u32) {
    println!("variant: {}", pos.variant_id());
    let mut total = 0;
    if depth == 0 {
        // A zero-depth perft counts the current node itself: exactly one.
        println!();
        println!("depth 0: 1 node");
        return;
    }
    for mv in pos.legal_moves() {
        let count = pos.play(&mv).perft(depth - 1);
        println!("{}: {count}", pos.to_uci(&mv));
        total += count;
    }
    println!();
    println!("depth {depth}: {total} nodes");
}
