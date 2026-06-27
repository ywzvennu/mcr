//! Differential fuzzing harness: mce vs the reference engine (shakmaty).
//!
//! Issue #109. This is the strongest ongoing correctness net for the engine: a
//! fully *seeded* loop that, for every variant, generates random legal positions
//! and asserts that mce and shakmaty agree on four independent properties of each
//! position:
//!
//! 1. **the legal-move SET** — compared as sorted UCI strings (so a missing,
//!    extra, or mis-rendered move is caught regardless of generation order);
//! 2. **shallow perft** — node counts at a fixed small depth must match exactly;
//! 3. **check / terminal status** — `is_check()` and the single-position game
//!    result (decisive winner / draw / ongoing) must agree;
//! 4. **FEN round-trip** — mce's `to_fen()` must re-parse to the same FEN, and
//!    must agree with shakmaty's serialization of the same position.
//!
//! Everything is driven by the seeded [`mce_compare::gen`] generator (splitmix64
//! from a fixed per-variant seed), so a run is byte-for-byte reproducible across
//! machines: the same `--count` always checks the same positions. On ANY
//! divergence the harness prints the exact FEN plus the differing data needed to
//! reproduce, and exits non-zero. A clean run reports the positions checked and
//! exits zero.
//!
//! ```text
//! cargo run --release --bin difftest                 # default budget (a few thousand / variant)
//! cargo run --release --bin difftest -- --full       # a much larger budget
//! cargo run --release --bin difftest -- --count 500  # explicit per-variant target
//! cargo run --release --bin difftest -- --depth 3    # override the shallow perft depth
//! cargo run --release --bin difftest -- --all        # accepted for compatibility; now a no-op
//! ```
//!
//! ## All nine variants run by default
//!
//! Every variant — including atomic and antichess — runs by default and is
//! expected to report **0 divergences**. The two move-generation bugs this
//! harness originally found (atomic king-adjacency legality, issue **#121**;
//! antichess multi-king move generation, issue **#122**) and the atomic
//! FEN-validation / check bug (adjacent kings wrongly rejected / reported in
//! check, issue **#134**) are all fixed and merged, so there is no longer a
//! default-skip allowlist. The legacy `--all` (alias `--include-known-bugs`)
//! flag is still accepted for script compatibility but is a no-op.
//!
//! If a genuine new divergence appears, it must be filed and fixed — never
//! masked by re-introducing a silent skip.
//!
//! ## GPL isolation
//!
//! Like the rest of `compare/`, this binary links GPL-3.0+ shakmaty for
//! cross-checking only. It is never published or distributed and is not part of
//! the `mce` library, which remains shakmaty-free and clean-room MIT OR
//! Apache-2.0.
//!
//! ## Why some generated positions are skipped (counted, never silent)
//!
//! shakmaty refuses a handful of positions mce accepts (e.g. a variant-terminal
//! position, or an over-material crazyhouse pocket). The generator already avoids
//! snapshotting terminal positions, but any position shakmaty rejects on parse is
//! recorded as a skip rather than a divergence — a surprise is surfaced, never
//! dropped.

use std::process::ExitCode;

use mce_compare::gen::{self, GenPos};
use mce_compare::runtime::{McePos, ShakPos};
use mce_compare::VARIANTS;

/// Per-variant position target for the default run. The seeded generator plays
/// enough games to yield at least this many *distinct* positions per variant
/// (capped by what random play reaches), so the default budget is several
/// thousand positions across all nine variants.
const DEFAULT_COUNT: usize = 400;
/// Per-variant target for `--full`: a much larger, still-reproducible sweep.
const FULL_COUNT: usize = 3000;

/// Shallow perft depth for the per-position cross-check. Small enough that the
/// whole default sweep finishes quickly, deep enough to exercise full move-gen
/// (and several plies of make/unmake) on every position.
const DEFAULT_DEPTH: u32 = 3;

/// Max plies per seeded game; snapshots spread across the game (opening→endgame).
const GEN_MAX_PLIES: u32 = 120;

/// Parsed command line.
struct Opts {
    /// Per-variant distinct-position target.
    count: usize,
    /// Shallow perft depth.
    depth: u32,
}

fn parse_args() -> Opts {
    let mut count = DEFAULT_COUNT;
    let mut depth = DEFAULT_DEPTH;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--full" => count = FULL_COUNT,
            // Legacy flags: all variants now run by default (see #121/#122/#134).
            // Accepted for script compatibility, but a no-op.
            "--all" | "--include-known-bugs" => {}
            "--count" => {
                count = args
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| fail_usage("--count needs a positive integer"));
            }
            "--depth" => {
                depth = args
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| fail_usage("--depth needs a non-negative integer"));
            }
            "--help" | "-h" => {
                println!("usage: difftest [--full] [--count N] [--depth D] [--all]");
                println!("  differential fuzz: mce vs shakmaty over seeded random positions");
                println!("  --full       large per-variant budget ({FULL_COUNT})");
                println!(
                    "  --count N    per-variant distinct-position target (default {DEFAULT_COUNT})"
                );
                println!("  --depth D    shallow perft depth (default {DEFAULT_DEPTH})");
                println!(
                    "  --all        no-op (alias --include-known-bugs); all 9 variants now \
run by default."
                );
                std::process::exit(0);
            }
            other => eprintln!("warning: ignoring unknown argument {other:?}"),
        }
    }
    Opts { count, depth }
}

fn fail_usage(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(2);
}

/// How a single position compared across the two engines.
enum Check {
    /// Both engines agreed on every property.
    Agree,
    /// shakmaty would not represent this position (rejected on parse); skipped.
    SkippedShakmaty,
    /// A perft mismatch reconciled as the documented terminal divergence: a
    /// variant terminal (king on the hill, third check, completed race, an
    /// exploded king in atomic, a captured side in antichess) is reachable within
    /// the shallow perft tree, so shakmaty prunes the decided line while mce keeps
    /// counting. Not a bug — counted and skipped, exactly as the perft-parity
    /// harness does. See `terminal_divergent`.
    SkippedTerminalDivergence,
    /// At least one property diverged; the message is the full reproducer.
    Divergence(String),
}

/// Variants whose terminal condition is *path-dependent* and can fire partway
/// down a perft tree from a non-terminal root, making the deep node count diverge
/// from shakmaty (which prunes at the variant terminal). For these, a perft
/// mismatch is reconciled as an incomparable skip *after confirming a terminal is
/// actually reachable within the tree* — never silently. Mirrors the same list in
/// the perft-parity harness (`main.rs`).
fn terminal_divergent(variant: &str) -> bool {
    // atomic (king exploded) and antichess (a side has no pieces) are likewise
    // path-dependent terminals now that their move-gen / validation bugs
    // (#121 / #122 / #134) are fixed, so their in-tree terminals are reconciled
    // here too rather than reported as divergences.
    matches!(
        variant,
        "king-of-the-hill" | "three-check" | "racing-kings" | "atomic" | "antichess"
    )
}

/// The per-variant running tally.
#[derive(Default)]
struct Tally {
    checked: usize,
    /// Positions shakmaty would not represent (rejected on parse).
    skipped: usize,
    /// Perft mismatches reconciled as the documented terminal divergence.
    terminal: usize,
    divergences: usize,
}

fn main() -> ExitCode {
    let opts = parse_args();

    println!("mce vs shakmaty — differential fuzz (issue #109)");
    #[cfg(feature = "magic")]
    println!("mce slider backend: magic bitboards (--features magic)");
    #[cfg(not(feature = "magic"))]
    println!("mce slider backend: hyperbola-quintessence (default)");
    println!("engines: mce (path) vs shakmaty 0.27");
    println!(
        "budget: up to {} distinct positions / variant, shallow perft depth {}",
        opts.count, opts.depth
    );
    println!("seed: fixed per-variant (splitmix64) — this run is fully reproducible");
    println!();

    // All nine variants run, every time. The atomic/antichess move-gen and
    // FEN-validation bugs (#121 / #122 / #134) are fixed, so there is no longer a
    // default-skip allowlist; this harness must report 0 divergences across all
    // variants. A genuine new divergence is filed and fixed, never re-masked.
    println!(
        "running ALL {} variants (no skip allowlist).",
        VARIANTS.len()
    );
    println!();

    let mut total = Tally::default();
    let mut first_failure: Option<String> = None;

    let head = format!(
        "{:<16} {:>10} {:>9} {:>10} {:>12}",
        "variant", "checked", "skipped", "term-skip", "divergences"
    );
    println!("{head}");
    println!("{}", "-".repeat(head.len()));

    for &variant in VARIANTS {
        let positions = generate_for(variant, opts.count);
        let mut t = Tally::default();
        for g in &positions {
            match check_position(variant, &g.fen, opts.depth) {
                Check::Agree => t.checked += 1,
                Check::SkippedShakmaty => t.skipped += 1,
                Check::SkippedTerminalDivergence => t.terminal += 1,
                Check::Divergence(report) => {
                    t.divergences += 1;
                    eprintln!("\n*** DIVERGENCE [{variant}] {} ***\n{report}", g.label);
                    if first_failure.is_none() {
                        first_failure = Some(format!("{variant} / {}", g.label));
                    }
                }
            }
        }
        println!(
            "{:<16} {:>10} {:>9} {:>10} {:>12}",
            variant, t.checked, t.skipped, t.terminal, t.divergences
        );
        total.checked += t.checked;
        total.skipped += t.skipped;
        total.terminal += t.terminal;
        total.divergences += t.divergences;
    }

    println!("{}", "-".repeat(head.len()));
    println!(
        "{:<16} {:>10} {:>9} {:>10} {:>12}",
        "TOTAL", total.checked, total.skipped, total.terminal, total.divergences
    );
    println!();

    if total.divergences == 0 {
        println!(
            "OK: {} positions checked across {} variants — \
mce agrees with shakmaty on legal moves, perft (d{}), check/terminal status, \
and FEN round-trip. {} skipped (shakmaty rejected the FEN); {} perft mismatches \
reconciled as the documented variant-terminal divergence (a terminal fires inside \
the perft tree — shakmaty prunes, mce counts on).",
            total.checked,
            VARIANTS.len(),
            opts.depth,
            total.skipped,
            total.terminal,
        );
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "ERROR: {} divergence(s) across {} positions checked (first: {}). \
Reproduce with the printed FEN.",
            total.divergences,
            total.checked + total.divergences,
            first_failure.as_deref().unwrap_or("?"),
        );
        ExitCode::FAILURE
    }
}

/// Generate at least `target` distinct positions for `variant` from the seeded
/// generator. The generator yields one position per snapshot stride; we scale the
/// game count so the deduped basket reaches the target (random play caps what is
/// reachable, so the actual count may be slightly under for low-branching
/// variants — that is fine and reported).
fn generate_for(variant: &'static str, target: usize) -> Vec<GenPos> {
    // Each game contributes roughly max_plies / stride (~20) snapshots before
    // dedup; budget generously so dedup still clears the target.
    let games = (target / 8 + 8) as u32;
    gen::generate_variant(variant, games, GEN_MAX_PLIES, target)
}

/// Cross-check a single position across both engines on all four properties.
///
/// Returns [`Check::SkippedShakmaty`] when shakmaty cannot represent the position
/// (counted, not failed), [`Check::Agree`] when every property matches, or
/// [`Check::Divergence`] with a complete, copy-pasteable reproducer otherwise.
fn check_position(variant: &str, fen: &str, depth: u32) -> Check {
    let Some(m) = McePos::parse(variant, fen) else {
        // The generator only emits mce-parseable FENs, so this is unreachable in
        // practice; treat a parse failure as a divergence to surface any drift.
        return Check::Divergence(format!(
            "  fen:    {fen}\n  mce failed to parse its own generated FEN"
        ));
    };
    let Some(s) = ShakPos::parse(variant, fen) else {
        return Check::SkippedShakmaty;
    };

    let mut diffs: Vec<String> = Vec::new();

    // 1. legal-move SET (sorted UCI).
    let m_moves = m.legal_ucis();
    let s_moves = s.legal_ucis();
    if m_moves != s_moves {
        let only_mce: Vec<&String> = m_moves.iter().filter(|u| !s_moves.contains(*u)).collect();
        let only_shak: Vec<&String> = s_moves.iter().filter(|u| !m_moves.contains(*u)).collect();
        diffs.push(format!(
            "  legal moves differ:\n    only in mce ({}): {:?}\n    only in shakmaty ({}): {:?}",
            only_mce.len(),
            only_mce,
            only_shak.len(),
            only_shak,
        ));
    }

    // 2. shallow perft. A mismatch in a terminal-divergent variant where a
    // variant terminal is actually reachable within `depth` plies is the
    // documented incomparable case (shakmaty prunes the decided line, mce counts
    // on); we reconcile it as a counted skip rather than a divergence — but only
    // after confirming a terminal is reachable, so a real movegen bug still fails.
    // The root position itself is non-terminal (the generator never snapshots a
    // terminal), so the move-set / check / terminal-status / FEN checks below
    // remain valid and are still asserted even when the deep perft is skipped.
    let mut terminal_skip = false;
    let m_perft = m.perft(depth);
    let s_perft = s.perft(depth);
    if m_perft != s_perft {
        if terminal_divergent(variant) && m.any_reaches_terminal(depth) {
            terminal_skip = true;
        } else {
            diffs.push(format!(
                "  perft(d{depth}) differ: mce={m_perft} shakmaty={s_perft}"
            ));
        }
    }

    // 3. check status + single-position terminal status.
    if m.is_check() != s.is_check() {
        diffs.push(format!(
            "  is_check differ: mce={} shakmaty={}",
            m.is_check(),
            s.is_check()
        ));
    }
    let (m_term, s_term) = (m.term_status(), s.term_status());
    if m_term != s_term {
        diffs.push(format!(
            "  terminal status differ: mce={m_term:?} shakmaty={s_term:?}"
        ));
    }

    // 4. FEN round-trip (mce self-consistency + agreement with shakmaty).
    match m.fen_roundtrip(variant) {
        None => diffs.push("  mce FEN round-trip failed: to_fen() did not re-parse".to_string()),
        Some(rt) if rt != m.to_fen() => diffs.push(format!(
            "  mce FEN round-trip changed the position:\n    before: {}\n    after:  {rt}",
            m.to_fen()
        )),
        Some(_) => {}
    }
    // Cross-engine FEN agreement (board, side, castling, ep, clocks).
    let (m_fen, s_fen) = (m.to_fen(), s.to_fen());
    if !fens_equivalent(&m_fen, &s_fen) {
        diffs.push(format!(
            "  serialized FEN differ:\n    mce:      {m_fen}\n    shakmaty: {s_fen}"
        ));
    }

    if !diffs.is_empty() {
        Check::Divergence(format!(
            "  variant: {variant}\n  fen:     {fen}\n{}",
            diffs.join("\n")
        ))
    } else if terminal_skip {
        // Every comparable property at the root matched; only the deep perft
        // diverged, and a variant terminal is reachable inside the tree — the
        // documented incomparable case.
        Check::SkippedTerminalDivergence
    } else {
        Check::Agree
    }
}

/// Compare two FENs for equivalence on the fields both engines agree to model,
/// modulo a few *cosmetic notation* differences that are not positional bugs.
///
/// The load-bearing FEN correctness check is mce's own `to_fen → from_fen`
/// round-trip (asserted separately). This cross-engine comparison is a second,
/// looser net: it confirms mce and shakmaty serialize the *same position* while
/// tolerating the known places where the two crates spell the same fact
/// differently:
///
/// * **placement / crazyhouse pocket** — we strip a trailing `[...]` pocket so an
///   empty `[]` vs an omitted pocket is not a difference; pocket *contents* are
///   already validated through the drop moves in the legal-move set.
/// * **castling rights** — chess960 has two equivalent encodings: mce emits
///   Shredder file-letters (`GBgb`) while shakmaty in `CastlingMode::Chess960`
///   emits X-FEN shorthand (`KQkq`) for a standard-arrangement back rank. These
///   denote the *same* rights, so we compare only **which sides still hold a
///   castling right** (white-has-any / black-has-any by letter case), not the
///   exact letters. Whether each specific castle is *legal* is independently and
///   exactly checked through the legal-move set.
/// * **en-passant square** — mce writes the X-FEN ep square whenever a pawn just
///   double-pushed; shakmaty (`EnPassantMode::Legal`) writes it only when a legal
///   ep capture exists. We require agreement only when *both* name a square (a
///   `-` vs a square is the documented ep-mode convention difference); the
///   *availability* of the ep capture is, again, checked through the move set.
/// * **move clocks** — ignored (cosmetic; not compared).
fn fens_equivalent(a: &str, b: &str) -> bool {
    let fa: Vec<&str> = a.split_whitespace().collect();
    let fb: Vec<&str> = b.split_whitespace().collect();
    if fa.is_empty() || fb.is_empty() {
        return false;
    }
    // Field 0: placement (pocket-stripped).
    let strip_pocket = |s: &str| -> String { s.split('[').next().unwrap_or(s).to_string() };
    if strip_pocket(fa[0]) != strip_pocket(fb[0]) {
        return false;
    }
    // Field 1: side to move.
    if fa.get(1) != fb.get(1) {
        return false;
    }
    // Field 2: castling rights, compared as (white-has-any, black-has-any) to
    // bridge the Shredder vs X-FEN encodings.
    let sides_with_rights = |s: Option<&&str>| -> (bool, bool) {
        let v = s.copied().unwrap_or("-");
        let white = v.chars().any(|c| c != '-' && c.is_ascii_uppercase());
        let black = v.chars().any(|c| c != '-' && c.is_ascii_lowercase());
        (white, black)
    };
    if sides_with_rights(fa.get(2)) != sides_with_rights(fb.get(2)) {
        return false;
    }
    // Field 3: en-passant. Allow `-` vs a square (an ep-mode convention
    // difference), but if BOTH name a square they must name the same one.
    match (fa.get(3), fb.get(3)) {
        (Some(&x), Some(&y)) if x != "-" && y != "-" => x == y,
        _ => true,
    }
}
