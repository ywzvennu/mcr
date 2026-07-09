//! Omicron — Omega chess on a **12x10 walled board**, on the generic engine. It
//! reuses the new [`Omicron12x10`] geometry (twelve files by ten ranks, `120 <= 128`
//! bits) and the two Omega-family leapers already fielded by Ten-Cubed and Opulent —
//! the Wizard ([`WideRole::Wizard`], Betza `CF` = Camel + Ferz) and the Champion
//! ([`WideRole::TencubedChampion`], Betza `WAD` = Wazir + Alfil + Dabbaba, i.e. FSF
//! Omicron's `DAW`) — so it introduces **no new role**, only a new geometry.
//!
//! Omicron is Fairy-Stockfish's built-in `omicron` (`omicron_variant()`): a
//! `chess_variant_base` widened to `maxFile = FILE_L` (twelve files) and `maxRank =
//! RANK_10` (ten ranks), with a Champion (`c`) and Wizard (`w`) added to the army and
//! a heavily walled board.
//!
//! ## The walled board
//!
//! The FSF start FEN is
//! `w**********w/*crnbqkbnrc*/*pppppppppp*/*10*/*10*/*10*/*10*/*PPPPPPPPPP*/*CRNBQKBNRC*/W**********W`.
//! The `*` cells are **wall squares**. Reading the board: the **a- and l-files**
//! (0 / 11) exist only on ranks 1 and 10 — their corners a1 / a10 / l1 / l10 hold the
//! four Wizards, and the intervening a2–a9 / l2–l9 are permanently blocked. The
//! **top and bottom ranks** (rank 10 / rank 1) exist only on files a and l — their
//! interiors b10–k10 / b1–k1 are walls. So the real play area is a **10x8 interior**
//! (files b..k, ranks 2..9) plus the **four corner Wizard cells**, 84 playable of the
//! 120 squares. A wall blocks a slider exactly like an occupied square, and no piece —
//! leap, pawn capture, king step, or slide — may ever land on one.
//!
//! mcr models the walls as a compile-time [`WideVariant::board_walls`] mask folded
//! into the move-generation occupancy and target masks, **not** as FEN state — so the
//! wall cells render as ordinary empty squares (`w10w`, `10`), and the `*` FEN token
//! stays free for the fairy pieces' `**`-overflow spellings (a `*`-wall FEN and a
//! `*`-prefixed overflow role cannot coexist in one placement string). Exactly as
//! [`Gustav3`](super::gustav3) does. Because a wall can sit "inside" the board and the
//! king could in principle wander toward a walled file, Omicron routes king safety
//! through the make/unmake [`WideVariant::multi_royal`] path, where every generated
//! target — king included — is masked against the walls.
//!
//! ## Pieces (confirmed against FSF `omicron_variant()`)
//!
//! The standard chess army plus a Champion and a Wizard. The interior back rank is
//! `C R N B Q K B N R C` (Champion, Rook, Knight, Bishop, Queen, King, Bishop, Knight,
//! Rook, Champion) on files b..k, king on the **g**-file, with a Wizard tucked in each
//! board corner:
//!
//! * **Champion (`**x`/`**X`, [`WideRole::TencubedChampion`], Betza `WAD` = FSF
//!   `DAW`)** — a pure leaper to the four Wazir `(±1,0)`/`(0,±1)` one-steps, the four
//!   Dabbaba `(±2,0)`/`(0,±2)` two-orthogonal jumps, and the four Alfil `(±2,±2)`
//!   two-diagonal jumps (twelve targets), leaping over any intervening piece. FSF
//!   spells it `c`; mcr's second-bank overflow token is `**x`/`**X` (the same
//!   reconciliation Ten-Cubed uses, since the Champion's FSF letter `c` is already the
//!   CrownPrince's `**` base).
//! * **Wizard (`**w`/`**W`, [`WideRole::Wizard`], Betza `CF`)** — a pure leaper to the
//!   eight Camel `(±1,±3)`/`(±3,±1)` jumps and the four Ferz `(±1,±1)` diagonal
//!   one-steps (twelve targets). FSF spells it `w`; mcr uses `**w`/`**W`.
//! * **King / Queen / Rook / Bishop / Knight / Pawn** — standard chess, with the pawn
//!   double-step (from rank 3 / 8), en passant, and promotion.
//!
//! ## Castling
//!
//! The king and rooks sit on **rank 2** (white) / **rank 9** (black), the Wizards
//! holding the true back rank corners (FSF `castlingRank = RANK_2`, via the
//! [`WideVariant::castle_rank`] hook). The king starts on **g** with the castling
//! rooks on the **c**- and **j**-files (the Champions occupy b/k, outside the rooks).
//! Kingside the king goes to **i** with its rook to **h**; queenside the king goes to
//! **e** with its rook to **f** (FSF `castlingKingsideFile = FILE_I`,
//! `castlingQueensideFile = FILE_E`, rook beside the king toward the centre).
//!
//! ## Promotion
//!
//! FSF's `promotionRegion = Rank9 | Rank10`, but a pawn (confined to files b..k) can
//! never reach rank 10 there — those cells are walls — so promotion happens on **rank
//! 9** (white) / **rank 2** (black), the last reachable rank, modelled as a
//! single-rank zone via [`WideVariant::promotion_rank`]. A pawn promotes to a
//! **Wizard, Champion, Queen, Rook, Bishop, or Knight** (FSF `promotionPieceTypes`).
//!
//! ## Validation
//!
//! The available Fairy-Stockfish binary is a **non-large-board build** and does not
//! implement `omicron` (asked for `UCI_Variant omicron` it silently falls back to
//! standard chess and returns chess-root perft counts), so Omicron carries **no live
//! FSF perft oracle**. Like the other oracle-less variants (Gustav 3, Okisaki Shogi,
//! Yari Shogi, Wa Shogi, Alice; see `docs/oracle-less-validation.md`) it is
//! *rules-validated*: `tests/perft_omicron.rs` hand-derives the start-position move
//! count and cross-checks the engine's perft node-for-node against a fully
//! **independent, from-scratch 12x10 generator** (issue #500's
//! two-implementations-agree pattern).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Geometry, Omicron12x10, PromotionConfig, Square, StandardChess,
    WideRole, WideVariant,
};
use crate::Color;

/// The confirmed Omicron starting placement in the mcr dialect (Champion = `**x`/`**X`,
/// Wizard = `**w`/`**W`; the wall squares render as ordinary empty squares).
///
/// Ranks 10→1: the black Wizards on the a10/l10 corners; the black interior back rank;
/// the black pawns; four empty interior ranks; the white pawns; the white interior back
/// rank; the white Wizards on the a1/l1 corners.
const OMICRON_START_PLACEMENT: &str =
    "**w10**w/1**xrnbqkbnr**x1/1pppppppppp1/12/12/12/12/1PPPPPPPPPP1/1**XRNBQKBNR**X1/**W10**W";

/// The kingside castle side index, matching the position layer's `KINGSIDE`.
const KINGSIDE: usize = 0;
/// The queenside castle side index, matching the position layer's `QUEENSIDE`.
const QUEENSIDE: usize = 1;

/// The Wizard's leaps (Betza `CF`): the eight Camel `(±1,±3)`/`(±3,±1)` jumps and the
/// four Ferz `(±1,±1)` diagonal one-steps.
const WIZARD_OFFSETS: [(i8, i8); 12] = [
    (1, 3),
    (1, -3),
    (-1, 3),
    (-1, -3),
    (3, 1),
    (3, -1),
    (-3, 1),
    (-3, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

/// The Champion's leaps (FSF `DAW` = Betza `WAD`): the four Wazir `(±1,0)`/`(0,±1)`
/// one-steps, the four Dabbaba `(±2,0)`/`(0,±2)` jumps, and the four Alfil `(±2,±2)`
/// jumps.
const CHAMPION_OFFSETS: [(i8, i8); 12] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (2, 0),
    (-2, 0),
    (0, 2),
    (0, -2),
    (2, 2),
    (2, -2),
    (-2, 2),
    (-2, -2),
];

/// The Omicron rule layer: a zero-sized [`WideVariant`] over [`Omicron12x10`].
///
/// It overrides the starting array (Wizards in the corners, Champions flanking the
/// back rank), the two leapers' movement (Wizard, Champion), the wider promotion set,
/// the custom castle rank (2 / 9), destination files (i/e) and rook files (j/c), the
/// rank-9 promotion zone, and the static wall mask on the a/l files and top/bottom
/// ranks (bar the four corners). Every other rule — pawns, en passant, check /
/// checkmate — is standard chess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct OmicronRules;

impl WideVariant<Omicron12x10> for OmicronRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field: through the Champion ([`WideRole::TencubedChampion`], index
    /// 107), so the span is 108 — the same as Ten-Cubed. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 108;

    /// The western **fifty-move rule** (FSF `nMoveRule = 50`, i.e. 100 plies) for this
    /// standard-army board. Adjudication-only (the clock never gates move generation),
    /// so perft stays byte-identical.
    fn move_rule_plies() -> Option<u16> {
        Some(100)
    }

    /// Records a position history for the standard **threefold** repetition draw.
    /// History-dependent and never consulted by a bare [`GenericPosition`], so perft is
    /// unchanged.
    fn tracks_repetition() -> bool {
        true
    }

    fn starting_position() -> (Board<Omicron12x10>, GenericState<Omicron12x10>) {
        let board = Board::<Omicron12x10>::from_fen_placement(OMICRON_START_PLACEMENT)
            .expect("the Omicron starting placement is valid on a 12x10 board");
        // The castling rooks are the c-file (queenside, file 2) and j-file (kingside,
        // file 9) rooks — the Champions occupy the b/k corners of the interior back
        // rank, so these are the outermost rooks the `KQkq` field names. The king and
        // rooks live on rank 2 / 9 (see `castle_rank`); the king castles to i/e (see
        // `castle_dest_files`).
        let mut castling = GenericCastling::NONE;
        for color in Color::ALL {
            castling.set(color, KINGSIDE, Some(9)); // j-file rook
            castling.set(color, QUEENSIDE, Some(2)); // c-file rook
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
            board_b: Bitboard::EMPTY,
            petrified: Bitboard::EMPTY,
            checks_against: [0, 0],
            jieqi_seed: None,
        };
        (board, state)
    }

    /// The static wall squares. The a-/l-files (0 / 11) exist only on ranks 1 and 10
    /// (the corner Wizards), so ranks 2..9 (0-based 1..=8) of those files are blocked;
    /// the top/bottom ranks (0-based 0 / 9) exist only on the a/l corners, so their
    /// interiors — files b..k (0-based 1..=10) — are blocked. 36 walls in all; the play
    /// area is the 10x8 interior plus the four corners.
    fn board_walls() -> Bitboard<Omicron12x10> {
        let mut walls = Bitboard::EMPTY;
        // a-/l-file walls (ranks 2..9, i.e. 0-based 1..=8).
        for rank in 1u8..=(Omicron12x10::HEIGHT - 2) {
            for file in [0u8, Omicron12x10::WIDTH - 1] {
                if let Some(sq) = Square::<Omicron12x10>::from_file_rank(file, rank) {
                    walls = walls.with(sq);
                }
            }
        }
        // top/bottom-rank walls (files b..k, i.e. 0-based 1..=10).
        for file in 1u8..=(Omicron12x10::WIDTH - 2) {
            for rank in [0u8, Omicron12x10::HEIGHT - 1] {
                if let Some(sq) = Square::<Omicron12x10>::from_file_rank(file, rank) {
                    walls = walls.with(sq);
                }
            }
        }
        walls
    }

    /// King safety runs on the make/unmake multi-royal path so that every generated
    /// target — the king's own steps included — is masked against the walls (the fast
    /// single-king generator does not consult the wall mask). Result-identical to the
    /// standard path for a lone royal king.
    fn multi_royal() -> bool {
        true
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Omicron12x10>,
        occupancy: Bitboard<Omicron12x10>,
    ) -> Bitboard<Omicron12x10> {
        match role {
            // Wizard (Camel + Ferz) and Champion (Wazir + Alfil + Dabbaba): pure
            // leapers over any intervening piece.
            WideRole::Wizard => attacks::leaper_attacks::<Omicron12x10>(sq, &WIZARD_OFFSETS),
            WideRole::TencubedChampion => {
                attacks::leaper_attacks::<Omicron12x10>(sq, &CHAMPION_OFFSETS)
            }
            // Everything else is standard chess.
            _ => <StandardChess as WideVariant<Omicron12x10>>::role_attacks(
                role, color, sq, occupancy,
            ),
        }
    }

    /// The Champion ([`WideRole::TencubedChampion`]) leaps two squares straight
    /// (Dabbaba) and two diagonally (Alfil) — jumps **collinear** with a pin ray — so a
    /// pinned Champion could otherwise vault past its own king to a far-side square on
    /// the pin line. Confine every pinned piece to the king–pinner segment so that
    /// cannot happen (matching Fairy-Stockfish and Ten-Cubed); ordinary sliders are
    /// unaffected, as occupancy already limits them to the segment.
    fn confine_pins_to_segment() -> bool {
        true
    }

    fn promotion_config() -> PromotionConfig {
        // FSF `promotionPieceTypes = piece_set(Wizard) | Champion | QUEEN | ROOK |
        // BISHOP | KNIGHT`. Order affects only move-enumeration order, not the perft
        // leaf count.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Wizard,
                WideRole::TencubedChampion,
                WideRole::Queen,
                WideRole::Rook,
                WideRole::Bishop,
                WideRole::Knight,
            ],
        }
    }

    /// A pawn promotes on the last **reachable** rank — rank 9 (0-based 8) for white,
    /// rank 2 (0-based 1) for black. FSF's zone is `Rank9 | Rank10`, but a pawn (files
    /// b..k) can never reach rank 10 (those cells are walls), so the effective,
    /// byte-identical zone is the single reachable rank.
    fn promotion_rank(color: Color) -> u8 {
        match color {
            Color::White => Omicron12x10::HEIGHT - 2, // rank 9 (0-based 8)
            Color::Black => 1,                        // rank 2 (0-based 1)
        }
    }

    fn double_push_rank(color: Color) -> u8 {
        // Pawns start on (and double-push from) rank 3 (index 2) for white and rank 8
        // (index 7) for black — FSF's `doubleStepRegion` Rank3 / Rank8.
        match color {
            Color::White => 2,
            Color::Black => 7,
        }
    }

    fn has_castling() -> bool {
        true
    }

    fn castle_rank(color: Color) -> u8 {
        // King and rooks live on rank 2 (index 1) for white, rank 9 (index 8) for
        // black — the Wizards hold the true corner squares on rank 1 / 10 (FSF
        // `castlingRank = RANK_2`).
        match color {
            Color::White => 1,
            Color::Black => Omicron12x10::HEIGHT - 2,
        }
    }

    fn castle_dest_files(side: usize) -> (u8, u8) {
        // FSF `castlingKingsideFile = FILE_I` (8), `castlingQueensideFile = FILE_E`
        // (4), with the rook ending beside the king toward the centre. The king starts
        // on the g-file (6).
        if side == KINGSIDE {
            // King g2 -> i2 (file 8); rook j2 -> h2 (file 7).
            (8, 7)
        } else {
            // King g2 -> e2 (file 4); rook c2 -> f2 (file 5).
            (4, 5)
        }
    }

    /// Omicron keeps the standard chess army plus the always-mating Wizard and
    /// Champion, so the ordinary insufficient-material draw applies: king vs king, king
    /// and a lone minor vs king, and same-colour bishops only. Every fairy piece counts
    /// as mating material. Adjudication-only and behind the default-off hook, so perft
    /// stays byte-identical.
    fn is_insufficient_material<const R: usize>(
        board: &Board<Omicron12x10, R>,
        _state: &GenericState<Omicron12x10, R>,
    ) -> bool {
        crate::geometry::variant::standard_insufficient_material(board)
    }
}

/// Omicron chess as a [`GenericPosition`] over the 12x10 [`Omicron12x10`] geometry.
///
/// Construct the starting position with
/// [`Omicron::startpos`](GenericPosition::startpos) or parse a FEN (mcr dialect) with
/// [`Omicron::from_fen`](GenericPosition::from_fen). See the [module docs](self) for
/// the army (Wizard + Champion leapers), the walled board, and the castling / promotion
/// rules.
pub type Omicron = GenericPosition<
    Omicron12x10,
    OmicronRules,
    { <OmicronRules as WideVariant<Omicron12x10>>::ROLE_SPAN },
>;

#[cfg(test)]
mod tests {
    use super::*;

    /// The canonical start FEN round-trips (walls render as empty squares) and the
    /// hand-derived opening move count is reproduced.
    #[test]
    fn startpos_round_trips() {
        let pos = Omicron::startpos();
        assert_eq!(
            pos.to_fen(),
            "**w10**w/1**xrnbqkbnr**x1/1pppppppppp1/12/12/12/12/1PPPPPPPPPP1/1**XRNBQKBNR**X1/**W10**W w KQkq - 0 1"
        );
    }

    /// A wall blocks a slider exactly like an occupied square and no piece may land on
    /// one. A White rook on the interior slides along its rank but is stopped short of
    /// the walled a-/l-files.
    #[test]
    fn walls_block_and_are_unreachable() {
        // Rook on d6 (file 3, rank 6) slides west along rank 6: c6/b6 are open interior
        // but a6 is a wall, so it reaches b6 yet never a6.
        let pos = Omicron::from_fen("**w10**w/10k1/12/12/3R8/12/12/12/6K5/**W10**W w - - 0 1")
            .expect("valid Omicron FEN");
        let dests: alloc::vec::Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.to_uci::<Omicron12x10>().starts_with("d6"))
            .map(|m| m.to_uci::<Omicron12x10>()[2..].to_string())
            .collect();
        assert!(
            dests.contains(&"b6".to_string()),
            "rook reaches the open b6"
        );
        assert!(
            !dests.contains(&"a6".to_string()),
            "rook can never land on the a6 wall"
        );
    }

    /// The Champion ([`WideRole::TencubedChampion`]) leaps as Wazir + Alfil + Dabbaba
    /// (twelve targets on an open interior), jumping over intervening pieces.
    #[test]
    fn champion_leaps_as_wad() {
        // A lone White Champion on e6 (file 4, rank 6), kings tucked away, reaches all
        // twelve WAD targets on the open interior.
        let pos = Omicron::from_fen("**w10**w/10k1/12/12/4**X7/12/12/12/6K5/**W10**W w - - 0 1")
            .expect("valid Omicron FEN");
        let n = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.to_uci::<Omicron12x10>().starts_with("e6"))
            .count();
        assert_eq!(n, 12, "an open-board Champion has twelve WAD leaps");
    }

    /// The corner Wizard ([`WideRole::Wizard`]) leaps as Camel + Ferz, and its leaps are
    /// masked against the walls (it cannot land on a walled cell). From a1 the Ferz step
    /// to b2 is onto its own Champion; its Camel/Ferz leaps otherwise reach the interior.
    #[test]
    fn corner_wizard_leaps_as_cf() {
        let pos = Omicron::startpos();
        let a1_targets: alloc::vec::Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.to_uci::<Omicron12x10>().starts_with("a1"))
            .map(|m| m.to_uci::<Omicron12x10>())
            .collect();
        // From a1 (file 0, rank 0): Ferz b2 is a friendly Champion; Camel (1,3)->b4 and
        // (3,1)->d2 (d2 friendly rook). Only b4 is an empty interior target at start.
        assert_eq!(
            a1_targets,
            alloc::vec!["a1b4".to_string()],
            "cornered wizard opening: only the Camel leap a1-b4"
        );
    }

    /// Kingside castling lands the king on the custom **i**-file with the rook beside it
    /// on **h**; the king and rook castle on rank 2.
    #[test]
    fn kingside_castle_lands_on_i_file() {
        // King g2, kingside rook j2, h2/i2 empty; Black king tucked on b9.
        let pos = Omicron::from_fen("**w10**w/1k10/12/12/12/12/12/12/6K2R2/**W10**W w K - 0 1")
            .expect("valid Omicron FEN");
        assert!(
            pos.legal_moves()
                .into_iter()
                .any(|m| m.to_uci::<Omicron12x10>() == "g2i2"),
            "the king must castle from g2 to i2"
        );
    }
}
