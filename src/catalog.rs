//! The unified variant catalog: [`VariantRef`], one spanning identifier over both
//! variant families, and the single [`VariantRef::rules`] entry point.
//!
//! mcr ships two parallel variant families (see the crate docs): the concrete 8x8
//! engine ([`VariantId`]: standard chess and the eight classic 8x8 variants) and
//! the generic-geometry fairy layer
//! ([`WideVariantId`](crate::geometry::WideVariantId): the 90 wider or
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
use crate::geometry::WideVariantId;
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
    /// The single unified entry point spanning all ~99 variants: available for
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
