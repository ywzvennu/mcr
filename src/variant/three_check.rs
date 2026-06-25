//! Three-check as a [`Variant`]: standard chess plus a win condition for the
//! side that delivers check three times.
//!
//! All standard rules (movegen, king safety, castling, promotion, the ordinary
//! checkmate / stalemate / draw terminations) are inherited unchanged; the only
//! additions are a per-side count of checks *given* and the terminal rule that
//! the first side to reach three checks wins immediately.
//!
//! # FEN convention
//!
//! Three-check FEN carries a seventh field in the Lichess *remaining-checks*
//! form `W+B`, where `W` is the number of checks White still needs to give and
//! `B` the number Black still needs, each counting down from three. The starting
//! position is therefore `3+3` (no checks delivered yet); once White has given
//! one check the field reads `2+3`, and a field component of `0` means that side
//! has already delivered its three checks and won.
//!
//! Internally the state stores checks *given* ([`CheckCounters`]); the remaining
//! form is `3 - given` per side. A counter is clamped at three (it never exceeds
//! the winning total), so the remaining value is always in `0..=3`.

use super::{Variant, VariantId, VariantPosition, VariantState};
use crate::position::FenError;
use crate::{Color, EndReason, Move, Position};

/// The number of checks a side must deliver to win.
const WIN_CHECKS: u8 = 3;

/// The count of checks *given* by each side in a three-check game.
///
/// Each counter is the number of times that color has delivered check, clamped
/// at [`WIN_CHECKS`]. The startpos value is `0`/`0`; reaching `WIN_CHECKS` is an
/// immediate win for that side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CheckCounters {
    /// Checks delivered by White.
    pub white: u8,
    /// Checks delivered by Black.
    pub black: u8,
}

impl CheckCounters {
    /// The number of checks `color` has delivered.
    #[must_use]
    #[inline]
    pub const fn given(self, color: Color) -> u8 {
        match color {
            Color::White => self.white,
            Color::Black => self.black,
        }
    }

    /// Increments `color`'s counter, saturating at [`WIN_CHECKS`].
    #[inline]
    fn record(&mut self, color: Color) {
        let slot = match color {
            Color::White => &mut self.white,
            Color::Black => &mut self.black,
        };
        *slot = (*slot).saturating_add(1).min(WIN_CHECKS);
    }

    /// The side that has reached the winning number of checks, if any.
    #[must_use]
    #[inline]
    fn winner(self) -> Option<Color> {
        if self.white >= WIN_CHECKS {
            Some(Color::White)
        } else if self.black >= WIN_CHECKS {
            Some(Color::Black)
        } else {
            None
        }
    }
}

impl VariantState for CheckCounters {}

/// One Zobrist constant per `(color, count)` check-count slot, generated
/// deterministically at compile time so the keys are identical across builds and
/// process runs (no `rand` dependency).
///
/// Indexed `[color][count]` with `count` in `0..=WIN_CHECKS`. Slot `count == 0`
/// holds zero, so an unchecked side contributes nothing and the startpos key is
/// left equal to the plain core key.
static CHECK_COUNT_KEYS: [[u64; WIN_CHECKS as usize + 1]; 2] = generate_check_count_keys();

/// The fixed seed for the check-count constant generator. Distinct from the core
/// Zobrist seed so these keys occupy their own feature space.
const CHECK_COUNT_SEED: u64 = 0x3CEC_4B1D_7C9E_2A57;

/// One step of splitmix64 (a tiny deterministic mixing function, not an RNG),
/// matching the core Zobrist table generator.
#[inline]
const fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Fills the check-count key table deterministically from [`CHECK_COUNT_SEED`].
/// Slot `count == 0` is left at zero so an unchecked side hashes to nothing.
const fn generate_check_count_keys() -> [[u64; WIN_CHECKS as usize + 1]; 2] {
    let mut keys = [[0u64; WIN_CHECKS as usize + 1]; 2];
    let mut state = CHECK_COUNT_SEED;
    let mut c = 0;
    while c < 2 {
        let mut n = 1;
        while n <= WIN_CHECKS as usize {
            keys[c][n] = splitmix64(&mut state);
            n += 1;
        }
        c += 1;
    }
    keys
}

/// The Zobrist constant for `color` having delivered `count` checks. Mixed into
/// the key so two positions differing only in their check counts hash apart.
#[inline]
fn check_count_key(color: Color, count: u8) -> u64 {
    let count = count.min(WIN_CHECKS) as usize;
    let c = match color {
        Color::White => 0,
        Color::Black => 1,
    };
    CHECK_COUNT_KEYS[c][count]
}

/// The three-check rule layer: standard chess plus per-side check counting and a
/// three-check win condition. A zero-sized marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ThreeCheckRules;

impl Variant for ThreeCheckRules {
    type State = CheckCounters;
    const ID: VariantId = VariantId::ThreeCheck;

    /// H1: the first side to reach [`WIN_CHECKS`] checks wins immediately.
    ///
    /// The winning side is whichever counter is at the limit; this is the side
    /// that just moved (you reach three checks by *delivering* a check, after
    /// which it is the opponent's turn), so the winner is the side *not* to move
    /// and [`EndReason::Checkmate`] yields exactly that decisive outcome.
    fn extra_terminal(_core: &Position, state: &Self::State) -> Option<EndReason> {
        // In every position reachable by play the three-check winner is the side
        // that just moved, i.e. the side *not* to move, because you reach three
        // checks by delivering one (after which it is the opponent's turn).
        // `EndReason::Checkmate` is the single-position reason whose outcome,
        // `Checkmate.outcome(turn) = Decisive { winner: turn.opposite() }`,
        // awards the win to exactly that side.
        state.winner().map(|_| EndReason::Checkmate)
    }

    /// H14: after each move, if the move left the opponent in check, credit the
    /// mover with a delivered check.
    fn post_apply(core: &mut Position, state: &mut Self::State, _mv: &Move) {
        // After the move, `core.turn()` is the opponent; `is_check()` reports
        // whether *that* side (the side to move) is in check, i.e. whether the
        // move just played delivered a check. The mover is the opposite color.
        if core.is_check() {
            state.record(core.turn().opposite());
        }
    }

    /// H12: fold both check counters into the Zobrist key.
    fn hash_state(state: &Self::State, hash: &mut u64) {
        *hash ^= check_count_key(Color::White, state.white);
        *hash ^= check_count_key(Color::Black, state.black);
    }

    /// H13 read: parse the seventh `W+B` remaining-checks field (see the module
    /// docs). Absent field defaults to `3+3` (no checks given).
    fn fen_extra_read<'a>(
        fields: &mut impl Iterator<Item = &'a str>,
    ) -> Result<Self::State, FenError> {
        let Some(field) = fields.next() else {
            return Ok(CheckCounters::default());
        };
        let (white_rem, black_rem) = field
            .split_once('+')
            .ok_or_else(|| FenError::BadNumber(field.to_owned()))?;
        let white = parse_remaining(white_rem)?;
        let black = parse_remaining(black_rem)?;
        Ok(CheckCounters {
            white: WIN_CHECKS - white,
            black: WIN_CHECKS - black,
        })
    }

    /// H13 write: emit the seventh `W+B` remaining-checks field.
    fn fen_extra_write(state: &Self::State, out: &mut String) {
        let white_rem = WIN_CHECKS - state.white.min(WIN_CHECKS);
        let black_rem = WIN_CHECKS - state.black.min(WIN_CHECKS);
        out.push(' ');
        out.push((b'0' + white_rem) as char);
        out.push('+');
        out.push((b'0' + black_rem) as char);
    }
}

/// Parses one component of the `W+B` remaining-checks field: a single digit in
/// `0..=3`.
fn parse_remaining(s: &str) -> Result<u8, FenError> {
    let value: u8 = s.parse().map_err(|_| FenError::BadNumber(s.to_owned()))?;
    if value > WIN_CHECKS {
        return Err(FenError::BadNumber(s.to_owned()));
    }
    Ok(value)
}

/// Three-check as a [`VariantPosition`].
///
/// Movegen and king safety are exactly standard chess; the only differences are
/// the per-side check count carried in [`CheckCounters`] and the three-check win
/// reported through [`VariantPosition::outcome`].
pub type ThreeCheck = VariantPosition<ThreeCheckRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variant::perft_variant;
    use crate::{Color, EndReason, Outcome};

    /// Plays a sequence of UCI moves from a starting three-check position.
    fn play_line(mut pos: ThreeCheck, ucis: &[&str]) -> ThreeCheck {
        for uci in ucis {
            let mv = pos.parse_uci(uci).expect("legal uci move");
            pos = pos.play(&mv);
        }
        pos
    }

    #[test]
    fn startpos_has_no_checks() {
        let pos = ThreeCheck::startpos();
        assert_eq!(pos.state(), &CheckCounters { white: 0, black: 0 });
        assert_eq!(pos.variant_id(), VariantId::ThreeCheck);
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 3+3"
        );
        assert_eq!(pos.legal_moves().len(), 20);
        assert!(pos.outcome().is_none());
    }

    #[test]
    fn quiet_check_increments_counter() {
        // White delivers a check with a non-capturing move; the counter must rise
        // even though nothing was captured.
        let pos: ThreeCheck = "4k3/8/8/8/8/8/8/Q3K3 w - - 0 1 3+3".parse().unwrap();
        let after = play_line(pos, &["a1a8"]); // Qa8+ checks the black king.
        assert!(after.is_check());
        assert_eq!(after.state(), &CheckCounters { white: 1, black: 0 });
        assert_eq!(after.to_fen(), "Q3k3/8/8/8/8/8/8/4K3 b - - 1 1 2+3");
    }

    #[test]
    fn capturing_check_increments_counter() {
        // A capture that also gives check counts exactly once.
        let pos: ThreeCheck = "3qk3/8/8/8/8/8/8/3QK3 w - - 0 1 3+3".parse().unwrap();
        let after = play_line(pos, &["d1d8"]); // Qxd8+ captures and checks.
        assert!(after.is_check());
        assert_eq!(after.state(), &CheckCounters { white: 1, black: 0 });
    }

    #[test]
    fn non_check_move_leaves_counter() {
        let pos = ThreeCheck::startpos();
        let after = play_line(pos, &["e2e4"]);
        assert!(!after.is_check());
        assert_eq!(after.state(), &CheckCounters { white: 0, black: 0 });
    }

    #[test]
    fn three_checks_win_for_checker() {
        // A lone white queen delivers three checks against a bare black king,
        // chasing it down a file; on the third check White wins immediately.
        let pos: ThreeCheck = "4k3/8/8/8/8/8/8/3QK3 w - - 0 1 3+3".parse().unwrap();
        let p1 = play_line(pos, &["d1d8"]); // Qd8+ check 1
        assert_eq!(p1.state().white, 1);
        let p2 = play_line(p1, &["e8f7"]); // Kf7
        let p3 = play_line(p2, &["d8d7"]); // Qd7+ check 2
        assert_eq!(p3.state().white, 2);
        let p4 = play_line(p3, &["f7f6"]); // Kf6
        let p5 = play_line(p4, &["d7d6"]); // Qd6+ check 3 -> White wins
        assert_eq!(p5.state().white, 3);
        assert!(p5.is_check());
        assert_eq!(
            p5.outcome(),
            Some(Outcome::Decisive {
                winner: Color::White
            })
        );
        assert_eq!(p5.end_reason(), Some(EndReason::Checkmate));
    }

    #[test]
    fn black_can_win_by_three_checks() {
        let pos: ThreeCheck = "3qk3/8/8/8/8/8/8/4K3 b - - 0 1 3+3".parse().unwrap();
        let p1 = play_line(pos, &["d8d1"]); // Qd1+ check 1
        assert_eq!(p1.state().black, 1);
        let p2 = play_line(p1, &["e1f2"]);
        let p3 = play_line(p2, &["d1d2"]); // Qd2+ check 2
        assert_eq!(p3.state().black, 2);
        let p4 = play_line(p3, &["f2f3"]);
        let p5 = play_line(p4, &["d2d3"]); // Qd3+ check 3 -> Black wins
        assert_eq!(p5.state().black, 3);
        assert_eq!(
            p5.outcome(),
            Some(Outcome::Decisive {
                winner: Color::Black
            })
        );
    }

    #[test]
    fn ordinary_checkmate_still_wins() {
        // Three-check inherits ordinary checkmate: fool's mate ends decisively for
        // Black even though no side reached three checks.
        let pos: ThreeCheck = "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3 3+3"
            .parse()
            .unwrap();
        assert_eq!(
            pos.outcome(),
            Some(Outcome::Decisive {
                winner: Color::Black
            })
        );
        assert!(pos.state().black < WIN_CHECKS);
    }

    #[test]
    fn fen_round_trips_with_check_field() {
        for fen in [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 3+3",
            "4k3/8/8/8/8/8/8/3QK3 b - - 1 1 2+3",
            "4k3/8/8/8/8/8/8/3QK3 w - - 4 3 1+0",
        ] {
            let pos: ThreeCheck = fen.parse().unwrap();
            assert_eq!(pos.to_fen(), fen, "round trip for {fen}");
        }
    }

    #[test]
    fn fen_given_form_maps_from_remaining() {
        // `2+3` remaining means White has given one check, Black none.
        let pos: ThreeCheck = "4k3/8/8/8/8/8/8/3QK3 b - - 1 1 2+3".parse().unwrap();
        assert_eq!(pos.state(), &CheckCounters { white: 1, black: 0 });
        // `1+0` remaining means White has given two, Black three (Black won).
        let pos: ThreeCheck = "4k3/8/8/8/8/8/8/3QK3 w - - 4 3 1+0".parse().unwrap();
        assert_eq!(pos.state(), &CheckCounters { white: 2, black: 3 });
    }

    #[test]
    fn missing_check_field_defaults_to_startpos_counts() {
        let pos: ThreeCheck = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        assert_eq!(pos.state(), &CheckCounters { white: 0, black: 0 });
    }

    #[test]
    fn malformed_check_field_rejected() {
        for bad in [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 3-3",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 4+3",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 x+3",
        ] {
            assert!(bad.parse::<ThreeCheck>().is_err(), "should reject {bad}");
        }
    }

    #[test]
    fn check_counts_affect_zobrist() {
        // Two positions identical except for the check counts must hash apart.
        let a: ThreeCheck = "4k3/8/8/8/8/8/8/3QK3 w - - 0 1 3+3".parse().unwrap();
        let b: ThreeCheck = "4k3/8/8/8/8/8/8/3QK3 w - - 0 1 2+3".parse().unwrap();
        let c: ThreeCheck = "4k3/8/8/8/8/8/8/3QK3 w - - 0 1 3+2".parse().unwrap();
        assert_ne!(a.zobrist(), b.zobrist());
        assert_ne!(a.zobrist(), c.zobrist());
        assert_ne!(b.zobrist(), c.zobrist());
        // Equal counts hash equal.
        let a2: ThreeCheck = "4k3/8/8/8/8/8/8/3QK3 w - - 0 1 3+3".parse().unwrap();
        assert_eq!(a.zobrist(), a2.zobrist());
    }

    #[test]
    fn zero_counts_match_core_zobrist() {
        // With no checks given the variant key equals the plain core key, so the
        // startpos hash is unchanged from standard chess.
        let pos = ThreeCheck::startpos();
        assert_eq!(pos.zobrist(), pos.core().zobrist());
    }

    #[test]
    fn incremental_hash_matches_after_check() {
        // Playing a checking move and reparsing the resulting FEN must yield the
        // same key, proving the incremental state-hash update is consistent.
        let pos: ThreeCheck = "4k3/8/8/8/8/8/8/Q3K3 w - - 0 1 3+3".parse().unwrap();
        let after = play_line(pos, &["a1a8"]);
        let reparsed: ThreeCheck = after.to_fen().parse().unwrap();
        assert_eq!(after.zobrist(), reparsed.zobrist());
        assert_eq!(after.state(), reparsed.state());
    }

    #[test]
    fn movegen_unaffected_by_counts() {
        // The check count never changes the legal-move set: a position with one
        // check already given has the same moves and perft as the same placement
        // with zero checks.
        let zero: ThreeCheck = "4k3/8/8/8/8/8/8/3QK3 w - - 0 1 3+3".parse().unwrap();
        let one: ThreeCheck = "4k3/8/8/8/8/8/8/3QK3 w - - 0 1 2+3".parse().unwrap();
        assert_eq!(
            perft_variant(&zero, 3),
            perft_variant(&one, 3),
            "counts must not change perft"
        );
    }
}
