//! `mcr` — the unified command-line front end for the mcr chess library.
//!
//! A single binary consolidating the repo's example tools and a few new
//! utilities, so the library is usable straight from the shell:
//!
//! - `mcr perft <FEN|startpos> <depth>` — node counts, optionally per-root-move
//!   (`--divide`), parallel (`--parallel`), and per variant (`--variant`).
//! - `mcr inspect <FEN>` — an ASCII board plus everything the library knows
//!   about the position.
//! - `mcr play` — seeded random self-play, printing the move list and result.
//! - `mcr convert` — PGN(mainline) → final FEN, or FEN → one-line summary.
//! - `mcr validate <FEN>` — parse + validate a FEN, exit 0 / nonzero.
//!
//! Every subcommand uses only the public mcr API and reports bad input as an
//! error on stderr with a nonzero exit code rather than panicking.

use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod commands;
mod util;

/// Top-level command-line interface.
#[derive(Debug, Parser)]
#[command(
    name = "mcr",
    version,
    about = "Unified command-line tool for the mcr chess rules library",
    long_about = None,
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// The subcommands exposed by the `mcr` binary.
#[derive(Debug, Subcommand)]
enum Command {
    /// Count the leaf nodes of the move tree to a fixed depth (perft).
    Perft(commands::perft::PerftArgs),
    /// Dump everything the library knows about a position.
    Inspect(commands::inspect::InspectArgs),
    /// Play a seeded random game and print the moves and result.
    Play(commands::play::PlayArgs),
    /// Convert between FEN and PGN on stdin/stdout.
    Convert(commands::convert::ConvertArgs),
    /// Parse and validate a FEN; exit 0 on success, nonzero on failure.
    Validate(commands::validate::ValidateArgs),
    /// Work with the geometry-layer fairy variants (xiangqi, shogi, janggi, …).
    Fairy(commands::fairy::FairyArgs),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Perft(args) => commands::perft::run(args),
        Command::Inspect(args) => commands::inspect::run(args),
        Command::Play(args) => commands::play::run(args),
        Command::Convert(args) => commands::convert::run(args),
        Command::Validate(args) => commands::validate::run(args),
        Command::Fairy(args) => commands::fairy::run(args),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}
