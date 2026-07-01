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
