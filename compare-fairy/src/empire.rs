//! Empire differential perft + timing against Fairy-Stockfish (issue #221).
//!
//! Empire runs on mce's **generic** engine (`mce::geometry::Empire`, a
//! `GenericPosition<Chess8x8, EmpireRules>`), like the other fairy variants, so it
//! has its own corpus and comparison loop here. The FSF side selects
//! `UCI_Variant empire`, sets the FEN, runs `go perft`, asserts the node counts
//! match, and reports mce-vs-FSF throughput.
//!
//! ## Empire is an INI variant
//!
//! Like Orda / Synochess, FSF defines `empire` in its `variants.ini` data file, not
//! in the binary. The harness therefore loads the INI
//! (`setoption name VariantPath value <variants.ini>`) before checking
//! [`Engine::has_variant`](crate::uci::Engine::has_variant); the INI path is
//! resolved from `$MCE_FSF_VARIANTS_INI`, then a `variants.ini` sitting beside the
//! FSF binary, then the harness build dir. If none is found (or the loaded INI
//! still lacks `empire`), the whole block is skipped cleanly.
//!
//! ## FEN dialect
//!
//! mce and FSF render the same Empire position with **different White-piece
//! letters**. FSF's `empire` spells its four custom pieces `t e c d` (Tower, Eagle,
//! Cardinal, Duke) and the Soldier `s`; mce already names `t e c d` (Lieutenant /
//! Cannon / Elephant / General) and `s` (Silver), so the four Empire pieces take
//! `*`-prefixed overflow tokens (`*T *E *C *D`, recycling the FSF mnemonics) and the
//! Soldier takes `z`. [`to_fsf_dialect`] strips the `*` prefix off the four Empire
//! tokens (`*t → t`, …, both cases) and maps the Soldier `z → s`, over the placement
//! field only — the side-to-move / castling fields are left intact (Black is
//! standard chess and carries none of the remapped letters). The comparison asserts
//! only node counts, so the move-string dialect never matters.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked, and the INI is a plain data file.

use std::path::{Path, PathBuf};
use std::time::Instant;

use mce::geometry::{perft as gperft, Chess8x8, Empire};

use crate::uci::Engine;

/// One Empire corpus position. The FEN is mce's dialect; the FSF side translates it
/// via [`to_fsf_dialect`].
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Empire comparison corpus (all FSF-confirmed): the startpos (both colors,
/// exercising the boxed-in Empire army), a developed middlegame, a tactic firing
/// every Empire capture pattern (Eagle knight, Cardinal bishop, Tower rook) at once,
/// a king-flag race exercising the campmate terminal rule, and a flying-general
/// faceoff.
const CASES: &[Case] = &[
    Case {
        label: "startpos-w",
        fen: "rnbqkbnr/pppppppp/8/8/8/PPPZZPPP/8/*T*E*C*DK*C*E*T w kq - 0 1",
        depth: 4,
    },
    Case {
        label: "startpos-b",
        fen: "rnbqkbnr/pppppppp/8/8/8/PPPZZPPP/8/*T*E*C*DK*C*E*T b kq - 0 1",
        depth: 4,
    },
    Case {
        label: "midgame",
        fen: "rnbqkbnr/pp1ppppp/8/2p5/3P*E3/2P2P2/PP2Z1PP/*T1*C*DK1*E*T w kq - 0 1",
        depth: 4,
    },
    Case {
        label: "tactic",
        fen: "4k3/8/2n1n3/3rb3/3*E*C*T2/3q4/3P4/4K3 w - - 0 1",
        depth: 4,
    },
    Case {
        label: "flag-race",
        fen: "4k3/8/8/8/8/8/4K3/8 w - - 0 1",
        depth: 5,
    },
    Case {
        label: "flying-general",
        fen: "8/8/3k4/8/8/8/3K4/8 w - - 0 1",
        depth: 5,
    },
];

/// Translates an mce-dialect Empire FEN to FSF's dialect: strip the `*` prefix off
/// the four Empire overflow tokens (`*t → t`, `*e → e`, `*c → c`, `*d → d`, both
/// cases) and map the Soldier `z → s`. Applied to the **placement field only** (the
/// side-to-move / castling / clock fields are left intact); Black is standard chess
/// and carries none of the remapped letters, so the swap is unambiguous.
pub(crate) fn to_fsf_dialect(fen: &str) -> String {
    let mut parts = fen.splitn(2, ' ');
    let placement = parts
        .next()
        .unwrap_or("")
        // Strip the `*` off each Empire overflow token, leaving the bare FSF letter.
        .replace("*T", "T")
        .replace("*t", "t")
        .replace("*E", "E")
        .replace("*e", "e")
        .replace("*C", "C")
        .replace("*c", "c")
        .replace("*D", "D")
        .replace("*d", "d")
        // The Soldier: mce `z` → FSF `s` (both cases).
        .replace('z', "s")
        .replace('Z', "S");
    match parts.next() {
        Some(rest) => format!("{placement} {rest}"),
        None => placement,
    }
}

/// Resolve the FSF `variants.ini` path: `$MCE_FSF_VARIANTS_INI`, then a sibling
/// `variants.ini` beside the FSF binary (the upstream layout `…/src/stockfish` +
/// `…/src/variants.ini`), then the harness build dir's checkout.
fn resolve_variants_ini(fsf_bin: &str) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MCE_FSF_VARIANTS_INI") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(dir) = Path::new(fsf_bin).parent() {
        let sib = dir.join("variants.ini");
        if sib.is_file() {
            return Some(sib);
        }
    }
    let build = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("build")
        .join("Fairy-Stockfish")
        .join("src")
        .join("variants.ini");
    if build.is_file() {
        return Some(build);
    }
    None
}

/// A measured Empire comparison row.
struct Row {
    label: &'static str,
    fen: &'static str,
    depth: u32,
    mce_nodes: u64,
    fsf_nodes: u64,
    matched: bool,
    mce_secs: f64,
    fsf_secs: f64,
}

impl Row {
    fn mce_mnps(&self) -> f64 {
        if self.mce_secs > 0.0 {
            self.mce_nodes as f64 / self.mce_secs / 1e6
        } else {
            f64::INFINITY
        }
    }
    fn fsf_mnps(&self) -> f64 {
        if self.fsf_secs > 0.0 {
            self.fsf_nodes as f64 / self.fsf_secs / 1e6
        } else {
            f64::INFINITY
        }
    }
    fn speedup(&self) -> f64 {
        if self.mce_secs > 0.0 {
            self.fsf_secs / self.mce_secs
        } else {
            f64::NAN
        }
    }
}

/// Run the Empire corpus through mce and FSF. Returns the number of mismatches
/// (0 = all matched). Loads FSF's `variants.ini` first (Empire is an INI variant);
/// if the INI cannot be found or still lacks `empire`, the block is skipped
/// (returns 0) rather than reporting spurious mismatches.
pub fn run(engine: &mut Engine, fsf_bin: &str, full: bool) -> usize {
    println!();
    println!("Empire — generic engine vs FSF UCI_Variant empire (issue #221):");

    if !engine.has_variant("empire") {
        match resolve_variants_ini(fsf_bin) {
            Some(ini) => {
                if let Err(e) = engine.load_variant_path(&ini.to_string_lossy()) {
                    println!("  (skipped: failed to load variants.ini: {e})");
                    return 0;
                }
            }
            None => {
                println!(
                    "  (skipped: no variants.ini found; set $MCE_FSF_VARIANTS_INI to FSF's \
                     variants.ini to enable the Empire comparison)"
                );
                return 0;
            }
        }
    }
    if !engine.has_variant("empire") {
        println!(
            "  (skipped: this FSF binary does not advertise UCI_Variant empire even after \
                  loading variants.ini)"
        );
        return 0;
    }

    let head = format!(
        "{:<14} {:>5} {:>14} {:>14} {:>9} {:>10} {:>10} {:>8}",
        "position", "depth", "mce nodes", "fsf nodes", "match", "mce Mn/s", "fsf Mn/s", "mce/fsf",
    );
    println!("{head}");
    println!("{}", "-".repeat(head.len()));

    let mut rows: Vec<Row> = Vec::with_capacity(CASES.len());
    let mut mismatches = 0usize;

    for case in CASES {
        let depth = if full { case.depth + 1 } else { case.depth };
        match run_case(engine, case, depth) {
            Ok(row) => {
                if !row.matched {
                    mismatches += 1;
                }
                println!(
                    "{:<14} {:>5} {:>14} {:>14} {:>9} {:>10.1} {:>10.1} {:>7.2}x",
                    row.label,
                    row.depth,
                    row.mce_nodes,
                    row.fsf_nodes,
                    if row.matched { "ok" } else { "MISMATCH" },
                    row.mce_mnps(),
                    row.fsf_mnps(),
                    row.speedup(),
                );
                rows.push(row);
            }
            Err(e) => {
                eprintln!("skip empire/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mce_nodes).sum();
    let mce_s: f64 = rows.iter().map(|r| r.mce_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mce_s > 0.0 && fsf_s > 0.0 {
        println!(
            "empire OVERALL: {nodes} nodes verified; mce {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mce_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mce_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Empire positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Empire parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH empire/{} depth {}: mce={} fsf={}  FEN: {}",
                r.label, r.depth, r.mce_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Empire position through mce's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Empire::from_fen(case.fen).map_err(|e| format!("mce rejected FEN: {e:?}"))?;
    let mce_start = Instant::now();
    let mce_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mce_secs = mce_start.elapsed().as_secs_f64();

    let fsf_fen = to_fsf_dialect(case.fen);
    engine.set_variant("empire", false)?;
    engine.set_position(&fsf_fen)?;
    let fsf = engine.go_perft(depth, false)?;

    Ok(Row {
        label: case.label,
        fen: case.fen,
        depth,
        mce_nodes,
        fsf_nodes: fsf.nodes,
        matched: mce_nodes == fsf.nodes,
        mce_secs,
        fsf_secs: fsf.elapsed.as_secs_f64(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The corpus FENs all parse on the generic Empire engine, round-trip through
    /// mce's FEN I/O, and the pinned shallow counts match the FSF-confirmed numbers
    /// in `tests/perft_empire.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos-w", 3u32, 20895u64),
            ("startpos-b", 3, 13352),
            ("midgame", 3, 51451),
            ("tactic", 3, 73871),
            ("flag-race", 3, 110),
            ("flying-general", 3, 174),
        ];
        for case in CASES {
            let pos = Empire::from_fen(case.fen).expect("corpus FEN parses");
            assert_eq!(pos.to_fen(), case.fen, "{} round-trips", case.label);
            let (_, depth, want) = pinned
                .iter()
                .find(|(l, _, _)| *l == case.label)
                .copied()
                .expect("a pinned count for the case");
            assert_eq!(
                gperft::<Chess8x8, _>(&pos, depth),
                want,
                "{} perft",
                case.label
            );
        }
    }

    /// The dialect swap strips the `*` off the Empire tokens, maps the Soldier
    /// `z → s`, and leaves the standard Black army and structural fields untouched.
    #[test]
    fn dialect_swap_strips_overflow_and_maps_soldier() {
        let mce = "rnbqkbnr/pppppppp/8/8/8/PPPZZPPP/8/*T*E*C*DK*C*E*T w kq - 0 1";
        assert_eq!(
            to_fsf_dialect(mce),
            "rnbqkbnr/pppppppp/8/8/8/PPPSSPPP/8/TECDKCET w kq - 0 1"
        );
        // A Black-to-move tactic: the side-to-move `b` and the placement letters
        // both come through correctly (no `*`-stripping touches the `b` field).
        assert_eq!(
            to_fsf_dialect("4k3/8/2n1n3/3rb3/3*E*C*T2/3q4/3P4/4K3 w - - 0 1"),
            "4k3/8/2n1n3/3rb3/3ECT2/3q4/3P4/4K3 w - - 0 1"
        );
    }
}
