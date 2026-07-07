//! Yari Shogi ("spear shogi") — a 9-rank by 7-file Shogi drop variant.
//!
//! Yari Shogi (槍将棋, [Wikipedia](https://en.wikipedia.org/wiki/Yari_shogi)) is
//! played on a **9-rank by 7-file** board (mcr's [`YariShogi7x9`] geometry, seven
//! files by nine ranks in the module's files-by-ranks naming) with a Shogi-style
//! persistent capture-fed **hand** and **drops**. Its army replaces the Shogi
//! pieces with forward-biased "spear" (yari) pieces, matching Fairy-Stockfish's
//! built-in `yarishogi` variant:
//!
//! * **King (K, 玉)** — the royal one-step mover.
//! * **Shogi Pawn (P, 歩 → Yari Silver)** — one step straight forward, and
//!   captures straight forward too (unlike a chess pawn). Reuses
//!   [`WideRole::Pawn`].
//! * **Rook (`l`)** — a full orthogonal slider; here it is reached only as the
//!   **promoted Yari Rook**, and reuses the standard [`WideRole::Rook`].
//! * **Yari Rook (`r`, `frlR` → Rook)** — slides forward or sideways, never back.
//! * **Yari Knight (`n`, `fRffN` → Yari Gold)** — a forward rook plus the two
//!   narrow-forward knight leaps.
//! * **Yari Bishop (`b`, `fFfR` → Yari Gold)** — a forward rook plus the two
//!   forward diagonal steps.
//! * **Yari Gold (`g`, `WfFbR`)** — a Wazir plus the forward diagonals plus a
//!   backward rook slide; the promoted Yari Knight / Yari Bishop.
//! * **Yari Silver (`s`, `fKbR`)** — the three forward King steps plus a backward
//!   rook slide; the promoted Shogi Pawn.
//!
//! Promotion is offered when a move starts or ends in the **far three ranks** and
//! is **forced** only when the piece would otherwise be immobile (a Pawn, Yari
//! Knight, or Yari Bishop reaching the last rank). Drops follow the Shogi rules:
//! no dead piece (a Pawn / Yari Knight / Yari Bishop may not be dropped on the
//! last rank) and **nifu** (no two unpromoted Pawns of one side on a file). Unlike
//! Shogi, a pawn-drop mate **is** allowed (FSF `shogiPawnDropMateIllegal = false`).
//!
//! ## Validation
//!
//! Yari Shogi is validated **oracle-less**. Although Fairy-Stockfish defines
//! `yarishogi`, the project's built FSF binary is compiled without large boards,
//! so its move generator cannot host the 9-rank board and there is no live FSF
//! oracle (nor a HaChu one). Yari is therefore held to a hand-derived low-depth
//! perft plus the crate-wide make/unmake, perft-children-sum, colour-symmetry, and
//! attacker-consistency invariants (see `docs/oracle-less-validation.md`).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Geometry, PromotionConfig, Square, WideRole, WideVariant,
};
use crate::Color;

use super::super::YariShogi7x9;

/// The Yari Shogi rule layer: a zero-sized [`WideVariant`] over [`YariShogi7x9`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct YariRules;

/// The Yari Shogi starting placement (the hand is empty at the start), in mcr's
/// overflow spelling (`****X` = White, `****x` = Black for the fifth-tier spear
/// roles). Read from the top rank (Black's back rank, rank 9) down to rank 1
/// (White's back rank). The back rank a..g is Yari Rook, Yari Knight, Yari Knight,
/// King, Yari Bishop, Yari Bishop, Yari Rook; the pawns sit on the third rank from
/// each side.
const YARI_PLACEMENT: &str = concat!(
    "****o****j****jk****a****a****o/", // rank 9 (Black back)
    "7/",                               // rank 8
    "ppppppp/",                         // rank 7 (Black pawns)
    "7/7/7/",                           // ranks 6..4
    "PPPPPPP/",                         // rank 3 (White pawns)
    "7/",                               // rank 2
    "****O****A****AK****J****J****O"   // rank 1 (White back)
);

/// The depth of the promotion zone: the furthest three ranks from each side.
const ZONE_DEPTH: u8 = 3;

impl YariRules {
    /// Rotates a White-orientation step into `color`'s orientation: White keeps it,
    /// Black takes its vertical mirror (the rank axis negated). Every spear piece is
    /// left-right symmetric, so the vertical mirror is its Black move set.
    const fn orient((df, dr): (i8, i8), color: Color) -> (i8, i8) {
        match color {
            Color::White => (df, dr),
            Color::Black => (df, -dr),
        }
    }

    /// The leaper attack set for `white_offsets`, oriented for `color`.
    fn leaper(
        color: Color,
        sq: Square<YariShogi7x9>,
        white_offsets: &[(i8, i8)],
    ) -> Bitboard<YariShogi7x9> {
        let mut bb = Bitboard::<YariShogi7x9>::EMPTY;
        for &off in white_offsets {
            let (df, dr) = Self::orient(off, color);
            if let Some(dest) = sq.offset(df, dr) {
                bb.set(dest);
            }
        }
        bb
    }

    /// Slides from `sq` along each White-orientation direction in `white_dirs`
    /// (oriented for `color`), stopping on the first blocker (a capture square is
    /// included). Every spear slide is unbounded.
    fn ray(
        color: Color,
        sq: Square<YariShogi7x9>,
        occupancy: Bitboard<YariShogi7x9>,
        white_dirs: &[(i8, i8)],
    ) -> Bitboard<YariShogi7x9> {
        let mut bb = Bitboard::<YariShogi7x9>::EMPTY;
        for &dir in white_dirs {
            let (df, dr) = Self::orient(dir, color);
            let mut cur = sq.offset(df, dr);
            while let Some(dest) = cur {
                bb.set(dest);
                if occupancy.contains(dest) {
                    break;
                }
                cur = dest.offset(df, dr);
            }
        }
        bb
    }

    /// The last rank for `color` (rank 8 white / rank 0 black) — a Pawn, Yari
    /// Knight, or Yari Bishop there has no further move (forced promotion / no drop).
    fn last_rank(color: Color) -> u8 {
        match color {
            Color::White => YariShogi7x9::HEIGHT - 1,
            Color::Black => 0,
        }
    }

    /// The mask of every square on `rank`.
    fn rank_mask(rank: u8) -> Bitboard<YariShogi7x9> {
        let mut bb = Bitboard::<YariShogi7x9>::EMPTY;
        for file in 0..YariShogi7x9::WIDTH {
            if let Some(sq) = Square::<YariShogi7x9>::from_file_rank(file, rank) {
                bb.set(sq);
            }
        }
        bb
    }

    /// The mask of every square on `file`.
    fn file_mask(file: u8) -> Bitboard<YariShogi7x9> {
        let mut bb = Bitboard::<YariShogi7x9>::EMPTY;
        for rank in 0..YariShogi7x9::HEIGHT {
            if let Some(sq) = Square::<YariShogi7x9>::from_file_rank(file, rank) {
                bb.set(sq);
            }
        }
        bb
    }
}

impl WideVariant<YariShogi7x9> for YariRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field (King, Pawn, Rook, and the five spear roles up to
    /// [`WideRole::YariSilver`] at index 153). See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = WideRole::YariSilver.index() + 1;

    fn starting_position() -> (Board<YariShogi7x9>, GenericState<YariShogi7x9>) {
        let board = Board::<YariShogi7x9>::from_fen_placement(YARI_PLACEMENT)
            .expect("the Yari Shogi starting placement is valid on a 7x9 board");
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::NONE,
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
        };
        (board, state)
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<YariShogi7x9>,
        occupancy: Bitboard<YariShogi7x9>,
    ) -> Bitboard<YariShogi7x9> {
        match role {
            // Royal one-stepper.
            WideRole::King => attacks::king_attacks::<YariShogi7x9>(sq),
            // Shogi Pawn (`fW`): one step straight forward, move and capture alike.
            WideRole::Pawn => Self::leaper(color, sq, &[(0, 1)]),
            // Yari Rook (`frlR`): forward, left, and right rook slides — never back.
            WideRole::YariRook => Self::ray(color, sq, occupancy, &[(0, 1), (1, 0), (-1, 0)]),
            // Yari Knight (`fRffN`): a forward rook plus the two narrow-forward
            // knight leaps `(±1, +2)`.
            WideRole::YariKnight => {
                Self::ray(color, sq, occupancy, &[(0, 1)])
                    | Self::leaper(color, sq, &[(1, 2), (-1, 2)])
            }
            // Yari Bishop (`fFfR`): a forward rook plus the two forward-diagonal
            // Ferz steps.
            WideRole::YariBishop => {
                Self::ray(color, sq, occupancy, &[(0, 1)])
                    | Self::leaper(color, sq, &[(1, 1), (-1, 1)])
            }
            // Yari Gold (`WfFbR`): the four orthogonal steps and the two forward
            // diagonals, plus a backward rook slide (the straight-back Wazir step
            // extended to a full slide).
            WideRole::YariGold => {
                Self::leaper(color, sq, &[(1, 0), (-1, 0), (0, 1), (1, 1), (-1, 1)])
                    | Self::ray(color, sq, occupancy, &[(0, -1)])
            }
            // Yari Silver (`fKbR`): the three forward King steps plus a backward
            // rook slide.
            WideRole::YariSilver => {
                Self::leaper(color, sq, &[(0, 1), (1, 1), (-1, 1)])
                    | Self::ray(color, sq, occupancy, &[(0, -1)])
            }
            // The promoted Yari Rook is a standard full orthogonal rook.
            WideRole::Rook => attacks::rook_attacks::<YariShogi7x9>(sq, occupancy),
            _ => Bitboard::EMPTY,
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // Every spear piece is forward-biased (its attack set is not symmetric
        // under a vertical flip), so an attacker of one colour is found by
        // projecting the opposite colour's pattern back from the target square, as
        // a Shogi Pawn / Lance is. Every spear piece is left-right symmetric, so
        // this colour-flipped reverse projection is exact. The King and the
        // (symmetric) promoted Rook take the default reverse projection.
        matches!(
            role,
            WideRole::Pawn
                | WideRole::YariRook
                | WideRole::YariKnight
                | WideRole::YariBishop
                | WideRole::YariGold
                | WideRole::YariSilver
        )
    }

    fn role_is_slider(role: WideRole) -> bool {
        // Every spear piece with an unbounded rook ride can pin / be pinned along a
        // ray (the Yari Knight and Yari Bishop ride forward; the Yari Gold and Yari
        // Silver ride backward; the Yari Rook rides forward and sideways; the
        // promoted Rook rides everywhere). The Pawn and King are steppers.
        matches!(
            role,
            WideRole::YariRook
                | WideRole::YariKnight
                | WideRole::YariBishop
                | WideRole::YariGold
                | WideRole::YariSilver
                | WideRole::Rook
        )
    }

    fn promotion_config() -> PromotionConfig {
        // Yari's promotions are per-piece (each promotable base has exactly one
        // promoted form, handled by the generic per-piece promotion path); this
        // static set is unused, but the trait requires it.
        PromotionConfig {
            roles: alloc::vec![WideRole::YariGold],
        }
    }

    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        match color {
            Color::White => rank >= YariShogi7x9::HEIGHT - ZONE_DEPTH,
            Color::Black => rank < ZONE_DEPTH,
        }
    }

    fn has_castling() -> bool {
        false
    }

    // --- hand / drops + per-piece promotion -------------------------------

    fn has_hand() -> bool {
        true
    }

    fn role_can_promote(role: WideRole) -> bool {
        // The promotable base pieces; the King, the Yari Gold / Yari Silver, and the
        // promoted Rook never promote, and a piece already promoted does not promote
        // again.
        matches!(
            role,
            WideRole::Pawn | WideRole::YariRook | WideRole::YariKnight | WideRole::YariBishop
        )
    }

    fn role_promoted_to(role: WideRole) -> WideRole {
        match role {
            WideRole::Pawn => WideRole::YariSilver,
            WideRole::YariRook => WideRole::Rook,
            // Both the Yari Knight and the Yari Bishop promote to a Yari Gold.
            WideRole::YariKnight | WideRole::YariBishop => WideRole::YariGold,
            other => other,
        }
    }

    fn role_promotion_forced(role: WideRole, color: Color, to_rank: u8) -> bool {
        match role {
            // On the last rank a Pawn (forward step), Yari Knight (forward slide +
            // forward jumps), or Yari Bishop (forward slide + forward diagonals) has
            // no further move, so it must promote. The Yari Rook keeps its sideways
            // slides and is never forced.
            WideRole::Pawn | WideRole::YariKnight | WideRole::YariBishop => {
                to_rank == Self::last_rank(color)
            }
            _ => false,
        }
    }

    fn role_hand_base(role: WideRole) -> WideRole {
        // A captured promoted piece sheds its promotion before entering the hand.
        // The Yari Gold is the promoted form of *both* the Yari Knight and the Yari
        // Bishop; matching Fairy-Stockfish's canonical (last-assignment) demotion,
        // a captured Yari Gold banks as a Yari Bishop.
        match role {
            WideRole::YariSilver => WideRole::Pawn,
            WideRole::Rook => WideRole::YariRook,
            WideRole::YariGold => WideRole::YariBishop,
            other => other,
        }
    }

    // NOTE on *uchifuzume* (no pawn-drop mate): unlike orthodox Shogi, Yari Shogi
    // permits a pawn drop that delivers immediate checkmate (FSF
    // `shogiPawnDropMateIllegal = false`), so `pawn_drop_mate_forbidden` stays at
    // its `false` default and no uchifuzume filter is applied.

    fn drop_targets<const R: usize>(
        role: WideRole,
        color: Color,
        board: &Board<YariShogi7x9, R>,
    ) -> Bitboard<YariShogi7x9> {
        let mut mask = !board.occupied();
        // Dead-piece rule: a dropped Pawn, Yari Knight, or Yari Bishop may not land
        // on the last rank (it would then have no move).
        if matches!(
            role,
            WideRole::Pawn | WideRole::YariKnight | WideRole::YariBishop
        ) {
            mask &= !Self::rank_mask(Self::last_rank(color));
        }
        // Nifu: a Pawn may not be dropped onto a file that already holds an
        // unpromoted friendly Pawn (a promoted Yari Silver does not count).
        if role == WideRole::Pawn {
            for pawn in board.pieces(color, WideRole::Pawn) {
                mask &= !Self::file_mask(pawn.file());
            }
        }
        mask
    }

    // --- Sennichite / perpetual check / stalemate (terminal adjudication) --
    //
    // These affect only terminal adjudication in [`GenericGame`], never move
    // generation, so perft is byte-identical.

    fn tracks_repetition() -> bool {
        true
    }

    fn repetition_fold() -> usize {
        // Sennichite: the same position (including both hands) repeating is a draw
        // (Fairy-Stockfish `yarishogi` `nFoldRule = 3`).
        3
    }

    fn repetition_draw_reason() -> crate::geometry::WideEndReason {
        crate::geometry::WideEndReason::Sennichite
    }

    fn perpetual_check_loses() -> bool {
        // A repetition brought about by perpetual check is a loss for the checking
        // side (FSF `perpetualCheckIllegal = true`).
        true
    }

    fn stalemate_is_loss() -> bool {
        // Stalemate is a loss for the stalemated side (FSF `stalemateValue =
        // -VALUE_MATE`); adjudication only, so perft is byte-identical.
        true
    }
}

/// Yari Shogi ("spear shogi") as a [`GenericPosition`] over the 7x9 geometry.
///
/// Construct the starting position with
/// [`Yari::startpos`](GenericPosition::startpos) or parse a FEN — the placement
/// may carry the hand as a `[..]` holdings bracket — with
/// [`Yari::from_fen`](GenericPosition::from_fen). See the [module docs](self) for
/// the spear army, the hand, drops, and the promotion zone.
pub type Yari = GenericPosition<
    YariShogi7x9,
    YariRules,
    { <YariRules as WideVariant<YariShogi7x9>>::ROLE_SPAN },
>;
