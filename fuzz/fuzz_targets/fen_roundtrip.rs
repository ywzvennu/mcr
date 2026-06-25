//! FEN parse / serialize round-trip target.
//!
//! Interprets the fuzz input as a UTF-8 FEN string and feeds it to the standard
//! [`Position`] parser and to a selection of variant parsers. Parsing arbitrary
//! bytes must never panic; whenever a FEN *does* parse, re-serializing it with
//! `to_fen` and parsing the result back must reproduce an equal position and an
//! equal Zobrist key. This pins both the parser's tolerance and the
//! serializer's faithfulness.

#![no_main]

use libfuzzer_sys::fuzz_target;
use mce::{
    Antichess, Atomic, Chess960, Crazyhouse, Horde, KingOfTheHill, Position, RacingKings,
    ThreeCheck, VariantPosition,
};

/// Asserts the FEN round-trip invariant for any position type whose parser and
/// serializer are exposed through the same `from_fen` / `to_fen` shape.
fn check_roundtrip<P, F>(fen: &str, from_fen: F)
where
    P: PartialEq + std::fmt::Debug,
    F: Fn(&str) -> Option<(P, String, u64)>,
{
    if let Some((pos, serialized, key)) = from_fen(fen) {
        // The serializer must produce something the parser accepts again.
        let (reparsed, reserialized, key2) = from_fen(&serialized)
            .expect("to_fen output must parse back via from_fen");
        assert_eq!(pos, reparsed, "from_fen(to_fen(pos)) != pos");
        assert_eq!(
            serialized, reserialized,
            "to_fen is not idempotent across a parse round-trip"
        );
        assert_eq!(key, key2, "Zobrist key changed across a FEN round-trip");
    }
}

fuzz_target!(|data: &[u8]| {
    // FEN is text; non-UTF-8 inputs are simply not FENs.
    let Ok(fen) = std::str::from_utf8(data) else {
        return;
    };

    check_roundtrip(fen, |f| {
        Position::from_fen(f)
            .ok()
            .map(|p| (p.clone(), p.to_fen(), p.zobrist().get()))
    });

    check_roundtrip(fen, |f| {
        Chess960::from_fen(f)
            .ok()
            .map(|p| (p.clone(), p.to_fen(), p.zobrist().get()))
    });

    check_roundtrip(fen, |f| {
        VariantPosition::<mce::AtomicRules>::from_fen(f)
            .ok()
            .map(|p: Atomic| (p.clone(), p.to_fen(), p.zobrist().get()))
    });

    check_roundtrip(fen, |f| {
        VariantPosition::<mce::AntichessRules>::from_fen(f)
            .ok()
            .map(|p: Antichess| (p.clone(), p.to_fen(), p.zobrist().get()))
    });

    check_roundtrip(fen, |f| {
        VariantPosition::<mce::KingOfTheHillRules>::from_fen(f)
            .ok()
            .map(|p: KingOfTheHill| (p.clone(), p.to_fen(), p.zobrist().get()))
    });

    check_roundtrip(fen, |f| {
        VariantPosition::<mce::ThreeCheckRules>::from_fen(f)
            .ok()
            .map(|p: ThreeCheck| (p.clone(), p.to_fen(), p.zobrist().get()))
    });

    check_roundtrip(fen, |f| {
        VariantPosition::<mce::RacingKingsRules>::from_fen(f)
            .ok()
            .map(|p: RacingKings| (p.clone(), p.to_fen(), p.zobrist().get()))
    });

    check_roundtrip(fen, |f| {
        VariantPosition::<mce::HordeRules>::from_fen(f)
            .ok()
            .map(|p: Horde| (p.clone(), p.to_fen(), p.zobrist().get()))
    });

    check_roundtrip(fen, |f| {
        VariantPosition::<mce::CrazyhouseRules>::from_fen(f)
            .ok()
            .map(|p: Crazyhouse| (p.clone(), p.to_fen(), p.zobrist().get()))
    });
});
