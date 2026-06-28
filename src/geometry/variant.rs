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

    /// Returns the set of royal squares of `color` whose safety defines check.
    ///
    /// The default is every king of `color` (one in standard chess). Multi-king
    /// variants (Spartan) and non-royal-king variants (Duck) override this; the
    /// generic king-safety machinery treats an empty royal set as "never in
    /// check".
    fn royal_squares(board: &Board<G>, color: Color) -> Bitboard<G> {
        board.kings_of(color)
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

    // --- Shogi hand / drops + per-piece promotion (default OFF) ----------

    /// Returns `true` if this variant has a **persistent hand**: a captured piece
    /// flips side and enters the captor's hand, from which it may later be
    /// **dropped** back onto an empty square as the captor's own piece (Shogi,
    /// crazyhouse). The hand rides in [`GenericPlacement`](super::position::GenericPlacement)
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

    // --- Orda flag-win / campmate (default OFF) ---------------------------

    /// Returns `true` if this variant ends the game the instant a **king reaches
    /// the far rank** — the Orda "flag" / campmate rule: White wins when its king
    /// reaches the last rank, Black when its king reaches the first rank
    /// (`flagRegionWhite = *8`, `flagRegionBlack = *1` in Fairy-Stockfish).
    ///
    /// The default is `false`; while it is `false` the generic engine never
    /// consults [`opponent_reached_flag`](WideVariant::opponent_reached_flag), so
    /// the move generator produces children for every position exactly as before
    /// and every other variant is byte-identical. When `true`, the **standard**
    /// generator (Orda is on the single-king fast path) short-circuits to *zero
    /// moves* at any node where the side to move has already lost — i.e. the
    /// opponent's king sits on the opponent's goal rank — so a flag win terminates
    /// perft descent move-for-move with FSF (which counts such a node as terminal
    /// with no children). The winning side, on its **own** turn, still has moves:
    /// FSF adjudicates the flag only when the *loser* is to move (`go perft` on
    /// White-king-on-rank-8 / White-to-move keeps generating, but Black-to-move is
    /// terminal). This hook expresses exactly that loser-to-move test.
    fn has_flag_win() -> bool {
        false
    }

    /// Returns `true` if, under the Orda flag rule, the side to move (`turn`) has
    /// **already lost** because its opponent's king has reached the opponent's
    /// goal rank — White's king on the last rank, Black's on the first. The side
    /// to move then has no legal move and the node is terminal.
    ///
    /// Only consulted when [`has_flag_win`](WideVariant::has_flag_win) is `true`;
    /// the default is `false` (never terminal by flag), so every other variant is
    /// byte-identical. The winner, on its own move, is **not** caught here: the
    /// test is purely "the *opponent* is on its goal rank," matching FSF's flag
    /// adjudication, which fires only on the losing side's turn.
    fn opponent_reached_flag(board: &Board<G>, turn: Color) -> bool {
        let _ = board;
        let _ = turn;
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
            placement: GenericPlacement::NONE,
            halfmove_clock: 0,
            fullmove_number: 1,
            consecutive_passes: 0,
        };
        (board, state)
    }
}
