//! EPD / perft test-suite runner over a pinned corpus (issue #356).
//!
//! The EPD parsers landed in #317 (`mcr::Epd` for standard chess and
//! `mcr::geometry::WideEpd` for the fairy/wide layer) but were never wired into a
//! *test harness*. This file is that harness: it reads the pinned corpus
//! [`CORPUS`] (`tests/epd_suite_corpus.epd`), and for every record checks two
//! kinds of assertion against the live engine.
//!
//! ## Corpus format
//!
//! Each non-blank, non-`#` line is `<variant> <EPD>`. The first whitespace token
//! is a variant tag the runner maps to an engine:
//!
//! * `standard` — the concrete 8x8 engine (`mcr::Epd` + `mcr::perft`).
//! * anything else — a [`WideVariantId`] name/alias, driving the wide layer
//!   (`WideEpd` + `AnyWideVariant::perft`).
//!
//! The remainder is an EPD record: the variant's structural FEN fields (the two
//! move clocks dropped) followed by operations.
//!
//! ## Opcode conventions
//!
//! * **perft suite** — `Dn <count>` (e.g. `D1 20; D2 400; D3 8902`), the classic
//!   CPW perft-suite opcode also documented by [`mcr::Epd`]. For each `Dn` the
//!   runner builds the position, runs `perft(n)`, and asserts the node count
//!   equals the pinned `<count>`.
//! * **best/avoid-move suite** — `bm <SAN>...` / `am <SAN>...`. Each operand is
//!   resolved through the variant's own SAN parser and must (a) resolve to a
//!   legal move and (b) be present in `legal_moves()`. `am` moves are validated
//!   the same way (legal-but-flagged): mcr is a move-generation library, not a
//!   search engine, so the suite verifies legality + parse + presence, not which
//!   move an engine would choose.
//!
//! Every pinned perft count is FSF-confirmed and reuses the figures already in
//! the crate's `tests/perft_*.rs` suites (the standard-chess counts are the
//! published CPW / Kiwipete numbers).

use mcr::geometry::{AnyWideVariant, WideVariantId};
use mcr::{Epd, Position};

/// The pinned corpus, compiled into the test binary so the suite is hermetic.
const CORPUS: &str = include_str!("epd_suite_corpus.epd");

/// Tallies the assertions a single record contributed, for the run summary.
#[derive(Default, Clone, Copy)]
struct Counts {
    records: usize,
    perft_checks: usize,
    move_checks: usize,
}

impl Counts {
    fn add(&mut self, other: Counts) {
        self.records += other.records;
        self.perft_checks += other.perft_checks;
        self.move_checks += other.move_checks;
    }
}

/// Parses `opcode` as a `Dn` perft opcode, returning the depth `n` (e.g. `"D3"`
/// → `Some(3)`). Any non-`D`, empty, or non-numeric suffix yields `None`.
fn perft_depth(opcode: &str) -> Option<u32> {
    let digits = opcode.strip_prefix('D')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// Runs the standard-chess record `epd` (already stripped of its variant tag)
/// through the concrete engine.
fn check_standard(epd: &str, label: &str) -> Counts {
    let record = Epd::parse(epd).unwrap_or_else(|e| panic!("[{label}] EPD parse failed: {e}"));
    let pos: &Position = record.position();
    let legal = pos.legal_moves();
    let mut counts = Counts {
        records: 1,
        ..Counts::default()
    };

    for (opcode, operands) in record.operations() {
        if let Some(depth) = perft_depth(opcode) {
            let expected: u64 = operands
                .first()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| panic!("[{label}] {opcode} needs a numeric operand"));
            let got = mcr::perft(pos, depth);
            assert_eq!(
                got, expected,
                "[{label}] perft({depth}): pinned {expected}, got {got}"
            );
            counts.perft_checks += 1;
        }
    }

    for (opcode, resolved) in [("bm", record.best_moves()), ("am", record.avoid_moves())] {
        let Some(result) = resolved else { continue };
        let moves = result
            .unwrap_or_else(|e| panic!("[{label}] {opcode} did not resolve to legal SAN: {e}"));
        for mv in moves {
            assert!(
                legal.contains(&mv),
                "[{label}] {opcode} move {mv:?} is not in legal_moves()"
            );
            counts.move_checks += 1;
        }
    }

    counts
}

/// Runs a wide-layer record `epd` (already stripped of its variant tag) of the
/// variant named by `tag` through the wide engine.
fn check_wide(tag: &str, epd: &str, label: &str) -> Counts {
    let variant: WideVariantId = tag
        .parse()
        .unwrap_or_else(|e| panic!("[{label}] unknown variant tag {tag:?}: {e}"));
    let record = mcr::geometry::WideEpd::parse(variant, epd)
        .unwrap_or_else(|e| panic!("[{label}] EPD parse failed: {e}"));
    let pos: &AnyWideVariant = record.position();
    let legal = pos.legal_moves();
    let mut counts = Counts {
        records: 1,
        ..Counts::default()
    };

    for (opcode, operands) in record.operations() {
        if let Some(depth) = perft_depth(opcode) {
            let expected: u64 = operands
                .first()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| panic!("[{label}] {opcode} needs a numeric operand"));
            let got = pos.perft(depth);
            assert_eq!(
                got, expected,
                "[{label}] perft({depth}): pinned {expected}, got {got}"
            );
            counts.perft_checks += 1;
        }
    }

    for (opcode, resolved) in [("bm", record.best_moves()), ("am", record.avoid_moves())] {
        let Some(result) = resolved else { continue };
        let moves = result
            .unwrap_or_else(|e| panic!("[{label}] {opcode} did not resolve to legal SAN: {e}"));
        for mv in moves {
            assert!(
                legal.contains(&mv),
                "[{label}] {opcode} move {mv:?} is not in legal_moves()"
            );
            counts.move_checks += 1;
        }
    }

    counts
}

/// Checks a single corpus line, dispatching on its variant tag. Returns `None`
/// for blank / comment lines.
fn check_line(line: &str) -> Option<Counts> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (tag, rest) = line
        .split_once(char::is_whitespace)
        .unwrap_or_else(|| panic!("corpus line has no EPD after its variant tag: {line:?}"));
    let epd = rest.trim_start();
    // The record id (if any) makes panics point at the offending line.
    let label = epd
        .split("id \"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .unwrap_or(tag);

    Some(if tag.eq_ignore_ascii_case("standard") {
        check_standard(epd, label)
    } else {
        check_wide(tag, epd, label)
    })
}

/// The whole pinned corpus passes: every `Dn` perft count matches the live
/// engine and every `bm`/`am` SAN operand parses to a present legal move.
#[test]
fn pinned_corpus_passes() {
    let mut total = Counts::default();
    for line in CORPUS.lines() {
        if let Some(counts) = check_line(line) {
            total.add(counts);
        }
    }

    // Guard against an empty / truncated corpus silently "passing".
    assert!(
        total.records >= 15,
        "expected the pinned corpus to carry records, saw {}",
        total.records
    );
    assert!(
        total.perft_checks >= 30,
        "too few perft checks: {}",
        total.perft_checks
    );
    assert!(
        total.move_checks >= 5,
        "too few bm/am checks: {}",
        total.move_checks
    );

    // Visible with `cargo test -- --nocapture`.
    println!(
        "EPD suite: {} records, {} perft assertions, {} bm/am assertions",
        total.records, total.perft_checks, total.move_checks
    );
}

/// Both engine paths are exercised: the corpus tags at least one `standard`
/// record and several distinct wide variants, so a regression in either
/// dispatch surfaces here rather than hiding behind the other.
#[test]
fn corpus_covers_both_engines() {
    let mut standard = 0usize;
    let mut wide_variants: Vec<&'static str> = Vec::new();
    for line in CORPUS.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let tag = line.split_whitespace().next().unwrap();
        if tag.eq_ignore_ascii_case("standard") {
            standard += 1;
        } else {
            let variant: WideVariantId = tag.parse().expect("known wide variant tag");
            wide_variants.push(variant.as_str());
        }
    }
    wide_variants.sort_unstable();
    wide_variants.dedup();

    assert!(standard >= 1, "corpus must include a standard-chess record");
    assert!(
        wide_variants.len() >= 3,
        "corpus must span several wide variants, saw {wide_variants:?}"
    );
}
