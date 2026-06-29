//! Cambodian chess / Ouk Chaktrang on the generic engine — a Makruk variant
//! with one-time first-move leaps for the king and the queen (Neang).
//!
//! Cambodian chess is identical to [`Makruk`](super::makruk::Makruk) — the same
//! [`Chess8x8`] geometry, the same Met (ferz), Khon (silver), single-step
//! promote-to-Met pawns, and the same counting endgame rule (terminal only, so it
//! never affects move generation; modelled in simplified board-honour form via
//! [`WideVariant::counting_rule`]) — **except** that, at most once per
//! side, the king and the queen/Met may use a special leap on their first move:
//!
//! * **King (Sdech)** — a one-time leap to either of the two **forward-knight**
//!   squares from its home square. It jumps over any intervening piece and may
//!   land only on an **empty** square (it never captures with the leap), and —
//!   like castling — the leap is offered only when the king is **not in check**.
//! * **Queen / Neang (Met)** — a one-time **two-square straight advance** from
//!   its home square. It jumps the square in front and may land only on an empty
//!   square; otherwise it is an ordinary piece move, so it is confined by the
//!   check mask and the Met's pin line.
//!
//! Each leap right is lost the first time its piece makes **any** move (it does
//! not return if the piece comes back home), exactly like a castling right.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant cambodian` / `position
//! startpos`:
//!
//! ```text
//! rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w DEde - 0 1
//! ```
//!
//! The array is the Makruk array; the two leap rights are encoded in the
//! castling field by the **home file letter** of each piece — uppercase for
//! white (`D` = king on the d-file, `E` = Met on the e-file), lowercase for
//! black (`e` = king on the e-file, `d` = Met on the d-file). FSF mirrors the
//! king/Met pair between the colors (white king on d, black king on e), so the
//! field reads `DEde`.
//!
//! ## Implementation
//!
//! Cambodian reuses the entire Makruk rule layer and adds the leaps behind the
//! default-off [`WideVariant::has_first_move_leaps`] hook. The two leap rights
//! are stored in the existing
//! [`GenericCastling`] field — the
//! **kingside** slot for the king (keyed to its home file) and the **queenside**
//! slot for the Met — so they are parsed, serialized, and revoked (on the
//! piece's first move) by the same machinery as a castling right.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::variants::makruk::MakrukRules;
use crate::geometry::{
    Bitboard, Board, Chess8x8, Geometry, PromotionConfig, Square, WideRole, WideVariant,
};
use crate::Color;

/// The Cambodian rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// Every movement / promotion / pawn rule is delegated to [`MakrukRules`]; the
/// only additions are the one-time king and Met leaps, enabled through the
/// [`WideVariant::has_first_move_leaps`] hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CambodianRules;

/// The confirmed Cambodian starting array (the Makruk array), validated against
/// Fairy-Stockfish `UCI_Variant cambodian`.
const CAMBODIAN_START_PLACEMENT: &str = "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR";

/// The kingside castling slot index (reused to carry the king's leap right).
const KINGSIDE: usize = 0;
/// The queenside castling slot index (reused to carry the Met's leap right).
const QUEENSIDE: usize = 1;

/// The king's home file by color: white on the d-file (3), black on the e-file
/// (4) — the FSF mirror of the king/Met pair.
const fn king_home_file(color: Color) -> u8 {
    match color {
        Color::White => 3,
        Color::Black => 4,
    }
}

/// The Met's home file by color: white on the e-file (4), black on the d-file
/// (3).
const fn met_home_file(color: Color) -> u8 {
    match color {
        Color::White => 4,
        Color::Black => 3,
    }
}

/// The king's one-time forward-knight leap offsets, color-relative.
const KING_LEAP_WHITE: [(i8, i8); 2] = [(-2, 1), (2, 1)];
const KING_LEAP_BLACK: [(i8, i8); 2] = [(-2, -1), (2, -1)];

/// The Met's one-time two-square straight advance, color-relative.
const MET_LEAP_WHITE: [(i8, i8); 1] = [(0, 2)];
const MET_LEAP_BLACK: [(i8, i8); 1] = [(0, -2)];

impl WideVariant<Chess8x8> for CambodianRules {
    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(CAMBODIAN_START_PLACEMENT)
            .expect("the Cambodian starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            // The two leap rights, carried in the castling field: the kingside
            // slot for each king (its home file) and the queenside slot for each
            // Met. This is the `DEde` field of the start FEN.
            castling: starting_leap_rights(),
            ep_square: None,
            gating: GenericGating::NONE,
            duck: None,
            placement: GenericPlacement::NONE,
            halfmove_clock: 0,
            fullmove_number: 1,
            consecutive_passes: 0,
            board_b: crate::geometry::Bitboard::EMPTY,
        };
        (board, state)
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Chess8x8>,
        occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        // The static movement of every piece is exactly Makruk's; the leaps are
        // dynamic (right-gated, home-square-gated) and emitted by the generic
        // generator, not by this static attack relation.
        MakrukRules::role_attacks(role, color, sq, occupancy)
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        MakrukRules::role_attack_is_directional(role)
    }

    fn promotion_config() -> PromotionConfig {
        MakrukRules::promotion_config()
    }

    fn promotion_rank(color: Color) -> u8 {
        MakrukRules::promotion_rank(color)
    }

    fn double_push_rank(color: Color) -> u8 {
        MakrukRules::double_push_rank(color)
    }

    fn has_castling() -> bool {
        // Cambodian has no castling; the castling-rights field is repurposed for
        // the leap rights, which are emitted by the first-move-leap path instead.
        false
    }

    fn has_first_move_leaps() -> bool {
        true
    }

    fn king_leap_offsets(color: Color) -> &'static [(i8, i8)] {
        match color {
            Color::White => &KING_LEAP_WHITE,
            Color::Black => &KING_LEAP_BLACK,
        }
    }

    fn met_leap_offsets(color: Color) -> &'static [(i8, i8)] {
        match color {
            Color::White => &MET_LEAP_WHITE,
            Color::Black => &MET_LEAP_BLACK,
        }
    }

    fn parse_first_move_rights(field: &str) -> Option<GenericCastling> {
        parse_leap_rights(field)
    }

    fn write_first_move_rights(rights: GenericCastling, out: &mut alloc::string::String) {
        write_leap_rights(rights, out);
    }

    fn counting_rule() -> bool {
        // Cambodian (Ouk Chaktrang) shares Makruk's board-honour counting endgame
        // (simplified; see [`GenericGame`](crate::geometry::game::GenericGame)).
        // Terminal-only, so perft is byte-identical.
        true
    }
}

/// Builds the full set of leap rights for a fresh game: both kings (kingside
/// slot) and both Mets (queenside slot) hold their leap.
fn starting_leap_rights() -> GenericCastling {
    let mut rights = GenericCastling::NONE;
    for color in [Color::White, Color::Black] {
        rights.set(color, KINGSIDE, Some(king_home_file(color)));
        rights.set(color, QUEENSIDE, Some(met_home_file(color)));
    }
    rights
}

/// Parses the Cambodian leap-rights field (the `DEde`-style file-letter field in
/// the FEN's castling slot) into the [`GenericCastling`] rights.
///
/// Each letter names a **home file**: an uppercase letter is a white right, a
/// lowercase letter a black right. A king-file letter sets the kingside slot, a
/// Met-file letter the queenside slot. `-` means no rights remain. Returns
/// `None` on any unrecognized letter.
fn parse_leap_rights(field: &str) -> Option<GenericCastling> {
    let mut rights = GenericCastling::NONE;
    if field == "-" {
        return Some(rights);
    }
    for ch in field.chars() {
        let color = if ch.is_ascii_uppercase() {
            Color::White
        } else {
            Color::Black
        };
        let file = file_of_letter(ch)?;
        let side = if file == king_home_file(color) {
            KINGSIDE
        } else if file == met_home_file(color) {
            QUEENSIDE
        } else {
            // A file letter that is neither this color's king nor Met home is not
            // a valid Cambodian leap right.
            return None;
        };
        rights.set(color, side, Some(file));
    }
    Some(rights)
}

/// Serializes the Cambodian leap-rights field, matching the FSF canonical order
/// (`DEde`): all of white's rights first then all of black's, and within each
/// color in ascending file order. Writes `-` if no rights remain.
fn write_leap_rights(rights: GenericCastling, out: &mut alloc::string::String) {
    let before = out.len();
    for color in [Color::White, Color::Black] {
        // Collect this color's two rights as (file, letter) and emit in ascending
        // file order — for Cambodian the king and Met sit on adjacent files (d/e),
        // mirrored between the colors, so this reproduces `DE` for white and `de`
        // for black.
        let mut files: alloc::vec::Vec<u8> = [KINGSIDE, QUEENSIDE]
            .into_iter()
            .filter_map(|side| rights.rook_file(color, side))
            .collect();
        files.sort_unstable();
        for file in files {
            out.push(letter_of_file(file, color));
        }
    }
    if out.len() == before {
        out.push('-');
    }
}

/// Maps a file letter (`a`..`h`, case-insensitive) to its 0-based file index,
/// rejecting any letter outside the board width.
fn file_of_letter(ch: char) -> Option<u8> {
    let lower = ch.to_ascii_lowercase();
    if !lower.is_ascii_lowercase() {
        return None;
    }
    let file = (lower as u8) - b'a';
    if (file as usize) < Chess8x8::WIDTH as usize {
        Some(file)
    } else {
        None
    }
}

/// Maps a 0-based file index to its FEN letter, uppercase for white, lowercase
/// for black.
fn letter_of_file(file: u8, color: Color) -> char {
    let base = b'a' + file;
    match color {
        Color::White => base.to_ascii_uppercase() as char,
        Color::Black => base as char,
    }
}

/// Cambodian chess (Ouk Chaktrang) as a [`GenericPosition`] over the 8x8
/// geometry.
///
/// Construct the starting position with
/// [`Cambodian::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Cambodian::from_fen`](GenericPosition::from_fen).
pub type Cambodian = GenericPosition<Chess8x8, CambodianRules>;
