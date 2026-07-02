//! Fairy-Stockfish perft comparison harness (issue #158).
//!
//! Differential perft + head-to-head timing of mce against Fairy-Stockfish (FSF)
//! on the variants both engines share — atomic, king-of-the-hill, three-check,
//! antichess (FSF: giveaway), racing-kings, horde, crazyhouse, chess960, and
//! standard. For each shared position the two engines run perft to the same
//! depth; the node counts are asserted equal, and the throughput (mce Mn/s vs
//! FSF Mn/s) is reported. A mismatch prints the FEN + depth to reproduce.
//!
//! GPL FENCE: FSF is GPL-3.0+. This harness NEVER links FSF; it drives an
//! externally provided `fairy-stockfish` UCI binary purely as a subprocess (see
//! `uci.rs`). The mce library does not depend on FSF. This crate is
//! `publish = false` and the FSF binary is never committed.
//!
//! ```text
//! cargo run --release                # locate FSF (env / PATH / prebuilt), compare
//! cargo run --release -- --build     # also clone + build FSF if not found
//! cargo run --release -- --full      # one ply deeper
//! cargo run --release --features magic   # mce magic-bitboard sliders
//! ```
//!
//! If no FSF binary can be obtained, the harness SKIPS gracefully with install
//! instructions and exits 0 (it never blocks or fails hard on FSF absence).

mod asean;
mod ataxx;
mod bughouse;
mod cambodian;
mod cannonshogi;
mod capablanca;
mod capahouse;
mod chak;
mod chennis;
mod corpus;
mod courier;
mod difffuzz;
mod dobutsu;
mod dragon;
mod duck;
mod empire;
mod fogofwar;
mod gorogoro;
mod grand;
mod grandhouse;
mod hachu;
mod hoppelpoppel;
mod janggi;
mod jieqi;
mod khans;
mod knightmate;
mod kyotoshogi;
mod locate;
mod locate_hachu;
mod makpong;
mod makruk;
mod manchu;
mod mansindam;
mod minishogi;
mod minixiangqi;
mod opulent;
mod orda;
mod ordamirror;
mod placement;
mod seirawan;
mod shako;
mod shatar;
mod shatranj;
mod shinobi;
mod shogi;
mod shogun;
mod shoshogi;
mod shouse;
mod sittuyin;
mod spartan;
mod synochess;
mod tencubed;
mod tori;
mod uci;
mod variants;
mod xboard;
mod xiangfu;
mod xiangqi;

use std::time::Instant;

use mce::{AnyVariant, VariantId};

use corpus::{Case, CASES, VARIANTS};
use locate::Source;

/// Parsed command line.
struct Opts {
    /// Allow cloning + building FSF if no binary is found.
    build: bool,
    /// One ply deeper per position.
    full: bool,
    /// Run the differential fuzzer (issue #239) instead of the pinned corpus.
    difffuzz: bool,
    /// Differential-fuzzer tunables (only meaningful when `difffuzz` is set).
    fuzz: difffuzz::Config,
    /// Run the HaChu large-shogi differential-oracle mode (issue #379) instead of
    /// the FSF comparison.
    hachu: bool,
    /// Allow cloning + building HaChu if no binary is found (HaChu mode only).
    build_hachu: bool,
}

/// A single measured comparison row.
struct Row {
    id: VariantId,
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

fn main() {
    let opts = parse_args();

    // ---- HaChu large-shogi differential-oracle mode (issue #379) ----------
    // Independent of FSF: HaChu covers the large-shogi variants (Chu / Dai /
    // Tenjiku) that FSF does not, so this mode locates/drives HaChu on its own
    // and never touches the FSF path below.
    if opts.hachu {
        println!("mce vs HaChu — large-shogi differential oracle (issue #379)");
        let mismatches = hachu::run(opts.build_hachu);
        if mismatches > 0 {
            std::process::exit(1);
        }
        return;
    }

    println!("mce vs Fairy-Stockfish — perft comparison harness (issue #158)");
    #[cfg(feature = "magic")]
    println!("mce slider backend: magic bitboards (--features magic)");
    #[cfg(not(feature = "magic"))]
    println!("mce slider backend: hyperbola-quintessence (default)");

    // ---- locate (or build) the FSF binary ---------------------------------
    let located = match locate::locate(opts.build) {
        Ok(l) => l,
        Err(reason) => {
            println!();
            println!("SKIP: {reason}");
            println!();
            println!("{}", locate::INSTALL_HELP);
            // Skipping on FSF absence is a clean, expected outcome — exit 0.
            return;
        }
    };
    let src = match &located.source {
        Source::Env(v) => format!("env ${v}"),
        Source::Path(n) => format!("PATH ({n})"),
        Source::Prebuilt(p) => format!("prebuilt {}", p.display()),
        Source::Built(p) => format!("built {}", p.display()),
    };
    println!("FSF binary: {} (via {src})", located.bin);

    let mut engine = match uci::Engine::spawn(&located.bin) {
        Ok(e) => e,
        Err(e) => {
            println!();
            println!("SKIP: could not start FSF over UCI: {e}");
            println!();
            println!("{}", locate::INSTALL_HELP);
            return;
        }
    };
    println!(
        "tier: {}",
        if opts.full {
            "--full (+1 ply)"
        } else {
            "default"
        }
    );

    // ---- differential fuzzer (issue #239) ---------------------------------
    // When `--difffuzz` is requested, run the seeded random-game perft cross-check
    // instead of the pinned corpus, and exit with its divergence count.
    if opts.difffuzz {
        let divergences = difffuzz::run(&mut engine, &located.bin, &opts.fuzz);
        engine.quit();
        if divergences > 0 {
            std::process::exit(1);
        }
        return;
    }

    // ---- run the corpus through both engines ------------------------------
    let mut rows: Vec<Row> = Vec::with_capacity(CASES.len());
    let mut mismatches = 0usize;
    let mut skipped = 0usize;

    for case in CASES {
        match run_case(&mut engine, case, opts.full) {
            Ok(row) => {
                if !row.matched {
                    mismatches += 1;
                    report_mismatch(&mut engine, &row);
                }
                rows.push(row);
            }
            Err(e) => {
                skipped += 1;
                eprintln!("skip {}/{}: {e}", case.id.as_str(), case.label);
            }
        }
    }

    print_table(&rows);
    println!();
    print_summary(&rows, mismatches, skipped);

    // Makruk, Capablanca, and Seirawan ride the generic engine (not the
    // `AnyVariant` corpus above), so each has its own comparison loop. Fold their
    // mismatches into the exit status.
    let makruk_mismatches = makruk::run(&mut engine, opts.full);
    // Makpong is a FSF built-in (no variants.ini needed), like makruk; it is
    // Makruk plus the king-may-not-flee-check rule, on the same generic engine.
    let makpong_mismatches = makpong::run(&mut engine, opts.full);
    // Cambodian is a FSF built-in (no variants.ini needed), like makruk; it rides
    // the same generic engine.
    let cambodian_mismatches = cambodian::run(&mut engine, opts.full);
    // ASEAN is a FSF built-in (no variants.ini needed), like makruk; it is
    // Makruk with the symmetric FIDE start array and FIDE-style last-rank,
    // four-target promotion, on the same generic engine. Its mce FEN dialect
    // (`s`/`m`) is rewritten to FSF's `b`/`q` inside asean::run.
    let asean_mismatches = asean::run(&mut engine, opts.full);
    let capablanca_mismatches = capablanca::run(&mut engine, opts.full);
    let capahouse_mismatches = capahouse::run(&mut engine, opts.full);
    let seirawan_mismatches = seirawan::run(&mut engine, opts.full);
    let shouse_mismatches = shouse::run(&mut engine, opts.full);
    let grand_mismatches = grand::run(&mut engine, opts.full);
    let grandhouse_mismatches = grandhouse::run(&mut engine, opts.full);
    // Ten-Cubed and Opulent are FSF built-ins (no variants.ini needed) on the same
    // 10x10 Grand geometry; their mce dialects (`**w`/`**x` and `**w`/`**y`/`**z`
    // second-bank leaper tokens, plus Elephant `e`) are rewritten to FSF's letters
    // inside each module's run (issue #375).
    let tencubed_mismatches = tencubed::run(&mut engine, opts.full);
    let opulent_mismatches = opulent::run(&mut engine, opts.full);
    let duck_mismatches = duck::run(&mut engine, opts.full);
    // Dragon is a FSF built-in (no variants.ini needed): standard chess plus a
    // Bishop+Knight Dragon in each fixed pocket, droppable onto the back rank. Its
    // mce dialect (`a`/`A`) is rewritten to FSF's `d`/`D` inside dragon::run.
    let dragon_mismatches = dragon::run(&mut engine, opts.full);
    // Fog of War is an INI variant FSF lacks entirely: fogofwar::run bundles its
    // own variants.ini definition (inheriting built-in chess) and loads it via
    // VariantPath before comparing.
    let fogofwar_mismatches = fogofwar::run(&mut engine, opts.full);
    let sittuyin_mismatches = sittuyin::run(&mut engine, opts.full);
    // Placement (Pre-Chess) is a FSF built-in (no variants.ini needed), like
    // sittuyin; it rides the same generic engine's deployment phase.
    let placement_mismatches = placement::run(&mut engine, opts.full);
    // Bughouse is a FSF built-in (no variants.ini needed): on a single board it is
    // crazyhouse with the hand fed externally (FSF `twoBoards`), so `go perft` is
    // meaningful and the standard piece letters need no translation.
    let bughouse_mismatches = bughouse::run(&mut engine, opts.full);
    let spartan_mismatches = spartan::run(&mut engine, opts.full);
    let shako_mismatches = shako::run(&mut engine, opts.full);
    let shatar_mismatches = shatar::run(&mut engine, opts.full);
    // Shatranj is a FSF built-in (no variants.ini needed), like makruk; its mce
    // dialect (`*x`/`m`) is rewritten to FSF's `b`/`q` inside shatranj::run.
    let shatranj_mismatches = shatranj::run(&mut engine, opts.full);
    // Courier is a FSF built-in (needs a `largeboards=yes` build for the 12-wide
    // board); its mce dialect (`*x`/`*u`/`*j`/`m`) is rewritten to FSF's
    // `e`/`m`/`w`/`f` inside courier::run.
    let courier_mismatches = courier::run(&mut engine, opts.full);
    let shinobi_mismatches = shinobi::run(&mut engine, opts.full);
    // Shogun is an INI variant (like Shinobi): shogun::run loads FSF's
    // variants.ini (resolved from `$MCE_FSF_VARIANTS_INI`) before driving
    // `UCI_Variant shogun`.
    let shogun_mismatches = shogun::run(&mut engine, opts.full);
    let knightmate_mismatches = knightmate::run(&mut engine, opts.full);
    let xiangqi_mismatches = xiangqi::run(&mut engine, opts.full);
    // Manchu is a FSF built-in (no variants.ini needed), like xiangqi.
    let manchu_mismatches = manchu::run(&mut engine, opts.full);
    let janggi_mismatches = janggi::run(&mut engine, opts.full);
    let shogi_mismatches = shogi::run(&mut engine, opts.full);
    // Sho Shogi is a FSF built-in (no variants.ini needed), like shogi.
    let shoshogi_mismatches = shoshogi::run(&mut engine, opts.full);
    let minishogi_mismatches = minishogi::run(&mut engine, opts.full);
    // Kyoto Shogi is a FSF built-in (no variants.ini needed), like minishogi.
    let kyotoshogi_mismatches = kyotoshogi::run(&mut engine, opts.full);
    // Dobutsu is a FSF built-in (no variants.ini needed), like minishogi.
    let dobutsu_mismatches = dobutsu::run(&mut engine, opts.full);
    let minixiangqi_mismatches = minixiangqi::run(&mut engine, opts.full);
    // Orda is an INI variant: orda::run loads FSF's variants.ini (resolved from the
    // located binary) before driving `UCI_Variant orda`.
    let orda_mismatches = orda::run(&mut engine, &located.bin, opts.full);
    // Gorogoro Shogi Plus is an INI variant: gorogoro::run loads FSF's variants.ini
    // (resolved from the located binary) before driving `UCI_Variant gorogoroplus`.
    let gorogoro_mismatches = gorogoro::run(&mut engine, &located.bin, opts.full);
    // Ordamirror is also an INI variant: ordamirror::run loads FSF's variants.ini
    // (resolved from the located binary) before driving `UCI_Variant ordamirror`.
    let ordamirror_mismatches = ordamirror::run(&mut engine, &located.bin, opts.full);
    // Khan's Chess is an INI variant: khans::run loads FSF's variants.ini (resolved
    // from the located binary) before driving `UCI_Variant khans`.
    let khans_mismatches = khans::run(&mut engine, &located.bin, opts.full);
    let synochess_mismatches = synochess::run(&mut engine, opts.full);
    // Empire is an INI variant: empire::run loads FSF's variants.ini (resolved from
    // the located binary) before driving `UCI_Variant empire`.
    let empire_mismatches = empire::run(&mut engine, &located.bin, opts.full);
    // Hoppel-Poppel is a FSF built-in (no variants.ini needed).
    let hoppelpoppel_mismatches = hoppelpoppel::run(&mut engine, opts.full);
    // Chak is an INI variant: chak::run loads FSF's variants.ini (resolved from the
    // located binary) before driving `UCI_Variant chak`.
    let chak_mismatches = chak::run(&mut engine, &located.bin, opts.full);
    // Mansindam is an INI variant: mansindam::run loads FSF's variants.ini (resolved
    // from the located binary) before driving `UCI_Variant mansindam`.
    let mansindam_mismatches = mansindam::run(&mut engine, &located.bin, opts.full);
    let tori_mismatches = tori::run(&mut engine, opts.full);
    // Cannon Shogi is an INI variant: cannonshogi::run loads FSF's variants.ini
    // (resolved from the located binary) before driving `UCI_Variant cannonshogi`.
    let cannonshogi_mismatches = cannonshogi::run(&mut engine, &located.bin, opts.full);
    // Chennis (7x7 tennis-themed flipping variant) is an INI variant like
    // Mansindam: load `variants.ini` inside chennis::run if the binary lacks it.
    let chennis_mismatches = chennis::run(&mut engine, &located.bin, opts.full);
    // Xiang Fu is an INI variant: xiangfu::run loads FSF's variants.ini (resolved
    // from the located binary) before driving `UCI_Variant xiangfu`.
    let xiangfu_mismatches = xiangfu::run(&mut engine, &located.bin, opts.full);
    // Jieqi (hidden Xiangqi) is not an FSF variant; its full-information core is
    // Xiangqi, so jieqi::run drives FSF `UCI_Variant xiangqi` on the identity-
    // reveal Xiangqi equivalent of each Jieqi position.
    let jieqi_mismatches = jieqi::run(&mut engine, opts.full);
    // Ataxx is a FSF built-in (no variants.ini needed). It is not a chess
    // variant, so mce drives its standalone `mce::ataxx` module, not AnyVariant.
    let ataxx_mismatches = ataxx::run(&mut engine, opts.full);

    engine.quit();

    if mismatches
        + makruk_mismatches
        + makpong_mismatches
        + cambodian_mismatches
        + asean_mismatches
        + capablanca_mismatches
        + capahouse_mismatches
        + seirawan_mismatches
        + shouse_mismatches
        + grand_mismatches
        + grandhouse_mismatches
        + tencubed_mismatches
        + opulent_mismatches
        + duck_mismatches
        + dragon_mismatches
        + fogofwar_mismatches
        + sittuyin_mismatches
        + placement_mismatches
        + bughouse_mismatches
        + spartan_mismatches
        + shako_mismatches
        + shatar_mismatches
        + shatranj_mismatches
        + courier_mismatches
        + shinobi_mismatches
        + shogun_mismatches
        + knightmate_mismatches
        + xiangqi_mismatches
        + manchu_mismatches
        + janggi_mismatches
        + shogi_mismatches
        + shoshogi_mismatches
        + minishogi_mismatches
        + kyotoshogi_mismatches
        + dobutsu_mismatches
        + minixiangqi_mismatches
        + orda_mismatches
        + gorogoro_mismatches
        + ordamirror_mismatches
        + khans_mismatches
        + synochess_mismatches
        + empire_mismatches
        + hoppelpoppel_mismatches
        + chak_mismatches
        + mansindam_mismatches
        + tori_mismatches
        + cannonshogi_mismatches
        + chennis_mismatches
        + xiangfu_mismatches
        + jieqi_mismatches
        + ataxx_mismatches
        > 0
    {
        std::process::exit(1);
    }
}

/// Parse `--build` / `--full` / `--difffuzz [--seed N] [--games K] [--plies P]
/// [--variant X]` / `--help`.
fn parse_args() -> Opts {
    let mut o = Opts {
        build: false,
        full: false,
        difffuzz: false,
        fuzz: difffuzz::Config::default(),
        hachu: false,
        build_hachu: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--build" => o.build = true,
            "--full" => o.full = true,
            "--hachu" => o.hachu = true,
            "--build-hachu" => {
                o.hachu = true;
                o.build_hachu = true;
            }
            "--difffuzz" | "--fuzz" => o.difffuzz = true,
            "--seed" => o.fuzz.seed = parse_value(&mut args, "--seed", parse_seed),
            "--games" => o.fuzz.games = parse_value(&mut args, "--games", |s| s.parse().ok()),
            "--plies" => o.fuzz.plies = parse_value(&mut args, "--plies", |s| s.parse().ok()),
            "--variant" => {
                o.fuzz.variant = Some(parse_value(&mut args, "--variant", |s| Some(s.to_string())))
            }
            "--help" | "-h" => {
                println!("usage: compare-fairy [--build] [--full]");
                println!("       compare-fairy --difffuzz [--seed N] [--games K] [--plies P] [--variant X]");
                println!("       compare-fairy --hachu [--build-hachu]");
                println!("  --build       : clone + build Fairy-Stockfish if no binary is found");
                println!("  --full        : one ply deeper per position");
                println!(
                    "  --hachu       : run the HaChu large-shogi differential-oracle mode (issue #379)"
                );
                println!(
                    "  --build-hachu : clone + build HaChu if no binary is found (implies --hachu)"
                );
                println!("  env MCE_HACHU_BIN=<path> selects an existing HaChu binary");
                println!("  --difffuzz : seeded random-game perft(1..2)+divide fuzzer vs FSF (issue #239)");
                println!("  --seed N   : fuzzer base seed (decimal or 0x-hex; default 0x239)");
                println!("  --games K  : random games per variant (default 3)");
                println!("  --plies P  : max plies per game (default 30)");
                println!("  --variant X: fuzz only mce variant X (e.g. xiangqi, orda)");
                println!("  env MCE_FSF_BIN=<path> selects an existing FSF binary");
                std::process::exit(0);
            }
            other => eprintln!("warning: ignoring unknown argument {other:?}"),
        }
    }
    o
}

/// Pull the next CLI token and parse it with `f`, exiting with a clear message if
/// the value is missing or malformed.
fn parse_value<T>(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
    f: impl Fn(&str) -> Option<T>,
) -> T {
    match args.next().as_deref().and_then(f) {
        Some(v) => v,
        None => {
            eprintln!("error: {flag} needs a valid value");
            std::process::exit(2);
        }
    }
}

/// Parse a `u64` seed in decimal or `0x`-prefixed hexadecimal.
fn parse_seed(s: &str) -> Option<u64> {
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

/// Run one corpus case through both engines and measure it.
fn run_case(engine: &mut uci::Engine, case: &Case, full: bool) -> Result<Row, String> {
    let depth = if full { case.depth + 1 } else { case.depth };

    // mce side.
    let mce_pos =
        AnyVariant::from_fen(case.id, case.fen).map_err(|e| format!("mce rejected FEN: {e:?}"))?;
    let mce_start = Instant::now();
    let mce_nodes = mce_pos.perft(depth);
    let mce_secs = mce_start.elapsed().as_secs_f64();

    // FSF side.
    let fsf = variants::to_fsf(case.id).ok_or("variant not shared with FSF")?;
    let fsf_fen = variants::fen_to_fsf(case.id, case.fen);
    engine.set_variant(fsf.uci_variant, fsf.chess960)?;
    engine.set_position(&fsf_fen)?;
    let fsf_res = engine.go_perft(depth, false)?;

    Ok(Row {
        id: case.id,
        label: case.label,
        fen: case.fen,
        depth,
        mce_nodes,
        fsf_nodes: fsf_res.nodes,
        matched: mce_nodes == fsf_res.nodes,
        mce_secs,
        fsf_secs: fsf_res.elapsed.as_secs_f64(),
    })
}

/// On a mismatch, re-run with the per-move divide on both sides to localise the
/// diverging move, and print the reproduction recipe.
fn report_mismatch(engine: &mut uci::Engine, row: &Row) {
    eprintln!(
        "*** PARITY MISMATCH {}/{} depth {}: mce={} fsf={} ***",
        row.id.as_str(),
        row.label,
        row.depth,
        row.mce_nodes,
        row.fsf_nodes,
    );
    eprintln!("    mce FEN : {}", row.fen);
    let fsf_fen = variants::fen_to_fsf(row.id, row.fen);
    eprintln!("    FSF FEN : {fsf_fen}");
    eprintln!(
        "    reproduce: UCI_Variant={} go perft {} on the FEN above",
        variants::to_fsf(row.id)
            .map(|v| v.uci_variant)
            .unwrap_or("?"),
        row.depth,
    );

    // FSF divide (mce divide is not part of the public API; FSF's localises the
    // diverging first move, which is enough to start debugging).
    if let Some(fsf) = variants::to_fsf(row.id) {
        if engine.set_variant(fsf.uci_variant, fsf.chess960).is_ok()
            && engine.set_position(&fsf_fen).is_ok()
        {
            if let Ok(res) = engine.go_perft(row.depth, true) {
                eprintln!("    FSF divide ({} moves):", res.divide.len());
                for (mv, n) in res.divide.iter().take(40) {
                    eprintln!("      {mv}: {n}");
                }
            }
        }
    }
}

/// Print the per-position comparison table.
fn print_table(rows: &[Row]) {
    let head = format!(
        "{:<16} {:<16} {:>5} {:>14} {:>14} {:>7} {:>10} {:>10} {:>8}",
        "variant",
        "position",
        "depth",
        "mce nodes",
        "fsf nodes",
        "match",
        "mce Mn/s",
        "fsf Mn/s",
        "mce/fsf",
    );
    println!("{head}");
    println!("{}", "-".repeat(head.len()));
    for r in rows {
        println!(
            "{:<16} {:<16} {:>5} {:>14} {:>14} {:>7} {:>10.1} {:>10.1} {:>7.2}x",
            r.id.as_str(),
            r.label,
            r.depth,
            r.mce_nodes,
            r.fsf_nodes,
            if r.matched { "ok" } else { "MISMATCH" },
            r.mce_mnps(),
            r.fsf_mnps(),
            r.speedup(),
        );
    }
}

/// Print the per-variant aggregate + overall summary.
fn print_summary(rows: &[Row], mismatches: usize, skipped: usize) {
    println!("per-variant parity + node-weighted throughput:");
    let head = format!(
        "{:<16} {:>5} {:>16} {:>10} {:>10} {:>9}",
        "variant", "pos", "nodes verified", "mce Mn/s", "fsf Mn/s", "mce/fsf",
    );
    println!("{head}");
    println!("{}", "-".repeat(head.len()));

    let (mut g_nodes, mut g_mce_s, mut g_fsf_s) = (0u64, 0.0f64, 0.0f64);
    for &id in VARIANTS {
        let group: Vec<&Row> = rows.iter().filter(|r| r.id == id).collect();
        if group.is_empty() {
            continue;
        }
        let nodes: u64 = group.iter().map(|r| r.mce_nodes).sum();
        let mce_s: f64 = group.iter().map(|r| r.mce_secs).sum();
        let fsf_s: f64 = group.iter().map(|r| r.fsf_secs).sum();
        let all_ok = group.iter().all(|r| r.matched);
        g_nodes += nodes;
        g_mce_s += mce_s;
        g_fsf_s += fsf_s;
        println!(
            "{:<16} {:>5} {:>16} {:>10.1} {:>10.1} {:>8.2}x {}",
            id.as_str(),
            group.len(),
            nodes,
            if mce_s > 0.0 {
                nodes as f64 / mce_s / 1e6
            } else {
                0.0
            },
            if fsf_s > 0.0 {
                nodes as f64 / fsf_s / 1e6
            } else {
                0.0
            },
            if mce_s > 0.0 { fsf_s / mce_s } else { 0.0 },
            if all_ok { "" } else { "<- MISMATCH" },
        );
    }
    println!("{}", "-".repeat(head.len()));
    println!(
        "{:<16} {:>5} {:>16} {:>10.1} {:>10.1} {:>8.2}x",
        "OVERALL",
        rows.len(),
        g_nodes,
        if g_mce_s > 0.0 {
            g_nodes as f64 / g_mce_s / 1e6
        } else {
            0.0
        },
        if g_fsf_s > 0.0 {
            g_nodes as f64 / g_fsf_s / 1e6
        } else {
            0.0
        },
        if g_mce_s > 0.0 {
            g_fsf_s / g_mce_s
        } else {
            0.0
        },
    );
    println!();

    if mismatches == 0 {
        println!(
            "OK: all {} shared positions matched FSF ({} nodes verified across {} variants); \
{} skipped.",
            rows.len(),
            g_nodes,
            VARIANTS
                .iter()
                .filter(|&&id| rows.iter().any(|r| r.id == id))
                .count(),
            skipped,
        );
    } else {
        eprintln!(
            "ERROR: {mismatches} parity mismatch(es) vs FSF (see the FEN+depth above to reproduce).",
        );
    }
}
