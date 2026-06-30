//! Generic (geometry-layer) UCI move-parsing target.
//!
//! Picks a fairy variant by a fuzzed selector, derives a position (a fuzzed FEN
//! when it parses, else the variant start position), and feeds arbitrary text
//! to [`AnyWideVariant::parse_uci`]. Parsing untrusted move strings must never
//! panic. When a move *does* parse it must be one the position considers legal,
//! and re-serializing it with `to_uci` must round-trip back to the same move.
//! Independently, every generated legal move must render to a UCI string that
//! parses back to itself — pinning the [`WideMove`](mce::geometry::WideMove)
//! UCI parser and renderer (#238) against each other across all geometries.
//!
//! Seed corpus: UCI tokens from the parent crate's `tests/` move walks (e.g.
//! `e2e4`, drops like `P@e4`, shogi promotions like `7g7f+`), each prefixed
//! with a variant selector byte and an optional `\n`-separated FEN seed.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use mce::geometry::{AnyWideVariant, WideVariantId};

#[derive(Arbitrary, Debug)]
struct Input<'a> {
    /// Selects the variant: `selector % WideVariantId::ALL.len()`.
    selector: u8,
    /// Bytes interpreted as a candidate FEN; falls back to the start position.
    seed_fen: &'a [u8],
    /// Arbitrary text fed to `parse_uci` to probe the parser.
    uci: &'a [u8],
}

fuzz_target!(|input: Input<'_>| {
    let ids = WideVariantId::ALL;
    let id = ids[input.selector as usize % ids.len()];

    let pos = std::str::from_utf8(input.seed_fen)
        .ok()
        .and_then(|s| AnyWideVariant::from_fen(id, s).ok())
        .unwrap_or_else(|| AnyWideVariant::startpos(id));

    let legal = pos.legal_moves();

    // Arbitrary UCI text must not panic the parser; accepted moves must be legal
    // and must survive a render round-trip.
    if let Ok(uci) = std::str::from_utf8(input.uci) {
        if let Some(mv) = pos.parse_uci(uci) {
            assert!(
                legal.contains(&mv),
                "parse_uci accepted an illegal move: {uci:?} -> {mv:?} ({id})"
            );
            let serialized = pos.to_uci(&mv);
            let reparsed = pos
                .parse_uci(&serialized)
                .expect("to_uci output must parse back via parse_uci");
            assert_eq!(mv, reparsed, "UCI move round-trip changed the move ({id})");
        }
    }

    // Every legal move must round-trip through UCI.
    for mv in &legal {
        let uci = pos.to_uci(mv);
        let reparsed = pos
            .parse_uci(&uci)
            .expect("to_uci output of a legal move must parse back via parse_uci");
        assert_eq!(
            *mv, reparsed,
            "legal-move UCI round-trip changed the move: {uci:?} ({id})"
        );
    }
});
