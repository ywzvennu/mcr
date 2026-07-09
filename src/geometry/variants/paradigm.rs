//! Paradigm chess (8x8) on the generic engine — **standard chess with both
//! bishops replaced by a Bishop + Xiangqi-Horse compound**, and nothing else
//! changed. A Fairy-Stockfish built-in (`UCI_Variant paradigm`; FSF's
//! `paradigm_variant()` does `remove_piece(BISHOP)` +
//! `add_piece(CUSTOM_PIECE_1, 'b', "BnN")`). Validated square-for-square against
//! Fairy-Stockfish `go perft`.
//!
//! ## The Bishop-Horse
//!
//! The piece ([`WideRole::BishopHorse`], FSF letter `b`, Betza `BnN`) is a
//! **compound**: a **Bishop** slide (`B`) plus a **lame/hobbled Knight** (`nN`).
//! In FSF's Betza parser (`piece.cpp`) the modifier `n` is the **lame-leaper**
//! flag — **not** "narrow": `nN` is the Xiangqi **Horse** ([`WideRole::Horse`],
//! [`attacks::horse_attacks`]). It reaches all eight knight squares, but each leap
//! is **blocked** when the orthogonally-adjacent square one step toward the leap's
//! long axis — the "leg" — is occupied. So on an open board the Bishop-Horse
//! reaches the thirteen bishop diagonal squares **and** all eight knight squares;
//! a blocker on a leg removes exactly the two leaps that leg hobbles. It moves and
//! captures alike on both components, gives **check** and creates **pins** along
//! its bishop diagonals, and its horse leaps give check like a knight's.
//!
//! ## King safety — the full-verify path
//!
//! The Horse component is a **lame leaper**, so two facts break the generic
//! line-based king-safety machinery, exactly as the Nightrider's knight-rays do
//! ([`super::nightrider`]):
//!
//! * A Horse **check may be answered by hobbling its leg** — interposing a piece
//!   on the single orthogonal square adjacent to the Horse toward the leap. That
//!   square is **not** on the king's rank / file / diagonal, so the line-based
//!   check-interposition mask (`between`) never offers it, and the fast-accept
//!   would wrongly reject the leg-block. (Confirmed a legal reply in FSF.)
//! * The Horse's leg is adjacent to the **Horse**, not the target, so
//!   reverse-projecting its pattern from the king tests the wrong leg — hence
//!   [`role_attack_is_leg_asymmetric`](WideVariant::role_attack_is_leg_asymmetric).
//!
//! So this variant opts into [`WideVariant::needs_full_verify`], routing every move
//! through the per-move make/unmake verify generator with the fast-accept disabled.
//! Each pseudo-legal move is authoritatively re-tested by `king_safe_after`, which
//! forward-projects the Bishop-Horse's own (occupancy-aware) attack set — the
//! bishop slide *and* the hobbled horse leaps — so leg-blocks, pins, and discovered
//! checks all resolve exactly.
//!
//! ## Promotion
//!
//! A pawn of either colour reaching the last rank promotes to a **Queen**,
//! **Bishop-Horse**, **Rook**, or **Knight** (FSF
//! `promotionPieceTypes = piece_set(QUEEN) | CUSTOM_PIECE_1 | ROOK | KNIGHT`) —
//! never an ordinary Bishop (there are none in this army; the Knight is retained).
//!
//! ## Confirmed starting FEN
//!
//! Paradigm chess is a FSF **built-in** derived from the standard chess base, so
//! its position is the standard chess start with the bishops being Bishop-Horses:
//!
//! ```text
//! FSF dialect: rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! mcr dialect: rn****xqk****xnr/pppppppp/8/8/8/8/PPPPPPPP/RN****XQK****XNR w KQkq - 0 1
//! ```
//!
//! In FSF the back rank's `b` is the Bishop-Horse. mcr already names `b` the
//! standard Bishop, and every single-letter base plus the `*` / `**` / `=` / `***`
//! overflow banks are exhausted, so the Bishop-Horse takes the fifth-tier
//! **overflow token** `****x` (the free base `x`, distinct by the `****` prefix):
//! the standard back rank `r n b q k b n r` becomes `r n ****x q k ****x n r`. The
//! two FENs are the same position; the `compare-fairy/` harness rewrites mcr's
//! `****x → b` when driving FSF. Both sides have full castling rights (`KQkq`).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Chess8x8, Geometry, PromotionConfig, Square, StandardChess, WideRole,
    WideVariant,
};
use crate::Color;

/// The confirmed Paradigm starting placement in mcr's role letters: standard chess
/// with the two bishops replaced by Bishop-Horses (`****x`), so each back rank is
/// `r n ****x q k ****x n r` and the pawns / king / rooks / queen / knights are
/// standard.
const PARADIGM_START_PLACEMENT: &str =
    "rn****xqk****xnr/pppppppp/8/8/8/8/PPPPPPPP/RN****XQK****XNR";

/// The Paradigm-chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides only the Bishop-Horse's movement (a Bishop slide + a hobbled Horse
/// leap), the `q b r n` promotion set (`b` being the Bishop-Horse, never an
/// ordinary Bishop), the Horse component's leg-asymmetric attacker detection, and
/// opts into the per-move full king-safety verification the lame-leaper leg-block
/// requires ([`needs_full_verify`](WideVariant::needs_full_verify)). Every other
/// piece, castling, the double pawn step, and en passant are the standard-chess
/// trait defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ParadigmRules;

impl WideVariant<Chess8x8> for ParadigmRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field (the start army Pawn / Knight / Rook / Queen / King, and
    /// the [`WideRole::BishopHorse`] at index `154`, also a promotion target); the
    /// movegen loops iterate this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = WideRole::BishopHorse.index() + 1;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(PARADIGM_START_PLACEMENT)
            .expect("the Paradigm starting placement is valid on an 8x8 board");
        // Standard chess castling rights for both sides: the kingside rook sits on
        // the last file, the queenside rook on file 0.
        let mut castling = GenericCastling::NONE;
        for color in Color::ALL {
            castling.set(color, 0, Some(Chess8x8::WIDTH - 1));
            castling.set(color, 1, Some(0));
        }
        let state = GenericState {
            turn: Color::White,
            castling,
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
            jieqi_seed: None,
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
            // The Bishop-Horse (Betza `BnN`): a Bishop slide unioned with the
            // hobbled Xiangqi-Horse leap. Both components move and capture alike, so
            // the whole set is the attack set. Occupancy blocks the bishop slide and
            // hobbles a horse leg, so `attackers_to` / `king_safe_after` must
            // forward-project it (see `role_attack_is_leg_asymmetric`).
            WideRole::BishopHorse => {
                attacks::bishop_attacks::<Chess8x8>(sq, occupancy)
                    | attacks::horse_attacks::<Chess8x8>(sq, occupancy)
            }
            // Everything else (pawn, knight, rook, queen, king) is standard chess.
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn role_attack_is_leg_asymmetric(role: WideRole) -> bool {
        // The Bishop-Horse's Horse component is a **lame** leaper: its hobbling leg
        // is adjacent to the *piece* and points toward the leap, a different square
        // than the leg a reverse leap from the target would test. So attacker
        // detection cannot reverse-project the pattern from the target square; it
        // forward-projects the whole (bishop + horse) set from each Bishop-Horse
        // origin, exactly as the move generator does. Every other role is a
        // symmetric standard-chess mover.
        matches!(role, WideRole::BishopHorse)
    }

    fn role_is_slider(role: WideRole) -> bool {
        // The Bishop-Horse's Bishop component is a genuine diagonal line slider: it
        // pins a friendly piece to the king and can give a discovered check along a
        // diagonal (the en-passant discovered-check test and `compute_pins` iterate
        // sliders). Its Horse leaps are non-collinear with any diagonal, so the pin
        // machinery only ever produces the correct bishop-diagonal pins from it.
        // Classify it as a slider like the Bishop / Queen / Hawk it extends.
        match role {
            WideRole::BishopHorse => true,
            _ => <StandardChess as WideVariant<Chess8x8>>::role_is_slider(role),
        }
    }

    /// Every move is verified by the per-move make/unmake king-safety re-test, with
    /// the geometry fast-accept disabled. The Bishop-Horse's Horse component is a
    /// lame leaper whose **leg-block** check evasion (interposing on the horse's leg,
    /// a square off every king line) and whose leg-asymmetric attacks the line-based
    /// pin / interposition / fast-accept machinery cannot express, so this routes
    /// the variant through the authoritative `king_safe_after` verify. See
    /// [`WideVariant::needs_full_verify`].
    fn needs_full_verify() -> bool {
        true
    }

    // --- promotion: pawns -> Queen / Bishop-Horse / Rook / Knight --------------

    fn promotion_config() -> PromotionConfig {
        // FSF `promotionPieceTypes = piece_set(QUEEN) | CUSTOM_PIECE_1 | ROOK |
        // KNIGHT`, where `CUSTOM_PIECE_1` is the Bishop-Horse (there is no ordinary
        // Bishop in this army; the Knight is retained).
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Queen,
                WideRole::BishopHorse,
                WideRole::Rook,
                WideRole::Knight,
            ],
        }
    }

    fn has_castling() -> bool {
        true
    }

    /// The western **fifty-move rule**: a position whose halfmove clock has reached
    /// 100 plies is a
    /// [`WideEndReason::MoveRule`](crate::geometry::WideEndReason::MoveRule) draw,
    /// matching Fairy-Stockfish's default `nMoveRule = 50` for its standard-chess
    /// base. Adjudication-only (the clock never gates move generation), so perft
    /// stays byte-identical.
    fn move_rule_plies() -> Option<u16> {
        Some(100)
    }
}

/// Paradigm chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position (standard chess with Bishop-Horses in place of
/// the bishops) with [`Paradigm::startpos`](GenericPosition::startpos) or parse a
/// FEN (mcr dialect) with [`Paradigm::from_fen`](GenericPosition::from_fen). See the
/// [module docs](self) for the Bishop + hobbled-Horse movement, the full-verify king
/// safety, and the `q b r n` promotion.
pub type Paradigm = GenericPosition<
    Chess8x8,
    ParadigmRules,
    { <ParadigmRules as WideVariant<Chess8x8>>::ROLE_SPAN },
>;

#[cfg(test)]
mod tests {
    use super::*;

    /// The canonical start FEN round-trips (mcr `****x` dialect) and has 20 legal
    /// moves — the same as standard chess at the root (16 pawn pushes + 4 knight
    /// moves; the Bishop-Horses are hemmed in on the back rank exactly as bishops).
    #[test]
    fn startpos_round_trips() {
        let pos = Paradigm::startpos();
        assert_eq!(
            pos.to_fen(),
            "rn****xqk****xnr/pppppppp/8/8/8/8/PPPPPPPP/RN****XQK****XNR w KQkq - 0 1"
        );
        assert_eq!(pos.turn(), Color::White);
        // Matches Fairy-Stockfish `UCI_Variant paradigm` startpos perft(1).
        assert_eq!(pos.legal_move_count(), 20);
    }

    /// A lone Bishop-Horse on an open board reaches its bishop diagonals **and** all
    /// eight knight squares (the un-hobbled Horse), matching FSF `go perft 1`: from
    /// d4 that is 13 diagonal squares + 8 knight squares = 21 targets.
    #[test]
    fn open_board_reaches_diagonals_and_knight_squares() {
        let pos = Paradigm::from_fen("4k3/8/8/8/3****X4/8/8/4K3 w - - 0 1").expect("valid FEN");
        let dests: alloc::vec::Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Chess8x8>() == Square::new(27)) // d4
            .map(|m| m.to::<Chess8x8>().index())
            .collect();
        assert_eq!(dests.len(), 21, "13 bishop diagonals + 8 knight leaps");
        // Bishop diagonal sample: a1 (0), h8 (63); knight-leap sample: c2 (10),
        // e6 (44) — the (±1,±2) leaps that a "narrow" reading would keep — plus
        // b3 (17) and f5 (37) — the (±2,±1) leaps a narrow reading would DROP.
        for expected in [0u8, 63, 10, 44, 17, 37] {
            assert!(
                dests.contains(&expected),
                "d4 Bishop-Horse reaches {expected}"
            );
        }
    }

    /// The Horse component is **hobbled**: a blocker on the leg square removes
    /// exactly the two leaps that leg blocks (the lame-leaper rule), while the
    /// bishop diagonals through unrelated squares are unaffected. A friendly pawn on
    /// d5 (the leg for the two up-vertical leaps c6 / e6) removes just those two.
    #[test]
    fn horse_leg_is_hobbled_by_a_blocker() {
        let open = Paradigm::from_fen("4k3/8/8/8/3****X4/8/8/4K3 w - - 0 1").expect("valid FEN");
        let open_d4 = open
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Chess8x8>() == Square::new(27))
            .count();
        // Same position with a white pawn on d5 (leg of the c6 / e6 leaps). d5 is not
        // on any diagonal from d4, so only the two hobbled leaps disappear.
        let blocked =
            Paradigm::from_fen("4k3/8/8/3P4/3****X4/8/8/4K3 w - - 0 1").expect("valid FEN");
        let blocked_d4 = blocked
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Chess8x8>() == Square::new(27))
            .map(|m| m.to::<Chess8x8>().index())
            .collect::<alloc::vec::Vec<_>>();
        assert_eq!(
            blocked_d4.len(),
            open_d4 - 2,
            "the d5 leg hobbles exactly the c6 and e6 leaps"
        );
        assert!(!blocked_d4.contains(&42), "c6 leap hobbled"); // c6 = 42
        assert!(!blocked_d4.contains(&44), "e6 leap hobbled"); // e6 = 44
        assert!(
            blocked_d4.contains(&37),
            "the f5 leap (leg e4, empty) survives"
        ); // f5 = 37
    }

    /// A Horse **check may be answered by hobbling its leg** — a defense the
    /// line-based interposition mask cannot express, so it exercises the full-verify
    /// path. Black king on e8 is checked by a white Bishop-Horse on d6 (the (1,2)
    /// leap, leg d7); a black rook on a7 can block on d7 to hobble it. Legal replies
    /// are the two king steps (d8, d7) plus Ra7-d7. Matches FSF perft(1) = 3.
    #[test]
    fn horse_check_answered_by_leg_block() {
        let pos = Paradigm::from_fen("4k3/r7/3****X4/8/8/8/8/4K3 b - - 0 1").expect("valid FEN");
        let moves = pos.legal_moves();
        assert_eq!(moves.len(), 3);
        // The rook hobbles the horse's leg on d7 (square 51).
        assert!(moves
            .iter()
            .any(|m| m.from::<Chess8x8>() == Square::new(48) // a7
                && m.to::<Chess8x8>() == Square::new(51))); // d7
    }

    /// A pawn promotes to a Bishop-Horse (not an ordinary Bishop): the `q b r n`
    /// promotion set offers exactly four targets on the last rank, including the
    /// Bishop-Horse and the Knight but never a plain Bishop.
    #[test]
    fn pawn_promotes_to_bishop_horse() {
        let pos = Paradigm::from_fen("4k3/P7/8/8/8/8/8/4K3 w - - 0 1").expect("valid FEN");
        let promos: alloc::vec::Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter_map(|m| m.promotion())
            .collect();
        assert!(promos.contains(&WideRole::BishopHorse));
        assert!(!promos.contains(&WideRole::Bishop));
        assert_eq!(promos.len(), 4, "q b r n promotion targets");
    }
}
