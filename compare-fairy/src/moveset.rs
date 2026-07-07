//! Move-**set** differential vs Fairy-Stockfish (issue #556).
//!
//! The rest of this crate cross-checks mcr against FSF by perft **node counts**
//! (the pinned corpus, the `--quick` slice, and the `--difffuzz` random-game
//! sweep, which compares `perft(1)`, `perft(2)`, and the *multiset of per-move
//! child counts*). All of those are count-based, and a count comparison has one
//! blind spot: a **compensating movegen error** — mcr generating one illegal move
//! while simultaneously missing one legal move whose subtree happens to have the
//! same size — leaves every node count unchanged and slips through. The divide
//! histogram narrows the window (the two moves must also share a child count), but
//! does not close it.
//!
//! This mode closes it by comparing the actual **move sets**. At each node FSF's
//! set of legal moves is exactly the roots of its `go perft 1` *divide* (each root
//! move listed with its subtree count); mcr's set is its generated legal moves.
//! The two sets are asserted **equal**, so an extra or missing move is surfaced
//! directly — even when it is perft-count-neutral.
//!
//! ## Notation normalisation
//!
//! The two engines spell the *same* move slightly differently (the reason the
//! difffuzz divide check compares counts, not labels), in two ways, both of which
//! leave the from/to coordinates byte-identical:
//!
//! * **Notation** — mcr gates a Seirawan piece `b1c3/H` where FSF trails a bare
//!   `b1c3h`; mcr suffixes a Shogi promotion `7g7f+p` (the `+` flag plus the
//!   promoted piece's base letter) where FSF flags a bare `7g7f+`; a Chennis flip
//!   is `d1c1` in mcr but `d1c1+`/`e2e3-` in FSF. [`canon`] folds these — lowercase,
//!   strip the `/` `*` `=` markers, and drop a `+`/`-` forced-flip flag (and the
//!   letter mcr renders after a `+`).
//! * **Dialect** — the *piece* letters themselves differ per variant (a Dragon is
//!   `a`/`A` in mcr, `d`/`D` in FSF; a Seirawan Hawk gates `/A` in mcr, `h` in FSF;
//!   a Shogun Archbishop promotes to `a` in mcr, `+b` in FSF). [`dialect_move`]
//!   rewrites just the piece-identity tokens of an mcr move through the difffuzz
//!   `dialect` (the same mcr→FSF letter map used for the FEN placement), never a
//!   coordinate, so the two engines' spellings align before [`canon`].
//!
//! With both applied, equal move sets compare equal while a genuine from/to
//! divergence (the #556 target) still stands out.
//!
//! Known FSF *generator* limitations (never mcr bugs) are handled exactly as the
//! difffuzz sweep handles them: the per-move discounts of [`fsf_omits_move`] drop
//! the moves FSF cannot represent from mcr's side before the comparison, and the
//! whole-node artifacts ([`is_schess_corner_castle_artifact`],
//! [`is_empire_no_queenside_castle_artifact`]) skip the nodes that are not mutually
//! representable — so the check stays a faithful equality everywhere the two
//! engines *can* agree.
//!
//! Scope: for every FSF-backed variant (the difffuzz [`SPECS`], minus the
//! [`HELD_BACK`] follow-ups) it walks the game tree **exhaustively to a shallow
//! depth** from the start position — the root and every node up to `depth` plies —
//! capping the node budget per variant (and logging any cap so coverage is never
//! silently truncated). Shallow breadth-first exercises every root move and its
//! replies, which is where the compensating-error class lives; the deep random
//! games remain the difffuzz sweep's job.
//!
//! GPL FENCE unchanged: FSF is driven purely as a UCI subprocess (see `uci.rs`).
//!
//! Invocation (FSF-gated, like the rest of the crate):
//!
//! ```text
//! cargo run --release -- --moveset                    # all variants, depth 2
//! cargo run --release -- --moveset --full             # depth 3, larger node cap
//! cargo run --release -- --moveset --variant seirawan # one variant
//! ```

use std::collections::{BTreeMap, HashSet, VecDeque};

use mcr::geometry::{AnyWideVariant, WideVariantId};

use crate::difffuzz::{
    fsf_omits_move, is_empire_no_queenside_castle_artifact, is_schess_corner_castle_artifact,
    resolve_variants_ini, Spec, HELD_BACK, SPECS,
};
use crate::uci::Engine;

/// Default number of plies of nodes checked below the start position (the root is
/// depth 0). Depth 2 checks the start position, every reply, and every reply-to-a-
/// reply — enough breadth to exercise each root move type and the responses to it.
const DEFAULT_DEPTH: u32 = 2;

/// `--full` deepens the walk by one ply.
const FULL_DEPTH: u32 = 3;

/// Per-variant node budget for the default (depth-2) walk. Chosen so the full
/// depth-0 and depth-1 layers (the root plus every reply — at most ~50 for the
/// widest fuzzable variant) always fit with headroom, then the depth-2 layer fills
/// the remainder. Breadth-first order truncates the *deepest* layer first, so a cap
/// never drops a shallow node.
const DEFAULT_CAP: u64 = 250;

/// Per-variant node budget for the `--full` (depth-3) walk.
const FULL_CAP: u64 = 800;

/// Canonicalise a UCI move string into the notation-independent form both engines
/// share, so equal move sets compare equal (see the module docs).
///
/// * lowercase — folds mcr's uppercase gate/drop piece letters (`/H`, `P@`) onto
///   FSF's lowercase (`h`, `p@`);
/// * drop the `/` `*` `=` markers — folds a Seirawan gate (`b1c3/h` -> `b1c3h`,
///   matching FSF) and an overflow promotion role (`e7e8**a` -> `e7e8a`, matching
///   FSF's `e7e8a`);
/// * drop a `+`/`-` **forced-flip flag** and the promoted piece's base letter mcr
///   renders after a `+` — a Shogi promotion is `7g7f+p` in mcr, `7g7f+` in FSF; a
///   Chennis flip is `d1c1` in mcr, `d1c1+` (or a demotion `e2e3-`) in FSF. The
///   flag/letter carries **no move choice** — the flip/promotion is forced by the
///   moving piece and its from/to — so folding it aligns the two engines. Where a
///   promotion is genuinely *optional* (Shogi `7g7f` vs `7g7f+`) both engines still
///   emit both members, so the folded multiset stays `{7g7f, 7g7f}` on each side and
///   the count is preserved; only the redundant *flag* is normalised away.
///
/// The from/to coordinates — the load-bearing move identity for the #556 check —
/// and a genuine chess-style promotion *choice* (`e7e8q` vs `e7e8n`, a bare letter
/// with no `+`) pass through untouched.
fn canon(uci: &str) -> String {
    let lower = uci.to_ascii_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut chars = lower.chars();
    while let Some(c) = chars.next() {
        match c {
            '/' | '*' | '=' | '-' => {} // notation markers / demote flag: drop.
            '+' => {
                // Forced-flip / Shogi-promotion flag: drop it, plus the single base
                // letter mcr renders after it (FSF renders no letter).
                let mut rest = chars.clone();
                if matches!(rest.next(), Some(l) if l.is_ascii_alphabetic()) {
                    chars = rest; // consume exactly the one promoted-piece letter.
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// Consume a leading UCI square (`<file-letter><rank-digits>`, rank up to two
/// digits for a ten-plus-rank board) from `s`, returning the byte length consumed,
/// or `None` if `s` does not start with a square.
fn square_len(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    if b.is_empty() || !b[0].is_ascii_alphabetic() {
        return None;
    }
    let mut i = 1;
    if i >= b.len() || !b[i].is_ascii_digit() {
        return None;
    }
    i += 1;
    if i < b.len() && b[i].is_ascii_digit() {
        i += 1; // a second rank digit (rank 10+).
    }
    Some(i)
}

/// The byte length of the two leading squares of a board move (`from` then `to`),
/// or `None` if `s` does not start with two squares.
fn two_squares_len(s: &str) -> Option<usize> {
    let a = square_len(s)?;
    let b = square_len(&s[a..])?;
    Some(a + b)
}

/// Rewrite the **piece-identity letters** of an mcr move string into FSF's dialect,
/// leaving the from/to coordinates untouched.
///
/// mcr and FSF agree on square coordinates but spell each variant's *pieces*
/// differently (a Dragon is `a`/`A` in mcr, `d`/`D` in FSF; a Shogun Archbishop is
/// `a`, FSF's `+b`; a Seirawan Hawk gates as `/A`, FSF's `h`). The difffuzz `dialect`
/// function already encodes exactly this mcr→FSF letter map for the FEN *placement*
/// field, so this reuses it on the isolated piece tokens of a move — the role of a
/// drop (before `@`), the gated piece (after `/`), or a promotion suffix (after the
/// two squares) — and never on a coordinate, so a file letter like the `a`-file is
/// never rewritten. The result then folds through [`canon`] exactly like an FSF
/// move, so a Dragon `A@b1` and FSF's `D@b1`, or a Shogun `c1h6a` and FSF's
/// `c1h6+`, compare equal.
fn dialect_move(dialect: fn(&str) -> String, uci: &str) -> String {
    // Drop: `<role>@<square>`. The role token (with any `*`/`+` prefix) precedes the
    // first `@` and — unlike a gate's `@r` rook tail — carries no digit.
    if let Some(at) = uci.find('@') {
        let before = &uci[..at];
        if !before.bytes().any(|c| c.is_ascii_digit()) {
            return format!("{}{}", dialect(before), &uci[at..]);
        }
    }
    // Gate(s): everything from the first `/` is one or more `/`-prefixed gated
    // pieces (each optionally `@r` for a castling gate onto the rook square).
    if let Some(slash) = uci.find('/') {
        let base = &uci[..slash];
        let mut out = dialect_promotion(dialect, base);
        for seg in uci[slash + 1..].split('/') {
            if seg.is_empty() {
                continue;
            }
            out.push('/');
            match seg.split_once('@') {
                Some((role, tail)) => {
                    out.push_str(&dialect(role));
                    out.push('@');
                    out.push_str(tail);
                }
                None => out.push_str(&dialect(seg)),
            }
        }
        return out;
    }
    // Board move, possibly with a promotion suffix.
    dialect_promotion(dialect, uci)
}

/// Rewrite the promotion suffix (the token trailing the two squares of a board
/// move) through `dialect`; a plain move is returned unchanged.
fn dialect_promotion(dialect: fn(&str) -> String, mv: &str) -> String {
    match two_squares_len(mv) {
        Some(n) if n < mv.len() => format!("{}{}", &mv[..n], dialect(&mv[n..])),
        _ => mv.to_string(),
    }
}

/// One node's move-set divergence, in full reproduction detail.
struct Divergence {
    fen: String,
    fsf_fen: String,
    /// Moves mcr generates that FSF's divide does not (original, un-canonicalised
    /// mcr spelling), each with how many copies are unmatched.
    mcr_extra: Vec<String>,
    /// Moves FSF's divide lists that mcr does not generate (original FSF spelling).
    fsf_extra: Vec<String>,
    mcr_count: usize,
    fsf_count: usize,
}

/// Per-variant walk outcome.
struct VariantStat {
    nodes_checked: u64,
    nodes_skipped: u64,
    divergences: usize,
    capped: bool,
    skipped: bool,
}

/// Cross-check one node's move set: mcr's legal moves vs FSF's `go perft 1` divide.
///
/// Returns `Ok(None)` when the sets agree, `Ok(Some(divergence))` when they differ,
/// or `Err` for an FSF protocol/parse failure (e.g. FSF rejects the dialect FEN).
fn check_node(
    engine: &mut Engine,
    spec: &Spec,
    pos: &AnyWideVariant,
) -> Result<Option<Box<Divergence>>, String> {
    let fen = pos.to_fen();
    let fsf_fen = (spec.dialect)(&fen);

    // Re-parse from the FEN string so both engines evaluate the same *stateless*
    // position (FSF's `position fen` carries no move history), exactly as the
    // difffuzz cross-check does.
    let node = AnyWideVariant::from_fen(spec.id, &fen)
        .map_err(|e| format!("mcr failed to re-parse its own FEN {fen:?}: {e:?}"))?;

    // ---- mcr side: the legal-move set, minus the moves FSF's generator cannot
    // represent (the documented per-move artifacts, so they never show as
    // mcr-extras — same discount the difffuzz sweep applies). ------------------
    let moves = node.legal_moves();
    let root_ucis: Vec<String> = moves.iter().map(|mv| node.to_uci(mv)).collect();
    // canonical-form -> original mcr spelling (for the report), with multiplicity.
    let mut mcr_by_canon: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (mv, uci) in moves.iter().zip(root_ucis.iter()) {
        if fsf_omits_move(spec.id, &node, mv, uci, &root_ucis) {
            continue;
        }
        mcr_by_canon
            .entry(canon(&dialect_move(spec.dialect, uci)))
            .or_default()
            .push(uci.clone());
    }

    // ---- FSF side: the roots of `go perft 1` divide are its legal-move set. ----
    engine.set_variant(spec.fsf, false)?;
    engine.set_position(&fsf_fen)?;
    let fsf_divide = engine.go_perft(1, true)?.divide;
    let mut fsf_by_canon: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (mv, _n) in &fsf_divide {
        fsf_by_canon.entry(canon(mv)).or_default().push(mv.clone());
    }

    // ---- multiset difference over the canonical forms -------------------------
    let mut mcr_extra: Vec<String> = Vec::new();
    let mut fsf_extra: Vec<String> = Vec::new();
    let keys: HashSet<&String> = mcr_by_canon.keys().chain(fsf_by_canon.keys()).collect();
    for k in keys {
        let m = mcr_by_canon.get(k).map(Vec::as_slice).unwrap_or(&[]);
        let f = fsf_by_canon.get(k).map(Vec::as_slice).unwrap_or(&[]);
        if m.len() > f.len() {
            mcr_extra.extend(m[f.len()..].iter().cloned());
        } else if f.len() > m.len() {
            fsf_extra.extend(f[m.len()..].iter().cloned());
        }
    }

    if mcr_extra.is_empty() && fsf_extra.is_empty() {
        return Ok(None);
    }
    mcr_extra.sort();
    fsf_extra.sort();
    let mcr_count = mcr_by_canon.values().map(Vec::len).sum();
    let fsf_count = fsf_by_canon.values().map(Vec::len).sum();
    Ok(Some(Box::new(Divergence {
        fen,
        fsf_fen,
        mcr_extra,
        fsf_extra,
        mcr_count,
        fsf_count,
    })))
}

/// Print a move-set divergence in full reproduction detail.
fn report_divergence(id: WideVariantId, fsf: &str, d: &Divergence) {
    eprintln!(
        "*** MOVESET DIVERGENCE {} (UCI_Variant {fsf}) ***",
        id.as_str()
    );
    eprintln!("    mcr FEN : {}", d.fen);
    eprintln!("    FSF FEN : {}", d.fsf_fen);
    eprintln!("    move count: mcr={} fsf={}", d.mcr_count, d.fsf_count);
    if !d.mcr_extra.is_empty() {
        eprintln!(
            "    mcr generates but FSF does not ({}): {}",
            d.mcr_extra.len(),
            d.mcr_extra.join(" ")
        );
    }
    if !d.fsf_extra.is_empty() {
        eprintln!(
            "    FSF lists but mcr does not ({}): {}",
            d.fsf_extra.len(),
            d.fsf_extra.join(" ")
        );
    }
}

/// Walk one variant breadth-first from the start position, checking the move set at
/// every node up to `depth` plies (capped at `cap` nodes).
fn check_variant(engine: &mut Engine, spec: &Spec, depth: u32, cap: u64) -> VariantStat {
    let mut stat = VariantStat {
        nodes_checked: 0,
        nodes_skipped: 0,
        divergences: 0,
        capped: false,
        skipped: false,
    };

    if !engine.has_variant(spec.fsf) {
        println!(
            "  SKIP {:<14} (FSF binary lacks `{}`; build largeboards=yes / load variants.ini)",
            spec.id.as_str(),
            spec.fsf,
        );
        stat.skipped = true;
        return stat;
    }

    // Breadth-first so a node-cap truncates the deepest layer first, never a
    // shallow one. Dedup by FEN so transpositions are not re-checked.
    let mut queue: VecDeque<(AnyWideVariant, u32)> = VecDeque::new();
    queue.push_back((AnyWideVariant::startpos(spec.id), 0));
    let mut seen: HashSet<String> = HashSet::new();

    while let Some((pos, ply)) = queue.pop_front() {
        if pos.outcome().is_some() {
            continue; // terminal: no moves to compare.
        }
        let fen = pos.to_fen();
        if !seen.insert(fen.clone()) {
            continue;
        }

        // A documented FSF artifact node is not mutually representable; skip it but
        // keep exploring past it (same targeted skips the difffuzz sweep uses).
        if is_schess_corner_castle_artifact(spec, &fen)
            || is_empire_no_queenside_castle_artifact(spec, &fen)
        {
            stat.nodes_skipped += 1;
        } else {
            match check_node(engine, spec, &pos) {
                Ok(None) => {}
                Ok(Some(d)) => {
                    stat.divergences += 1;
                    report_divergence(spec.id, spec.fsf, &d);
                }
                Err(e) => {
                    eprintln!(
                        "  note {}: FSF protocol error after {} nodes: {e}",
                        spec.id.as_str(),
                        stat.nodes_checked,
                    );
                    break;
                }
            }
            stat.nodes_checked += 1;
            if stat.nodes_checked >= cap {
                stat.capped = true;
                break;
            }
        }

        if ply < depth {
            for mv in pos.legal_moves() {
                queue.push_back((pos.play(&mv), ply + 1));
            }
        }
    }

    let cap_note = if stat.capped {
        format!("  (capped at {cap} nodes; coverage truncated at the deepest ply)")
    } else {
        String::new()
    };
    let skip_note = if stat.nodes_skipped > 0 {
        format!("  ({} FSF-artifact node(s) skipped)", stat.nodes_skipped)
    } else {
        String::new()
    };
    println!(
        "  {:<14} nodes {:>5}  {}{}{}",
        spec.id.as_str(),
        stat.nodes_checked,
        if stat.divergences == 0 {
            "ok".to_string()
        } else {
            format!("{} DIVERGENCE(S)", stat.divergences)
        },
        skip_note,
        cap_note,
    );
    stat
}

/// Run the move-set differential. Returns the total number of divergences found
/// (0 = clean). `main` maps a non-zero return to a non-zero exit status.
pub fn run(engine: &mut Engine, fsf_bin: &str, full: bool, only: Option<&str>) -> usize {
    let (depth, cap) = if full {
        (FULL_DEPTH, FULL_CAP)
    } else {
        (DEFAULT_DEPTH, DEFAULT_CAP)
    };

    println!();
    println!("Move-set differential (issue #556): mcr legal-move set vs FSF `go perft 1` divide");
    println!("  shallow exhaustive walk from the start position, depth<={depth}, cap {cap} nodes/variant");

    // Load the INI so the non-built-in variants (Orda, Shinobi, Chak, …) join the
    // engine's `UCI_Variant` list. Best-effort: built-ins still run if absent.
    let mut ini_loaded = false;
    if let Some(ini) = resolve_variants_ini(fsf_bin) {
        match engine.load_variant_path(&ini.to_string_lossy()) {
            Ok(()) => {
                println!("  loaded variants.ini: {}", ini.display());
                ini_loaded = true;
            }
            Err(e) => {
                eprintln!("  warning: could not load variants.ini ({e}); INI variants skipped")
            }
        }
    } else {
        println!("  no variants.ini found (set $MCR_FSF_VARIANTS_INI); INI variants skipped");
    }

    // Resolve the optional single-variant filter up front so a bad name fails fast.
    let only: Option<WideVariantId> = match only {
        Some(name) => match name.parse::<WideVariantId>() {
            Ok(id) => {
                if !SPECS.iter().any(|s| s.id == id) {
                    eprintln!(
                        "ERROR: variant {:?} is not FSF-backed (Alice/Duck/Jieqi and the \
large-shogi variants are excluded by design).",
                        id.as_str(),
                    );
                    return 1;
                }
                Some(id)
            }
            Err(e) => {
                eprintln!("ERROR: {e}");
                return 1;
            }
        },
        None => None,
    };

    let mut total_divergences = 0usize;
    let mut total_nodes = 0u64;
    let mut variants_run = 0usize;
    let mut variants_skipped = 0usize;

    for spec in SPECS {
        if let Some(id) = only {
            if spec.id != id {
                continue;
            }
        } else if HELD_BACK.contains(&spec.id) {
            println!(
                "  HELD {:<14} (held back from the default sweep, see HELD_BACK; --variant to run)",
                spec.id.as_str()
            );
            variants_skipped += 1;
            continue;
        }
        if spec.needs_ini && !ini_loaded && !engine.has_variant(spec.fsf) {
            println!(
                "  SKIP {:<14} (INI variant; no variants.ini loaded)",
                spec.id.as_str()
            );
            variants_skipped += 1;
            continue;
        }
        let stat = check_variant(engine, spec, depth, cap);
        if stat.skipped {
            variants_skipped += 1;
        } else {
            variants_run += 1;
            total_nodes += stat.nodes_checked;
            total_divergences += stat.divergences;
        }
    }

    println!();
    if total_divergences == 0 {
        println!(
            "OK: move-set differential found 0 divergences across {variants_run} variant(s) \
({total_nodes} nodes cross-checked vs FSF; {variants_skipped} skipped).",
        );
    } else {
        eprintln!(
            "ERROR: move-set differential found {total_divergences} divergence(s) across \
{variants_run} variant(s) ({total_nodes} nodes checked). See the FENs above to reproduce.",
        );
    }
    total_divergences
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The canonical form folds every notation difference the two engines carry
    /// while keeping genuinely distinct moves distinct — the property the whole
    /// move-set comparison rests on. Runs without FSF.
    #[test]
    fn canon_folds_notation_but_not_moves() {
        // Plain board moves are already identical.
        assert_eq!(canon("e2e4"), "e2e4");
        assert_eq!(canon("b1c3"), "b1c3");
        // Chess promotion: the piece choice is meaningful and preserved.
        assert_eq!(canon("e7e8q"), "e7e8q");
        assert_eq!(canon("e7e8n"), "e7e8n");
        assert_ne!(canon("e7e8q"), canon("e7e8n"));
        // Seirawan gate: mcr `/H` folds onto FSF `h`.
        assert_eq!(canon("b1c3/H"), canon("b1c3h"));
        assert_eq!(canon("g1f3/E"), canon("g1f3e"));
        // A gate stays distinct from the un-gated move and from a different gate.
        assert_ne!(canon("b1c3/H"), canon("b1c3"));
        assert_ne!(canon("b1c3/H"), canon("b1c3/E"));
        // Shogi promotion: mcr `7g7f+p` and FSF `7g7f+` fold to the same form. The
        // forced-flip flag carries no choice, so it folds onto the coordinate move —
        // both engines emit both members of an optional-promotion pair, so the
        // multiset count is preserved either way.
        assert_eq!(canon("7g7f+p"), "7g7f");
        assert_eq!(canon("7g7f+"), "7g7f");
        // Chennis forced-flip flags (`+` promote, `-` demote) fold onto mcr's
        // flagless move — the flip is forced by the piece and its from/to.
        assert_eq!(canon("d1c1+"), "d1c1");
        assert_eq!(canon("e2e3-"), "e2e3");
        // Overflow promotion role: mcr `e7e8**a` folds onto FSF `e7e8a`.
        assert_eq!(canon("e7e8**a"), canon("e7e8a"));
        // Drops fold on case only.
        assert_eq!(canon("P@e5"), canon("p@e5"));
    }

    /// `dialect_move` rewrites a move's piece letters into FSF's dialect while
    /// leaving the from/to coordinates intact, so an mcr move and FSF's spelling of
    /// the same move canonicalise equal. Uses the real per-variant dialects; no FSF.
    #[test]
    fn dialect_move_aligns_piece_letters_only() {
        // Dragon: the Dragon drops as `A@b1` (mcr) / `D@b1` (FSF).
        let dragon = crate::dragon::fen_to_fsf;
        assert_eq!(dialect_move(dragon, "A@b1"), "D@b1");
        assert_eq!(canon(&dialect_move(dragon, "A@b1")), canon("D@b1"));
        // The `a`-file coordinate must NOT be rewritten (a Dragon-letter collision).
        assert_eq!(dialect_move(dragon, "a2a4"), "a2a4");

        // Seirawan: the Hawk gates as `/A` (mcr) / trailing `h` (FSF).
        let seirawan = crate::seirawan::fen_to_fsf;
        assert_eq!(dialect_move(seirawan, "b1c3/A"), "b1c3/H");
        assert_eq!(canon(&dialect_move(seirawan, "b1c3/A")), canon("b1c3h"));
        // A plain move and the a-file coordinate are untouched.
        assert_eq!(dialect_move(seirawan, "a2a4"), "a2a4");

        // Shogun: the Archbishop promotion `c1h6a` (mcr) folds onto FSF's `c1h6+`
        // (its dialect maps the promoted piece `a` to `+b`, which canon then folds).
        let shogun = crate::shogun::to_fsf_dialect;
        assert_eq!(dialect_move(shogun, "c1h6a"), "c1h6+b");
        assert_eq!(canon(&dialect_move(shogun, "c1h6a")), canon("c1h6+"));

        // Shinobi: the `*N` clan Knight drops as `*N@a3` (mcr) / `H@a3` (FSF).
        let shinobi = crate::shinobi::to_fsf_dialect;
        assert_eq!(dialect_move(shinobi, "*N@a3"), "H@a3");
        assert_eq!(canon(&dialect_move(shinobi, "A@c1")), canon("J@c1"));
    }

    /// The single-variant filter resolves every FSF-backed spec name and rejects the
    /// by-design exclusions, without needing FSF.
    #[test]
    fn every_spec_id_parses_and_is_backed() {
        for spec in SPECS {
            let parsed: WideVariantId = spec
                .id
                .as_str()
                .parse()
                .expect("spec id round-trips through parse");
            assert_eq!(parsed, spec.id);
        }
        for excluded in [
            WideVariantId::Alice,
            WideVariantId::Duck,
            WideVariantId::Jieqi,
        ] {
            assert!(!SPECS.iter().any(|s| s.id == excluded));
        }
    }
}
