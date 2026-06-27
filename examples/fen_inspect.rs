//! FEN inspector: parse a position and dump everything the library knows about it.
//!
//! Given a six-field FEN it prints an ASCII board, the side to move, castling
//! rights, the en-passant target, the move clocks, the set of checkers, every
//! legal move in both SAN and UCI, and the game outcome if the position is
//! already terminal.
//!
//! ```text
//! cargo run --example fen_inspect -- "<FEN>"
//! cargo run --example fen_inspect -- startpos
//! cargo run --example fen_inspect -- "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3"
//! ```
//!
//! `startpos` is accepted as a shorthand for the standard starting FEN.

use std::process::ExitCode;

use mce::{CastleSide, Color, File, Position, Rank, Square};

const STARTPOS_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 1 {
        eprintln!("usage: fen_inspect <FEN|startpos>");
        return ExitCode::FAILURE;
    }

    let fen = if args[0] == "startpos" {
        STARTPOS_FEN
    } else {
        &args[0]
    };

    let pos = match Position::from_fen(fen) {
        Ok(pos) => pos,
        Err(err) => {
            eprintln!("error: bad FEN: {err}");
            return ExitCode::FAILURE;
        }
    };

    print_board(&pos);
    println!();
    println!("fen:        {}", pos.to_fen());
    println!("side:       {}", side_name(pos.turn()));
    println!("castling:   {}", castling_string(&pos));
    println!(
        "en passant: {}",
        pos.ep_square().map_or("-".to_owned(), |sq| sq.to_string())
    );
    println!("halfmove:   {}", pos.halfmove_clock());
    println!("fullmove:   {}", pos.fullmove_number());
    println!("checkers:   {}", squares_string(pos.checkers()));

    println!();
    let moves = pos.legal_moves();
    println!("legal moves ({}):", moves.len());
    for mv in &moves {
        // SAN is the human-readable form; UCI is the long-algebraic form engines
        // exchange. Printing both makes the example useful as a quick reference.
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

    ExitCode::SUCCESS
}

/// Prints the position as an 8x8 grid, rank 8 at the top, with file and rank
/// labels. Empty squares show as a dot.
fn print_board(pos: &Position) {
    let board = pos.board();
    for rank_idx in (0..8).rev() {
        let rank = Rank::new(rank_idx).expect("0..8 is a valid rank index");
        print!("{}  ", rank.char());
        for file_idx in 0..8 {
            let file = File::new(file_idx).expect("0..8 is a valid file index");
            let sq = Square::from_file_rank(file, rank);
            match board.piece_at(sq) {
                // Piece::char yields the FEN letter: uppercase white, lowercase black.
                Some(piece) => print!("{} ", piece.char()),
                None => print!(". "),
            }
        }
        println!();
    }
    print!("   ");
    for file_idx in 0..8 {
        let file = File::new(file_idx).expect("0..8 is a valid file index");
        print!("{} ", file.char());
    }
    println!();
}

fn side_name(color: Color) -> &'static str {
    if color.is_white() {
        "white"
    } else {
        "black"
    }
}

/// Renders castling rights in the FEN order (KQkq), or `-` when none remain.
fn castling_string(pos: &Position) -> String {
    let rights = pos.castling_rights();
    let mut s = String::new();
    for (color, side, letter) in [
        (Color::White, CastleSide::King, 'K'),
        (Color::White, CastleSide::Queen, 'Q'),
        (Color::Black, CastleSide::King, 'k'),
        (Color::Black, CastleSide::Queen, 'q'),
    ] {
        if rights.has(color, side) {
            s.push(letter);
        }
    }
    if s.is_empty() {
        s.push('-');
    }
    s
}

/// Renders a bitboard of squares as a space-separated list, or `-` when empty.
fn squares_string(bb: mce::Bitboard) -> String {
    if bb.is_empty() {
        return "-".to_owned();
    }
    bb.into_iter()
        .map(|sq| sq.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

fn outcome_string(outcome: mce::Outcome) -> String {
    match outcome {
        mce::Outcome::Decisive { winner } => format!("{} wins", side_name(winner)),
        mce::Outcome::Draw => "draw".to_owned(),
    }
}
