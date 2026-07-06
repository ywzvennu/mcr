//! Runtime fairy-variant dispatch: [`AnyWideVariant`], a type-erased enum wrapper
//! over the geometry layer's monomorphized [`GenericPosition<G, V>`](super::GenericPosition).
//!
//! Each shipped fairy variant is a distinct compile-time type
//! [`GenericPosition<G, V>`](super::GenericPosition): `G` is its board geometry
//! (Chess8x8, Shogi9x9, Xiangqi9x10, …) and `V` a zero-sized rule layer, so every
//! hook is monomorphized — exactly what a consumer that knows its variant at
//! compile time wants. But a CLI, a binding, or a server often picks the variant
//! from a string at runtime and cannot name `G`/`V`, and — unlike the concrete
//! 8x8 engine — the geometries differ, so a single generic position type cannot
//! span them.
//!
//! [`AnyWideVariant`] closes that gap: a plain enum with one arm per shipped fairy
//! variant, each wrapping that variant's concrete position alias, plus a runtime
//! dispatch (a single `match`) for the common surface. It adds no state beyond the
//! wrapped position and introduces no `unsafe` or trait-object indirection; the
//! per-variant fast paths inside [`GenericPosition`](super::GenericPosition) are
//! untouched. The companion [`WideVariantId`] is the string-addressable selector
//! ([`FromStr`] from a canonical name or a common alias).

use alloc::{boxed::Box, string::String, vec::Vec};
use core::str::FromStr;

use super::{
    perft, Aiwok, Alice, Almost, Amazon, Asean, Berolina, Bughouse, Cambodian, CannonShogi,
    Capablanca, Capahouse, Caparandom, Centaur, Chak, Chancellor, Chaturanga, CheckShogi, Chennis,
    Chigorin, Chu, Coregal, Courier, Dai, Dobutsu, Dragon, Duck, Embassy, Empire, EuroShogi,
    Extinction, FogOfWar, GameStatus, GenericPosition, Geometry, Georgian, Gorogoro, Gothic, Grand,
    Grandhouse, Grasshopper, HoppelPoppel, Janggi, Janus, Jieqi, Judkins, Karouk, Khans, Kinglet,
    Knightmate, Kyotoshogi, Legan, Losalamos, Makpong, Makruk, Manchu, Mansindam, Micro, Minishogi,
    Minixiangqi, Modern, Newzealand, Nightrider, Nocastle, Opulent, Orda, Ordamirror, Pawnback,
    Pawnsideways, Perfect, Petrified, Placement, Pocketknight, Seirawan, Shako, Shatar, Shatranj,
    Shinobi, ShoShogi, Shogi, Shogun, Shouse, Sittuyin, Spartan, Square, Synochess, Tencubed,
    Tenjiku, Threekings, Tori, Torpedo, Washogi, WideEndReason, WideFenError, WideMove,
    WideMoveList, WideOutcome, WideVariant, Xiangfu, Xiangqi,
};
use crate::Color;

/// The error returned by [`WideVariantId::from_str`] when a name is not a
/// recognized fairy variant or alias.
///
/// The wrapped [`String`] is the offending input, lowercased and trimmed exactly
/// as it was matched, so callers can echo it back in a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownWideVariant(pub String);

impl core::fmt::Display for UnknownWideVariant {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "unknown wide variant: {:?}", self.0)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for UnknownWideVariant {}

/// Renders `mv` as UCI long algebraic notation for the geometry of `pos`.
///
/// [`WideMove::to_uci`] needs the board geometry as a type argument; this helper
/// recovers it from the position type so the runtime [`AnyWideVariant`] dispatch
/// can forward without naming `G`.
fn move_to_uci<G: Geometry, V: WideVariant<G>>(
    _pos: &GenericPosition<G, V>,
    mv: &WideMove,
) -> String {
    mv.to_uci::<G>()
}

/// Parses a UCI move string against `pos` by matching it against the position's
/// own legal-move renderings, returning the matching [`WideMove`] or `None`.
///
/// This is the geometry-aware counterpart of [`move_to_uci`]: there is no
/// `parse_uci` on [`GenericPosition`](super::GenericPosition), so a UCI string is
/// resolved against the legal moves it could name — guaranteeing the result is
/// both legal and renders back to the same string.
fn find_uci<G: Geometry, V: WideVariant<G>>(
    pos: &GenericPosition<G, V>,
    uci: &str,
) -> Option<WideMove> {
    pos.legal_moves()
        .into_iter()
        .find(|m| m.to_uci::<G>() == uci)
}

/// Resolves a geometry-agnostic square index against the geometry of `pos`,
/// yielding `None` when `index` names no square on that board.
///
/// The type-erased [`AnyWideVariant`] analysis queries take a bare `u8` index
/// (`0..G::SQUARES`, the little-endian `rank * width + file` numbering) rather
/// than a geometry-parameterized [`Square<G>`](super::Square); this helper
/// recovers `G` from the position type — as [`move_to_uci`] does — and validates
/// the index against that board so an out-of-range value is handled instead of
/// panicking.
fn square_of<G: Geometry, V: WideVariant<G>>(
    _pos: &GenericPosition<G, V>,
    index: u8,
) -> Option<Square<G>> {
    Square::try_new(index)
}

/// Collects the set squares of a [`Bitboard<G>`](super::Bitboard) as their bare
/// `u8` indices, ascending — the type-erased form the [`AnyWideVariant`] analysis
/// queries return in place of a geometry-parameterized bitboard.
fn square_indices<G: Geometry>(squares: impl IntoIterator<Item = Square<G>>) -> Vec<u8> {
    squares.into_iter().map(|sq| sq.index()).collect()
}

/// Generates the [`WideVariantId`] selector and the [`AnyWideVariant`] runtime
/// wrapper from a single table of
/// `Variant, ConcreteAlias, StorageType, "canonical" [, "alias"…]` rows, keeping
/// the two enums and every forwarding `match` exhaustively in sync.
///
/// `ConcreteAlias` is the variant's concrete [`GenericPosition`](super::GenericPosition)
/// type; `StorageType` is how the [`AnyWideVariant`] arm stores it — normally the
/// same concrete type (stored inline), but a heap-`Box` of it for the few large
/// arms (see the table's comment) so that the enum is not sized by its widest
/// arm. Storage is only ever the concrete type or a `Box<ConcreteType>`, both of
/// which are `From<ConcreteType>` (identity / `Box::new`) and `Deref` to the
/// position, so the wrapping and forwarding below stay uniform.
macro_rules! wide_variants {
    ( $( $variant:ident, $pos:ty, $store:ty, $name:literal $( , $alt:literal )* ; )+ ) => {
        /// A stable, string-addressable identifier for a shipped fairy variant.
        ///
        /// Parse one from a name or alias with [`FromStr`] (case-insensitive,
        /// surrounding whitespace ignored), render it back with [`as_str`] /
        /// [`Display`], and enumerate them all with [`WideVariantId::ALL`]. Feed
        /// it to [`AnyWideVariant::startpos`] / [`AnyWideVariant::from_fen`] to
        /// build a runtime position.
        ///
        /// [`as_str`]: WideVariantId::as_str
        /// [`Display`]: core::fmt::Display
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub enum WideVariantId {
            $(
                #[doc = concat!("The `", $name, "` variant.")]
                $variant,
            )+
        }

        impl WideVariantId {
            /// Every shipped fairy-variant identifier, in declaration order. The
            /// canonical entry point for bindings, a CLI variant list, or docs.
            pub const ALL: &'static [WideVariantId] = &[ $( WideVariantId::$variant ),+ ];

            /// The canonical lowercase name, the inverse of the canonical-name
            /// branch of [`FromStr`]: `id.as_str().parse() == Ok(id)`.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $( WideVariantId::$variant => $name, )+
                }
            }

            /// The `size_of` and `align_of`, in bytes, of this variant's concrete
            /// [`GenericPosition`](super::GenericPosition) — the inline size the
            /// matching [`AnyWideVariant`] arm holds (before the few widest arms
            /// are boxed; see the enum's storage note). A static per-variant fact,
            /// independent of any live position, so the memory-footprint benches
            /// can report it across the whole registry without constructing one.
            #[must_use]
            pub const fn position_footprint(self) -> (usize, usize) {
                match self {
                    $( WideVariantId::$variant =>
                        (core::mem::size_of::<$pos>(), core::mem::align_of::<$pos>()), )+
                }
            }

            /// The bit width of this variant's bitboard backing integer — `64`,
            /// `128`, or `256`, the geometry's
            /// [`Bits::BITS`](super::BitboardBacking::BITS). A static per-variant
            /// fact that buckets variants by storage backing (a small board on a
            /// `u64`, a large one on a `u128`, the widest on the two-limb `U256`)
            /// without naming the geometry — the axis the per-backing
            /// position-size regression ceiling groups by. Pairs with
            /// [`position_footprint`](Self::position_footprint).
            #[must_use]
            pub const fn position_backing_bits(self) -> u32 {
                match self {
                    $( WideVariantId::$variant => <$pos>::BACKING_BITS, )+
                }
            }

            /// This variant's [`WideVariant::ROLE_SPAN`](super::WideVariant::ROLE_SPAN)
            /// — the number of leading [`WideRole`](super::role::WideRole)s its
            /// movegen loops iterate. A static per-variant fact used by the
            /// `role_span_covers_all_fieldable_roles` meta-test.
            #[cfg(test)]
            #[must_use]
            pub(crate) const fn role_span(self) -> usize {
                match self {
                    $( WideVariantId::$variant => <$pos>::ROLE_SPAN, )+
                }
            }

            /// Which of this variant's history-independent **draw / adjudication
            /// hooks** are overridden away from their [`WideVariant`](super::WideVariant)
            /// defaults, computed by calling the concrete rules' hook methods (see
            /// [`GenericPosition::overridden_draw_hooks`](super::GenericPosition::overridden_draw_hooks)).
            ///
            /// A static per-variant fact, independent of any live position, so the
            /// coverage-gate meta-test can introspect the special terminal rules the
            /// whole registry carries without constructing one.
            #[must_use]
            pub fn draw_hooks(self) -> super::variant::DrawHooks {
                match self {
                    $( WideVariantId::$variant => <$pos>::overridden_draw_hooks(), )+
                }
            }
        }

        impl core::fmt::Display for WideVariantId {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for WideVariantId {
            type Err = UnknownWideVariant;

            /// Parses a [`WideVariantId`] from its canonical name or a common
            /// alias. Matching is case-insensitive and ignores surrounding
            /// whitespace.
            ///
            /// # Errors
            ///
            /// Returns [`UnknownWideVariant`] (carrying the normalized input) when
            /// the name matches no variant.
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let normalized = s.trim().to_ascii_lowercase();
                let id = match normalized.as_str() {
                    $( $name $( | $alt )* => WideVariantId::$variant, )+
                    _ => return Err(UnknownWideVariant(normalized)),
                };
                Ok(id)
            }
        }

        /// A fairy-chess position whose variant is chosen at runtime: a thin enum
        /// wrapper with one arm per shipped variant, each holding that variant's
        /// concrete [`GenericPosition<G, V>`](super::GenericPosition) alias.
        ///
        /// Construct one with [`AnyWideVariant::startpos`] or
        /// [`AnyWideVariant::from_fen`] from a [`WideVariantId`] (which a string
        /// yields via [`WideVariantId::from_str`]), then use the uniform surface —
        /// [`legal_moves`](AnyWideVariant::legal_moves),
        /// [`play`](AnyWideVariant::play), [`perft`](AnyWideVariant::perft),
        /// [`to_fen`](AnyWideVariant::to_fen),
        /// [`outcome`](AnyWideVariant::outcome), … — without naming the variant's
        /// geometry or rule type. Every method forwards through a single `match`
        /// to the inner generic position, so it is exactly as correct as the
        /// underlying [`GenericPosition<G, V>`](super::GenericPosition) and pays
        /// only one branch.
        // The arms wrap positions of differing board geometries (`u64` vs `u128`
        // vs the two-limb `U256` backing, 3x4 up to 12x12) so their sizes
        // genuinely differ. The common u64/u128 arms are stored inline — no
        // allocation, no indirection — while the single U256 arm (12x12 Chu, at
        // ~4512 B nearly 2x a u128 position) is boxed via its `StorageType` in the
        // table, so the enum is sized by a u128 arm (~2400 B) instead of by Chu.
        // Only Chu pays a heap indirection, and it is both rare and already the
        // heaviest variant to compute.
        #[derive(Clone, Debug)]
        pub enum AnyWideVariant {
            $(
                #[doc = concat!("The `", $name, "` variant.")]
                $variant($store),
            )+
        }

        impl AnyWideVariant {
            /// The starting position of the variant named by `id`.
            #[must_use]
            pub fn startpos(id: WideVariantId) -> Self {
                match id {
                    $( WideVariantId::$variant => AnyWideVariant::$variant(<$store>::from(<$pos>::startpos())), )+
                }
            }

            /// Parses a position of the variant named by `id` from `fen`.
            ///
            /// # Errors
            ///
            /// Returns [`WideFenError`] if `fen` is malformed or fails the
            /// variant's validation (the same errors
            /// [`GenericPosition::from_fen`](super::GenericPosition::from_fen)
            /// reports).
            pub fn from_fen(id: WideVariantId, fen: &str) -> Result<Self, WideFenError> {
                let pos = match id {
                    $( WideVariantId::$variant => AnyWideVariant::$variant(<$store>::from(<$pos>::from_fen(fen)?)), )+
                };
                Ok(pos)
            }

            /// The identifier of the wrapped variant.
            #[must_use]
            pub fn variant_id(&self) -> WideVariantId {
                match self {
                    $( AnyWideVariant::$variant(_) => WideVariantId::$variant, )+
                }
            }

            /// The `size_of` / `align_of` of the wrapped variant's concrete
            /// [`GenericPosition`](super::GenericPosition), the forward of
            /// [`WideVariantId::position_footprint`] for [`variant_id`](Self::variant_id).
            #[must_use]
            pub fn position_footprint(&self) -> (usize, usize) {
                self.variant_id().position_footprint()
            }

            /// The wrapped variant's board dimensions `(width, height)` in
            /// `(files, ranks)`, read from its [`Geometry`] — e.g. `(8, 8)` for
            /// the 8x8 fairy variants, `(9, 10)` for Xiangqi, `(12, 8)` for
            /// Courier. Fixed by the variant, independent of the position.
            #[must_use]
            pub fn dimensions(&self) -> (u8, u8) {
                match self {
                    $( AnyWideVariant::$variant(p) => p.dimensions(), )+
                }
            }

            /// The side to move.
            #[must_use]
            pub fn turn(&self) -> Color {
                match self {
                    $( AnyWideVariant::$variant(p) => p.turn(), )+
                }
            }

            /// The 1-based fullmove number (incremented after each Black move).
            #[must_use]
            pub fn fullmove_number(&self) -> u16 {
                match self {
                    $( AnyWideVariant::$variant(p) => p.fullmove_number(), )+
                }
            }

            /// The legal moves of the side to move under the wrapped variant.
            #[must_use]
            pub fn legal_moves(&self) -> Vec<WideMove> {
                match self {
                    $( AnyWideVariant::$variant(p) => p.legal_moves(), )+
                }
            }

            /// The highest [`WideRole`](super::role::WideRole) index this position
            /// currently fields (board or hand). Forwards to
            /// [`GenericPosition::max_fielded_role_index`](super::GenericPosition::max_fielded_role_index);
            /// used only by the `role_span_covers_all_fieldable_roles` meta-test.
            #[cfg(test)]
            #[must_use]
            pub(crate) fn max_fielded_role_index(&self) -> usize {
                match self {
                    $( AnyWideVariant::$variant(p) => p.max_fielded_role_index(), )+
                }
            }

            /// Fills `out` with the legal moves of the side to move under the
            /// wrapped variant, reusing the caller's buffer instead of allocating
            /// a fresh `Vec` per call — the allocation-free analogue of
            /// [`legal_moves`](Self::legal_moves). A consumer that lists moves for
            /// many positions keeps one [`WideMoveList`] and hands it back each
            /// call; the common (non-spilling) path allocates nothing. The move
            /// set is byte-identical to [`legal_moves`](Self::legal_moves).
            pub fn legal_moves_into(&self, out: &mut WideMoveList) {
                match self {
                    $( AnyWideVariant::$variant(p) => p.legal_moves_into(out), )+
                }
            }

            /// A stable 64-bit Zobrist key identifying this position — the same
            /// value as
            /// [`GenericPosition::zobrist`](super::GenericPosition::zobrist) for
            /// the wrapped variant. Suitable for opening-book lookup (see
            /// [`WideBook`](super::WideBook)), repetition detection, and position
            /// deduplication, without naming the variant's geometry / rule types.
            #[must_use]
            pub fn position_key(&self) -> u64 {
                match self {
                    $( AnyWideVariant::$variant(p) => p.zobrist(), )+
                }
            }

            /// Applies `mv`, returning the successor position in the same variant
            /// arm. The move must be legal (as for
            /// [`GenericPosition::play`](super::GenericPosition::play)).
            #[must_use]
            pub fn play(&self, mv: &WideMove) -> Self {
                match self {
                    $( AnyWideVariant::$variant(p) => AnyWideVariant::$variant(<$store>::from(p.play(mv))), )+
                }
            }

            /// Counts the leaf nodes reachable in exactly `depth` plies, forwarding
            /// to the geometry layer's [`perft`](super::perft).
            #[must_use]
            pub fn perft(&self, depth: u32) -> u64 {
                match self {
                    $( AnyWideVariant::$variant(p) => perft(p, depth), )+
                }
            }

            /// Test support (issue #309): walks the legal-move tree to `depth`,
            /// asserting `apply_with_undo` then `undo` restores every node
            /// byte-for-byte and matches `play`.
            #[cfg(test)]
            pub(crate) fn assert_make_unmake_walk(&self, depth: u32) {
                match self {
                    $( AnyWideVariant::$variant(p) => <$pos as Clone>::clone(p).assert_make_unmake_walk(depth), )+
                }
            }

            /// Test support (issue #438): walks a seeded random line of `plies`
            /// legal moves, asserting make/unmake byte-identity (and key identity)
            /// at every node — the deep-line counterpart of
            /// [`assert_make_unmake_walk`](Self::assert_make_unmake_walk).
            #[cfg(test)]
            pub(crate) fn assert_make_unmake_line(&self, seed: u64, plies: u32) {
                match self {
                    $( AnyWideVariant::$variant(p) => <$pos as Clone>::clone(p).assert_make_unmake_line(seed, plies), )+
                }
            }

            /// The variant-aware game result, or `None` if the game is not over.
            #[must_use]
            pub fn outcome(&self) -> Option<WideOutcome> {
                match self {
                    $( AnyWideVariant::$variant(p) => p.outcome(), )+
                }
            }

            /// The variant-aware [`WideEndReason`], or `None` if the game is not
            /// over.
            #[must_use]
            pub fn end_reason(&self) -> Option<WideEndReason> {
                match self {
                    $( AnyWideVariant::$variant(p) => p.end_reason(), )+
                }
            }

            /// Whether the side to move is in check (always `false` where the king
            /// is not royal).
            #[must_use]
            pub fn is_check(&self) -> bool {
                match self {
                    $( AnyWideVariant::$variant(p) => p.is_check(), )+
                }
            }

            /// The consolidated [`GameStatus`] of this position — the
            /// single-`match` forward of
            /// [`GenericPosition::status`](super::GenericPosition::status), which
            /// folds the wrapped variant's `end_reason` / `outcome` into one total
            /// enum (ongoing, checkmate, stalemate, a variant win, or a draw).
            ///
            /// This covers the single-position rules; the history-dependent rules
            /// (repetition, sennichite, perpetual check / chase, bikjang, counting)
            /// need a game wrapper and are out of scope for a bare position.
            #[must_use]
            pub fn status(&self) -> GameStatus {
                match self {
                    $( AnyWideVariant::$variant(p) => p.status(), )+
                }
            }

            /// Whether `square` is attacked by a piece of `by_color`, under the
            /// live board occupancy — the type-erased forward of
            /// [`GenericPosition::is_attacked`](super::GenericPosition::is_attacked).
            ///
            /// `square` is a bare index (`0..width * height`, the little-endian
            /// `rank * width + file` numbering); an index off the wrapped variant's
            /// board yields `false`.
            #[must_use]
            pub fn is_attacked(&self, square: u8, by_color: Color) -> bool {
                match self {
                    $( AnyWideVariant::$variant(p) => {
                        square_of(p, square).is_some_and(|sq| p.is_attacked(sq, by_color))
                    } )+
                }
            }

            /// The `side` pieces that attack `square`, as their bare square indices
            /// (ascending) — the type-erased forward of
            /// [`GenericPosition::attackers_of`](super::GenericPosition::attackers_of).
            ///
            /// `square` is a bare index (see [`is_attacked`](Self::is_attacked));
            /// an off-board index yields an empty list. Because a geometry-
            /// parameterized [`Bitboard<G>`](super::Bitboard) cannot be named at the
            /// type-erased level, the attacker *set* is returned as an index list;
            /// [`attacker_count`](Self::attacker_count) gives just its size.
            #[must_use]
            pub fn attackers_of(&self, square: u8, side: Color) -> Vec<u8> {
                match self {
                    $( AnyWideVariant::$variant(p) => match square_of(p, square) {
                        Some(sq) => square_indices(p.attackers_of(sq, side)),
                        None => Vec::new(),
                    }, )+
                }
            }

            /// The number of `side` pieces that attack `square` — the population
            /// count of [`attackers_of`](Self::attackers_of), forwarding
            /// [`GenericPosition::attackers_of`](super::GenericPosition::attackers_of)
            /// without materializing the index list. An off-board index yields `0`.
            #[must_use]
            pub fn attacker_count(&self, square: u8, side: Color) -> u32 {
                match self {
                    $( AnyWideVariant::$variant(p) => {
                        square_of(p, square).map_or(0, |sq| p.attackers_of(sq, side).count())
                    } )+
                }
            }

            /// The attack (threat) set of the piece standing on `square`, as its
            /// bare square indices (ascending), or `None` if `square` is empty or
            /// off the board — the type-erased forward of
            /// [`GenericPosition::piece_attacks`](super::GenericPosition::piece_attacks).
            ///
            /// The geometry-parameterized bitboard is erased to an index list, as
            /// for [`attackers_of`](Self::attackers_of);
            /// [`piece_mobility`](Self::piece_mobility) gives just its size.
            #[must_use]
            pub fn piece_attacks(&self, square: u8) -> Option<Vec<u8>> {
                match self {
                    $( AnyWideVariant::$variant(p) => {
                        p.piece_attacks(square_of(p, square)?).map(square_indices)
                    } )+
                }
            }

            /// The number of squares the piece on `square` attacks (its mobility),
            /// or `0` if `square` is empty or off the board — the type-erased
            /// forward of
            /// [`GenericPosition::piece_mobility`](super::GenericPosition::piece_mobility).
            #[must_use]
            pub fn piece_mobility(&self, square: u8) -> u32 {
                match self {
                    $( AnyWideVariant::$variant(p) => {
                        square_of(p, square).map_or(0, |sq| p.piece_mobility(sq))
                    } )+
                }
            }

            /// Every square attacked by at least one piece of `side`, as bare
            /// square indices (ascending) — the type-erased forward of
            /// [`GenericPosition::attack_map`](super::GenericPosition::attack_map).
            ///
            /// The geometry-parameterized bitboard is erased to an index list;
            /// [`attack_count`](Self::attack_count) gives just its size.
            #[must_use]
            pub fn attack_map(&self, side: Color) -> Vec<u8> {
                match self {
                    $( AnyWideVariant::$variant(p) => square_indices(p.attack_map(side)), )+
                }
            }

            /// The squares of `side`'s own pieces that `side` also attacks (its
            /// defended pieces), as bare square indices (ascending) — the
            /// type-erased forward of
            /// [`GenericPosition::defense_map`](super::GenericPosition::defense_map).
            #[must_use]
            pub fn defense_map(&self, side: Color) -> Vec<u8> {
                match self {
                    $( AnyWideVariant::$variant(p) => square_indices(p.defense_map(side)), )+
                }
            }

            /// The number of distinct squares `side` attacks — the type-erased
            /// forward of
            /// [`GenericPosition::attack_count`](super::GenericPosition::attack_count),
            /// equal to the length of [`attack_map`](Self::attack_map).
            #[must_use]
            pub fn attack_count(&self, side: Color) -> u32 {
                match self {
                    $( AnyWideVariant::$variant(p) => p.attack_count(side), )+
                }
            }

            /// Whether `color`'s king(s) are in check right now, regardless of
            /// whose turn it is — the type-erased forward of
            /// [`GenericPosition::is_in_check`](super::GenericPosition::is_in_check).
            /// `is_in_check(turn)` equals [`is_check`](Self::is_check).
            #[must_use]
            pub fn is_in_check(&self, color: Color) -> bool {
                match self {
                    $( AnyWideVariant::$variant(p) => p.is_in_check(color), )+
                }
            }

            /// The royal (king) squares of `color`, as bare square indices
            /// (ascending) — the type-erased forward of
            /// [`GenericPosition::royal_squares`](super::GenericPosition::royal_squares).
            /// Empty for a side whose king is non-royal (Duck, Dobutsu).
            #[must_use]
            pub fn royal_squares(&self, color: Color) -> Vec<u8> {
                match self {
                    $( AnyWideVariant::$variant(p) => square_indices(p.royal_squares(color)), )+
                }
            }

            /// The enemy pieces that attack a royal square of `color`, as bare
            /// square indices (ascending) — the type-erased forward of
            /// [`GenericPosition::checkers_of`](super::GenericPosition::checkers_of).
            /// Excludes the royal-only flying-general confrontation; see
            /// [`is_in_check`](Self::is_in_check) for the full verdict.
            #[must_use]
            pub fn checkers_of(&self, color: Color) -> Vec<u8> {
                match self {
                    $( AnyWideVariant::$variant(p) => square_indices(p.checkers_of(color)), )+
                }
            }

            /// The checkers of the side to move, as bare square indices
            /// (ascending) — [`checkers_of`](Self::checkers_of) for
            /// [`turn`](Self::turn), the type-erased forward of
            /// [`GenericPosition::checkers`](super::GenericPosition::checkers).
            #[must_use]
            pub fn checkers(&self) -> Vec<u8> {
                match self {
                    $( AnyWideVariant::$variant(p) => square_indices(p.checkers()), )+
                }
            }

            /// The absolutely pinned pieces of `color`, as bare square indices
            /// (ascending) — the type-erased forward of
            /// [`GenericPosition::pinned_pieces`](super::GenericPosition::pinned_pieces).
            #[must_use]
            pub fn pinned_pieces(&self, color: Color) -> Vec<u8> {
                match self {
                    $( AnyWideVariant::$variant(p) => square_indices(p.pinned_pieces(color)), )+
                }
            }

            /// The line a pinned piece of `color` on `square` is confined to, as
            /// bare square indices (ascending), or `None` if `square` holds no
            /// absolutely pinned piece of `color` (or is off the board) — the
            /// type-erased forward of
            /// [`GenericPosition::pin_ray_of`](super::GenericPosition::pin_ray_of).
            #[must_use]
            pub fn pin_ray_of(&self, color: Color, square: u8) -> Option<Vec<u8>> {
                match self {
                    $( AnyWideVariant::$variant(p) => {
                        p.pin_ray_of(color, square_of(p, square)?).map(square_indices)
                    } )+
                }
            }

            /// The legal moves of the side to move whose origin is `square` — the
            /// type-erased forward of
            /// [`GenericPosition::legal_moves_from`](super::GenericPosition::legal_moves_from).
            /// An off-board index yields an empty list. A drop is grouped under the
            /// square it drops onto (its packed origin equals its target).
            #[must_use]
            pub fn legal_moves_from(&self, square: u8) -> Vec<WideMove> {
                match self {
                    $( AnyWideVariant::$variant(p) => match square_of(p, square) {
                        Some(sq) => p.legal_moves_from(sq),
                        None => Vec::new(),
                    }, )+
                }
            }

            /// Serializes this position to FEN.
            #[must_use]
            pub fn to_fen(&self) -> String {
                match self {
                    $( AnyWideVariant::$variant(p) => p.to_fen(), )+
                }
            }

            /// Renders `mv` as UCI long algebraic notation for this variant's
            /// geometry.
            #[must_use]
            pub fn to_uci(&self, mv: &WideMove) -> String {
                match self {
                    $( AnyWideVariant::$variant(p) => move_to_uci(p, mv), )+
                }
            }

            /// Resolves a UCI move string to a legal [`WideMove`] in this position,
            /// or `None` if it names no legal move.
            #[must_use]
            pub fn parse_uci(&self, uci: &str) -> Option<WideMove> {
                match self {
                    $( AnyWideVariant::$variant(p) => find_uci(p, uci), )+
                }
            }

            /// Parses and applies a UCI move string, returning the successor
            /// position, or `None` if the string names no legal move.
            #[must_use]
            pub fn play_uci(&self, uci: &str) -> Option<Self> {
                let mv = self.parse_uci(uci)?;
                Some(self.play(&mv))
            }

            /// Renders the legal move `mv` as SAN for this variant's geometry.
            #[must_use]
            pub fn san(&self, mv: &WideMove) -> String {
                match self {
                    $( AnyWideVariant::$variant(p) => p.san(mv), )+
                }
            }

            /// Resolves a SAN move string to a legal [`WideMove`] in this
            /// position, or `None` if it names no (or an ambiguous) legal move.
            #[must_use]
            pub fn parse_san(&self, san: &str) -> Option<WideMove> {
                match self {
                    $( AnyWideVariant::$variant(p) => p.parse_san(san).ok(), )+
                }
            }

            /// Encodes this position to the compact, self-describing binary wire
            /// format: a tag byte, the 1-byte [`WideVariantId`] selector, then the
            /// variant's compact [`GenericPosition`](super::GenericPosition) body.
            /// Smaller than the FEN for every variant; the inverse is
            /// [`from_bytes`](Self::from_bytes). See [`super::binary`].
            #[must_use]
            pub fn to_bytes(&self) -> Vec<u8> {
                let mut out = Vec::new();
                out.push(super::binary::TAG_ANY_POSITION);
                out.push(self.variant_id().to_index());
                match self {
                    $( AnyWideVariant::$variant(p) => p.encode_body(&mut out), )+
                }
                out
            }

            /// Decodes a position previously produced by [`to_bytes`](Self::to_bytes),
            /// dispatching on the embedded [`WideVariantId`] to the matching variant
            /// arm.
            ///
            /// # Errors
            ///
            /// Returns [`super::binary::WireError`] if `bytes` is truncated, carries
            /// the wrong tag, names an unknown variant, or holds an out-of-range
            /// square / role — without panicking on any input.
            pub fn from_bytes(bytes: &[u8]) -> Result<Self, super::binary::WireError> {
                use super::binary::WireError;
                let (&tag, rest) = bytes.split_first().ok_or(WireError::Truncated)?;
                if tag != super::binary::TAG_ANY_POSITION {
                    return Err(WireError::BadTag(tag));
                }
                let (&vid, rest) = rest.split_first().ok_or(WireError::Truncated)?;
                let id = WideVariantId::from_index(vid).ok_or(WireError::UnknownVariant(vid))?;
                match id {
                    $( WideVariantId::$variant =>
                        Ok(AnyWideVariant::$variant(<$store>::from(<$pos>::decode_body(rest)?))), )+
                }
            }
        }
    };
}

wide_variants! {
    Aiwok, Aiwok, Aiwok, "ai-wok", "aiwok";
    Alice, Alice, Alice, "alice";
    Almost, Almost, Almost, "almost", "almostchess";
    Amazon, Amazon, Amazon, "amazon", "amazonchess";
    Asean, Asean, Asean, "asean";
    Berolina, Berolina, Berolina, "berolina";
    Bughouse, Bughouse, Bughouse, "bughouse", "bug";
    Cambodian, Cambodian, Cambodian, "cambodian", "ouk", "kambodja";
    CannonShogi, CannonShogi, CannonShogi, "cannonshogi", "cannon-shogi";
    Capablanca, Capablanca, Capablanca, "capablanca", "capa";
    Capahouse, Capahouse, Capahouse, "capahouse";
    Caparandom, Caparandom, Caparandom, "caparandom", "caparandomchess", "capa960";
    Centaur, Centaur, Centaur, "centaur";
    Chak, Chak, Chak, "chak";
    Chancellor, Chancellor, Chancellor, "chancellor";
    Chaturanga, Chaturanga, Chaturanga, "chaturanga";
    CheckShogi, CheckShogi, CheckShogi, "checkshogi", "check-shogi";
    Chennis, Chennis, Chennis, "chennis";
    Chigorin, Chigorin, Chigorin, "chigorin";
    Chu, Chu, Box<Chu>, "chu", "chushogi", "chu-shogi";
    Coregal, Coregal, Coregal, "coregal";
    Courier, Courier, Courier, "courier";
    Dai, Dai, Box<Dai>, "dai", "daishogi", "dai-shogi";
    Dobutsu, Dobutsu, Dobutsu, "dobutsu";
    Dragon, Dragon, Dragon, "dragon";
    Duck, Duck, Duck, "duck";
    Embassy, Embassy, Embassy, "embassy";
    Empire, Empire, Empire, "empire";
    EuroShogi, EuroShogi, EuroShogi, "euroshogi", "euro-shogi";
    Extinction, Extinction, Extinction, "extinction";
    FogOfWar, FogOfWar, FogOfWar, "fogofwar", "fog", "dark";
    Georgian, Georgian, Georgian, "georgian";
    Gorogoro, Gorogoro, Gorogoro, "gorogoro", "gorogoroplus";
    Gothic, Gothic, Gothic, "gothic";
    Grand, Grand, Grand, "grand";
    Grandhouse, Grandhouse, Grandhouse, "grandhouse";
    Grasshopper, Grasshopper, Grasshopper, "grasshopper";
    HoppelPoppel, HoppelPoppel, HoppelPoppel, "hoppelpoppel", "hoppel-poppel";
    Janggi, Janggi, Janggi, "janggi", "korean";
    Janus, Janus, Janus, "janus", "januschess";
    Jieqi, Jieqi, Jieqi, "jieqi";
    Judkins, Judkins, Judkins, "judkins", "judkinsshogi", "judkins-shogi";
    Karouk, Karouk, Karouk, "karouk", "kar-ouk", "kaouk";
    Khans, Khans, Khans, "khans";
    Kinglet, Kinglet, Kinglet, "kinglet";
    Knightmate, Knightmate, Knightmate, "knightmate";
    Kyotoshogi, Kyotoshogi, Kyotoshogi, "kyotoshogi", "kyoto", "kyoto-shogi";
    Legan, Legan, Legan, "legan";
    Losalamos, Losalamos, Losalamos, "losalamos", "losalamoschess", "los-alamos";
    Makpong, Makpong, Makpong, "makpong";
    Makruk, Makruk, Makruk, "makruk";
    Manchu, Manchu, Manchu, "manchu", "manchuchess";
    Mansindam, Mansindam, Mansindam, "mansindam";
    Micro, Micro, Micro, "micro", "microshogi", "micro-shogi";
    Minishogi, Minishogi, Minishogi, "minishogi";
    Minixiangqi, Minixiangqi, Minixiangqi, "minixiangqi", "minixq";
    Modern, Modern, Modern, "modern";
    Newzealand, Newzealand, Newzealand, "newzealand";
    Nightrider, Nightrider, Nightrider, "nightrider";
    Nocastle, Nocastle, Nocastle, "nocastle";
    Opulent, Opulent, Opulent, "opulent";
    Orda, Orda, Orda, "orda";
    Ordamirror, Ordamirror, Ordamirror, "ordamirror", "orda-mirror";
    Pawnback, Pawnback, Pawnback, "pawnback", "pawn-back", "pawnbackchess";
    Pawnsideways, Pawnsideways, Pawnsideways, "pawnsideways", "pawn-sideways";
    Perfect, Perfect, Perfect, "perfect", "perfectchess";
    Petrified, Petrified, Petrified, "petrified";
    Placement, Placement, Placement, "placement";
    Pocketknight, Pocketknight, Pocketknight, "pocketknight", "pocket-knight";
    Seirawan, Seirawan, Seirawan, "seirawan", "schess", "s-chess";
    Shako, Shako, Shako, "shako";
    Shatar, Shatar, Shatar, "shatar";
    Shatranj, Shatranj, Shatranj, "shatranj";
    Shinobi, Shinobi, Shinobi, "shinobi", "shinobiplus";
    Shogi, Shogi, Shogi, "shogi";
    Shogun, Shogun, Shogun, "shogun";
    ShoShogi, ShoShogi, ShoShogi, "shoshogi", "sho-shogi";
    Shouse, Shouse, Shouse, "shouse", "seirawanhouse";
    Sittuyin, Sittuyin, Sittuyin, "sittuyin", "burmese";
    Spartan, Spartan, Spartan, "spartan";
    Synochess, Synochess, Synochess, "synochess";
    Tencubed, Tencubed, Tencubed, "tencubed";
    Tenjiku, Tenjiku, Box<Tenjiku>, "tenjiku", "tenjikushogi", "tenjiku-shogi";
    Threekings, Threekings, Threekings, "threekings", "three-kings", "3kings";
    Tori, Tori, Tori, "tori", "torishogi";
    Torpedo, Torpedo, Torpedo, "torpedo";
    Washogi, Washogi, Washogi, "washogi", "wa-shogi", "wa";
    Xiangfu, Xiangfu, Xiangfu, "xiangfu";
    Xiangqi, Xiangqi, Xiangqi, "xiangqi", "cchess", "chinesechess";
}

impl WideVariantId {
    /// This identifier's stable index in [`WideVariantId::ALL`] (its declaration
    /// order), the 1-byte variant tag the self-describing binary wire format
    /// stores. The inverse of [`from_index`](Self::from_index).
    #[must_use]
    pub fn to_index(self) -> u8 {
        Self::ALL
            .iter()
            .position(|&id| id == self)
            .expect("every WideVariantId is in ALL") as u8
    }

    /// Builds an identifier from its [`to_index`](Self::to_index) tag, returning
    /// `None` if `index` names no variant (the wire decoder rejects such input).
    #[must_use]
    pub fn from_index(index: u8) -> Option<WideVariantId> {
        Self::ALL.get(index as usize).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// Fills `list` with `pos`'s legal moves via the allocation-free
    /// `legal_moves_into` and asserts they equal, byte-for-byte and in order, the
    /// move set the Vec-returning `legal_moves` produces.
    fn assert_into_agrees(pos: &AnyWideVariant, list: &mut WideMoveList) {
        let via_vec = pos.legal_moves();
        pos.legal_moves_into(list);
        let mut via_list = Vec::with_capacity(list.len());
        list.for_each(|mv| via_list.push(mv));
        assert_eq!(
            via_vec,
            via_list,
            "legal_moves_into diverged from legal_moves for {}",
            pos.variant_id()
        );
    }

    /// Every variant's [`WideVariant::ROLE_SPAN`](super::WideVariant::ROLE_SPAN)
    /// is a valid prefix length of `WideRole::ALL`(super::role::WideRole::ALL):
    /// at least one role wide and never past the global
    /// [`WideRole::COUNT`](crate::geometry::WideRole::COUNT). A span `> COUNT` would
    /// panic when the bounded movegen loops slice `WideRole::ALL[..ROLE_SPAN]`.
    #[test]
    fn role_span_within_count() {
        for &id in WideVariantId::ALL {
            let span = id.role_span();
            assert!(
                (1..=crate::geometry::WideRole::COUNT).contains(&span),
                "{id}: ROLE_SPAN {span} must be in 1..={}",
                crate::geometry::WideRole::COUNT
            );
        }
    }

    /// Safety meta-test for issue #514: the movegen role loops are bounded to each
    /// variant's [`ROLE_SPAN`](super::WideVariant::ROLE_SPAN), so a span set below a
    /// role the variant can actually field would silently drop that role's pieces.
    ///
    /// This walks every shipped variant from its start position over a bounded
    /// legal-move tree (full width, depth- and node-capped) and asserts that **no**
    /// reachable position ever fields a role at index `>= ROLE_SPAN` — the strongest
    /// form: the OR of the board occupancy and both hands is checked at every node
    /// via [`max_fielded_role_index`](AnyWideVariant::max_fielded_role_index), so it
    /// catches roles introduced by promotions, drops, gating, and reveals — not just
    /// the start army. A too-small span fails here (and would also diverge the
    /// per-variant perft suites); this test makes the requirement explicit.
    #[test]
    fn role_span_covers_all_fieldable_roles() {
        fn walk(
            pos: &AnyWideVariant,
            span: usize,
            id: WideVariantId,
            depth: u32,
            budget: &mut u64,
        ) {
            let max = pos.max_fielded_role_index();
            assert!(
                max < span,
                "{id}: reached role index {max} (>= ROLE_SPAN {span}); \
                 the bounded movegen loops would drop this role's pieces — \
                 raise this variant's ROLE_SPAN to at least {}",
                max + 1
            );
            if depth == 0 || *budget == 0 {
                return;
            }
            for mv in pos.legal_moves() {
                if *budget == 0 {
                    break;
                }
                *budget -= 1;
                let child = pos.play(&mv);
                walk(&child, span, id, depth - 1, budget);
            }
        }
        for &id in WideVariantId::ALL {
            let pos = AnyWideVariant::startpos(id);
            // A full-width tree to depth 6, capped at 100k explored moves per
            // variant so the walk reaches promotions/drops/reveals on many lines
            // without an unbounded blow-up on the wide giant-shogi variants.
            let mut budget: u64 = 100_000;
            walk(&pos, id.role_span(), id, 6, &mut budget);
        }
    }

    #[test]
    fn legal_moves_into_matches_legal_moves_every_variant() {
        // One buffer reused across every shipped variant and one ply of children,
        // proving the allocation-free entry point agrees with the Vec form on
        // every generator dispatch (standard and the special paths — duck, Alice,
        // placement, multi-royal, cannon, gating, hands) without corruption from
        // reuse.
        let mut list = WideMoveList::new();
        for &id in WideVariantId::ALL {
            let pos = AnyWideVariant::startpos(id);
            assert_into_agrees(&pos, &mut list);
            for mv in pos.legal_moves() {
                let child = pos.play(&mv);
                assert_into_agrees(&child, &mut list);
            }
        }
    }

    #[test]
    fn from_str_round_trips_every_canonical_name() {
        for &id in WideVariantId::ALL {
            assert_eq!(id.as_str().parse::<WideVariantId>(), Ok(id), "{id}");
            // Case-insensitive and whitespace-tolerant.
            let padded = alloc::format!("  {}  ", id.as_str().to_ascii_uppercase());
            assert_eq!(padded.parse::<WideVariantId>(), Ok(id), "{id} padded");
        }
        // Every canonical name is distinct (the round trip above would otherwise
        // be ambiguous).
        let mut names: Vec<&str> = WideVariantId::ALL.iter().map(|id| id.as_str()).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "canonical names must be unique");
        assert_eq!(count, 89, "all 89 fairy variants are covered");
    }

    #[test]
    fn position_footprint_reports_every_variant() {
        for &id in WideVariantId::ALL {
            let (size, align) = id.position_footprint();
            assert!(size > 0, "{id} position has non-zero size");
            assert!(align.is_power_of_two(), "{id} alignment is a power of two");
            assert_eq!(size % align, 0, "{id} size is a multiple of its alignment");
            // The static accessor agrees with the wrapped position's own size.
            assert_eq!(
                AnyWideVariant::startpos(id).position_footprint(),
                (size, align),
                "{id} instance footprint matches the id accessor"
            );
        }
        // The U256-backed large-shogi boards are the widest positions and dwarf
        // an 8x8 u64 arm.
        assert!(
            WideVariantId::Tenjiku.position_footprint().0
                > WideVariantId::Seirawan.position_footprint().0,
            "Tenjiku (16x16 U256) is larger than Seirawan (8x8 u64)"
        );
    }

    // ---- Memory regression gate (issue #504) --------------------------------
    //
    // These are the cheap, compile-time-stable per-PR memory guards: they run on
    // every `cargo test` at zero runtime cost and fail immediately if a type that
    // is on the size-sensitive hot path (the runtime facade enum, the wide move,
    // or any concrete position) grows past its current, deliberately-set ceiling.
    // Each ceiling is a MEASURED current value; bumping one is allowed only as a
    // conscious decision (see the per-test comment) — a change that grows a role
    // array, a move word, or the facade fails here first, before it can silently
    // regress the library's footprint. See `docs/perf-regression.md`.

    /// The runtime facade [`AnyWideVariant`] is sized by its widest **inline**
    /// arm (a `u128` position; the three U256 large-shogi arms are boxed, so they
    /// do not inflate it — see the enum's storage note). This ceiling pins that
    /// size so adding a variant, widening a position's inline state, or
    /// un-boxing a large arm fails here.
    ///
    /// Current measured value: 2784 bytes (was 2768 before the Grasshopper role took
    /// [`WideRole::COUNT`](super::role::WideRole::COUNT) from 146 to 147, adding one
    /// `u128` slot — 16 bytes — to the widest inline board's per-role array). Raise
    /// this **only** deliberately, with a justified reason (a genuinely needed larger
    /// arm) — never to paper over an accidental bloat.
    #[test]
    fn any_wide_variant_size_ceiling() {
        const CEILING: usize = 2832;
        let actual = core::mem::size_of::<AnyWideVariant>();
        assert!(
            actual <= CEILING,
            "size_of::<AnyWideVariant>() = {actual} exceeds the {CEILING}-byte \
             ceiling; a size-sensitive type grew. Bump the ceiling only if the \
             growth is justified (see docs/perf-regression.md)."
        );
    }

    /// The wide move packs into a single `u64`. This centralizes the memory gate's
    /// view of that guarantee (the structural guard also lives beside the type in
    /// `wide_move.rs`); it is an exact `== 8`, not a ceiling — the packed layout
    /// depends on the word being exactly eight bytes, so any change is a
    /// deliberate wire-format decision, not a silent regression.
    #[test]
    fn wide_move_size_is_eight() {
        assert_eq!(
            core::mem::size_of::<WideMove>(),
            8,
            "WideMove must stay a packed u64 (8 bytes); see wide_move.rs"
        );
    }

    /// Per-backing position-size ceiling: for each bitboard backing (`u64` /
    /// `u128` / `U256`), the largest concrete position over that backing must stay
    /// within its current measured size. Buckets every shipped variant by
    /// [`WideVariantId::position_backing_bits`] and checks the max
    /// [`position_footprint`](WideVariantId::position_footprint) size against a
    /// per-backing ceiling — so growing any position's role array / inline state
    /// past its geometry class's current widest fails here.
    ///
    /// Current measured maxima: u64 = 1560 (ai-wok), u128 = 2800 (cannonshogi),
    /// U256 = 5264 (chu) — above the pre-Grasshopper values (1528 / 2752 / 5168), the
    /// extra per-role slots the Grasshopper, Nightrider and New Zealand ROOKNI roles'
    /// [`WideRole::COUNT`](super::role::WideRole::COUNT) 146->149 bump adds to every
    /// board's role array (one bitboard per backing width per role, plus alignment
    /// padding). Raise a ceiling only deliberately.
    #[test]
    fn per_backing_position_size_ceiling() {
        // (backing bits, ceiling in bytes). Measured on main; bump only with cause.
        const CEILINGS: &[(u32, usize)] = &[(64, 1568), (128, 2816), (256, 5296)];
        for &(bits, ceiling) in CEILINGS {
            let mut max = 0usize;
            let mut worst = "";
            for &id in WideVariantId::ALL {
                if id.position_backing_bits() != bits {
                    continue;
                }
                let (size, _align) = id.position_footprint();
                if size > max {
                    max = size;
                    worst = id.as_str();
                }
            }
            assert!(
                max <= ceiling,
                "max {bits}-bit-backed position size = {max} ({worst}) exceeds the \
                 {ceiling}-byte ceiling; a position over the {bits}-bit backing grew. \
                 Bump the ceiling only deliberately (see docs/perf-regression.md)."
            );
        }
        // Sanity: every backing bucket is actually populated, so a ceiling can
        // never pass vacuously (e.g. if a backing's variants all disappeared).
        for &(bits, _) in CEILINGS {
            assert!(
                WideVariantId::ALL
                    .iter()
                    .any(|id| id.position_backing_bits() == bits),
                "no shipped variant uses the {bits}-bit backing; ceiling would be vacuous"
            );
        }
    }

    #[test]
    fn from_str_accepts_documented_aliases() {
        let cases: &[(&str, WideVariantId)] = &[
            ("aiwok", WideVariantId::Aiwok),
            ("kaouk", WideVariantId::Karouk),
            ("bug", WideVariantId::Bughouse),
            ("ouk", WideVariantId::Cambodian),
            ("capa", WideVariantId::Capablanca),
            ("dark", WideVariantId::FogOfWar),
            ("KOREAN", WideVariantId::Janggi),
            ("kyoto", WideVariantId::Kyotoshogi),
            ("s-chess", WideVariantId::Seirawan),
            ("schess", WideVariantId::Seirawan),
            ("cchess", WideVariantId::Xiangqi),
            ("chinesechess", WideVariantId::Xiangqi),
            ("  torishogi ", WideVariantId::Tori),
        ];
        for (name, id) in cases {
            assert_eq!(name.parse::<WideVariantId>(), Ok(*id), "parsing {name:?}");
        }
    }

    #[test]
    fn from_str_rejects_junk() {
        for junk in ["", "chess", "xyzzy", "shogi9", "  not a variant  "] {
            let err = junk.parse::<WideVariantId>().unwrap_err();
            assert_eq!(err.0, junk.trim().to_ascii_lowercase());
        }
    }

    #[test]
    fn startpos_fen_round_trips_for_every_variant() {
        for &id in WideVariantId::ALL {
            let pos = AnyWideVariant::startpos(id);
            assert_eq!(pos.variant_id(), id);
            let fen = pos.to_fen();
            let reparsed = AnyWideVariant::from_fen(id, &fen).expect("startpos fen parses");
            assert_eq!(reparsed.to_fen(), fen, "fen round trip for {id}");
            assert_eq!(reparsed.variant_id(), id);
        }
    }

    /// Make/unmake round-trip (issue #309): for every variant, walking the
    /// legal-move tree from the start position and reaching each child by
    /// `apply_with_undo` must (a) match `play` exactly and (b) be restored
    /// byte-for-byte by the matching `undo` — board, state, and promoted mask.
    ///
    /// The always-run sweep is a depth-2 walk across all 47 variants (fast even in
    /// a debug build); the exhaustive deeper coverage of every move kind reached at
    /// depth — captures, en passant, promotions, drops, gates, Alice transfers,
    /// the Janggi pass — is provided by [`make_unmake_round_trips_deep`] and, more
    /// thoroughly still, by the pinned perft suites (which now walk the tree by
    /// make/unmake, so any undo defect surfaces as a node-count mismatch).
    #[test]
    fn make_unmake_round_trips_for_every_variant() {
        for &id in WideVariantId::ALL {
            AnyWideVariant::startpos(id).assert_make_unmake_walk(2);
        }
    }

    /// The deep make/unmake round-trip sweep (issue #309): a depth-3 walk across
    /// all 47 variants, reaching captures, en passant, promotions, drops, gates,
    /// and the Janggi pass as *applied* moves. `#[ignore]`d so the default
    /// `cargo test` stays fast; run with
    /// `cargo test --release --lib -- --ignored make_unmake_round_trips_deep`.
    #[test]
    #[ignore = "deep make/unmake walk; run with --release --ignored"]
    fn make_unmake_round_trips_deep() {
        for &id in WideVariantId::ALL {
            AnyWideVariant::startpos(id).assert_make_unmake_walk(3);
        }
    }

    // Make/unmake byte-identity over a *deep* seeded random line (issue #438):
    // for every variant, `AnyWideVariant::assert_make_unmake_line` plays a random
    // self-play line and, at each node, checks that
    // `GenericPosition::apply_with_undo` matches `play` and that `undo` restores
    // the node byte-for-byte, hash included. The always-run walk above is
    // exhaustive but shallow (depth 2); this reaches far deeper single lines,
    // exercising captures, promotions, drops, and gates that only appear many
    // plies in. Seeded via proptest for determinism, with a small case count
    // since each case sweeps all 47+ variants.
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(6))]

        /// Deep-line make/unmake byte-identity for every fairy variant.
        #[test]
        fn make_unmake_line_for_every_variant(seed in any::<u64>(), plies in 0u32..14) {
            for &id in WideVariantId::ALL {
                AnyWideVariant::startpos(id).assert_make_unmake_line(seed, plies);
            }
        }
    }

    /// The enum dispatch must agree, node for node, with the typed
    /// [`GenericPosition`](super::GenericPosition) path: same perft, same legal
    /// moves and FEN, and the same successor after the first legal move.
    macro_rules! agrees_with_typed {
        ($id:expr, $alias:ty, $arm:path, $depth:expr) => {{
            let typed = <$alias>::startpos();
            let any = AnyWideVariant::startpos($id);
            assert!(matches!(any, $arm(_)), "{} arm", $id);

            assert_eq!(any.to_fen(), typed.to_fen(), "{} fen", $id);
            assert_eq!(any.turn(), typed.turn(), "{} turn", $id);
            assert_eq!(any.is_check(), typed.is_check(), "{} check", $id);
            assert_eq!(any.legal_moves(), typed.legal_moves(), "{} moves", $id);
            assert_eq!(any.outcome(), typed.outcome(), "{} outcome", $id);
            assert_eq!(any.end_reason(), typed.end_reason(), "{} end", $id);

            // Enum perft equals the typed-path perft at a distinctive depth.
            for depth in 0..=$depth {
                assert_eq!(
                    any.perft(depth),
                    perft(&typed, depth),
                    "{} perft {}",
                    $id,
                    depth
                );
            }

            // Playing the first legal move keeps the two in lockstep, and UCI
            // round-trips through the enum.
            if let Some(mv) = typed.legal_moves().first() {
                let uci = any.to_uci(mv);
                assert_eq!(any.parse_uci(&uci).as_ref(), Some(mv), "{} parse_uci", $id);
                let any_after = any.play(mv);
                let typed_after = typed.play(mv);
                assert_eq!(
                    any_after.to_fen(),
                    typed_after.to_fen(),
                    "{} after fen",
                    $id
                );
                assert_eq!(
                    any.play_uci(&uci).map(|p| p.to_fen()),
                    Some(typed_after.to_fen()),
                    "{} play_uci",
                    $id
                );
            }
        }};
    }

    #[test]
    fn enum_dispatch_matches_typed_path_for_every_variant() {
        agrees_with_typed!(WideVariantId::Aiwok, Aiwok, AnyWideVariant::Aiwok, 2);
        agrees_with_typed!(WideVariantId::Alice, Alice, AnyWideVariant::Alice, 2);
        agrees_with_typed!(WideVariantId::Almost, Almost, AnyWideVariant::Almost, 2);
        agrees_with_typed!(WideVariantId::Amazon, Amazon, AnyWideVariant::Amazon, 2);
        agrees_with_typed!(WideVariantId::Asean, Asean, AnyWideVariant::Asean, 2);
        agrees_with_typed!(
            WideVariantId::Berolina,
            Berolina,
            AnyWideVariant::Berolina,
            2
        );
        agrees_with_typed!(
            WideVariantId::Bughouse,
            Bughouse,
            AnyWideVariant::Bughouse,
            2
        );
        agrees_with_typed!(
            WideVariantId::Cambodian,
            Cambodian,
            AnyWideVariant::Cambodian,
            2
        );
        agrees_with_typed!(
            WideVariantId::CannonShogi,
            CannonShogi,
            AnyWideVariant::CannonShogi,
            2
        );
        agrees_with_typed!(
            WideVariantId::Capablanca,
            Capablanca,
            AnyWideVariant::Capablanca,
            2
        );
        agrees_with_typed!(
            WideVariantId::Capahouse,
            Capahouse,
            AnyWideVariant::Capahouse,
            2
        );
        agrees_with_typed!(
            WideVariantId::Caparandom,
            Caparandom,
            AnyWideVariant::Caparandom,
            2
        );
        agrees_with_typed!(WideVariantId::Chak, Chak, AnyWideVariant::Chak, 2);
        agrees_with_typed!(
            WideVariantId::Chancellor,
            Chancellor,
            AnyWideVariant::Chancellor,
            2
        );
        agrees_with_typed!(
            WideVariantId::Chaturanga,
            Chaturanga,
            AnyWideVariant::Chaturanga,
            2
        );
        agrees_with_typed!(
            WideVariantId::CheckShogi,
            CheckShogi,
            AnyWideVariant::CheckShogi,
            2
        );
        agrees_with_typed!(WideVariantId::Chennis, Chennis, AnyWideVariant::Chennis, 2);
        agrees_with_typed!(
            WideVariantId::Chigorin,
            Chigorin,
            AnyWideVariant::Chigorin,
            2
        );
        agrees_with_typed!(WideVariantId::Courier, Courier, AnyWideVariant::Courier, 2);
        agrees_with_typed!(WideVariantId::Dobutsu, Dobutsu, AnyWideVariant::Dobutsu, 2);
        agrees_with_typed!(WideVariantId::Dragon, Dragon, AnyWideVariant::Dragon, 2);
        agrees_with_typed!(WideVariantId::Duck, Duck, AnyWideVariant::Duck, 2);
        agrees_with_typed!(WideVariantId::Embassy, Embassy, AnyWideVariant::Embassy, 2);
        agrees_with_typed!(WideVariantId::Empire, Empire, AnyWideVariant::Empire, 2);
        agrees_with_typed!(
            WideVariantId::EuroShogi,
            EuroShogi,
            AnyWideVariant::EuroShogi,
            2
        );
        agrees_with_typed!(
            WideVariantId::FogOfWar,
            FogOfWar,
            AnyWideVariant::FogOfWar,
            2
        );
        agrees_with_typed!(
            WideVariantId::Georgian,
            Georgian,
            AnyWideVariant::Georgian,
            2
        );
        agrees_with_typed!(
            WideVariantId::Gorogoro,
            Gorogoro,
            AnyWideVariant::Gorogoro,
            2
        );
        agrees_with_typed!(WideVariantId::Gothic, Gothic, AnyWideVariant::Gothic, 2);
        agrees_with_typed!(WideVariantId::Grand, Grand, AnyWideVariant::Grand, 2);
        agrees_with_typed!(
            WideVariantId::Grandhouse,
            Grandhouse,
            AnyWideVariant::Grandhouse,
            2
        );
        agrees_with_typed!(
            WideVariantId::Grasshopper,
            Grasshopper,
            AnyWideVariant::Grasshopper,
            2
        );
        agrees_with_typed!(
            WideVariantId::HoppelPoppel,
            HoppelPoppel,
            AnyWideVariant::HoppelPoppel,
            2
        );
        agrees_with_typed!(WideVariantId::Janggi, Janggi, AnyWideVariant::Janggi, 2);
        agrees_with_typed!(WideVariantId::Janus, Janus, AnyWideVariant::Janus, 2);
        agrees_with_typed!(WideVariantId::Jieqi, Jieqi, AnyWideVariant::Jieqi, 2);
        agrees_with_typed!(WideVariantId::Judkins, Judkins, AnyWideVariant::Judkins, 2);
        agrees_with_typed!(WideVariantId::Karouk, Karouk, AnyWideVariant::Karouk, 2);
        agrees_with_typed!(WideVariantId::Khans, Khans, AnyWideVariant::Khans, 2);
        agrees_with_typed!(
            WideVariantId::Knightmate,
            Knightmate,
            AnyWideVariant::Knightmate,
            2
        );
        agrees_with_typed!(
            WideVariantId::Kyotoshogi,
            Kyotoshogi,
            AnyWideVariant::Kyotoshogi,
            2
        );
        agrees_with_typed!(WideVariantId::Legan, Legan, AnyWideVariant::Legan, 2);
        agrees_with_typed!(
            WideVariantId::Losalamos,
            Losalamos,
            AnyWideVariant::Losalamos,
            2
        );
        agrees_with_typed!(WideVariantId::Makpong, Makpong, AnyWideVariant::Makpong, 2);
        agrees_with_typed!(WideVariantId::Makruk, Makruk, AnyWideVariant::Makruk, 2);
        agrees_with_typed!(WideVariantId::Manchu, Manchu, AnyWideVariant::Manchu, 2);
        agrees_with_typed!(
            WideVariantId::Mansindam,
            Mansindam,
            AnyWideVariant::Mansindam,
            2
        );
        agrees_with_typed!(WideVariantId::Micro, Micro, AnyWideVariant::Micro, 2);
        agrees_with_typed!(
            WideVariantId::Minishogi,
            Minishogi,
            AnyWideVariant::Minishogi,
            2
        );
        agrees_with_typed!(
            WideVariantId::Minixiangqi,
            Minixiangqi,
            AnyWideVariant::Minixiangqi,
            2
        );
        agrees_with_typed!(
            WideVariantId::Newzealand,
            Newzealand,
            AnyWideVariant::Newzealand,
            2
        );
        agrees_with_typed!(
            WideVariantId::Nightrider,
            Nightrider,
            AnyWideVariant::Nightrider,
            2
        );
        agrees_with_typed!(WideVariantId::Orda, Orda, AnyWideVariant::Orda, 2);
        agrees_with_typed!(
            WideVariantId::Ordamirror,
            Ordamirror,
            AnyWideVariant::Ordamirror,
            2
        );
        agrees_with_typed!(
            WideVariantId::Pawnback,
            Pawnback,
            AnyWideVariant::Pawnback,
            2
        );
        agrees_with_typed!(
            WideVariantId::Pawnsideways,
            Pawnsideways,
            AnyWideVariant::Pawnsideways,
            2
        );
        agrees_with_typed!(WideVariantId::Perfect, Perfect, AnyWideVariant::Perfect, 2);
        agrees_with_typed!(
            WideVariantId::Petrified,
            Petrified,
            AnyWideVariant::Petrified,
            2
        );
        agrees_with_typed!(
            WideVariantId::Placement,
            Placement,
            AnyWideVariant::Placement,
            2
        );
        agrees_with_typed!(
            WideVariantId::Seirawan,
            Seirawan,
            AnyWideVariant::Seirawan,
            2
        );
        agrees_with_typed!(WideVariantId::Shako, Shako, AnyWideVariant::Shako, 2);
        agrees_with_typed!(WideVariantId::Shatar, Shatar, AnyWideVariant::Shatar, 2);
        agrees_with_typed!(
            WideVariantId::Shatranj,
            Shatranj,
            AnyWideVariant::Shatranj,
            2
        );
        agrees_with_typed!(WideVariantId::Shinobi, Shinobi, AnyWideVariant::Shinobi, 2);
        agrees_with_typed!(WideVariantId::Shogi, Shogi, AnyWideVariant::Shogi, 2);
        agrees_with_typed!(WideVariantId::Shogun, Shogun, AnyWideVariant::Shogun, 2);
        agrees_with_typed!(
            WideVariantId::ShoShogi,
            ShoShogi,
            AnyWideVariant::ShoShogi,
            2
        );
        agrees_with_typed!(WideVariantId::Shouse, Shouse, AnyWideVariant::Shouse, 2);
        agrees_with_typed!(
            WideVariantId::Sittuyin,
            Sittuyin,
            AnyWideVariant::Sittuyin,
            2
        );
        agrees_with_typed!(WideVariantId::Spartan, Spartan, AnyWideVariant::Spartan, 2);
        agrees_with_typed!(
            WideVariantId::Synochess,
            Synochess,
            AnyWideVariant::Synochess,
            2
        );
        agrees_with_typed!(WideVariantId::Tori, Tori, AnyWideVariant::Tori, 2);
        agrees_with_typed!(WideVariantId::Xiangfu, Xiangfu, AnyWideVariant::Xiangfu, 2);
        agrees_with_typed!(WideVariantId::Xiangqi, Xiangqi, AnyWideVariant::Xiangqi, 2);
    }

    // --- Issue #392: type-erased status / analysis forwards ---------------

    /// The consolidated [`GameStatus`] forward reports the right terminal state
    /// for a variant win, a draw, and an ongoing game — through the runtime
    /// [`AnyWideVariant`] enum, without naming any variant's geometry.
    #[test]
    fn status_forward_reports_terminal_state() {
        // Synochess campmate: a Black king reaching its goal rank is a variant win.
        let win =
            AnyWideVariant::from_fen(WideVariantId::Synochess, "8/8/8/8/8/8/4K3/3k4 w - - 0 1")
                .expect("valid synochess fen");
        assert_eq!(
            win.status(),
            GameStatus::VariantWin {
                winner: Color::Black,
                reason: WideEndReason::VariantWin,
            }
        );
        assert!(win.status().is_decisive());

        // Capablanca king vs. king: an insufficient-material draw.
        let draw = AnyWideVariant::from_fen(
            WideVariantId::Capablanca,
            "5k4/10/10/10/10/10/10/5K4 w - - 0 1",
        )
        .expect("valid capablanca fen");
        assert_eq!(
            draw.status(),
            GameStatus::Draw {
                reason: WideEndReason::InsufficientMaterial,
            }
        );
        assert!(draw.status().is_draw());

        // A start position is ongoing, and the folded status agrees with the
        // already-forwarded outcome.
        let start = AnyWideVariant::startpos(WideVariantId::Shogi);
        assert_eq!(start.status(), GameStatus::Ongoing);
        assert_eq!(start.status().outcome(), start.outcome());
    }

    /// The per-square analysis forwards are geometry-correct and semantically
    /// right: on an 8x8 board a lone rook attacks its own rank and file, and every
    /// query agrees square-for-square with the typed [`GenericPosition`] path.
    #[test]
    fn analysis_forwards_agree_with_typed_path() {
        let fen = "4k3/8/8/8/8/8/8/R3K3 w - - 0 1";
        let any = AnyWideVariant::from_fen(WideVariantId::Almost, fen).expect("valid almost fen");
        let typed = Almost::from_fen(fen).expect("valid almost fen");

        // Semantic anchors: the a1 rook (index 0) attacks along rank 1 and file a.
        assert!(
            any.is_attacked(1, Color::White),
            "b1 attacked by the a1 rook"
        );
        assert!(
            any.is_attacked(8, Color::White),
            "a2 attacked by the a1 rook"
        );
        assert!(!any.is_attacked(9, Color::White), "b2 is not attacked");
        assert_eq!(
            any.attackers_of(1, Color::White),
            alloc::vec![0u8],
            "only the a1 rook attacks b1"
        );
        assert_eq!(any.attacker_count(1, Color::White), 1);

        // Full square-for-square agreement with the typed path over the board.
        for i in 0u8..64 {
            let sq = Square::new(i);
            for side in [Color::White, Color::Black] {
                assert_eq!(
                    any.is_attacked(i, side),
                    typed.is_attacked(sq, side),
                    "is_attacked {i} {side:?}"
                );
                let typed_attackers: Vec<u8> = typed
                    .attackers_of(sq, side)
                    .into_iter()
                    .map(|s| s.index())
                    .collect();
                assert_eq!(
                    any.attackers_of(i, side),
                    typed_attackers,
                    "attackers_of {i} {side:?}"
                );
                assert_eq!(
                    any.attacker_count(i, side) as usize,
                    typed_attackers.len(),
                    "attacker_count {i} {side:?}"
                );
            }
            let typed_piece: Option<Vec<u8>> = typed
                .piece_attacks(sq)
                .map(|bb| bb.into_iter().map(|s| s.index()).collect());
            assert_eq!(any.piece_attacks(i), typed_piece, "piece_attacks {i}");
            assert_eq!(
                any.piece_mobility(i),
                typed.piece_mobility(sq),
                "piece_mobility {i}"
            );
        }

        // Off-board indices are handled gracefully rather than panicking.
        assert!(!any.is_attacked(200, Color::White));
        assert!(any.attackers_of(200, Color::White).is_empty());
        assert_eq!(any.attacker_count(200, Color::White), 0);
        assert_eq!(any.piece_attacks(200), None);
        assert_eq!(any.piece_mobility(200), 0);
    }

    /// The per-side aggregate forwards (status, attack / defense map, attack
    /// count) agree with the typed path across a spread of geometries — 8x8,
    /// 9x10 Xiangqi, 9x9 Shogi, 10x8 Capablanca, the 12x8 `u128`-backed Courier,
    /// and the tiny 3x4 Dobutsu board.
    #[test]
    fn analysis_aggregates_agree_with_typed_path() {
        macro_rules! aggregates_agree {
            ($id:expr, $alias:ty) => {{
                let typed = <$alias>::startpos();
                let any = AnyWideVariant::startpos($id);
                assert_eq!(any.status(), typed.status(), "{} status", $id);
                for side in [Color::White, Color::Black] {
                    assert_eq!(
                        any.attack_count(side),
                        typed.attack_count(side),
                        "{} attack_count",
                        $id
                    );
                    let typed_attacks: Vec<u8> = typed
                        .attack_map(side)
                        .into_iter()
                        .map(|s| s.index())
                        .collect();
                    assert_eq!(any.attack_map(side), typed_attacks, "{} attack_map", $id);
                    let typed_defense: Vec<u8> = typed
                        .defense_map(side)
                        .into_iter()
                        .map(|s| s.index())
                        .collect();
                    assert_eq!(any.defense_map(side), typed_defense, "{} defense_map", $id);
                    // The count and the erased index list stay consistent.
                    assert_eq!(
                        any.attack_count(side) as usize,
                        any.attack_map(side).len(),
                        "{} count/len",
                        $id
                    );
                }
            }};
        }

        aggregates_agree!(WideVariantId::Almost, Almost);
        aggregates_agree!(WideVariantId::Xiangqi, Xiangqi);
        aggregates_agree!(WideVariantId::Shogi, Shogi);
        aggregates_agree!(WideVariantId::Capablanca, Capablanca);
        aggregates_agree!(WideVariantId::Courier, Courier);
        aggregates_agree!(WideVariantId::Dobutsu, Dobutsu);
    }

    // --- Issue #411: type-erased check / pin / per-piece-move forwards -----

    /// The check / pin / per-piece-move forwards agree with the typed
    /// [`GenericPosition`](super::GenericPosition) path, square for square, across
    /// a spread of geometries and a crafted check + pin position.
    #[test]
    fn check_pin_forwards_agree_with_typed_path() {
        macro_rules! check_pin_agree {
            ($any:expr, $typed:expr) => {{
                let any = &$any;
                let typed = &$typed;
                let (w, h) = any.dimensions();
                let squares = w * h;
                for color in [Color::White, Color::Black] {
                    assert_eq!(
                        any.is_in_check(color),
                        typed.is_in_check(color),
                        "is_in_check"
                    );
                    let typed_royals: Vec<u8> = square_indices(typed.royal_squares(color));
                    assert_eq!(any.royal_squares(color), typed_royals, "royal_squares");
                    let typed_checkers: Vec<u8> = square_indices(typed.checkers_of(color));
                    assert_eq!(any.checkers_of(color), typed_checkers, "checkers_of");
                    let typed_pinned: Vec<u8> = square_indices(typed.pinned_pieces(color));
                    assert_eq!(any.pinned_pieces(color), typed_pinned, "pinned_pieces");
                    for i in 0..squares {
                        let sq = Square::new(i);
                        let typed_ray: Option<Vec<u8>> =
                            typed.pin_ray_of(color, sq).map(square_indices);
                        assert_eq!(any.pin_ray_of(color, i), typed_ray, "pin_ray {i}");
                    }
                }
                let typed_checkers: Vec<u8> = square_indices(typed.checkers());
                assert_eq!(any.checkers(), typed_checkers, "checkers");
                for i in 0..squares {
                    let sq = Square::new(i);
                    assert_eq!(
                        any.legal_moves_from(i),
                        typed.legal_moves_from(sq),
                        "legal_moves_from {i}",
                    );
                }
                // Off-board indices are handled gracefully.
                assert!(any.legal_moves_from(250).is_empty(), "off-board moves");
                assert_eq!(any.pin_ray_of(Color::White, 250), None, "off-board pin ray");
            }};
        }

        // A crafted 8x8 position: the white bishop on e2 is pinned to its king on
        // e1 by the black rook on e8 along the e-file (so the king is screened, not
        // itself in check) — exercising royal_squares, pinned_pieces, and pin_ray.
        let fen = "4r3/8/8/8/8/8/4B3/4K3 w - - 0 1";
        check_pin_agree!(
            AnyWideVariant::from_fen(WideVariantId::Almost, fen).expect("almost fen"),
            Almost::from_fen(fen).expect("almost fen")
        );

        // Start positions spanning several geometries and royalty rules.
        check_pin_agree!(
            AnyWideVariant::startpos(WideVariantId::Xiangqi),
            Xiangqi::startpos()
        );
        check_pin_agree!(
            AnyWideVariant::startpos(WideVariantId::Shogi),
            Shogi::startpos()
        );
        check_pin_agree!(
            AnyWideVariant::startpos(WideVariantId::Spartan),
            Spartan::startpos()
        );
        check_pin_agree!(
            AnyWideVariant::startpos(WideVariantId::Duck),
            Duck::startpos()
        );
        check_pin_agree!(
            AnyWideVariant::startpos(WideVariantId::Dobutsu),
            Dobutsu::startpos()
        );
        check_pin_agree!(
            AnyWideVariant::startpos(WideVariantId::Courier),
            Courier::startpos()
        );
    }
}
