//! `mcr convert` — convert between FEN and PGN on stdin/stdout.
//!
//! Supported directions (`--from <fmt> --to <fmt>`), reading stdin and writing
//! stdout:
//!
//! - `--from pgn --to fen` — PGN game (mainline) → the final position's FEN.
//! - `--from pgn --to pgn` — parse + re-emit canonical PGN (round-trip / tidy).
//! - `--from fen --to summary` — FEN → a one-line summary (variant, side, legal
//!   move count, outcome).
//! - `--from fen --to fen` — parse + re-emit the normalized FEN.
//!
//! The start position and variant of a PGN are taken from its header tags
//! (`[Variant]`, `[SetUp]`, `[FEN]`); a bare FEN is read as standard chess unless
//! it parses only under a variant — pass `--variant` if a variant FEN is meant.

use std::io::Read;

use clap::{Args, ValueEnum};

use mcr::Pgn;

use crate::util::{self, CliError, CliResult};

/// The input/output formats `mcr convert` understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// FEN (a single position).
    Fen,
    /// PGN (a single game, mainline only).
    Pgn,
    /// A one-line human summary (output only).
    Summary,
}

/// Arguments for `mcr convert`.
#[derive(Debug, Args)]
pub struct ConvertArgs {
    /// Input format on stdin.
    #[arg(long, value_enum)]
    from: Format,
    /// Output format on stdout.
    #[arg(long, value_enum)]
    to: Format,
    /// Variant for a FEN input (default: standard chess). PGN inputs carry their
    /// own variant in the header.
    #[arg(long, value_name = "V")]
    variant: Option<String>,
}

pub fn run(args: ConvertArgs) -> CliResult {
    let input = read_stdin()?;
    match (args.from, args.to) {
        (Format::Pgn, Format::Fen) => pgn_to_fen(&input),
        (Format::Pgn, Format::Pgn) => pgn_to_pgn(&input),
        (Format::Fen, Format::Summary) => fen_to_summary(&input, args.variant.as_deref()),
        (Format::Fen, Format::Fen) => fen_to_fen(&input, args.variant.as_deref()),
        (from, to) => Err(CliError::new(format!(
            "unsupported conversion {from:?} -> {to:?}; supported: \
             pgn->fen, pgn->pgn, fen->summary, fen->fen"
        ))),
    }
}

/// Reads all of stdin into a `String`.
fn read_stdin() -> CliResult<String> {
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|err| CliError::new(format!("could not read stdin: {err}")))?;
    Ok(buf)
}

/// Parses a PGN from the input text, mapping the error to a `CliError`.
fn parse_pgn(input: &str) -> CliResult<Pgn> {
    Pgn::from_pgn(input).map_err(|err| CliError::new(format!("bad PGN: {err}")))
}

/// PGN(mainline) → the final position's FEN.
fn pgn_to_fen(input: &str) -> CliResult {
    let pgn = parse_pgn(input)?;
    println!("{}", pgn.final_position().to_fen());
    Ok(())
}

/// PGN → canonical PGN (parse and re-serialize).
fn pgn_to_pgn(input: &str) -> CliResult {
    let pgn = parse_pgn(input)?;
    print!("{}", pgn.to_pgn());
    Ok(())
}

/// FEN → a one-line summary: variant, side to move, legal-move count, outcome.
fn fen_to_summary(input: &str, variant: Option<&str>) -> CliResult {
    let fen = util::resolve_fen(input.trim());
    let id = util::parse_variant(variant)?;
    let pos = util::load_variant(id, fen)?;

    let side = util::side_name(pos.turn());
    let count = pos.legal_moves().len();
    let outcome = match pos.outcome() {
        None => "in progress".to_owned(),
        Some(mcr::Outcome::Decisive { winner }) => format!("{} wins", util::side_name(winner)),
        Some(mcr::Outcome::Draw) => "draw".to_owned(),
    };

    println!(
        "{} | {} to move | {count} legal move(s) | {outcome} | {}",
        pos.variant_id(),
        side,
        pos.to_fen()
    );
    Ok(())
}

/// FEN → normalized FEN (parse and re-serialize).
fn fen_to_fen(input: &str, variant: Option<&str>) -> CliResult {
    let fen = util::resolve_fen(input.trim());
    let id = util::parse_variant(variant)?;
    let pos = util::load_variant(id, fen)?;
    println!("{}", pos.to_fen());
    Ok(())
}
