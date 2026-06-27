//! The wide variant trait: the generic analogue of the concrete
//! [`crate::variant::Variant`] for the large-board [`Geometry`] layer.
//!
//! Where the concrete [`Variant`](crate::variant::Variant) drives the frozen 8x8
//! [`crate::Position`], [`WideVariant`] drives a
//! [`GenericPosition<G, V>`](super::position::GenericPosition) over an arbitrary
//! [`Geometry`]. It is a zero-sized rule layer — every method has a sensible
//! default implementing **standard chess rules**, so a variant overrides only
//! the hooks it changes, exactly as the concrete trait does
//! (`docs/fairy-variants-architecture.md` §4, §5).
//!
//! The reference instantiation, [`StandardChess`], overrides nothing but the
//! starting array, proving the generic engine reproduces concrete 8x8 perft.
//! The fairy hooks (drops, regions, multi-royal sets) are present as reserved
//! no-ops so later phases extend the trait without churn.

use alloc::vec::Vec;

use super::attacks;
use super::position::{GenericCastling, GenericGating, GenericState};
use super::role::WideRole;
use super::{Bitboard, Board, Geometry, Square};
use crate::Color;

/// A region of the board a variant may mask off (palace, river-half, promotion
/// zone). Reserved for Phase 3 (Xiangqi/Janggi) region confinement; the standard
/// rules never consult it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WideRegion {
    /// The promotion zone for the given color (the squares on which a pawn-like
    /// piece promotes, or from which it must).
    PromotionZone(Color),
    /// The palace mask for the given color (Xiangqi/Janggi). Reserved.
    Palace(Color),
    /// The own-half / river-bound mask for the given color. Reserved.
    OwnHalf(Color),
}

/// The promotion configuration a variant exposes: which squares promote and to
/// which roles. The default is standard chess — the last rank, promoting to
/// knight, bishop, rook, or queen.
#[derive(Debug, Clone)]
pub struct PromotionConfig {
    /// The roles a promoting pawn may become, in a deterministic order. For
    /// standard chess this is `[Knight, Bishop, Rook, Queen]` (the same order
    /// the concrete engine emits).
    pub roles: Vec<WideRole>,
}

/// The wide variant trait: a zero-sized rule layer over a [`Geometry`].
///
/// Every method defaults to standard chess, so [`StandardChess`] need only
/// supply the starting board. The trait is the single extension point for the
/// Milestone 10 fairy variants: each implements only the hooks whose behaviour
/// differs from the standard defaults below.
///
/// Implementors are zero-sized markers (`Copy + 'static`), so a
/// [`GenericPosition<G, V>`](super::position::GenericPosition) monomorphises to
/// dispatch-free code — there is no per-hook vtable, exactly as the concrete
/// [`Variant`](crate::variant::Variant) layer guarantees.
pub trait WideVariant<G: Geometry>: Copy + 'static {
    /// Returns the starting board and state for a fresh game of this variant.
    ///
    /// The board carries the piece placement; the state carries the side to
    /// move, castling rights, en-passant target, and clocks.
    fn starting_position() -> (Board<G>, GenericState<G>);

    /// Returns the pseudo-attacks of a `role` of `color` standing on `sq` under
    /// the given `occupancy`.
    ///
    /// This is the movement vocabulary of the variant. The default covers the
    /// standard six (pawn diagonals, knight, bishop/rook/queen sliders, king
    /// steps) plus the two census compounds — `Hawk = Bishop + Knight` and
    /// `Elephant = Rook + Knight` — built from the generic [`attacks`]
    /// primitives. A variant adding a new role overrides this to extend the
    /// match.
    ///
    /// For a pawn this returns only the two diagonal capture squares; the
    /// forward pushes are handled by the position's pawn generator, which needs
    /// the occupancy and the double-push / promotion geometry the attack set
    /// does not carry.
    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<G>,
        occupancy: Bitboard<G>,
    ) -> Bitboard<G> {
        match role {
            WideRole::Pawn => attacks::pawn_attacks(color, sq),
            WideRole::Knight => attacks::knight_attacks(sq),
            WideRole::Bishop => attacks::bishop_attacks(sq, occupancy),
            WideRole::Rook => attacks::rook_attacks(sq, occupancy),
            WideRole::Queen => attacks::queen_attacks(sq, occupancy),
            WideRole::King => attacks::king_attacks(sq),
            // Census compounds (Seirawan / Capablanca family).
            WideRole::Hawk => attacks::bishop_attacks(sq, occupancy) | attacks::knight_attacks(sq),
            WideRole::Elephant => {
                attacks::rook_attacks(sq, occupancy) | attacks::knight_attacks(sq)
            }
            // Other fairy roles have no standard movement; a variant that uses
            // them overrides this hook. Returning empty keeps the default total.
            _ => Bitboard::EMPTY,
        }
    }

    /// Returns `true` if a piece of `role` slides (its attack set depends on the
    /// occupancy and is blocked along rays). Steppers return `false`. Used by the
    /// generic generator to decide whether a piece can be pinned along a line.
    ///
    /// The default classifies the standard sliders and the two compounds; their
    /// sliding component can be pinned, so they are treated as sliders.
    fn role_is_slider(role: WideRole) -> bool {
        matches!(
            role,
            WideRole::Bishop
                | WideRole::Rook
                | WideRole::Queen
                | WideRole::Hawk
                | WideRole::Elephant
        )
    }

    /// Returns the promotion configuration. The default is the standard
    /// `[Knight, Bishop, Rook, Queen]`.
    fn promotion_config() -> PromotionConfig {
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Knight,
                WideRole::Bishop,
                WideRole::Rook,
                WideRole::Queen,
            ],
        }
    }

    /// Returns the legal promotion target roles for a pawn of `color` on the
    /// current `board`, in a deterministic order.
    ///
    /// The default ignores `board` and returns
    /// [`promotion_config`](WideVariant::promotion_config)'s static role set —
    /// the behaviour of every variant whose promotion targets are fixed (standard
    /// chess, Makruk, Capablanca, Seirawan). Only a variant whose legal targets
    /// depend on the running position overrides this. Grand chess does: a pawn may
    /// promote to a type only while the player has fewer than the **starting army
    /// count** of that type on the board (Archbishop / Chancellor / Queen at most
    /// one, Rook / Bishop / Knight at most two) — equivalently, only to a type the
    /// player has had captured. The set is read live from the board, so no extra
    /// position state is needed and every non-overriding variant enumerates
    /// byte-identically to a build without this hook.
    fn promotion_targets(_color: Color, _board: &Board<G>) -> Vec<WideRole> {
        Self::promotion_config().roles
    }

    /// Returns the rank (0-based) a pawn of `color` promotes on. The default is
    /// the furthest rank: `HEIGHT - 1` for white, `0` for black.
    fn promotion_rank(color: Color) -> u8 {
        match color {
            Color::White => G::HEIGHT - 1,
            Color::Black => 0,
        }
    }

    /// Returns `true` if a pawn of `color` arriving on `rank` is **in the
    /// promotion zone** — i.e. it may (or must) promote there.
    ///
    /// The default is the single promotion rank ([`promotion_rank`]): standard
    /// chess, Makruk, Capablanca, and Seirawan all promote on exactly one rank, so
    /// the zone is that rank and nothing changes for them. Grand chess overrides
    /// this to a three-rank zone (the far three ranks): promotion is *available*
    /// throughout the zone but only *forced* on the last rank (see
    /// [`promotion_is_forced`](WideVariant::promotion_is_forced)).
    ///
    /// [`promotion_rank`]: WideVariant::promotion_rank
    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        rank == Self::promotion_rank(color)
    }

    /// Returns `true` if a pawn of `color` arriving on `rank` (already known to be
    /// [`in_promotion_zone`](WideVariant::in_promotion_zone)) **must** promote —
    /// a non-promoting move to that square is then illegal.
    ///
    /// The default is "always forced" (`true`): in the single-rank model the
    /// promotion rank is the last rank, where a pawn cannot stay a pawn, so every
    /// existing variant forces promotion and emits no non-promoting alternative —
    /// byte-identical to before this hook. Grand chess overrides this so promotion
    /// is *optional* on the near zone ranks (a plain push or capture is also
    /// legal) and forced only on the final rank, matching Fairy-Stockfish's
    /// `mandatoryPawnPromotion = false` with `immobilityIllegal = true`.
    fn promotion_is_forced(color: Color, _rank: u8) -> bool {
        let _ = color;
        true
    }

    /// Returns the rank (0-based) from which a pawn of `color` may make its
    /// initial double advance. The default is the standard second rank: rank `1`
    /// for white, `HEIGHT - 2` for black.
    fn double_push_rank(color: Color) -> u8 {
        match color {
            Color::White => 1,
            Color::Black => G::HEIGHT - 2,
        }
    }

    /// Returns `true` if this variant offers standard castling. The default is
    /// `true`. A variant without castling overrides this to `false`.
    fn has_castling() -> bool {
        true
    }

    /// Returns the castle destination files `(king_dest_file, rook_dest_file)`
    /// for a castling side (`0` = kingside, `1` = queenside).
    ///
    /// The default is the standard 8x8 geometry: kingside the king lands on file
    /// `6` (g) with the rook on `5` (f); queenside the king lands on file `2` (c)
    /// with the rook on `3` (d). These hold for any board where the king starts
    /// on the e-file, so [`StandardChess`] (8x8) keeps the byte-identical
    /// behaviour the concrete engine and the existing perft suites pin.
    ///
    /// Wider boards whose king and rooks sit on different files (Capablanca: king
    /// on the f-file, rooks on the a/j files; the king castles to the i/c files)
    /// override this with the variant's own castle geometry. The king and rook
    /// destinations must lie on the board (`< WIDTH`); an off-board file
    /// suppresses that castle.
    fn castle_dest_files(side: usize) -> (u8, u8) {
        if side == 0 {
            // Kingside: king to file 6 (g), rook to file 5 (f).
            (6, 5)
        } else {
            // Queenside: king to file 2 (c), rook to file 3 (d).
            (2, 3)
        }
    }

    /// Returns the set of royal squares of `color` whose safety defines check.
    ///
    /// The default is every king of `color` (one in standard chess). Multi-king
    /// variants (Spartan) and non-royal-king variants (Duck) override this; the
    /// generic king-safety machinery treats an empty royal set as "never in
    /// check".
    fn royal_squares(board: &Board<G>, color: Color) -> Bitboard<G> {
        board.kings_of(color)
    }

    // --- Duck chess (default OFF) -----------------------------------------

    /// Returns `true` if this variant has the neutral Duck: a single blocker
    /// belonging to neither side that is added to the occupancy for movegen and
    /// is moved to a fresh empty square as the second half of every ply
    /// (`docs/fairy-variants-architecture.md` §4.4).
    ///
    /// The default is `false`. While it is `false` the generic engine skips every
    /// duck code path — the duck never enters the occupancy, no king-safety
    /// relaxation applies, no two-part move is emitted, and the FEN carries no
    /// `*` — so a non-duck variant produces byte-identical moves, state, and FEN
    /// to a build without the duck feature. Only Duck chess overrides this to
    /// `true`.
    fn has_duck() -> bool {
        false
    }

    // --- reserved fairy hooks (no-ops for standard rules) -----------------

    /// Returns the region mask for a [`WideRegion`]. Reserved for Phase 3
    /// region confinement; the default is the full board (no confinement).
    fn region_mask(_region: WideRegion) -> Bitboard<G> {
        Bitboard::FULL
    }

    /// Hook for variant-specific terminal conditions (king-capture wins, race
    /// goals). The default reports `None` — standard chess ends only by the
    /// generic checkmate / stalemate / material rules the position computes.
    fn extra_terminal(_board: &Board<G>, _state: &GenericState<G>) -> Option<WideEndReason> {
        None
    }

    /// Reserved no-op hook for drop generation (Shogi / crazyhouse). Standard
    /// chess emits no drops, so the default does nothing.
    fn emit_drops(_board: &Board<G>, _state: &GenericState<G>, _out: &mut Vec<super::WideMove>) {}

    // --- Seirawan gating (default OFF) ------------------------------------

    /// Returns `true` if this variant supports Seirawan gating: a back-rank piece
    /// making its first move (including the king and rook of a castling move) may
    /// optionally gate a reserve piece (Hawk or Elephant) onto the vacated
    /// square.
    ///
    /// The default is `false`. While it is `false` the generic engine skips every
    /// gating code path — move generation, application, and state never consult
    /// the [`GenericGating`] field — so a variant that does not gate produces
    /// byte-identical moves and state to a build without the gating feature. Only
    /// Seirawan overrides this to `true`.
    fn supports_gating() -> bool {
        false
    }

    /// The initial gating state for a fresh game: the gating-eligible squares
    /// (the original back-rank squares whose first move may gate) and each side's
    /// reserve pieces in hand.
    ///
    /// The default is [`GenericGating::NONE`] (no eligible squares, no reserves),
    /// matching `supports_gating() == false`. Seirawan overrides
    /// [`WideVariant::starting_position`] to seed a populated value.
    fn initial_gating() -> GenericGating<G> {
        GenericGating::NONE
    }
}

/// The reason a wide game ended, the generic analogue of
/// [`crate::EndReason`]. Only the standard outcomes are produced this phase;
/// the variant arm is reserved for later fairy terminal rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WideEndReason {
    /// The side to move is in check and has no legal move. Decisive for the
    /// side that delivered it.
    Checkmate,
    /// The side to move is not in check but has no legal move. Draw.
    Stalemate,
    /// Neither side has the material to deliver checkmate. Draw.
    InsufficientMaterial,
    /// A variant-specific decisive end for the side to move (reserved).
    VariantWin,
    /// A variant-specific drawn end (reserved).
    VariantDraw,
}

/// The standard-chess wide variant over an 8x8 [`Geometry`]: the reference
/// instantiation that proves the generic engine reproduces concrete perft.
///
/// It overrides only [`WideVariant::starting_position`] (the standard array);
/// every other rule is the trait default, which *is* standard chess. Instantiate
/// it as `GenericPosition<Chess8x8, StandardChess>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct StandardChess;

impl<G: Geometry> WideVariant<G> for StandardChess {
    fn starting_position() -> (Board<G>, GenericState<G>) {
        // The standard 8x8 array. This variant is only instantiated at 8x8
        // (`Chess8x8`); the FEN is the canonical start placement.
        let board = Board::<G>::from_fen_placement("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR")
            .expect("standard starting placement is valid for an 8x8 geometry");
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::standard::<G>(),
            ep_square: None,
            gating: GenericGating::NONE,
            duck: None,
            halfmove_clock: 0,
            fullmove_number: 1,
        };
        (board, state)
    }
}
