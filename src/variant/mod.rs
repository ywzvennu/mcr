//! The variant abstraction: a single generic [`VariantPosition<V>`] that
//! composes the standard-chess [`Position`] with a per-variant rule layer `V`.
//!
//! # Why composition, not replacement
//!
//! Standard chess is the hot path and already has a fast, pin- and
//! check-mask-aware move generator. Rather than reimplement that per variant, a
//! [`VariantPosition<V>`] *wraps* a [`Position`] and adds a thin set of rule
//! hooks (the [`Variant`] trait). The standard variant [`ChessRules`] overrides
//! nothing, so [`Chess`] = `VariantPosition<ChessRules>` reproduces every
//! [`Position`] behaviour — perft, SAN, UCI, Zobrist — bit for bit.
//!
//! # Why generics, not trait objects or an enum
//!
//! `V` is a zero-sized type and every hook is monomorphized, so the standard
//! path pays no dispatch cost: the compiler can see that `ChessRules` takes the
//! sentinel fast-legality branch and inline it. Trait objects would add a vtable
//! indirection to every hook on the hot path; a single `enum Variant` would
//! force every position to carry the union of all variants' state and would
//! branch at runtime. Monomorphized composition keeps standard chess free and
//! lets each variant carry exactly its own [`Variant::State`].
//!
//! # The fast-legality sentinel (H2)
//!
//! Most variants share standard king safety, so [`Variant::USES_FAST_LEGALITY`]
//! defaults to `true`: [`VariantPosition::legal_moves`] then delegates straight
//! to the core [`Position`]'s fast generator. A variant that needs a different
//! king-safety rule sets it to `false` and overrides
//! [`Variant::is_legal_after`]; legal-move generation then runs the slower
//! pseudo-legal + make-move filter. Either way the same hooks for extra moves,
//! forced-move filtering, and terminal detection apply.

mod antichess;
mod any;
mod atomic;
mod chess;
mod chess960;
mod crazyhouse;
mod horde;
mod koth;
mod racing;
mod three_check;

use core::fmt;
use core::hash::Hash;

pub use antichess::{Antichess, AntichessRules};
pub use any::{AnyVariant, UnknownVariant};
pub use atomic::{Atomic, AtomicRules};
pub use chess::{Chess, ChessRules};
pub use chess960::{Chess960, Chess960Rules};
pub use crazyhouse::{Crazyhouse, CrazyhouseRules, CrazyhouseState};
pub use horde::{Horde, HordeRules};
pub use koth::{KingOfTheHill, KingOfTheHillRules};
pub use racing::{RacingKings, RacingKingsRules};
pub use three_check::{CheckCounters, ThreeCheck, ThreeCheckRules};

use crate::board::Board;
use crate::position::{
    parse_castling_field, parse_clock, parse_ep_field, write_standard_castling_field,
    CastlingRights, FenError, ParseUciError, Position,
};
use crate::{Color, EndReason, Move, Outcome, Role, Square, Zobrist};

/// A stable identifier for a chess variant, used for `Display` and FEN dispatch.
///
/// Only [`VariantId::Standard`] is used by this crate so far; the remaining
/// identifiers are reserved for the variants that will build on this layer, so
/// their numbering and naming are fixed up front.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VariantId {
    /// Standard chess.
    Standard,
    /// Chess960 (Fischer random).
    Chess960,
    /// Atomic chess.
    Atomic,
    /// Antichess (losing chess).
    Antichess,
    /// Crazyhouse.
    Crazyhouse,
    /// King of the Hill.
    KingOfTheHill,
    /// Three-check.
    ThreeCheck,
    /// Racing Kings.
    RacingKings,
    /// Horde.
    Horde,
}

impl VariantId {
    /// The lowercase identifier used in textual contexts.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            VariantId::Standard => "standard",
            VariantId::Chess960 => "chess960",
            VariantId::Atomic => "atomic",
            VariantId::Antichess => "antichess",
            VariantId::Crazyhouse => "crazyhouse",
            VariantId::KingOfTheHill => "kingofthehill",
            VariantId::ThreeCheck => "threecheck",
            VariantId::RacingKings => "racingkings",
            VariantId::Horde => "horde",
        }
    }
}

impl fmt::Display for VariantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The per-variant extra state carried alongside the core [`Position`] — pockets
/// for crazyhouse, check counters for three-check, and so on.
///
/// Standard chess and any variant that needs no extra state use the unit type
/// `()`, which is a zero-sized field.
pub trait VariantState: Clone + Eq + Hash + Default + fmt::Debug {}

impl VariantState for () {}

/// The roles a pawn may promote to in standard chess, in a stable order.
const STANDARD_PROMOTION_ROLES: [Role; 4] = [Role::Knight, Role::Bishop, Role::Rook, Role::Queen];

/// A chess variant: a zero-sized rule layer that customizes the generic
/// [`VariantPosition`] through a small set of hooks, each with a standard-chess
/// default.
///
/// Implementors are zero-sized marker types ([`ChessRules`] is the standard
/// one). Every hook has a provided default equal to standard chess, so a variant
/// overrides only the rules it changes. The hooks fall into a few groups:
///
/// - **Legality / terminal:** [`Variant::USES_FAST_LEGALITY`],
///   [`Variant::is_legal_after`], [`Variant::extra_terminal`],
///   [`Variant::king_is_royal`].
/// - **Move set:** [`Variant::extra_moves`], [`Variant::filter_forced`],
///   [`Variant::promotion_roles`], [`Variant::castling_allowed`],
///   [`Variant::castle_geometry`].
/// - **Make-move side effects:** [`Variant::capture_side_effects`],
///   [`Variant::apply_extra`].
/// - **State / serialization:** [`Variant::starting_board`],
///   [`Variant::hash_state`], [`Variant::fen_extra_read`],
///   [`Variant::fen_extra_write`].
pub trait Variant: Clone + fmt::Debug + PartialEq + Eq + 'static {
    /// The extra per-position state this variant carries (`()` for most).
    type State: VariantState;

    /// The stable identifier of this variant.
    const ID: VariantId;

    /// Whether legal-move generation may use the core [`Position`]'s fast
    /// pin/check-mask generator directly (the sentinel for H2).
    ///
    /// `true` (the default) means king safety is exactly standard, so
    /// [`VariantPosition::legal_moves`] delegates to [`Position::legal_moves`].
    /// A variant that overrides [`Variant::is_legal_after`] with a different
    /// king-safety rule must set this to `false`, switching generation to the
    /// pseudo-legal + make-move filter path.
    const USES_FAST_LEGALITY: bool = true;

    /// A variant-specific terminal condition derivable from a single position,
    /// consulted before the standard checkmate/stalemate/material/clock rules.
    ///
    /// Default: `None` (no extra terminal). Used by king-of-the-hill,
    /// three-check, racing kings, atomic, antichess, and horde.
    #[must_use]
    fn extra_terminal(_core: &Position, _state: &Self::State) -> Option<EndReason> {
        None
    }

    /// King-safety rule (H2): whether `mv`, taking `parent` to `child`, is legal
    /// with respect to king safety.
    ///
    /// The default is standard chess: the move is legal iff it does not leave the
    /// moving side's king attacked. This is only consulted when
    /// [`Variant::USES_FAST_LEGALITY`] is `false`; with the sentinel default the
    /// fast core generator is used instead and this method is never called.
    #[must_use]
    fn is_legal_after(parent: &Position, mv: &Move, child: &Position) -> bool {
        let _ = (mv, child);
        parent.move_keeps_king_safe(mv)
    }

    /// Whether the king is a royal piece, i.e. whether check, checkmate, and
    /// king-capture-avoidance apply (H3).
    ///
    /// Default: `true`. Antichess overrides to `false` (the king is an ordinary
    /// piece with no check concept).
    #[must_use]
    fn king_is_royal() -> bool {
        true
    }

    /// Whether a position with insufficient mating material is an automatic draw.
    ///
    /// Default: `true` (standard chess). Variants whose goal is not checkmate —
    /// racing kings, where a lone king still races to the eighth rank — override
    /// this to `false` so that sparse material never ends the game on its own. The
    /// check is consulted by [`VariantPosition::end_reason`] only when the king is
    /// royal, so non-royal variants are unaffected either way.
    #[must_use]
    fn insufficient_material_is_draw() -> bool {
        true
    }

    /// Whether FEN validation requires exactly one king for *each* side.
    ///
    /// Default: `true` (standard chess: both sides have one king). Horde overrides
    /// this to `false` because white is a kingless pawn horde — black still has a
    /// royal king ([`Variant::king_is_royal`] stays `true`), so this is an
    /// independent knob: it relaxes only the king *count*, not the check rules.
    #[must_use]
    fn requires_two_kings() -> bool {
        true
    }

    /// Applies variant side effects of a capture to the just-produced `core`
    /// position and `state` (H4).
    ///
    /// Called after the core make-move whenever `mv` removed `captured` from the
    /// board. Default: no-op. Used by atomic (explosion) and crazyhouse (pocket
    /// fill).
    fn capture_side_effects(
        _core: &mut Position,
        _state: &mut Self::State,
        _mv: &Move,
        _captured: (crate::Piece, Square),
    ) {
    }

    /// Appends the variant-only pseudo-moves available in this position (H5).
    ///
    /// Default: no-op. Crazyhouse uses this to emit pocket drops. The provided
    /// `core` and `state` describe the current position.
    fn extra_moves(_core: &Position, _state: &Self::State, _out: &mut Vec<Move>) {}

    /// Applies a variant-only move kind (such as [`crate::MoveKind::Drop`]) to
    /// the `core` position and `state` (H6).
    ///
    /// Called only for moves the core make-move cannot itself apply. Default:
    /// unreachable, since the base variant emits no such moves. Crazyhouse
    /// overrides this to apply drops.
    fn apply_extra(_core: &mut Position, _state: &mut Self::State, mv: &Move) {
        unreachable!("variant emits no extra move kinds: {mv:?}");
    }

    /// Updates the variant state after `mv` has been fully applied to `core`,
    /// for every move kind (capture, quiet, or extra) (H14).
    ///
    /// This runs at the very end of [`VariantPosition::play`], once `core` is the
    /// finished child position and any capture / extra-move side effects have
    /// already been applied. Unlike [`Variant::capture_side_effects`] (captures
    /// only) and [`Variant::apply_extra`] (extra move kinds only), this fires on
    /// *every* move, which is what three-check needs to count quiet checking
    /// moves. Default: no-op, so standard chess and every other variant are
    /// unaffected.
    fn post_apply(_core: &mut Position, _state: &mut Self::State, _mv: &Move) {}

    /// Filters the move list down to the forced subset, if the variant forces
    /// certain moves (H7).
    ///
    /// Default: no-op. Antichess uses this to keep only captures when a capture
    /// is available.
    fn filter_forced(_core: &Position, _state: &Self::State, _moves: &mut Vec<Move>) {}

    /// The roles a pawn may promote to, in a stable order (H8).
    ///
    /// Default: knight, bishop, rook, queen. Antichess adds the king. This also
    /// governs which promotion letters a variant-aware UCI/SAN parser accepts.
    #[must_use]
    fn promotion_roles() -> &'static [Role] {
        &STANDARD_PROMOTION_ROLES
    }

    /// Whether castling exists in this variant (H9).
    ///
    /// Default: `true`. Racing kings and antichess override to `false`.
    #[must_use]
    fn castling_allowed() -> bool {
        true
    }

    /// Whether this variant supplies its own castle generation instead of the
    /// core's standard one (the sentinel for [`Variant::generate_castles`]).
    ///
    /// `false` (the default) means the core [`Position`] generator emits the
    /// standard castles. Chess960 sets this `true` (and overrides
    /// [`Variant::generate_castles`] and [`Variant::USES_FAST_LEGALITY`]): the
    /// pseudo-legal pass then omits the standard castles and the variant appends
    /// its arbitrary-geometry castles before the make-move king-safety filter.
    const VARIANT_CASTLING: bool = false;

    /// Appends this variant's castling moves into the pre-filter pseudo-legal set
    /// (only consulted when [`Variant::VARIANT_CASTLING`] is `true`).
    ///
    /// The moves are validated by the same make-move king-safety filter as every
    /// other pseudo-legal move, so a castle that opens a line onto the king's
    /// destination is correctly rejected. Default: no-op.
    fn generate_castles(_core: &Position, _out: &mut Vec<Move>) {}

    /// Appends the variant's full pseudo-legal move set (including the standard
    /// castles) into the pre-filter set, used on the slow path when this variant
    /// supplies the standard castles itself (i.e. [`Variant::VARIANT_CASTLING`]
    /// is `false`) and [`Variant::USES_FAST_LEGALITY`] is `false`.
    ///
    /// Default: the standard pseudo-legal generator
    /// ([`Position::pseudo_into`]), which suits every variant whose piece movement
    /// is standard. Horde overrides this so white's first-rank pawns may
    /// double-push; the override is purely additive (standard pawns are generated
    /// identically) and never touches the fast path.
    fn gen_pseudo(core: &Position, out: &mut Vec<Move>) {
        core.pseudo_into(out);
    }

    /// The castling geometry: for the given side to move and castle side, the
    /// king's destination file and the rook's destination file (H10).
    ///
    /// Default: standard chess (king to the g-/c-file, rook to the f-/d-file).
    /// Chess960 generalizes this. Returning `None` means that castle is not
    /// offered. The default is independent of color.
    #[must_use]
    fn castle_geometry(_color: Color, side: crate::CastleSide) -> Option<CastleGeometry> {
        Some(match side {
            crate::CastleSide::King => CastleGeometry {
                king_dest_file: crate::File::G,
                rook_dest_file: crate::File::F,
            },
            crate::CastleSide::Queen => CastleGeometry {
                king_dest_file: crate::File::C,
                rook_dest_file: crate::File::D,
            },
        })
    }

    /// The starting board, castling rights, and extra state of this variant
    /// (H11).
    ///
    /// Default: the standard chess placement with full castling rights and
    /// default state. Chess960, horde, and crazyhouse override this.
    #[must_use]
    fn starting_board() -> (Board, CastlingRights, Self::State) {
        (
            Board::standard(),
            CastlingRights::STANDARD,
            Self::State::default(),
        )
    }

    /// Folds the variant's extra `state` into a Zobrist accumulator (H12).
    ///
    /// Default: no-op (the unit state contributes nothing). Three-check and
    /// crazyhouse mix their counters / pockets into the key here.
    fn hash_state(_state: &Self::State, _hash: &mut u64) {}

    /// Reads the variant's extra state from the trailing FEN fields after the six
    /// standard ones (H13 read).
    ///
    /// `fields` is the iterator positioned just past the six standard fields.
    /// Default: consume nothing and return the default state. Three-check (a 7th
    /// field) and crazyhouse (pocket in the placement) override this.
    ///
    /// # Errors
    ///
    /// Returns a [`FenError`] if the extra fields are malformed.
    fn fen_extra_read<'a>(
        _fields: &mut impl Iterator<Item = &'a str>,
    ) -> Result<Self::State, FenError> {
        Ok(Self::State::default())
    }

    /// Writes the variant's extra FEN fields after the six standard ones (H13
    /// write).
    ///
    /// Implementations should append a leading space before each field they emit.
    /// Default: write nothing.
    fn fen_extra_write(_state: &Self::State, _out: &mut String) {}

    /// Parses the FEN placement field (the first of the six) into a [`Board`] and
    /// any per-variant state encoded *on the placement itself* (H13b read).
    ///
    /// Default: the standard parser, which reads the board and yields the default
    /// state. Crazyhouse overrides this because its pocket rides on the placement
    /// as a bracketed suffix and its promoted pieces carry a trailing `~`; reading
    /// those there (rather than in [`Variant::fen_extra_read`], which only sees
    /// the *trailing* fields) keeps the core [`Board`] parser unaware of variant
    /// markers. A variant that overrides this typically returns the default state
    /// from [`Variant::fen_extra_read`] (there is nothing left to read), and
    /// overrides [`Variant::write_placement`] symmetrically.
    ///
    /// # Errors
    ///
    /// Returns a [`FenError`] if the placement (or its variant markers) is
    /// malformed.
    fn read_placement(token: &str) -> Result<(Board, Self::State), FenError> {
        let board = Board::from_fen_placement(token).map_err(FenError::Placement)?;
        Ok((board, Self::State::default()))
    }

    /// Writes the FEN placement field, re-emitting any per-variant markers that
    /// [`Variant::read_placement`] consumes (H13b write).
    ///
    /// Default: the standard placement with no extra markers. Crazyhouse overrides
    /// this to append its `[...]` pocket suffix and `~` promotion markers.
    fn write_placement(board: &Board, _state: &Self::State, out: &mut String) {
        out.push_str(&board.to_fen_placement());
    }

    /// Parses the castling-rights FEN field (the third of the six standard
    /// fields) into [`CastlingRights`] (H10 read).
    ///
    /// Default: the standard parser, which accepts `KQkq` (a subset, or `-`)
    /// with the rooks on the a-/h-files. Chess960 overrides this to also accept
    /// Shredder-FEN file letters (`AHah`) and X-FEN `KQkq` interpreted as the
    /// outermost rooks, so its arbitrary rook files round-trip.
    ///
    /// # Errors
    ///
    /// Returns a [`FenError`] if the field is malformed or inconsistent with the
    /// placement.
    fn read_castling_field(field: &str, board: &Board) -> Result<CastlingRights, FenError> {
        parse_castling_field(field, board)
    }

    /// Writes the castling-rights FEN field for the given rights and placement
    /// (H10 write).
    ///
    /// Default: the standard `KQkq` form. Chess960 overrides this to emit
    /// Shredder file letters when a castling rook is off the a-/h-file.
    fn write_castling_field(rights: CastlingRights, _board: &Board, out: &mut String) {
        write_standard_castling_field(rights, out);
    }
}

/// The destination files of a castling move: where the king and rook end up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CastleGeometry {
    /// The file the king lands on.
    pub king_dest_file: crate::File,
    /// The file the rook lands on.
    pub rook_dest_file: crate::File,
}

/// A chess position under the rules of variant `V`: the standard-chess core plus
/// the variant's extra state.
///
/// For standard chess use the [`Chess`] alias. The generic operations
/// ([`VariantPosition::legal_moves`], [`VariantPosition::play`],
/// [`VariantPosition::outcome`], FEN, and [`perft_variant`]) route through the
/// [`Variant`] hooks, all of which default to standard chess.
#[derive(Clone)]
pub struct VariantPosition<V: Variant> {
    core: Position,
    state: V::State,
    variant: V,
}

impl<V: Variant + fmt::Debug> fmt::Debug for VariantPosition<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VariantPosition")
            .field("variant", &V::ID)
            .field("core", &self.core)
            .field("state", &self.state)
            .finish()
    }
}

impl<V: Variant> PartialEq for VariantPosition<V> {
    fn eq(&self, other: &Self) -> bool {
        self.core == other.core && self.state == other.state
    }
}

impl<V: Variant> Eq for VariantPosition<V> {}

impl<V: Variant + Default> Default for VariantPosition<V> {
    fn default() -> Self {
        Self::startpos()
    }
}

impl<V: Variant + Default> VariantPosition<V> {
    /// The starting position of variant `V` (its [`Variant::starting_board`]).
    #[must_use]
    pub fn startpos() -> Self {
        let (board, castling, state) = V::starting_board();
        let core = Position::from_fields(board, Color::White, castling, None, 0, 1);
        let mut pos = VariantPosition {
            core,
            state,
            variant: V::default(),
        };
        pos.fold_state_hash();
        pos
    }
}

impl<V: Variant> VariantPosition<V> {
    /// Wraps an existing core [`Position`] and `state` under variant `V`.
    #[must_use]
    pub fn from_parts(core: Position, state: V::State, variant: V) -> Self {
        let mut pos = VariantPosition {
            core,
            state,
            variant,
        };
        pos.fold_state_hash();
        pos
    }

    /// The standard-chess core of this position.
    #[must_use]
    #[inline]
    pub const fn core(&self) -> &Position {
        &self.core
    }

    /// The variant's extra state.
    #[must_use]
    #[inline]
    pub const fn state(&self) -> &V::State {
        &self.state
    }

    /// The side to move.
    #[must_use]
    #[inline]
    pub const fn turn(&self) -> Color {
        self.core.turn()
    }

    /// The stable identifier of this variant.
    #[must_use]
    #[inline]
    pub const fn variant_id(&self) -> VariantId {
        V::ID
    }

    /// Folds the variant state contribution into the core's incremental hash so
    /// [`VariantPosition::zobrist`] reflects pockets / counters.
    fn fold_state_hash(&mut self) {
        let mut extra = 0u64;
        V::hash_state(&self.state, &mut extra);
        self.core.xor_hash(extra);
    }

    /// The Zobrist key of this position, including the variant state contribution
    /// from [`Variant::hash_state`].
    ///
    /// The core key is XOR-folded with the variant's [`Variant::hash_state`]
    /// contribution so that stateful variants (three-check counters, crazyhouse
    /// pockets) hash apart on otherwise-identical boards. For the unit state the
    /// contribution is zero, so [`Chess`] reproduces the plain core key exactly.
    #[must_use]
    pub fn zobrist(&self) -> Zobrist {
        let mut key = self.core.zobrist().get();
        let mut extra = 0u64;
        V::hash_state(&self.state, &mut extra);
        key ^= extra;
        Zobrist(key)
    }

    /// Whether the side to move is in check. Always `false` when the king is not
    /// royal in this variant ([`Variant::king_is_royal`] is `false`).
    #[must_use]
    pub fn is_check(&self) -> bool {
        V::king_is_royal() && self.core.is_check()
    }

    /// The legal moves of the side to move under variant `V`.
    ///
    /// Pipeline: core pseudo-legal generation, then the king-safety filter (the
    /// fast core path for the sentinel default, otherwise the
    /// [`Variant::is_legal_after`] make-move filter), then the variant's
    /// [`Variant::extra_moves`] and [`Variant::filter_forced`].
    #[must_use]
    pub fn legal_moves(&self) -> Vec<Move> {
        let mut moves = if V::USES_FAST_LEGALITY {
            // Sentinel: standard king safety, so reuse the fast core generator.
            self.core.legal_moves()
        } else {
            let mut pseudo = Vec::with_capacity(64);
            if V::VARIANT_CASTLING {
                // The variant generates castling itself (arbitrary geometry), so
                // suppress the core's standard castles and append the variant's
                // candidates into the same pre-filter set.
                self.core.pseudo_no_castles_into(&mut pseudo);
                V::generate_castles(&self.core, &mut pseudo);
            } else {
                V::gen_pseudo(&self.core, &mut pseudo);
            }
            pseudo.retain(|mv| {
                let child = self.core.play(mv);
                V::is_legal_after(&self.core, mv, &child)
            });
            pseudo
        };

        V::extra_moves(&self.core, &self.state, &mut moves);
        V::filter_forced(&self.core, &self.state, &mut moves);
        moves
    }

    /// Whether `mv` is among this position's legal moves.
    #[must_use]
    pub fn is_legal(&self, mv: &Move) -> bool {
        self.legal_moves().contains(mv)
    }

    /// The number of legal moves.
    #[must_use]
    pub fn legal_move_count(&self) -> usize {
        self.legal_moves().len()
    }

    /// Applies `mv`, returning the successor position.
    ///
    /// The move must be legal. A standard move kind goes through the core
    /// make-move; a variant-only kind (drop) goes through
    /// [`Variant::apply_extra`]. After the core edit, capture side effects
    /// ([`Variant::capture_side_effects`]) and the state hash are folded in.
    #[must_use]
    pub fn play(&self, mv: &Move) -> Self {
        let mut state = self.state.clone();

        // Remove the parent's state hash contribution; the child's is folded in
        // at the end. (For the unit state both are zero.)
        let mut parent_extra = 0u64;
        V::hash_state(&self.state, &mut parent_extra);

        let mut core = if mv.is_drop() {
            let mut core = self.core.clone();
            V::apply_extra(&mut core, &mut state, mv);
            core
        } else {
            let (core, captured) = self.core.play_tracking_capture(mv);
            let mut core = core;
            if let Some((piece, _sq)) = captured {
                let sq = match mv.kind() {
                    crate::MoveKind::EnPassant => {
                        Square::from_file_rank(mv.to().file(), mv.from().rank())
                    }
                    _ => mv.to(),
                };
                V::capture_side_effects(&mut core, &mut state, mv, (piece, sq));
            }
            core
        };

        // Per-move post-apply hook (H14): runs for every move kind once the child
        // `core` is finished, before the state hash is rebalanced. The default is
        // a no-op, so standard chess and other variants are unaffected.
        V::post_apply(&mut core, &mut state, mv);

        // Rebalance the state-hash contribution: out with the parent's, in with
        // the child's.
        core.xor_hash(parent_extra);
        let mut child_extra = 0u64;
        V::hash_state(&state, &mut child_extra);
        core.xor_hash(child_extra);

        VariantPosition {
            core,
            state,
            variant: self.variant.clone(),
        }
    }

    /// The variant-aware game result derivable from this position, or `None`.
    ///
    /// Consults [`Variant::extra_terminal`] first, then the standard
    /// single-position rules (respecting [`Variant::king_is_royal`]: a non-royal
    /// king has no checkmate, so a position with no legal move is a stalemate).
    #[must_use]
    pub fn outcome(&self) -> Option<Outcome> {
        self.end_reason().map(|reason| reason.outcome(self.turn()))
    }

    /// The variant-aware [`EndReason`], or `None` if the game is not over.
    #[must_use]
    pub fn end_reason(&self) -> Option<EndReason> {
        if let Some(reason) = V::extra_terminal(&self.core, &self.state) {
            return Some(reason);
        }
        if self.legal_move_count() == 0 {
            return Some(if V::king_is_royal() && self.core.is_check() {
                EndReason::Checkmate
            } else {
                EndReason::Stalemate
            });
        }
        // Material / clock draws only apply when the standard concepts do; the
        // core check is reused as the standard default.
        if V::king_is_royal()
            && V::insufficient_material_is_draw()
            && self.core.is_insufficient_material()
        {
            return Some(EndReason::InsufficientMaterial);
        }
        if self.core.halfmove_clock() >= SEVENTY_FIVE_MOVE_PLIES {
            return Some(EndReason::SeventyFiveMoveRule);
        }
        None
    }

    /// Renders `mv` as UCI long algebraic notation (drops as `N@f3`).
    #[must_use]
    pub fn to_uci(&self, mv: &Move) -> String {
        mv.to_uci()
    }

    /// Parses a UCI move string against this position, accepting the variant's
    /// promotion roles and the drop form `X@e4`.
    ///
    /// # Errors
    ///
    /// Returns [`ParseUciError`] if the string is malformed or names no legal
    /// move in this position.
    pub fn parse_uci(&self, uci: &str) -> Result<Move, ParseUciError> {
        let bytes = uci.as_bytes();

        // Drop form: `{ROLE}@{square}`.
        if bytes.len() == 4 && bytes[1] == b'@' {
            let role = Role::from_char(bytes[0] as char).ok_or(ParseUciError::Malformed)?;
            let to = uci[2..4]
                .parse::<Square>()
                .map_err(|_| ParseUciError::Malformed)?;
            let want = Move::drop(role, to);
            return if self.is_legal(&want) {
                Ok(want)
            } else {
                Err(ParseUciError::Illegal)
            };
        }

        if bytes.len() != 4 && bytes.len() != 5 {
            return Err(ParseUciError::Malformed);
        }
        let from = uci[0..2]
            .parse::<Square>()
            .map_err(|_| ParseUciError::Malformed)?;
        let to = uci[2..4]
            .parse::<Square>()
            .map_err(|_| ParseUciError::Malformed)?;
        let promo = if bytes.len() == 5 {
            let role = Role::from_char(bytes[4] as char).ok_or(ParseUciError::Malformed)?;
            if role == Role::Pawn || !V::promotion_roles().contains(&role) {
                return Err(ParseUciError::Malformed);
            }
            Some(role)
        } else {
            None
        };

        for mv in self.legal_moves() {
            if mv.from() == from && mv.to() == to && mv.promotion() == promo && !mv.is_drop() {
                return Ok(mv);
            }
        }
        Err(ParseUciError::Illegal)
    }
}

impl<V: Variant + Default> VariantPosition<V> {
    /// Parses a position of variant `V` from FEN: the six standard fields via the
    /// core [`Position`] sub-parsers, then [`Variant::fen_extra_read`] for any
    /// trailing variant fields.
    ///
    /// # Errors
    ///
    /// Returns [`FenError`] if the standard fields are malformed or the position
    /// fails variant validation, or the variant's extra fields are malformed.
    pub fn from_fen(fen: &str) -> Result<Self, FenError> {
        let mut fields = fen.split_whitespace();

        let placement = fields.next().ok_or(FenError::MissingField)?;
        let (board, placement_state) = V::read_placement(placement)?;

        let turn = match fields.next().ok_or(FenError::MissingField)? {
            "w" => Color::White,
            "b" => Color::Black,
            other => return Err(FenError::BadTurn(other.to_owned())),
        };

        let castling_field = fields.next().ok_or(FenError::MissingField)?;
        let castling = V::read_castling_field(castling_field, &board)?;

        let ep_field = fields.next().ok_or(FenError::MissingField)?;
        let ep_square = parse_ep_field(ep_field)?;

        let halfmove_clock = match fields.next() {
            Some(s) => parse_clock(s)?,
            None => 0,
        };
        let fullmove_number = match fields.next() {
            Some(s) => parse_clock(s)?,
            None => 1,
        };

        // A variant encodes its extra state either *on the placement* (read by
        // `read_placement`, e.g. crazyhouse pockets) or in *trailing fields* (read
        // by `fen_extra_read`, e.g. three-check), never both. Whichever source is
        // non-default is the real state; both default to `Default::default()`, so
        // taking the non-default one composes correctly without either variant
        // needing to know about the other.
        let trailing_state = V::fen_extra_read(&mut fields)?;
        let state = if placement_state == V::State::default() {
            trailing_state
        } else {
            placement_state
        };

        if fields.next().is_some() {
            return Err(FenError::TrailingData);
        }

        let core = Position::from_fields(
            board,
            turn,
            castling,
            ep_square,
            halfmove_clock,
            fullmove_number,
        );
        core.validate_core(V::requires_two_kings(), V::king_is_royal())?;

        Ok(Self::from_parts(core, state, V::default()))
    }
}

impl<V: Variant> VariantPosition<V> {
    /// Serializes this position to FEN: the six standard fields, then any variant
    /// extra fields from [`Variant::fen_extra_write`].
    #[must_use]
    pub fn to_fen(&self) -> String {
        let mut castling = String::new();
        V::write_castling_field(
            self.core.castling_rights(),
            self.core.board(),
            &mut castling,
        );
        let mut placement = String::new();
        V::write_placement(self.core.board(), &self.state, &mut placement);
        let mut fen = String::new();
        self.core
            .write_core_fen_with_placement(&placement, &castling, &mut fen);
        V::fen_extra_write(&self.state, &mut fen);
        fen
    }
}

impl<V: Variant + Default> core::str::FromStr for VariantPosition<V> {
    type Err = FenError;

    fn from_str(s: &str) -> Result<Self, FenError> {
        Self::from_fen(s)
    }
}

/// The halfmove-clock value (in plies) at which the seventy-five-move rule ends
/// the game automatically. Kept in step with [`crate::outcome`].
const SEVENTY_FIVE_MOVE_PLIES: u32 = 150;

/// Counts the leaf nodes reachable in exactly `depth` plies from a
/// [`VariantPosition`], the variant-generic analogue of [`crate::perft`].
///
/// For [`Chess`] this returns the same counts as the standard [`crate::perft`].
#[must_use]
pub fn perft_variant<V: Variant>(position: &VariantPosition<V>, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    let moves = position.legal_moves();
    if depth == 1 {
        return moves.len() as u64;
    }
    let mut nodes = 0;
    for mv in moves {
        nodes += perft_variant(&position.play(&mv), depth - 1);
    }
    nodes
}
