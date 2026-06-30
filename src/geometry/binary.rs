//! A compact, versioned **binary wire format** for wide-layer moves and
//! positions (issue #319), for the mcs server transport and on-disk storage.
//!
//! It is smaller and faster to parse than the FEN + UCI strings, and — unlike a
//! FEN round trip — it is **lossless across every variant**: it preserves the
//! Alice plane mask, the Janggi pass counter, and the crazyhouse promoted mask,
//! none of which survive `from_fen`. `decode(encode(x))` reproduces `x` exactly
//! (a position byte-for-byte in board, state, and promoted mask; a move in its
//! full packed word), so it round-trips for all 47 fairy variants.
//!
//! # Versioning
//!
//! Every encoding begins with a **single tag byte** that selects both the value
//! kind and the format version. The version-1 tags are [`TAG_MOVE`],
//! [`TAG_POSITION`], [`TAG_ANY_POSITION`], and [`TAG_GAME`] (all in the `0xC_`
//! range). A future incompatible layout takes a fresh tag value (e.g. the `0xD_`
//! range) so an old decoder rejects it with [`WireError::BadTag`] rather than
//! misreading it. The decoders never panic: malformed, truncated, or
//! out-of-range input returns a [`WireError`].
//!
//! # Move layout ([`WideMove`])
//!
//! `WideMove` is a packed `u64` (origin / destination square indices, kind, role,
//! and the Seirawan / S-House / Duck addenda — see the `wide_move` module). The wire
//! form is that word, little-endian with trailing zero bytes trimmed: a length
//! byte `0..=8` followed by that many value bytes. An ordinary move needs 2–4
//! bytes; only the rare fairy addenda reach the high bytes. The standalone
//! [`WideMove::to_bytes`] prefixes [`TAG_MOVE`].
//!
//! # Position layout ([`GenericPosition`])
//!
//! After the tag byte (omitted for the body embedded in [`AnyWideVariant`] /
//! game records):
//!
//! 1. a **`u16` flags** word (little-endian) marking which optional sections
//!    follow (`F_EP` … `F_CLOCKS`) plus the side to move (`F_TURN_BLACK`);
//! 2. the **board**: an occupancy bitset of `ceil(SQUARES / 8)` bytes, then one
//!    byte per occupied square in ascending order — `color << 7 | role` (the role
//!    index is `0..76`, so it fits the low 7 bits);
//! 3. the present optional sections, in flag-bit order: en passant (1 square
//!    byte), castling (4 rook-file bytes, `255` = no right), Seirawan gating
//!    (eligible bitset + a reserve nibble byte), the hand / setup pocket (a
//!    sparse `(role, count)` list per colour), the Duck square, the crazyhouse
//!    promoted bitset, the Alice plane-B bitset, the Janggi pass counter, and the
//!    two clocks (LEB128 varints; absent means halfmove `0`, fullmove `1`).
//!
//! The geometry and variant are fixed by the concrete `GenericPosition<G, V>`
//! type, so they are not stored; [`AnyWideVariant`] prepends a 1-byte
//! [`WideVariantId`](crate::geometry::WideVariantId) selector for a self-describing form.

use alloc::vec::Vec;

use super::{
    AnyWideVariant, Bitboard, Board, GateRole, GenericCastling, GenericGating, GenericPlacement,
    GenericPosition, GenericState, Geometry, Square, WideMove, WidePiece, WideRole, WideVariant,
};
use crate::Color;

/// Format tag for a standalone [`WideMove`] (version 1).
pub const TAG_MOVE: u8 = 0xC1;
/// Format tag for a typed [`GenericPosition`] body (version 1).
pub const TAG_POSITION: u8 = 0xC2;
/// Format tag for a self-describing [`AnyWideVariant`] position (version 1).
pub const TAG_ANY_POSITION: u8 = 0xC3;
/// Format tag for a game record — a start position plus a move list (version 1).
pub const TAG_GAME: u8 = 0xC4;

// Position flag bits (a little-endian `u16` after the tag).
const F_TURN_BLACK: u16 = 1 << 0;
const F_EP: u16 = 1 << 1;
const F_CASTLING: u16 = 1 << 2;
const F_GATING: u16 = 1 << 3;
const F_HAND: u16 = 1 << 4;
const F_DUCK: u16 = 1 << 5;
const F_PROMOTED: u16 = 1 << 6;
const F_BOARD_B: u16 = 1 << 7;
const F_PASSES: u16 = 1 << 8;
const F_CLOCKS: u16 = 1 << 9;

/// The widest board the geometry layer supports is 128 squares, so an occupancy
/// bitset never exceeds this many bytes; using a fixed buffer keeps the bitset
/// encoder allocation-free.
const MAX_BITSET_BYTES: usize = 16;

/// The error returned when a [`WireError`]-producing decoder is handed input it
/// cannot parse. Every decoder returns this rather than panicking.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum WireError {
    /// The input ended before a complete value was read.
    Truncated,
    /// The leading tag byte is not the one this decoder expects (the wrapped
    /// byte is what was found).
    BadTag(u8),
    /// The variant selector byte names no [`WideVariantId`](crate::geometry::WideVariantId).
    UnknownVariant(u8),
    /// A square-index byte is out of range for the board geometry.
    BadSquare(u8),
    /// A role-index byte is out of range for [`WideRole`].
    BadRole(u8),
    /// A field holds a value the format cannot represent (e.g. an over-long
    /// move word or a clock past `u16`).
    BadValue,
    /// Bytes remained after a complete value was decoded.
    TrailingData,
}

impl core::fmt::Display for WireError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            WireError::Truncated => f.write_str("binary wire input ended unexpectedly"),
            WireError::BadTag(b) => write!(f, "unexpected binary wire tag byte {b:#04x}"),
            WireError::UnknownVariant(b) => write!(f, "unknown variant selector byte {b}"),
            WireError::BadSquare(s) => write!(f, "square index {s} out of range"),
            WireError::BadRole(r) => write!(f, "role index {r} out of range"),
            WireError::BadValue => f.write_str("binary wire field holds an unrepresentable value"),
            WireError::TrailingData => f.write_str("trailing bytes after a complete binary value"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for WireError {}

/// A forward-only reader over a byte slice, bounds-checking every read so a
/// short or malformed buffer yields [`WireError::Truncated`] instead of a panic.
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    #[inline]
    fn new(bytes: &'a [u8]) -> Cursor<'a> {
        Cursor { bytes, pos: 0 }
    }

    /// Borrows the next `n` bytes, advancing past them.
    #[inline]
    fn take(&mut self, n: usize) -> Result<&'a [u8], WireError> {
        let end = self.pos.checked_add(n).ok_or(WireError::Truncated)?;
        let slice = self.bytes.get(self.pos..end).ok_or(WireError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    /// Reads one byte.
    #[inline]
    fn u8(&mut self) -> Result<u8, WireError> {
        Ok(self.take(1)?[0])
    }

    /// Reads a little-endian `u16`.
    #[inline]
    fn u16_le(&mut self) -> Result<u16, WireError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    /// Reads an unsigned LEB128 varint, rejecting an over-long encoding.
    fn varint(&mut self) -> Result<u64, WireError> {
        let mut result: u64 = 0;
        let mut shift: u32 = 0;
        loop {
            let byte = self.u8()?;
            if shift >= 64 {
                return Err(WireError::BadValue);
            }
            result |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        Ok(result)
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.pos >= self.bytes.len()
    }
}

/// Appends an unsigned LEB128 varint.
fn push_varint(mut value: u64, out: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            break;
        }
        out.push(byte | 0x80);
    }
}

/// The number of bytes a `G` occupancy / plane bitset occupies.
#[inline]
fn bitset_len<G: Geometry>() -> usize {
    (G::SQUARES as usize).div_ceil(8)
}

/// Appends `bb` as a fixed-width little-endian bitset over `G`'s squares.
fn encode_bitset<G: Geometry>(bb: Bitboard<G>, out: &mut Vec<u8>) {
    let mut buf = [0u8; MAX_BITSET_BYTES];
    for sq in bb {
        let i = sq.index() as usize;
        buf[i / 8] |= 1 << (i % 8);
    }
    out.extend_from_slice(&buf[..bitset_len::<G>()]);
}

/// Reads a `G` bitset, ignoring any bits above `G::SQUARES`.
fn decode_bitset<G: Geometry>(cur: &mut Cursor<'_>) -> Result<Bitboard<G>, WireError> {
    let slice = cur.take(bitset_len::<G>())?;
    let mut bb = Bitboard::<G>::EMPTY;
    let squares = G::SQUARES as usize;
    for (byte_ix, &byte) in slice.iter().enumerate() {
        if byte == 0 {
            continue;
        }
        for bit in 0..8u8 {
            if byte & (1 << bit) != 0 {
                let i = byte_ix * 8 + bit as usize;
                if i < squares {
                    bb.set(Square::<G>::new(i as u8));
                }
            }
        }
    }
    Ok(bb)
}

/// Appends one colour's hand / setup pocket as a sparse `(role, count)` list.
fn encode_hand(pocket: &GenericPlacement, color: Color, out: &mut Vec<u8>) {
    let count_at = out.len();
    out.push(0);
    let mut n = 0u8;
    for role in WideRole::ALL {
        let c = pocket.count(color, role);
        if c != 0 {
            out.push(role.index() as u8);
            out.push(c);
            n += 1;
        }
    }
    out[count_at] = n;
}

/// Reads one colour's pocket into a per-role count array.
fn decode_hand(cur: &mut Cursor<'_>) -> Result<[u8; WideRole::COUNT], WireError> {
    let n = cur.u8()?;
    let mut counts = [0u8; WideRole::COUNT];
    for _ in 0..n {
        let role = cur.u8()?;
        let count = cur.u8()?;
        let idx = role as usize;
        if idx >= WideRole::COUNT {
            return Err(WireError::BadRole(role));
        }
        counts[idx] = count;
    }
    Ok(counts)
}

/// Appends a [`WideMove`] as a length-prefixed, trailing-zero-trimmed
/// little-endian word (self-delimiting, for embedding in a game record).
fn encode_move_body(mv: WideMove, out: &mut Vec<u8>) {
    let bytes = mv.to_raw().to_le_bytes();
    let mut len = bytes.len();
    while len > 0 && bytes[len - 1] == 0 {
        len -= 1;
    }
    out.push(len as u8);
    out.extend_from_slice(&bytes[..len]);
}

/// Reads a move written by [`encode_move_body`].
fn decode_move_body(cur: &mut Cursor<'_>) -> Result<WideMove, WireError> {
    let len = cur.u8()? as usize;
    if len > 8 {
        return Err(WireError::BadValue);
    }
    let slice = cur.take(len)?;
    let mut buf = [0u8; 8];
    buf[..len].copy_from_slice(slice);
    Ok(WideMove::from_raw(u64::from_le_bytes(buf)))
}

impl WideMove {
    /// Encodes this move to its compact binary wire form: [`TAG_MOVE`], a length
    /// byte, then the trailing-zero-trimmed little-endian packed word (typically
    /// 4–6 bytes, up to 10 when a Duck / gating addendum reaches the high word).
    /// The inverse is [`from_bytes`](Self::from_bytes).
    #[must_use]
    pub fn to_bytes(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(4);
        out.push(TAG_MOVE);
        encode_move_body(self, &mut out);
        out
    }

    /// Decodes a move written by [`to_bytes`](Self::to_bytes).
    ///
    /// # Errors
    ///
    /// Returns [`WireError`] if `bytes` is truncated, carries the wrong tag, or
    /// has trailing data — never panicking.
    pub fn from_bytes(bytes: &[u8]) -> Result<WideMove, WireError> {
        let (&tag, rest) = bytes.split_first().ok_or(WireError::Truncated)?;
        if tag != TAG_MOVE {
            return Err(WireError::BadTag(tag));
        }
        let mut cur = Cursor::new(rest);
        let mv = decode_move_body(&mut cur)?;
        if !cur.is_empty() {
            return Err(WireError::TrailingData);
        }
        Ok(mv)
    }
}

impl<G: Geometry, V: WideVariant<G>> GenericPosition<G, V> {
    /// Encodes this position to the compact binary wire form: [`TAG_POSITION`]
    /// followed by the position body (see the [module docs](self)). The geometry
    /// and variant are fixed by the type, so they are not stored; use
    /// [`AnyWideVariant::to_bytes`] for a self-describing form. The inverse is
    /// [`from_bytes`](Self::from_bytes).
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(TAG_POSITION);
        self.encode_body(&mut out);
        out
    }

    /// Decodes a position written by [`to_bytes`](Self::to_bytes) for the same
    /// `G` / `V`.
    ///
    /// # Errors
    ///
    /// Returns [`WireError`] for truncated, wrong-tagged, trailing, or
    /// out-of-range input — never panicking.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, WireError> {
        let (&tag, rest) = bytes.split_first().ok_or(WireError::Truncated)?;
        if tag != TAG_POSITION {
            return Err(WireError::BadTag(tag));
        }
        Self::decode_body(rest)
    }

    /// Appends this position's body (no tag) — shared by [`to_bytes`](Self::to_bytes)
    /// and the self-describing [`AnyWideVariant::to_bytes`].
    pub(crate) fn encode_body(&self, out: &mut Vec<u8>) {
        let board = self.board();
        let state = self.state();
        let promoted = self.promoted();
        let gating = state.gating;
        let pocket = state.placement;

        let mut flags: u16 = 0;
        if state.turn == Color::Black {
            flags |= F_TURN_BLACK;
        }
        if state.ep_square.is_some() {
            flags |= F_EP;
        }
        if !state.castling.is_empty() {
            flags |= F_CASTLING;
        }
        if !gating.eligible().is_empty()
            || gating.any_reserve(Color::White)
            || gating.any_reserve(Color::Black)
        {
            flags |= F_GATING;
        }
        if pocket.any(Color::White) || pocket.any(Color::Black) {
            flags |= F_HAND;
        }
        if state.duck.is_some() {
            flags |= F_DUCK;
        }
        if !promoted.is_empty() {
            flags |= F_PROMOTED;
        }
        if !state.board_b.is_empty() {
            flags |= F_BOARD_B;
        }
        if state.consecutive_passes != 0 {
            flags |= F_PASSES;
        }
        if state.halfmove_clock != 0 || state.fullmove_number != 1 {
            flags |= F_CLOCKS;
        }
        out.extend_from_slice(&flags.to_le_bytes());

        // Board: occupancy bitset, then a colour+role byte per occupied square in
        // ascending index order (the same order the decoder walks).
        let occupied = board.occupied();
        encode_bitset(occupied, out);
        for sq in occupied {
            let piece = board
                .piece_at(sq)
                .expect("occupied square holds a piece by construction");
            let color_bit = if piece.color == Color::Black { 0x80 } else { 0 };
            out.push(color_bit | piece.role.index() as u8);
        }

        if let Some(ep) = state.ep_square {
            out.push(ep.index());
        }
        if flags & F_CASTLING != 0 {
            for color in [Color::White, Color::Black] {
                for side in 0..2usize {
                    out.push(state.castling.rook_file(color, side).unwrap_or(255));
                }
            }
        }
        if flags & F_GATING != 0 {
            encode_bitset(gating.eligible(), out);
            let mut reserves = 0u8;
            if gating.has_reserve(Color::White, GateRole::Hawk) {
                reserves |= 0x01;
            }
            if gating.has_reserve(Color::White, GateRole::Elephant) {
                reserves |= 0x02;
            }
            if gating.has_reserve(Color::Black, GateRole::Hawk) {
                reserves |= 0x04;
            }
            if gating.has_reserve(Color::Black, GateRole::Elephant) {
                reserves |= 0x08;
            }
            out.push(reserves);
        }
        if flags & F_HAND != 0 {
            encode_hand(&pocket, Color::White, out);
            encode_hand(&pocket, Color::Black, out);
        }
        if let Some(duck) = state.duck {
            out.push(duck.index());
        }
        if flags & F_PROMOTED != 0 {
            encode_bitset(promoted, out);
        }
        if flags & F_BOARD_B != 0 {
            encode_bitset(state.board_b, out);
        }
        if flags & F_PASSES != 0 {
            out.push(state.consecutive_passes);
        }
        if flags & F_CLOCKS != 0 {
            push_varint(u64::from(state.halfmove_clock), out);
            push_varint(u64::from(state.fullmove_number), out);
        }
    }

    /// Decodes a position body (no tag) for the given `G` / `V`, validating that
    /// the bytes are exactly consumed.
    pub(crate) fn decode_body(bytes: &[u8]) -> Result<Self, WireError> {
        let mut cur = Cursor::new(bytes);
        let flags = cur.u16_le()?;

        let occupied = decode_bitset::<G>(&mut cur)?;
        let mut board = Board::<G>::empty();
        for sq in occupied {
            let byte = cur.u8()?;
            let color = if byte & 0x80 != 0 {
                Color::Black
            } else {
                Color::White
            };
            let role_ix = byte & 0x7f;
            let role = WideRole::from_index(role_ix as usize).ok_or(WireError::BadRole(role_ix))?;
            board.set_piece(sq, WidePiece::new(color, role));
        }

        let ep_square = if flags & F_EP != 0 {
            Some(read_square::<G>(&mut cur)?)
        } else {
            None
        };

        let mut castling = GenericCastling::NONE;
        if flags & F_CASTLING != 0 {
            for color in [Color::White, Color::Black] {
                for side in 0..2usize {
                    let file = cur.u8()?;
                    castling.set(color, side, if file == 255 { None } else { Some(file) });
                }
            }
        }

        let gating = if flags & F_GATING != 0 {
            let eligible = decode_bitset::<G>(&mut cur)?;
            let reserves = cur.u8()?;
            GenericGating::new(
                eligible,
                [reserves & 0x01 != 0, reserves & 0x02 != 0],
                [reserves & 0x04 != 0, reserves & 0x08 != 0],
            )
        } else {
            GenericGating::NONE
        };

        let placement = if flags & F_HAND != 0 {
            let white = decode_hand(&mut cur)?;
            let black = decode_hand(&mut cur)?;
            GenericPlacement::new(white, black)
        } else {
            GenericPlacement::NONE
        };

        let duck = if flags & F_DUCK != 0 {
            Some(read_square::<G>(&mut cur)?)
        } else {
            None
        };

        let promoted = if flags & F_PROMOTED != 0 {
            decode_bitset::<G>(&mut cur)?
        } else {
            Bitboard::EMPTY
        };

        let board_b = if flags & F_BOARD_B != 0 {
            decode_bitset::<G>(&mut cur)?
        } else {
            Bitboard::EMPTY
        };

        let consecutive_passes = if flags & F_PASSES != 0 { cur.u8()? } else { 0 };

        let (halfmove_clock, fullmove_number) = if flags & F_CLOCKS != 0 {
            let h = cur.varint()?;
            let f = cur.varint()?;
            (
                u16::try_from(h).map_err(|_| WireError::BadValue)?,
                u16::try_from(f).map_err(|_| WireError::BadValue)?,
            )
        } else {
            (0, 1)
        };

        if !cur.is_empty() {
            return Err(WireError::TrailingData);
        }

        let state = GenericState {
            turn: if flags & F_TURN_BLACK != 0 {
                Color::Black
            } else {
                Color::White
            },
            castling,
            ep_square,
            gating,
            duck,
            placement,
            halfmove_clock,
            fullmove_number,
            consecutive_passes,
            board_b,
        };
        let mut pos = Self::from_parts(board, state);
        pos.set_promoted(promoted);
        Ok(pos)
    }
}

/// Reads a square byte, validating it against the geometry.
#[inline]
fn read_square<G: Geometry>(cur: &mut Cursor<'_>) -> Result<Square<G>, WireError> {
    let index = cur.u8()?;
    Square::<G>::try_new(index).ok_or(WireError::BadSquare(index))
}

/// Encodes a **game record**: a start [`AnyWideVariant`] position and the move
/// list played from it. The layout is [`TAG_GAME`], the byte length of the
/// embedded position, the position ([`AnyWideVariant::to_bytes`]), the move count,
/// then each move (`encode_move_body`). The inverse is [`decode_game`].
#[must_use]
pub fn encode_game(start: &AnyWideVariant, moves: &[WideMove]) -> Vec<u8> {
    let position = start.to_bytes();
    let mut out = Vec::with_capacity(position.len() + moves.len() * 3 + 8);
    out.push(TAG_GAME);
    push_varint(position.len() as u64, &mut out);
    out.extend_from_slice(&position);
    push_varint(moves.len() as u64, &mut out);
    for &mv in moves {
        encode_move_body(mv, &mut out);
    }
    out
}

/// Decodes a game record written by [`encode_game`], returning the start
/// position and its move list.
///
/// # Errors
///
/// Returns [`WireError`] for truncated, wrong-tagged, trailing, or out-of-range
/// input — never panicking.
pub fn decode_game(bytes: &[u8]) -> Result<(AnyWideVariant, Vec<WideMove>), WireError> {
    let (&tag, rest) = bytes.split_first().ok_or(WireError::Truncated)?;
    if tag != TAG_GAME {
        return Err(WireError::BadTag(tag));
    }
    let mut cur = Cursor::new(rest);
    let position_len = usize::try_from(cur.varint()?).map_err(|_| WireError::BadValue)?;
    let position = cur.take(position_len)?;
    let start = AnyWideVariant::from_bytes(position)?;
    let move_count = usize::try_from(cur.varint()?).map_err(|_| WireError::BadValue)?;
    // Cap the initial reservation so an adversarial count cannot force a huge
    // allocation; the vector still grows as real moves are decoded.
    let mut moves = Vec::with_capacity(move_count.min(1024));
    for _ in 0..move_count {
        moves.push(decode_move_body(&mut cur)?);
    }
    if !cur.is_empty() {
        return Err(WireError::TrailingData);
    }
    Ok((start, moves))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Chess8x8, Seirawan, Shogi, Square, WideMoveKind, WideVariantId};

    #[test]
    fn move_round_trips_through_bytes() {
        let from = Square::<Chess8x8>::new(12);
        let to = Square::<Chess8x8>::new(28);
        let m = WideMove::new(from, to, WideMoveKind::DoublePawnPush);
        let bytes = m.to_bytes();
        assert_eq!(WideMove::from_bytes(&bytes), Ok(m));
        // The trimmed packed word stays small (tag + length + up to 8 value bytes).
        assert!(bytes.len() <= 6);
    }

    #[test]
    fn move_with_every_addendum_round_trips() {
        let base = WideMove::new(
            Square::<Chess8x8>::new(4),
            Square::<Chess8x8>::new(6),
            WideMoveKind::CastleKingside,
        );
        let gated = base.with_gate(
            super::GateRole::Hawk,
            crate::geometry::GateSquare::RookOrigin,
        );
        let bytes = gated.to_bytes();
        assert_eq!(WideMove::from_bytes(&bytes), Ok(gated));
    }

    #[test]
    fn malformed_move_input_is_rejected_without_panic() {
        assert_eq!(WideMove::from_bytes(&[]), Err(WireError::Truncated));
        assert_eq!(WideMove::from_bytes(&[0x00]), Err(WireError::BadTag(0x00)));
        // Tag present but length claims 9 value bytes (over the 8-byte word).
        assert_eq!(
            WideMove::from_bytes(&[TAG_MOVE, 9, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            Err(WireError::BadValue)
        );
        // Length claims more than is present.
        assert_eq!(
            WideMove::from_bytes(&[TAG_MOVE, 4, 1, 2]),
            Err(WireError::Truncated)
        );
        // Trailing byte after a complete move.
        assert_eq!(
            WideMove::from_bytes(&[TAG_MOVE, 1, 0x10, 0xFF]),
            Err(WireError::TrailingData)
        );
    }

    #[test]
    fn typed_position_round_trips_via_to_fen() {
        let pos = GenericPosition::<Chess8x8, crate::geometry::StandardChess>::startpos();
        let bytes = pos.to_bytes();
        let back = GenericPosition::<Chess8x8, crate::geometry::StandardChess>::from_bytes(&bytes)
            .unwrap();
        assert_eq!(back.to_fen(), pos.to_fen());
        // Smaller than the FEN it replaces.
        assert!(bytes.len() < pos.to_fen().len());
    }

    #[test]
    fn malformed_position_input_is_rejected_without_panic() {
        // `GenericPosition` has no `PartialEq`, so match on the error directly.
        type Pos = GenericPosition<Chess8x8, crate::geometry::StandardChess>;
        assert!(matches!(Pos::from_bytes(&[]), Err(WireError::Truncated)));
        assert!(matches!(
            Pos::from_bytes(&[0xEE, 0, 0]),
            Err(WireError::BadTag(0xEE))
        ));
        // Truncated body (tag + partial flags).
        assert!(Pos::from_bytes(&[TAG_POSITION, 0]).is_err());
    }

    #[test]
    fn gating_and_hand_positions_round_trip() {
        // `Seirawan` and `Shogi` are concrete position aliases: the former
        // exercises the gating section, the latter the hand pocket.
        let seirawan = Seirawan::startpos();
        let sbytes = seirawan.to_bytes();
        assert_eq!(
            Seirawan::from_bytes(&sbytes).unwrap().to_fen(),
            seirawan.to_fen()
        );

        let shogi = Shogi::startpos();
        let shbytes = shogi.to_bytes();
        assert_eq!(
            Shogi::from_bytes(&shbytes).unwrap().to_fen(),
            shogi.to_fen()
        );
    }

    #[test]
    fn any_variant_and_game_record_round_trip() {
        let start = AnyWideVariant::startpos(WideVariantId::Duck);
        let mut moves = Vec::new();
        let mut cursor = start.clone();
        for _ in 0..4 {
            let Some(mv) = cursor.legal_moves().into_iter().next() else {
                break;
            };
            moves.push(mv);
            cursor = cursor.play(&mv);
        }
        let bytes = encode_game(&start, &moves);
        let (decoded_start, decoded_moves) = decode_game(&bytes).unwrap();
        assert_eq!(decoded_start.to_fen(), start.to_fen());
        assert_eq!(decoded_moves, moves);
        // Replaying the decoded moves reaches the same final position.
        let mut replay = decoded_start;
        for mv in &decoded_moves {
            replay = replay.play(mv);
        }
        assert_eq!(replay.to_fen(), cursor.to_fen());
    }
}
