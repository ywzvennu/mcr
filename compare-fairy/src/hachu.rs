//! HaChu large-shogi differential oracle mode (issue #379).
//!
//! Wires HaChu into the harness as a SECOND differential oracle, alongside
//! Fairy-Stockfish, for the large-shogi variants FSF does not cover — Chu, Dai,
//! and Tenjiku shogi. HaChu (H.G. Muller's reference engine) is driven purely as
//! a GPL-style SUBPROCESS oracle: never linked, never source-copied (same fence
//! as FSF). Locating/building it lives in `locate_hachu.rs`; the CECP driver is
//! `xboard.rs`.
//!
//! ## What this mode does today
//!
//! Run with `cargo run --release -- --hachu` (add `--build-hachu` to clone +
//! compile HaChu into the git-ignored `build/hachu/` dir if no binary is found):
//!
//! 1. locate (or build) a `hachu` binary, or SKIP cleanly with build
//!    instructions if none is available (mirrors the FSF skip);
//! 2. complete the `xboard` / `protover 2` handshake and capture HaChu's
//!    advertised `variants="..."` list;
//! 3. confirm the large-shogi variants needed downstream (`chu`, `dai`,
//!    `tenjiku`) are advertised — the readiness signal for the Chu-Shogi work in
//!    issue #380;
//! 4. drive HaChu to one concrete large-shogi position (the Chu-Shogi start
//!    position, established by `variant chu`) and confirm the oracle stays
//!    responsive — i.e. the harness can position and question HaChu about a real
//!    large-shogi board.
//!
//! ## The node-by-node perft comparison (Dai Shogi, issue #401)
//!
//! HaChu has no native `perft` command, so a node-by-node perft/divide is driven
//! *externally*: the harness reads HaChu's generated move list for a position by
//! handing it a deliberately illegal `usermove`, which under HaChu's always-on
//! debug output makes it print its full move list (see
//! [`Engine::dump_legal_moves`](crate::xboard::Engine::dump_legal_moves)). With
//! mce's Dai Shogi variant now implemented (`mce::geometry::Dai`), this mode walks
//! the start-position tree and cross-checks mce against the oracle:
//!
//! * **perft(1)** — the full legal-move set, node-for-node (71 moves);
//! * **perft(2)** — a divide (each root move's Black-reply count).
//!
//! HaChu 0.23 segfaults nondeterministically on the 15x15 board, so each node is a
//! fresh subprocess retried a few times; nodes that never yield a dump are reported
//! as HaChu crashes and skipped (oracle flakiness, not move differences). Chu is
//! still driven for the readiness handshake above; its perft was validated
//! separately (issue #380, see `tests/perft_chu.rs`).
//!
//! GPL FENCE unchanged: HaChu is spawned as a subprocess (see `xboard.rs`); no
//! HaChu code is linked or vendored, and the built binary is git-ignored.

use crate::locate_hachu::{self, Source};
use crate::xboard::Engine;

/// The large-shogi variants this oracle must advertise to be useful for the
/// Chu-Shogi work (issue #380). Chu is the immediate target; Dai and Tenjiku are
/// the follow-on large-shogi boards HaChu also serves.
const REQUIRED_VARIANTS: &[&str] = &["chu", "dai", "tenjiku"];

/// Run the HaChu-oracle mode: confirm the large-shogi variants, drive the Chu
/// readiness handshake, then run the Dai Shogi external perft comparison (issue
/// #401). Returns the number of real parity mismatches (0 when mce matches HaChu at
/// the validated depths); a missing HaChu binary or an absent Dai move dump skips
/// cleanly, returning 0, exactly like the FSF-absent skip.
pub fn run(build: bool) -> usize {
    println!();
    println!("HaChu large-shogi differential oracle (issue #379):");

    // ---- locate (or build) the HaChu binary ------------------------------
    let located = match locate_hachu::locate(build) {
        Ok(l) => l,
        Err(reason) => {
            println!("  SKIP: {reason}");
            println!();
            println!("{}", locate_hachu::INSTALL_HELP);
            return 0;
        }
    };
    let src = match &located.source {
        Source::Env => "env $MCE_HACHU_BIN".to_string(),
        Source::Path(n) => format!("PATH ({n})"),
        Source::Prebuilt(p) => format!("prebuilt {}", p.display()),
        Source::Built(p) => format!("built {}", p.display()),
    };
    println!("  HaChu binary: {} (via {src})", located.bin);

    // ---- handshake + variant advertisement -------------------------------
    let mut engine = match Engine::spawn(&located.bin) {
        Ok(e) => e,
        Err(e) => {
            println!("  SKIP: could not start HaChu over XBoard/CECP: {e}");
            println!();
            println!("{}", locate_hachu::INSTALL_HELP);
            return 0;
        }
    };
    println!("  advertised variants: {}", engine.variants().join(", "));

    let missing: Vec<&str> = REQUIRED_VARIANTS
        .iter()
        .copied()
        .filter(|v| !engine.has_variant(v))
        .collect();
    if !missing.is_empty() {
        println!(
            "  SKIP: this HaChu binary does not advertise the large-shogi variant(s): {}",
            missing.join(", ")
        );
        engine.quit();
        return 0;
    }
    println!(
        "  large-shogi variants present: {} (oracle covers Chu / Dai / Tenjiku)",
        REQUIRED_VARIANTS.join(", ")
    );

    // `variant chu` establishes the Chu-Shogi start position internally; a
    // ping/pong round-trip (inside `select_variant`) confirms the oracle is
    // positioned and responsive on a real 12x12 large-shogi board.
    if let Err(e) = engine.select_variant("chu") {
        println!("  SKIP: HaChu did not accept `variant chu`: {e}");
        engine.quit();
        return 0;
    }
    println!("  driven to Chu-Shogi start position (variant chu); oracle responsive.");
    // This handshake engine is done; the Dai walk spawns fresh subprocesses per
    // node (HaChu 0.23 is fragile across many `usermove`s, so a per-node process is
    // the reliable unit).
    engine.quit();

    // ---- Dai Shogi (15x15) external perft comparison (issue #401) ---------
    compare_dai(&located.bin)
}

/// The coordinate string for an mce Dai move on the 15x15 board (files `a..o`,
/// ranks `1..15`), matching HaChu's move-dump notation. Promotions are dropped to
/// the bare origin/destination (start-position perft never reaches the far five
/// ranks, so no promotion suffix arises at the validated depths).
fn mce_uci(m: &mce::geometry::WideMove) -> String {
    use mce::geometry::Dai15x15;
    let f = m.from::<Dai15x15>();
    let t = m.to::<Dai15x15>();
    format!(
        "{}{}{}{}",
        (b'a' + f.file()) as char,
        f.rank() + 1,
        (b'a' + t.file()) as char,
        t.rank() + 1
    )
}

/// mce's legal moves for `pos` as sorted, deduped coordinate strings.
fn mce_moves(pos: &mce::geometry::Dai) -> Vec<String> {
    let mut v: Vec<String> = pos.legal_moves().iter().map(|m| mce_uci(&m)).collect();
    v.sort();
    v.dedup();
    v
}

/// Play the coordinate move `uci` on `pos`, returning the resulting position, or
/// `None` if it is not one of `pos`'s legal moves.
fn mce_play(pos: &mce::geometry::Dai, uci: &str) -> Option<mce::geometry::Dai> {
    pos.legal_moves()
        .iter()
        .find(|m| mce_uci(&m) == uci)
        .map(|m| pos.play(&m))
}

/// HaChu's legal-move list at the Dai position reached by replaying `seq` from the
/// start. A fresh subprocess per call (HaChu is fragile across long move
/// sequences); retried a few times to ride out its nondeterministic segfaults.
/// Returns `None` if every attempt failed to produce a move dump.
fn hachu_moves(bin: &str, seq: &[String]) -> Option<Vec<String>> {
    const TRIES: usize = 8;
    for _ in 0..TRIES {
        let attempt = (|| -> Result<Vec<String>, String> {
            let mut e = Engine::spawn(bin)?;
            e.start_variant("dai", 1000)?;
            for m in seq {
                e.usermove(m)?;
            }
            let moves = e.dump_legal_moves();
            e.quit();
            moves
        })();
        if let Ok(mv) = attempt {
            if !mv.is_empty() {
                return Some(mv);
            }
        }
    }
    None
}

/// Compare mce's Dai move generation against the HaChu oracle at the start
/// position: perft(1) node-for-node (the full move set) and perft(2) as a divide
/// (each root move's reply count). Returns the number of real mismatches (HaChu
/// per-node subprocess crashes are reported but not counted, as they are oracle
/// flakiness, not move differences).
fn compare_dai(bin: &str) -> usize {
    println!();
    println!("  Dai Shogi (15x15) external perft vs HaChu (issue #401):");
    let start = mce::geometry::Dai::startpos();
    let mce_start = mce_moves(&start);

    // perft(1): full move-set comparison.
    let Some(hachu_start) = hachu_moves(bin, &[]) else {
        println!("    SKIP: HaChu produced no move dump for the Dai start position.");
        return 0;
    };
    let mut mismatches = 0usize;
    let only_mce: Vec<&String> = mce_start.iter().filter(|m| !hachu_start.contains(m)).collect();
    let only_hachu: Vec<&String> = hachu_start.iter().filter(|m| !mce_start.contains(m)).collect();
    if only_mce.is_empty() && only_hachu.is_empty() {
        println!(
            "    perft(1): {} moves, node-for-node identical to HaChu.",
            mce_start.len()
        );
    } else {
        mismatches += only_mce.len() + only_hachu.len();
        println!(
            "    perft(1) MISMATCH: mce={} hachu={} (mce-only {:?}, hachu-only {:?})",
            mce_start.len(),
            hachu_start.len(),
            only_mce,
            only_hachu
        );
    }

    // perft(2): per-root divide (Black's reply count after each White root move).
    let mut roots_ok = 0usize;
    let mut root_mismatch = 0usize;
    let mut crashes = 0usize;
    for root in &mce_start {
        let child = mce_play(&start, root).expect("mce root move replays");
        let mce_replies = mce_moves(&child).len();
        match hachu_moves(bin, std::slice::from_ref(root)) {
            Some(hc) => {
                if hc.len() == mce_replies {
                    roots_ok += 1;
                } else {
                    root_mismatch += 1;
                    mismatches += 1;
                    println!(
                        "    perft(2) MISMATCH after {root}: mce={mce_replies} hachu={}",
                        hc.len()
                    );
                }
            }
            None => crashes += 1,
        }
    }
    println!(
        "    perft(2) divide: {roots_ok}/{} roots match, {root_mismatch} mismatch, {crashes} HaChu-crash node(s) skipped.",
        mce_start.len()
    );
    println!("    total real mismatches: {mismatches}");
    mismatches
}

#[cfg(test)]
mod tests {
    use super::REQUIRED_VARIANTS;

    #[test]
    fn mce_dai_start_has_seventy_one_moves() {
        // The mce side of the oracle comparison, independent of HaChu: the Dai
        // start position has the HaChu-validated 71 legal moves.
        let start = mce::geometry::Dai::startpos();
        assert_eq!(super::mce_moves(&start).len(), 71);
    }

    #[test]
    fn required_variants_cover_the_large_shogi_targets() {
        // Chu is the immediate #380 target; Dai and Tenjiku are the follow-on
        // large-shogi boards HaChu also serves. Guard against accidental edits.
        assert!(REQUIRED_VARIANTS.contains(&"chu"));
        assert!(REQUIRED_VARIANTS.contains(&"dai"));
        assert!(REQUIRED_VARIANTS.contains(&"tenjiku"));
    }
}
