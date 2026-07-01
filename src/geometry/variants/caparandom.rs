//! Capablanca-Random (`caparandom`) — the Capablanca 10x8 army shuffled on the
//! back rank with Chess960-style castling, on the generic engine's [`WideVariant`]
//! layer.
//!
//! Caparandom is to Capablanca what Chess960 is to standard chess: identical
//! pieces and movement, but the back-rank army is one of the many legal shuffles
//! (bishops on opposite colours, the king between its two rooks), and castling
//! generalises to the king's and rooks' *actual* start files. It reuses the whole
//! Capablanca rule layer ([`CapablancaRules`]): the 10x8 [`Cap10x8`] geometry, the
//! Chancellor ([`Elephant`](crate::geometry::WideRole::Elephant), `e`/`E`) and
//! Archbishop ([`Hawk`](crate::geometry::WideRole::Hawk), `a`/`A`) compounds, the
//! six-role promotion set, and the **fixed** castle destinations. Only two things
//! change:
//!
//! * The castling FEN field is written in **Shredder** form (rook *file* letters,
//!   e.g. `JAja`), matching Fairy-Stockfish's `UCI_Variant caparandom` output. The
//!   generic reader already accepts both Shredder file letters and `KQkq`
//!   (outermost-rook X-FEN), so arbitrary shuffled rook files parse unchanged.
//! * The starting position is a representative shuffle. To match FSF's own
//!   `caparandom` `startpos`, mce uses the canonical Capablanca array — the arms
//!   are identical, so the only visible difference from `capablanca` is the
//!   `JAja` castling field.
//!
//! ## Confirmed startpos (Fairy-Stockfish `UCI_Variant caparandom`)
//!
//! ```text
//! FSF dialect: rnabqkbcnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBCNR w JAja - 0 1
//! mce dialect: rnabqkbenr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBENR w JAja - 0 1
//! ```
//!
//! (perft(2) = 784, identical to Capablanca — the two share the startpos array and
//! differ only in the castling field's notation.)
//!
//! ## Castling geometry
//!
//! The castle destinations are Capablanca's, independent of where the king and
//! rooks start (exactly as Fairy-Stockfish, whose `caparandom` keeps
//! `castlingKingsideFile = FILE_I`, `castlingQueensideFile = FILE_C`):
//!
//! * **Kingside**: king to **i** (file 8), rook to **h** (file 7).
//! * **Queenside**: king to **c** (file 2), rook to **d** (file 3).
//!
//! The wide castle generator already walks the king from its *actual* square to
//! that destination and re-tests the king's landing under the post-castle
//! occupancy, so a shuffled back rank where a castling rook shields the king's
//! target (a Chess960 discovered-check corner case) is handled correctly.

use crate::geometry::position::{GenericPosition, GenericState};
use crate::geometry::variants::capablanca::CapablancaRules;
use crate::geometry::{Board, Cap10x8, PromotionConfig, WideVariant};

/// The Capablanca-Random rule layer: [`CapablancaRules`] with Chess960-style
/// shuffled starts and Shredder-FEN castling.
///
/// Every rule is delegated to [`CapablancaRules`] except the castling FEN
/// notation ([`shredder_castling_fen`](WideVariant::shredder_castling_fen)), which
/// switches to rook-file letters so arbitrary shuffled rook files round-trip and
/// the FEN matches Fairy-Stockfish's `caparandom` output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CaparandomRules;

impl WideVariant<Cap10x8> for CaparandomRules {
    fn starting_position() -> (Board<Cap10x8>, GenericState<Cap10x8>) {
        // Byte-identical to Capablanca's start (same array, same `KQkq`-equivalent
        // rights: rooks on the a/j files); only the FEN *writer* differs, rendering
        // those rights as `JAja`.
        CapablancaRules::starting_position()
    }

    fn promotion_config() -> PromotionConfig {
        CapablancaRules::promotion_config()
    }

    fn castle_dest_files(side: usize) -> (u8, u8) {
        // Capablanca's fixed destinations, independent of the shuffled start files.
        CapablancaRules::castle_dest_files(side)
    }

    fn shredder_castling_fen() -> bool {
        // Write rook-file letters (`JAja`), matching Fairy-Stockfish's `caparandom`.
        true
    }

    fn is_insufficient_material(board: &Board<Cap10x8>, state: &GenericState<Cap10x8>) -> bool {
        CapablancaRules::is_insufficient_material(board, state)
    }
}

/// Capablanca-Random chess as a [`GenericPosition`] over the 10x8 [`Cap10x8`]
/// geometry.
///
/// Construct the canonical starting position with
/// [`Caparandom::startpos`](GenericPosition::startpos) or parse any legal shuffle
/// (with Shredder or `KQkq` castling rights) via
/// [`Caparandom::from_fen`](GenericPosition::from_fen).
pub type Caparandom = GenericPosition<Cap10x8, CaparandomRules>;

#[cfg(test)]
mod tests {
    use super::Caparandom;
    use crate::geometry::{perft as gperft, Cap10x8};

    /// The startpos renders with the file-letter (`JAja`) castling field and
    /// round-trips through the FEN parser.
    #[test]
    fn startpos_uses_shredder_castling_and_round_trips() {
        let pos = Caparandom::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnabqkbenr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBENR w JAja - 0 1"
        );
        let re = Caparandom::from_fen(&pos.to_fen()).expect("startpos fen parses");
        assert_eq!(re.to_fen(), pos.to_fen());
        // Same shallow perft as Capablanca (shared startpos array).
        assert_eq!(gperft::<Cap10x8, _>(&pos, 2), 784);
    }

    /// A shuffled start with rooks off the a/j files round-trips its Shredder
    /// castling field through parse -> write.
    #[test]
    fn shuffled_start_round_trips_file_letter_rights() {
        // King e, queenside rook b, kingside rook i -> rights `IBib`.
        let fen = "crnbkqbnra/pppppppppp/10/10/10/10/PPPPPPPPPP/CRNBKQBNRA w IBib - 0 1";
        let pos = Caparandom::from_fen(fen).expect("shuffled fen parses");
        assert_eq!(pos.to_fen(), fen);
    }
}
