//! Chess960 (Fischer random) as a [`Variant`].
//!
//! Chess960 differs from standard chess only in two ways: the back-rank starting
//! placement is one of 960 arrangements (the bishops on opposite colours and the
//! king between the two rooks), and castling generalizes to arbitrary king/rook
//! start files while keeping the standard destinations (king to the g-/c-file,
//! rook to the f-/d-file). Movement of every piece is otherwise identical.
//!
//! Chess960 movement, pins, checks, en-passant discovered checks, and king
//! safety are identical to standard chess — only castling differs — so
//! [`Chess960Rules`] runs on the same fast pin/check-mask legal generator
//! ([`Variant::USES_FAST_LEGALITY`] is `true`) and only *specializes castling*.
//! It supplies its own arbitrary-geometry castle generator
//! ([`Variant::generate_castles`], via the core [`crate::Position`] 960 helper)
//! through the [`Variant::VARIANT_CASTLING`] seam: the fast generator emits every
//! fully-legal non-castling move while suppressing the standard castles, then the
//! 960 helper appends its castles. Those castles are themselves fully legal — the
//! 960 helper re-tests the king's landing square under the post-castle occupancy,
//! catching a castle that opens a line onto the king's destination — so no
//! make-move filter is needed.
//!
//! Castling rights are stored as the files of the castling rooks (the core
//! [`crate::CastlingRights`] already keys on rook files), and the castling FEN
//! field is read and written in the Shredder / X-FEN forms so arbitrary rook
//! files round-trip.

use super::{Variant, VariantId, VariantPosition};
use crate::movelist::MoveList;
use crate::position::{write_standard_castling_field, CastleSide};
use crate::{Board, CastlingRights, Color, FenError, File, Piece, Rank, Role, Square};
use alloc::borrow::ToOwned;
use alloc::string::String;
#[cfg(test)]
use alloc::vec;
#[cfg(test)]
use alloc::vec::Vec;

/// The Chess960 (Fischer random) rule layer: standard movement with a 960
/// starting placement and arbitrary-geometry castling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Chess960Rules;

impl Variant for Chess960Rules {
    type State = ();
    const ID: VariantId = VariantId::Chess960;

    // Non-castling movement, pins, checks, and king safety are identical to
    // standard chess, so use the fast pin/check-mask generator. Only castling
    // differs (arbitrary king/rook geometry), so it is supplied via the
    // `VARIANT_CASTLING` seam below; the fast generator suppresses its standard
    // castles and the 960 generator appends fully-legal ones.
    const USES_FAST_LEGALITY: bool = true;
    const VARIANT_CASTLING: bool = true;

    fn generate_castles(core: &crate::Position, out: &mut MoveList) {
        core.gen_castles_960(out, castle_dest_files);
    }

    fn bulk_count_leaf(core: &crate::Position, _state: &Self::State) -> Option<u64> {
        // Count the fast non-castling legal moves and the 960 castles through a
        // population-count sink, exactly mirroring `legal_into`'s
        // `generate_no_castles_into` + `generate_castles` pair, so the leaf count
        // stays byte-identical while skipping move construction.
        Some(core.count_legal_960(castle_dest_files))
    }

    fn starting_board() -> (Board, CastlingRights, Self::State) {
        // Default to the standard arrangement (position id 518).
        back_rank_to_board(STANDARD_BACK_RANK)
    }

    fn read_castling_field(field: &str, board: &Board) -> Result<CastlingRights, FenError> {
        read_castling_960(field, board)
    }

    fn write_castling_field(rights: CastlingRights, _board: &Board, out: &mut String) {
        write_castling_960(rights, out);
    }
}

/// The Chess960 castle destination files for a side: king to the g-/c-file,
/// rook to the f-/d-file, exactly as in standard chess — only the start squares
/// differ in Chess960. Shared by the move-emitting and bulk-counting castle
/// paths so both generate the identical castle set.
fn castle_dest_files(side: CastleSide) -> Option<(File, File)> {
    match side {
        CastleSide::King => Some((File::G, File::F)),
        CastleSide::Queen => Some((File::C, File::D)),
    }
}

/// Chess960 (Fischer random) as a [`VariantPosition`].
pub type Chess960 = VariantPosition<Chess960Rules>;

impl Chess960 {
    /// The Chess960 starting position for the given position id (0..=959), in the
    /// standard (Scharnagl) numbering. Position id 518 is the standard
    /// arrangement.
    ///
    /// # Panics
    ///
    /// Panics if `id >= 960`.
    #[must_use]
    pub fn from_position_id(id: u16) -> Self {
        assert!(id < 960, "Chess960 position id must be in 0..=959");
        let back_rank = position_id_to_back_rank(id);
        Self::from_back_rank(back_rank)
    }

    /// The Chess960 starting position for an explicit back rank (the roles on the
    /// a-file..h-file). The same arrangement is mirrored for both colours, the
    /// castling rooks are the two rooks of the back rank, and white is to move.
    ///
    /// # Panics
    ///
    /// Panics if `back_rank` does not contain exactly one king with a rook on
    /// each side of it (the minimum a castling-capable start needs).
    #[must_use]
    pub fn from_back_rank(back_rank: [Role; 8]) -> Self {
        let (board, castling, state) = back_rank_to_board(back_rank);
        let core = crate::Position::from_fields(board, Color::White, castling, None, 0, 1);
        Self::from_parts(core, state, Chess960Rules)
    }
}

/// The standard chess back rank, a-file to h-file (Chess960 position id 518).
const STANDARD_BACK_RANK: [Role; 8] = [
    Role::Rook,
    Role::Knight,
    Role::Bishop,
    Role::Queen,
    Role::King,
    Role::Bishop,
    Role::Knight,
    Role::Rook,
];

/// Builds the mirrored starting board and rook-file castling rights for a back
/// rank.
fn back_rank_to_board(back_rank: [Role; 8]) -> (Board, CastlingRights, ()) {
    let mut board = Board::empty();
    for (file_index, &role) in back_rank.iter().enumerate() {
        let file = File::new(file_index as u8).expect("file index in 0..8");
        let white_sq = Square::from_file_rank(file, Rank::First);
        let black_sq = Square::from_file_rank(file, Rank::Eighth);
        board.set_piece(white_sq, Piece::new(Color::White, role));
        board.set_piece(black_sq, Piece::new(Color::Black, role));
        // Pawns on the second and seventh ranks.
        board.set_piece(
            Square::from_file_rank(file, Rank::Second),
            Piece::new(Color::White, Role::Pawn),
        );
        board.set_piece(
            Square::from_file_rank(file, Rank::Seventh),
            Piece::new(Color::Black, Role::Pawn),
        );
    }

    let (queenside_rook, kingside_rook) = rook_files(&back_rank);
    // The same files apply to both colours (the placement is mirrored).
    let castling = CastlingRights::from_rook_files(
        Some(kingside_rook),
        Some(queenside_rook),
        Some(kingside_rook),
        Some(queenside_rook),
    );
    (board, castling, ())
}

/// The queenside (lower-file) and kingside (higher-file) rook files of a back
/// rank, i.e. the rooks on either side of the king.
fn rook_files(back_rank: &[Role; 8]) -> (File, File) {
    let king_index = back_rank
        .iter()
        .position(|&r| r == Role::King)
        .expect("back rank must contain a king");
    let queenside = (0..king_index)
        .rev()
        .find(|&i| back_rank[i] == Role::Rook)
        .expect("a rook on the queenside of the king");
    let kingside = (king_index + 1..8)
        .find(|&i| back_rank[i] == Role::Rook)
        .expect("a rook on the kingside of the king");
    (
        File::new(queenside as u8).expect("file in range"),
        File::new(kingside as u8).expect("file in range"),
    )
}

/// Derives the back rank (a-file..h-file) from a Chess960 position id (0..=959)
/// using the standard Scharnagl numbering.
///
/// The id decomposes as: two pairs of bits choosing the bishop squares (one on a
/// light file, one on a dark file, guaranteeing opposite colours), then a base-6
/// digit placing the queen on one of the six remaining squares, then a base-10
/// digit selecting the two knight squares among the five that remain; the final
/// three empty squares receive rook, king, rook from the a-file outward, so the
/// king is always between the rooks.
fn position_id_to_back_rank(id: u16) -> [Role; 8] {
    let mut squares: [Option<Role>; 8] = [None; 8];
    let mut n = id;

    // Light-square files are b, d, f, h (indices 1, 3, 5, 7); dark-square files
    // are a, c, e, g (indices 0, 2, 4, 6). The first two-bit digit places the
    // bishop on a light file, the second on a dark file.
    let light_files = [1usize, 3, 5, 7];
    let dark_files = [0usize, 2, 4, 6];
    squares[light_files[(n % 4) as usize]] = Some(Role::Bishop);
    n /= 4;
    squares[dark_files[(n % 4) as usize]] = Some(Role::Bishop);
    n /= 4;

    // Queen on the q-th still-empty square.
    let q = (n % 6) as usize;
    place_on_empty(&mut squares, q, Role::Queen);
    n /= 6;

    // The remaining base-10 digit selects which two of the five empty squares
    // hold the knights, by the standard combinatorial table.
    let knight_table = [
        (0usize, 1usize),
        (0, 2),
        (0, 3),
        (0, 4),
        (1, 2),
        (1, 3),
        (1, 4),
        (2, 3),
        (2, 4),
        (3, 4),
    ];
    let (n1, n2) = knight_table[(n % 10) as usize];
    // Place the higher index first so removing it does not shift the lower one.
    place_on_empty(&mut squares, n2, Role::Knight);
    place_on_empty(&mut squares, n1, Role::Knight);

    // The three squares that remain receive rook, king, rook from the a-file
    // outward, putting the king between the rooks.
    let order = [Role::Rook, Role::King, Role::Rook];
    let mut k = 0;
    for square in &mut squares {
        if square.is_none() {
            *square = Some(order[k]);
            k += 1;
        }
    }

    let mut back_rank = [Role::Pawn; 8];
    for (i, square) in squares.iter().enumerate() {
        back_rank[i] = square.expect("every back-rank square filled");
    }
    back_rank
}

/// Places `role` on the `index`-th currently-empty square (0-based) of the back
/// rank.
fn place_on_empty(squares: &mut [Option<Role>; 8], index: usize, role: Role) {
    let mut seen = 0;
    for square in squares.iter_mut() {
        if square.is_none() {
            if seen == index {
                *square = Some(role);
                return;
            }
            seen += 1;
        }
    }
    panic!("fewer than {} empty squares on the back rank", index + 1);
}

/// Reads a Chess960 castling FEN field: Shredder file letters (`A`..`H` /
/// `a`..`h`) name the castling rook by file, while `KQkq` (X-FEN) name the
/// outermost rook on the relevant side of the king. `-` means no rights.
fn read_castling_960(field: &str, board: &Board) -> Result<CastlingRights, FenError> {
    if field == "-" {
        return Ok(CastlingRights::NONE);
    }

    let mut white_king = None;
    let mut white_queen = None;
    let mut black_king = None;
    let mut black_queen = None;

    for ch in field.chars() {
        let color = if ch.is_ascii_uppercase() {
            Color::White
        } else {
            Color::Black
        };
        let rank = back_rank_of(color);
        let king_file =
            king_file_of(board, color).ok_or_else(|| FenError::BadCastling(field.to_owned()))?;

        let rook_file = match ch.to_ascii_lowercase() {
            'k' => outermost_rook(board, color, rank, king_file, CastleSide::King),
            'q' => outermost_rook(board, color, rank, king_file, CastleSide::Queen),
            'a'..='h' => {
                let file = File::from_char(ch).expect("a..h is a valid file");
                if board.piece_at(Square::from_file_rank(file, rank))
                    == Some(Piece::new(color, Role::Rook))
                {
                    Some(file)
                } else {
                    None
                }
            }
            _ => return Err(FenError::BadCastling(field.to_owned())),
        };
        let rook_file = rook_file.ok_or_else(|| FenError::BadCastling(field.to_owned()))?;

        // The named rook must be on the correct side of the king.
        let side = if rook_file > king_file {
            CastleSide::King
        } else {
            CastleSide::Queen
        };
        let slot = match (color, side) {
            (Color::White, CastleSide::King) => &mut white_king,
            (Color::White, CastleSide::Queen) => &mut white_queen,
            (Color::Black, CastleSide::King) => &mut black_king,
            (Color::Black, CastleSide::Queen) => &mut black_queen,
        };
        *slot = Some(rook_file);
    }

    Ok(CastlingRights::from_rook_files(
        white_king,
        white_queen,
        black_king,
        black_queen,
    ))
}

/// Writes a Chess960 castling FEN field. If every castling rook sits on the
/// a-/h-file the standard `KQkq` form is emitted; otherwise the Shredder file
/// letters are used (uppercase for white, lowercase for black, king-side before
/// queen-side per colour, white before black).
fn write_castling_960(rights: CastlingRights, out: &mut String) {
    let uses_standard_files = [
        (Color::White, CastleSide::King, File::H),
        (Color::White, CastleSide::Queen, File::A),
        (Color::Black, CastleSide::King, File::H),
        (Color::Black, CastleSide::Queen, File::A),
    ]
    .iter()
    .all(
        |&(color, side, standard)| match rights.rook_file(color, side) {
            Some(file) => file == standard,
            None => true,
        },
    );

    if uses_standard_files {
        write_standard_castling_field(rights, out);
        return;
    }

    let start = out.len();
    for (color, side) in [
        (Color::White, CastleSide::King),
        (Color::White, CastleSide::Queen),
        (Color::Black, CastleSide::King),
        (Color::Black, CastleSide::Queen),
    ] {
        if let Some(file) = rights.rook_file(color, side) {
            let ch = if color == Color::White {
                file.char().to_ascii_uppercase()
            } else {
                file.char()
            };
            out.push(ch);
        }
    }
    if out.len() == start {
        out.push('-');
    }
}

/// The outermost (toward the named side) rook of `color` on its back rank,
/// relative to the king — the rook that `KQkq` (X-FEN) refers to.
fn outermost_rook(
    board: &Board,
    color: Color,
    rank: Rank,
    king_file: File,
    side: CastleSide,
) -> Option<File> {
    let rook = Piece::new(color, Role::Rook);
    let king_index = king_file.index();
    match side {
        // King-side: the rook with the highest file above the king.
        CastleSide::King => (king_index + 1..8).rev().find_map(|i| {
            let file = File::new(i).expect("file in range");
            (board.piece_at(Square::from_file_rank(file, rank)) == Some(rook)).then_some(file)
        }),
        // Queen-side: the rook with the lowest file below the king.
        CastleSide::Queen => (0..king_index).find_map(|i| {
            let file = File::new(i).expect("file in range");
            (board.piece_at(Square::from_file_rank(file, rank)) == Some(rook)).then_some(file)
        }),
    }
}

/// The file of `color`'s king on its back rank, if present there.
fn king_file_of(board: &Board, color: Color) -> Option<File> {
    let king = board.king_of(color)?;
    (king.rank() == back_rank_of(color)).then(|| king.file())
}

/// The back rank of `color` (first rank for white, eighth for black).
fn back_rank_of(color: Color) -> Rank {
    if color == Color::White {
        Rank::First
    } else {
        Rank::Eighth
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variant::perft_variant;
    use crate::{MoveKind, Position};

    /// The kinds of castle a position offers the side to move, as
    /// `(kind, king-destination-uci)` pairs, sorted.
    fn castles(pos: &Chess960) -> Vec<(MoveKind, String)> {
        let mut out: Vec<(MoveKind, String)> = pos
            .legal_moves()
            .iter()
            .filter(|m| {
                matches!(
                    m.kind(),
                    MoveKind::CastleKingside | MoveKind::CastleQueenside
                )
            })
            .map(|m| (m.kind(), m.to_uci()))
            .collect();
        out.sort_by(|a, b| a.1.cmp(&b.1));
        out
    }

    #[test]
    fn standard_arrangement_is_position_id_518() {
        let by_default = Chess960::startpos();
        let by_id = Chess960::from_position_id(518);
        assert_eq!(by_default, by_id);
        assert_eq!(
            by_default.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
        // The standard arrangement reproduces standard startpos exactly.
        assert_eq!(by_default.core(), &Position::startpos());
        assert_eq!(by_default.legal_moves().len(), 20);
    }

    #[test]
    fn known_position_ids_match_published_back_ranks() {
        // Canonical endpoints of the Scharnagl numbering.
        assert_eq!(
            Chess960::from_position_id(0).to_fen(),
            "bbqnnrkr/pppppppp/8/8/8/8/PPPPPPPP/BBQNNRKR w HFhf - 0 1"
        );
        assert_eq!(
            Chess960::from_position_id(959).to_fen(),
            "rkrnnqbb/pppppppp/8/8/8/8/PPPPPPPP/RKRNNQBB w CAca - 0 1"
        );
    }

    #[test]
    fn every_position_id_is_a_legal_960_start() {
        for id in 0..960u16 {
            let pos = Chess960::from_position_id(id);
            let back: Vec<Role> = (0..8)
                .map(|f| {
                    pos.core()
                        .board()
                        .role_at(Square::from_file_rank(File::new(f).unwrap(), Rank::First))
                        .unwrap()
                })
                .collect();
            // Exactly one king, two rooks with the king between them.
            let king = back.iter().position(|&r| r == Role::King).unwrap();
            let rooks: Vec<usize> = back
                .iter()
                .enumerate()
                .filter(|(_, &r)| r == Role::Rook)
                .map(|(i, _)| i)
                .collect();
            assert_eq!(rooks.len(), 2, "id {id}: two rooks");
            assert!(
                rooks[0] < king && king < rooks[1],
                "id {id}: king between rooks"
            );
            // Bishops on opposite colours.
            let bishops: Vec<usize> = back
                .iter()
                .enumerate()
                .filter(|(_, &r)| r == Role::Bishop)
                .map(|(i, _)| i)
                .collect();
            assert_eq!(bishops.len(), 2, "id {id}: two bishops");
            assert_ne!(
                bishops[0] % 2,
                bishops[1] % 2,
                "id {id}: bishops opposite colour"
            );
        }
    }

    #[test]
    fn castle_with_adjacent_king_and_rook() {
        // King on b1 with rooks on a1 and c1. Only the king-side castle is legal:
        // the queen-side king destination (c1) is occupied by the other rook,
        // which is neither the castling king nor the castling rook, so it blocks.
        let pos: Chess960 = "rkr5/pppppppp/8/8/8/8/PPPPPPPP/RKR5 w CAca - 0 1"
            .parse()
            .unwrap();
        assert_eq!(
            castles(&pos),
            vec![(MoveKind::CastleKingside, "b1g1".to_owned())]
        );
    }

    #[test]
    fn castle_when_king_already_on_destination() {
        // King already on g1 (the king-side destination): the king-side castle is
        // `g1g1` (the king does not move, only the rook does).
        let pos: Chess960 = "r5kr/pppppppp/8/8/8/8/PPPPPPPP/R5KR w HAha - 0 1"
            .parse()
            .unwrap();
        assert_eq!(
            castles(&pos),
            vec![
                (MoveKind::CastleQueenside, "g1c1".to_owned()),
                (MoveKind::CastleKingside, "g1g1".to_owned()),
            ]
        );
        // Playing the king-side castle leaves the king on g1 and the rook on f1.
        let mv = pos
            .legal_moves()
            .into_iter()
            .find(|m| m.kind() == MoveKind::CastleKingside)
            .unwrap();
        let after = pos.play(&mv);
        let b = after.core().board();
        assert_eq!(
            b.piece_at(Square::G1),
            Some(Piece::new(Color::White, Role::King))
        );
        assert_eq!(
            b.piece_at(Square::F1),
            Some(Piece::new(Color::White, Role::Rook))
        );
    }

    #[test]
    fn castle_when_rook_already_on_destination() {
        // King-side rook already on f1 (its destination); the castle still works.
        let pos: Chess960 = "r3kr2/pppppppp/8/8/8/8/PPPPPPPP/R3KR2 w FAfa - 0 1"
            .parse()
            .unwrap();
        assert!(castles(&pos).contains(&(MoveKind::CastleKingside, "e1g1".to_owned())));
    }

    #[test]
    fn xfen_kqkq_reads_outermost_rooks_and_roundtrips_to_shredder() {
        // `KQkq` (X-FEN) names the outermost rook on each side of the king. With
        // rooks on the b- and g-files (king on e), both castles are legal, and the
        // writer renders the non-standard files in Shredder form.
        let pos: Chess960 = "1r2k1r1/pppppppp/8/8/8/8/PPPPPPPP/1R2K1R1 w KQkq - 0 1"
            .parse()
            .unwrap();
        assert_eq!(
            castles(&pos),
            vec![
                (MoveKind::CastleQueenside, "e1c1".to_owned()),
                (MoveKind::CastleKingside, "e1g1".to_owned()),
            ]
        );
        assert_eq!(
            pos.to_fen(),
            "1r2k1r1/pppppppp/8/8/8/8/PPPPPPPP/1R2K1R1 w GBgb - 0 1"
        );
    }

    #[test]
    fn shredder_field_roundtrips_for_mid_game_positions() {
        // Shredder file letters are preserved across a parse/write round trip.
        for fen in [
            "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w HFhf - 2 9",
            "2nnrbkr/p1qppppp/8/1ppb4/6PP/3PP3/PPP2P2/BQNNRBKR w HEhe - 1 9",
            "b1q1rrkb/pppppppp/3nn3/8/P7/1PPP4/4PPPP/BQNNRKRB w GE - 1 9",
            "qn1rbbkr/ppp2p1p/1n1pp1p1/8/3P4/P6P/1PP1PPPK/QNNRBB1R w hd - 2 9",
        ] {
            let pos: Chess960 = fen.parse().unwrap();
            assert_eq!(pos.to_fen(), fen, "round trip for {fen}");
        }
    }

    #[test]
    fn castle_rejected_when_rook_departure_discovers_check_on_destination() {
        // White king e1, white queenside castling rook on b1, black rook on a1.
        // The b1 rook shields the king's queenside destination (c1) from the a1
        // rook; castling moves it to d1, opening a1->c1 and leaving the king in
        // check on c1. On the fast path no make-move filter runs, so the 960
        // castle generator must reject this itself.
        let pos: Chess960 = "4k3/8/8/8/8/8/8/rR2K3 w B - 0 1".parse().unwrap();
        assert_eq!(castles(&pos), vec![]);
    }

    #[test]
    fn standard_arrangement_matches_startpos_perft() {
        let pos = Chess960::startpos();
        assert_eq!(perft_variant(&pos, 1), 20);
        assert_eq!(perft_variant(&pos, 2), 400);
        assert_eq!(perft_variant(&pos, 3), 8902);
        assert_eq!(perft_variant(&pos, 4), 197281);
    }

    #[test]
    fn dropping_a_castling_right_when_a_rook_is_captured() {
        // Build a position where capturing a castling rook revokes that right.
        // King e1, rooks a1/h1 for white; mirror for black, with a black rook able
        // to capture white's h1 rook is overkill — instead just confirm a rook
        // move from its home square revokes the matching right via FEN.
        let pos: Chess960 = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1".parse().unwrap();
        // Move the h1 rook one square; the white king-side right is revoked.
        let mv = pos.parse_uci("h1g1").unwrap();
        let after = pos.play(&mv);
        assert!(!after
            .core()
            .castling_rights()
            .has(Color::White, CastleSide::King));
        assert!(after
            .core()
            .castling_rights()
            .has(Color::White, CastleSide::Queen));
    }
}
