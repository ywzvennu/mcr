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
use super::position::{GenericCastling, GenericGating, GenericPlacement, GenericState};
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

/// Which plain (occupancy-only, geometry-standard) slider pattern a role's
/// **king-safety reverse projection** is exactly equal to, when projecting from
/// the royal square.
///
/// The cannon king-safety verify re-tests "is the king attacked" once per sibling
/// move; for a symmetric slider role it reverse-projects the role's pattern back
/// from the (fixed) king square. When that pattern is precisely a standard rook /
/// bishop / queen ray, the projection can reuse the king's precomputed line masks
/// (`KingLineMasks`) instead of re-deriving them every move — bit-for-bit
/// identical, just without the per-move mask rebuild.
///
/// A variant opts a role in via [`WideVariant::royal_slider_kind`] **only** when
/// that role's [`role_attacks`](WideVariant::role_attacks) is exactly the plain
/// slider for every square the king could be on (no palace-diagonal addendum, no
/// region masking). The default is `None`, so every variant keeps the existing
/// reverse-projection path untouched and byte-identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoyalSlider {
    /// A plain rook (orthogonal rays).
    Rook,
    /// A plain bishop (diagonal rays).
    Bishop,
    /// A plain queen (orthogonal + diagonal rays).
    Queen,
}

/// Which **counting** endgame rule a variant uses (Makruk / Cambodian / ASEAN /
/// Burmese).
///
/// Each selects a distinct material-scaled countdown table — see
/// [`GenericGame`](super::game::GenericGame), which reproduces Fairy-Stockfish's
/// `count_limit` exactly. A variant opts in through
/// [`WideVariant::counting_rule`]; the default is `None` (no counting), so every
/// non-counting variant is byte-identical and the count is never tracked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WideCountingRule {
    /// Thai Makruk: board-honour (64 full moves) while the counted side still has
    /// material, then pieces-honour (8 / 16 / 22 / 32 / 44 moves, scaled by the
    /// superior side's rooks / khons / knights) once it is a lone king.
    Makruk,
    /// Cambodian Ouk Chaktrang: like Makruk but the board-honour count is 63 and
    /// applies only while the counted side has at most three pieces; the
    /// pieces-honour tiers are 7 / 15 / 21 / 31 / 43.
    Cambodian,
    /// ASEAN (modernised Makruk): pieces-honour only — counting begins once the
    /// counted side is a lone king and no pawns remain, with a 16 / 44 / 64-move
    /// limit by the superior side's strongest piece (rook / khon / knight).
    Asean,
    /// Burmese (Sittuyin): pieces-honour only, with the same 16 / 44 / 64-move
    /// tiers as ASEAN (rook / sin / knight — the general/Met alone cannot mate and
    /// draws at once), but with Sittuyin's distinctive **centre-square exception**:
    /// a lone king standing on one of the four central squares (d4 / d5 / e4 / e5)
    /// when the count starts is granted five extra moves — the count begins only
    /// after the king's fifth move — so the limits become 21 / 49 / 69. The
    /// published Burmese counting; note Fairy-Stockfish itself models Sittuyin as
    /// plain [`Asean`](WideCountingRule::Asean) and omits the centre exception.
    Burmese,
}

/// The **impasse / jishogi (entering-king)** declaration rule a shogi-family
/// variant uses — the "27-point rule" of modern Shogi.
///
/// When a king can no longer realistically be mated it marches into the far
/// promotion zone; the game is then decided by a **piece-point declaration**
/// rather than by checkmate. This type carries the parameters of that count so
/// the terminal test in
/// [`GenericPosition::end_reason`](super::position::GenericPosition::end_reason)
/// is a pure position property (it needs no move history — only the board, the
/// hands, and the promotion-zone geometry).
///
/// A variant opts in through [`WideVariant::impasse_rule`]; the default is `None`
/// (no impasse), so every non-shogi variant is byte-identical and the rule is
/// never evaluated. It is a **terminal-only** adjudication and is never consulted
/// by move generation, so perft stays byte-identical.
///
/// ## The declaration (lishogi 27-point rule)
///
/// At the **start of the side-to-move's turn** that side wins outright if all of:
///
/// 1. its king is **not in check**;
/// 2. its king stands **inside its own promotion zone** (the three farthest ranks);
/// 3. it has at least [`min_pieces_in_zone`](Self::min_pieces_in_zone) of its
///    **other** pieces (the king excluded) inside that zone; and
/// 4. its **point count** reaches the per-side threshold —
///    [`sente_threshold`](Self::sente_threshold) for the first player (mcr's
///    [`Color::White`], drawn "Black"/☗ in Japanese usage) or
///    [`gote_threshold`](Self::gote_threshold) for the second player.
///
/// The point count sums, over every one of the side's pieces that is either
/// **inside its promotion zone** or **in hand** (the king counts for neither):
/// [`big_piece_points`](Self::big_piece_points) for each Rook / Bishop and their
/// promotions ([`big_roles`](Self::big_roles)), and
/// [`small_piece_points`](Self::small_piece_points) for every other piece.
///
/// The rule is **win-only**: a side that cannot meet the threshold simply does not
/// declare (there is no "declare and lose" branch), so a met declaration is
/// reported as a decisive [`WideEndReason::Impasse`] for the side to move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImpasseRule {
    /// The minimum number of the declaring side's **own** pieces (the king
    /// excluded) that must stand inside its promotion zone. Shogi requires 10.
    pub min_pieces_in_zone: u32,
    /// The point threshold the **first player** must reach — mcr's
    /// [`Color::White`], the uppercase side that moves first (Sente / ☗ in
    /// Japanese usage). Shogi: 28 (the first-move advantage costs one extra point).
    pub sente_threshold: u32,
    /// The point threshold the **second player** must reach — mcr's
    /// [`Color::Black`] (Gote / ☖). Shogi: 27.
    pub gote_threshold: u32,
    /// The point value of a "big" piece — a Rook or Bishop (promoted or not);
    /// see [`big_roles`](Self::big_roles). Shogi: 5.
    pub big_piece_points: u32,
    /// The point value of every other counted (non-king) piece. Shogi: 1.
    pub small_piece_points: u32,
    /// The roles scored at [`big_piece_points`](Self::big_piece_points): the Rook,
    /// the Bishop, and their promoted forms (Dragon King, Dragon Horse).
    pub big_roles: &'static [WideRole],
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

    /// Returns `true` if this variant fields **Chu-Shogi Lion** pieces — pieces
    /// whose move includes the two-step area move, the *igui* stationary capture,
    /// the double capture, and the *jitto* pass. Default `false`; only Chu Shogi
    /// overrides it. When `true`, the multi-royal generator runs an extra
    /// `gen_lion_moves` pass that emits these
    /// [`WideMoveKind::LionMove`](super::WideMoveKind::LionMove) moves for the
    /// pieces [`role_is_full_lion`](WideVariant::role_is_full_lion) /
    /// [`role_lion_lines`](WideVariant::role_lion_lines) identify. Every other
    /// variant leaves it `false`, so the pass is never run and their move
    /// generation is byte-identical.
    fn has_lion_moves() -> bool {
        false
    }

    /// Returns `true` if `role` is a **full Lion** — a piece with lion power in all
    /// eight directions (Chu Shogi's Lion): its two King-steps may turn, so it
    /// reaches, captures on, and igui-captures across every adjacent and
    /// distance-two square. Default `false`. Consulted only under
    /// [`has_lion_moves`](WideVariant::has_lion_moves).
    fn role_is_full_lion(_role: WideRole) -> bool {
        false
    }

    /// Returns the White-orientation **lion-power line directions** of `role` — the
    /// straight lines along which a partial lion-power piece (Chu Shogi's Horned
    /// Falcon: forward; Soaring Eagle: the two forward diagonals) may make its
    /// two-step / igui / pass Lion move, without turning. Empty for every other
    /// role. Consulted only under [`has_lion_moves`](WideVariant::has_lion_moves);
    /// a role is either a full lion or a line-lion, never both.
    fn role_lion_lines(_role: WideRole) -> &'static [(i8, i8)] {
        &[]
    }

    /// Returns `true` if this variant fields a **Fire Demon** — the Tenjiku-Shogi
    /// piece that, after its Flying-Ox move, **burns** (captures) every enemy on
    /// the up-to-eight squares adjacent to its destination, and may **igui** (burn
    /// in place). Default `false`; only Tenjiku overrides it. When `true`, the
    /// multi-royal generator emits the Fire Demon's slides (and its igui) as
    /// [`WideMoveKind::FireDemonMove`](super::WideMoveKind::FireDemonMove) moves
    /// whose burn victims are recomputed at apply-time. Every other variant leaves
    /// it `false`, so no such move is ever produced and their move generation is
    /// byte-identical.
    fn has_area_burn() -> bool {
        false
    }

    /// Returns `true` if `role` is an **area burner** — a Fire Demon (Tenjiku's
    /// [`WideRole::FireDemon`]): a piece whose move is a
    /// Flying-Ox slide that additionally burns every adjacent enemy, and which may
    /// igui. Default `false`. Consulted only under
    /// [`has_area_burn`](WideVariant::has_area_burn).
    fn role_is_area_burner(_role: WideRole) -> bool {
        false
    }

    /// Returns `true` if this variant fields **range-jumping Generals** — Tenjiku
    /// Shogi's Great / Vice / Rook / Bishop General, which slide as their base piece
    /// (Free King / Bishop / Rook / Bishop) but, **when capturing**, may jump over
    /// any number of consecutive *lower-ranked* pieces (friend or foe) in a straight
    /// line to capture an enemy beyond, stopped only by an equal-or-higher-ranked
    /// piece. Default `false`; only Tenjiku overrides it. When `true`, the
    /// multi-royal generator runs an extra `gen_jump_general_moves` pass that emits
    /// these jump-captures as ordinary single-victim
    /// [`WideMoveKind::Capture`](super::WideMoveKind::Capture) moves (a jump-capture
    /// removes only its landing square), and masks out captures forbidden by
    /// [`role_is_capture_immune`](WideVariant::role_is_capture_immune). Every other
    /// variant leaves it `false`, so no such move is produced and its move generation
    /// is byte-identical.
    fn has_jump_captures() -> bool {
        false
    }

    /// The piece-value **rank** of `role` in the Tenjiku range-jump hierarchy. A
    /// range-jumping General may jump over pieces of **strictly lower** rank only,
    /// and is stopped by any piece of equal-or-higher rank. The hierarchy is: King /
    /// Crown Prince = `4` (never jumped), Great General = `3`, Vice General = `2`,
    /// Rook General / Bishop General = `1`, every other piece = `0`. Default `0`;
    /// consulted only under [`has_jump_captures`](WideVariant::has_jump_captures).
    fn role_jump_rank(_role: WideRole) -> u8 {
        0
    }

    /// Returns `true` if `role` is a **range-jumping General** (Tenjiku's Great /
    /// Vice / Rook / Bishop General). Default `false`. Consulted only under
    /// [`has_jump_captures`](WideVariant::has_jump_captures).
    fn role_is_jump_capturer(_role: WideRole) -> bool {
        false
    }

    /// The straight-line directions along which `role`'s range-jump capture may
    /// travel — the same lines as its ordinary slide (the Rook General's four
    /// orthogonals, the Vice / Bishop General's four diagonals, the Great General's
    /// eight). Every Tenjiku General is left-right *and* up-down symmetric, so these
    /// are colour-independent. Empty for every other role. Consulted only under
    /// [`has_jump_captures`](WideVariant::has_jump_captures).
    fn role_jump_dirs(_role: WideRole) -> &'static [(i8, i8)] {
        &[]
    }

    /// Returns `true` if `role` is **capture-immune** — capturable only by another
    /// piece of the *same* role. Tenjiku's Great General is immune to every capture
    /// except by an enemy Great General. Default `false`; consulted only under
    /// [`has_jump_captures`](WideVariant::has_jump_captures), where the multi-royal
    /// generator removes an immune enemy's square from the capture targets of every
    /// mover of a different role (its ordinary slides / leaps and the range-jump
    /// captures alike).
    fn role_is_capture_immune(_role: WideRole) -> bool {
        false
    }

    /// Returns `true` if this variant promotes by the **Chu-Shogi rule**: a piece
    /// may promote only when it *enters* the promotion zone from outside, or makes
    /// a *capture* on a move that begins inside the zone — never on a non-capturing
    /// move that stays within or leaves the zone. Default `false` (the standard
    /// "starts-or-ends in the zone" rule). Only Chu Shogi overrides it, so every
    /// other promotion variant is byte-identical.
    fn lion_style_promotion() -> bool {
        false
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

    /// Returns the 0-based rank on which `color`'s king and castling rooks
    /// start — the rank a castle moves along.
    ///
    /// The default is the back rank (rank `0` for white, the top rank for black),
    /// where standard chess and every existing variant keep their king and rooks,
    /// so this hook is inert and those variants are byte-identical. Shako
    /// overrides it: its king and rooks sit on **rank 2** (the cannons occupy the
    /// back rank), so its castle, castling-rights bookkeeping, and the `KQkq` FEN
    /// rook-file scan all run on rank 2 (white) / rank 9 (black). The generic
    /// castling code consults this everywhere it previously assumed the back rank.
    fn castle_rank(color: Color) -> u8 {
        match color {
            Color::White => 0,
            Color::Black => G::HEIGHT - 1,
        }
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

    /// Returns `true` if the castling FEN field is written in **Shredder** form —
    /// explicit rook-**file** letters, uppercase for White and lowercase for Black
    /// (e.g. `JAja`) — rather than the standard `KQkq`.
    ///
    /// A Chess960-style shuffled variant (Caparandom) whose king and rooks start on
    /// arbitrary files sets this so its castling rights round-trip unambiguously and
    /// its FEN matches Fairy-Stockfish's own file-letter output byte-for-byte. The
    /// reader accepts both forms regardless (`KQkq` is read as the outermost rook on
    /// each side of the king); only the *writer* is switched here.
    ///
    /// The default is `false`, so every standard-castling variant keeps the `KQkq`
    /// field it always emitted and stays byte-identical.
    fn shredder_castling_fen() -> bool {
        false
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

    /// Returns `true` if this variant's king role is **non-royal** — there is no
    /// check, and a side instead **loses by extinction** (its king captured). On a
    /// **hand** variant (Dobutsu) this routes move generation through the per-move
    /// verify path, whose non-royal branch emits every pseudo-legal board move and
    /// drop **unverified** (no self-check filter), exactly as Fairy-Stockfish's
    /// extinction rule: the king may step into capture, and capturing the enemy
    /// king is legal.
    ///
    /// The default is `false`, so every other variant keeps its existing path
    /// (Duck rides its own generator off [`royal_squares`](WideVariant::royal_squares) alone, and the
    /// single-king / multi-royal / cannon paths are unchanged). A variant that sets
    /// this `true` should also return an empty [`royal_squares`](WideVariant::royal_squares) set. Only the
    /// hand-path routing consults this hook, so non-hand non-royal variants (Duck)
    /// stay byte-identical.
    fn non_royal_king() -> bool {
        false
    }

    // --- Spartan multi-king / duple-check (default OFF) -------------------

    /// Returns `true` if this variant can give a side **more than one royal
    /// king** at once, so "in check" generalises to a *set* of royal squares and
    /// the single-king legality fast path no longer applies
    /// (`docs/fairy-variants-architecture.md` §4.4). Spartan is the only such
    /// variant (Black starts with two kings).
    ///
    /// The default is `false`. While it is `false` the generic engine takes the
    /// single-king legality path — one king square, one check mask, one pin set —
    /// exactly as before, so every other variant produces byte-identical moves and
    /// state. When `true`, the engine instead generates pseudo-legal moves and
    /// keeps each one whose result leaves **at least one** of the side's kings
    /// unattacked: a side with several kings is "in check" only when **every** king
    /// is attacked (duple check, for two kings), and may otherwise leave a king en
    /// prise — losing it and continuing with the survivor. This matches
    /// Fairy-Stockfish's `spartan` king rule move-for-move.
    fn multi_royal() -> bool {
        false
    }

    /// Returns `true` if **every** royal piece must be left safe by a move — the
    /// strict pseudo-royal rule where a side may not leave *any* king (or extra
    /// royal piece) en prise (Chak: royals are the King and the promoted Divine
    /// Lord, FSF `extinctionPseudoRoyal`). Only consulted on the multi-royal path
    /// (with [`multi_royal`](WideVariant::multi_royal) `true`).
    ///
    /// The default is `false`: Spartan's duple-check rule instead keeps a side
    /// legal while **at least one** royal survives (a side may sacrifice a king and
    /// play on with the survivor). While it is `false` the multi-royal path is
    /// byte-identical to Spartan. When `true`, "in check" means *any* royal is
    /// attacked and a legal move must leave *every* royal unattacked — exactly a
    /// generalisation of single-king check to a set of royals that must *all*
    /// survive. For Chak the side always has exactly one royal in a reachable
    /// position, so this only differs from the default on artificial multi-royal
    /// positions, but it matches Fairy-Stockfish's pseudo-royal rule on those too.
    fn royals_all_must_survive() -> bool {
        false
    }

    /// Returns `true` if `color`'s royal pieces (those [`royal_squares`] reports)
    /// currently impose a king-safety constraint — a move may not leave them
    /// unsafe and the side can be checked / mated. Only consulted on the
    /// multi-royal path ([`multi_royal`](WideVariant::multi_royal) `true`).
    ///
    /// The default is `true`: every royal is always royal (Spartan, Chak), so the
    /// multi-royal path is byte-identical. Sho Shogi overrides it for its
    /// **count-thresholded pseudo-royalty** (FSF `extinctionPseudoRoyal` with
    /// `extinctionPieceCount = 0`): a King and a Crown Prince are royal **only
    /// while a side holds at most one of them**. While a side has **both**, neither
    /// is royal — it may leave either (or both) en prise and play on, and is never
    /// in check — so the constraint is **inactive** and every pseudo-legal move is
    /// legal. When it returns `false` the multi-royal generator emits the side's
    /// pseudo-legal moves unverified, and the per-move survival predicates report
    /// the side as safe. With the constraint active there is exactly one royal, so
    /// it reduces to ordinary single-king check.
    ///
    /// [`royal_squares`]: WideVariant::royal_squares
    fn royal_constraint_active(_board: &Board<G>, _color: Color) -> bool {
        true
    }

    /// Returns `true` if a side that has lost **all** its royal pieces still
    /// generates its pseudo-legal moves (rather than being treated as an
    /// already-terminal, move-less node). Only consulted on the multi-royal path
    /// ([`multi_royal`](WideVariant::multi_royal) `true`).
    ///
    /// The default is `false`: a side whose [`royal_squares`](WideVariant::royal_squares)
    /// set is empty has been eliminated (its last king captured) and has no legal
    /// continuation, exactly as Spartan / Chak truncate the node. Xiang Fu sets it
    /// `true` because its royalty is **pseudo-royal** (the Champions are not the
    /// `KING` piece type): Fairy-Stockfish never truncates the move list of a side
    /// that has lost both Champions — with no pseudo-royal pieces left there is no
    /// king-safety constraint, so every pseudo-legal move is legal — and perft must
    /// match that node-for-node. With the constraint inactive (an empty royal set),
    /// every pseudo-legal move (and drop) is emitted unverified.
    fn royalless_generates() -> bool {
        false
    }

    /// Returns the **forward step** a Berolina-style pawn (the Spartan Hoplite)
    /// uses for its *non-capturing* move: a diagonal advance. Returns the two
    /// diagonal-forward landing squares from `from` for `color`, or
    /// [`Bitboard::EMPTY`] for a variant whose pawn pushes straight.
    ///
    /// The default is `EMPTY` — the standard pawn pushes straight (handled by the
    /// generic pawn generator), so this hook is inert and every non-Berolina
    /// variant is byte-identical. Spartan overrides it so the Hoplite's quiet
    /// move is the diagonal one (and a two-square diagonal jump from the start
    /// rank), while its capture stays the straight-forward square.
    fn berolina_push_targets(_color: Color, _from: Square<G>) -> Bitboard<G> {
        Bitboard::EMPTY
    }

    /// Returns `true` if the side-to-move pawns move as **Berolina** pawns
    /// (diagonal advance, straight capture) — the Spartan Hoplite. The default is
    /// `false` (standard straight-push / diagonal-capture pawns), keeping every
    /// other variant on the standard pawn path.
    fn has_berolina_pawns() -> bool {
        false
    }

    /// Returns the squares a piece of `role` of `color` on `sq` may move to but
    /// **never capture on** — non-capturing "quiet-only" steps that the role's
    /// [`role_attacks`](WideVariant::role_attacks) set deliberately omits (so they
    /// do not threaten the enemy king or count as attacks).
    ///
    /// The default is [`Bitboard::EMPTY`] — every standard and existing-variant
    /// move can also capture, so this hook is inert and consulted only on the
    /// multi-king generation path (itself default-off). Spartan uses it for the
    /// Lieutenant's sideways step, which slides onto an empty square but cannot
    /// capture. The generic generator emits each returned square as a quiet move
    /// only if it is empty.
    fn quiet_only_targets(
        _role: WideRole,
        _color: Color,
        _sq: Square<G>,
        _occupancy: Bitboard<G>,
    ) -> Bitboard<G> {
        Bitboard::EMPTY
    }

    /// Returns `true` if a piece of `role`'s [`role_attacks`](WideVariant::role_attacks)
    /// set is **capture-only** — its squares may be reached **only** by capturing
    /// an enemy piece there, never as a quiet move to an empty square. The role's
    /// quiet moves then come **solely** from
    /// [`quiet_only_targets`](WideVariant::quiet_only_targets).
    ///
    /// This is the dual of [`quiet_only_targets`](WideVariant::quiet_only_targets)
    /// (which adds move-only squares to a role whose `role_attacks` set is normally
    /// move-and-capture). The canonical case is the Orda Lancer (captures like a
    /// rook) and Archer (captures like a bishop): each **moves** like a knight (its
    /// `quiet_only_targets`) but **captures** along a slider line (its
    /// `role_attacks`), so the slider squares must never be emitted as quiet moves.
    ///
    /// The default is `false` for every role; while it is `false` the generator
    /// emits each `role_attacks` square as a quiet move (empty) or a capture
    /// (enemy) exactly as before, so every other variant is byte-identical. This
    /// affects **only** the generator's quiet/capture split; the role's attack
    /// relation (check, king-danger, `attackers_to`) still uses the full
    /// `role_attacks` set — a Lancer genuinely threatens to capture along its rook
    /// lines.
    fn role_attacks_are_capture_only(_role: WideRole) -> bool {
        false
    }

    // --- Cannon king-safety (default OFF) ---------------------------------

    /// Returns `true` if this variant fields **cannons** (the Xiangqi-style
    /// piece that captures by jumping a single screen) — pieces whose attack
    /// relationship to the king depends on a *screen* and therefore breaks the
    /// standard mask-based king-safety fast path.
    ///
    /// The default is `false`. While it is `false` the generic engine takes the
    /// usual single-king path — one precomputed king-danger map, one check mask,
    /// one pin set — exactly as before, so every non-cannon variant produces
    /// byte-identical moves and state. When `true`, the engine instead generates
    /// pseudo-legal moves and keeps each one whose resulting position leaves the
    /// king unattacked, computing attacks (including the cannon's over-screen
    /// captures) on the **actual post-move occupancy**. This is required because a
    /// cannon's check and king-danger are screen-dependent: a king sliding along a
    /// cannon's ray, or interposing/removing a screen, changes the attack in a way
    /// the lifted-king danger map and the `between` interpose mask cannot capture.
    /// Shako is the only such variant so far; future Xiangqi/Janggi reuse it.
    fn has_cannons() -> bool {
        false
    }

    // --- Board-aware cannon attacks (default OFF) -------------------------

    /// Returns `true` if this variant computes some role's attack / quiet-move
    /// sets from the **whole board** rather than from the `(sq, occupancy)` pair
    /// the [`role_attacks`](WideVariant::role_attacks) /
    /// [`quiet_only_targets`](WideVariant::quiet_only_targets) hooks receive —
    /// because the set depends on *which* occupied squares hold *which* pieces.
    ///
    /// The canonical case is the **Janggi cannon** (포): it must jump exactly one
    /// **screen** and may neither use a cannon as a screen nor capture a cannon, so
    /// its move/attack set needs to know which squares hold cannons (and, for the
    /// palace-diagonal jump, the palace geometry). The occupancy-only primitive
    /// cannot express that.
    ///
    /// The default is `false`; while it is `false` the generic engine never calls
    /// [`role_attacks_board`](WideVariant::role_attacks_board) or
    /// [`quiet_targets_board`](WideVariant::quiet_targets_board), so every other
    /// variant pays nothing and is byte-identical. Only Janggi overrides this. The
    /// board hooks are consulted only on the cannon-verify generation path and the
    /// attacker / king-safety path (both already gated by
    /// [`has_cannons`](WideVariant::has_cannons)).
    fn uses_board_attacks() -> bool {
        false
    }

    /// A **board-aware** override of [`role_attacks`](WideVariant::role_attacks)
    /// for a `role` of `color` on `sq`, returning `None` to fall back to the
    /// occupancy-only hook.
    ///
    /// Only consulted when [`uses_board_attacks`](WideVariant::uses_board_attacks)
    /// is `true`. The default is `None` (no override) for every role, so every
    /// other variant is byte-identical. Janggi overrides it for the Cannon to
    /// compute the screen-mandatory, no-cannon-screen, no-cannon-capture set
    /// (including the palace-diagonal jump) from the live board. The returned set
    /// is the cannon's combined move-and-attack set: its over-screen capture
    /// targets **and** the empty squares it may quietly jump to past a screen.
    /// Fed to the generator it splits into quiet/capture by enemy occupancy; fed to
    /// the king-safety test it correctly reports a cannon "check" (a royal square is
    /// occupied, so it can only fall in the capture portion).
    fn role_attacks_board(
        _role: WideRole,
        _color: Color,
        _sq: Square<G>,
        _board: &Board<G>,
    ) -> Option<Bitboard<G>> {
        None
    }

    /// A **board-aware** override of a role's pure **threat** set — the squares it
    /// attacks / checks / could capture on — as distinct from its full *move* set
    /// ([`role_attacks_board`](WideVariant::role_attacks_board)). Returning `None`
    /// falls back to the occupancy-only [`role_attacks`](WideVariant::role_attacks).
    ///
    /// Only consulted when [`uses_board_attacks`](WideVariant::uses_board_attacks)
    /// is `true`, and only on the threat-detection paths
    /// ([`attackers_to`](crate::geometry::position::GenericPosition::attackers_to)
    /// and the king-safety verify), never during move generation.
    ///
    /// The default returns [`role_attacks_board`](WideVariant::role_attacks_board),
    /// so Janggi — whose cannon's move set and threat set coincide — is
    /// byte-identical. A variant whose **move set differs from its threat set** must
    /// override this. Empire's "move like a Queen, capture short" pieces are the
    /// case: their move set folds in quiet Queen slides onto empty squares, which are
    /// *not* threats (a piece reachable only by the quiet Queen move is not under
    /// attack), so projecting the full move set from an empty square (a castling
    /// transit / destination) would invent a phantom attacker. Empire overrides this
    /// to return just the short capture pattern — the squares it genuinely threatens.
    fn role_threats_board(
        role: WideRole,
        color: Color,
        sq: Square<G>,
        board: &Board<G>,
    ) -> Option<Bitboard<G>> {
        Self::role_attacks_board(role, color, sq, board)
    }

    /// A **board-aware** override of the
    /// [`quiet_only_targets`](WideVariant::quiet_only_targets) set for a `role` of
    /// `color` on `sq`, returning `None` to fall back to the occupancy-only hook.
    ///
    /// Only consulted when [`uses_board_attacks`](WideVariant::uses_board_attacks)
    /// is `true`. The default is `None`. Janggi folds the cannon's quiet jumps into
    /// [`role_attacks_board`](WideVariant::role_attacks_board) (the generator's
    /// `emit_targets` splits quiet from capture by occupancy), so it leaves this at
    /// the default and the cannon emits no separate quiet-only set.
    fn quiet_targets_board(
        _role: WideRole,
        _color: Color,
        _sq: Square<G>,
        _board: &Board<G>,
    ) -> Option<Bitboard<G>> {
        None
    }

    // --- Pass move (default OFF) ------------------------------------------

    /// Returns `true` if this variant lets a side **pass** the turn — a legal
    /// null move that changes only the side to move (Janggi). The default is
    /// `false`; while it is `false` the generator never emits a pass, so every
    /// other variant is byte-identical.
    ///
    /// Janggi overrides it to `true`. Fairy-Stockfish counts the pass as a move in
    /// `go perft` and encodes it as a king "stays put" move (`from == to == the
    /// general's square`); it is **not** available while the side to move is in
    /// check. The generic cannon-verify path emits exactly one such pass per node
    /// (when a royal piece exists and the side is not in check).
    fn allows_pass() -> bool {
        false
    }

    // --- Flying-general king-safety (default OFF) -------------------------

    /// Returns `true` if this variant has an **extra, geometry-derived attack on
    /// a royal square** that the per-role [`role_attacks`](WideVariant::role_attacks)
    /// vocabulary does not express — namely the Xiangqi **flying general**: the
    /// two generals may not face each other on an otherwise-empty file, and a
    /// general gives "check" down such an open file.
    ///
    /// The default is `false`; while it is `false` the generic king-safety code
    /// never calls [`extra_royal_attack`](WideVariant::extra_royal_attack), so
    /// every other variant is byte-identical. When `true`, the engine ORs that
    /// hook into every test of "is this royal square attacked," so a move leaving
    /// the generals facing is rejected (illegal), and a general down an open file
    /// counts as a checker. Only Xiangqi (and future Janggi) override this.
    fn has_flying_general() -> bool {
        false
    }

    /// Returns `true` if, under `occupied`, the royal square `sq` of the side
    /// **not** equal to `by` is subject to an extra geometry-derived attack from
    /// color `by` — the Xiangqi flying-general confrontation: `by`'s general faces
    /// `sq` down a file with no piece between them.
    ///
    /// Only consulted when [`has_flying_general`](WideVariant::has_flying_general)
    /// is `true`; the default is `false` (no extra attack), so every other variant
    /// is unaffected. The engine ORs this into the attacked-square test on the
    /// king-safety verify path and in `is_check`, exactly modelling the rule that
    /// the generals may never see each other down an open file.
    fn extra_royal_attack(
        _board: &Board<G>,
        _sq: Square<G>,
        _by: Color,
        _occupied: Bitboard<G>,
    ) -> bool {
        false
    }

    // --- Flag-rank "campmate" / flag win (default OFF) -------------------

    /// Returns `true` if this variant is won when a king reaches the **opponent's
    /// far rank** — the Synochess "campmate" and the Orda "flag". The default is
    /// `false`; while it is `false` the engine never evaluates the flag rule, so
    /// every other variant is byte-identical.
    ///
    /// When `true`, a king of color `c` that stands on
    /// [`flag_rank(c)`](WideVariant::flag_rank) has won: on the **opponent's** turn
    /// the move generator short-circuits to *zero moves* (the node is a perft leaf,
    /// exactly as Fairy-Stockfish truncates it — the winner, on its **own** turn,
    /// still has moves), and the win is reported as a
    /// [`WideEndReason::VariantWin`](WideEndReason). This single hook serves both
    /// flag variants regardless of which generator they ride: Orda is on the
    /// standard single-king fast path and Synochess (with cannons + flying general)
    /// on the per-move verify path, and each consults this gate on its own path.
    ///
    /// Synochess additionally forbids a king from **moving onto** its own flag rank
    /// while the enemy king already occupies it *and faces it down that rank* (the
    /// flag is contested) — a piece strictly between the two kings breaks the faceoff
    /// and makes the step legal, exactly as the flying general does
    /// ([`flag_contest_defers_to_facing`](WideVariant::flag_contest_defers_to_facing)).
    /// That contested-flag restriction is enforced only on the verify path Synochess
    /// already takes for its cannons / flying general, so it never affects Orda.
    fn has_flag_win() -> bool {
        false
    }

    /// Returns `true` if the contested-flag-rank restriction is governed by the
    /// **flying-general faceoff** rather than a coarse whole-rank ban.
    ///
    /// When the enemy king already stands on this side's flag rank, a king may not
    /// step onto that rank into a position **facing** the enemy king down the rank.
    /// That faceoff — like the [`extra_royal_attack`](WideVariant::extra_royal_attack)
    /// flying general — is broken by any piece strictly **between** the two kings, so
    /// the step is legal when a blocker interposes. The default `false` forbids the
    /// whole contested rank outright (the shared behaviour every flag variant without
    /// a flying general relies on). A flying-general flag variant (Synochess) returns
    /// `true`, deferring the contest to its per-move flying-general verify, which
    /// respects the blocker instead of banning the entire rank. Only consulted while
    /// [`has_flag_win`](WideVariant::has_flag_win) is `true`.
    fn flag_contest_defers_to_facing() -> bool {
        false
    }

    /// The rank a king of `color` wins by reaching, when
    /// [`has_flag_win`](WideVariant::has_flag_win) is `true`. The default is the
    /// opponent's back rank — rank `HEIGHT-1` for White, rank `0` for Black — which
    /// is both the Synochess "campmate" and the Orda "flag" goal, so neither
    /// variant overrides it. Only consulted while `has_flag_win()`.
    fn flag_rank(color: Color) -> u8 {
        if color.is_white() {
            G::HEIGHT - 1
        } else {
            0
        }
    }

    /// Returns `true` if the flag win additionally requires the king on the goal
    /// rank to be **safe** — unattacked by the opponent (Dobutsu's "try" rule: the
    /// Lion wins by reaching the far rank only when it cannot be captured there).
    /// The default is `false`, so the flag win is purely positional (Orda /
    /// Synochess: a king on its goal rank wins even while attacked), keeping every
    /// other flag variant byte-identical. Only consulted while
    /// [`has_flag_win`](WideVariant::has_flag_win) is `true`. When `true`, a king on
    /// its goal rank that the opponent attacks is **not** yet a win — the game
    /// continues, exactly as Fairy-Stockfish's `flagPieceSafe` rule.
    fn flag_win_requires_safe() -> bool {
        false
    }

    // --- Bare-king "Robado" draw (default OFF) ---------------------------

    /// Returns `true` if this variant draws the instant **either side is reduced
    /// to a lone king** — the Shatar "Robado" rule (Mongolian chess: a side
    /// stripped of every piece but its king is "robbed", and the game is an
    /// immediate draw). The default is `false`; while it is `false` the engine
    /// never evaluates the rule, so every other variant is byte-identical.
    ///
    /// When `true`, a position in which some side's only remaining piece is its
    /// king is **terminal**: on **either** side's turn the move generator
    /// short-circuits to *zero moves* (the node is a perft leaf, exactly as
    /// Fairy-Stockfish truncates it — FSF's `extinctionValue = VALUE_DRAW` with
    /// `extinctionPieceCount = 1` over all piece types reports the game over
    /// before generating any move), and the draw is reported as a
    /// [`WideEndReason::VariantDraw`](WideEndReason). The single
    /// [`bare_king_present`](super::position::GenericPosition::bare_king_present)
    /// chokepoint both the standard generator and the bulk-count leaf path funnel
    /// through truncates the perft descent the same way FSF does. Only Shatar
    /// overrides this so far.
    fn has_bare_king_draw() -> bool {
        false
    }

    /// Returns `true` if this variant **decides the game by baring the king** — a
    /// side stripped of every piece but its king has **lost**, the Shatranj baring
    /// rule (FSF `extinctionValue = -VALUE_MATE`, `extinctionPieceCount = 1`,
    /// `extinctionOpponentPieceCount = 2`, `extinctionClaim = true`). The default
    /// is `false`; while it is `false` the engine never evaluates the rule, so
    /// every other variant is byte-identical.
    ///
    /// This is the **loss** counterpart of the Shatar [`has_bare_king_draw`] rule,
    /// and — unlike that draw — it is **not** unconditional: it mirrors FSF's
    /// `extinctionClaim` exactly, granting the bared side a single "bare-back"
    /// reply. A node is terminal (a perft leaf, the bared side having lost) when
    /// one side holds exactly its lone king and the opponent holds **three or more**
    /// pieces (no single capture could bare it back), **or** when it is the
    /// opponent's turn (the bared side's bare-back chance already spent). While the
    /// bared side is to move and its opponent has only two pieces (a king it might
    /// capture next move into a King-vs-King draw), the node is **not** terminal.
    /// The single
    /// [`bare_king_loss_loser`](super::position::GenericPosition::bare_king_loss_loser)
    /// chokepoint both the standard generator and the bulk-count leaf path funnel
    /// through truncates the perft descent exactly as Fairy-Stockfish does. Only
    /// Shatranj overrides this so far.
    ///
    /// [`has_bare_king_draw`]: WideVariant::has_bare_king_draw
    fn has_bare_king_loss() -> bool {
        false
    }

    /// Returns `true` if **stalemate is a loss** for the stalemated side rather
    /// than a draw (Synochess `stalemateValue = loss`). The default is `false`
    /// (the standard draw). This affects only the reported [outcome]; it has no
    /// effect on move generation or perft, since a stalemated node already
    /// generates zero moves regardless.
    ///
    /// [outcome]: super::position::GenericPosition::outcome
    fn stalemate_is_loss() -> bool {
        false
    }

    // --- Pin confinement for leapers (default OFF) ------------------------

    /// Returns `true` if a **pinned piece must be confined to the segment**
    /// between its king and the pinning slider (inclusive of the pinner's
    /// square), rather than to the full king–slider line. The default is
    /// `false`, which keeps the original full-line pin mask so **every existing
    /// variant is byte-identical** by construction.
    ///
    /// The two masks differ only for a **leaper** that can jump *past* the
    /// pinning slider (or *past* its own king): on the full line such a leaper
    /// would be wrongly permitted to land on a collinear square beyond the pinner
    /// or the king, off the shielding segment. Variants with such leapers override
    /// this to `true` — Courier (whose Alfil leaps two squares diagonally), the
    /// Opulent and Ten-Cubed compounds, and Tori Shogi (whose Pheasant/Goose leap
    /// straight past a pinner, issue #416). For sliders the two masks are always
    /// equivalent (the king blocks the far side and the pinner blocks beyond), so
    /// enabling it never changes a slider's legal moves.
    fn confine_pins_to_segment() -> bool {
        false
    }

    // --- Janggi bikjang general-facing (default OFF) ----------------------

    /// Returns `true` if this variant restricts the **general's own move** when the
    /// two generals face each other on an open line (Janggi bikjang). The default
    /// is `false`; while it is `false` the engine never evaluates the facing rule,
    /// so every other variant is byte-identical.
    ///
    /// Janggi's facing rule is **narrower** than Xiangqi's flying general: facing
    /// the enemy general on an open file or rank does **not** make the position an
    /// ordinary check, and a side may freely **expose** its own general by moving a
    /// blocking piece off the line (Fairy-Stockfish allows this; Xiangqi forbids
    /// it). The single restriction is that the **general itself** may not move from
    /// a facing square to another square that **still** faces the enemy general
    /// (i.e. it may not slide along the contested line staying faced) — it must
    /// leave the line or **pass**. So this is *not* modelled through
    /// [`extra_royal_attack`](WideVariant::extra_royal_attack) (which is ORed into
    /// every move's king-safety and would wrongly forbid exposure); the engine
    /// applies it as a dedicated filter on a non-pass general move, using its
    /// generic open-line facing test (both generals exist, share a file or rank,
    /// and have no piece strictly between): the move is rejected iff the generals
    /// faced both **before and after** it. Only Janggi overrides this.
    fn restricts_facing_general() -> bool {
        false
    }

    // --- Cannon-royal fast-accept diagonal cap (default OFF) ----------------

    /// Returns `Some(radius)` if this cannon-royal variant has **no long-range
    /// diagonal attacker on the king** — no piece slides to the king down a board
    /// diagonal — so the cannon king-safety fast-accept may **truncate the king's
    /// diagonals to `radius` squares** per direction. Returns `None` (the default)
    /// for every variant with a diagonal slider (a bishop / queen), keeping the
    /// full board-length diagonals.
    ///
    /// On the per-move verify path ([`has_cannons`](WideVariant::has_cannons) /
    /// [`has_flying_general`](WideVariant::has_flying_general)) a move whose origin
    /// and destination both lie off every line through the king is accepted
    /// without a make/unmake re-test, because it can change no blocker, screen, or
    /// open file bearing on the king. That set of "king lines" is the king's rank,
    /// file, and two diagonals. When the variant has no diagonal slider, the only
    /// diagonal squares whose occupancy can ever change the king's safety are the
    /// few near ones: a hobbled leaper's leg and a palace screen, all within a
    /// small fixed radius (the Horse's leg is the king's diagonal neighbour; the
    /// Janggi Elephant's two legs lie within two diagonal steps; the cannon's
    /// palace-diagonal screen and a palace-diagonal chariot blocker are one step
    /// away). Capping the diagonals at that radius drops only far-diagonal squares
    /// that provably cannot bear on the king, so it widens the fast-accept while
    /// leaving the authoritative king-safety scan — and therefore the legal move
    /// set — byte-identical.
    ///
    /// Janggi and Xiangqi (cannon, hobbled leapers, palace, no bishop) return
    /// `Some(2)`; every other variant keeps the default `None`.
    fn king_diag_attack_radius() -> Option<u8> {
        None
    }

    // --- Makpong king-may-not-flee-check (default OFF) --------------------

    /// Returns `true` if this variant forbids the king from **fleeing** a check:
    /// while the side to move is in check, the king may move **only to capture
    /// the single checker** — it may not step to a safe empty square, and it may
    /// not capture a checker that is itself defended (such a capture lands on an
    /// attacked square and is rejected by the ordinary king-danger filter). The
    /// check must otherwise be answered by another piece (a block or a capture of
    /// the checker), exactly as in [Makpong](super::variants::makpong) — a Makruk
    /// tie-break variant ("Defensive Chess").
    ///
    /// This mirrors Fairy-Stockfish's `makpongRule`, whose legality test rejects a
    /// king move while in check unless its destination is the (lone) checker's
    /// square. Under **double check** there is no single checker square the king
    /// could capture, so no king move is legal at all — the king-target set is
    /// emptied, matching FSF (its `checkers() ^ to` is never zero with two checker
    /// bits set).
    ///
    /// The default is `false`. While it is `false` the generic engine never
    /// inspects this rule — the king's escape squares are generated exactly as
    /// before — so every other variant is byte-identical. Only Makpong overrides
    /// this to `true`; it otherwise reuses the entire Makruk rule layer unchanged.
    fn king_may_only_capture_checker() -> bool {
        false
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

    // --- Alice chess two-board transfer (default OFF) ---------------------

    /// Returns `true` if this variant is **Alice chess**: the game is played over
    /// two mirror 8x8 boards (A and B), at most one piece per square across *both*
    /// boards, and a piece **moves** by normal chess rules on the board it
    /// currently occupies and then **transfers** to the same square on the *other*
    /// board (`docs`/Wikipedia "Alice chess").
    ///
    /// The default is `false`. While it is `false` the generic engine never
    /// consults the per-piece board-membership mask
    /// ([`GenericState::board_b`](super::position::GenericState::board_b), which
    /// stays empty), never restricts movement or king-safety to a single plane,
    /// and never applies the post-move transfer — so every other variant produces
    /// byte-identical moves, state, and FEN to a build without the Alice mechanic.
    /// Only [`Alice`](super::variants::alice) overrides this to `true`.
    ///
    /// When `true`, move generation, legality (king-safety), and move application
    /// all route through the dedicated Alice path, which reads each piece's plane
    /// from the `board_b` mask: a piece on plane B is in `board_b`, a piece on
    /// plane A is not. Captures, checks, and blocking are all **same-plane** only;
    /// the destination of every move must be **vacant on the opposite plane** (the
    /// plane the piece transfers to).
    fn is_alice() -> bool {
        false
    }

    // --- Sittuyin placement phase (default OFF) ---------------------------

    /// Returns `true` if this variant has a **setup / placement phase**: the
    /// non-pawn pieces start off-board in a pocket and are dropped, one per ply
    /// in alternation, onto the player's own territory before normal play begins
    /// (`docs/fairy-variants-architecture.md` §4.4). Sittuyin is the only such
    /// variant.
    ///
    /// The default is `false`. While it is `false` the generic engine skips every
    /// placement code path — the pocket stays [`GenericPlacement::NONE`], no drop
    /// is ever emitted, and the FEN carries no holdings bracket — so a
    /// non-placement variant produces byte-identical moves, state, and FEN to a
    /// build without the feature.
    ///
    /// [`GenericPlacement::NONE`]: super::position::GenericPlacement::NONE
    fn has_placement() -> bool {
        false
    }

    /// The initial setup-phase pocket for a fresh game: the pieces each side must
    /// deploy. The default is [`GenericPlacement::NONE`] (nothing to deploy),
    /// matching `has_placement() == false`. A placement variant overrides
    /// [`WideVariant::starting_position`] to seed a populated value.
    ///
    /// [`GenericPlacement::NONE`]: super::position::GenericPlacement::NONE
    fn initial_placement() -> super::position::GenericPlacement {
        super::position::GenericPlacement::NONE
    }

    /// Returns the squares onto which `color` may **drop** a pocketed `role`
    /// during the placement phase, given the current `board`.
    ///
    /// Only consulted when [`has_placement`](WideVariant::has_placement) is
    /// `true`. The default — the full board minus all occupied squares — is a
    /// safe fallback; Sittuyin overrides it with its territory rule (the three
    /// nearest ranks, minus own pawns, with Rooks confined to the back rank). A
    /// drop is unconditionally pseudo-legal there (FSF applies no check filtering
    /// during placement).
    fn placement_targets(_role: WideRole, _color: Color, board: &Board<G>) -> Bitboard<G> {
        !board.occupied()
    }

    /// Returns the **special-promotion landing squares** for the side-to-move
    /// pawn standing on `from`, or `None` if the pawn may not specially promote.
    ///
    /// Only consulted when [`has_placement`](WideVariant::has_placement) is
    /// `true`. The default is `None` (no special promotion). Sittuyin overrides
    /// it: while a side has **no Met on the board**, each of its pawns may
    /// transform into a Met (the only [`promotion_config`] role) either **in
    /// place** — the returned set then contains `from` itself, a null-displacement
    /// promotion — or by a one-step ferz move to an **empty** diagonal square.
    /// The returned set is the union of those landing squares; the generic pawn
    /// generator filters each square by the live check mask and pin line, so the
    /// emitted promotions obey the same legality as every other move. This
    /// expresses a promotion the rank-based standard path cannot.
    ///
    /// [`promotion_config`]: WideVariant::promotion_config
    fn special_promotion_targets(
        _board: &Board<G>,
        _from: Square<G>,
        _color: Color,
    ) -> Option<Bitboard<G>> {
        None
    }

    /// For a placement variant whose deployment **grants standard castling**
    /// (Placement / Pre-Chess), the file the king must occupy on its
    /// [`castle_rank`](WideVariant::castle_rank) for a freshly dropped king or
    /// rook to confer castling rights; `None` if the placement phase never grants
    /// castling.
    ///
    /// Only consulted when [`has_placement`](WideVariant::has_placement) is
    /// `true`. The default is `None`: a placement variant whose deployment confers
    /// no castling (Sittuyin) leaves the castling rights exactly as the drops left
    /// them, so it — and every non-placement variant — is byte-identical to a
    /// build without this hook.
    ///
    /// When `Some(file)`, the generic engine re-derives the dropping side's rights
    /// after each placement drop: with that side's king on `(file, castle_rank)`,
    /// a rook on the queenside corner (file `0`) confers the queenside right and a
    /// rook on the kingside corner (file `WIDTH - 1`) the kingside right — the
    /// standard a-/h-file rook castling [`GenericCastling::standard`] uses. This
    /// matches Fairy-Stockfish's `placement`, which assigns `KQkq` incrementally
    /// as the king and corner rooks reach their squares.
    ///
    /// [`GenericCastling::standard`]: super::position::GenericCastling::standard
    fn placement_castling_king_file() -> Option<u8> {
        None
    }

    // --- Shogi hand / drops + per-piece promotion (default OFF) ----------

    /// Returns `true` if this variant has a **persistent hand**: a captured piece
    /// flips side and enters the captor's hand, from which it may later be
    /// **dropped** back onto an empty square as the captor's own piece (Shogi,
    /// crazyhouse). The hand rides in [`GenericPlacement`]
    /// — the same per-color, per-role count store the Sittuyin placement pocket
    /// uses — but here it persists for the whole game and is fed by captures.
    ///
    /// The default is `false`. While it is `false` the generic engine never banks
    /// a captured piece, never emits a drop, and writes no holdings bracket, so a
    /// variant without a hand produces byte-identical moves, state, and FEN to a
    /// build without the hand mechanic. Only Shogi overrides this to `true`.
    fn has_hand() -> bool {
        false
    }

    /// Returns `true` if a captured piece is **banked into the captor's hand**.
    ///
    /// Only consulted when [`has_hand`](WideVariant::has_hand) is `true`. The
    /// default is `true` — the Shogi / crazyhouse rule, where a capture flips the
    /// taken piece to the captor's side and adds it to the hand. Synochess sets a
    /// hand for its **fixed** Black soldier reinforcement pocket, but that pocket
    /// is never replenished (FSF `capturesToHand = false`): it overrides this to
    /// `false` so a capture drops nothing into either hand. Shinobi likewise has a
    /// **fixed starting reserve** consumed only by drops, so it too overrides this
    /// to `false`. Keeping the default `true` leaves every hand-banking site
    /// byte-identical for Shogi / crazyhouse.
    fn captures_to_hand() -> bool {
        true
    }

    /// Returns `true` if a piece of this variant's **Pawn moves as a forward
    /// stepper** (the Shogi pawn: one square straight forward, capturing straight
    /// ahead) rather than as a standard chess pawn (double push, diagonal capture,
    /// en passant). Only consulted when [`has_hand`](WideVariant::has_hand) is
    /// `true`.
    ///
    /// The default mirrors the pre-hook behaviour: a hand variant's Pawn is a
    /// forward stepper (`has_hand()`), keeping Shogi byte-identical. Shinobi
    /// overrides it to `false` — it has a hand and drops, but its Pawn is an
    /// ordinary chess pawn (it promotes into a Commoner on entering the far zone).
    fn pawn_is_stepper() -> bool {
        Self::has_hand()
    }

    /// Returns the role a promotable piece of `role` **becomes** when it promotes.
    /// Only consulted when [`has_hand`](WideVariant::has_hand) is `true` and the
    /// role [`role_can_promote`](WideVariant::role_can_promote)s, on the generic
    /// per-piece promotion path.
    ///
    /// The default is [`WideRole::promoted_form`] — the Shogi mapping (Pawn→Tokin,
    /// Rook→Dragon, …), keeping Shogi byte-identical. Shinobi overrides it: a Fers
    /// promotes to a Bishop, a Shogi Knight to a Knight, and a Lance to a Rook
    /// (its Pawn promotes via the standard pawn path, not here).
    fn role_promoted_to(role: WideRole) -> WideRole {
        role.promoted_form()
    }

    /// Returns the role a piece of `role` **flips into after every board move it
    /// makes** (the Kyoto Shogi mechanic), or `None` if the piece does not flip.
    ///
    /// The default is `None` — no piece flips, so the move-application path never
    /// rewrites a moved piece's role and every other variant is byte-identical.
    /// Only Kyoto Shogi overrides it: each of its five flipping pieces alternates
    /// between two forms move-to-move (Pawn ↔ promoted-Pawn, Silver ↔
    /// promoted-Silver, Lance ↔ promoted-Lance, Knight ↔ promoted-Knight), so a
    /// base piece flips to its promoted form and a promoted piece flips back to its
    /// base. The King has no alternate form and never flips (`None`). The flip is a
    /// pure post-move state transform — it changes the moved piece's role at its
    /// destination *after* legality is decided, so the mask-based legality of the
    /// move itself is unaffected (a flip can neither expose nor shield the mover's
    /// own king, only the **next** position sees the flipped role).
    fn flips_on_move(_role: WideRole) -> Option<WideRole> {
        None
    }

    /// Returns the role a piece of `role` **flips into after any move that
    /// captures** (the Micro Shogi `piecePromotionOnCapture` mechanic), or `None`
    /// if the piece does not flip on capture. A base piece flips to its promoted
    /// form and a promoted piece flips back to its base — but **only on a capturing
    /// move**; a quiet move never flips.
    ///
    /// The default is `None` — no piece flips on capture, so the move-application
    /// path never rewrites a captor's role and every other variant is
    /// byte-identical. Only Micro Shogi overrides it: its Pawn ↔ promoted-Pawn,
    /// Lance ↔ promoted-Lance, Bishop ↔ promoted-Bishop, and Rook ↔ promoted-Rook
    /// pairs each toggle whenever the piece captures (FSF's
    /// `mandatoryPiecePromotion` + `pieceDemotion` gated by
    /// `piecePromotionOnCapture`). The King has no alternate form and never flips
    /// (`None`).
    ///
    /// Like [`flips_on_move`](WideVariant::flips_on_move) this is a pure post-move
    /// state transform — it changes the captor's role at its destination *after*
    /// legality is decided, so the mask-based legality of the move itself is
    /// unaffected (a flip can neither expose nor shield the mover's own king, only
    /// the **next** position sees the flipped role). It also drives the dual-form
    /// drop expansion (FSF `dropPromoted`) exactly as `flips_on_move` does for
    /// Kyoto Shogi: a held base role may be dropped as either form.
    fn flips_on_capture(_role: WideRole) -> Option<WideRole> {
        None
    }

    /// Returns the role a face-down piece of `role` standing on its origin square
    /// `from` is **revealed** to when it makes its first board move (the Jieqi
    /// mechanic), or `None` if the piece does not reveal.
    ///
    /// The default is `None` — no piece reveals, so the move-application path never
    /// rewrites a moved piece's role through this hook and every other variant is
    /// byte-identical. Only Jieqi overrides it: a face-down [`WideRole::Dark`]
    /// piece reveals on its first move. The **deterministic baseline** (the one in
    /// the make-move / perft path) reveals it to the Xiangqi piece native to its
    /// `from` (home) square — the no-shuffle identity assignment, under which the
    /// whole Jieqi tree collapses to standard Xiangqi and is perft-validatable
    /// against Fairy-Stockfish `UCI_Variant xiangqi`. The full **stochastic**
    /// reveal-from-pool (a random unrevealed identity) is a separate, explicitly
    /// seeded model (see `variants::jieqi`); it is not baked into the deterministic
    /// perft path. Like the Kyoto flip this is a pure post-move state transform — it
    /// rewrites the moved piece's role at its destination *after* legality is
    /// decided, so the move's own legality is unaffected; only the next position
    /// sees the revealed role.
    fn reveal_on_move(_role: WideRole, _from: Square<G>) -> Option<WideRole> {
        None
    }

    /// Returns `true` if a held piece may be **dropped in either its base or its
    /// promoted form** (FSF `dropPromoted`; the Kyoto Shogi rule). Only consulted
    /// when [`has_hand`](WideVariant::has_hand) is `true`.
    ///
    /// The default is `false` — a drop always deploys the (base) role banked in
    /// hand, the Shogi / crazyhouse rule, keeping every hand variant
    /// byte-identical. Kyoto overrides it to `true`: the hand stores the base role,
    /// but on a drop the side chooses to place it either as that base role or as
    /// its [`role_promoted_to`](WideVariant::role_promoted_to) form, so the drop
    /// generator emits both and the drop-application path consumes the **base**
    /// role from hand ([`role_hand_base`](WideVariant::role_hand_base)) regardless
    /// of the deployed form.
    fn drops_can_promote() -> bool {
        false
    }

    /// Returns `true` if a promotable piece **must** promote on any move that
    /// starts or ends in the promotion zone — there is no non-promoting
    /// alternative. Only consulted when [`has_hand`](WideVariant::has_hand) is
    /// `true`, on the generic per-piece promotion path.
    ///
    /// The default is `false`: Shogi promotion is *optional* in the zone (the
    /// generator emits both the promoting and the non-promoting move) except where
    /// [`role_promotion_forced`](WideVariant::role_promotion_forced) makes it
    /// compulsory. Shinobi overrides it to `true`, matching FSF's
    /// `mandatoryPiecePromotion = true`: a zone move is always the promoting form.
    fn promotion_mandatory_in_zone() -> bool {
        false
    }

    /// Returns the squares onto which `color` may **drop** a held `role`, given
    /// the current `board`. Only consulted when [`has_hand`](WideVariant::has_hand)
    /// is `true`.
    ///
    /// The default — every empty square — is the crazyhouse rule. Shogi overrides
    /// it with its drop restrictions: a piece may not be dropped where it would
    /// have no future move (a Pawn or Lance on the last rank, a Knight on the last
    /// two ranks), and a Pawn may not be dropped onto a file that already holds an
    /// unpromoted friendly Pawn (**nifu**). The pawn-drop-mate restriction
    /// (**uchifuzume**) is *not* expressed here — it depends on the resulting
    /// position, so the generic drop generator applies it via
    /// [`drop_gives_legal_mate_ok`](WideVariant::pawn_drop_mate_forbidden).
    fn drop_targets(_role: WideRole, _color: Color, board: &Board<G>) -> Bitboard<G> {
        !board.occupied()
    }

    /// Returns `true` if this variant forbids a **pawn drop that delivers
    /// immediate checkmate** (Shogi's *uchifuzume*). Only consulted when
    /// [`has_hand`](WideVariant::has_hand) is `true` and the dropped role is the
    /// one [`pawn_drop_role`](WideVariant::pawn_drop_role) names.
    ///
    /// The default is `false` (crazyhouse allows pawn-drop mate). Shogi overrides
    /// it to `true`; the generic drop generator then suppresses a pawn drop whose
    /// resulting position is checkmate for the opponent.
    fn pawn_drop_mate_forbidden() -> bool {
        false
    }

    /// Returns the role whose drop is subject to the *uchifuzume* mate check (the
    /// Shogi Pawn). Only consulted when
    /// [`pawn_drop_mate_forbidden`](WideVariant::pawn_drop_mate_forbidden) is
    /// `true`. The default is [`WideRole::Pawn`].
    fn pawn_drop_role() -> WideRole {
        WideRole::Pawn
    }

    /// Returns `true` if a piece of `role`'s attack set is **direction-dependent**
    /// — asymmetric under a color flip, so a piece of one color attacking `sq` is
    /// found by projecting the *opposite* color's pattern back from `sq` (as a
    /// pawn's diagonal capture is). The generic [`attackers_to`] uses this when
    /// scanning for attackers of a square.
    ///
    /// The default classifies only the Pawn and the Berolina Hoplite, matching the
    /// pre-hook behaviour exactly (every existing variant). Shogi overrides it to
    /// add its forward-biased steppers — the Gold and Silver Generals, the Knight,
    /// the Lance, and the Gold-moving promoted minors (+P/+L/+N/+S) — whose attack
    /// sets all point forward and so must be projected with the opposite color.
    ///
    /// [`attackers_to`]: super::position::GenericPosition::attackers_to
    fn role_attack_is_directional(role: WideRole) -> bool {
        matches!(role, WideRole::Pawn | WideRole::Hoplite)
    }

    /// Returns `true` if a piece of `role` has a **geometrically asymmetric,
    /// occupancy-dependent** attack set — one where "a attacks b" is *not* the same
    /// as "b attacks a" under the same occupancy, so it cannot be detected by
    /// reverse-projecting the role's pattern from the target square.
    ///
    /// The canonical case is the Xiangqi / Minixiangqi **Horse**: its leap is
    /// hobbled by the leg square *adjacent to the horse* toward the leap, which is a
    /// *different* square than the leg adjacent to the target toward the horse. A
    /// reverse-projection from the target therefore tests the wrong leg and misses
    /// (or invents) horse attacks. For such a role the generic
    /// [`attackers_to`](super::position::GenericPosition::attackers_to) and the
    /// cannon-verify king-safety test instead detect attackers by projecting the
    /// role's attack set **forward from each candidate origin square** and asking
    /// whether it reaches the target — i.e. consistent with the move generator.
    ///
    /// This is independent of [`role_attack_is_directional`]: that hook only flips
    /// the color used for the projection (a pawn's diagonal capture is symmetric
    /// *geometrically*, just color-mirrored), which cannot fix a per-leap geometric
    /// asymmetry. The default is `false`; Xiangqi overrides it for the Horse.
    ///
    /// [`role_attack_is_directional`]: WideVariant::role_attack_is_directional
    fn role_attack_is_leg_asymmetric(_role: WideRole) -> bool {
        false
    }

    /// Returns the plain slider pattern a role's **king-safety reverse projection**
    /// is exactly equal to, enabling the cannon verify path to reuse the king's
    /// precomputed `KingLineMasks` instead of
    /// re-deriving the slider's line masks on every sibling move.
    ///
    /// This is a pure performance hook for the cannon king-safety verify
    /// ([`king_safe_after`]): it changes *how* a symmetric slider's reverse
    /// projection from the king is computed, never *what* it computes. A variant
    /// returns `Some(kind)` for a role **only** when that role's
    /// [`role_attacks`](WideVariant::role_attacks), reverse-projected from any
    /// square the king may occupy, is bit-for-bit the standard rook / bishop /
    /// queen ray (no palace-diagonal addendum, no region masking, not directional,
    /// not leg-asymmetric). The default is `None`, so every variant — including
    /// every non-cannon variant, which never reaches this path — keeps the existing
    /// reverse-projection and is byte-identical.
    ///
    /// [`king_safe_after`]: super::position::GenericPosition
    fn royal_slider_kind(_role: WideRole) -> Option<RoyalSlider> {
        None
    }

    /// Returns a **superset** of the squares from which a piece of `role` could
    /// attack the royal square `king` — a cheap, occupancy-independent over-estimate
    /// the cannon king-safety verify uses to skip enemy pieces that cannot possibly
    /// reach the king before running the exact (and costlier) forward projection.
    ///
    /// This is a pure performance hook for the leg-asymmetric / forward-projected
    /// roles in [`king_safe_after`]: instead of computing the full attack set of
    /// **every** enemy piece of `role` and testing whether it contains the king,
    /// the verify first intersects the role's enemy pieces with this mask, then runs
    /// the exact forward projection only on the survivors. Because the mask is a
    /// **superset** (it ignores hobbling legs, region confinement, and cannon
    /// screens — all re-checked exactly by the forward projection), no genuine
    /// attacker is ever excluded, so the result is bit-for-bit identical. The king
    /// square is fixed across a node's sibling moves, so the mask is computed once
    /// per node and reused.
    ///
    /// The default is `None`, meaning "no cheap superset is available, test every
    /// piece" — the existing behaviour, so every non-cannon variant (which never
    /// reaches this path) is byte-identical. A cannon variant returns `Some(mask)`
    /// for each forward-projected role whose reach geometry has such a superset
    /// (e.g. the Horse's knight-shape neighbourhood of the king, the Cannon's king
    /// rank/file plus any palace-diagonal corner).
    ///
    /// [`king_safe_after`]: super::position::GenericPosition
    fn royal_reach_superset(_role: WideRole, _king: Square<G>) -> Option<Bitboard<G>> {
        None
    }

    /// Returns `true` if a piece of `role` **may promote** by a move that starts
    /// or ends in the promotion zone. Only consulted when
    /// [`has_hand`](WideVariant::has_hand) is `true` (the generic per-piece
    /// promotion path is otherwise inert).
    ///
    /// The default is `false`. Shogi overrides it for its promotable pieces (Pawn,
    /// Lance, Knight, Silver, Rook, Bishop); the Gold General and King never
    /// promote, and an already-promoted piece never promotes again.
    fn role_can_promote(_role: WideRole) -> bool {
        false
    }

    /// Returns `true` if a piece of `role` of `color` moving to `to_rank` **must**
    /// promote — a non-promoting move there is then illegal because the piece
    /// would have no further move. Only consulted when
    /// [`has_hand`](WideVariant::has_hand) is `true` and the role
    /// [`role_can_promote`](WideVariant::role_can_promote)s.
    ///
    /// The default is `false`. Shogi overrides it: a Pawn or Lance on the last
    /// rank, and a Knight on the last two ranks, must promote.
    fn role_promotion_forced(_role: WideRole, _color: Color, _to_rank: u8) -> bool {
        false
    }

    /// Returns the **base role banked into the captor's hand** when a piece of
    /// `role` is captured under [`captures_to_hand`](WideVariant::captures_to_hand).
    /// A captured *promoted* piece reverts to its unpromoted base before entering
    /// the hand (Shogi banks a captured Dragon as a Rook; crazyhouse banks a
    /// captured queen as a queen).
    ///
    /// The default is [`WideRole::promoted_base`] — the Shogi promoted-form
    /// mapping, with every other role banked as itself, keeping Shogi and
    /// crazyhouse byte-identical. Shogun overrides it because its promoted forms
    /// **reuse roles that also exist as full pieces** (a promoted Bishop is the
    /// Hawk / Archbishop, a promoted Rook the Elephant / Chancellor, a promoted
    /// Knight the Kheshig / Centaur, a promoted Pawn the Commoner, a promoted Fers
    /// the Queen), so it cannot rely on the global [`WideRole::promoted_base`] (that
    /// would mis-bank a crazyhouse queen as a fers). In Shogun every one of those
    /// promoted forms banks back to its base — Hawk → Bishop, Elephant → Rook,
    /// Kheshig → Knight, Commoner → Pawn, Queen → Met (fers) — matching FSF, where
    /// a promoted piece is a `+X` token that sheds its `+` on capture.
    fn role_hand_base(role: WideRole) -> WideRole {
        role.promoted_base()
    }

    /// Returns `true` if a captured piece that **reached the board by promotion**
    /// is banked into the captor's hand as a **Pawn** rather than by its own role
    /// — the crazyhouse "promoted pieces demote" rule. Only consulted when
    /// [`has_hand`](WideVariant::has_hand) and
    /// [`captures_to_hand`](WideVariant::captures_to_hand) are both `true`.
    ///
    /// The default is `false`. Shogi-family hand variants (Shogi, Shogun, …)
    /// represent every promoted form as a **distinct role** and revert it via
    /// [`role_hand_base`](WideVariant::role_hand_base), so they need no separate
    /// promoted bookkeeping and keep this off. Crazyhouse-style variants
    /// (Capahouse) instead promote a Pawn into an ordinary army role (Queen,
    /// Archbishop, …) that is **indistinguishable on the board** from a natural
    /// piece of that role, so a per-square *promoted mask* records which occupants
    /// arrived by promotion; capturing one banks a Pawn. The mask rides the FEN as
    /// a trailing `~` on the promoted piece's token (e.g. `Q~`) and is maintained
    /// across moves by the make-move path. Default-off, so every other variant
    /// carries an empty mask, never reads the `~` token, and is byte-identical.
    fn demotes_promoted_captures() -> bool {
        false
    }

    /// Returns `true` if a piece of `role` of `color` is **forbidden from
    /// promoting** in the current `board` because the variant caps the number of
    /// its promoted form (FSF `promotionLimit`). When `true`, the generic per-piece
    /// promotion path emits only the non-promoting move on a zone move (it never
    /// collides with [`role_promotion_forced`](WideVariant::role_promotion_forced) in any variant that uses both, since
    /// a forced promotion never targets a capped form).
    ///
    /// Only consulted when [`has_hand`](WideVariant::has_hand) is `true` and the
    /// role [`role_can_promote`](WideVariant::role_can_promote)s, on the per-piece
    /// promotion path. The default is `false` (no cap), keeping Shogi and every
    /// other hand variant byte-identical. Shogun overrides it: a Knight, Bishop,
    /// Rook, or Fers may not promote while the side already holds **one** of the
    /// corresponding Centaur, Archbishop, Chancellor, or Queen on the board
    /// (`promotionLimit = g:1 a:1 m:1 q:1`); the Commoner (promoted Pawn) is
    /// uncapped.
    fn role_promotion_blocked_by_limit(_role: WideRole, _color: Color, _board: &Board<G>) -> bool {
        false
    }

    // --- reserved fairy hooks (no-ops for standard rules) -----------------

    /// Returns the region mask for a [`WideRegion`]. Reserved for Phase 3
    /// region confinement; the default is the full board (no confinement).
    fn region_mask(_region: WideRegion) -> Bitboard<G> {
        Bitboard::FULL
    }

    // --- Chak per-piece promotion without a hand (default OFF) ------------

    /// Returns `true` if this variant promotes non-pawn pieces by a move that
    /// **ends in the promotion zone**, *without* a hand (Chak: the King promotes
    /// to the Divine Lord and the Soldier to the Shaman). The default is `false`.
    ///
    /// While it is `false` the generic engine never expands a non-pawn move into a
    /// promotion on the multi-royal pseudo path, so every other variant is
    /// byte-identical. When `true`, a move of a [`role_can_promote`] piece whose
    /// destination [`in_promotion_zone`] is emitted as a promotion to
    /// [`role_promoted_to`] (plus the non-promoting alternative unless
    /// [`promotion_mandatory_in_zone`]). This is the no-hand analogue of the
    /// hand-variant per-piece promotion (which stays gated behind [`has_hand`]);
    /// Chak rides the multi-royal verify path, where the King's promotion to the
    /// (also-royal) Divine Lord is naturally re-verified for king safety.
    ///
    /// [`role_can_promote`]: WideVariant::role_can_promote
    /// [`in_promotion_zone`]: WideVariant::in_promotion_zone
    /// [`role_promoted_to`]: WideVariant::role_promoted_to
    /// [`promotion_mandatory_in_zone`]: WideVariant::promotion_mandatory_in_zone
    /// [`has_hand`]: WideVariant::has_hand
    fn has_piece_promotion() -> bool {
        false
    }

    // --- Chak temple win (default OFF) -----------------------------------

    /// Returns `true` if this variant is won by moving a **Divine Lord** onto the
    /// enemy **temple square** (Chak; FSF `flagPiece = d`, `flagRegion…`). The
    /// default is `false`.
    ///
    /// While it is `false` the engine never evaluates the rule, so every other
    /// variant is byte-identical. When `true`, a position in which some side's
    /// Divine Lord stands on its [`temple_goal`](WideVariant::temple_goal) square
    /// is **terminal**: the move generator short-circuits to *zero moves* (the node
    /// is a perft leaf, exactly as Fairy-Stockfish truncates it), and the win is
    /// reported as a [`WideEndReason::VariantWin`]. Chak rides the multi-royal
    /// verify path, whose single chokepoint funnels the truncation the same way
    /// FSF does.
    fn has_temple_win() -> bool {
        false
    }

    /// Returns the goal **temple square(s)** a Divine Lord of `color` wins by
    /// reaching (Chak: the enemy side's central temple). Only consulted while
    /// [`has_temple_win`](WideVariant::has_temple_win) is `true`; the default is
    /// the empty board (no goal).
    fn temple_goal(_color: Color) -> Bitboard<G> {
        Bitboard::EMPTY
    }

    /// Hook for variant-specific terminal conditions (king-capture wins, race
    /// goals). The default reports `None` — standard chess ends only by the
    /// generic checkmate / stalemate / material rules the position computes.
    fn extra_terminal(_board: &Board<G>, _state: &GenericState<G>) -> Option<WideEndReason> {
        None
    }

    // --- Check-win (Checkshogi; default OFF) ------------------------------

    /// Returns `true` if **giving check wins the game** — the side that delivers a
    /// check has won immediately, so a position with the side to move **in check**
    /// is terminal (Checkshogi; FSF's `checkCounting = true` with a one-check goal,
    /// the `1+1` FEN field). The default is `false`.
    ///
    /// While it is `false` the engine never evaluates the rule, so every other
    /// variant is byte-identical. When `true`, a position in which the side to move
    /// is in check is **terminal**: the move generator short-circuits to *zero
    /// moves* (the node is a perft leaf, exactly as Fairy-Stockfish truncates it),
    /// and the win is reported as a [`WideEndReason::VariantWin`] credited to the
    /// **checker** (the side *not* to move). Because a single check ends the game,
    /// there is no cross-move counter to track — the terminal is a pure property of
    /// the current position.
    fn wins_on_check() -> bool {
        false
    }

    // --- Repetition / draw rules (default OFF) ----------------------------
    //
    // These hooks drive the history-dependent terminal rules, which a bare
    // [`GenericPosition`] cannot see (it is deliberately history-free, so perft
    // never pays for a position history and stays byte-identical). They are
    // consulted only by [`GenericGame`](super::game::GenericGame), the opt-in
    // wrapper that records the key of every position that has occurred. A variant
    // that leaves [`tracks_repetition`](Self::tracks_repetition) `false` (every
    // variant but the Asian families below) records no history and is unaffected.

    /// Returns `true` if [`GenericGame`](super::game::GenericGame) should record a
    /// position history for this variant so its repetition / perpetual-check rules
    /// can fire. The default is `false`: no history is kept, the repetition hooks
    /// are never consulted, and the game wrapper behaves like a thin
    /// [`GenericPosition`](crate::geometry::GenericPosition) driver. Shogi, Xiangqi, and Janggi override it.
    fn tracks_repetition() -> bool {
        false
    }

    /// The number of occurrences of a position that draws (or, under
    /// [`perpetual_check_loses`](Self::perpetual_check_loses), is adjudicated) by
    /// repetition. Consulted only when [`tracks_repetition`](Self::tracks_repetition)
    /// is `true`. The default is the western three-fold count; Shogi's sennichite
    /// overrides it to four.
    fn repetition_fold() -> usize {
        3
    }

    /// The [`WideEndReason`] a plain (non-perpetual-check) repetition draw is
    /// reported as. The default is [`WideEndReason::Repetition`]; Shogi overrides
    /// it to [`WideEndReason::Sennichite`]. Consulted only when
    /// [`tracks_repetition`](Self::tracks_repetition) is `true`.
    fn repetition_draw_reason() -> WideEndReason {
        WideEndReason::Repetition
    }

    /// Returns `true` if a repetition brought about by **perpetual check** is a
    /// **loss for the checking side** rather than a draw (Shogi sennichite's
    /// perpetual-check exception; Xiangqi `perpetualCheckIllegal`). The default is
    /// `false`. Consulted only when [`tracks_repetition`](Self::tracks_repetition)
    /// is `true`.
    fn perpetual_check_loses() -> bool {
        false
    }

    /// Returns `true` if this variant draws by **bikjang** — the two generals
    /// facing each other down an open file when the opponent passes, leaving the
    /// side to move with no legal continuation (Janggi). The default is `false`, so
    /// [`end_reason`](super::position::GenericPosition::end_reason) never runs the
    /// [`is_facing_generals`](super::position::GenericPosition::is_facing_generals)
    /// test and every other variant is byte-identical. Only Janggi overrides it.
    ///
    /// The zero-move truncation that makes the node terminal already lives in the
    /// move generator (gated on [`allows_pass`](Self::allows_pass) +
    /// [`restricts_facing_general`](Self::restricts_facing_general)) and is counted
    /// by perft; this hook only relabels that terminal from a stalemate into the
    /// [`WideEndReason::Bikjang`] draw it is.
    fn has_bikjang() -> bool {
        false
    }

    /// The ply count at which the variant's **move-count rule** (the generic
    /// analogue of the fifty-move rule) draws: `Some(n)` means a position whose
    /// halfmove clock has reached `n` is a [`WideEndReason::MoveRule`] draw. The
    /// default is `None` (no move-count rule), so the clock never ends the game and
    /// every variant that does not opt in is byte-identical. Reported from the
    /// single position.
    fn move_rule_plies() -> Option<u16> {
        None
    }

    /// Returns the variant's **counting** endgame rule, or `None` if it has none.
    /// Tracked by [`GenericGame`](super::game::GenericGame), which reproduces
    /// Fairy-Stockfish's board-honour and material-scaled pieces-honour countdown
    /// exactly (see [`WideCountingRule`] and that type). The default is `None`;
    /// Makruk, Cambodian, and ASEAN override it. The rule is terminal-only and
    /// never consulted by move generation, so perft is byte-identical.
    fn counting_rule() -> Option<WideCountingRule> {
        None
    }

    /// Returns this variant's **impasse / jishogi (entering-king)** declaration
    /// rule, or `None` if it has none. The default is `None`, so every non-shogi
    /// variant is byte-identical and the rule is never evaluated. Standard Shogi
    /// overrides it with the [`ImpasseRule`] describing the 27-point declaration.
    ///
    /// The rule is a **terminal-only** adjudication reported from the single
    /// position via [`WideEndReason::Impasse`] (it needs no move history — only the
    /// board, the hands, and the promotion-zone geometry), so move generation and
    /// perft are untouched. See [`ImpasseRule`] for the exact declaration.
    fn impasse_rule() -> Option<ImpasseRule> {
        None
    }

    /// Returns `true` if this variant adjudicates **perpetual chase** as a loss for
    /// the chasing side — the Xiangqi/AXF rule that a side which, on every move
    /// through a repeated cycle, attacks the **same kind of** unprotected (or
    /// value-superior) enemy piece, forcing the repetition, loses exactly as a
    /// perpetual checker does. The default is `false`. Consulted only when
    /// [`tracks_repetition`](Self::tracks_repetition) is `true`; the detection lives
    /// in [`GenericGame`](super::game::GenericGame), so move generation and perft
    /// are untouched. Only Xiangqi overrides it.
    fn perpetual_chase_loses() -> bool {
        false
    }

    /// Returns `true` if this variant adjudicates the **large-shogi
    /// attack-repetition ("chase") rule** as a loss for the attacking side — the
    /// Chu / Dai Shogi rule that, on top of sennichite, a side which keeps
    /// **attacking** enemy pieces through the repeated cycle while the other side
    /// attacks nothing must break the pattern or **lose** (chessvariants Chu
    /// ruleset). The default is `false`. Consulted only when
    /// [`tracks_repetition`](Self::tracks_repetition) is `true`; the detection lives
    /// in [`GenericGame`](super::game::GenericGame), so move generation and perft
    /// are untouched.
    ///
    /// This is distinct from the Xiangqi
    /// [`perpetual_chase_loses`](Self::perpetual_chase_loses) rule: the large-shogi
    /// test applies **no** value-superiority or protection filter — *any* threat on
    /// *any* non-royal enemy piece counts ("however futile") — and adjudicates by
    /// the **asymmetry** of who attacked, not by an FSF-style single-victim identity.
    /// A variant enables at most one of the two chase models. Only Chu and Dai
    /// override it.
    fn attack_repetition_loses() -> bool {
        false
    }

    /// Returns `true` if the position is an **insufficient-material** draw for this
    /// variant. The default is `false` (no material draw is imposed — most fairy
    /// variants do not have one). Reported from the single position via
    /// [`WideEndReason::InsufficientMaterial`]; a variant that wants the rule
    /// overrides this with its own material test.
    fn is_insufficient_material(_board: &Board<G>, _state: &GenericState<G>) -> bool {
        false
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

    /// Returns `true` if a gate deploys a piece drawn from the variant's
    /// **crazyhouse hand** ([`GenericPlacement`])
    /// rather than from the fixed [`GenericGating`] Hawk/Elephant reserve — so
    /// **any** held non-pawn, non-king role may be gated, and the gate consumes it
    /// from the same hand its drops do. Only consulted when
    /// [`supports_gating`](WideVariant::supports_gating) is `true`.
    ///
    /// The default is `false` — the Seirawan model, where the two reserves live in
    /// [`GenericGating`] and a gate places a Hawk or Elephant via the 2-state
    /// `GateRole` move encoding. Seirawan and every non-gating variant are
    /// byte-identical. S-House overrides it to `true`: its reserves and captures
    /// share one hand, the starting Hawk/Elephant are **droppable as well as
    /// gateable**, and a gate emits the wider hand-gate move encoding (an arbitrary
    /// [`WideRole`]). The [`GenericGating`] field still supplies the
    /// gating-**eligible square set** (the virgin back-rank squares); only the
    /// reserve source changes.
    fn gates_from_hand() -> bool {
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

    /// Returns `true` if this variant grants its king and queen a one-time
    /// first-move leap (Cambodian / Ouk Chaktrang).
    ///
    /// The default is `false`. While it is `false` the generic engine emits no
    /// leap moves and never revokes a leap right, so the king's castling-right
    /// revocation stays the standard "any king move clears both rights" — every
    /// other variant is byte-identical. Cambodian overrides this to `true`,
    /// reusing the [`GenericCastling`]
    /// rights field to carry the two per-side leap rights: the **kingside** slot
    /// holds the king's leap right (its home file) and the **queenside** slot the
    /// queen/Met's leap right (its home file). A right is revoked the first time
    /// its piece leaves home (the standard
    /// [`revoke_rights_for_square`](crate::geometry::position) path), exactly as
    /// Fairy-Stockfish's `cambodianMoves` rights behave.
    fn has_first_move_leaps() -> bool {
        false
    }

    /// The king's one-time leap offsets `(file_delta, rank_delta)` from its home
    /// square, color-relative (forward is toward the far rank). Consulted only
    /// when [`has_first_move_leaps`](Self::has_first_move_leaps) is `true` and the
    /// king still holds its leap right; the default is empty.
    ///
    /// In Cambodian the king leaps to the two forward knight squares (it jumps
    /// over any intervening piece and may land only on an empty square), and the
    /// leap is offered only when the king is not in check — the same restriction
    /// FSF applies.
    fn king_leap_offsets(_color: Color) -> &'static [(i8, i8)] {
        &[]
    }

    /// The queen/Met's one-time leap offsets `(file_delta, rank_delta)` from its
    /// home square, color-relative. Consulted only when
    /// [`has_first_move_leaps`](Self::has_first_move_leaps) is `true` and the Met
    /// still holds its leap right; the default is empty.
    ///
    /// In Cambodian the Met (Neang) makes a single two-square straight advance
    /// (jumping the square in front, landing only on an empty square). Unlike the
    /// king leap this is an ordinary piece move, confined by the check mask and
    /// the Met's pin line.
    fn met_leap_offsets(_color: Color) -> &'static [(i8, i8)] {
        &[]
    }

    /// Parses the FEN castling-field encoding of the first-move leap rights into
    /// the [`GenericCastling`] slots, for a `has_first_move_leaps()` variant.
    ///
    /// Cambodian encodes each right by its piece's home **file letter**
    /// (uppercase white, lowercase black; the `DEde` field). The default returns
    /// `None` (unsupported) and is consulted only when
    /// [`has_first_move_leaps`](Self::has_first_move_leaps) is `true`, so every
    /// other variant keeps the plain `KQkq` castling parser unchanged.
    fn parse_first_move_rights(_field: &str) -> Option<GenericCastling> {
        None
    }

    /// Serializes the first-move leap rights into the FEN castling field, for a
    /// `has_first_move_leaps()` variant — the inverse of
    /// [`parse_first_move_rights`](Self::parse_first_move_rights). The default is
    /// a no-op (consulted only when [`has_first_move_leaps`] is `true`).
    ///
    /// [`has_first_move_leaps`]: Self::has_first_move_leaps
    fn write_first_move_rights(_rights: GenericCastling, _out: &mut alloc::string::String) {}
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
    /// The same position (board, side to move, hands, rights — excluding the
    /// move clocks) has recurred enough times to draw under the variant's
    /// [`repetition fold`](WideVariant::repetition_fold). The generic
    /// repetition draw, reported by [`GenericGame`](super::game::GenericGame) for
    /// Xiangqi, Janggi, and any variant whose
    /// [`repetition_draw_reason`](WideVariant::repetition_draw_reason) is this.
    /// Draw.
    Repetition,
    /// Shogi **sennichite**: the same position (board, side to move, **and both
    /// hands**) has recurred four times. Draw, unless it was brought about by
    /// perpetual check, in which case [`PerpetualCheckLoss`](Self::PerpetualCheckLoss)
    /// is reported instead. Reported by [`GenericGame`](super::game::GenericGame).
    Sennichite,
    /// A repetition was brought about by **perpetual check**: one side gave check
    /// on every one of its moves through the repeated cycle. That side (the
    /// checker) **loses** (Shogi, Xiangqi `perpetualCheckIllegal`). Decisive for
    /// the side that was *being* checked. Reported by
    /// [`GenericGame`](super::game::GenericGame), which resolves the winner from
    /// the recorded check history.
    PerpetualCheckLoss,
    /// A repetition was brought about by **perpetual chase** (Xiangqi / AXF
    /// `chasingRule`): one side made a qualifying chase — a fresh attack on the same
    /// kind of unprotected or value-superior enemy piece — on every one of its moves
    /// through the repeated cycle. That side (the chaser) **loses**. Decisive for
    /// the side being chased. Reported by
    /// [`GenericGame`](super::game::GenericGame), which resolves the winner from the
    /// recorded chase history.
    PerpetualChaseLoss,
    /// A repetition was brought about by the **large-shogi attack-repetition
    /// ("chase") rule** (Chu / Dai Shogi): through the repeated cycle one side
    /// **attacked** enemy pieces (any threat on any non-royal, "however futile")
    /// while the other side attacked nothing. That side (the attacker) **loses**;
    /// decisive for the side being attacked. Reported by
    /// [`GenericGame`](super::game::GenericGame), which resolves the loser from the
    /// recorded per-move attack history. Distinct from
    /// [`PerpetualCheckLoss`](Self::PerpetualCheckLoss) (which concerns the enemy
    /// royal and is scored first) and from the Xiangqi
    /// [`PerpetualChaseLoss`](Self::PerpetualChaseLoss) (a value/protection model).
    AttackRepetitionLoss,
    /// Janggi **bikjang**: the two generals face each other down an open file with
    /// the side to move unable to break the confrontation. Draw. Reported from the
    /// single position via [`WideVariant::has_bikjang`].
    Bikjang,
    /// Makruk / Cambodian **counting**: the board-honour countdown expired before
    /// the superior side delivered mate. Draw. Reported by
    /// [`GenericGame`](super::game::GenericGame).
    CountingDraw,
    /// The variant's **move-count rule** (the generic analogue of the fifty-move
    /// rule) elapsed: [`move_rule_plies`](WideVariant::move_rule_plies) plies have
    /// passed with no capture or pawn move. Draw.
    MoveRule,
    /// Shogi **impasse / jishogi (entering-king)**: at the start of its turn the
    /// side to move met the point-count declaration — its king is in the promotion
    /// zone (and not in check), it has enough other pieces in the zone, and its
    /// [`impasse_rule`](WideVariant::impasse_rule) point total reaches the
    /// per-side threshold. Decisive for the side to move (the declaring side).
    /// Reported from the single position via [`WideVariant::impasse_rule`].
    Impasse,
}

/// The standard-chess **insufficient-material** test, shared by the wide variants
/// whose army is the standard chess set — optionally extended with always-mating
/// compounds such as the Capablanca / Grand archbishop and chancellor
/// ([`WideRole::Hawk`] / [`WideRole::Elephant`]).
///
/// It mirrors [`crate::Position::is_insufficient_material`] and is exactly
/// Fairy-Stockfish's `has_insufficient_material` reduced to the standard piece
/// classification (rook / queen / compound = major, knight = unbound minor,
/// bishop = colour-bound minor):
///
/// - **King vs king** is a draw.
/// - **King and a single minor** (one bishop or one knight, either side) **vs
///   king** is a draw.
/// - **Bishops only, all on one colour complex** (any number, either side) is a
///   draw — none can ever guard the other colour, so mate is impossible.
///
/// Everything else is **sufficient**: any pawn, rook, queen, or mating compound
/// (anything that is not a king, knight, or bishop), bishops on both colours, and
/// any knight standing beside another minor (`K+N+N` is *not* an automatic draw —
/// it is unforced but a helpmate exists). A square's colour is its `(file + rank)`
/// parity, so the test is correct on any rectangular [`Geometry`], not only 8x8.
///
/// Consulted only by [`GenericPosition::end_reason`](super::GenericPosition)
/// through the opt-in [`WideVariant::is_insufficient_material`] hook, never by the
/// move generator — so a variant that enables it stays byte-identical under perft.
pub(crate) fn standard_insufficient_material<G: Geometry>(board: &Board<G>) -> bool {
    let knights = board.by_role(WideRole::Knight);
    let bishops = board.by_role(WideRole::Bishop);
    let kings = board.by_role(WideRole::King);
    // Any occupied square that is neither a king nor a minor holds a pawn, rook,
    // queen, or mating compound — all sufficient to (help-)force mate.
    let others = board.occupied() & !(kings | knights | bishops);
    if !others.is_empty() {
        return false;
    }
    let minors = knights | bishops;
    match minors.count() {
        0 | 1 => true,
        // A knight alongside any further minor is treated as sufficient (it can
        // help-mate, and K+N+N is not an automatic draw); only an all-one-colour
        // bishop battery is the guaranteed draw.
        _ if !knights.is_empty() => false,
        _ => bishops_share_one_colour(bishops),
    }
}

/// Returns `true` if every bishop in `bishops` stands on the same colour complex
/// (so the set can never guard a square of the other colour). Empty and singleton
/// sets trivially qualify. The colour of a square is its `(file + rank)` parity.
fn bishops_share_one_colour<G: Geometry>(bishops: Bitboard<G>) -> bool {
    let mut colour: Option<u8> = None;
    for square in bishops {
        let parity = (square.file() + square.rank()) & 1;
        match colour {
            None => colour = Some(parity),
            Some(seen) if seen != parity => return false,
            Some(_) => {}
        }
    }
    true
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
            placement: GenericPlacement::NONE,
            halfmove_clock: 0,
            fullmove_number: 1,
            consecutive_passes: 0,
            board_b: crate::geometry::Bitboard::EMPTY,
        };
        (board, state)
    }
}
