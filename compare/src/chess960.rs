//! Chess960 (Fischer-random) start-position generation by Scharnagl id.
//!
//! For the chess960 basket we want games starting from many of the 960 legal
//! back-rank arrangements, not just one. Each arrangement has a canonical id in
//! `0..960` (the *Scharnagl* / SP-number scheme used by FICS, Lichess, and the
//! chess960 literature). This module maps an id to the back-rank placement and
//! emits a full start FEN with X-FEN castling rights (file letters for the rook
//! files, e.g. `HAha`), which is exactly the format both mce's `Chess960` and
//! shakmaty's `Chess` (in [`CastlingMode::Chess960`]) parse.
//!
//! The placement algorithm is the standard one:
//!
//! 1. Split the id: `n = id`. The two bishops go on opposite-coloured squares —
//!    `n % 4` selects the light-square bishop file, `n / 4 % 4` the dark-square
//!    bishop file.
//! 2. `n2 = n / 16` picks the queen's file among the four remaining empty files.
//! 3. The last digit `n2` (0..=9) selects which of the ten knight-pair
//!    arrangements fills two of the three remaining files; the king and two
//!    rooks fill the final three files in order R K R (king between the rooks),
//!    which fixes the castling rook files.
//!
//! [`CastlingMode::Chess960`]: shakmaty::CastlingMode::Chess960

/// The ten ways to place the two knights among the remaining (post bishops +
/// queen) `n` empty files, indexed by the standard knight-table value 0..=9.
/// Each entry is the pair of *slot indices* (into the list of still-empty files)
/// the knights occupy.
const KNIGHT_TABLE: [(usize, usize); 10] = [
    (0, 1),
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

/// Compute the back-rank file layout (eight piece chars, files a..h) for a
/// Scharnagl id in `0..960`.
fn back_rank(id: u32) -> [u8; 8] {
    assert!(id < 960, "Scharnagl id out of range");
    let mut rank = [0u8; 8]; // 0 = empty, else piece char (uppercase)

    let n = id;
    // Light-square bishop: light squares are files 1,3,5,7 (b,d,f,h, 0-indexed
    // odd). n % 4 selects among the four.
    let light_files = [1usize, 3, 5, 7];
    rank[light_files[(n % 4) as usize]] = b'B';
    let n = n / 4;
    // Dark-square bishop: dark squares are files 0,2,4,6 (a,c,e,g).
    let dark_files = [0usize, 2, 4, 6];
    rank[dark_files[(n % 4) as usize]] = b'B';
    let n = n / 4;

    // Remaining empty files, left to right.
    let empties = |rank: &[u8; 8]| -> Vec<usize> { (0..8).filter(|&f| rank[f] == 0).collect() };

    // Queen into the (n % 6)-th remaining empty file.
    let empt = empties(&rank);
    rank[empt[(n % 6) as usize]] = b'Q';
    let n = n / 6;

    // Knights via the table into the now-five remaining empty files.
    let (k1, k2) = KNIGHT_TABLE[(n % 10) as usize];
    let empt = empties(&rank);
    rank[empt[k1]] = b'N';
    rank[empt[k2]] = b'N';

    // Final three empty files get rook, king, rook in order.
    let empt = empties(&rank);
    rank[empt[0]] = b'R';
    rank[empt[1]] = b'K';
    rank[empt[2]] = b'R';

    rank
}

/// Build a full chess960 start FEN for a Scharnagl id, with X-FEN castling
/// rights (uppercase rook files for White, lowercase for Black).
pub fn scharnagl_start_fen(id: u32) -> String {
    let rank = back_rank(id);

    // Piece placement: black back rank (lowercase), black pawns, four empty
    // ranks, white pawns, white back rank.
    let white: String = rank.iter().map(|&c| c as char).collect();
    let black: String = rank
        .iter()
        .map(|&c| c.to_ascii_lowercase() as char)
        .collect();

    // Castling rights: the two rook files. With king between the rooks, the
    // left rook is queenside, the right rook kingside; X-FEN names them by file
    // letter, kingside first by convention (the order does not matter to either
    // parser, but we emit H-side then A-side per the existing basket style).
    let rook_files: Vec<usize> = (0..8).filter(|&f| rank[f] == b'R').collect();
    let qfile = (b'A' + rook_files[0] as u8) as char;
    let kfile = (b'A' + rook_files[1] as u8) as char;
    let white_rights = format!("{kfile}{qfile}");
    let black_rights = white_rights.to_ascii_lowercase();

    format!("{black}/pppppppp/8/8/8/8/PPPPPPPP/{white} w {white_rights}{black_rights} - 0 1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id518_is_standard_chess() {
        // Scharnagl id 518 is the standard chess start position.
        let fen = scharnagl_start_fen(518);
        assert!(
            fen.starts_with("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w "),
            "id 518 should be standard chess, got {fen}"
        );
    }

    #[test]
    fn bishops_on_opposite_colors() {
        for id in 0..960 {
            let r = back_rank(id);
            let bishops: Vec<usize> = (0..8).filter(|&f| r[f] == b'B').collect();
            assert_eq!(bishops.len(), 2, "id {id}");
            assert_ne!(bishops[0] % 2, bishops[1] % 2, "id {id} bishops same color");
        }
    }

    #[test]
    fn king_between_rooks() {
        for id in 0..960 {
            let r = back_rank(id);
            let king = (0..8).find(|&f| r[f] == b'K').unwrap();
            let rooks: Vec<usize> = (0..8).filter(|&f| r[f] == b'R').collect();
            assert_eq!(rooks.len(), 2, "id {id}");
            assert!(
                rooks[0] < king && king < rooks[1],
                "id {id} king not between rooks"
            );
        }
    }

    #[test]
    fn all_ids_have_full_back_rank() {
        for id in 0..960 {
            let r = back_rank(id);
            assert!(r.iter().all(|&c| c != 0), "id {id} has empty file");
        }
    }
}
