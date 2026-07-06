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
//! mcr's Dai Shogi variant now implemented (`mcr::geometry::Dai`), this mode walks
//! the start-position tree and cross-checks mcr against the oracle:
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
/// #401). Returns the number of real parity mismatches (0 when mcr matches HaChu at
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
        Source::Env => "env $MCR_HACHU_BIN".to_string(),
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
    let dai_mismatches = compare_dai(&located.bin);

    // ---- Tenjiku Shogi (16x16) external perft comparison (issue #402) -----
    let mid = dai_mismatches + compare_tenjiku(&located.bin);

    // ---- Wa Shogi (11x11): probe HaChu's `wa-shogi` (issue #500) ----------
    // HaChu advertises `wa-shogi`, so — unlike Tenjiku — it does not crash; but it
    // implements a *different* Wa ruleset (a different start array and piece set),
    // so it is not a usable node-for-node oracle for mcr's Wa Shogi. This records
    // that finding in-repo rather than leaving "HaChu unreliable on Wa" a bare claim.
    mid + probe_washogi(&located.bin)
}

/// The coordinate string for an mcr Wa Shogi move on the 11x11 board (files `a..k`,
/// ranks `1..11`), matching HaChu's move-dump notation.
fn mcr_uci_washogi(m: &mcr::geometry::WideMove) -> String {
    use mcr::geometry::Washogi11x11;
    let f = m.from::<Washogi11x11>();
    let t = m.to::<Washogi11x11>();
    format!(
        "{}{}{}{}",
        (b'a' + f.file()) as char,
        f.rank() + 1,
        (b'a' + t.file()) as char,
        t.rank() + 1
    )
}

/// Probe HaChu's `wa-shogi` at the start position (issue #500). HaChu runs Wa Shogi
/// (no segfault, unlike Tenjiku), but its start move set does **not** match mcr's,
/// because HaChu ships a *different* Wa Shogi ruleset (different start array / piece
/// definitions). Reports the disagreement and returns **0** (this is a documented
/// oracle-mismatch, not an mcr bug): mcr's Wa Shogi is validated by the fully
/// independent in-repo brute-force generator in `tests/perft_washogi.rs`, not HaChu.
fn probe_washogi(bin: &str) -> usize {
    println!();
    println!("  Wa Shogi (11x11) HaChu probe (issue #500):");
    let start = mcr::geometry::Washogi::startpos();
    let mut mcr_start: Vec<String> = start.legal_moves().iter().map(mcr_uci_washogi).collect();
    mcr_start.sort();
    mcr_start.dedup();

    let hachu_start = {
        const TRIES: usize = 8;
        let mut out = None;
        for _ in 0..TRIES {
            let attempt = (|| -> Result<Vec<String>, String> {
                let mut e = Engine::spawn(bin)?;
                e.start_variant("wa-shogi", 1000)?;
                let moves = e.dump_legal_moves();
                e.quit();
                moves
            })();
            if let Ok(mv) = attempt {
                if !mv.is_empty() {
                    out = Some(mv);
                    break;
                }
            }
        }
        out
    };
    let Some(hachu_start) = hachu_start else {
        println!("    SKIP: HaChu produced no move dump for the Wa Shogi start position.");
        return 0;
    };
    if hachu_start == mcr_start {
        println!(
            "    perft(1): {} moves, node-for-node identical to HaChu.",
            mcr_start.len()
        );
    } else {
        let only_mcr = mcr_start
            .iter()
            .filter(|m| !hachu_start.contains(m))
            .count();
        let only_hachu = hachu_start
            .iter()
            .filter(|m| !mcr_start.contains(m))
            .count();
        println!(
            "    perft(1): mcr={} vs HaChu={} — DIFFERENT rulesets (mcr-only {only_mcr}, \
             hachu-only {only_hachu}). HaChu's `wa-shogi` is a different Wa variant and is \
             NOT a usable oracle for mcr's Wa Shogi; validated by the independent brute \
             force in tests/perft_washogi.rs instead.",
            mcr_start.len(),
            hachu_start.len()
        );
    }
    0
}

/// The coordinate string for an mcr Dai move on the 15x15 board (files `a..o`,
/// ranks `1..15`), matching HaChu's move-dump notation. Promotions are dropped to
/// the bare origin/destination (start-position perft never reaches the far five
/// ranks, so no promotion suffix arises at the validated depths).
fn mcr_uci(m: &mcr::geometry::WideMove) -> String {
    use mcr::geometry::Dai15x15;
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

/// mcr's legal **board** moves for `pos` as sorted, deduped coordinate strings.
///
/// The Chu/Dai Lion's jitto **pass** (a `from == to` null move — mcr's `h3h3`
/// notation) is excluded, because HaChu renders its single tracked null / lion-pass
/// move as the `p32p32` / `@@@@` tokens which [`Engine::dump_legal_moves`] filters
/// out. So both sides are compared **board-move to board-move**: at every position a
/// Lion can jitto-pass, mcr would otherwise report exactly one extra move (the pass)
/// against HaChu's filtered null — a pure notation artifact, not a move difference
/// (confirmed by the depth-3 walk: every such node's other moves match node-for-node;
/// issue #500).
fn mcr_moves(pos: &mcr::geometry::Dai) -> Vec<String> {
    use mcr::geometry::Dai15x15;
    let mut v: Vec<String> = pos
        .legal_moves()
        .iter()
        .filter(|m| m.from::<Dai15x15>() != m.to::<Dai15x15>())
        .map(mcr_uci)
        .collect();
    v.sort();
    v.dedup();
    v
}

/// Play the coordinate move `uci` on `pos`, returning the resulting position, or
/// `None` if it is not one of `pos`'s legal moves.
fn mcr_play(pos: &mcr::geometry::Dai, uci: &str) -> Option<mcr::geometry::Dai> {
    pos.legal_moves()
        .iter()
        .find(|m| mcr_uci(m) == uci)
        .map(|m| pos.play(m))
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

/// Compare mcr's Dai move generation against the HaChu oracle at the start
/// position: perft(1) node-for-node (the full move set) and perft(2) as a divide
/// (each root move's reply count). Returns the number of real mismatches (HaChu
/// per-node subprocess crashes are reported but not counted, as they are oracle
/// flakiness, not move differences).
fn compare_dai(bin: &str) -> usize {
    println!();
    println!("  Dai Shogi (15x15) external perft vs HaChu (issue #401):");
    let start = mcr::geometry::Dai::startpos();
    let mcr_start = mcr_moves(&start);

    // perft(1): full move-set comparison.
    let Some(hachu_start) = hachu_moves(bin, &[]) else {
        println!("    SKIP: HaChu produced no move dump for the Dai start position.");
        return 0;
    };
    let mut mismatches = 0usize;
    let only_mcr: Vec<&String> = mcr_start
        .iter()
        .filter(|m| !hachu_start.contains(m))
        .collect();
    let only_hachu: Vec<&String> = hachu_start
        .iter()
        .filter(|m| !mcr_start.contains(m))
        .collect();
    if only_mcr.is_empty() && only_hachu.is_empty() {
        println!(
            "    perft(1): {} moves, node-for-node identical to HaChu.",
            mcr_start.len()
        );
    } else {
        mismatches += only_mcr.len() + only_hachu.len();
        println!(
            "    perft(1) MISMATCH: mcr={} hachu={} (mcr-only {:?}, hachu-only {:?})",
            mcr_start.len(),
            hachu_start.len(),
            only_mcr,
            only_hachu
        );
    }

    // perft(2): per-root divide (Black's reply count after each White root move).
    let mut roots_ok = 0usize;
    let mut root_mismatch = 0usize;
    let mut crashes = 0usize;
    for root in &mcr_start {
        let child = mcr_play(&start, root).expect("mcr root move replays");
        let mcr_replies = mcr_moves(&child).len();
        match hachu_moves(bin, std::slice::from_ref(root)) {
            Some(hc) => {
                if hc.len() == mcr_replies {
                    roots_ok += 1;
                } else {
                    root_mismatch += 1;
                    mismatches += 1;
                    println!(
                        "    perft(2) MISMATCH after {root}: mcr={mcr_replies} hachu={}",
                        hc.len()
                    );
                }
            }
            None => crashes += 1,
        }
    }
    println!(
        "    perft(2) divide: {roots_ok}/{} roots match, {root_mismatch} mismatch, {crashes} HaChu-crash node(s) skipped.",
        mcr_start.len()
    );
    println!("    total real mismatches: {mismatches}");

    // perft(3) node-for-node divide (issue #500): opt-in via $MCR_HACHU_DAI_DEPTH3
    // because it spawns one HaChu subprocess per depth-2 node (~5041 nodes) and is
    // the flaky-but-thorough push from the depth-2 cross-oracle to depth 3.
    if std::env::var_os("MCR_HACHU_DAI_DEPTH3").is_some() {
        mismatches += compare_dai_depth3(bin, &start, &mcr_start);
    }
    mismatches
}

/// **Dai perft(3) node-for-node cross-check (issue #500).** For every depth-2 node
/// (each White-root → Black-reply sequence) compare mcr's grandchild reply count to
/// HaChu's, turning the depth-3 pin (357836) from an mcr-only regression into a real
/// HaChu cross-oracle at every node HaChu does not segfault on. Returns the number
/// of real mismatches (HaChu per-node crashes are reported, not counted). Bounded by
/// an optional `$MCR_HACHU_DAI_MAX_NODES` cap for a time-boxed partial run.
fn compare_dai_depth3(bin: &str, start: &mcr::geometry::Dai, roots: &[String]) -> usize {
    println!();
    println!("  Dai Shogi perft(3) node-for-node vs HaChu (issue #500):");
    let cap: usize = std::env::var("MCR_HACHU_DAI_MAX_NODES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);
    let mut nodes_ok = 0usize;
    let mut mismatches = 0usize;
    let mut crashes = 0usize;
    let mut checked = 0usize;
    let mut mcr_sum: u64 = 0;
    'outer: for root in roots {
        let child = mcr_play(start, root).expect("mcr root move replays");
        for m2 in &mcr_moves(&child) {
            if checked >= cap {
                break 'outer;
            }
            checked += 1;
            let gc = mcr_play(&child, m2).expect("mcr reply move replays");
            let mcr_gc = mcr_moves(&gc).len();
            mcr_sum += mcr_gc as u64;
            let seq = [root.clone(), m2.clone()];
            match hachu_moves(bin, &seq) {
                Some(hc) => {
                    if hc.len() == mcr_gc {
                        nodes_ok += 1;
                    } else {
                        mismatches += 1;
                        println!(
                            "    perft(3) MISMATCH after {root} {m2}: mcr={mcr_gc} hachu={}",
                            hc.len()
                        );
                    }
                }
                None => crashes += 1,
            }
        }
    }
    println!(
        "    perft(3) divide: {checked} depth-2 nodes checked, {nodes_ok} match, \
         {mismatches} mismatch, {crashes} HaChu-crash node(s) skipped."
    );
    println!("    mcr grandchild-sum over checked nodes: {mcr_sum} (full perft(3) = 357836).");
    println!("    total real perft(3) mismatches: {mismatches}");
    mismatches
}

/// The coordinate string for an mcr Tenjiku move on the 16x16 board (files `a..p`,
/// ranks `1..16`), matching HaChu's move-dump notation. Promotions are dropped to
/// the bare origin/destination (start-position perft never reaches the promotion
/// zone, so no promotion suffix arises at the validated depths).
fn mcr_uci_tenjiku(m: &mcr::geometry::WideMove) -> String {
    use mcr::geometry::Tenjiku16x16;
    let f = m.from::<Tenjiku16x16>();
    let t = m.to::<Tenjiku16x16>();
    format!(
        "{}{}{}{}",
        (b'a' + f.file()) as char,
        f.rank() + 1,
        (b'a' + t.file()) as char,
        t.rank() + 1
    )
}

/// mcr's legal moves for `pos` as sorted, deduped coordinate strings.
fn mcr_moves_tenjiku(pos: &mcr::geometry::Tenjiku) -> Vec<String> {
    let mut v: Vec<String> = pos.legal_moves().iter().map(mcr_uci_tenjiku).collect();
    v.sort();
    v.dedup();
    v
}

/// Play the coordinate move `uci` on `pos`, returning the resulting position, or
/// `None` if it is not one of `pos`'s legal moves.
fn mcr_play_tenjiku(pos: &mcr::geometry::Tenjiku, uci: &str) -> Option<mcr::geometry::Tenjiku> {
    pos.legal_moves()
        .iter()
        .find(|m| mcr_uci_tenjiku(m) == uci)
        .map(|m| pos.play(m))
}

/// HaChu's legal-move list at the Tenjiku position reached by replaying `seq` from
/// the start. A fresh subprocess per call (HaChu is fragile across long move
/// sequences); retried a few times to ride out its nondeterministic segfaults on
/// the 16x16 board. Returns `None` if every attempt failed to produce a move dump.
fn hachu_moves_tenjiku(bin: &str, seq: &[String]) -> Option<Vec<String>> {
    const TRIES: usize = 16;
    for _ in 0..TRIES {
        let attempt = (|| -> Result<Vec<String>, String> {
            let mut e = Engine::spawn(bin)?;
            e.start_variant("tenjiku", 1000)?;
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

/// Compare mcr's Tenjiku move generation against the HaChu oracle at the start
/// position: perft(1) node-for-node (the full move set) and perft(2) as a divide
/// (each root move's reply count). Returns the number of real mismatches (HaChu
/// per-node subprocess crashes are reported but not counted, as they are oracle
/// flakiness, not move differences). At the start the two armies are separated by
/// empty ranks, so every move here is a non-capture — the depth range over which
/// mcr's ordinary movement (and the documented-unmodelled jump-capture / burn
/// powers, which fire only on captures) is provably exercised node-for-node.
fn compare_tenjiku(bin: &str) -> usize {
    println!();
    println!("  Tenjiku Shogi (16x16) external perft vs HaChu (issue #402):");
    let start = mcr::geometry::Tenjiku::startpos();
    let mcr_start = mcr_moves_tenjiku(&start);

    // perft(1): full move-set comparison.
    let Some(hachu_start) = hachu_moves_tenjiku(bin, &[]) else {
        // HaChu 0.23 (the ddugovic build) **segfaults deterministically** on
        // `variant tenjiku`: its 16x16 play area fills the entire BW*BH board array,
        // leaving no EDGE-sentinel border for its 0x88-style neighbour scans (its own
        // burn code comments "assumes 32x16 board"), and a padded rebuild crashes
        // just the same. So HaChu cannot serve as a live Tenjiku oracle here — a
        // genuine HaChu limitation, not an mcr difference. mcr's Tenjiku start
        // position and per-piece movement are instead validated directly against
        // HaChu's own source tables (`variant.c` tenjikuPieces / tenArray and the
        // `GenNonCapts` non-capture semantics); see `tests/perft_tenjiku.rs`.
        println!(
            "    SKIP: HaChu crashes on `variant tenjiku` (deterministic segfault; \
             HaChu limitation). mcr generates {} start-position moves, validated \
             against HaChu's source tables in tests/perft_tenjiku.rs.",
            mcr_start.len()
        );
        return 0;
    };
    let mut mismatches = 0usize;
    let only_mcr: Vec<&String> = mcr_start
        .iter()
        .filter(|m| !hachu_start.contains(m))
        .collect();
    let only_hachu: Vec<&String> = hachu_start
        .iter()
        .filter(|m| !mcr_start.contains(m))
        .collect();
    if only_mcr.is_empty() && only_hachu.is_empty() {
        println!(
            "    perft(1): {} moves, node-for-node identical to HaChu.",
            mcr_start.len()
        );
    } else {
        mismatches += only_mcr.len() + only_hachu.len();
        println!(
            "    perft(1) MISMATCH: mcr={} hachu={} (mcr-only {:?}, hachu-only {:?})",
            mcr_start.len(),
            hachu_start.len(),
            only_mcr,
            only_hachu
        );
    }

    // perft(2): per-root divide (Black's reply count after each White root move).
    let mut roots_ok = 0usize;
    let mut root_mismatch = 0usize;
    let mut crashes = 0usize;
    for root in &mcr_start {
        let child = mcr_play_tenjiku(&start, root).expect("mcr root move replays");
        let mcr_replies = mcr_moves_tenjiku(&child).len();
        match hachu_moves_tenjiku(bin, std::slice::from_ref(root)) {
            Some(hc) => {
                if hc.len() == mcr_replies {
                    roots_ok += 1;
                } else {
                    root_mismatch += 1;
                    mismatches += 1;
                    println!(
                        "    perft(2) MISMATCH after {root}: mcr={mcr_replies} hachu={}",
                        hc.len()
                    );
                }
            }
            None => crashes += 1,
        }
    }
    println!(
        "    perft(2) divide: {roots_ok}/{} roots match, {root_mismatch} mismatch, {crashes} HaChu-crash node(s) skipped.",
        mcr_start.len()
    );
    println!("    total real mismatches: {mismatches}");
    mismatches
}

#[cfg(test)]
mod tests {
    use super::REQUIRED_VARIANTS;

    #[test]
    fn mcr_dai_start_has_seventy_one_moves() {
        // The mcr side of the oracle comparison, independent of HaChu: the Dai
        // start position has the HaChu-validated 71 legal moves.
        let start = mcr::geometry::Dai::startpos();
        assert_eq!(super::mcr_moves(&start).len(), 71);
    }

    #[test]
    fn mcr_tenjiku_start_has_seventy_two_moves() {
        // The mcr side of the Tenjiku comparison, independent of HaChu (which
        // crashes on `variant tenjiku`): the start position has 72 legal moves,
        // reconciled move-for-move against HaChu's source tables (see
        // `tests/perft_tenjiku.rs`).
        let start = mcr::geometry::Tenjiku::startpos();
        assert_eq!(super::mcr_moves_tenjiku(&start).len(), 72);
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
