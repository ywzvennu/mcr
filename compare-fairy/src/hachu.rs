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
//! ## The node-by-node perft comparison is gated on issue #380
//!
//! HaChu has no native `perft` command, so a node-by-node perft/divide is driven
//! *externally*: the harness walks the move tree itself, using HaChu as a
//! per-move legality/move-generation oracle (via `usermove`). That walk needs
//! mce's own large-shogi move generator to compare against — and mce does not yet
//! implement a Chu-Shogi *variant* (only the 12x12 `Chu12x12` board geometry
//! exists). Adding Chu-Shogi rules is issue #380. Until then the mce side of the
//! comparison is gated behind the `large-shogi` cargo feature and this mode
//! reports the oracle as READY rather than running an unbacked comparison.
//!
//! GPL FENCE unchanged: HaChu is spawned as a subprocess (see `xboard.rs`); no
//! HaChu code is linked or vendored, and the built binary is git-ignored.

use crate::locate_hachu::{self, Source};
use crate::xboard::Engine;

/// The large-shogi variants this oracle must advertise to be useful for the
/// Chu-Shogi work (issue #380). Chu is the immediate target; Dai and Tenjiku are
/// the follow-on large-shogi boards HaChu also serves.
const REQUIRED_VARIANTS: &[&str] = &["chu", "dai", "tenjiku"];

/// Run the HaChu-oracle mode. Returns the number of parity mismatches (always 0
/// today: this mode never fails the harness — it either reports the oracle READY
/// or skips cleanly when HaChu is absent, exactly like the FSF-absent skip).
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

    // ---- drive one concrete large-shogi position -------------------------
    // `variant chu` establishes the Chu-Shogi start position internally, then
    // `force` stops the engine auto-replying. A ping/pong round-trip confirms the
    // oracle is positioned and responsive on a real 12x12 large-shogi board.
    if let Err(e) = engine.select_variant("chu") {
        println!("  SKIP: HaChu did not accept `variant chu`: {e}");
        engine.quit();
        return 0;
    }
    println!("  driven to Chu-Shogi start position (variant chu); oracle responsive.");

    // ---- node-by-node perft comparison: gated on issue #380 --------------
    report_perft_status();

    engine.quit();
    0
}

/// Print the perft/divide comparison status. The mce side is gated behind the
/// `large-shogi` cargo feature, which issue #380 will enable once mce implements
/// a Chu-Shogi move generator.
fn report_perft_status() {
    #[cfg(feature = "large-shogi")]
    {
        // Issue #380: with mce's Chu-Shogi move generator available, walk the
        // move tree here and use HaChu (`usermove` legality) as the per-move
        // oracle to cross-check mce's perft/divide node counts. The oracle
        // plumbing (locate/build + CECP driver + variant confirmation) above is
        // ready; this block is the integration point.
        println!(
            "  perft: `large-shogi` feature enabled — wire mce Chu-Shogi perft here (issue #380)."
        );
    }
    #[cfg(not(feature = "large-shogi"))]
    {
        println!("  perft: node-by-node comparison PENDING issue #380.");
        println!(
            "         mce has the 12x12 Chu board geometry but no Chu-Shogi variant/move\n\
             \x20        generator yet, so there is nothing to cross-check against the oracle.\n\
             \x20        Enable it once #380 lands: build with `--features large-shogi`."
        );
    }
}

#[cfg(test)]
mod tests {
    use super::REQUIRED_VARIANTS;

    #[test]
    fn required_variants_cover_the_large_shogi_targets() {
        // Chu is the immediate #380 target; Dai and Tenjiku are the follow-on
        // large-shogi boards HaChu also serves. Guard against accidental edits.
        assert!(REQUIRED_VARIANTS.contains(&"chu"));
        assert!(REQUIRED_VARIANTS.contains(&"dai"));
        assert!(REQUIRED_VARIANTS.contains(&"tenjiku"));
    }
}
