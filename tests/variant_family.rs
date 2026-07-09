//! Pins the piece-set [`VariantFamily`] classification of every shipped variant.
//!
//! [`VariantRef::rules().family`](mcr::VariantRef::rules) is derived (never a
//! hand-maintained table) from each variant's board, army, and mechanics by
//! `classify_family`. This test locks the resulting classification: it is both the
//! documentation of what every variant classifies as and the guard that a change to
//! the heuristic (or to a variant's declared rules) cannot silently re-bucket a
//! variant. The table is in [`VariantRef::ALL`](mcr::VariantRef::ALL) registry order
//! (concrete 8x8 variants first, then the wide fairy layer).

use mcr::geometry::VariantFamily;
use mcr::VariantRef;

/// The expected family of every variant, keyed by its canonical name, in
/// [`VariantRef::ALL`] order.
const EXPECTED: &[(&str, VariantFamily)] = &[
    // --- the concrete 8x8 engine: all chess-family ---
    ("standard", VariantFamily::Chess),
    ("chess960", VariantFamily::Chess),
    ("atomic", VariantFamily::Chess),
    ("antichess", VariantFamily::Chess),
    ("crazyhouse", VariantFamily::Chess),
    ("kingofthehill", VariantFamily::Chess),
    ("threecheck", VariantFamily::Chess),
    ("racingkings", VariantFamily::Chess),
    ("horde", VariantFamily::Chess),
    // --- the wide fairy layer ---
    ("ai-wok", VariantFamily::Makruk),
    ("alice", VariantFamily::Chess),
    ("almost", VariantFamily::Chess),
    ("amazon", VariantFamily::Chess),
    ("asean", VariantFamily::Makruk),
    ("atomar", VariantFamily::Chess),
    ("berolina", VariantFamily::Chess),
    ("bughouse", VariantFamily::Chess),
    ("cambodian", VariantFamily::Makruk),
    ("cannonshogi", VariantFamily::Shogi),
    ("capablanca", VariantFamily::Capablanca),
    ("capahouse", VariantFamily::Capablanca),
    ("caparandom", VariantFamily::Capablanca),
    ("centaur", VariantFamily::Capablanca),
    ("chak", VariantFamily::Fairy),
    ("chancellor", VariantFamily::Capablanca),
    ("chaturanga", VariantFamily::Chess),
    ("checkshogi", VariantFamily::Shogi),
    ("chennis", VariantFamily::Fairy),
    ("chessgi", VariantFamily::Chess),
    ("chigorin", VariantFamily::Chess),
    ("chu", VariantFamily::Shogi),
    ("codrus", VariantFamily::Chess),
    ("coregal", VariantFamily::Chess),
    ("courier", VariantFamily::Capablanca),
    ("dai", VariantFamily::Shogi),
    ("dobutsu", VariantFamily::Shogi),
    ("dragon", VariantFamily::Chess),
    ("duck", VariantFamily::Chess),
    ("embassy", VariantFamily::Capablanca),
    ("empire", VariantFamily::Chess),
    ("euroshogi", VariantFamily::Shogi),
    ("extinction", VariantFamily::Chess),
    ("5check", VariantFamily::Chess),
    ("fogofwar", VariantFamily::Chess),
    ("gardner", VariantFamily::Chess),
    ("georgian", VariantFamily::Chess),
    ("giveaway", VariantFamily::Chess),
    ("gorogoro", VariantFamily::Shogi),
    ("gothic", VariantFamily::Capablanca),
    ("grand", VariantFamily::Capablanca),
    ("grandhouse", VariantFamily::Capablanca),
    ("grasshopper", VariantFamily::Chess),
    ("gustav3", VariantFamily::Capablanca),
    ("hoppelpoppel", VariantFamily::Chess),
    ("janggi", VariantFamily::Janggi),
    ("janus", VariantFamily::Capablanca),
    ("jieqi", VariantFamily::Xiangqi),
    ("judkins", VariantFamily::Shogi),
    ("karouk", VariantFamily::Makruk),
    ("khans", VariantFamily::Chess),
    ("kinglet", VariantFamily::Chess),
    ("knightmate", VariantFamily::Chess),
    ("koedem", VariantFamily::Chess),
    ("kyotoshogi", VariantFamily::Shogi),
    ("legan", VariantFamily::Chess),
    ("loop", VariantFamily::Chess),
    ("losalamos", VariantFamily::Chess),
    ("losers", VariantFamily::Chess),
    ("makpong", VariantFamily::Makruk),
    ("makruk", VariantFamily::Makruk),
    ("manchu", VariantFamily::Xiangqi),
    // A modern Korean variant with shogi-style drops on a 9x9 board — grouped by
    // its shogi drop mechanic despite a western/fairy piece set.
    ("mansindam", VariantFamily::Shogi),
    ("micro", VariantFamily::Shogi),
    ("minishogi", VariantFamily::Shogi),
    ("minixiangqi", VariantFamily::Xiangqi),
    ("misere", VariantFamily::Chess),
    ("modern", VariantFamily::Capablanca),
    ("newzealand", VariantFamily::Chess),
    ("nightrider", VariantFamily::Chess),
    ("nocastle", VariantFamily::Chess),
    ("nocheckatomic", VariantFamily::Chess),
    ("okisakishogi", VariantFamily::Shogi),
    ("omicron", VariantFamily::Capablanca),
    ("opulent", VariantFamily::Capablanca),
    ("orda", VariantFamily::Chess),
    ("ordamirror", VariantFamily::Chess),
    ("paradigm", VariantFamily::Chess),
    ("pawnback", VariantFamily::Chess),
    ("pawnsideways", VariantFamily::Chess),
    ("perfect", VariantFamily::Chess),
    ("petrified", VariantFamily::Chess),
    ("placement", VariantFamily::Chess),
    ("pocketknight", VariantFamily::Chess),
    ("raazuvaa", VariantFamily::Chess),
    ("seirawan", VariantFamily::Chess),
    ("shako", VariantFamily::Capablanca),
    ("shatar", VariantFamily::Chess),
    ("shatranj", VariantFamily::Chess),
    ("shinobi", VariantFamily::Chess),
    ("shogi", VariantFamily::Shogi),
    ("shogun", VariantFamily::Chess),
    ("shoshogi", VariantFamily::Shogi),
    ("shouse", VariantFamily::Chess),
    ("sittuyin", VariantFamily::Makruk),
    ("sortofalmost", VariantFamily::Chess),
    ("spartan", VariantFamily::Chess),
    ("suicide", VariantFamily::Chess),
    ("supply", VariantFamily::Xiangqi),
    ("synochess", VariantFamily::Chess),
    ("tencubed", VariantFamily::Capablanca),
    ("tenjiku", VariantFamily::Shogi),
    ("threekings", VariantFamily::Chess),
    ("tori", VariantFamily::Shogi),
    ("torpedo", VariantFamily::Chess),
    ("washogi", VariantFamily::Shogi),
    ("wolf", VariantFamily::Capablanca),
    // A cannon/shogi hybrid on a 9x9 shogi board with drops — grouped by its shogi
    // drop mechanic (its cannon is not the xiangqi has_cannons mechanic).
    ("xiangfu", VariantFamily::Shogi),
    ("xiangqi", VariantFamily::Xiangqi),
    ("yarishogi", VariantFamily::Shogi),
];

#[test]
fn every_variant_has_its_pinned_family() {
    assert_eq!(
        EXPECTED.len(),
        VariantRef::ALL.len(),
        "the pinned table must cover every variant exactly once"
    );
    for (variant, &(name, expected)) in VariantRef::ALL.iter().zip(EXPECTED) {
        assert_eq!(
            variant.name(),
            name,
            "pinned table is out of registry order at {name}"
        );
        assert_eq!(
            variant.rules().family,
            expected,
            "family of {name} changed; update the classifier or the pin deliberately"
        );
    }
}

#[test]
fn classification_is_total_and_every_family_is_used() {
    // Every variant resolves to a family (the derivation never panics), and the
    // taxonomy is not carrying a dead arm: each of the seven families is populated.
    use VariantFamily::*;
    for family in [Chess, Capablanca, Xiangqi, Janggi, Shogi, Makruk, Fairy] {
        assert!(
            VariantRef::ALL.iter().any(|v| v.rules().family == family),
            "no variant classified as {family:?}"
        );
    }
}

#[test]
fn representative_variants_land_in_the_expected_family() {
    let family = |name: &str| {
        VariantRef::from_name(name)
            .expect("known variant name")
            .rules()
            .family
    };
    assert_eq!(family("standard"), VariantFamily::Chess);
    assert_eq!(family("grasshopper"), VariantFamily::Chess); // 8x8 fairy
    assert_eq!(family("capablanca"), VariantFamily::Capablanca);
    assert_eq!(family("grand"), VariantFamily::Capablanca);
    assert_eq!(family("xiangqi"), VariantFamily::Xiangqi);
    assert_eq!(family("janggi"), VariantFamily::Janggi);
    assert_eq!(family("shogi"), VariantFamily::Shogi);
    assert_eq!(family("chu"), VariantFamily::Shogi); // large shogi
    assert_eq!(family("makruk"), VariantFamily::Makruk);
    assert_eq!(family("chak"), VariantFamily::Fairy);
}
