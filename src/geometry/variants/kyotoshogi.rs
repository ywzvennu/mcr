//! Kyoto Shogi (5x5 flipping Shogi) on the generic engine — a reuse of the
//! Minishogi (#195) / Shogi (#190) **persistent hand**, **drops**, and 5x5
//! [`Minishogi5x5`] geometry, with one distinctive new mechanic: **every piece
//! flips to its alternate form after each move it makes**. Validated against
//! Fairy-Stockfish `UCI_Variant kyotoshogi`.
//!
//! Kyoto Shogi (京都将棋) is a 5x5 Shogi where each non-royal piece carries **two
//! forms** and **alternates between them move-to-move**: making a move toggles the
//! moving piece's form. There is no promotion *zone* — the flip is unconditional
//! and happens on **every** board move, anywhere on the board. Captured pieces
//! enter the hand in their **base** form, and a held piece may be **dropped in
//! either form** (FSF `dropPromoted`). The King has no alternate form and never
//! flips.
//!
//! ## The five flipping pairs (confirmed against FSF)
//!
//! Each pair is `base ↔ promoted`; the form on the left moves as the named piece,
//! and a move flips it to the form on the right (and vice-versa):
//!
//! | base (moves as) | promoted (moves as) | base role | promoted role |
//! |-----------------|---------------------|-----------|---------------|
//! | **P** Pawn (1 step forward) | **+P** Rook | [`WideRole::Pawn`] | [`WideRole::Tokin`] |
//! | **S** Silver General | **+S** Bishop | [`WideRole::Silver`] | [`WideRole::PromotedSilver`] |
//! | **L** Lance (forward slider) | **+L** Gold General | [`WideRole::Lance`] | [`WideRole::PromotedLance`] |
//! | **N** Shogi Knight (2-1 forward jump) | **+N** Gold General | [`WideRole::Knight`] | [`WideRole::PromotedKnight`] |
//! | **K** King | — (never flips) | [`WideRole::King`] | — |
//!
//! The pairs **reuse the existing Shogi promoted roles** (`Tokin`,
//! `PromotedSilver`, `PromotedLance`, `PromotedKnight`) purely as the
//! "alternate-form" markers — they already carry the right [`+`-prefixed FEN
//! token](crate::geometry::WideRole::is_promoted), and the right base on capture
//! ([`WideRole::promoted_base`]: `+P→P`, `+S→S`, `+L→L`, `+N→N`) — but Kyoto gives
//! several of them **different movement** than Shogi does: a Shogi `+P`/`+S` move
//! as a Gold, whereas Kyoto's `+P` moves as a **Rook** and `+S` as a **Bishop**.
//! The movement is supplied here by [`role_attacks`](KyotoshogiRules::role_attacks),
//! independent of the Shogi rule layer, so the two variants stay byte-identical.
//!
//! The flip itself is the default-off [`WideVariant::flips_on_move`] hook (a base
//! role flips to its [`WideRole::promoted_form`], a promoted role back to its
//! [`WideRole::promoted_base`]); the dual-form drops are the default-off
//! [`WideVariant::drops_can_promote`] hook. Both are inert for every other variant.
//!
//! ## Hand and drops
//!
//! A captured piece is banked **unpromoted** and flipped to the captor's side
//! (e.g. a captured `+S` enters the hand as a Silver). On a turn a side may,
//! instead of a board move, **drop** a held piece onto any empty square, choosing
//! either its base or its promoted form. Kyoto imposes **no** drop restriction
//! (FSF `immobilityIllegal = false`, `dropNoDoubled = none`): a Pawn may be
//! dropped on the last rank and a file may hold any number of Pawns.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant kyotoshogi`, `position startpos`) renders the start as
//!
//! ```text
//! p+nks+l/5/5/5/+LSK+NP[-] w 0 1
//! ```
//!
//! mcr uses the same board placement (`+`-prefixed promoted tokens are written by
//! the shared board FEN I/O) and an empty `[]` holdings bracket; the
//! `compare-fairy/` harness reconciles the empty-hand rendering when driving FSF.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{attacks, Bitboard, Board, PromotionConfig, Square, WideRole, WideVariant};
use crate::Color;

use super::super::Minishogi5x5;

/// The Kyoto Shogi rule layer: a zero-sized [`WideVariant`] over [`Minishogi5x5`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct KyotoshogiRules;

/// The confirmed Kyoto Shogi starting placement (the hand is empty at the start).
///
/// `p+nks+l` (Black back rank, rank 5) / three empty ranks / `+LSK+NP` (White back
/// rank, rank 1): a base Pawn and Silver, a King, and a promoted (Gold-moving)
/// Knight and Lance per side.
const KYOTOSHOGI_PLACEMENT: &str = "p+nks+l/5/5/5/+LSK+NP";

/// The four diagonal one-step (ferz) offsets.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

impl KyotoshogiRules {
    /// The Gold General's attack set from `sq` for `color`: one step orthogonally
    /// (four directions) plus one step diagonally **forward** (two directions) —
    /// six squares. The promoted Lance (`+L`) and promoted Knight (`+N`) move as a
    /// Gold.
    fn gold_attacks(color: Color, sq: Square<Minishogi5x5>) -> Bitboard<Minishogi5x5> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        let offsets = [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            // The two forward diagonals.
            (1, fwd),
            (-1, fwd),
        ];
        attacks::leaper_attacks::<Minishogi5x5>(sq, &offsets)
    }

    /// The Silver General's attack set: the four diagonals plus one straight
    /// forward step (five squares).
    fn silver_attacks(color: Color, sq: Square<Minishogi5x5>) -> Bitboard<Minishogi5x5> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = attacks::leaper_attacks::<Minishogi5x5>(sq, &FERZ_OFFSETS);
        if let Some(dest) = sq.offset(0, fwd) {
            bb.set(dest);
        }
        bb
    }

    /// The Shogi Knight's attack set: the two forward 2-1 jumps (it never moves
    /// sideways or backward). Distinct from a standard chess Knight; this is the
    /// movement of Kyoto's base Knight (`N`).
    fn knight_attacks(color: Color, sq: Square<Minishogi5x5>) -> Bitboard<Minishogi5x5> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        attacks::leaper_attacks::<Minishogi5x5>(sq, &[(1, 2 * fwd), (-1, 2 * fwd)])
    }

    /// The Pawn's attack/movement square: the single square straight forward (it
    /// both moves and captures there).
    fn pawn_attacks(color: Color, sq: Square<Minishogi5x5>) -> Bitboard<Minishogi5x5> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = Bitboard::<Minishogi5x5>::EMPTY;
        if let Some(dest) = sq.offset(0, fwd) {
            bb.set(dest);
        }
        bb
    }
}

impl WideVariant<Minishogi5x5> for KyotoshogiRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 27;

    fn starting_position() -> (Board<Minishogi5x5>, GenericState<Minishogi5x5>) {
        let board = Board::<Minishogi5x5>::from_fen_placement(KYOTOSHOGI_PLACEMENT)
            .expect("the Kyoto Shogi starting placement is valid on a 5x5 board");
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
        sq: Square<Minishogi5x5>,
        occupancy: Bitboard<Minishogi5x5>,
    ) -> Bitboard<Minishogi5x5> {
        match role {
            // Base forms.
            WideRole::Pawn => Self::pawn_attacks(color, sq),
            WideRole::Silver => Self::silver_attacks(color, sq),
            WideRole::Knight => Self::knight_attacks(color, sq),
            WideRole::Lance => attacks::lance_attacks::<Minishogi5x5>(color, sq, occupancy),
            WideRole::King => attacks::king_attacks::<Minishogi5x5>(sq),
            // Promoted forms — the Kyoto movement, *not* the Shogi one: the
            // promoted Pawn moves as a Rook, the promoted Silver as a Bishop, and
            // the promoted Lance / Knight as a Gold General.
            WideRole::Tokin => attacks::rook_attacks::<Minishogi5x5>(sq, occupancy),
            WideRole::PromotedSilver => attacks::bishop_attacks::<Minishogi5x5>(sq, occupancy),
            WideRole::PromotedLance | WideRole::PromotedKnight => Self::gold_attacks(color, sq),
            _ => Bitboard::EMPTY,
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // The forward-biased pieces: the Pawn (forward step), the Silver and Shogi
        // Knight, the Lance (forward slider), and the Gold-moving promoted Lance /
        // Knight. Their attack sets point forward, so the attacker scan must
        // project the opposite color from the target. The promoted Pawn (Rook) and
        // promoted Silver (Bishop) and the King are color-symmetric.
        matches!(
            role,
            WideRole::Pawn
                | WideRole::Silver
                | WideRole::Knight
                | WideRole::Lance
                | WideRole::PromotedLance
                | WideRole::PromotedKnight
        )
    }

    fn role_is_slider(role: WideRole) -> bool {
        // The promoted Pawn (Rook), the promoted Silver (Bishop), and the Lance
        // slide and so can pin / be pinned along a ray. Every stepper (Gold-moving
        // promoted minors, Silver, Pawn, Knight, King) does not.
        matches!(
            role,
            WideRole::Tokin | WideRole::PromotedSilver | WideRole::Lance
        )
    }

    fn promotion_config() -> PromotionConfig {
        // Kyoto has no promotion *zone* (the flip is per-move, via `flips_on_move`),
        // so this static set is unused; the trait requires it.
        PromotionConfig {
            roles: alloc::vec![WideRole::Gold],
        }
    }

    fn in_promotion_zone(_color: Color, _rank: u8) -> bool {
        // No promotion zone: the per-move flip replaces zone promotion entirely, so
        // the generic per-piece promotion path is never engaged
        // (`role_can_promote` stays `false`).
        false
    }

    fn has_castling() -> bool {
        false
    }

    // --- hand / drops + per-move flip -------------------------------------

    fn has_hand() -> bool {
        true
    }

    fn role_can_promote(_role: WideRole) -> bool {
        // No *zone* promotion (the flip is the per-move `flips_on_move` mechanic),
        // so the generic in-zone promotion expansion never fires. The dual-form
        // drop expansion uses `flips_on_move` to know which roles have a second
        // form, not this hook.
        false
    }

    fn drops_can_promote() -> bool {
        // FSF `dropPromoted`: a held piece may be deployed in its base or promoted
        // form.
        true
    }

    fn flips_on_move(role: WideRole) -> Option<WideRole> {
        // Every move flips the moving piece to its alternate form: a base piece to
        // its promoted form, a promoted piece back to its base. The King has no
        // alternate form and never flips.
        match role {
            WideRole::Pawn
            | WideRole::Silver
            | WideRole::Lance
            | WideRole::Knight
            | WideRole::Tokin
            | WideRole::PromotedSilver
            | WideRole::PromotedLance
            | WideRole::PromotedKnight => Some(if role.is_promoted() {
                role.promoted_base()
            } else {
                role.promoted_form()
            }),
            _ => None,
        }
    }

    // --- Sennichite / perpetual check (default-off draw rules) -------------
    //
    // These affect only terminal adjudication in [`GenericGame`], never move
    // generation, so perft is byte-identical.

    fn tracks_repetition() -> bool {
        true
    }

    fn repetition_fold() -> usize {
        // Sennichite: the same position (including both hands) occurring a fourth
        // time is a draw.
        4
    }

    fn repetition_draw_reason() -> crate::geometry::WideEndReason {
        crate::geometry::WideEndReason::Sennichite
    }

    fn perpetual_check_loses() -> bool {
        // A sennichite brought about by perpetual check is a loss for the checking
        // side.
        true
    }
}

/// Kyoto Shogi (5x5 flipping Shogi) as a [`GenericPosition`] over the 5x5
/// geometry.
///
/// Construct the starting position with
/// [`Kyotoshogi::startpos`](GenericPosition::startpos) or parse a FEN — the
/// placement may carry the hand as a `[..]` holdings bracket — with
/// [`Kyotoshogi::from_fen`](GenericPosition::from_fen). See the [module
/// docs](self) for the per-move flip, the hand, and the dual-form drops.
pub type Kyotoshogi = GenericPosition<Minishogi5x5, KyotoshogiRules>;
