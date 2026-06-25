//! Move-generation / make-move target.
//!
//! Derives a starting position (either a fuzzed FEN, or the standard start
//! position when the FEN does not parse), then walks a fuzzed sequence of moves:
//! at each step it generates [`Position::legal_moves`], picks one by a fuzzed
//! index, and applies it with [`Position::play`]. The invariants checked at
//! every step:
//!
//! * generating and playing legal moves never panics;
//! * a played position serializes and parses back to an equal position (FEN
//!   round-trip stability on reachable positions); and
//! * the played position's Zobrist key is reproduced exactly by that FEN
//!   round-trip — i.e. `play`'s incrementally maintained state and the
//!   from-scratch key agree once routed through the serializer.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use mce::Position;

/// A fuzzed game: an optional FEN seed plus a list of move selectors. Each
/// selector is reduced modulo the number of legal moves to choose one.
#[derive(Arbitrary, Debug)]
struct Game<'a> {
    /// Bytes interpreted as a candidate FEN; if they are not valid UTF-8 or do
    /// not parse, the standard start position is used instead.
    seed_fen: &'a [u8],
    /// Move selectors driving the walk; `move_index % legal_moves.len()` picks
    /// the move to play at each step.
    selectors: Vec<u16>,
}

/// Checks the FEN + Zobrist round-trip invariant for a single position.
fn check_consistency(pos: &Position) {
    let fen = pos.to_fen();
    let reparsed = Position::from_fen(&fen)
        .expect("to_fen of a reachable position must parse back via from_fen");
    assert_eq!(*pos, reparsed, "FEN round-trip changed a reachable position");
    assert_eq!(
        pos.zobrist(),
        reparsed.zobrist(),
        "Zobrist key changed across a FEN round-trip of a played position"
    );
}

fuzz_target!(|game: Game<'_>| {
    let mut pos = std::str::from_utf8(game.seed_fen)
        .ok()
        .and_then(|s| Position::from_fen(s).ok())
        .unwrap_or_else(Position::startpos);

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
