//! Hoppel-Poppel (8x8) on the generic engine — standard chess except the
//! **knight** and **bishop** swap their *capture* methods: the knight captures
//! like a bishop (but still moves like a knight), and the bishop captures like a
//! knight (but still moves like a bishop). A German chess variant; see
//! <https://www.chessvariants.com/diffmove.dir/hoppel-poppel.html>. Validated
//! square-for-square against Fairy-Stockfish `UCI_Variant hoppelpoppel`.
//!
//! ## Armies (identical, symmetric — both colours)
//!
//! Standard chess pieces, pawns, castling, en passant and a standard king, with
//! exactly two pieces redefined into **move≠capture** roles:
//!
//! * **Knight-Bishop** ([`WideRole::KnightBishop`], FSF `KNIBIS` `n`, Betza
//!   `mNcB`, mcr overflow `*h`) — **moves like a knight** (the eight 2-1 leaps) to
//!   an empty square but **captures like a bishop** (a diagonal slide). Its quiet
//!   knight jumps are non-capturing; its only attacking / checking / capturing
//!   squares are the bishop diagonals. This is the same *move-knight /
//!   capture-slider* shape as the Orda Archer, but a **distinct** role (different
//!   army, FEN token, and promotion semantics).
//! * **Bishop-Knight** ([`WideRole::BishopKnight`], FSF `BISKNI` `b`, Betza
//!   `mBcN`, mcr overflow `*b`) — the **inverse**: **moves like a bishop** (a
//!   diagonal slide) to an empty square but **captures like a knight** (a 2-1
//!   leap). Its quiet bishop slides are non-capturing; its only attacking /
//!   checking / capturing squares are the knight leaps.
//! * **King / Rook / Queen / Pawns** — all standard, with standard castling (both
//!   sides), the double pawn step, and en passant.
//!
//! Because the knight's and bishop's *moves* are unchanged (only their capture
//! method swaps), every quiet line is ordinary chess; the two armies differ from
//! standard chess only when a capture is available.
//!
//! ## Promotion
//!
//! A pawn of **either** colour reaching the last rank promotes to a **Queen**,
//! **Rook**, **Bishop-Knight**, or **Knight-Bishop** (FSF
//! `promotionPieceTypes = q r b n`, i.e. Queen / Rook / `BISKNI` / `KNIBIS`) —
//! the variant's `b` and `n` are the Hoppel-Poppel pieces, **not** the standard
//! Bishop / Knight, so a pawn never promotes to an ordinary Bishop or Knight.
//!
//! ## Confirmed starting FEN
//!
//! Hoppel-Poppel is a FSF **built-in** (not an INI variant) derived from the
//! standard chess base, so its start FEN is the **standard chess start**:
//!
//! ```text
//! FSF dialect: rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! mcr dialect: r*h*bqk*b*hr/pppppppp/8/8/8/8/PPPPPPPP/R*H*BQK*B*HR w KQkq - 0 1
//! ```
//!
//! In FSF the back rank's `n` / `b` are the redefined pieces. mcr already names
//! `n` the standard Knight and `b` the standard Bishop, so the Hoppel-Poppel
//! pieces take **overflow tokens** `*h` (Knight-Bishop) and `*b` (Bishop-Knight):
//! the standard back rank `r n b q k b n r` becomes `r *h *b q k *b *h r`, with
//! standard pawns / king / rooks / queen. The two FENs are the same position; the
//! `compare-fairy/` harness rewrites mcr's `*h → n`, `*b → b` when driving FSF.
//! Both sides have full castling rights (`KQkq`).
//!
//! ## Insufficient material — deliberately **default-off** (#350)
//!
//! Hoppel-Poppel does **not** opt into the
//! [`is_insufficient_material`](WideVariant::is_insufficient_material) hook, even
//! though Fairy-Stockfish *does* adjudicate material draws here. The reason is a
//! genuine classification divergence the standard-army
//! `standard_insufficient_material` helper cannot express:
//!
//! FSF classes both [`WideRole::KnightBishop`] (`KNIBIS`) and
//! [`WideRole::BishopKnight`] (`BISKNI`) as **unbound minors** — neither is a major
//! piece nor colour-bound — so it draws **king + one such piece vs king** exactly
//! as it draws K+N vs K. Verified against `UCI_Variant hoppelpoppel`: both
//! `K+KnightBishop vs K` and `K+BishopKnight vs K` report insufficient. The
//! standard helper, however, only knows the three standard classes (king, knight,
//! colour-bound bishop); it treats every other role — including these two — as
//! **mating material** (an "other" piece), so it would report those same positions
//! as *sufficient*. Using the helper here would therefore **contradict** FSF, and
//! reproducing FSF faithfully would need a Hoppel-Poppel-specific minor
//! classification (KnightBishop / BishopKnight as unbound minors, with the K+two
//! threshold). That is out of scope for #350's conservative, default-off
//! adjudication hook, so the variant is correctly left off: it simply reports no
//! material draw (the pre-#350 status quo), never a *wrong* one. The hook is
//! adjudication-only and never touched movegen, so perft is unaffected either way.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Chess8x8, Geometry, PromotionConfig, Square, StandardChess, WideRole,
    WideVariant,
};
use crate::Color;

/// The confirmed Hoppel-Poppel starting placement in mcr's role letters: standard
/// chess with the two knights replaced by the Knight-Bishop (`*h`) and the two
/// bishops by the Bishop-Knight (`*b`), so each back rank is
/// `r *h *b q k *b *h r` and the pawns / king / rooks / queen are standard.
const HOPPELPOPPEL_START_PLACEMENT: &str = "r*h*bqk*b*hr/pppppppp/8/8/8/8/PPPPPPPP/R*H*BQK*B*HR";

/// The Hoppel-Poppel rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides only the two redefined pieces — the Knight-Bishop (moves knight /
/// captures bishop) and the Bishop-Knight (moves bishop / captures knight) — and
/// the `q r b n` promotion target set (`b` / `n` being the variant pieces). Every
/// other piece, castling, the double pawn step, and en passant are the trait
/// defaults (standard chess).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct HoppelPoppelRules;

impl WideVariant<Chess8x8> for HoppelPoppelRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 42;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(HOPPELPOPPEL_START_PLACEMENT)
            .expect("the Hoppel-Poppel starting placement is valid on an 8x8 board");
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
            // Knight-Bishop (FSF `mNcB`): captures like a bishop — its only
            // capturing / checking squares. Its knight jumps are quiet-only (see
            // `quiet_only_targets`), so they are NOT in the attack set.
            WideRole::KnightBishop => attacks::bishop_attacks::<Chess8x8>(sq, occupancy),
            // Bishop-Knight (FSF `mBcN`): captures like a knight — its only
            // capturing / checking squares. Its bishop slides are quiet-only, so
            // they are NOT in the attack set.
            WideRole::BishopKnight => attacks::knight_attacks::<Chess8x8>(sq),
            // Everything else (king, rook, queen, pawn) is standard chess.
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn quiet_only_targets(
        role: WideRole,
        _color: Color,
        sq: Square<Chess8x8>,
        occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        // The Knight-Bishop **moves** like a knight (to empty squares) but captures
        // like a bishop (its `role_attacks`). The Bishop-Knight is the inverse: it
        // **moves** like a bishop (its slide stops at any blocker and is confined to
        // empty squares) but captures like a knight. The generic generator filters
        // these by emptiness, emitting them as quiet moves only.
        match role {
            WideRole::KnightBishop => attacks::knight_attacks::<Chess8x8>(sq),
            WideRole::BishopKnight => attacks::bishop_attacks::<Chess8x8>(sq, occupancy),
            _ => Bitboard::EMPTY,
        }
    }

    fn role_attacks_are_capture_only(role: WideRole) -> bool {
        // The Knight-Bishop (bishop slide) and Bishop-Knight (knight leap) reach
        // their `role_attacks` squares only by capturing — they move by their
        // distinct `quiet_only_targets` pattern instead.
        matches!(role, WideRole::KnightBishop | WideRole::BishopKnight)
    }

    fn role_is_slider(role: WideRole) -> bool {
        match role {
            // The Knight-Bishop *captures* along a bishop line, so it slides in the
            // attack relation and can pin / be pinned.
            WideRole::KnightBishop => true,
            // The Bishop-Knight *attacks* only by knight leaps (its bishop slides
            // are non-capturing quiet moves), so it is a leaper for attack purposes
            // and cannot pin nor act as a slider in the attack relation.
            WideRole::BishopKnight => false,
            _ => <StandardChess as WideVariant<Chess8x8>>::role_is_slider(role),
        }
    }

    // --- promotion: pawns -> Queen / Rook / Bishop-Knight / Knight-Bishop ------

    fn promotion_config() -> PromotionConfig {
        // FSF `promotionPieceTypes = q r b n`, where `b` / `n` are the variant's
        // Bishop-Knight / Knight-Bishop (never the ordinary Bishop / Knight).
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Queen,
                WideRole::Rook,
                WideRole::BishopKnight,
                WideRole::KnightBishop,
            ],
        }
    }

    // --- attacker-detection consistency ---------------------------------------

    fn role_attack_is_directional(role: WideRole) -> bool {
        // Only the pawn is colour-directional here. The Knight-Bishop's capture set
        // is a plain bishop slide and the Bishop-Knight's is the knight pattern —
        // both geometrically symmetric — so `attackers_to` reverse-projects them
        // with no colour flip and no leg asymmetry.
        matches!(role, WideRole::Pawn)
    }

    fn has_castling() -> bool {
        true
    }
}

/// Hoppel-Poppel as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position (standard chess with the Knight-Bishop and
/// Bishop-Knight in place of the knights and bishops) with
/// [`HoppelPoppel::startpos`](GenericPosition::startpos) or parse a FEN (mcr
/// dialect) with [`HoppelPoppel::from_fen`](GenericPosition::from_fen). See the
/// [module docs](self) for the move≠capture pieces and the `q r b n` promotion.
pub type HoppelPoppel = GenericPosition<
    Chess8x8,
    HoppelPoppelRules,
    { <HoppelPoppelRules as WideVariant<Chess8x8>>::ROLE_SPAN },
>;

#[cfg(test)]
mod tests {
    use super::*;

    /// The canonical start FEN round-trips.
    #[test]
    fn startpos_round_trips() {
        let pos = HoppelPoppel::startpos();
        assert_eq!(
            pos.to_fen(),
            "r*h*bqk*b*hr/pppppppp/8/8/8/8/PPPPPPPP/R*H*BQK*B*HR w KQkq - 0 1"
        );
    }
}
