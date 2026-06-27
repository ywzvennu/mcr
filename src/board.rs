//! Piece placement on the 64 squares of a chessboard.
//!
//! A [`Board`] records *only* which piece (if any) sits on each square. It is
//! the placement half of a full game position — there is no side-to-move,
//! castling rights, en-passant target, or move clock here. Those belong to the
//! richer position type built on top of this one.
//!
//! Placement is stored as the usual "by-color plus by-role" set of
//! [`Bitboard`]s: two color masks and six role masks. A square is occupied by a
//! piece exactly when it appears in one color mask and one role mask, and the
//! union of the color masks is the set of occupied squares. All mutators keep
//! these masks in agreement.

#[cfg(test)]
use alloc::format;
use alloc::string::String;
#[cfg(test)]
use alloc::{string::ToString, vec::Vec};
use core::fmt;
use core::str::FromStr;

use crate::{Bitboard, Color, Piece, Role, Square};

/// The number of distinct colors.
const COLOR_COUNT: usize = 2;
/// The number of distinct roles.
const ROLE_COUNT: usize = 6;

/// Returns the array index used for a color's occupancy mask.
#[inline]
const fn color_index(color: Color) -> usize {
    match color {
        Color::White => 0,
        Color::Black => 1,
    }
}

/// Returns the array index used for a role's occupancy mask.
#[inline]
const fn role_index(role: Role) -> usize {
    match role {
        Role::Pawn => 0,
        Role::Knight => 1,
        Role::Bishop => 2,
        Role::Rook => 3,
        Role::Queen => 4,
        Role::King => 5,
    }
}

/// Maps a single FEN piece byte to its [`Piece`], matching [`Piece::from_char`]
/// for ASCII input: uppercase letters are white, lowercase are black. Returns
/// `None` for any byte that is not a piece letter, letting the placement parser
/// stay on the byte fast path while preserving the exact accept/reject set.
#[inline]
fn piece_from_ascii(b: u8) -> Option<Piece> {
    let (color, role) = match b {
        b'P' => (Color::White, Role::Pawn),
        b'N' => (Color::White, Role::Knight),
        b'B' => (Color::White, Role::Bishop),
        b'R' => (Color::White, Role::Rook),
        b'Q' => (Color::White, Role::Queen),
        b'K' => (Color::White, Role::King),
        b'p' => (Color::Black, Role::Pawn),
        b'n' => (Color::Black, Role::Knight),
        b'b' => (Color::Black, Role::Bishop),
        b'r' => (Color::Black, Role::Rook),
        b'q' => (Color::Black, Role::Queen),
        b'k' => (Color::Black, Role::King),
        _ => return None,
    };
    Some(Piece::new(color, role))
}

/// The piece placement of a chess board: which [`Piece`], if any, occupies each
/// of the 64 squares.
///
/// This is purely the board layout; it carries no side-to-move, castling, en
/// passant, or clock information. Two boards are equal when the same piece sits
/// on every square.
///
/// ```
/// use mce::{Board, Color, Piece, Role, Square};
///
/// let board = Board::standard();
/// assert_eq!(
///     board.piece_at(Square::E1),
///     Some(Piece::new(Color::White, Role::King)),
/// );
/// assert_eq!(board.king_of(Color::Black), Some(Square::E8));
/// assert_eq!(board.occupied().count(), 32);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Board {
    /// Occupancy mask per color, indexed by [`color_index`].
    by_color: [Bitboard; COLOR_COUNT],
    /// Occupancy mask per role, indexed by [`role_index`].
    by_role: [Bitboard; ROLE_COUNT],
}

impl Board {
    /// Creates a board with no pieces on it.
    ///
    /// ```
    /// use mce::Board;
    /// assert!(Board::empty().occupied().is_empty());
    /// ```
    #[must_use]
    #[inline]
    pub const fn empty() -> Board {
        Board {
            by_color: [Bitboard::EMPTY; COLOR_COUNT],
            by_role: [Bitboard::EMPTY; ROLE_COUNT],
        }
    }

    /// Creates a board in the standard chess starting placement.
    ///
    /// This is the placement of the conventional initial position:
    /// `rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR`.
    ///
    /// ```
    /// use mce::Board;
    /// assert_eq!(
    ///     Board::standard().to_fen_placement(),
    ///     "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR",
    /// );
    /// ```
    #[must_use]
    pub fn standard() -> Board {
        // Back-rank role order from the a-file to the h-file.
        const BACK_RANK: [Role; 8] = [
            Role::Rook,
            Role::Knight,
            Role::Bishop,
            Role::Queen,
            Role::King,
            Role::Bishop,
            Role::Knight,
            Role::Rook,
        ];

        let mut board = Board::empty();
        for file in 0..8u8 {
            let role = BACK_RANK[file as usize];
            // White back rank on rank 1, pawns on rank 2.
            board.set_piece(Square::new(file), Piece::new(Color::White, role));
            board.set_piece(Square::new(8 + file), Piece::new(Color::White, Role::Pawn));
            // Black pawns on rank 7, back rank on rank 8.
            board.set_piece(Square::new(48 + file), Piece::new(Color::Black, Role::Pawn));
            board.set_piece(Square::new(56 + file), Piece::new(Color::Black, role));
        }
        board
    }

    /// Alias for [`Board::standard`], the conventional starting placement.
    #[must_use]
    #[inline]
    pub fn new() -> Board {
        Board::standard()
    }

    /// Returns the set of all occupied squares.
    #[must_use]
    #[inline]
    pub const fn occupied(self) -> Bitboard {
        Bitboard(self.by_color[0].0 | self.by_color[1].0)
    }

    /// Returns `true` if the given square holds a piece.
    #[must_use]
    #[inline]
    pub const fn is_occupied(self, square: Square) -> bool {
        self.occupied().contains(square)
    }

    /// Returns the occupancy mask for one color.
    #[must_use]
    #[inline]
    pub const fn by_color(self, color: Color) -> Bitboard {
        self.by_color[color_index(color)]
    }

    /// Returns the occupancy mask for one role, across both colors.
    #[must_use]
    #[inline]
    pub const fn by_role(self, role: Role) -> Bitboard {
        self.by_role[role_index(role)]
    }

    /// Returns the squares occupied by the given piece (a specific color and
    /// role).
    ///
    /// ```
    /// use mce::{Board, Color, Piece, Role};
    /// let board = Board::standard();
    /// let white_pawns = board.by_piece(Piece::new(Color::White, Role::Pawn));
    /// assert_eq!(white_pawns.count(), 8);
    /// ```
    #[must_use]
    #[inline]
    pub const fn by_piece(self, piece: Piece) -> Bitboard {
        Bitboard(self.by_color(piece.color).0 & self.by_role(piece.role).0)
    }

    /// Alias for [`Board::by_piece`], naming the color and role separately.
    #[must_use]
    #[inline]
    pub const fn pieces(self, color: Color, role: Role) -> Bitboard {
        self.by_piece(Piece::new(color, role))
    }

    /// Returns the color of the piece on the given square, or `None` if empty.
    #[must_use]
    #[inline]
    pub fn color_at(self, square: Square) -> Option<Color> {
        Color::ALL
            .into_iter()
            .find(|&color| self.by_color(color).contains(square))
    }

    /// Returns the role of the piece on the given square, or `None` if empty.
    #[must_use]
    #[inline]
    pub fn role_at(self, square: Square) -> Option<Role> {
        Role::ALL
            .into_iter()
            .find(|&role| self.by_role(role).contains(square))
    }

    /// Returns the piece on the given square, or `None` if it is empty.
    #[must_use]
    #[inline]
    pub fn piece_at(self, square: Square) -> Option<Piece> {
        let color = self.color_at(square)?;
        let role = self.role_at(square)?;
        Some(Piece::new(color, role))
    }

    /// Returns the square the given color's king stands on, if there is one.
    ///
    /// If a color somehow has more than one king (which never happens for a
    /// legally derived board) the lowest-indexed square is returned.
    #[must_use]
    #[inline]
    pub fn king_of(self, color: Color) -> Option<Square> {
        self.pieces(color, Role::King).lsb()
    }

    /// Places `piece` on `square`, replacing whatever was there.
    ///
    /// All occupancy masks are updated together so the board stays consistent.
    ///
    /// ```
    /// use mce::{Board, Color, Piece, Role, Square};
    /// let mut board = Board::empty();
    /// board.set_piece(Square::D4, Piece::new(Color::White, Role::Queen));
    /// assert_eq!(
    ///     board.piece_at(Square::D4),
    ///     Some(Piece::new(Color::White, Role::Queen)),
    /// );
    /// ```
    #[inline]
    pub fn set_piece(&mut self, square: Square, piece: Piece) {
        // Clear any existing occupant first so the masks never disagree.
        self.remove_piece(square);
        self.by_color[color_index(piece.color)].set(square);
        self.by_role[role_index(piece.role)].set(square);
    }

    /// Removes any piece from `square`, returning what was there (if anything).
    ///
    /// ```
    /// use mce::{Board, Color, Piece, Role, Square};
    /// let mut board = Board::standard();
    /// assert_eq!(
    ///     board.remove_piece(Square::E1),
    ///     Some(Piece::new(Color::White, Role::King)),
    /// );
    /// assert!(board.piece_at(Square::E1).is_none());
    /// ```
    #[inline]
    pub fn remove_piece(&mut self, square: Square) -> Option<Piece> {
        let piece = self.piece_at(square)?;
        self.by_color[color_index(piece.color)].clear(square);
        self.by_role[role_index(piece.role)].clear(square);
        Some(piece)
    }

    /// Removes any piece from `square` without reporting what was removed.
    ///
    /// A convenience for callers that do not care about the previous occupant.
    #[inline]
    pub fn discard(&mut self, square: Square) {
        let _ = self.remove_piece(square);
    }

    /// Parses a board from a FEN piece-placement field — the first
    /// space-separated token of a full FEN string.
    ///
    /// The field lists the eight ranks from rank 8 down to rank 1, separated by
    /// `/`. Within a rank, a piece letter (see [`Piece::from_char`]) places one
    /// piece and a digit `1..=8` skips that many empty squares, walking from the
    /// a-file to the h-file.
    ///
    /// ```
    /// use mce::Board;
    /// let board = Board::from_fen_placement("8/8/8/8/8/8/8/8").unwrap();
    /// assert!(board.occupied().is_empty());
    /// ```
    ///
    /// # Errors
    ///
    /// Returns a [`ParseBoardError`] if the field does not have exactly eight
    /// ranks, a rank does not describe exactly eight files, or a character is
    /// neither a piece letter nor a digit `1..=8`.
    pub fn from_fen_placement(placement: &str) -> Result<Board, ParseBoardError> {
        let mut board = Board::empty();
        let mut ranks = placement.split('/');

        // Ranks are listed from 8 down to 1.
        for rank_from_top in 0..8u8 {
            let rank_str = ranks.next().ok_or(ParseBoardError::TooFewRanks)?;
            let rank = 7 - rank_from_top;

            // Accumulate in `usize` so an adversarial digit run (e.g. a long
            // string of `9`s) can never overflow the counter and panic. The
            // running total is reported back through `RankWrongWidth`, saturated
            // into the `u8` field so even an enormous overrun stays well-defined.
            let mut file: usize = 0;
            let bytes = rank_str.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                let b = bytes[i];
                if b.is_ascii_digit() {
                    if b == b'0' {
                        return Err(ParseBoardError::InvalidChar('0'));
                    }
                    file += (b - b'0') as usize;
                    i += 1;
                } else if let Some(piece) = piece_from_ascii(b) {
                    if file >= 8 {
                        return Err(ParseBoardError::RankTooLong(rank + 1));
                    }
                    board.set_piece(Square::new(rank * 8 + file as u8), piece);
                    file += 1;
                    i += 1;
                } else {
                    // Not a digit or piece letter: report the offending character
                    // exactly as the char-based parser did. ASCII bytes are a
                    // single char; otherwise decode the UTF-8 codepoint starting
                    // here so a multibyte char is reported intact (preserving the
                    // non-ASCII handling fixed in #44/#47).
                    let ch = rank_str[i..]
                        .chars()
                        .next()
                        .unwrap_or(char::REPLACEMENT_CHARACTER);
                    return Err(ParseBoardError::InvalidChar(ch));
                }
            }

            if file != 8 {
                return Err(ParseBoardError::RankWrongWidth {
                    rank: rank + 1,
                    files: u8::try_from(file).unwrap_or(u8::MAX),
                });
            }
        }

        if ranks.next().is_some() {
            return Err(ParseBoardError::TooManyRanks);
        }

        Ok(board)
    }

    /// Serializes the placement as a FEN piece-placement field.
    ///
    /// This is the inverse of [`Board::from_fen_placement`]: ranks run from 8
    /// down to 1, runs of empty squares collapse into a digit, and pieces use
    /// their FEN letters.
    ///
    /// ```
    /// use mce::Board;
    /// assert_eq!(
    ///     Board::standard().to_fen_placement(),
    ///     "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR",
    /// );
    /// ```
    #[must_use]
    pub fn to_fen_placement(self) -> String {
        // Each rank is at most eight cells; eight ranks plus seven separators.
        let mut fen = String::with_capacity(8 * 8 + 7);
        self.write_fen_placement(&mut fen);
        fen
    }

    /// Appends the FEN piece-placement field to `out` without allocating an
    /// intermediate `String`, so callers serializing a full FEN can write
    /// straight into their output buffer. See [`Board::to_fen_placement`].
    pub(crate) fn write_fen_placement(self, out: &mut String) {
        for rank in (0..8u8).rev() {
            let mut empty = 0u8;
            for file in 0..8u8 {
                match self.piece_at(Square::new(rank * 8 + file)) {
                    Some(piece) => {
                        if empty > 0 {
                            out.push((b'0' + empty) as char);
                            empty = 0;
                        }
                        out.push(piece.char());
                    }
                    None => empty += 1,
                }
            }
            if empty > 0 {
                out.push((b'0' + empty) as char);
            }
            if rank > 0 {
                out.push('/');
            }
        }
    }
}

impl Default for Board {
    /// The standard chess starting placement; see [`Board::standard`].
    #[inline]
    fn default() -> Board {
        Board::standard()
    }
}

impl fmt::Display for Board {
    /// Renders the board as eight rows of eight cells, rank 8 at the top, using a
    /// piece's FEN letter for occupied squares and `.` for empty ones.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for rank in (0..8u8).rev() {
            for file in 0..8u8 {
                let square = Square::new(rank * 8 + file);
                match self.piece_at(square) {
                    Some(piece) => f.write_fmt(format_args!("{}", piece.char()))?,
                    None => f.write_str(".")?,
                }
                if file < 7 {
                    f.write_str(" ")?;
                }
            }
            if rank > 0 {
                f.write_str("\n")?;
            }
        }
        Ok(())
    }
}

impl FromStr for Board {
    type Err = ParseBoardError;

    /// Parses the FEN piece-placement field; see [`Board::from_fen_placement`].
    fn from_str(s: &str) -> Result<Board, ParseBoardError> {
        Board::from_fen_placement(s)
    }
}

/// The error returned when a FEN piece-placement field cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ParseBoardError {
    /// The placement field had fewer than eight ranks.
    TooFewRanks,
    /// The placement field had more than eight ranks.
    TooManyRanks,
    /// A rank described a number of files other than eight. `rank` is the
    /// human-readable rank number (`1..=8`) and `files` is how many it covered.
    RankWrongWidth {
        /// The rank number (`1..=8`) whose width was wrong.
        rank: u8,
        /// The number of files the rank actually described.
        files: u8,
    },
    /// A rank tried to place a piece past the h-file. `0` is the rank number.
    RankTooLong(u8),
    /// An unexpected character appeared in the field.
    InvalidChar(char),
}

impl fmt::Display for ParseBoardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseBoardError::TooFewRanks => f.write_str("FEN placement has fewer than 8 ranks"),
            ParseBoardError::TooManyRanks => f.write_str("FEN placement has more than 8 ranks"),
            ParseBoardError::RankWrongWidth { rank, files } => write!(
                f,
                "rank {rank} of FEN placement covers {files} files, expected 8",
            ),
            ParseBoardError::RankTooLong(rank) => {
                write!(f, "rank {rank} of FEN placement extends past the h-file")
            }
            ParseBoardError::InvalidChar(ch) => {
                write!(f, "invalid character {ch:?} in FEN placement")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ParseBoardError {}

#[cfg(test)]
mod tests {
    use super::*;

    const STANDARD_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";
    const EMPTY_FEN: &str = "8/8/8/8/8/8/8/8";
    // Kiwipete: a well-known perft test position's placement field.
    const KIWIPETE_FEN: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R";

    #[test]
    fn empty_board_has_nothing() {
        let board = Board::empty();
        assert!(board.occupied().is_empty());
        for index in 0..64u8 {
            assert_eq!(board.piece_at(Square::new(index)), None);
        }
        assert_eq!(board.king_of(Color::White), None);
        assert_eq!(board.king_of(Color::Black), None);
    }

    #[test]
    fn standard_placement_round_trips() {
        let board = Board::standard();
        assert_eq!(board.to_fen_placement(), STANDARD_FEN);
        let parsed = Board::from_fen_placement(STANDARD_FEN).unwrap();
        assert_eq!(parsed, board);
        // Default is the standard placement.
        assert_eq!(Board::default(), board);
        assert_eq!(Board::new(), board);
    }

    #[test]
    fn standard_piece_positions() {
        let board = Board::standard();
        assert_eq!(
            board.piece_at(Square::A1),
            Some(Piece::new(Color::White, Role::Rook)),
        );
        assert_eq!(
            board.piece_at(Square::E1),
            Some(Piece::new(Color::White, Role::King)),
        );
        assert_eq!(
            board.piece_at(Square::D1),
            Some(Piece::new(Color::White, Role::Queen)),
        );
        assert_eq!(
            board.piece_at(Square::E8),
            Some(Piece::new(Color::Black, Role::King)),
        );
        assert_eq!(
            board.piece_at(Square::A7),
            Some(Piece::new(Color::Black, Role::Pawn)),
        );
        assert_eq!(board.piece_at(Square::E4), None);

        assert_eq!(board.king_of(Color::White), Some(Square::E1));
        assert_eq!(board.king_of(Color::Black), Some(Square::E8));
    }

    #[test]
    fn standard_masks() {
        let board = Board::standard();
        assert_eq!(board.occupied().count(), 32);
        assert_eq!(board.by_color(Color::White).count(), 16);
        assert_eq!(board.by_color(Color::Black).count(), 16);
        assert_eq!(board.by_role(Role::Pawn).count(), 16);
        assert_eq!(board.by_role(Role::King).count(), 2);
        assert_eq!(board.pieces(Color::White, Role::Pawn).count(), 8);
        assert_eq!(
            board
                .by_piece(Piece::new(Color::Black, Role::Knight))
                .count(),
            2,
        );
        // By-color and by-role masks agree with occupied().
        let union = board.by_color(Color::White) | board.by_color(Color::Black);
        assert_eq!(union, board.occupied());
    }

    #[test]
    fn color_and_role_at() {
        let board = Board::standard();
        assert_eq!(board.color_at(Square::A1), Some(Color::White));
        assert_eq!(board.role_at(Square::A1), Some(Role::Rook));
        assert_eq!(board.color_at(Square::A8), Some(Color::Black));
        assert_eq!(board.color_at(Square::E4), None);
        assert_eq!(board.role_at(Square::E4), None);
        assert!(board.is_occupied(Square::A1));
        assert!(!board.is_occupied(Square::E4));
    }

    #[test]
    fn set_remove_keep_masks_consistent() {
        let mut board = Board::empty();
        let queen = Piece::new(Color::White, Role::Queen);
        board.set_piece(Square::D4, queen);
        assert_eq!(board.piece_at(Square::D4), Some(queen));
        assert_eq!(board.occupied().count(), 1);
        assert_eq!(board.by_color(Color::White).count(), 1);
        assert_eq!(board.by_role(Role::Queen).count(), 1);

        // Overwriting replaces the occupant and leaves no stale bits.
        let rook = Piece::new(Color::Black, Role::Rook);
        board.set_piece(Square::D4, rook);
        assert_eq!(board.piece_at(Square::D4), Some(rook));
        assert_eq!(board.occupied().count(), 1);
        assert_eq!(board.by_color(Color::White).count(), 0);
        assert_eq!(board.by_role(Role::Queen).count(), 0);
        assert_eq!(board.by_color(Color::Black).count(), 1);
        assert_eq!(board.by_role(Role::Rook).count(), 1);

        assert_eq!(board.remove_piece(Square::D4), Some(rook));
        assert_eq!(board.remove_piece(Square::D4), None);
        assert!(board.occupied().is_empty());

        // discard is the value-dropping form of remove_piece.
        board.set_piece(Square::H8, queen);
        board.discard(Square::H8);
        assert!(board.occupied().is_empty());
    }

    #[test]
    fn fen_round_trips_several_positions() {
        for fen in [STANDARD_FEN, EMPTY_FEN, KIWIPETE_FEN] {
            let board = Board::from_fen_placement(fen).unwrap();
            assert_eq!(board.to_fen_placement(), fen);
            // FromStr is the same path.
            assert_eq!(fen.parse::<Board>().unwrap(), board);
        }
    }

    #[test]
    fn kiwipete_spot_checks() {
        let board = Board::from_fen_placement(KIWIPETE_FEN).unwrap();
        assert_eq!(board.king_of(Color::White), Some(Square::E1));
        assert_eq!(board.king_of(Color::Black), Some(Square::E8));
        // A black knight sits on b6.
        assert_eq!(
            board.piece_at(Square::B6),
            Some(Piece::new(Color::Black, Role::Knight)),
        );
        // The advanced white pawn is on d5.
        assert_eq!(
            board.piece_at(Square::D5),
            Some(Piece::new(Color::White, Role::Pawn)),
        );
    }

    #[test]
    fn rejects_too_few_ranks() {
        let err = Board::from_fen_placement("8/8/8/8/8/8/8").unwrap_err();
        assert_eq!(err, ParseBoardError::TooFewRanks);
    }

    #[test]
    fn rejects_too_many_ranks() {
        let err = Board::from_fen_placement("8/8/8/8/8/8/8/8/8").unwrap_err();
        assert_eq!(err, ParseBoardError::TooManyRanks);
    }

    #[test]
    fn rejects_rank_wrong_width() {
        // Rank 8 covers only 7 files.
        let err = Board::from_fen_placement("7/8/8/8/8/8/8/8").unwrap_err();
        assert_eq!(err, ParseBoardError::RankWrongWidth { rank: 8, files: 7 },);
        // Rank 8 covers 9 files via digits.
        let err = Board::from_fen_placement("9/8/8/8/8/8/8/8").unwrap_err();
        // '9' is not a valid 1..=8 digit run? It is a digit but skips 9 > 8.
        // The width check catches it once the rank overruns.
        assert_eq!(err, ParseBoardError::RankWrongWidth { rank: 8, files: 9 },);
    }

    #[test]
    fn rejects_rank_too_long_with_pieces() {
        // Eight pawns then another piece overruns the h-file.
        let err = Board::from_fen_placement("ppppppppp/8/8/8/8/8/8/8").unwrap_err();
        assert_eq!(err, ParseBoardError::RankTooLong(8));
    }

    #[test]
    fn rejects_invalid_char() {
        let err = Board::from_fen_placement("xnbqkbnr/8/8/8/8/8/8/8").unwrap_err();
        assert_eq!(err, ParseBoardError::InvalidChar('x'));
        // A zero digit is not allowed.
        let err = Board::from_fen_placement("08/8/8/8/8/8/8/8").unwrap_err();
        assert_eq!(err, ParseBoardError::InvalidChar('0'));
        // A non-ASCII (multibyte) character must be reported as its full
        // codepoint, not a lone UTF-8 byte: the byte-level scanner decodes the
        // offending char before reporting it.
        let err = Board::from_fen_placement("é7/8/8/8/8/8/8/8").unwrap_err();
        assert_eq!(err, ParseBoardError::InvalidChar('é'));
    }

    #[test]
    fn rejects_oversized_skip_run_without_panicking() {
        // Regression for issue #47: a rank built from a run of digits must not
        // overflow the `u8` file counter (which panicked in debug builds via
        // `file += skip`). Each of these returns `Err`, never panics.

        // The exact case named in the issue: eight nines (sum 72).
        let err = Board::from_fen_placement("99999999/8/8/8/8/8/8/8").unwrap_err();
        assert_eq!(err, ParseBoardError::RankWrongWidth { rank: 8, files: 72 });

        // "71" itself sums to exactly 8 (a valid empty rank); appending a piece
        // overruns the h-file and is rejected.
        let err = Board::from_fen_placement("71p/8/8/8/8/8/8/8").unwrap_err();
        assert_eq!(err, ParseBoardError::RankTooLong(8));

        // A digit run long enough to overflow a `u8` if accumulated naively
        // (29 nines = 261 > 255). Must saturate the reported width, not panic.
        let long_run = format!("{}/8/8/8/8/8/8/8", "9".repeat(29));
        let err = Board::from_fen_placement(&long_run).unwrap_err();
        assert_eq!(
            err,
            ParseBoardError::RankWrongWidth {
                rank: 8,
                files: 255
            }
        );

        // A pathologically long run cannot panic regardless of length.
        let huge = format!("{}/8/8/8/8/8/8/8", "9".repeat(10_000));
        assert!(matches!(
            Board::from_fen_placement(&huge),
            Err(ParseBoardError::RankWrongWidth { rank: 8, .. })
        ));
    }

    #[test]
    fn position_from_fen_rejects_oversized_skip_run() {
        use crate::Position;
        // The full-FEN entry point must also reject without panicking.
        assert!(Position::from_fen("99999999/8/8/8/8/8/8/8 w - - 0 1").is_err());
        let huge = format!("{} w - - 0 1", "9".repeat(10_000));
        assert!(Position::from_fen(&huge).is_err());
    }

    #[test]
    fn display_renders_standard_board() {
        let text = Board::standard().to_string();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 8);
        // Rank 8 (black back rank) is at the top.
        assert_eq!(lines[0], "r n b q k b n r");
        assert_eq!(lines[1], "p p p p p p p p");
        assert_eq!(lines[4], ". . . . . . . .");
        assert_eq!(lines[6], "P P P P P P P P");
        assert_eq!(lines[7], "R N B Q K B N R");
    }
}
