//! Generic (geometry-layer) FEN parse / serialize round-trip target.
//!
//! Picks one of the shipped fairy variants by a fuzzed selector byte and feeds
//! the rest of the input, as a UTF-8 FEN string, to
//! [`AnyWideVariant::from_fen`]. Parsing arbitrary bytes for an arbitrary
//! variant must never panic — only return `Err`. Whenever a FEN *does* parse,
//! re-serializing it with `to_fen` and parsing the result back must succeed and
//! be a *fixed point*: the second serialization equals the first. This pins the
//! geometry board parser's tolerance (including the `*` / `**` / `=` overflow
//! role prefixes, the `+` promoted-piece prefix, and the `[..]` hand field) and
//! the serializer's idempotence on the wide layer.
//!
//! Seed corpus: the pinned FENs in the parent crate's `tests/perft_*.rs` (e.g.
//! the Xiangqi / Shogi / Capablanca start positions and their mid-game probes)
//! are good seeds; prefix each with a selector byte for the matching variant.

#![no_main]

use libfuzzer_sys::fuzz_target;
use mce::geometry::{AnyWideVariant, WideVariantId};

fuzz_target!(|data: &[u8]| {
    // The first byte selects the variant; the remainder is the candidate FEN.
    let Some((&sel, rest)) = data.split_first() else {
        return;
    };
    let ids = WideVariantId::ALL;
    let id = ids[sel as usize % ids.len()];

    // FEN is text; non-UTF-8 input is simply not a FEN.
    let Ok(fen) = std::str::from_utf8(rest) else {
        return;
    };

    let Ok(pos) = AnyWideVariant::from_fen(id, fen) else {
        return;
    };

    // The serializer must produce something the parser accepts again.
    let serialized = pos.to_fen();
    let reparsed = AnyWideVariant::from_fen(id, &serialized)
        .expect("to_fen output must parse back via from_fen");
    let reserialized = reparsed.to_fen();
    assert_eq!(
        serialized, reserialized,
        "to_fen is not idempotent across a parse round-trip for {id}"
    );
    assert_eq!(
        pos.variant_id(),
        reparsed.variant_id(),
        "variant changed across a FEN round-trip"
    );
});
