//! Integration tests for the `mce-uci` binary: drive it over stdin/stdout the
//! way an external UCI perft harness would, and check the handshake and the
//! Fairy-Stockfish-shaped `go perft` output.

use std::io::Write;
use std::process::{Command, Stdio};

/// Feeds `input` to a fresh `mce-uci` process and returns its stdout.
fn run(input: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mce-uci"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn mce-uci");
    child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(input.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait mce-uci");
    assert!(out.status.success(), "mce-uci exited with {:?}", out.status);
    String::from_utf8(out.stdout).expect("utf8 stdout")
}

/// The last `Nodes searched:` total in `text`, parsed.
fn nodes_searched(text: &str) -> u64 {
    text.lines()
        .filter_map(|l| l.strip_prefix("Nodes searched:"))
        .next_back()
        .expect("a Nodes searched line")
        .trim()
        .parse()
        .expect("node count")
}

#[test]
fn handshake_lists_variants_and_answers_ready() {
    let out = run("uci\nisready\nquit\n");
    assert!(out.contains("id name mce-uci"));
    assert!(out.contains("uciok"));
    assert!(out.contains("readyok"));
    // The combo option enumerates variants from both families.
    assert!(out.contains("option name UCI_Variant type combo"));
    assert!(out.contains("var chess"));
    assert!(out.contains("var xiangqi"));
    assert!(out.contains("var shogi"));
}

#[test]
fn standard_startpos_perft4() {
    let out = run("position startpos\ngo perft 4\nquit\n");
    assert_eq!(nodes_searched(&out), 197_281);
    // Divide lines present and parseable.
    assert!(out.lines().any(|l| l.starts_with("e2e4: ")));
}

#[test]
fn xiangqi_startpos_perft3() {
    let out =
        run("setoption name UCI_Variant value xiangqi\nposition startpos\ngo perft 3\nquit\n");
    assert_eq!(nodes_searched(&out), 79_666);
}

#[test]
fn shogi_startpos_perft3() {
    let out = run("setoption name UCI_Variant value shogi\nposition startpos\ngo perft 3\nquit\n");
    // Shogi perft(3) from the initial array.
    assert_eq!(nodes_searched(&out), 25_470);
}

#[test]
fn position_fen_and_moves_and_d() {
    let out = run(
        "position fen rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 moves e2e4\nd\nquit\n",
    );
    // The `d` output echoes a Fen line reflecting the applied move.
    assert!(out
        .lines()
        .any(|l| l.starts_with("Fen: ") && l.contains(" b ")));
}

#[test]
fn go_without_perft_is_not_a_search() {
    // A bare `go` must not emit a bestmove (this is a rules adapter, not a
    // search engine) and must not crash the process.
    let out = run("position startpos\ngo\nquit\n");
    assert!(!out.contains("bestmove"));
}

/// The last line with the given `prefix`, trimmed of the prefix and whitespace.
fn last_field<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.lines()
        .filter_map(|l| l.strip_prefix(prefix))
        .next_back()
        .map(str::trim)
}

#[test]
fn d_output_is_complete() {
    // The enriched `d` dump carries the grid, the FEN, the Zobrist key, the side
    // to move, the checker line, and the consolidated status.
    let out = run("position startpos\nd\nquit\n");
    assert!(out.lines().any(|l| l.starts_with("Fen: ")));
    assert!(out.lines().any(|l| l.starts_with("Key: ")));
    assert_eq!(last_field(&out, "Side to move:"), Some("white"));
    assert!(out.lines().any(|l| l.starts_with("Checkers:")));
    assert_eq!(last_field(&out, "Status:"), Some("ongoing"));
}

#[test]
fn status_command_reports_ongoing_and_checkmate() {
    let start = run("position startpos\nstatus\nquit\n");
    assert_eq!(last_field(&start, "Status:"), Some("ongoing"));

    // Fool's mate: Black mates; the status line names the winner and the reason.
    let mate = run("position startpos moves f2f3 e7e5 g2g4 d8h4\nstatus\nquit\n");
    assert_eq!(
        last_field(&mate, "Status:"),
        Some("decisive winner=black reason=Checkmate")
    );
}

#[test]
fn fairy_checkers_and_status_report_check() {
    // Almost Chess: Black king on e8 checked by a white rook on e1.
    let out = run(
        "setoption name UCI_Variant value almost\n\
         position fen 4k3/8/8/8/8/8/8/4R1K1 b - - 0 1\n\
         checkers\nstatus\nquit\n",
    );
    assert_eq!(last_field(&out, "Checkers:"), Some("e1"));
    assert_eq!(last_field(&out, "Status:"), Some("ongoing (check)"));
}

#[test]
fn fairy_pins_and_attacked_queries() {
    // A white bishop on e2 pinned to its king on e1 by a black rook on e8.
    let out = run(
        "setoption name UCI_Variant value almost\n\
         position fen 4r3/8/8/8/8/8/4B3/4K3 w - - 0 1\n\
         pins\nattacked white e2\nattacked black e2\nquit\n",
    );
    assert_eq!(last_field(&out, "Pinned:"), Some("e2"));
    // e2 is defended by the white king on e1, and also attacked by the black rook
    // on e8 (e2 is the rook's first blocker — the pinned bishop it could capture).
    let attacked: Vec<&str> = out
        .lines()
        .filter(|l| l.starts_with("Attacked:"))
        .collect();
    assert_eq!(attacked, ["Attacked: yes by e1", "Attacked: yes by e8"]);
}

#[test]
fn analysis_on_concrete_engine_degrades_gracefully() {
    // The concrete 8x8 engine does not expose the fairy analysis internals, so the
    // debug queries report on stderr and print nothing on stdout — without
    // crashing. (`stderr` is nulled by `run`, so stdout simply lacks the lines.)
    let out = run("position startpos\ncheckers\npins\nattacked white e4\nquit\n");
    assert!(!out.lines().any(|l| l.starts_with("Checkers:")));
    assert!(!out.lines().any(|l| l.starts_with("Pinned:")));
    assert!(!out.lines().any(|l| l.starts_with("Attacked:")));
}

#[test]
fn position_fen_edge_cases_do_not_crash() {
    // A bare `position fen` (no FEN), a malformed FEN, a bare `position`, an
    // illegal trailing move, and a bad `attacked` argument are all handled without
    // aborting the process; the following `status` still answers.
    let out = run(
        "position fen\n\
         position fen not a real fen\n\
         position\n\
         setoption name UCI_Variant value xiangqi\n\
         position startpos moves e2e4\n\
         attacked purple z9\n\
         position startpos\n\
         status\nquit\n",
    );
    // The process survived every malformed line and answered the final status.
    assert_eq!(last_field(&out, "Status:"), Some("ongoing"));
}

#[test]
fn checkers_square_names_match_divide_coordinates() {
    // The `checkers` / `attacked` coordinates use the same numbering as the
    // `go perft` divide moves: a rook checking down the e-file is reported as `e1`,
    // and a divide from that position lists moves in the same coordinate system.
    let out = run(
        "setoption name UCI_Variant value almost\n\
         position fen 4k3/8/8/8/8/8/8/4R1K1 b - - 0 1\n\
         checkers\ngo perft 1\nquit\n",
    );
    assert_eq!(last_field(&out, "Checkers:"), Some("e1"));
    // Every divide move origin is a coordinate the same parser reads (e.g. e8…).
    assert!(out.lines().any(|l| l.starts_with("e8")));
}
