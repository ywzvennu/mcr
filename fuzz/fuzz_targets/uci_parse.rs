//! UCI move-parsing target.
//!
//! Feeds arbitrary input to [`Position::parse_uci`] against the standard start
//! position (and a couple of variant start positions). Parsing untrusted move
//! strings must never panic. When a move *does* parse, it must be a move the
//! position actually considers legal, and re-serializing it with `to_uci` must
//! round-trip back to the same move.

#![no_main]

use libfuzzer_sys::fuzz_target;
use mcr::{Crazyhouse, Position};

fuzz_target!(|data: &[u8]| {
    // UCI move tokens are ASCII text; non-UTF-8 input is not a UCI move.
    let Ok(uci) = std::str::from_utf8(data) else {
        return;
    };

    // Standard chess from the start position.
    let pos = Position::startpos();
    if let Ok(mv) = pos.parse_uci(uci) {
        assert!(
            pos.legal_moves().contains(&mv),
            "parse_uci accepted an illegal move: {uci:?} -> {mv:?}"
        );
        // A parsed move must serialize back to a string the parser re-accepts as
        // the same move.
        let serialized = mv.to_uci();
        let reparsed = pos
            .parse_uci(&serialized)
            .expect("to_uci output must parse back via parse_uci");
        assert_eq!(mv, reparsed, "UCI move round-trip changed the move");
    }

    // A variant with drops, to exercise the drop branch of the UCI grammar.
    let zh = Crazyhouse::startpos();
    if let Ok(mv) = zh.parse_uci(uci) {
        assert!(
            zh.legal_moves().contains(&mv),
            "variant parse_uci accepted an illegal move: {uci:?} -> {mv:?}"
        );
        let serialized = mv.to_uci();
        let reparsed = zh
            .parse_uci(&serialized)
            .expect("variant to_uci output must parse back via parse_uci");
        assert_eq!(mv, reparsed, "variant UCI move round-trip changed the move");
    }
});
