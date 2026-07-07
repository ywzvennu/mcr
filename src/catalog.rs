//! The unified variant catalog: [`VariantRef`], one spanning identifier over both
//! variant families, and the single [`VariantRef::rules`] entry point.
//!
//! mcr ships two parallel variant families (see the crate docs): the concrete 8x8
//! engine ([`VariantId`]: standard chess and the eight classic 8x8 variants) and
//! the generic-geometry fairy layer
//! ([`WideVariantId`](crate::geometry::WideVariantId): the 101 wider or
//! ([`WideVariantId`](crate::geometry::WideVariantId): the 101 wider or
//! differently-shaped variants). Each exposes its own structured
//! [`VariantRules`](crate::geometry::VariantRules) model — [`VariantId::rules`]
//! and [`WideVariantId::rules`](crate::geometry::WideVariantId::rules) — but a
//! consumer that wants *every* variant's ruleset (a reference table, an API, a
//! docs page) would otherwise have to walk the two registries separately.
//!
//! [`VariantRef`] closes that gap: a plain enum with one arm per family,
//! [`VariantRef::ALL`] enumerating every variant across both, and
//! [`VariantRef::rules`] dispatching to the matching derivation. It is the single
//! surface a renderer builds on.

use crate::geometry::rules::VariantRules;
use crate::geometry::{WideRole, WideVariantId};
use crate::VariantId;

/// A stable identifier spanning **both** variant families: the concrete 8x8
/// engine ([`VariantId`]) and the generic-geometry fairy layer
/// ([`WideVariantId`]).
///
/// This is the unified catalog key. Enumerate every shipped variant across both
/// families with [`VariantRef::ALL`], read one's structured ruleset with
/// [`VariantRef::rules`], and look one up by name with [`VariantRef::name`] /
/// [`VariantRef::from_name`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum VariantRef {
    /// A concrete 8x8 variant: standard chess and the eight classic 8x8 variants.
    Concrete(VariantId),
    /// A generic-geometry fairy variant (Shogi, Xiangqi, Chu Shogi, Capablanca, …).
    Wide(WideVariantId),
}

/// The number of concrete 8x8 variants ([`VariantId::ALL`]).
const CONCRETE_COUNT: usize = VariantId::ALL.len();
/// The number of wide fairy variants ([`WideVariantId::ALL`]).
const WIDE_COUNT: usize = WideVariantId::ALL.len();

impl VariantRef {
    /// Every shipped variant across both families, concrete first (in
    /// [`VariantId::ALL`] order) then wide (in [`WideVariantId::ALL`] order) — the
    /// single unified registry a reference table or API iterates.
    pub const ALL: [VariantRef; CONCRETE_COUNT + WIDE_COUNT] = build_all();

    /// The structured, engine-derived [`VariantRules`](crate::geometry::VariantRules)
    /// for this variant, dispatched to the concrete
    /// ([`VariantId::rules`]) or wide
    /// ([`WideVariantId::rules`](crate::geometry::WideVariantId::rules)) derivation.
    ///
    /// The single unified entry point spanning all ~100 variants: available for
    /// every [`VariantRef::ALL`] without panicking, with every field derived from
    /// the variant's own move-generation hooks.
    #[must_use]
    pub fn rules(self) -> VariantRules {
        match self {
            VariantRef::Concrete(id) => id.rules(),
            VariantRef::Wide(id) => id.rules(),
        }
    }

    /// This variant's canonical lowercase name, the inverse of
    /// [`VariantRef::from_name`]. The concrete and wide name sets are disjoint, so
    /// the name identifies the ref unambiguously.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            VariantRef::Concrete(id) => id.as_str(),
            VariantRef::Wide(id) => id.as_str(),
        }
    }

    /// Resolves a name (or a documented alias) to its [`VariantRef`], trying the
    /// concrete family first and then the wide one. Matching is case-insensitive
    /// and ignores surrounding whitespace, as for the two underlying selectors.
    ///
    /// Returns `None` when the name matches no variant in either family.
    #[must_use]
    pub fn from_name(name: &str) -> Option<VariantRef> {
        if let Ok(id) = name.parse::<VariantId>() {
            return Some(VariantRef::Concrete(id));
        }
        name.parse::<WideVariantId>().ok().map(VariantRef::Wide)
    }
}

/// Builds [`VariantRef::ALL`] at compile time by concatenating the two registries.
const fn build_all() -> [VariantRef; CONCRETE_COUNT + WIDE_COUNT] {
    let mut out = [VariantRef::Concrete(VariantId::Standard); CONCRETE_COUNT + WIDE_COUNT];
    let mut i = 0;
    while i < CONCRETE_COUNT {
        out[i] = VariantRef::Concrete(VariantId::ALL[i]);
        i += 1;
    }
    let mut j = 0;
    while j < WIDE_COUNT {
        out[CONCRETE_COUNT + j] = VariantRef::Wide(WideVariantId::ALL[j]);
        j += 1;
    }
    out
}

// --- cross-queries ---------------------------------------------------------------
//
// Ergonomic finders over the unified catalog: iterate [`VariantRef::ALL`] and filter
// on each variant's derived [`VariantRules`], so a caller can select variants by a
// property (board size, a rule, a mechanic) without hand-walking the two registries
// or restating any rule. Every helper is a thin wrapper over [`VariantRef::matching`]
// and re-derives one variant's rules per candidate — cheap for a catalog of ~100, and
// pay-per-call (no cache), so use it for setup / tooling rather than a hot loop.

/// Whether a role is a pawn-family role (an ordinary pawn or a Spartan hoplite),
/// whose move geometry differs from its capture geometry by construction — excluded
/// from [`VariantRef::with_move_neq_capture`] so that finder surfaces only the
/// *fairy* move≠capture pieces (the Orda Lancer, the New Zealand Rook­ni).
fn is_pawn_role(role: WideRole) -> bool {
    matches!(role, WideRole::Pawn | WideRole::Hoplite)
}

impl VariantRef {
    /// Every shipped variant, in [`VariantRef::ALL`] order — the unfiltered base of
    /// the cross-queries. A convenience iterator over the `ALL` array for callers
    /// that want to chain their own adapters.
    pub fn all() -> impl Iterator<Item = VariantRef> {
        VariantRef::ALL.iter().copied()
    }

    /// Every variant whose derived [`VariantRules`] satisfy `predicate` — the general
    /// cross-query combinator the named finders below build on.
    ///
    /// The predicate is handed each candidate's freshly derived rules (via
    /// [`VariantRef::rules`]); compose your own selection when no named helper fits,
    /// e.g. the variants *without* castling are `VariantRef::matching(|r|
    /// !r.castling.enabled)`. Collect the result if you need a slice.
    pub fn matching(predicate: impl Fn(&VariantRules) -> bool) -> impl Iterator<Item = VariantRef> {
        VariantRef::ALL
            .iter()
            .copied()
            .filter(move |variant| predicate(&variant.rules()))
    }

    /// Every variant played on a board of exactly `files` by `ranks` squares — e.g.
    /// `by_board_size(5, 5)` selects the 5x5 variants (Gardner, Minishogi).
    pub fn by_board_size(files: u8, ranks: u8) -> impl Iterator<Item = VariantRef> {
        VariantRef::matching(move |rules| rules.board.width == files && rules.board.height == ranks)
    }

    /// Every variant **not** on the standard 8x8 board — the wider, smaller, or
    /// differently-shaped geometries (Shogi, Xiangqi, the minichess boards, …).
    pub fn on_non_standard_board() -> impl Iterator<Item = VariantRef> {
        VariantRef::matching(|rules| rules.board.width != 8 || rules.board.height != 8)
    }

    /// Every variant that offers en passant.
    pub fn with_en_passant() -> impl Iterator<Item = VariantRef> {
        VariantRef::matching(|rules| rules.pawns.en_passant)
    }

    /// Every variant that offers castling.
    pub fn with_castling() -> impl Iterator<Item = VariantRef> {
        VariantRef::matching(|rules| rules.castling.enabled)
    }

    /// Every variant with a persistent hand and drops (Crazyhouse and the shogi
    /// family).
    pub fn with_hand() -> impl Iterator<Item = VariantRef> {
        VariantRef::matching(|rules| rules.mechanics.has_hand)
    }

    /// Every variant decided by an **extinction** terminal — losing (or emptying) a
    /// watched role ends the game (Extinction, Kinglet, Threekings).
    pub fn won_by_extinction() -> impl Iterator<Item = VariantRef> {
        VariantRef::matching(|rules| rules.terminal.extinction.is_some())
    }

    /// Every variant with a **flag / campmate** win — a king reaching a goal rank
    /// (Racing Kings, Dobutsu's try).
    pub fn with_flag_win() -> impl Iterator<Item = VariantRef> {
        VariantRef::matching(|rules| rules.terminal.flag_win.is_some())
    }

    /// Every variant with a **region-goal** win — a king reaching any square in a
    /// goal set (King-of-the-Hill's central hill).
    pub fn with_region_win() -> impl Iterator<Item = VariantRef> {
        VariantRef::matching(|rules| rules.terminal.region_win.is_some())
    }

    /// Every variant fielding a **hopper** — a piece that moves or captures by
    /// hopping over a screen: a sampled screen-hopper such as the Grasshopper, or a
    /// cannon whose over-screen attack is computed from the whole board (the Xiangqi
    /// / Janggi / Manchu cannons, flagged by the cannon mechanic rather than sampled
    /// as a hopper piece).
    pub fn with_hoppers() -> impl Iterator<Item = VariantRef> {
        VariantRef::matching(|rules| {
            rules.mechanics.has_cannons || rules.army.iter().any(|piece| piece.hopper)
        })
    }

    /// Every variant that needs the full make/unmake king-safety re-test because it
    /// fields a **riding leaper** whose check geometry a static pin scan cannot
    /// capture (the Nightrider).
    pub fn with_riding_leapers() -> impl Iterator<Item = VariantRef> {
        VariantRef::matching(|rules| rules.mechanics.needs_full_verify)
    }

    /// Every variant with the **petrify-on-capture** mechanic (a capturing piece
    /// turns to stone).
    pub fn with_petrify() -> impl Iterator<Item = VariantRef> {
        VariantRef::matching(|rules| rules.mechanics.has_petrify)
    }

    /// Every variant in which **checks are wholly forbidden** — no move may give
    /// check (Racing Kings).
    pub fn forbidding_checks() -> impl Iterator<Item = VariantRef> {
        VariantRef::matching(|rules| rules.mechanics.checks_forbidden)
    }

    /// Every variant with **mandatory captures** — the side to move must play a
    /// capture when one is available (Antichess).
    pub fn with_mandatory_captures() -> impl Iterator<Item = VariantRef> {
        VariantRef::matching(|rules| rules.mechanics.mandatory_captures)
    }

    /// Every variant fielding a **move≠capture** fairy piece — a non-pawn whose
    /// movement geometry differs from its capture geometry (the Orda Lancer, the New
    /// Zealand Rook­ni). Pawns, whose move and capture always differ, are excluded so
    /// this surfaces only the fairy pieces.
    pub fn with_move_neq_capture() -> impl Iterator<Item = VariantRef> {
        VariantRef::matching(|rules| {
            rules
                .army
                .iter()
                .any(|piece| piece.move_neq_capture && !is_pawn_role(piece.role))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::VariantRef;
    use crate::VariantId;
    use alloc::vec::Vec;

    /// A concrete variant reference by name (panics if the name is unknown — a test
    /// helper only).
    fn v(name: &str) -> VariantRef {
        VariantRef::from_name(name).expect("known variant name")
    }

    fn collect(iter: impl Iterator<Item = VariantRef>) -> Vec<VariantRef> {
        iter.collect()
    }

    #[test]
    fn all_matches_the_registry() {
        assert_eq!(collect(VariantRef::all()), VariantRef::ALL.to_vec());
        // The unconstrained predicate is the whole catalog.
        assert_eq!(
            collect(VariantRef::matching(|_| true)).len(),
            VariantRef::ALL.len()
        );
        assert_eq!(collect(VariantRef::matching(|_| false)).len(), 0);
    }

    #[test]
    fn by_board_size_selects_the_5x5_boards() {
        let five = collect(VariantRef::by_board_size(5, 5));
        assert!(five.contains(&v("gardner")));
        assert!(five.contains(&v("minishogi")));
        // The standard 8x8 board is not 5x5.
        assert!(!five.contains(&VariantRef::Concrete(VariantId::Standard)));
    }

    #[test]
    fn non_standard_board_excludes_standard() {
        let wide = collect(VariantRef::on_non_standard_board());
        assert!(wide.contains(&v("gardner")));
        assert!(wide.contains(&v("shogi")));
        assert!(!wide.contains(&VariantRef::Concrete(VariantId::Standard)));
        // Every concrete 8x8 variant stays out of the non-standard set.
        assert!(!wide.contains(&VariantRef::Concrete(VariantId::Atomic)));
    }

    #[test]
    fn castling_and_no_castling_partition_sensibly() {
        let with = collect(VariantRef::with_castling());
        assert!(with.contains(&VariantRef::Concrete(VariantId::Standard)));

        // The complement, expressed with the general combinator.
        let without = collect(VariantRef::matching(|r| !r.castling.enabled));
        assert!(without.contains(&v("nocastle")));
        assert!(without.contains(&v("losalamos")));
        assert!(without.contains(&v("gardner")));
        assert!(!without.contains(&VariantRef::Concrete(VariantId::Standard)));
    }

    #[test]
    fn en_passant_and_hand_finders() {
        let ep = collect(VariantRef::with_en_passant());
        assert!(ep.contains(&VariantRef::Concrete(VariantId::Standard)));
        // Pawnless Racing Kings has no en passant.
        assert!(!ep.contains(&VariantRef::Concrete(VariantId::RacingKings)));

        let hand = collect(VariantRef::with_hand());
        assert!(hand.contains(&VariantRef::Concrete(VariantId::Crazyhouse)));
        assert!(hand.contains(&v("shogi")));
        assert!(!hand.contains(&VariantRef::Concrete(VariantId::Standard)));
    }

    #[test]
    fn extinction_win_set() {
        let ext = collect(VariantRef::won_by_extinction());
        assert!(ext.contains(&v("extinction")));
        assert!(ext.contains(&v("kinglet")));
        assert!(ext.contains(&v("threekings")));
        assert!(!ext.contains(&VariantRef::Concrete(VariantId::Standard)));
    }

    #[test]
    fn flag_and_region_win_sets() {
        let region = collect(VariantRef::with_region_win());
        assert!(region.contains(&VariantRef::Concrete(VariantId::KingOfTheHill)));
        assert!(!region.contains(&VariantRef::Concrete(VariantId::Standard)));

        let flag = collect(VariantRef::with_flag_win());
        assert!(flag.contains(&VariantRef::Concrete(VariantId::RacingKings)));
        assert!(!flag.contains(&VariantRef::Concrete(VariantId::Standard)));
    }

    #[test]
    fn hopper_and_riding_leaper_sets() {
        let hop = collect(VariantRef::with_hoppers());
        assert!(hop.contains(&v("grasshopper")));
        assert!(hop.contains(&v("xiangqi")));
        assert!(!hop.contains(&VariantRef::Concrete(VariantId::Standard)));

        let riders = collect(VariantRef::with_riding_leapers());
        assert!(riders.contains(&v("nightrider")));
        assert!(!riders.contains(&VariantRef::Concrete(VariantId::Standard)));
    }

    #[test]
    fn mandatory_captures_and_forbidden_checks() {
        let forced = collect(VariantRef::with_mandatory_captures());
        assert!(forced.contains(&VariantRef::Concrete(VariantId::Antichess)));
        assert!(!forced.contains(&VariantRef::Concrete(VariantId::Standard)));

        let no_check = collect(VariantRef::forbidding_checks());
        assert!(no_check.contains(&VariantRef::Concrete(VariantId::RacingKings)));
        assert!(!no_check.contains(&VariantRef::Concrete(VariantId::Standard)));
    }

    #[test]
    fn move_neq_capture_surfaces_fairy_pieces_not_pawns() {
        let mnc = collect(VariantRef::with_move_neq_capture());
        // Orda's Lancer moves unlike it captures.
        assert!(mnc.contains(&v("orda")));
        // Standard chess's only move≠capture piece is the pawn, which is excluded.
        assert!(!mnc.contains(&VariantRef::Concrete(VariantId::Standard)));
    }

    #[test]
    fn petrify_set_is_nonempty_and_excludes_standard() {
        let petrify = collect(VariantRef::with_petrify());
        assert!(petrify.contains(&v("petrified")));
        assert!(!petrify.contains(&VariantRef::Concrete(VariantId::Standard)));
    }
}
