//! Generic piece placement over an arbitrary [`Geometry`].
//!
//! This is the parallel generic analogue of the concrete [`crate::Board`]; see
//! the [module docs](super) for why the two hierarchies are separate. Where the
//! concrete board is a fixed 8x8 with six role masks, [`Board<G>`] is any board
//! shape `G` supports (up to 128 squares) with the full extended
//! [`WideRole`] set.
//!
//! Placement is stored, like the concrete board, as a by-color plus by-role set
//! of [`Bitboard<G>`]s: two color masks and [`WideRole::COUNT`] role masks. A
//! square is occupied exactly when it appears in one color mask and one role
//! mask; the union of the color masks is the occupied set. Every mutator keeps
//! the masks in agreement.

use alloc::string::String;
use core::fmt;

use super::role::WideRole;
use super::{Bitboard, Geometry, Square};
use crate::Color;

/// The number of colors.
const COLOR_COUNT: usize = 2;

/// Returns the array index for a color's occupancy mask.
#[inline]
const fn color_index(color: Color) -> usize {
    match color {
        Color::White => 0,
        Color::Black => 1,
    }
}

/// A colored piece on a generic board: a [`WideRole`] together with a [`Color`].
///
/// The generic-layer analogue of the concrete [`crate::Piece`], over the
/// extended role set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WidePiece {
    /// The side the piece belongs to.
    pub color: Color,
    /// The kind of piece.
    pub role: WideRole,
}

impl WidePiece {
    /// Creates a new colored piece.
    #[must_use]
    #[inline]
    pub const fn new(color: Color, role: WideRole) -> WidePiece {
        WidePiece { color, role }
    }

    /// Returns the FEN character for this piece: uppercase for white, lowercase
    /// for black.
    #[must_use]
    #[inline]
    pub const fn char(self) -> char {
        match self.color {
            Color::White => self.role.upper_char(),
            Color::Black => self.role.char(),
        }
    }

    /// Parses a piece from its FEN character: uppercase yields white, lowercase
    /// yields black. Returns `None` for any non-role character.
    #[must_use]
    #[inline]
    pub const fn from_char(ch: char) -> Option<WidePiece> {
        let color = if ch.is_ascii_uppercase() {
            Color::White
        } else {
            Color::Black
        };
        match WideRole::from_char(ch) {
            Some(role) => Some(WidePiece::new(color, role)),
            None => None,
        }
    }
}

impl fmt::Display for WidePiece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("{}", self.char()))
    }
}

/// The piece placement of a board with geometry `G`: which [`WidePiece`], if
/// any, occupies each square.
///
/// This is purely the layout; it carries no side-to-move, castling, en passant,
/// or clock information. Two boards are equal when the same piece sits on every
/// square.
///
/// ```
/// use mce::geometry::{Board, Chess8x8};
///
/// let board = Board::<Chess8x8>::from_fen_placement(
///     "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR",
/// )
/// .unwrap();
/// assert_eq!(board.occupied().count(), 32);
/// ```
#[derive(Clone)]
pub struct Board<G: Geometry> {
    /// Occupancy mask per color, indexed by [`color_index`].
    by_color: [Bitboard<G>; COLOR_COUNT],
    /// Occupancy mask per role, indexed by [`WideRole::index`].
    by_role: [Bitboard<G>; WideRole::COUNT],
}

// Manual trait impls so the geometry marker `G` (a zero-sized type) need not
// implement these; the bounds are on `Bitboard<G>` instead.
impl<G: Geometry> Copy for Board<G> {}

impl<G: Geometry> PartialEq for Board<G> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.by_color == other.by_color && self.by_role == other.by_role
    }
}

impl<G: Geometry> Eq for Board<G> {}

impl<G: Geometry> fmt::Debug for Board<G>
where
    G::Bits: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Board")
            .field("by_color", &self.by_color)
            .field("by_role", &self.by_role)
            .finish()
    }
}

impl<G: Geometry> Default for Board<G> {
    /// An empty board (no standard starting placement is defined generically —
    /// the start array is variant-specific).
    #[inline]
    fn default() -> Board<G> {
        Board::empty()
    }
}

impl<G: Geometry> Board<G> {
    /// Creates a board with no pieces on it.
    #[must_use]
    #[inline]
    pub const fn empty() -> Board<G> {
        Board {
            by_color: [Bitboard::EMPTY; COLOR_COUNT],
            by_role: [Bitboard::EMPTY; WideRole::COUNT],
        }
    }

    /// Returns the set of all occupied squares.
    #[must_use]
    #[inline]
    pub fn occupied(self) -> Bitboard<G> {
        self.by_color[0] | self.by_color[1]
    }

    /// Returns `true` if the given square holds a piece.
    #[must_use]
    #[inline]
    pub fn is_occupied(self, square: Square<G>) -> bool {
        self.occupied().contains(square)
    }

    /// Returns the occupancy mask for one color.
    #[must_use]
    #[inline]
    pub fn by_color(self, color: Color) -> Bitboard<G> {
        self.by_color[color_index(color)]
    }

    /// Returns the occupancy mask for one role, across both colors.
    #[must_use]
    #[inline]
    pub fn by_role(self, role: WideRole) -> Bitboard<G> {
        self.by_role[role.index()]
    }

    /// Returns the squares occupied by the given piece (a specific color and
    /// role).
    #[must_use]
    #[inline]
    pub fn by_piece(self, piece: WidePiece) -> Bitboard<G> {
        self.by_color(piece.color) & self.by_role(piece.role)
    }

    /// Alias for [`Board::by_piece`], naming the color and role separately.
    #[must_use]
    #[inline]
    pub fn pieces(self, color: Color, role: WideRole) -> Bitboard<G> {
        self.by_piece(WidePiece::new(color, role))
    }

    /// Returns the color of the piece on the given square, or `None` if empty.
    #[must_use]
    #[inline]
    pub fn color_at(self, square: Square<G>) -> Option<Color> {
        Color::ALL
            .into_iter()
            .find(|&color| self.by_color(color).contains(square))
    }

    /// Returns the role of the piece on the given square, or `None` if empty.
    #[must_use]
    #[inline]
    pub fn role_at(self, square: Square<G>) -> Option<WideRole> {
        WideRole::ALL
            .into_iter()
            .find(|&role| self.by_role(role).contains(square))
    }

    /// Returns the piece on the given square, or `None` if it is empty.
    #[must_use]
    #[inline]
    pub fn piece_at(self, square: Square<G>) -> Option<WidePiece> {
        let color = self.color_at(square)?;
        let role = self.role_at(square)?;
        Some(WidePiece::new(color, role))
    }

    /// Returns the squares occupied by the given color's kings.
    ///
    /// Generic boards may have more than one king per side (Spartan has two
    /// black kings), so this returns the full set rather than a single square.
    #[must_use]
    #[inline]
    pub fn kings_of(self, color: Color) -> Bitboard<G> {
        self.pieces(color, WideRole::King)
    }

    /// Returns the square of the given color's king, if there is exactly one (or
    /// the lowest-indexed one if there are several).
    ///
    /// Prefer [`Board::kings_of`] for multi-king variants.
    #[must_use]
    #[inline]
    pub fn king_of(self, color: Color) -> Option<Square<G>> {
        self.kings_of(color).lsb()
    }

    /// Places `piece` on `square`, replacing whatever was there.
    ///
    /// All occupancy masks are updated together so the board stays consistent.
    #[inline]
    pub fn set_piece(&mut self, square: Square<G>, piece: WidePiece) {
        // Clear any existing occupant first so the masks never disagree.
        self.remove_piece(square);
        self.by_color[color_index(piece.color)].set(square);
        self.by_role[piece.role.index()].set(square);
    }

    /// Removes any piece from `square`, returning what was there (if anything).
    #[inline]
    pub fn remove_piece(&mut self, square: Square<G>) -> Option<WidePiece> {
        let piece = self.piece_at(square)?;
        self.by_color[color_index(piece.color)].clear(square);
        self.by_role[piece.role.index()].clear(square);
        Some(piece)
    }

    /// Removes any piece from `square` without reporting what was removed.
    #[inline]
    pub fn discard(&mut self, square: Square<G>) {
        let _ = self.remove_piece(square);
    }

    /// Clears `square` knowing the exact `piece` standing on it, touching only
    /// that piece's color and role masks instead of scanning for the occupant.
    ///
    /// Equivalent to [`remove_piece`](Self::remove_piece) when `piece` is the
    /// actual occupant of `square` (the only contract the make-move hot path needs,
    /// since it reads the piece off the board first): the same two masks are
    /// cleared, so the board stays consistent and the result is byte-identical.
    /// It skips the [`piece_at`](Self::piece_at) lookup `remove_piece` performs —
    /// up to a full role-mask scan per call.
    #[inline]
    pub(crate) fn remove_known(&mut self, square: Square<G>, piece: WidePiece) {
        self.by_color[color_index(piece.color)].clear(square);
        self.by_role[piece.role.index()].clear(square);
    }

    /// Places `piece` on `square` knowing it is **empty**, skipping the
    /// occupant-clearing scan [`set_piece`](Self::set_piece) does first.
    ///
    /// The caller guarantees `square` holds no piece (a quiet move's destination, a
    /// capture's destination *after* the captured piece was removed, a castle's
    /// vacated-then-refilled squares). Under that contract the result is identical
    /// to `set_piece`; it merely avoids the redundant [`remove_piece`](Self::remove_piece).
    #[inline]
    pub(crate) fn set_empty(&mut self, square: Square<G>, piece: WidePiece) {
        self.by_color[color_index(piece.color)].set(square);
        self.by_role[piece.role.index()].set(square);
    }

    /// The two color occupancy masks, `[white, black]`. Used by the make/unmake
    /// [`Undo`](super::Undo) to snapshot and restore the board's color planes
    /// without a per-square scan.
    #[inline]
    pub(crate) fn color_masks(&self) -> [Bitboard<G>; COLOR_COUNT] {
        self.by_color
    }

    /// Overwrites both color occupancy masks. The make/unmake unmake half restores
    /// the color planes by direct assignment (no scan).
    #[inline]
    pub(crate) fn set_color_masks(&mut self, masks: [Bitboard<G>; COLOR_COUNT]) {
        self.by_color = masks;
    }

    /// Overwrites the role mask at the raw `WideRole` index `index`. The unmake
    /// half restores exactly the role masks a move touched, by direct assignment
    /// (its snapshot reads them through [`by_role`](Self::by_role)).
    #[inline]
    pub(crate) fn set_role_mask(&mut self, index: usize, mask: Bitboard<G>) {
        self.by_role[index] = mask;
    }

    /// Parses a board from a FEN piece-placement field over the geometry `G`.
    ///
    /// The field lists the ranks from the top (`HEIGHT - 1`) down to `0`,
    /// separated by `/`. Within a rank, a piece letter (see
    /// [`WidePiece::from_char`]) places one piece and a run of consecutive
    /// digits is read as a single decimal count of empty squares to skip (so a
    /// ten-wide empty rank is `"10"`), walking from the first file to the last.
    /// A rank must describe exactly `WIDTH` files, and there must be exactly
    /// `HEIGHT` ranks.
    ///
    /// # Errors
    ///
    /// Returns a [`ParseBoardError`] if the field does not have exactly
    /// `HEIGHT` ranks, a rank does not describe exactly `WIDTH` files, a piece
    /// is placed past the last file, or a character is neither a role letter nor
    /// a non-zero digit.
    pub fn from_fen_placement(placement: &str) -> Result<Board<G>, ParseBoardError> {
        let width = G::WIDTH as usize;
        let height = G::HEIGHT;
        let mut board = Board::empty();
        let mut ranks = placement.split('/');

        // Ranks are listed from the top down.
        for rank_from_top in 0..height {
            let rank_str = ranks.next().ok_or(ParseBoardError::TooFewRanks)?;
            let rank = height - 1 - rank_from_top;

            // Accumulate in `usize` so an adversarial digit run can never
            // overflow and panic; the running total is saturated into the `u16`
            // error field.
            let mut file: usize = 0;
            let bytes = rank_str.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                let b = bytes[i];
                if b.is_ascii_digit() {
                    // Wide boards can skip more than nine squares, so a run of
                    // consecutive digits is read as a single decimal count (e.g.
                    // "10" on a ten-wide board). A leading zero is rejected, as
                    // is a bare "0".
                    if b == b'0' {
                        return Err(ParseBoardError::InvalidChar('0'));
                    }
                    let mut skip: usize = 0;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        skip = skip
                            .saturating_mul(10)
                            .saturating_add((bytes[i] - b'0') as usize);
                        i += 1;
                    }
                    file = file.saturating_add(skip);
                } else if b == b'+' {
                    // A `+` prefix marks a Shogi promoted piece: the following
                    // letter is the base piece, placed in its promoted form (`+P`
                    // = Tokin, `+R` = Dragon, ...). The base role must have a Shogi
                    // promotion; a `+` before a non-promotable piece is rejected.
                    i += 1;
                    let next = bytes
                        .get(i)
                        .copied()
                        .ok_or(ParseBoardError::InvalidChar('+'))?;
                    let base = WidePiece::from_char(next as char)
                        .ok_or(ParseBoardError::InvalidChar('+'))?;
                    let promoted = base.role.promoted_form();
                    if promoted == base.role {
                        // The base piece has no Shogi promotion.
                        return Err(ParseBoardError::InvalidChar('+'));
                    }
                    if file >= width {
                        return Err(ParseBoardError::RankTooLong(rank + 1));
                    }
                    let square = Square::new(rank * G::WIDTH + file as u8);
                    board.set_piece(square, WidePiece::new(base.color, promoted));
                    file += 1;
                    i += 1;
                } else if b as char == crate::geometry::role::OVERFLOW_PREFIX {
                    // A `*` prefix marks an overflow role (added past the exhausted
                    // single-letter alphabet): the following letter is a recycled
                    // base letter whose case carries the colour, resolved to the
                    // overflow role via `WideRole::overflow_from_base` (`*U` =
                    // white Commoner, `*u` = black). A `*` before a letter that
                    // names no overflow role is rejected.
                    i += 1;
                    let next = bytes.get(i).copied().ok_or(ParseBoardError::InvalidChar(
                        crate::geometry::role::OVERFLOW_PREFIX,
                    ))?;
                    // A **doubled** prefix (`**`) marks a second-bank overflow role
                    // (the Sho Shogi royals, added after the single-`*` bank was
                    // exhausted): the letter after the second `*` is the recycled
                    // base, resolved via `WideRole::overflow2_from_base`.
                    let (role, base) = if next as char == crate::geometry::role::OVERFLOW_PREFIX {
                        i += 1;
                        let base = bytes.get(i).copied().ok_or(ParseBoardError::InvalidChar(
                            crate::geometry::role::OVERFLOW_PREFIX,
                        ))?;
                        // A **tripled** prefix (`***`) marks a fourth-tier overflow
                        // role (the Chu Shogi army, added after all three lower
                        // banks were exhausted): the letter after the third `*` is
                        // the recycled base, resolved via
                        // `WideRole::overflow4_from_base`.
                        if base as char == crate::geometry::role::OVERFLOW_PREFIX {
                            i += 1;
                            let base =
                                bytes.get(i).copied().ok_or(ParseBoardError::InvalidChar(
                                    crate::geometry::role::OVERFLOW_PREFIX,
                                ))?;
                            let role = WideRole::overflow4_from_base(base as char).ok_or(
                                ParseBoardError::InvalidChar(
                                    crate::geometry::role::OVERFLOW_PREFIX,
                                ),
                            )?;
                            (role, base)
                        } else {
                            let role = WideRole::overflow2_from_base(base as char).ok_or(
                                ParseBoardError::InvalidChar(
                                    crate::geometry::role::OVERFLOW_PREFIX,
                                ),
                            )?;
                            (role, base)
                        }
                    } else {
                        let role = WideRole::overflow_from_base(next as char).ok_or(
                            ParseBoardError::InvalidChar(crate::geometry::role::OVERFLOW_PREFIX),
                        )?;
                        (role, next)
                    };
                    let color = if (base as char).is_ascii_uppercase() {
                        Color::White
                    } else {
                        Color::Black
                    };
                    if file >= width {
                        return Err(ParseBoardError::RankTooLong(rank + 1));
                    }
                    let square = Square::new(rank * G::WIDTH + file as u8);
                    board.set_piece(square, WidePiece::new(color, role));
                    file += 1;
                    i += 1;
                } else if b as char == crate::geometry::role::OVERFLOW_PREFIX_3 {
                    // A `=` prefix marks a third-tier overflow role (added past the
                    // exhausted single-letter alphabet, the exhausted `*`-overflow
                    // bases and the doubled-`**` second tier — the Cannon Shogi cannon
                    // army): the following letter is a recycled base letter whose case
                    // carries the colour, resolved via `WideRole::overflow3_from_base`
                    // (`=A` = white Rook-cannon, `=a` = black). A `=` before a letter
                    // that names no third-tier overflow role is rejected.
                    i += 1;
                    let next = bytes.get(i).copied().ok_or(ParseBoardError::InvalidChar(
                        crate::geometry::role::OVERFLOW_PREFIX_3,
                    ))?;
                    let role = WideRole::overflow3_from_base(next as char).ok_or(
                        ParseBoardError::InvalidChar(crate::geometry::role::OVERFLOW_PREFIX_3),
                    )?;
                    let color = if (next as char).is_ascii_uppercase() {
                        Color::White
                    } else {
                        Color::Black
                    };
                    if file >= width {
                        return Err(ParseBoardError::RankTooLong(rank + 1));
                    }
                    let square = Square::new(rank * G::WIDTH + file as u8);
                    board.set_piece(square, WidePiece::new(color, role));
                    file += 1;
                    i += 1;
                } else if let Some(piece) = WidePiece::from_char(b as char) {
                    if file >= width {
                        return Err(ParseBoardError::RankTooLong(rank + 1));
                    }
                    let square = Square::new(rank * G::WIDTH + file as u8);
                    board.set_piece(square, piece);
                    file += 1;
                    i += 1;
                } else {
                    // Not a digit or role letter: report the offending char.
                    // Decode the UTF-8 codepoint so a multibyte char is reported
                    // intact rather than as a lone byte.
                    let ch = rank_str[i..]
                        .chars()
                        .next()
                        .unwrap_or(char::REPLACEMENT_CHARACTER);
                    return Err(ParseBoardError::InvalidChar(ch));
                }
            }

            if file != width {
                return Err(ParseBoardError::RankWrongWidth {
                    rank: rank + 1,
                    files: u16::try_from(file).unwrap_or(u16::MAX),
                });
            }
        }

        if ranks.next().is_some() {
            return Err(ParseBoardError::TooManyRanks);
        }

        Ok(board)
    }

    /// Serializes the placement as a FEN piece-placement field over `G`.
    ///
    /// The inverse of [`Board::from_fen_placement`]: ranks run from the top
    /// down, runs of empty squares collapse into a decimal digit run, and
    /// pieces use their FEN letters.
    #[must_use]
    pub fn to_fen_placement(self) -> String {
        let width = G::WIDTH;
        let height = G::HEIGHT;
        let mut fen = String::with_capacity(width as usize * height as usize + height as usize);
        for rank_from_top in 0..height {
            let rank = height - 1 - rank_from_top;
            let mut empty: u32 = 0;
            for file in 0..width {
                let square = Square::new(rank * width + file);
                match self.piece_at(square) {
                    Some(piece) => {
                        if empty > 0 {
                            push_empty_run(&mut fen, empty);
                            empty = 0;
                        }
                        // A Shogi promoted piece renders as its base letter with a
                        // `+` prefix (`+P`, `+R`, ...); every other piece is a
                        // single letter. `char()` already returns the base letter
                        // for a promoted role, so only the prefix is added.
                        if piece.role.is_promoted() {
                            fen.push('+');
                        } else if piece.role.is_overflow4() {
                            // A fourth-tier overflow role (the Chu Shogi army)
                            // renders as the **tripled** `***` prefix plus its
                            // recycled base letter.
                            fen.push(crate::geometry::role::OVERFLOW_PREFIX);
                            fen.push(crate::geometry::role::OVERFLOW_PREFIX);
                            fen.push(crate::geometry::role::OVERFLOW_PREFIX);
                        } else if piece.role.is_overflow2() {
                            // A second-bank overflow role (the Sho Shogi royals)
                            // renders as the **doubled** `**` prefix plus its
                            // recycled base letter.
                            fen.push(crate::geometry::role::OVERFLOW_PREFIX);
                            fen.push(crate::geometry::role::OVERFLOW_PREFIX);
                        } else if piece.role.is_overflow() {
                            // An overflow role renders as the `*` prefix plus its
                            // recycled base letter; `char()` returns that base
                            // letter and `piece.char()` applies the colour case.
                            fen.push(crate::geometry::role::OVERFLOW_PREFIX);
                        } else if piece.role.is_overflow3() {
                            // A third-tier overflow role (the Cannon Shogi cannon
                            // army) renders as the `=` prefix plus its recycled base
                            // letter, exactly as a `*` overflow role but in the
                            // third tier.
                            fen.push(crate::geometry::role::OVERFLOW_PREFIX_3);
                        }
                        fen.push(piece.char());
                    }
                    None => empty += 1,
                }
            }
            if empty > 0 {
                push_empty_run(&mut fen, empty);
            }
            if rank > 0 {
                fen.push('/');
            }
        }
        fen
    }
}

/// Appends a run of `n` empty squares to a FEN rank as its decimal count.
///
/// Wide boards can have a run longer than nine squares (a 10-wide empty rank is
/// `"10"`), so the count is written as a full decimal number rather than a
/// single digit.
fn push_empty_run(out: &mut String, n: u32) {
    if n >= 10 {
        // At most three digits for any board up to 128 wide.
        let mut started = false;
        for divisor in [100u32, 10, 1] {
            let digit = (n / divisor) % 10;
            if digit != 0 || started || divisor == 1 {
                out.push((b'0' + digit as u8) as char);
                started = true;
            }
        }
    } else {
        out.push((b'0' + n as u8) as char);
    }
}

impl<G: Geometry> fmt::Display for Board<G> {
    /// Renders the board as `HEIGHT` rows of `WIDTH` cells, the top rank first,
    /// using a piece's FEN letter for occupied squares and `.` for empty ones.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let width = G::WIDTH;
        for rank in (0..G::HEIGHT).rev() {
            for file in 0..width {
                let square = Square::new(rank * width + file);
                match self.piece_at(square) {
                    Some(piece) => f.write_fmt(format_args!("{}", piece.char()))?,
                    None => f.write_str(".")?,
                }
                if file + 1 < width {
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

/// The error returned when a FEN piece-placement field cannot be parsed over a
/// geometry.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ParseBoardError {
    /// The placement field had fewer than `HEIGHT` ranks.
    TooFewRanks,
    /// The placement field had more than `HEIGHT` ranks.
    TooManyRanks,
    /// A rank described a number of files other than `WIDTH`. `rank` is the
    /// 1-based rank number and `files` is how many it covered (saturated).
    RankWrongWidth {
        /// The 1-based rank number whose width was wrong.
        rank: u8,
        /// The number of files the rank actually described.
        files: u16,
    },
    /// A rank tried to place a piece past the last file. The value is the
    /// 1-based rank number.
    RankTooLong(u8),
    /// An unexpected character appeared in the field.
    InvalidChar(char),
}

impl fmt::Display for ParseBoardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseBoardError::TooFewRanks => {
                f.write_str("FEN placement has fewer ranks than the board height")
            }
            ParseBoardError::TooManyRanks => {
                f.write_str("FEN placement has more ranks than the board height")
            }
            ParseBoardError::RankWrongWidth { rank, files } => write!(
                f,
                "rank {rank} of FEN placement covers {files} files, expected the board width",
            ),
            ParseBoardError::RankTooLong(rank) => {
                write!(f, "rank {rank} of FEN placement extends past the last file")
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
    use crate::geometry::{Cap10x8, Chess8x8};
    use alloc::vec::Vec;

    const STANDARD_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

    #[test]
    fn empty_board_has_nothing() {
        let board = Board::<Chess8x8>::empty();
        assert!(board.occupied().is_empty());
        for index in 0..64u8 {
            assert_eq!(board.piece_at(Square::new(index)), None);
        }
        assert_eq!(board.king_of(Color::White), None);
        assert!(board.kings_of(Color::White).is_empty());
        assert_eq!(Board::<Chess8x8>::default(), board);
    }

    #[test]
    fn chess8x8_standard_round_trips_and_equals_piece_by_piece() {
        let board = Board::<Chess8x8>::from_fen_placement(STANDARD_FEN).unwrap();
        assert_eq!(board.to_fen_placement(), STANDARD_FEN);
        assert_eq!(board.occupied().count(), 32);

        // Compare piece-by-piece against the concrete board.
        let concrete = crate::Board::standard();
        for index in 0..64u8 {
            let generic = board.piece_at(Square::new(index));
            let conc = concrete.piece_at(crate::Square::new(index));
            match (generic, conc) {
                (None, None) => {}
                (Some(g), Some(c)) => {
                    assert_eq!(g.color, c.color, "color at {index}");
                    assert_eq!(g.char(), c.char(), "char at {index}");
                }
                _ => panic!("occupancy diverged at {index}"),
            }
        }

        // King and mask checks.
        assert_eq!(board.king_of(Color::White), Some(Square::new(4))); // e1
        assert_eq!(board.king_of(Color::Black), Some(Square::new(60))); // e8
        assert_eq!(board.by_color(Color::White).count(), 16);
        assert_eq!(board.by_role(WideRole::Pawn).count(), 16);
        assert_eq!(board.pieces(Color::White, WideRole::Rook).count(), 2);
    }

    #[test]
    fn cap10x8_round_trips_a_10x8_placement() {
        // Ten files, eight ranks. Each rank is ten columns. Place a fairy
        // archbishop (hawk, 'a') and chancellor (elephant, 'e') alongside the
        // standard back rank, exercising the wide roles and the tenth file.
        // Back rank (rank 1): r n b q k b n r then a (hawk) and e (elephant).
        let fen = "rnbqkbnrae/pppppppppp/10/10/10/10/PPPPPPPPPP/RNBQKBNRAE";
        let board = Board::<Cap10x8>::from_fen_placement(fen).unwrap();
        assert_eq!(board.to_fen_placement(), fen);
        assert_eq!(board.occupied().count(), 40);

        // The hawk (B+N) sits on file 8 (i1 -> index 8); the elephant on file 9.
        assert_eq!(
            board.piece_at(Square::new(8)),
            Some(WidePiece::new(Color::White, WideRole::Hawk)),
        );
        assert_eq!(
            board.piece_at(Square::new(9)),
            Some(WidePiece::new(Color::White, WideRole::Elephant)),
        );
        // Black's pair on the top rank (rank 8): indices 78, 79.
        assert_eq!(
            board.piece_at(Square::new(78)),
            Some(WidePiece::new(Color::Black, WideRole::Hawk)),
        );
        assert_eq!(
            board.piece_at(Square::new(79)),
            Some(WidePiece::new(Color::Black, WideRole::Elephant)),
        );
        assert_eq!(board.king_of(Color::White), Some(Square::new(4)));
        assert_eq!(board.by_role(WideRole::Hawk).count(), 2);
        assert_eq!(board.by_role(WideRole::Elephant).count(), 2);
        assert_eq!(board.by_role(WideRole::Pawn).count(), 20);
    }

    #[test]
    fn cap10x8_empty_rank_uses_two_digit_count() {
        // An empty 10x8 board: every rank is "10".
        let board = Board::<Cap10x8>::empty();
        let fen = board.to_fen_placement();
        assert_eq!(fen, "10/10/10/10/10/10/10/10");
        // Round-trips.
        let parsed = Board::<Cap10x8>::from_fen_placement(&fen).unwrap();
        assert_eq!(parsed, board);
    }

    #[test]
    fn set_remove_keep_masks_consistent() {
        let mut board = Board::<Cap10x8>::empty();
        let cannon = WidePiece::new(Color::White, WideRole::Cannon);
        let sq = Square::new(55);
        board.set_piece(sq, cannon);
        assert_eq!(board.piece_at(sq), Some(cannon));
        assert_eq!(board.occupied().count(), 1);
        assert_eq!(board.by_role(WideRole::Cannon).count(), 1);

        // Overwriting replaces the occupant with no stale bits.
        let lance = WidePiece::new(Color::Black, WideRole::Lance);
        board.set_piece(sq, lance);
        assert_eq!(board.piece_at(sq), Some(lance));
        assert_eq!(board.occupied().count(), 1);
        assert_eq!(board.by_role(WideRole::Cannon).count(), 0);
        assert_eq!(board.by_role(WideRole::Lance).count(), 1);
        assert_eq!(board.by_color(Color::White).count(), 0);

        assert_eq!(board.remove_piece(sq), Some(lance));
        assert_eq!(board.remove_piece(sq), None);
        assert!(board.occupied().is_empty());

        board.set_piece(sq, cannon);
        board.discard(sq);
        assert!(board.occupied().is_empty());
    }

    #[test]
    fn multi_king_via_kings_of() {
        // Spartan-style: two black kings. kings_of returns both.
        let mut board = Board::<Chess8x8>::empty();
        board.set_piece(
            Square::new(60),
            WidePiece::new(Color::Black, WideRole::King),
        );
        board.set_piece(
            Square::new(62),
            WidePiece::new(Color::Black, WideRole::King),
        );
        assert_eq!(board.kings_of(Color::Black).count(), 2);
        // king_of returns the lowest-indexed one.
        assert_eq!(board.king_of(Color::Black), Some(Square::new(60)));
    }

    #[test]
    fn rejects_too_few_ranks() {
        let err = Board::<Chess8x8>::from_fen_placement("8/8/8/8/8/8/8").unwrap_err();
        assert_eq!(err, ParseBoardError::TooFewRanks);
    }

    #[test]
    fn rejects_too_many_ranks() {
        let err = Board::<Chess8x8>::from_fen_placement("8/8/8/8/8/8/8/8/8").unwrap_err();
        assert_eq!(err, ParseBoardError::TooManyRanks);
    }

    #[test]
    fn rejects_rank_wrong_width() {
        // 8x8: a rank covering only 7 files.
        let err = Board::<Chess8x8>::from_fen_placement("7/8/8/8/8/8/8/8").unwrap_err();
        assert_eq!(err, ParseBoardError::RankWrongWidth { rank: 8, files: 7 });
        // 10x8: a rank of width 8 is too narrow for the ten-wide board.
        let err = Board::<Cap10x8>::from_fen_placement("8/10/10/10/10/10/10/10").unwrap_err();
        assert_eq!(err, ParseBoardError::RankWrongWidth { rank: 8, files: 8 });
    }

    #[test]
    fn rejects_rank_too_long_with_pieces() {
        // Nine pawns then another piece overruns the h-file on 8x8.
        let err = Board::<Chess8x8>::from_fen_placement("ppppppppp/8/8/8/8/8/8/8").unwrap_err();
        assert_eq!(err, ParseBoardError::RankTooLong(8));
        // Eleven pieces overrun a 10-wide rank.
        let err =
            Board::<Cap10x8>::from_fen_placement("ppppppppppp/10/10/10/10/10/10/10").unwrap_err();
        assert_eq!(err, ParseBoardError::RankTooLong(8));
    }

    #[test]
    fn rejects_invalid_char() {
        // Every ASCII letter now names a role (the alphabet is fully assigned), so
        // an invalid placement char is a non-letter symbol such as `.`.
        let err = Board::<Chess8x8>::from_fen_placement(".nbqkbnr/8/8/8/8/8/8/8").unwrap_err();
        assert_eq!(err, ParseBoardError::InvalidChar('.'));
        // A zero digit is not allowed.
        let err = Board::<Chess8x8>::from_fen_placement("08/8/8/8/8/8/8/8").unwrap_err();
        assert_eq!(err, ParseBoardError::InvalidChar('0'));
        // A multibyte char is reported as its full codepoint.
        let err = Board::<Chess8x8>::from_fen_placement("é7/8/8/8/8/8/8/8").unwrap_err();
        assert_eq!(err, ParseBoardError::InvalidChar('é'));
    }

    #[test]
    fn rejects_oversized_skip_run_without_panicking() {
        // A multi-digit count that would overflow a naive counter must saturate,
        // not panic. "99999999" is now read as one decimal number, well past the
        // board width, so it is rejected with a saturated file count.
        let err = Board::<Chess8x8>::from_fen_placement("99999999/8/8/8/8/8/8/8").unwrap_err();
        assert_eq!(
            err,
            ParseBoardError::RankWrongWidth {
                rank: 8,
                files: u16::MAX
            }
        );
        // A pathologically long run cannot panic regardless of length.
        let huge = {
            let mut s = String::new();
            for _ in 0..10_000 {
                s.push('9');
            }
            s.push_str("/8/8/8/8/8/8/8");
            s
        };
        assert!(matches!(
            Board::<Chess8x8>::from_fen_placement(&huge),
            Err(ParseBoardError::RankWrongWidth { rank: 8, .. })
        ));
    }

    #[test]
    fn display_renders_top_rank_first() {
        let board = Board::<Chess8x8>::from_fen_placement(STANDARD_FEN).unwrap();
        let text = alloc::format!("{board}");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 8);
        assert_eq!(lines[0], "r n b q k b n r");
        assert_eq!(lines[7], "R N B Q K B N R");
    }
}
