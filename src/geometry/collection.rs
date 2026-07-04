//! Wide-layer match records and multi-game PGN collections.
//!
//! Where [`WidePgn`] (issue #238) reads and writes a *single* fairy-variant game,
//! this module adds the *collection* layer that test corpora and game databases
//! need, purely additively (no move-generation or perft behaviour changes):
//!
//! - **A wide game record**, [`WideGameRecord`], a thin record view built on
//!   [`WidePgn`]: the start position, the validated move list, the result, the
//!   variant (carried as the `[Variant "..."]` tag via [`WideVariantId`]), and the
//!   header tags. Build one from a played move list with
//!   [`WideGameRecord::from_game`] or from PGN text with
//!   [`WideGameRecord::from_pgn`], then query [`moves`](WideGameRecord::moves),
//!   [`result`](WideGameRecord::result), and
//!   [`final_position`](WideGameRecord::final_position).
//! - **A multi-game PGN collection**, [`WidePgnCollection`]: parse a PGN string
//!   holding several games (split on the tag-pair boundary between games) into a
//!   `Vec` of records, iterate them, and write the whole collection back to one
//!   multi-game PGN string. A collection serialized with
//!   [`to_pgn`](WidePgnCollection::to_pgn) re-parses with
//!   [`from_pgn`](WidePgnCollection::from_pgn) to the identical records.
//!
//! ```
//! use mcr::geometry::{AnyWideVariant, WideGameRecord, WidePgnCollection, WideVariantId};
//!
//! // Two short games of different variants, played by a fixed move list.
//! fn played(id: WideVariantId, plies: usize) -> WideGameRecord {
//!     let start = AnyWideVariant::startpos(id);
//!     let mut pos = start.clone();
//!     let mut moves = Vec::new();
//!     for _ in 0..plies {
//!         let legal = pos.legal_moves();
//!         let mv = legal[0];
//!         moves.push(mv);
//!         pos = pos.play(&mv);
//!     }
//!     WideGameRecord::from_game(&start, &moves, Vec::new()).unwrap()
//! }
//!
//! let collection = WidePgnCollection::from_games(vec![
//!     played(WideVariantId::Makruk, 4),
//!     played(WideVariantId::Xiangqi, 4),
//! ]);
//! let text = collection.to_pgn();
//! let reparsed = WidePgnCollection::from_pgn(&text).unwrap();
//! assert_eq!(reparsed.len(), 2);
//! // The collection round-trips to the identical canonical text.
//! assert_eq!(reparsed.to_pgn(), text);
//! ```

use alloc::string::String;
use alloc::vec::Vec;

use super::{AnyWideVariant, WideMove, WidePgn, WidePgnError, WidePgnResult, WideVariantId};

/// A single fairy-variant match record: the start position, the validated
/// mainline of moves, the result, and the header tags, with the variant carried
/// as the `[Variant "..."]` tag.
///
/// This is a record-centric view built on [`WidePgn`] (the underlying PGN game),
/// and is the element type of a [`WidePgnCollection`]. Construct one from a
/// played move list with [`from_game`](WideGameRecord::from_game), from PGN text
/// with [`from_pgn`](WideGameRecord::from_pgn), or from an existing [`WidePgn`]
/// with [`from_pgn_game`](WideGameRecord::from_pgn_game).
#[derive(Debug, Clone)]
pub struct WideGameRecord {
    pgn: WidePgn,
}

impl WideGameRecord {
    /// Builds a record from a start position and a list of moves, validating each
    /// move in turn and recording its canonical SAN (see [`WidePgn::from_game`]).
    ///
    /// # Errors
    ///
    /// Returns [`WidePgnError::IllegalMove`] if any move is not legal in turn.
    pub fn from_game(
        start: &AnyWideVariant,
        moves: &[WideMove],
        tags: Vec<(String, String)>,
    ) -> Result<WideGameRecord, WidePgnError> {
        Ok(WideGameRecord {
            pgn: WidePgn::from_game(start, moves, tags)?,
        })
    }

    /// Parses a single game's PGN text into a record (see [`WidePgn::from_pgn`]).
    ///
    /// # Errors
    ///
    /// Returns [`WidePgnError`] for a malformed tag, an unknown variant, an
    /// invalid setup FEN, or a movetext token that names no legal move.
    pub fn from_pgn(text: &str) -> Result<WideGameRecord, WidePgnError> {
        Ok(WideGameRecord {
            pgn: WidePgn::from_pgn(text)?,
        })
    }

    /// Wraps an already-parsed [`WidePgn`] game as a record.
    #[must_use]
    pub fn from_pgn_game(pgn: WidePgn) -> WideGameRecord {
        WideGameRecord { pgn }
    }

    /// The underlying PGN game.
    #[must_use]
    pub fn pgn(&self) -> &WidePgn {
        &self.pgn
    }

    /// Consumes the record and returns its underlying [`WidePgn`].
    #[must_use]
    pub fn into_pgn(self) -> WidePgn {
        self.pgn
    }

    /// Serializes this record to single-game PGN text (see [`WidePgn::to_pgn`]).
    #[must_use]
    pub fn to_pgn(&self) -> String {
        self.pgn.to_pgn()
    }

    /// The variant this game is played under.
    #[must_use]
    pub fn variant(&self) -> WideVariantId {
        self.pgn.variant()
    }

    /// The validated mainline moves, in play order.
    #[must_use]
    pub fn moves(&self) -> &[WideMove] {
        self.pgn.moves()
    }

    /// The recorded SAN strings, parallel to [`moves`](WideGameRecord::moves).
    #[must_use]
    pub fn sans(&self) -> &[String] {
        self.pgn.sans()
    }

    /// The game result.
    #[must_use]
    pub fn result(&self) -> WidePgnResult {
        self.pgn.result()
    }

    /// The extra header tag pairs, in stored order.
    #[must_use]
    pub fn tags(&self) -> &[(String, String)] {
        self.pgn.tags()
    }

    /// The start position (the `[FEN ...]` setup, or the variant start).
    #[must_use]
    pub fn start_position(&self) -> AnyWideVariant {
        self.pgn.start_position()
    }

    /// The final position reached after replaying the whole mainline.
    #[must_use]
    pub fn final_position(&self) -> AnyWideVariant {
        self.pgn.final_position()
    }
}

/// A collection of fairy-variant games read from, or written to, a single
/// multi-game PGN string.
///
/// PGN concatenates games back-to-back, each beginning with its own tag-pair
/// header; [`from_pgn`](WidePgnCollection::from_pgn) splits a string on that
/// boundary (a `[`-led tag line that follows a game's movetext) and parses each
/// game into a [`WideGameRecord`], and [`to_pgn`](WidePgnCollection::to_pgn)
/// writes the records back out separated by a blank line. A collection
/// round-trips: `from_pgn(c.to_pgn())` reproduces `c`.
#[derive(Debug, Clone, Default)]
pub struct WidePgnCollection {
    games: Vec<WideGameRecord>,
}

impl WidePgnCollection {
    /// An empty collection.
    #[must_use]
    pub fn new() -> WidePgnCollection {
        WidePgnCollection { games: Vec::new() }
    }

    /// Wraps a vector of records as a collection.
    #[must_use]
    pub fn from_games(games: Vec<WideGameRecord>) -> WidePgnCollection {
        WidePgnCollection { games }
    }

    /// Parses a multi-game PGN string into a collection of records.
    ///
    /// Games are split on the tag-pair boundary — a `[`-led header line that
    /// follows a previous game's movetext — and each game is parsed with
    /// [`WideGameRecord::from_pgn`]. Blank lines between games are ignored.
    ///
    /// # Errors
    ///
    /// Returns the first [`WidePgnError`] encountered while parsing a game.
    pub fn from_pgn(text: &str) -> Result<WidePgnCollection, WidePgnError> {
        let mut games = Vec::new();
        for chunk in split_games(text) {
            games.push(WideGameRecord::from_pgn(chunk)?);
        }
        Ok(WidePgnCollection { games })
    }

    /// Serializes every game to one multi-game PGN string, separating successive
    /// games with a blank line. The output re-parses with
    /// [`from_pgn`](WidePgnCollection::from_pgn) to an equal collection.
    #[must_use]
    pub fn to_pgn(&self) -> String {
        let mut out = String::new();
        for (i, game) in self.games.iter().enumerate() {
            if i > 0 {
                // Each game's `to_pgn` ends in a newline; one more yields the
                // blank line that separates games in a PGN file.
                out.push('\n');
            }
            out.push_str(&game.to_pgn());
        }
        out
    }

    /// The records in the collection, in order.
    #[must_use]
    pub fn games(&self) -> &[WideGameRecord] {
        &self.games
    }

    /// The number of games.
    #[must_use]
    pub fn len(&self) -> usize {
        self.games.len()
    }

    /// Whether the collection holds no games.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.games.is_empty()
    }

    /// Appends a record to the collection.
    pub fn push(&mut self, game: WideGameRecord) {
        self.games.push(game);
    }

    /// An iterator over the records, in order.
    pub fn iter(&self) -> core::slice::Iter<'_, WideGameRecord> {
        self.games.iter()
    }

    /// Consumes the collection and returns its records.
    #[must_use]
    pub fn into_games(self) -> Vec<WideGameRecord> {
        self.games
    }
}

impl IntoIterator for WidePgnCollection {
    type Item = WideGameRecord;
    type IntoIter = alloc::vec::IntoIter<WideGameRecord>;

    fn into_iter(self) -> Self::IntoIter {
        self.games.into_iter()
    }
}

impl<'a> IntoIterator for &'a WidePgnCollection {
    type Item = &'a WideGameRecord;
    type IntoIter = core::slice::Iter<'a, WideGameRecord>;

    fn into_iter(self) -> Self::IntoIter {
        self.games.iter()
    }
}

impl FromIterator<WideGameRecord> for WidePgnCollection {
    fn from_iter<T: IntoIterator<Item = WideGameRecord>>(iter: T) -> WidePgnCollection {
        WidePgnCollection {
            games: iter.into_iter().collect(),
        }
    }
}

/// Splits a multi-game PGN string into one slice per game.
///
/// A new game begins at a `[`-led tag line that follows movetext already seen in
/// the current game (the standard PGN inter-game boundary). Leading and trailing
/// whitespace around each game slice is trimmed; empty slices are dropped.
fn split_games(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut games = Vec::new();
    let mut start = 0usize;
    let mut line_start = 0usize;
    let mut seen_movetext = false;
    let mut i = 0usize;

    while i <= bytes.len() {
        let at_end = i == bytes.len();
        if at_end || bytes[i] == b'\n' {
            let line = text[line_start..i].trim();
            if !line.is_empty() {
                if line.starts_with('[') {
                    if seen_movetext {
                        let chunk = text[start..line_start].trim();
                        if !chunk.is_empty() {
                            games.push(chunk);
                        }
                        start = line_start;
                        seen_movetext = false;
                    }
                } else {
                    seen_movetext = true;
                }
            }
            line_start = i + 1;
        }
        i += 1;
    }

    let chunk = text[start..].trim();
    if !chunk.is_empty() {
        games.push(chunk);
    }
    games
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    /// Plays the first legal move for `plies` plies and builds a record.
    fn played(id: WideVariantId, plies: usize) -> WideGameRecord {
        let start = AnyWideVariant::startpos(id);
        let mut pos = start.clone();
        let mut moves = Vec::new();
        for _ in 0..plies {
            let legal = pos.legal_moves();
            if legal.is_empty() {
                break;
            }
            let mv = legal[0];
            moves.push(mv);
            pos = pos.play(&mv);
        }
        WideGameRecord::from_game(&start, &moves, vec![("Event".into(), "Corpus".into())]).unwrap()
    }

    #[test]
    fn multi_game_corpus_round_trips_canonically() {
        // A pinned multi-game corpus spanning three variants and two geometries:
        // 8x8 (makruk, shatranj) and 9x10 (xiangqi).
        let collection = WidePgnCollection::from_games(vec![
            played(WideVariantId::Makruk, 5),
            played(WideVariantId::Shatranj, 4),
            played(WideVariantId::Xiangqi, 4),
        ]);
        let corpus = collection.to_pgn();

        let reparsed = WidePgnCollection::from_pgn(&corpus).unwrap();
        assert_eq!(reparsed.len(), 3);
        // Parsed -> records -> re-written equals the canonical original.
        assert_eq!(reparsed.to_pgn(), corpus);

        // Each game preserves its variant and SAN movetext across the trip.
        for (orig, back) in collection.games().iter().zip(reparsed.games()) {
            assert_eq!(orig.variant(), back.variant());
            assert_eq!(orig.sans(), back.sans());
            assert_eq!(orig.moves().len(), back.moves().len());
        }
    }

    #[test]
    fn parses_authored_two_game_pgn() {
        // Hand-authored multi-game PGN with the two games separated by a blank
        // line, each with its own tag header.
        let text = "[Variant \"makruk\"]\n[Result \"*\"]\n\n1. Kc2 Kd7 2. Kd1 *\n\n\
                    [Variant \"shatranj\"]\n[Result \"*\"]\n\n1. Na3 Na6 2. Nf3 *\n";
        let collection = WidePgnCollection::from_pgn(text).unwrap();
        assert_eq!(collection.len(), 2);
        assert_eq!(collection.games()[0].variant(), WideVariantId::Makruk);
        assert_eq!(collection.games()[1].variant(), WideVariantId::Shatranj);
        assert_eq!(collection.games()[0].sans(), &["Kc2", "Kd7", "Kd1"]);
        assert_eq!(collection.games()[1].sans(), &["Na3", "Na6", "Nf3"]);

        // Re-serializing then re-parsing is stable (canonical).
        let canonical = collection.to_pgn();
        let again = WidePgnCollection::from_pgn(&canonical).unwrap();
        assert_eq!(again.to_pgn(), canonical);
    }

    #[test]
    fn record_replays_to_expected_final_position() {
        let start = AnyWideVariant::startpos(WideVariantId::Makruk);
        let mut pos = start.clone();
        let mut moves = Vec::new();
        for _ in 0..4 {
            let mv = pos.legal_moves()[0];
            moves.push(mv);
            pos = pos.play(&mv);
        }
        let expected_fen = pos.to_fen();

        let record = WideGameRecord::from_game(&start, &moves, Vec::new()).unwrap();
        assert_eq!(record.moves().len(), 4);
        assert_eq!(record.final_position().to_fen(), expected_fen);
    }

    #[test]
    fn empty_input_is_an_empty_collection() {
        let collection = WidePgnCollection::from_pgn("").unwrap();
        assert!(collection.is_empty());
        assert_eq!(collection.to_pgn(), "");
    }

    #[test]
    fn collection_iterators_and_from_iter() {
        let games = vec![
            played(WideVariantId::Makruk, 3),
            played(WideVariantId::Shatranj, 3),
        ];
        let collection: WidePgnCollection = games.into_iter().collect();
        assert_eq!(collection.len(), 2);
        let by_ref: Vec<WideVariantId> = (&collection).into_iter().map(|g| g.variant()).collect();
        assert_eq!(by_ref, vec![WideVariantId::Makruk, WideVariantId::Shatranj]);
        let owned: Vec<WideVariantId> = collection.into_iter().map(|g| g.variant()).collect();
        assert_eq!(owned, vec![WideVariantId::Makruk, WideVariantId::Shatranj]);
    }
}
