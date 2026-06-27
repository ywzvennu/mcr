//! Minimal UCI-ish loop showing how to wire the library into an engine front-end.
//!
//! This is NOT a real engine — there is no search and no evaluation. It only
//! demonstrates the parsing a UCI host needs: setting up a board from
//! `position startpos|fen ... [moves ...]` and answering `go perft N` with a
//! perft node count and divide. That is enough to show the integration shape;
//! a real engine would replace `go perft` with a `go` that runs a search.
//!
//! It reads commands from stdin, one per line, so you can pipe a script in:
//!
//! ```text
//! printf 'position startpos moves e2e4 e7e5\ngo perft 3\nquit\n' \
//!     | cargo run --example uci_demo
//! ```
//!
//! Recognized commands:
//!
//! - `uci`            — identify and reply `uciok`
//! - `isready`        — reply `readyok`
//! - `ucinewgame`     — reset to the start position
//! - `position startpos [moves <uci>...]`
//! - `position fen <6-field FEN> [moves <uci>...]`
//! - `go perft <depth>`
//! - `d`              — dump the current FEN (a common engine debug command)
//! - `quit`           — exit

use std::io::{self, BufRead, Write};

use mce::{perft, perft_divide, Position};

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    // The single piece of state a UCI host maintains: the current position.
    let mut pos = Position::startpos();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => break,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let mut tokens = line.split_whitespace();
        let command = tokens.next().unwrap_or("");

        match command {
            "uci" => {
                let _ = writeln!(out, "id name mce-uci-demo");
                let _ = writeln!(out, "id author mce");
                let _ = writeln!(out, "uciok");
            }
            "isready" => {
                let _ = writeln!(out, "readyok");
            }
            "ucinewgame" => {
                pos = Position::startpos();
            }
            "position" => match parse_position(&mut tokens) {
                Ok(new_pos) => pos = new_pos,
                Err(err) => eprintln!("info string error: {err}"),
            },
            "go" => {
                // The only `go` subcommand we implement is `perft N`.
                if tokens.next() == Some("perft") {
                    match tokens.next().and_then(|d| d.parse::<u32>().ok()) {
                        Some(depth) => print_perft(&mut out, &pos, depth),
                        None => eprintln!("info string error: go perft needs a depth"),
                    }
                } else {
                    eprintln!("info string only 'go perft <depth>' is supported");
                }
            }
            "d" => {
                let _ = writeln!(out, "{}", pos.to_fen());
            }
            "quit" => break,
            other => eprintln!("info string unknown command: {other}"),
        }
        let _ = out.flush();
    }
}

/// Builds a [`Position`] from the tokens after the `position` keyword:
/// `startpos | fen <f1> <f2> <f3> <f4> <f5> <f6>`, optionally followed by
/// `moves <uci> <uci> ...`.
fn parse_position<'a>(tokens: &mut impl Iterator<Item = &'a str>) -> Result<Position, String> {
    let mut pos = match tokens.next() {
        Some("startpos") => Position::startpos(),
        Some("fen") => {
            // A FEN is exactly six space-separated fields; collect them back into
            // one string for the parser.
            let fields: Vec<&str> = tokens.by_ref().take(6).collect();
            if fields.len() < 6 {
                return Err("fen needs six fields".to_owned());
            }
            Position::from_fen(&fields.join(" ")).map_err(|e| format!("bad fen: {e}"))?
        }
        Some(other) => return Err(format!("expected 'startpos' or 'fen', got {other:?}")),
        None => return Err("position needs 'startpos' or 'fen'".to_owned()),
    };

    // An optional `moves` section applies UCI moves in order, each validated
    // against the running position.
    match tokens.next() {
        Some("moves") => {
            for uci in tokens.by_ref() {
                let mv = pos
                    .parse_uci(uci)
                    .map_err(|e| format!("illegal/invalid move {uci:?}: {e}"))?;
                pos = pos.play(&mv);
            }
        }
        Some(other) => return Err(format!("expected 'moves' or end of line, got {other:?}")),
        None => {}
    }

    Ok(pos)
}

/// Prints a perft divide and total in a format a UCI host would recognize: one
/// `move: count` line per root move, then `Nodes searched: <total>`.
fn print_perft(out: &mut impl Write, pos: &Position, depth: u32) {
    let mut total = 0;
    for (mv, count) in perft_divide(pos, depth) {
        let _ = writeln!(out, "{}: {count}", mv.to_uci());
        total += count;
    }
    // Cross-check the divide total against a direct whole-tree perft.
    debug_assert_eq!(total, perft(pos, depth));
    let _ = writeln!(out);
    let _ = writeln!(out, "Nodes searched: {total}");
}
