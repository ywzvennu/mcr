//! Variant name + FEN-dialect mapping between mcr and Fairy-Stockfish (FSF).
//!
//! mcr and FSF agree on the *positions* but spell them slightly differently.
//! This module maps the mcr [`VariantId`] to FSF's `UCI_Variant` name and the
//! `UCI_Chess960` flag, and rewrites an mcr FEN into the dialect FSF parses so
//! the two engines run the byte-identical position.
//!
//! Reconciled dialect differences:
//!
//! * **three-check** — both engines use Lichess *remaining-checks* `W+B`
//!   semantics, but place the field differently. mcr appends it after the
//!   fullmove number (`… 0 1 3+3`); FSF expects it as the field right after en
//!   passant (`… - 3+3 0 1`). We relocate the token.
//! * **chess960** — FSF needs `UCI_Variant fischerandom` and `UCI_Chess960
//!   true`; the FEN (including Shredder/X-FEN castling letters like `HFhf`) is
//!   passed through unchanged, which FSF accepts.
//! * **crazyhouse** — the bracketed pocket `[..]` (and empty `[]`) on the
//!   placement field is identical in both engines; passed through unchanged.
//! * **atomic / king-of-the-hill / racing-kings / antichess / horde /
//!   standard** — identical FEN; only the variant name differs.

use mcr::VariantId;

/// The FSF-side description of an mcr variant: the `UCI_Variant` name and
/// whether `UCI_Chess960` must be set.
#[derive(Debug, Clone, Copy)]
pub struct FsfVariant {
    /// The `UCI_Variant` value, e.g. `"atomic"`, `"3check"`, `"fischerandom"`.
    pub uci_variant: &'static str,
    /// Whether `UCI_Chess960` must be enabled for this variant.
    pub chess960: bool,
}

/// Map an mcr [`VariantId`] to its FSF `UCI_Variant` name + Chess960 flag.
///
/// Returns `None` for variants FSF does not share (there are none in the
/// current mcr set, but the signature keeps the door open).
pub fn to_fsf(id: VariantId) -> Option<FsfVariant> {
    let v = match id {
        VariantId::Standard => FsfVariant {
            uci_variant: "chess",
            chess960: false,
        },
        VariantId::Chess960 => FsfVariant {
            uci_variant: "fischerandom",
            chess960: true,
        },
        VariantId::Atomic => FsfVariant {
            uci_variant: "atomic",
            chess960: false,
        },
        VariantId::Antichess => FsfVariant {
            // FSF spells antichess "giveaway" (its "antichess" var is a
            // different ruleset). "giveaway" is the lichess-antichess match.
            uci_variant: "giveaway",
            chess960: false,
        },
        VariantId::Crazyhouse => FsfVariant {
            uci_variant: "crazyhouse",
            chess960: false,
        },
        VariantId::KingOfTheHill => FsfVariant {
            uci_variant: "kingofthehill",
            chess960: false,
        },
        VariantId::ThreeCheck => FsfVariant {
            uci_variant: "3check",
            chess960: false,
        },
        VariantId::RacingKings => FsfVariant {
            uci_variant: "racingkings",
            chess960: false,
        },
        VariantId::Horde => FsfVariant {
            uci_variant: "horde",
            chess960: false,
        },
    };
    Some(v)
}

/// Rewrite an mcr-dialect FEN into the FSF dialect for `id`.
///
/// All variants except three-check pass through unchanged. For three-check the
/// trailing `W+B` remaining-checks token is moved to the FSF position (right
/// after en passant).
pub fn fen_to_fsf(id: VariantId, fen: &str) -> String {
    match id {
        VariantId::ThreeCheck => three_check_fen_to_fsf(fen),
        _ => fen.to_string(),
    }
}

/// Move the Lichess trailing `W+B` check field into FSF's after-en-passant slot.
///
/// mcr:  `<board> <stm> <castle> <ep> <half> <full> <W+B>`
/// FSF:  `<board> <stm> <castle> <ep> <W+B> <half> <full>`
///
/// If the input does not carry a trailing `W+B` token (already FSF-shaped, or no
/// counter), it is returned unchanged. The semantics (remaining checks, white
/// first) are identical in both engines, so only the position moves.
fn three_check_fen_to_fsf(fen: &str) -> String {
    let fields: Vec<&str> = fen.split_whitespace().collect();
    // Expect 7 fields with the check token last and shaped like `3+3`.
    if fields.len() != 7 {
        return fen.to_string();
    }
    let check = fields[6];
    if !is_check_token(check) {
        return fen.to_string();
    }
    // board stm castle ep <check> half full
    format!(
        "{} {} {} {} {} {} {}",
        fields[0], fields[1], fields[2], fields[3], check, fields[4], fields[5]
    )
}

/// Is `tok` a `W+B` remaining-checks token (two single decimal digits around a
/// `+`)?
fn is_check_token(tok: &str) -> bool {
    match tok.split_once('+') {
        Some((w, b)) => {
            !w.is_empty()
                && !b.is_empty()
                && w.bytes().all(|c| c.is_ascii_digit())
                && b.bytes().all(|c| c.is_ascii_digit())
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_check_token_relocates() {
        let mcr = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 3+3";
        let fsf = three_check_fen_to_fsf(mcr);
        assert_eq!(
            fsf,
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 3+3 0 1"
        );
    }

    #[test]
    fn three_check_nondefault_counter_relocates() {
        let mcr = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 5 9 1+2";
        let fsf = three_check_fen_to_fsf(mcr);
        assert_eq!(
            fsf,
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 1+2 5 9"
        );
    }

    #[test]
    fn non_three_check_passes_through() {
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
        assert_eq!(fen_to_fsf(VariantId::Standard, fen), fen);
        assert_eq!(fen_to_fsf(VariantId::Atomic, fen), fen);
    }

    #[test]
    fn crazyhouse_pocket_passes_through() {
        let fen = "2k5/8/8/8/8/8/8/4K3[Qn] w - -";
        assert_eq!(fen_to_fsf(VariantId::Crazyhouse, fen), fen);
    }

    #[test]
    fn check_token_recogniser() {
        assert!(is_check_token("3+3"));
        assert!(is_check_token("0+2"));
        assert!(!is_check_token("KQkq"));
        assert!(!is_check_token("-"));
        assert!(!is_check_token("5"));
    }

    #[test]
    fn all_variants_map() {
        for id in [
            VariantId::Standard,
            VariantId::Chess960,
            VariantId::Atomic,
            VariantId::Antichess,
            VariantId::Crazyhouse,
            VariantId::KingOfTheHill,
            VariantId::ThreeCheck,
            VariantId::RacingKings,
            VariantId::Horde,
        ] {
            assert!(to_fsf(id).is_some());
        }
        assert!(to_fsf(VariantId::Chess960).unwrap().chess960);
        assert_eq!(
            to_fsf(VariantId::Antichess).unwrap().uci_variant,
            "giveaway"
        );
    }
}
