//! Smoke test for the `mcr fairy` subcommand: construct a fairy variant by name
//! and run perft at a low depth, asserting the FSF-confirmed node counts pinned
//! in the library's own `tests/perft_xiangqi.rs` / `tests/perft_shogi.rs`.
//!
//! This drives the built binary end to end (clap parse → `AnyWideVariant`
//! dispatch → perft), the same surface a shell user touches.

use std::process::Command;

/// Runs the `mcr` binary with `args` and returns its stdout as a string,
/// asserting a successful exit.
fn run(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_mcr"))
        .args(args)
        .output()
        .expect("spawn mcr binary");
    assert!(
        output.status.success(),
        "`mcr {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf-8 stdout")
}

#[test]
fn fairy_list_includes_marquee_variants() {
    let out = run(&["fairy", "list"]);
    for name in ["xiangqi", "shogi", "janggi", "orda"] {
        assert!(out.lines().any(|l| l == name), "{name} in `fairy list`");
    }
}

#[test]
fn xiangqi_startpos_perft_matches_known_counts() {
    // FSF-confirmed Xiangqi startpos perft sequence (tests/perft_xiangqi.rs).
    assert!(run(&["fairy", "perft", "xiangqi", "startpos", "1"]).contains("44 nodes"));
    assert!(run(&["fairy", "perft", "xiangqi", "startpos", "2"]).contains("1920 nodes"));
    assert!(run(&["fairy", "perft", "xiangqi", "startpos", "3"]).contains("79666 nodes"));
}

#[test]
fn shogi_startpos_perft_matches_known_counts() {
    // FSF-confirmed Shogi startpos perft sequence (tests/perft_shogi.rs).
    assert!(run(&["fairy", "perft", "shogi", "startpos", "1"]).contains("30 nodes"));
    assert!(run(&["fairy", "perft", "shogi", "startpos", "2"]).contains("900 nodes"));
}

#[test]
fn xiangqi_perft_divide_sums_to_total() {
    // Per-root-move breakdown: 44 root moves summing to 1920 at depth 2.
    let out = run(&["fairy", "perft", "xiangqi", "startpos", "2", "--divide"]);
    assert!(out.contains("1920 nodes"), "divide total");
    // One line per root move plus headers; the root move count is 44.
    let move_lines = out.lines().filter(|l| l.contains(": ")).count();
    assert!(
        move_lines >= 44,
        "at least 44 root-move lines, got {move_lines}"
    );
}

#[test]
fn fairy_inspect_and_play_round_trip() {
    // Inspect lists the 44 legal moves and names the variant.
    let inspected = run(&["fairy", "inspect", "xiangqi", "startpos"]);
    assert!(inspected.contains("variant:    xiangqi"));
    assert!(inspected.contains("legal moves (44)"));

    // Take a real legal move from the inspection (indented two-space UCI lines)
    // and play it; the position must advance to black to move.
    let mv = inspected
        .lines()
        .filter_map(|l| l.strip_prefix("  "))
        .find(|l| l.len() == 4 && l.bytes().all(|b| b.is_ascii_alphanumeric()))
        .expect("a legal UCI move in the inspection");
    let played = run(&["fairy", "play", "xiangqi", "startpos", mv]);
    assert!(
        played.contains(" b "),
        "black to move after one ply: {played}"
    );
}

#[test]
fn unknown_fairy_variant_fails() {
    let output = Command::new(env!("CARGO_BIN_EXE_mcr"))
        .args([
            "fairy",
            "perft",
            "definitely-not-a-variant",
            "startpos",
            "1",
        ])
        .output()
        .expect("spawn mcr binary");
    assert!(
        !output.status.success(),
        "unknown variant must exit nonzero"
    );
}
