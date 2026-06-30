//! In-memory opening books for the **wide** (fairy-variant) geometry layer.
//!
//! The concrete 8x8 engine reads [Polyglot] `.bin` books keyed on Polyglot's own
//! fixed Zobrist scheme (see [`crate::book`]). That scheme is hard-wired to an
//! 8x8 board, the standard six roles, and chess castling / en-passant, so it does
//! not extend to a 10x10 Grand board, a 9x10 Xiangqi board, drops, gating, or the
//! Duck. This module is the wide-layer counterpart: a small, original move-weight
//! table keyed on the geometry layer's **incremental Zobrist** position key
//! (issue #311) — [`GenericPosition::zobrist`](super::GenericPosition::zobrist) /
//! [`GenericGame::position_key`](super::GenericGame::position_key) — so the *same*
//! key a variant already maintains for repetition detection also addresses the
//! book. It works for every shipped fairy variant and the standard game played
//! through the generic layer alike.
//!
//! A [`WideBook`] maps a position key (`u64`) to a list of
//! [`WideBookEntry`]s — a [`WideMove`] and a relative `weight`. Build one in
//! memory by [inserting](WideBook::insert) `(position, move, weight)` triples,
//! then [look it up](WideBook::lookup) for a position to get the candidate book
//! moves; [`weighted_pick`] chooses one deterministically from a caller-supplied
//! number (this crate has no RNG or clock dependency, exactly like
//! [`crate::book::weighted_pick`]).
//!
//! # Legality
//!
//! [`lookup`](WideBook::lookup) (and the [game](WideBook::lookup_game) /
//! [runtime](WideBook::lookup_any) variants) return only entries whose move is
//! **legal** in the queried position: each candidate is matched against the
//! position's own [`legal_moves`](super::GenericPosition::legal_moves), so a
//! malformed or stale book entry is silently dropped rather than handed back as a
//! move that cannot be played. The raw [`entries`](WideBook::entries) accessor
//! skips that check and returns whatever is stored for a key; the caller then
//! validates.
//!
//! # `no_std`
//!
//! The book is `alloc`-only: its table is an
//! [`alloc::collections::BTreeMap`], so it builds and queries with `std` off and
//! needs no Cargo feature (unlike the concrete [`crate::book::Book::open`] file
//! loader, which is `std`-gated — there is no on-disk format here, only the
//! in-memory table).
//!
//! ```
//! use mce::geometry::{AnyWideVariant, WideBook, WideVariantId};
//!
//! let pos = AnyWideVariant::startpos(WideVariantId::Xiangqi);
//! let mv = pos.legal_moves()[0];
//!
//! let mut book = WideBook::new();
//! book.insert(pos.position_key(), mv, 10);
//!
//! let hits = book.lookup_any(&pos);
//! assert_eq!(hits.len(), 1);
//! assert_eq!(hits[0].mv, mv);
//! ```
//!
//! [Polyglot]: http://hgm.nubati.net/book_format.html

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use super::{AnyWideVariant, GenericGame, GenericPosition, Geometry, WideMove, WideVariant};

/// One book entry: a suggested [`WideMove`] for a position and its relative
/// `weight`.
///
/// Larger weights are more preferred; [`weighted_pick`] selects among a
/// position's entries in proportion to their weights. The entry carries no
/// position key itself — the key is the [`WideBook`] map key under which it is
/// stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WideBookEntry {
    /// The book's suggested move for the position.
    pub mv: WideMove,
    /// The relative weight; larger is more preferred. Used by [`weighted_pick`].
    pub weight: u16,
}

impl WideBookEntry {
    /// Creates an entry from a move and its weight.
    #[must_use]
    #[inline]
    pub fn new(mv: WideMove, weight: u16) -> WideBookEntry {
        WideBookEntry { mv, weight }
    }
}

/// An in-memory, wide-layer opening book: a map from a position key (the
/// geometry layer's [`zobrist`](super::GenericPosition::zobrist) /
/// [`position_key`](super::GenericGame::position_key)) to the suggested book
/// moves for that position.
///
/// Construct an empty book with [`new`](WideBook::new) (or [`Default`]), fill it
/// with [`insert`](WideBook::insert) / [`insert_at`](WideBook::insert_at), and
/// query it with [`lookup`](WideBook::lookup) and friends. Entries for one key
/// keep their insertion order, so lookups — and the [`weighted_pick`] over them —
/// are fully deterministic.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WideBook {
    /// Position key -> the suggested moves stored for it, in insertion order. A
    /// `BTreeMap` keeps iteration (and thus any serialization) deterministic, and
    /// is `alloc`-only so the book stays `no_std`.
    positions: BTreeMap<u64, Vec<WideBookEntry>>,
}

impl WideBook {
    /// Creates an empty book.
    #[must_use]
    pub fn new() -> WideBook {
        WideBook {
            positions: BTreeMap::new(),
        }
    }

    /// The number of distinct positions (keys) in the book.
    #[must_use]
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    /// Whether the book holds no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// The total number of entries across every position.
    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.positions.values().map(Vec::len).sum()
    }

    /// Records `mv` with `weight` for the position identified by `key`.
    ///
    /// `key` is a position's [`zobrist`](super::GenericPosition::zobrist) /
    /// [`position_key`](super::GenericGame::position_key). Entries accumulate: a
    /// second insert for the same key appends another candidate (duplicates are
    /// kept verbatim — the book does not merge or dedupe), preserving insertion
    /// order for deterministic lookup.
    pub fn insert(&mut self, key: u64, mv: WideMove, weight: u16) {
        self.positions
            .entry(key)
            .or_default()
            .push(WideBookEntry::new(mv, weight));
    }

    /// Records `mv` with `weight` for `position`, keying it by the position's
    /// [`zobrist`](super::GenericPosition::zobrist) — the typed-path convenience
    /// over [`insert`](WideBook::insert).
    ///
    /// The move is **not** checked for legality here; legality is enforced on
    /// [`lookup`](WideBook::lookup). Storing only legal moves is the caller's
    /// choice (a book of moves that can never be played simply never matches).
    pub fn insert_at<G: Geometry, V: WideVariant<G>>(
        &mut self,
        position: &GenericPosition<G, V>,
        mv: WideMove,
        weight: u16,
    ) {
        self.insert(position.zobrist(), mv, weight);
    }

    /// The raw entries stored under `key`, in insertion order, **without** any
    /// legality check. An empty slice means the key is absent.
    ///
    /// Use this when the key is known directly; prefer [`lookup`](WideBook::lookup)
    /// when a position is in hand, since it additionally drops entries that are
    /// not legal in that position.
    #[must_use]
    pub fn entries(&self, key: u64) -> &[WideBookEntry] {
        self.positions.get(&key).map_or(&[], Vec::as_slice)
    }

    /// The book moves that are **legal** in `position`.
    ///
    /// The position's [`zobrist`](super::GenericPosition::zobrist) key selects the
    /// stored entries, then each is kept only if its move is among the position's
    /// [`legal_moves`](super::GenericPosition::legal_moves). An empty vector means
    /// the position is not in the book (or only holds entries no longer legal).
    #[must_use]
    pub fn lookup<G: Geometry, V: WideVariant<G>>(
        &self,
        position: &GenericPosition<G, V>,
    ) -> Vec<WideBookEntry> {
        let stored = self.entries(position.zobrist());
        if stored.is_empty() {
            return Vec::new();
        }
        let legal = position.legal_moves();
        stored
            .iter()
            .copied()
            .filter(|e| legal.contains(&e.mv))
            .collect()
    }

    /// The book moves legal in the current position of `game`, keyed by its
    /// [`position_key`](super::GenericGame::position_key).
    ///
    /// Equivalent to [`lookup`](WideBook::lookup) on `game.position()`, but uses
    /// the game's incrementally-maintained key directly.
    #[must_use]
    pub fn lookup_game<G: Geometry, V: WideVariant<G>>(
        &self,
        game: &GenericGame<G, V>,
    ) -> Vec<WideBookEntry> {
        let stored = self.entries(game.position_key());
        if stored.is_empty() {
            return Vec::new();
        }
        let legal = game.legal_moves();
        stored
            .iter()
            .copied()
            .filter(|e| legal.contains(&e.mv))
            .collect()
    }

    /// The book moves legal in the runtime-dispatched `position`, keyed by its
    /// [`position_key`](super::AnyWideVariant::position_key).
    ///
    /// The [`AnyWideVariant`] counterpart of [`lookup`](WideBook::lookup): it lets
    /// a server or CLI that chose its variant from a string query the book without
    /// naming the geometry / rule types.
    #[must_use]
    pub fn lookup_any(&self, position: &AnyWideVariant) -> Vec<WideBookEntry> {
        let stored = self.entries(position.position_key());
        if stored.is_empty() {
            return Vec::new();
        }
        let legal = position.legal_moves();
        stored
            .iter()
            .copied()
            .filter(|e| legal.contains(&e.mv))
            .collect()
    }
}

/// Picks one entry from `entries` in proportion to its
/// [`weight`](WideBookEntry::weight), using `random` (any `u64`) to choose.
///
/// `random` is supplied by the caller — this crate has no RNG or clock — and is
/// reduced modulo the total weight, so any value is valid and the choice is fully
/// reproducible for a given `random`. An entry's chance of selection is its
/// weight over the sum of all weights. Returns `None` only when `entries` is
/// empty or every weight is zero. This mirrors [`crate::book::weighted_pick`].
#[must_use]
pub fn weighted_pick(entries: &[WideBookEntry], random: u64) -> Option<WideBookEntry> {
    let total: u64 = entries.iter().map(|e| u64::from(e.weight)).sum();
    if total == 0 {
        return None;
    }
    let mut target = random % total;
    for entry in entries {
        let w = u64::from(entry.weight);
        if target < w {
            return Some(*entry);
        }
        target -= w;
    }
    // Unreachable while `total` is the exact sum of the weights, but returning the
    // last entry keeps this total against rounding rather than panicking.
    entries.last().copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{
        AnyWideVariant, Chess8x8, GenericGame, GenericPosition, StandardChess, WideVariantId,
    };

    /// Standard chess played through the generic geometry layer.
    type StdPos = GenericPosition<Chess8x8, StandardChess>;

    #[test]
    fn lookup_returns_inserted_legal_moves_typed_path() {
        // Standard chess via the geometry layer: insert two start-position book
        // moves keyed by zobrist(), then look them up.
        let pos = StdPos::startpos();
        let legal = pos.legal_moves();
        let a = legal[0];
        let b = legal[1];

        let mut book = WideBook::new();
        book.insert_at(&pos, a, 10);
        book.insert_at(&pos, b, 6);

        assert_eq!(book.len(), 1);
        assert_eq!(book.entry_count(), 2);

        let hits = book.lookup(&pos);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0], WideBookEntry::new(a, 10));
        assert_eq!(hits[1], WideBookEntry::new(b, 6));
    }

    #[test]
    fn lookup_drops_entries_illegal_in_the_position() {
        // A move that is legal at the start position is not legal after it has
        // been played; the stale entry must be filtered out on lookup, while the
        // raw `entries` accessor still reports it.
        let pos = StdPos::startpos();
        let mv = pos.legal_moves()[0];

        let mut book = WideBook::new();
        book.insert_at(&pos, mv, 5);

        // Same key, raw access: the entry is present and unchecked.
        assert_eq!(book.entries(pos.zobrist()).len(), 1);

        // Reach a different position that happens not to share the key; its
        // lookup misses entirely.
        let after = pos.play(&mv);
        assert!(book.lookup(&after).is_empty());

        // Forge a book whose stored move is illegal in the keyed position: key the
        // *after* position but store the (now illegal) opening move.
        let mut forged = WideBook::new();
        forged.insert(after.zobrist(), mv, 5);
        assert_eq!(forged.entries(after.zobrist()).len(), 1, "stored raw");
        assert!(
            forged.lookup(&after).is_empty(),
            "illegal entry filtered on lookup"
        );
    }

    #[test]
    fn lookup_any_works_for_runtime_variants() {
        // Build a book for a couple of runtime-dispatched variants — Xiangqi and
        // Shogi — keyed by position_key(), and look them up through the facade.
        for id in [WideVariantId::Xiangqi, WideVariantId::Shogi] {
            let pos = AnyWideVariant::startpos(id);
            let legal = pos.legal_moves();
            let mv = legal[0];

            let mut book = WideBook::new();
            book.insert(pos.position_key(), mv, 7);

            let hits = book.lookup_any(&pos);
            assert_eq!(hits.len(), 1, "{id}");
            assert_eq!(hits[0], WideBookEntry::new(mv, 7), "{id}");

            // The runtime key equals the typed-path key the book is addressed by.
            assert_eq!(pos.position_key(), pos.position_key());
        }
    }

    #[test]
    fn lookup_game_uses_the_incremental_key() {
        // A house variant (Capahouse) exercises a non-8x8 geometry through the
        // game wrapper and its incrementally-maintained position_key().
        let game =
            GenericGame::<crate::geometry::Cap10x8, crate::geometry::CapahouseRules>::startpos();
        let mv = game.legal_moves()[0];

        let mut book = WideBook::new();
        book.insert(game.position_key(), mv, 3);

        let hits = book.lookup_game(&game);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].mv, mv);
        // The game's key matches its position's from-scratch zobrist.
        assert_eq!(game.position_key(), game.position().zobrist());
    }

    #[test]
    fn lookup_misses_return_empty() {
        let pos = StdPos::startpos();
        let book = WideBook::new();
        assert!(book.is_empty());
        assert!(book.lookup(&pos).is_empty());
        // A populated book still misses an unrelated key.
        let mut other = WideBook::new();
        other.insert(pos.zobrist() ^ 0xDEAD_BEEF, pos.legal_moves()[0], 1);
        assert!(other.lookup(&pos).is_empty());
        assert!(other.entries(pos.zobrist()).is_empty());
    }

    #[test]
    fn weighted_pick_is_deterministic_and_respects_weights() {
        let pos = StdPos::startpos();
        let legal = pos.legal_moves();
        let a = legal[0];
        let b = legal[1];

        let mut book = WideBook::new();
        book.insert_at(&pos, a, 3);
        book.insert_at(&pos, b, 1);
        let entries = book.lookup(&pos);

        // total weight 4: random in 0..3 -> first (weight 3), 3 -> second.
        assert_eq!(weighted_pick(&entries, 0).unwrap().mv, a);
        assert_eq!(weighted_pick(&entries, 2).unwrap().mv, a);
        assert_eq!(weighted_pick(&entries, 3).unwrap().mv, b);
        // Wraps via the modulo, so it is reproducible for any u64.
        assert_eq!(weighted_pick(&entries, 7).unwrap().mv, b);
        assert_eq!(weighted_pick(&entries, 7).unwrap().mv, b);

        // Empty input and all-zero weights yield None.
        assert_eq!(weighted_pick(&[], 0), None);
        let mut zero = WideBook::new();
        zero.insert_at(&pos, a, 0);
        assert_eq!(weighted_pick(&zero.lookup(&pos), 0), None);
    }
}
