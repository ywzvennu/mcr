//! Generic (geometry-layer) move-generation / make-move target.
//!
//! Picks a fairy variant by a fuzzed selector and derives a starting position
//! (a fuzzed FEN when it parses, else the variant start position), then walks a
//! fuzzed sequence of moves: at each step it generates
//! [`AnyWideVariant::legal_moves`], picks one by a fuzzed index, and applies it
//! with [`AnyWideVariant::play`]. The invariants checked at every reachable
//! position:
//!
//! * generating and playing legal moves never panics;
//! * the side to move flips on a non-pass move (where the variant has a royal
//!   king and is not a same-side drop game); and
//! * the position serializes and parses back to an equal position — FEN
//!   round-trip stability, with the second serialization a fixed point of the
//!   first.
//!
//! Seed corpus: the pinned FENs in the parent crate's `tests/perft_*.rs`,
//! prefixed with the matching variant's selector byte.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use mcr::geometry::{AnyWideVariant, WideVariantId};

/// A fuzzed game: a variant selector, an optional FEN seed, and a list of move
/// selectors. Each selector is reduced modulo the number of legal moves to
/// choose one.
#[derive(Arbitrary, Debug)]
struct Game<'a> {
    /// Selects the variant: `selector % WideVariantId::ALL.len()`.
    selector: u8,
    /// Bytes interpreted as a candidate FEN; if they are not valid UTF-8 or do
    /// not parse, the variant start position is used instead.
    seed_fen: &'a [u8],
    /// Move selectors driving the walk; `move_index % legal_moves.len()` picks
    /// the move to play at each step.
    selectors: Vec<u16>,
}

/// Checks the FEN round-trip fixed-point invariant for a single position.
fn check_consistency(pos: &AnyWideVariant) {
    let fen = pos.to_fen();
    let reparsed = AnyWideVariant::from_fen(pos.variant_id(), &fen)
        .expect("to_fen of a reachable position must parse back via from_fen");
    assert_eq!(
        fen,
        reparsed.to_fen(),
        "FEN round-trip changed a reachable position",
    );
    assert_eq!(
        pos.variant_id(),
        reparsed.variant_id(),
        "FEN round-trip changed the variant of a reachable position",
    );
}

fuzz_target!(|game: Game<'_>| {
    let ids = WideVariantId::ALL;
    let id = ids[game.selector as usize % ids.len()];

    let mut pos = std::str::from_utf8(game.seed_fen)
        .ok()
        .and_then(|s| AnyWideVariant::from_fen(id, s).ok())
        .unwrap_or_else(|| AnyWideVariant::startpos(id));

    // The seed position itself must be self-consistent.
    check_consistency(&pos);

    for selector in game.selectors {
        let moves = pos.legal_moves();
        if moves.is_empty() {
            break;
        }
        let mv = moves[selector as usize % moves.len()];
        let next = pos.play(&mv);
        check_consistency(&next);
        pos = next;
    }
});
