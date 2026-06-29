//! # Ataxx — a standalone 7×7 stones game (issue #280)
//!
//! This module is a **self-contained, non-chess game** that lives *beside* the
//! chess engine but shares none of its machinery. It does not use
//! [`crate::Board`], [`crate::Position`], [`crate::Bitboard`], the
//! [`crate::Variant`] trait, or any geometry type — it is its own tiny world
//! built on a single `u64` over the 49 squares of a 7×7 board. Ataxx has no
//! pieces, no king, no attacks, and no concept of check, so forcing it onto the
//! chess move generator would distort that engine for no gain. It is therefore
//! implemented here, cleanly fenced off, with its own [`Square`], [`Color`],
//! [`Move`], [`Position`], [`Outcome`], FEN parser, and [`Position::perft`]
//! node counter.
//!
//! ## Rules
//!
//! Two colours of stones share a 7×7 board. The starting position has each
//! colour on two opposite corners:
//!
//! ```text
//! P5p/7/7/7/7/7/p5P w 0 1
//! ```
//!
//! White starts on a7 and g1, Black on a1 and g7. On a turn the side to move
//! makes exactly one of:
//!
//! * **Clone** — pick an empty square at Chebyshev distance 1 from one of your
//!   own stones and place a new stone there; the source stone *stays*. The stone
//!   count grows by one.
//! * **Jump** — move one of your own stones to an empty square at Chebyshev
//!   distance *exactly* 2; the source square is vacated. The stone count is
//!   unchanged.
//! * **Pass** — only when you have at least one stone but no clone or jump is
//!   available *and* the opponent still has a move. (A side with no stones, or a
//!   full board, ends the game instead of passing.)
//!
//! After a stone lands on its destination square `to`, **every enemy stone in
//! the eight squares orthogonally or diagonally adjacent to `to` flips** to the
//! mover's colour (the "flip" / capture).
//!
//! The game ends when the side to move has no clone, jump, or pass available —
//! i.e. the board is full, the side to move has been wiped out, or neither side
//! can move. The winner is whoever has the most stones; equal counts are a draw.
//!
//! ## Validation
//!
//! Move generation and the terminal rules are pinned against Fairy-Stockfish
//! (`UCI_Variant ataxx`, `go perft`) — see `tests/perft_ataxx.rs` and the live
//! head-to-head in `compare-fairy/src/ataxx.rs`. From the start position the
//! node counts are `16, 256, 6460, 155888, 4752668, 141865520` at depths 1..=6.

use alloc::vec::Vec;
use core::cmp::Ordering;
use core::fmt;

/// The board is 7 files wide.
pub const FILES: u8 = 7;
/// The board is 7 ranks tall.
pub const RANKS: u8 = 7;
/// The board has 49 squares.
pub const NUM_SQUARES: usize = (FILES as usize) * (RANKS as usize);

/// A bitboard with every one of the 49 squares set.
const FULL_BOARD: u64 = (1u64 << NUM_SQUARES) - 1;

/// `CLONE[sq]` is the set of squares at Chebyshev distance 1 from `sq` — the
/// eight king-move neighbours. These are both the clone destinations reachable
/// from a stone on `sq` and the squares whose enemy stones flip when a stone
/// lands on `sq`.
const CLONE: [u64; NUM_SQUARES] = build_table(1);

/// `JUMP[sq]` is the set of squares at Chebyshev distance *exactly* 2 from `sq`
/// — the ring two squares out, the jump destinations.
const JUMP: [u64; NUM_SQUARES] = build_table(2);

/// Build a neighbour table for a fixed Chebyshev `distance` (1 or 2): bit `n` of
/// entry `sq` is set when `max(|Δfile|, |Δrank|) == distance`.
const fn build_table(distance: i32) -> [u64; NUM_SQUARES] {
    let mut table = [0u64; NUM_SQUARES];
    let mut rank = 0i32;
    while rank < RANKS as i32 {
        let mut file = 0i32;
        while file < FILES as i32 {
            let sq = (rank * FILES as i32 + file) as usize;
            let mut mask = 0u64;
            let mut dr = -distance;
            while dr <= distance {
                let mut df = -distance;
                while df <= distance {
                    let cheb = max(abs(dr), abs(df));
                    let nr = rank + dr;
                    let nf = file + df;
                    if cheb == distance
                        && nr >= 0
                        && nr < RANKS as i32
                        && nf >= 0
                        && nf < FILES as i32
                    {
                        mask |= 1u64 << ((nr * FILES as i32 + nf) as u32);
                    }
                    df += 1;
                }
                dr += 1;
            }
            table[sq] = mask;
            file += 1;
        }
        rank += 1;
    }
    table
}

/// `const`-context absolute value.
const fn abs(x: i32) -> i32 {
    if x < 0 {
        -x
    } else {
        x
    }
}

/// `const`-context maximum.
const fn max(a: i32, b: i32) -> i32 {
    if a > b {
        a
    } else {
        b
    }
}

/// One of the two stone colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Color {
    /// The side rendered with uppercase `P`, starting on a7 and g1.
    White,
    /// The side rendered with lowercase `p`, starting on a1 and g7.
    Black,
}

impl Color {
    /// The opposing colour.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Color::White => "w",
            Color::Black => "b",
        })
    }
}

/// A square of the 7×7 board, indexed `0..49` as `rank * 7 + file` with a1 = 0,
/// g1 = 6, a7 = 42, g7 = 48 (file a = 0, rank 1 = 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Square(u8);

impl Square {
    /// Build a square from a `file` (0 = a) and `rank` (0 = rank 1), each in
    /// `0..7`. Returns `None` if either is out of range.
    #[must_use]
    pub const fn new(file: u8, rank: u8) -> Option<Self> {
        if file < FILES && rank < RANKS {
            Some(Square(rank * FILES + file))
        } else {
            None
        }
    }

    /// Build a square from its raw `0..49` index, or `None` if out of range.
    #[must_use]
    pub const fn from_index(index: u8) -> Option<Self> {
        if (index as usize) < NUM_SQUARES {
            Some(Square(index))
        } else {
            None
        }
    }

    /// The raw `0..49` index.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// The file, `0..7` (a = 0).
    #[must_use]
    pub const fn file(self) -> u8 {
        self.0 % FILES
    }

    /// The rank, `0..7` (rank 1 = 0).
    #[must_use]
    pub const fn rank(self) -> u8 {
        self.0 / FILES
    }

    /// The single-bit bitboard for this square.
    const fn bit(self) -> u64 {
        1u64 << self.0
    }

    /// Parse a coordinate such as `a7` or `g1`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.len() != 2 {
            return None;
        }
        let file = bytes[0].checked_sub(b'a')?;
        let rank = bytes[1].checked_sub(b'1')?;
        Square::new(file, rank)
    }
}

impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}",
            (b'a' + self.file()) as char,
            (b'1' + self.rank()) as char
        )
    }
}

/// A single Ataxx move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Move {
    /// Clone a new stone onto an empty square adjacent (distance 1) to an own
    /// stone; the source is unchanged. Rendered as `@<to>`.
    Clone {
        /// The empty destination square the new stone appears on.
        to: Square,
    },
    /// Jump an own stone from `from` to an empty square exactly distance 2 away,
    /// vacating `from`. Rendered as `<from><to>`.
    Jump {
        /// The square the stone leaves.
        from: Square,
        /// The square the stone lands on.
        to: Square,
    },
    /// Pass the turn (only when no clone or jump is available). Rendered `pass`.
    Pass,
}

impl Move {
    /// The square a stone lands on, for [`Move::Clone`] and [`Move::Jump`]; the
    /// flip is computed around this square. `None` for [`Move::Pass`].
    #[must_use]
    pub const fn to(self) -> Option<Square> {
        match self {
            Move::Clone { to } | Move::Jump { to, .. } => Some(to),
            Move::Pass => None,
        }
    }
}

impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Move::Clone { to } => write!(f, "@{to}"),
            Move::Jump { from, to } => write!(f, "{from}{to}"),
            Move::Pass => f.write_str("pass"),
        }
    }
}

/// The result of a finished game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Outcome {
    /// White finished with strictly more stones.
    WhiteWins,
    /// Black finished with strictly more stones.
    BlackWins,
    /// The two stone counts were equal.
    Draw,
}

/// An error parsing an Ataxx FEN.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseFenError {
    /// The FEN did not have a placement and a side-to-move field.
    MissingField,
    /// The placement field did not describe exactly 7 ranks of 7 squares.
    BadPlacement,
    /// A placement character was not `P`, `p`, or a digit `1..=7`.
    BadSymbol,
    /// The side-to-move field was not `w` or `b`.
    BadSide,
    /// The half- or full-move counter was not a non-negative integer.
    BadCounter,
}

impl fmt::Display for ParseFenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ParseFenError::MissingField => "missing FEN field",
            ParseFenError::BadPlacement => "placement is not 7 ranks of 7 squares",
            ParseFenError::BadSymbol => "invalid placement symbol",
            ParseFenError::BadSide => "side to move is not 'w' or 'b'",
            ParseFenError::BadCounter => "move counter is not a non-negative integer",
        })
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ParseFenError {}

/// An Ataxx position: the two stone bitboards, the side to move, and the move
/// counters.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Position {
    white: u64,
    black: u64,
    side: Color,
    halfmove: u32,
    fullmove: u32,
}

impl Default for Position {
    fn default() -> Self {
        Self::startpos()
    }
}

impl Position {
    /// The standard starting position (`P5p/7/7/7/7/7/p5P w 0 1`).
    #[must_use]
    pub fn startpos() -> Self {
        // a7 = 42, g1 = 6 for White; a1 = 0, g7 = 48 for Black.
        Position {
            white: (1 << 42) | (1 << 6),
            black: (1 << 0) | (1 << 48),
            side: Color::White,
            halfmove: 0,
            fullmove: 1,
        }
    }

    /// The bitboard of White's stones.
    #[must_use]
    pub const fn white(&self) -> u64 {
        self.white
    }

    /// The bitboard of Black's stones.
    #[must_use]
    pub const fn black(&self) -> u64 {
        self.black
    }

    /// The side to move.
    #[must_use]
    pub const fn side_to_move(&self) -> Color {
        self.side
    }

    /// The number of White stones on the board.
    #[must_use]
    pub const fn white_count(&self) -> u32 {
        self.white.count_ones()
    }

    /// The number of Black stones on the board.
    #[must_use]
    pub const fn black_count(&self) -> u32 {
        self.black.count_ones()
    }

    /// The colour, if any, of the stone on `sq`.
    #[must_use]
    pub fn stone_at(&self, sq: Square) -> Option<Color> {
        if self.white & sq.bit() != 0 {
            Some(Color::White)
        } else if self.black & sq.bit() != 0 {
            Some(Color::Black)
        } else {
            None
        }
    }

    /// The bitboards of (own, opponent) stones for the side to move.
    const fn own_and_opp(&self) -> (u64, u64) {
        match self.side {
            Color::White => (self.white, self.black),
            Color::Black => (self.black, self.white),
        }
    }

    /// Does `stones` have any clone or jump move into `empty`?
    fn any_move(stones: u64, empty: u64) -> bool {
        let mut s = stones;
        while s != 0 {
            let sq = s.trailing_zeros() as usize;
            s &= s - 1;
            if CLONE[sq] & empty != 0 || JUMP[sq] & empty != 0 {
                return true;
            }
        }
        false
    }

    /// All legal moves in this position. Empty when the game is over. When the
    /// side to move has no clone or jump but is not eliminated, the board is not
    /// full, and the opponent can still move, the single move is [`Move::Pass`].
    #[must_use]
    pub fn legal_moves(&self) -> Vec<Move> {
        let occ = self.white | self.black;
        let empty = FULL_BOARD & !occ;
        let (own, opp) = self.own_and_opp();

        let mut moves = Vec::new();

        // Clones: empty squares adjacent to an own stone (one move each, no
        // matter how many own stones touch them — the result is identical).
        let mut e = empty;
        while e != 0 {
            let to = e.trailing_zeros() as usize;
            e &= e - 1;
            if CLONE[to] & own != 0 {
                moves.push(Move::Clone {
                    to: Square(to as u8),
                });
            }
        }

        // Jumps: each own stone to each empty square exactly distance 2 away.
        let mut o = own;
        while o != 0 {
            let from = o.trailing_zeros() as usize;
            o &= o - 1;
            let mut t = JUMP[from] & empty;
            while t != 0 {
                let to = t.trailing_zeros() as usize;
                t &= t - 1;
                moves.push(Move::Jump {
                    from: Square(from as u8),
                    to: Square(to as u8),
                });
            }
        }

        if moves.is_empty() && own != 0 && occ != FULL_BOARD && Self::any_move(opp, empty) {
            moves.push(Move::Pass);
        }

        moves
    }

    /// The position after `mv` (which must be legal in this position).
    #[must_use]
    pub fn make_move(&self, mv: &Move) -> Position {
        let mut white = self.white;
        let mut black = self.black;

        if let Some(to) = mv.to() {
            // Vacate the source of a jump.
            if let Move::Jump { from, .. } = mv {
                match self.side {
                    Color::White => white &= !from.bit(),
                    Color::Black => black &= !from.bit(),
                }
            }
            // Place the landing stone, then flip adjacent enemy stones.
            let landed = to.bit();
            let neighbours = CLONE[to.index()];
            match self.side {
                Color::White => {
                    let flips = black & neighbours;
                    white |= landed | flips;
                    black &= !flips;
                }
                Color::Black => {
                    let flips = white & neighbours;
                    black |= landed | flips;
                    white &= !flips;
                }
            }
        }

        Position {
            white,
            black,
            side: self.side.other(),
            // The 50-move-style counter resets whenever a stone is placed (a
            // clone or jump); it only advances across passes.
            halfmove: match mv {
                Move::Pass => self.halfmove + 1,
                _ => 0,
            },
            fullmove: self.fullmove + u32::from(self.side == Color::Black),
        }
    }

    /// Whether the game is over (no clone, jump, or pass is available).
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        self.legal_moves().is_empty()
    }

    /// The game result, or `None` while the game is still in progress.
    #[must_use]
    pub fn outcome(&self) -> Option<Outcome> {
        if !self.is_terminal() {
            return None;
        }
        Some(match self.white_count().cmp(&self.black_count()) {
            Ordering::Greater => Outcome::WhiteWins,
            Ordering::Less => Outcome::BlackWins,
            Ordering::Equal => Outcome::Draw,
        })
    }

    /// Count the leaf nodes of the move tree to `depth` plies.
    #[must_use]
    pub fn perft(&self, depth: u32) -> u64 {
        if depth == 0 {
            return 1;
        }
        let moves = self.legal_moves();
        if depth == 1 {
            return moves.len() as u64;
        }
        let mut nodes = 0;
        for mv in &moves {
            nodes += self.make_move(mv).perft(depth - 1);
        }
        nodes
    }

    /// Per-root-move [`perft`](Position::perft) breakdown: each legal move paired
    /// with the number of leaf nodes beneath it at `depth`.
    #[must_use]
    pub fn perft_divide(&self, depth: u32) -> Vec<(Move, u64)> {
        let mut out = Vec::new();
        for mv in self.legal_moves() {
            let n = if depth == 0 {
                0
            } else {
                self.make_move(&mv).perft(depth - 1)
            };
            out.push((mv, n));
        }
        out
    }

    /// Parse a position from its FEN, e.g. `P5p/7/7/7/7/7/p5P w 0 1`. The half-
    /// and full-move counters are optional and default to 0 and 1.
    pub fn from_fen(fen: &str) -> Result<Self, ParseFenError> {
        let mut fields = fen.split_whitespace();
        let placement = fields.next().ok_or(ParseFenError::MissingField)?;
        let side_field = fields.next().ok_or(ParseFenError::MissingField)?;

        let mut white = 0u64;
        let mut black = 0u64;
        let ranks: Vec<&str> = placement.split('/').collect();
        if ranks.len() != RANKS as usize {
            return Err(ParseFenError::BadPlacement);
        }
        // FEN lists ranks from rank 7 (top) down to rank 1 (bottom).
        for (row, rank_str) in ranks.iter().enumerate() {
            let rank = RANKS as usize - 1 - row;
            let mut file = 0usize;
            for ch in rank_str.bytes() {
                match ch {
                    b'1'..=b'7' => file += (ch - b'0') as usize,
                    b'P' | b'p' => {
                        if file >= FILES as usize {
                            return Err(ParseFenError::BadPlacement);
                        }
                        let bit = 1u64 << (rank * FILES as usize + file);
                        if ch == b'P' {
                            white |= bit;
                        } else {
                            black |= bit;
                        }
                        file += 1;
                    }
                    _ => return Err(ParseFenError::BadSymbol),
                }
            }
            if file != FILES as usize {
                return Err(ParseFenError::BadPlacement);
            }
        }

        let side = match side_field {
            "w" => Color::White,
            "b" => Color::Black,
            _ => return Err(ParseFenError::BadSide),
        };

        let halfmove = match fields.next() {
            None => 0,
            Some(s) => s.parse().map_err(|_| ParseFenError::BadCounter)?,
        };
        let fullmove = match fields.next() {
            None => 1,
            Some(s) => s.parse().map_err(|_| ParseFenError::BadCounter)?,
        };

        Ok(Position {
            white,
            black,
            side,
            halfmove,
            fullmove,
        })
    }

    /// Render the position as a FEN string (the inverse of [`from_fen`]).
    ///
    /// [`from_fen`]: Position::from_fen
    #[must_use]
    pub fn to_fen(&self) -> alloc::string::String {
        use core::fmt::Write as _;
        let mut out = alloc::string::String::new();
        for row in 0..RANKS as usize {
            let rank = RANKS as usize - 1 - row;
            let mut empty = 0u32;
            for file in 0..FILES as usize {
                let bit = 1u64 << (rank * FILES as usize + file);
                let symbol = if self.white & bit != 0 {
                    Some('P')
                } else if self.black & bit != 0 {
                    Some('p')
                } else {
                    None
                };
                match symbol {
                    Some(c) => {
                        if empty != 0 {
                            let _ = write!(out, "{empty}");
                            empty = 0;
                        }
                        out.push(c);
                    }
                    None => empty += 1,
                }
            }
            if empty != 0 {
                let _ = write!(out, "{empty}");
            }
            if row + 1 != RANKS as usize {
                out.push('/');
            }
        }
        let _ = write!(out, " {} {} {}", self.side, self.halfmove, self.fullmove);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn startpos_fen_round_trips() {
        let pos = Position::startpos();
        assert_eq!(pos.to_fen(), "P5p/7/7/7/7/7/p5P w 0 1");
        assert_eq!(Position::from_fen("P5p/7/7/7/7/7/p5P w 0 1").unwrap(), pos);
    }

    #[test]
    fn corner_squares_are_where_we_expect() {
        let pos = Position::startpos();
        assert_eq!(
            pos.stone_at(Square::parse("a7").unwrap()),
            Some(Color::White)
        );
        assert_eq!(
            pos.stone_at(Square::parse("g1").unwrap()),
            Some(Color::White)
        );
        assert_eq!(
            pos.stone_at(Square::parse("a1").unwrap()),
            Some(Color::Black)
        );
        assert_eq!(
            pos.stone_at(Square::parse("g7").unwrap()),
            Some(Color::Black)
        );
    }

    #[test]
    fn startpos_has_sixteen_moves() {
        let moves = Position::startpos().legal_moves();
        assert_eq!(moves.len(), 16);
        // Six clones, ten jumps, no pass.
        assert_eq!(
            moves
                .iter()
                .filter(|m| matches!(m, Move::Clone { .. }))
                .count(),
            6
        );
        assert_eq!(
            moves
                .iter()
                .filter(|m| matches!(m, Move::Jump { .. }))
                .count(),
            10
        );
    }

    #[test]
    fn clone_keeps_source_and_flips_neighbours() {
        // White clones a7 -> b7; the resulting b7 stone is adjacent to no enemy,
        // so no flip, and a7 (source) remains.
        let pos = Position::startpos();
        let b7 = Square::parse("b7").unwrap();
        let next = pos.make_move(&Move::Clone { to: b7 });
        assert_eq!(
            next.stone_at(Square::parse("a7").unwrap()),
            Some(Color::White)
        );
        assert_eq!(next.stone_at(b7), Some(Color::White));
        assert_eq!(next.white_count(), 3);
        assert_eq!(next.side_to_move(), Color::Black);
    }

    #[test]
    fn jump_vacates_source() {
        let pos = Position::startpos();
        let a7 = Square::parse("a7").unwrap();
        let c7 = Square::parse("c7").unwrap();
        let next = pos.make_move(&Move::Jump { from: a7, to: c7 });
        assert_eq!(next.stone_at(a7), None);
        assert_eq!(next.stone_at(c7), Some(Color::White));
        assert_eq!(next.white_count(), 2);
    }

    #[test]
    fn flip_converts_adjacent_enemy() {
        // White d4, Black e4 (adjacent). White clones d4 -> e5; e5 is adjacent to
        // the black e4, so e4 flips to White.
        let pos = Position::from_fen("7/7/7/3Pp2/7/7/7 w 0 1").unwrap();
        assert_eq!(
            pos.stone_at(Square::parse("e4").unwrap()),
            Some(Color::Black)
        );
        let e5 = Square::parse("e5").unwrap();
        let next = pos.make_move(&Move::Clone { to: e5 });
        assert_eq!(
            next.stone_at(Square::parse("e4").unwrap()),
            Some(Color::White)
        );
        assert_eq!(next.black_count(), 0);
        assert_eq!(next.white_count(), 3); // d4 (source), e5 (clone), e4 (flipped)
    }

    #[test]
    fn stuck_side_passes_when_opponent_can_move() {
        // White's lone a7 is walled in by Black; Black has moves, board not full.
        let pos = Position::from_fen("Ppp4/ppp4/ppp4/7/7/7/7 w 0 1").unwrap();
        let moves = pos.legal_moves();
        assert_eq!(moves, alloc::vec![Move::Pass]);
        assert!(!pos.is_terminal());
        assert_eq!(pos.outcome(), None);
    }

    #[test]
    fn eliminated_side_is_terminal_not_a_pass() {
        // White has no stones: the game is over (Black has won), no pass.
        let pos = Position::from_fen("p6/7/7/7/7/7/7 w 0 1").unwrap();
        assert!(pos.legal_moves().is_empty());
        assert!(pos.is_terminal());
        assert_eq!(pos.outcome(), Some(Outcome::BlackWins));
    }

    #[test]
    fn full_board_is_terminal() {
        let pos =
            Position::from_fen("PPPPPPP/PPPPPPP/PPPPPPP/PPPpPPP/PPPPPPP/PPPPPPP/PPPPPPP w 0 1")
                .unwrap();
        assert!(pos.is_terminal());
        assert_eq!(pos.outcome(), Some(Outcome::WhiteWins));
    }

    #[test]
    fn perft_startpos_matches_fsf() {
        let pos = Position::startpos();
        assert_eq!(pos.perft(1), 16);
        assert_eq!(pos.perft(2), 256);
        assert_eq!(pos.perft(3), 6460);
        assert_eq!(pos.perft(4), 155888);
    }

    #[test]
    fn move_display() {
        assert_eq!(Move::Pass.to_string(), "pass");
        assert_eq!(
            Move::Clone {
                to: Square::parse("f1").unwrap()
            }
            .to_string(),
            "@f1"
        );
        assert_eq!(
            Move::Jump {
                from: Square::parse("g1").unwrap(),
                to: Square::parse("e1").unwrap()
            }
            .to_string(),
            "g1e1"
        );
    }
}
