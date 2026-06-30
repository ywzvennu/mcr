//! Generic (geometry-layer) string-entry target.
//!
//! Exercises the public string entry points of the runtime fairy-variant
//! facade: [`WideVariantId::from_str`] on arbitrary text, and — when it names a
//! variant — [`AnyWideVariant::from_fen`] / [`AnyWideVariant::startpos`] on
//! arbitrary FEN text. Neither must ever panic on untrusted input; they may
//! only return `Err` / `Ok`. When a name parses, the resolved id must round-trip
//! through `as_str`, and the start position must be self-consistent
//! (`legal_moves` and `to_fen` never panic and `to_fen` re-parses).

#![no_main]

use std::str::FromStr;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use mce::geometry::{AnyWideVariant, WideVariantId};

#[derive(Arbitrary, Debug)]
struct Input<'a> {
    /// Arbitrary text fed to `WideVariantId::from_str`.
    name: &'a [u8],
    /// Arbitrary text fed to `AnyWideVariant::from_fen` once a name resolves.
    fen: &'a [u8],
}

fuzz_target!(|input: Input<'_>| {
    let Ok(name) = std::str::from_utf8(input.name) else {
        return;
    };

    // Parsing an arbitrary variant name must never panic.
    let Ok(id) = WideVariantId::from_str(name) else {
        return;
    };

    // A resolved id must round-trip through its canonical name.
    assert_eq!(
        WideVariantId::from_str(id.as_str()),
        Ok(id),
        "canonical name does not round-trip for {id}"
    );

    // The start position must be self-consistent and serialize/parse cleanly.
    let start = AnyWideVariant::startpos(id);
    let _ = start.legal_moves();
    let fen = start.to_fen();
    AnyWideVariant::from_fen(id, &fen).expect("startpos to_fen must parse back");

    // Parsing an arbitrary FEN for this variant must never panic.
    if let Ok(fen) = std::str::from_utf8(input.fen) {
        let _ = AnyWideVariant::from_fen(id, fen);
    }
});
