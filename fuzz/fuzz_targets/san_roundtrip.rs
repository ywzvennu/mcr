//! SAN parse / serialize round-trip target.
//!
//! Two invariants are exercised from a fuzzed position:
//!
//! * `parse_san` on arbitrary input never panics, and any move it accepts is
//!   legal; and
//! * for every legal move, the SAN produced by [`Position::san`] parses back via
//!   [`Position::parse_san`] to the same move — the serializer and parser agree
//!   on reachable positions.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use mcr::Position;

#[derive(Arbitrary, Debug)]
struct Input<'a> {
    /// Bytes interpreted as a candidate FEN; falls back to the start position.
    seed_fen: &'a [u8],
    /// Arbitrary text fed to `parse_san` to probe the parser.
    san: &'a [u8],
}

fuzz_target!(|input: Input<'_>| {
    let pos = std::str::from_utf8(input.seed_fen)
        .ok()
        .and_then(|s| Position::from_fen(s).ok())
        .unwrap_or_else(Position::startpos);

    // Arbitrary SAN text must not panic the parser; accepted moves must be legal.
    if let Ok(san) = std::str::from_utf8(input.san) {
        if let Ok(mv) = pos.parse_san(san) {
            assert!(
                pos.legal_moves().contains(&mv),
                "parse_san accepted an illegal move: {san:?} -> {mv:?}"
            );
        }
    }

    // Every legal move must round-trip through SAN.
    for mv in pos.legal_moves() {
        let san = pos.san(&mv);
        let reparsed = pos
            .parse_san(&san)
            .expect("san() output must parse back via parse_san");
        assert_eq!(mv, reparsed, "SAN round-trip changed the move: {san:?}");
    }
});
