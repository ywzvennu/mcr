//! Perfect chess (8x8) on the generic engine — standard chess with **three
//! compound pieces** added to the back rank. Validated square-for-square against
//! Fairy-Stockfish `UCI_Variant perfect`.
//!
//! Perfect chess is played on the standard 8x8 board. Alongside the standard army
//! it fields all three "minor-compound" pieces:
//!
//! * **Chancellor** (Rook + Knight) — mcr's [`WideRole::Elephant`], whose default
//!   movement (`rook | knight`) is already the chancellor's. FEN letter `e`/`E` in
//!   the mcr dialect (Fairy-Stockfish spells the chancellor `c`/`C`, a dialect
//!   difference the `compare-fairy/` harness reconciles).
//! * **Archbishop** (Bishop + Knight) — mcr's [`WideRole::Hawk`], whose default
//!   movement (`bishop | knight`) is already the archbishop's. FEN letter `a`/`A`
//!   in the mcr dialect (Fairy-Stockfish spells this archbishop `m`/`M`).
//! * **Amazon** (Queen + Knight) — mcr's [`WideRole::Angel`]. A genuinely-new mover
//!   on the 8x8 path (the trait default has no Amazon), so this variant supplies its
//!   [`role_attacks`](WideVariant::role_attacks). FEN token `**a`/`**A` in the mcr
//!   dialect (the second-bank overflow token; Fairy-Stockfish spells the amazon
//!   `g`/`G`).
//!
//! Every other rule is standard chess: pawns push one (or two from their second
//! rank), capture diagonally, take en passant, and promote on the last rank — here
//! to **Amazon, Chancellor, Archbishop, Queen, Rook, Bishop, or Knight**
//! (Fairy-Stockfish `promotionPieceTypes = g c m q r b n`).
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant perfect` (`position startpos`):
//!
//! ```text
//! FSF dialect: cmqgkbnr/pppppppp/8/8/8/8/PPPPPPPP/CMQGKBNR w KQkq - 0 1
//! mcr dialect: eaq**akbnr/pppppppp/8/8/8/8/PPPPPPPP/EAQ**AKBNR w KQkq - 0 1
//! ```
//!
//! The two strings are the same position; only the compound letters differ
//! (chancellor `c`->`e`, archbishop `m`->`a`, amazon `g`->`**a`). Back rank, a-file
//! to h-file: **Chancellor, Archbishop, Queen, Amazon, King, Bishop, Knight, Rook**.
//! The king stands on the e-file (file 4).
//!
//! ## Castling geometry (the Chancellor is the queen-side "rook")
//!
//! The king castles on the standard files — kingside king e1 -> **g1** (file 6)
//! with the castle piece to **f1** (file 5); queenside king e1 -> **c1** (file 2)
//! with the castle piece to **d1** (file 3). But — matching Fairy-Stockfish's
//! `castlingRookPieces |= CHANCELLOR` — the **queen-side** castle piece is the
//! **Chancellor** on a1 (not a Rook), while the king-side castle uses the ordinary
//! Rook on h1. So queenside `e1c1` slides the a1 Chancellor to d1; kingside `e1g1`
//! slides the h1 Rook to f1. Both were confirmed square-for-square against FSF.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Chess8x8, PromotionConfig, Square, StandardChess, WideRole,
    WideVariant,
};
use crate::Color;

/// The confirmed Perfect chess starting placement in the mcr dialect (chancellor =
/// `e`/`E`, archbishop = `a`/`A`, amazon = `**a`/`**A`), the same position as
/// Fairy-Stockfish's `cmqgkbnr/.../CMQGKBNR` modulo the compound letters.
const PERFECT_START_PLACEMENT: &str = "eaq**akbnr/pppppppp/8/8/8/8/PPPPPPPP/EAQ**AKBNR";

/// The queen-side castle index, matching the position layer's `QUEENSIDE`.
const QUEENSIDE: usize = 1;

/// The Perfect chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides the starting array (the three compounds on the back rank), the
/// Amazon's movement (Queen + Knight), the seven-role promotion set, and the
/// queen-side castle piece (the Chancellor). The Chancellor ([`WideRole::Elephant`])
/// and Archbishop ([`WideRole::Hawk`]) movement is already the standard-chess
/// default; pawns, knights, sliders, the king, and en passant are standard chess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PerfectRules;

impl WideVariant<Chess8x8> for PerfectRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field — here up to the Amazon ([`WideRole::Angel`]); the movegen
    /// loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 68;

    /// The western **fifty-move rule**: a position whose halfmove clock has reached
    /// 100 plies (50 full moves with no capture or pawn move) is a
    /// [`WideEndReason::MoveRule`](crate::geometry::WideEndReason::MoveRule) draw,
    /// matching Fairy-Stockfish's default `nMoveRule = 50`. Adjudication-only (the
    /// clock never gates move generation), so perft stays byte-identical.
    fn move_rule_plies() -> Option<u16> {
        Some(100)
    }

    /// Records a position history so the standard **threefold** repetition draw
    /// ([`WideEndReason::Repetition`](crate::geometry::WideEndReason::Repetition),
    /// fold 3) fires at the [`GenericGame`](crate::geometry::game::GenericGame)
    /// level. History-dependent and never consulted by a bare
    /// [`GenericPosition`], so perft is unchanged.
    fn tracks_repetition() -> bool {
        true
    }

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(PERFECT_START_PLACEMENT)
            .expect("the Perfect chess starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            // Standard chess castling rights for both sides: the queenside file is
            // the a-file (the Chancellor), the kingside file the h-file (the Rook);
            // the per-side castle piece role is read from `castle_rook_role`.
            castling: GenericCastling::standard::<Chess8x8>(),
            ep_square: None,
            ep_captured: None,
            gating: GenericGating::NONE,
            duck: None,
            placement: GenericPlacement::NONE,
            halfmove_clock: 0,
            fullmove_number: 1,
            consecutive_passes: 0,
            board_b: crate::geometry::Bitboard::EMPTY,
            petrified: crate::geometry::Bitboard::EMPTY,
            checks_against: [0, 0],
        };
        (board, state)
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Chess8x8>,
        occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        match role {
            // Amazon (Queen + Knight): a queen's slides plus the eight knight leaps.
            // The Chancellor (Elephant = Rook + Knight) and Archbishop (Hawk =
            // Bishop + Knight) are the standard-chess defaults, reached below.
            WideRole::Angel => {
                attacks::queen_attacks::<Chess8x8>(sq, occupancy)
                    | attacks::knight_attacks::<Chess8x8>(sq)
            }
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn role_is_slider(role: WideRole) -> bool {
        match role {
            // The Amazon slides along the queen lines, so it can pin and be pinned.
            // The Chancellor and Archbishop slide too, reported by the default below.
            WideRole::Angel => true,
            _ => <StandardChess as WideVariant<Chess8x8>>::role_is_slider(role),
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // Every Perfect-chess attack set is geometrically symmetric (queen/rook/
        // bishop slides plus knight leaps), so only the pawn is colour-directional.
        matches!(role, WideRole::Pawn)
    }

    fn promotion_config() -> PromotionConfig {
        // FSF `promotionPieceTypes = g c m q r b n`: Amazon, Chancellor, Archbishop,
        // then the four standard roles. Order affects only move enumeration order,
        // not the perft leaf count.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Angel,    // Amazon (Q+N)
                WideRole::Elephant, // Chancellor (R+N)
                WideRole::Hawk,     // Archbishop (B+N)
                WideRole::Queen,
                WideRole::Rook,
                WideRole::Bishop,
                WideRole::Knight,
            ],
        }
    }

    fn castle_rook_role(side: usize) -> WideRole {
        // FSF adds CHANCELLOR to `castlingRookPieces`: the queen-side castle piece is
        // the Chancellor (Elephant) on a1. The king-side castle uses the h1 Rook.
        if side == QUEENSIDE {
            WideRole::Elephant
        } else {
            WideRole::Rook
        }
    }

    /// Perfect chess keeps the standard chess army plus the always-mating Amazon
    /// ([`WideRole::Angel`]), Chancellor ([`WideRole::Elephant`]), and Archbishop
    /// ([`WideRole::Hawk`]), so the ordinary insufficient-material draw applies: king
    /// vs king, king and a lone minor (bishop or knight) vs king, and same-colour
    /// bishops only. The three compounds count as mating material. Adjudication-only
    /// and behind the default-off hook, so perft stays byte-identical.
    fn is_insufficient_material<const R: usize>(
        board: &Board<Chess8x8, R>,
        _state: &GenericState<Chess8x8, R>,
    ) -> bool {
        crate::geometry::variant::standard_insufficient_material(board)
    }
}

/// Perfect chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Perfect::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Perfect::from_fen`](GenericPosition::from_fen). The Chancellor and Archbishop
/// reuse the [`StandardChess`] compound defaults; the
/// array, the Amazon's Queen + Knight movement, the seven-role promotion set, and the
/// queen-side Chancellor castle distinguish it.
pub type Perfect =
    GenericPosition<Chess8x8, PerfectRules, { <PerfectRules as WideVariant<Chess8x8>>::ROLE_SPAN }>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::WideMoveKind;

    /// The canonical start FEN round-trips and opens with the FSF-confirmed 23
    /// moves (16 pawn pushes + 2 knight, 2 amazon-knight, 2 archbishop-knight, and
    /// 1 chancellor-knight hop out of the crowded back rank).
    #[test]
    fn startpos_round_trips() {
        let pos = Perfect::startpos();
        assert_eq!(
            pos.to_fen(),
            "eaq**akbnr/pppppppp/8/8/8/8/PPPPPPPP/EAQ**AKBNR w KQkq - 0 1"
        );
        assert_eq!(pos.legal_move_count(), 23);
    }

    /// A lone Amazon reaches queen rays plus the eight knight leaps (27 queen moves
    /// from d4 on an 8x8 board + 8 knight = 35).
    #[test]
    fn amazon_moves_as_queen_plus_knight() {
        let pos = Perfect::from_fen("8/8/8/8/3**A4/8/K7/7k w - - 0 1").expect("valid");
        let sq = Square::<Chess8x8>::from_file_rank(3, 3).unwrap();
        let n = count_from(&pos, sq);
        assert_eq!(n, 35, "amazon = queen (27 from d4) + knight (8)");
    }

    /// A lone Chancellor reaches rook rays plus the eight knight leaps (14 rook
    /// moves from d4 + 8 knight = 22).
    #[test]
    fn chancellor_moves_as_rook_plus_knight() {
        let pos = Perfect::from_fen("8/8/8/8/3E4/8/K7/7k w - - 0 1").expect("valid");
        let sq = Square::<Chess8x8>::from_file_rank(3, 3).unwrap();
        assert_eq!(
            count_from(&pos, sq),
            22,
            "chancellor = rook (14) + knight (8)"
        );
    }

    /// A lone Archbishop reaches bishop rays plus the eight knight leaps (13 bishop
    /// moves from d4 + 8 knight = 21).
    #[test]
    fn archbishop_moves_as_bishop_plus_knight() {
        let pos = Perfect::from_fen("8/8/8/8/3A4/8/K7/7k w - - 0 1").expect("valid");
        let sq = Square::<Chess8x8>::from_file_rank(3, 3).unwrap();
        assert_eq!(
            count_from(&pos, sq),
            21,
            "archbishop = bishop (13) + knight (8)"
        );
    }

    /// A pawn promotes to Amazon / Chancellor / Archbishop / Queen / Rook / Bishop /
    /// Knight — all seven of the non-pawn, non-king roles.
    #[test]
    fn pawn_promotes_to_every_compound() {
        let pos = Perfect::from_fen("4k3/2P5/8/8/8/8/8/4K3 w - - 0 1").expect("valid");
        let mut roles: alloc::vec::Vec<WideRole> = pos
            .legal_moves()
            .into_iter()
            .filter_map(|m| m.promotion())
            .collect();
        roles.sort();
        roles.dedup();
        let mut want = alloc::vec![
            WideRole::Angel,
            WideRole::Elephant,
            WideRole::Hawk,
            WideRole::Queen,
            WideRole::Rook,
            WideRole::Bishop,
            WideRole::Knight,
        ];
        want.sort();
        assert_eq!(roles, want);
    }

    /// Queen-side castling slides the a1 **Chancellor** (not a Rook) to d1 while the
    /// king lands on c1; king-side castling slides the h1 Rook to f1. Both king
    /// destinations and both castle-piece landings match Fairy-Stockfish.
    #[test]
    fn castling_uses_the_chancellor_on_the_queen_side() {
        let pos =
            Perfect::from_fen("e3k2r/pppppppp/8/8/8/8/PPPPPPPP/E3K2R w KQkq - 0 1").expect("valid");
        let mut saw_kingside = false;
        let mut saw_queenside = false;
        for mv in pos.legal_moves() {
            match mv.kind() {
                WideMoveKind::CastleKingside => {
                    saw_kingside = true;
                    assert_eq!(mv.to_uci::<Chess8x8>(), "e1g1");
                    let next = pos.play(&mv);
                    // King on g1 (file 6), Rook slid to f1 (file 5).
                    assert_eq!(
                        next.board().king_of(Color::White),
                        Square::<Chess8x8>::from_file_rank(6, 0),
                    );
                    assert_eq!(
                        next.board()
                            .piece_at(Square::<Chess8x8>::from_file_rank(5, 0).unwrap())
                            .map(|p| p.role),
                        Some(WideRole::Rook),
                    );
                }
                WideMoveKind::CastleQueenside => {
                    saw_queenside = true;
                    assert_eq!(mv.to_uci::<Chess8x8>(), "e1c1");
                    let next = pos.play(&mv);
                    // King on c1 (file 2), Chancellor slid to d1 (file 3).
                    assert_eq!(
                        next.board().king_of(Color::White),
                        Square::<Chess8x8>::from_file_rank(2, 0),
                    );
                    assert_eq!(
                        next.board()
                            .piece_at(Square::<Chess8x8>::from_file_rank(3, 0).unwrap())
                            .map(|p| p.role),
                        Some(WideRole::Elephant),
                    );
                }
                _ => {}
            }
        }
        assert!(
            saw_kingside && saw_queenside,
            "both castles available (queenside with the Chancellor)"
        );
    }

    /// Counts the legal moves originating on `sq`.
    fn count_from(pos: &Perfect, sq: Square<Chess8x8>) -> usize {
        pos.legal_moves()
            .into_iter()
            .filter(|m| m.from::<Chess8x8>() == sq)
            .count()
    }
}
