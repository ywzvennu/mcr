//! Jieqi (揭棋, "hidden Xiangqi", 9x10) on the generic engine — standard Xiangqi
//! with every piece except the two Generals starting **face-down**, its identity
//! concealed until it first moves (issue #278). It reuses the Xiangqi mover and
//! king-safety machinery ([`super::xiangqi::XiangqiRules`]) wholesale; the only
//! new ingredient is the face-down [`WideRole::Dark`] piece and the **reveal**
//! that turns it into a concrete Xiangqi piece on its first move.
//!
//! ## Rules
//!
//! Jieqi is played on the same 9x10 board as Xiangqi (mcr's [`Xiangqi9x10`]), with
//! the same palace, river, and flying-general rule. The two **Generals** start
//! face-up on their home squares (e1 / e10). **Every other piece starts
//! face-down** as a [`WideRole::Dark`] piece.
//!
//! * **Hidden movement.** A face-down piece **moves as the Xiangqi piece native to
//!   its start (home) square**: a dark piece on a chariot's home square moves as a
//!   Chariot, on a horse's home square as a Horse, on an elephant/advisor/cannon/
//!   soldier home square as that piece. (This is the standard Jieqi rule; the
//!   issue's "generic dark piece" is realised per-start-square.) A dark piece is
//!   therefore **always on its home square** — it reveals the instant it moves — so
//!   [`home_role`] is always well-defined for a live dark piece.
//! * **Reveal.** On its **first move** a dark piece is **revealed**: its true
//!   identity is drawn from the side's remaining hidden **pool** — the Xiangqi army
//!   minus the General: `{2 Chariot, 2 Horse, 2 Elephant, 2 Advisor, 2 Cannon,
//!   5 Soldier}` = [`HIDDEN_POOL_SIZE`] pieces (see [`Pool`]). Once revealed it
//!   moves as that standard Xiangqi piece for the rest of the game.
//!
//! ## The reveal: two modes, one wired
//!
//! The reveal transition has two modes, selected by the optional
//! [`GenericState::jieqi_seed`](crate::geometry::position::GenericState::jieqi_seed):
//!
//! * **No seed (default) — the home-role baseline.** A dark piece reveals to the
//!   Xiangqi piece native to its home square ([`home_role`]). Deterministic, so the
//!   whole Jieqi tree collapses to standard Xiangqi and there is no genuine hidden
//!   information. This is the perft-validated core.
//! * **Seed present — the stochastic shuffle (issue #609).** A dark piece reveals
//!   to a **seed-derived** true identity drawn without replacement from its side's
//!   [`Pool`], which may differ from its home role. The assignment is a pure
//!   function of `(seed, color)` — a bijection from the side's 15 home squares onto
//!   its 15-piece army (see the internal `seeded_reveal`) — so it is deterministic,
//!   army-conserving, and independent of the order pieces flip in. The hidden state
//!   is **symmetric**: neither player knows a face-down piece's identity before it
//!   flips (the seed is a chance assignment the engine holds, not an owner-private
//!   secret), and a flip is public. The seed rides an optional seventh FEN field
//!   (a plain `u64`); a six-field FEN parses as the baseline. A per-player
//!   [`view_for`](crate::geometry::GenericPosition::view_for) redaction **strips
//!   the seed** and keeps every concealed piece a generic `Dark`, so a client never
//!   learns an unflipped assignment. Face-down *movement* stays the home-role mover
//!   in both modes.
//!
//! ## Validation (why Jieqi is split into a deterministic core + a seeded layer)
//!
//! Jieqi is **not** a Fairy-Stockfish variant: its stochastic hidden-identity
//! reveal cannot be expressed in an FSF variant config, and `go perft` is only
//! meaningful for a **full-information** position — which is exactly standard
//! Xiangqi. Correctness is therefore split:
//!
//! * **Deterministic core, perft-validated against FSF `UCI_Variant xiangqi`.**
//!   The reveal model wired into the engine's make-move path is the *identity*
//!   (no-shuffle) baseline: a dark piece reveals as the very piece native to its
//!   home square ([`WideVariant::reveal_on_move`] → [`home_role`]). Under that
//!   baseline a dark piece on square *s* both *moves as* and *reveals to* the
//!   Xiangqi piece native to *s*, so **the entire Jieqi game tree from the
//!   all-dark startpos is bit-identical to standard Xiangqi**. `perft` of the
//!   all-dark startpos therefore equals the FSF-confirmed Xiangqi perft at every
//!   depth (pinned in `tests/perft_jieqi.rs`; head-to-head in
//!   `compare-fairy/src/jieqi.rs`). This deterministically validates the dark
//!   movement *and* the reveal transition against FSF.
//! * **Stochastic reveal-from-pool, validated by seeded unit/property tests.** The
//!   true reveal draws a *random* unrevealed identity rather than the home piece.
//!   That randomness is modelled by the explicit, deterministic-when-seeded [`Pool`]
//!   (a draw-without-replacement multiset) — *not* baked into the perft path. The
//!   unit/property tests below pin its determinism, without-replacement exhaustion,
//!   and multiset conservation.
//!
//! ## Starting FEN
//!
//! The all-dark start, in the mcr dialect (`=D`/`=d` is a face-down piece, `K`/`k`
//! the face-up General):
//!
//! ```text
//! =d=d=d=dk=d=d=d=d/9/1=d5=d1/=d1=d1=d1=d1=d/9/9/=D1=D1=D1=D1=D/1=D5=D1/9/=D=D=D=DK=D=D=D=D w - - 0 1
//! ```
//!
//! Its identity-reveal Xiangqi equivalent is FSF's Xiangqi startpos
//! `rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR` (mcr
//! `rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/9/RJOUKUOJR`).

use super::xiangqi::XiangqiRules;
use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    Bitboard, Board, Geometry, RoyalSlider, Square, WideRole, WideVariant, Xiangqi9x10,
};
use crate::Color;

/// The Jieqi rule layer: a zero-sized [`WideVariant`] over [`Xiangqi9x10`].
///
/// It delegates every revealed-piece mover and king-safety hook to
/// [`XiangqiRules`] (Jieqi reuses the Xiangqi machinery wholesale) and adds only
/// the face-down [`WideRole::Dark`] piece — which moves as the Xiangqi piece
/// native to its home square ([`home_role`]) and reveals on its first move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct JieqiRules;

/// The all-dark Jieqi starting placement in the mcr dialect: the two Generals
/// (`K`/`k`) face-up on e1/e10, every other piece a face-down [`WideRole::Dark`]
/// (`=D`/`=d`) on its standard Xiangqi home square.
const JIEQI_START_PLACEMENT: &str =
    "=d=d=d=dk=d=d=d=d/9/1=d5=d1/=d1=d1=d1=d1=d/9/9/=D1=D1=D1=D1=D/1=D5=D1/9/=D=D=D=DK=D=D=D=D";

/// The six kinds of hidden piece and their per-side starting counts — the
/// standard Xiangqi army **minus the General**. The order is canonical and fixes
/// the index expansion [`Pool::draw_at`] draws from.
const HIDDEN_ARMY: [(WideRole, u8); 6] = [
    (WideRole::Rook, 2),            // Chariot
    (WideRole::Horse, 2),           //
    (WideRole::XiangqiElephant, 2), // Elephant
    (WideRole::Advisor, 2),         //
    (WideRole::Cannon, 2),          //
    (WideRole::Soldier, 5),         // Pawn
];

/// The number of face-down pieces each side hides at the start: the Xiangqi army
/// (16) minus the General = 15, matching the 15 non-General home squares.
pub const HIDDEN_POOL_SIZE: usize = 15;

/// The Xiangqi piece **native to a Jieqi home square** — the piece that occupies
/// `sq` in the standard Xiangqi starting array, and so the move set a face-down
/// piece on `sq` uses while hidden. Returns `None` for the General's square (the
/// General is never face-down) and for any square that is not a back-rank,
/// cannon, or soldier home square (no dark piece ever stands there).
///
/// Files are `a..i` = `0..9`, ranks `1..10` = `0..9`. The back ranks (1 / 10,
/// indices 0 / 9) carry Chariot-Horse-Elephant-Advisor-General-Advisor-Elephant-
/// Horse-Chariot; the cannons sit on ranks 3 / 8 (indices 2 / 7) at files b / h;
/// the soldiers on ranks 4 / 7 (indices 3 / 6) at files a / c / e / g / i.
#[must_use]
pub fn home_role(sq: Square<Xiangqi9x10>) -> Option<WideRole> {
    let file = sq.file();
    match sq.rank() {
        // Back ranks: the symmetric chariot..advisor array (file 4 is the General).
        0 | 9 => match file {
            0 | 8 => Some(WideRole::Rook),
            1 | 7 => Some(WideRole::Horse),
            2 | 6 => Some(WideRole::XiangqiElephant),
            3 | 5 => Some(WideRole::Advisor),
            _ => None,
        },
        // Cannon ranks: files b / h.
        2 | 7 => (file == 1 || file == 7).then_some(WideRole::Cannon),
        // Soldier ranks: files a / c / e / g / i.
        3 | 6 => matches!(file, 0 | 2 | 4 | 6 | 8).then_some(WideRole::Soldier),
        _ => None,
    }
}

/// The Xiangqi role a [`WideRole::Dark`] piece on `sq` acts as: its [`home_role`].
/// A concrete (already-revealed) role acts as itself.
#[inline]
fn effective_role(role: WideRole, sq: Square<Xiangqi9x10>) -> Option<WideRole> {
    if matches!(role, WideRole::Dark) {
        home_role(sq)
    } else {
        Some(role)
    }
}

/// A deterministic [`splitmix64`](https://prng.di.unimi.it/splitmix64.c) step:
/// advances `state` and returns a well-mixed 64-bit value. Used only to turn an
/// explicit seed into pool-draw indices — there is **no** clock or OS randomness,
/// so a given seed always yields the same sequence of reveals.
#[must_use]
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A side's **remaining hidden pool**: the multiset of yet-unrevealed identities,
/// starting as the Xiangqi army minus the General (see `HIDDEN_ARMY`). A reveal
/// **draws without replacement** from this pool, so a seeded sequence of draws is
/// a deterministic permutation of the army and the multiset is conserved.
///
/// This is the explicit, testable reveal model: it carries **no** randomness of
/// its own — [`Pool::draw_at`] is a pure index into the remaining expansion, and
/// [`Pool::draw`] turns a caller-supplied seed into that index via `splitmix64`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pool {
    /// Remaining count of each [`HIDDEN_ARMY`] kind, in that canonical order.
    counts: [u8; 6],
}

impl Pool {
    /// A full pool: the complete per-side hidden army (the start of a game).
    #[must_use]
    pub fn full() -> Self {
        let mut counts = [0u8; 6];
        let mut i = 0;
        while i < HIDDEN_ARMY.len() {
            counts[i] = HIDDEN_ARMY[i].1;
            i += 1;
        }
        Pool { counts }
    }

    /// The number of identities still hidden in this pool.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.counts.iter().map(|&c| c as usize).sum()
    }

    /// `true` if every identity has been revealed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// The number of `role` identities still hidden in this pool.
    #[must_use]
    pub fn count(&self, role: WideRole) -> usize {
        HIDDEN_ARMY
            .iter()
            .position(|&(r, _)| r == role)
            .map_or(0, |i| self.counts[i] as usize)
    }

    /// Draws the identity at position `index` (`0..remaining`) in the canonical
    /// expansion of the remaining pool, **removing** it. Pure and deterministic —
    /// no RNG. Returns `None` if `index >= remaining()`.
    ///
    /// The expansion lists the `HIDDEN_ARMY` kinds in order, each repeated by its
    /// remaining count, so the same `index` always names the same kind for a given
    /// pool state.
    #[must_use = "draw_at returns the drawn identity; ignoring it discards the reveal"]
    pub fn draw_at(&mut self, index: usize) -> Option<WideRole> {
        let mut acc = 0usize;
        for (i, &(role, _)) in HIDDEN_ARMY.iter().enumerate() {
            let c = self.counts[i] as usize;
            if index < acc + c {
                self.counts[i] -= 1;
                return Some(role);
            }
            acc += c;
        }
        None
    }

    /// Draws a uniformly-random identity from the remaining pool, **without
    /// replacement**, advancing the caller's explicit `seed` (`splitmix64`).
    /// Deterministic for a given seed (no clock / OS randomness). Returns `None`
    /// once the pool is exhausted.
    #[must_use = "draw returns the drawn identity; ignoring it discards the reveal"]
    pub fn draw(&mut self, seed: &mut u64) -> Option<WideRole> {
        let n = self.remaining();
        if n == 0 {
            return None;
        }
        let index = (splitmix64(seed) % n as u64) as usize;
        self.draw_at(index)
    }
}

impl Default for Pool {
    fn default() -> Self {
        Self::full()
    }
}

/// `true` if `sq` is a face-down **home square of `color`** — one of that side's 15
/// non-General start squares. White's army stands on ranks 1/3/4 (indices 0/2/3),
/// Black's on ranks 10/8/7 (indices 9/7/6); a square with a [`home_role`] on the
/// other side's ranks is the *opponent's* home square, never this color's.
#[inline]
fn is_home_square_of(color: Color, sq: Square<Xiangqi9x10>) -> bool {
    if home_role(sq).is_none() {
        return false;
    }
    match sq.rank() {
        0 | 2 | 3 => color == Color::White,
        6 | 7 | 9 => color == Color::Black,
        _ => false,
    }
}

/// The per-color starting `splitmix64` state: the raw `seed` for White, a fixed
/// remixing of it for Black, so the two sides draw **independent** deterministic
/// permutations of their (identical) hidden pools from one shared seed.
#[inline]
fn color_seed(seed: u64, color: Color) -> u64 {
    match color {
        Color::White => seed,
        // A distinct, deterministic derivation (the splitmix64 increment) so
        // Black's draw stream is decoupled from White's for any seed.
        Color::Black => seed ^ 0x9E37_79B9_7F4A_7C15,
    }
}

/// The **seed-derived true identity** a face-down piece of `color` standing on its
/// home square `from` reveals to.
///
/// The assignment is a pure function of `(seed, color)`, computed by drawing
/// **without replacement** from that side's full [`Pool`] once per home square in a
/// fixed canonical order (rank-major, file within rank). It is therefore a
/// *bijection* from the side's 15 home squares onto its 15-piece hidden army — so
/// the revealed identities of a side conserve exactly the army (no duplication or
/// loss), and the result is independent of the order the pieces actually flip in.
/// Returns `None` for a square that is not one of `color`'s home squares.
fn seeded_reveal(seed: u64, color: Color, from: Square<Xiangqi9x10>) -> Option<WideRole> {
    if !is_home_square_of(color, from) {
        return None;
    }
    let mut pool = Pool::full();
    let mut s = color_seed(seed, color);
    for rank in 0..Xiangqi9x10::HEIGHT {
        for file in 0..Xiangqi9x10::WIDTH {
            let Some(sq) = Square::from_file_rank(file, rank) else {
                continue;
            };
            if !is_home_square_of(color, sq) {
                continue;
            }
            // Exactly 15 draws for 15 home squares exhaust the 15-piece pool.
            let drawn = pool
                .draw(&mut s)
                .expect("a draw for each of the 15 home squares");
            if sq == from {
                return Some(drawn);
            }
        }
    }
    None
}

impl WideVariant<Xiangqi9x10> for JieqiRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 76;

    fn starting_position() -> (Board<Xiangqi9x10>, GenericState<Xiangqi9x10>) {
        let board = Board::<Xiangqi9x10>::from_fen_placement(JIEQI_START_PLACEMENT)
            .expect("the Jieqi all-dark starting placement is valid on a 9x10 board");
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::NONE,
            ep_square: None,
            ep_captured: None,
            gating: GenericGating::NONE,
            duck: None,
            placement: GenericPlacement::NONE,
            halfmove_clock: 0,
            fullmove_number: 1,
            consecutive_passes: 0,
            board_b: Bitboard::EMPTY,
            petrified: Bitboard::EMPTY,
            checks_against: [0, 0],
            jieqi_seed: None,
        };
        (board, state)
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Xiangqi9x10>,
        occupancy: Bitboard<Xiangqi9x10>,
    ) -> Bitboard<Xiangqi9x10> {
        // A dark piece attacks as the Xiangqi piece native to its home square; a
        // revealed piece as itself. Every effective role is an existing Xiangqi
        // mover, so the dispatch is the Xiangqi one.
        match effective_role(role, sq) {
            Some(eff) => XiangqiRules::role_attacks(eff, color, sq, occupancy),
            None => Bitboard::EMPTY,
        }
    }

    fn quiet_only_targets(
        role: WideRole,
        color: Color,
        sq: Square<Xiangqi9x10>,
        occupancy: Bitboard<Xiangqi9x10>,
    ) -> Bitboard<Xiangqi9x10> {
        // The Cannon's quiet rook-rays (the only role with a quiet-only set). A
        // dark piece on a cannon home square delegates here exactly as a revealed
        // cannon does.
        match effective_role(role, sq) {
            Some(eff) => XiangqiRules::quiet_only_targets(eff, color, sq, occupancy),
            None => Bitboard::EMPTY,
        }
    }

    fn role_attack_is_leg_asymmetric(role: WideRole) -> bool {
        // The Dark piece is forward-projected: depending on its home square it
        // stands in for any of the asymmetric Xiangqi movers (Horse / Cannon /
        // Soldier / region-confined General / Advisor / Elephant), so attacker
        // detection must project its effective attack set forward from its origin
        // — exactly as the generator does — rather than reverse-project a single
        // pattern. Every concrete Xiangqi role keeps its own classification.
        matches!(role, WideRole::Dark) || XiangqiRules::role_attack_is_leg_asymmetric(role)
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        XiangqiRules::role_attack_is_directional(role)
    }

    fn role_is_slider(role: WideRole) -> bool {
        // Only the revealed Chariot slides; a face-down piece is forward-projected
        // (never reverse-projected as a pin line), so the Xiangqi classification —
        // which reports `false` for everything but the Rook — applies unchanged.
        XiangqiRules::role_is_slider(role)
    }

    fn royal_slider_kind(role: WideRole) -> Option<RoyalSlider> {
        // The revealed Chariot is the plain rook fast-path; the Dark piece (and
        // every other role) is `None`, taking the exact forward-projection path.
        XiangqiRules::royal_slider_kind(role)
    }

    fn royal_reach_superset(
        role: WideRole,
        king: Square<Xiangqi9x10>,
    ) -> Option<Bitboard<Xiangqi9x10>> {
        // Revealed pieces keep their cheap reach supersets; the Dark piece returns
        // `None` (Xiangqi's fall-through), so the cannon verify tests every dark
        // piece with the exact forward projection — no genuine attacker is ever
        // skipped.
        XiangqiRules::royal_reach_superset(role, king)
    }

    fn has_castling() -> bool {
        false
    }

    fn has_cannons() -> bool {
        // Jieqi fields cannons (face-down and revealed), so it takes the same
        // pseudo-legal + per-move verify king-safety path as Xiangqi; the
        // flying-general extra attack rides the same verify.
        true
    }

    fn has_flying_general() -> bool {
        true
    }

    fn extra_royal_attack<const R: usize>(
        board: &Board<Xiangqi9x10, R>,
        sq: Square<Xiangqi9x10>,
        by: Color,
        occupied: Bitboard<Xiangqi9x10>,
    ) -> bool {
        // The flying-general rule is identical to Xiangqi.
        XiangqiRules::extra_royal_attack(board, sq, by, occupied)
    }

    fn reveal_on_move(
        role: WideRole,
        from: Square<Xiangqi9x10>,
        color: Color,
        seed: Option<u64>,
    ) -> Option<WideRole> {
        // A face-down piece reveals on its first move. With **no seed** (the
        // default) it reveals to the Xiangqi piece native to its origin (home)
        // square: under this baseline the whole Jieqi tree is exactly Xiangqi
        // (perft-validated vs FSF). With a **seed** it reveals to the seed-derived
        // true identity drawn from the side's hidden pool — which may differ from
        // the home role. Face-down *movement* is the home-role mover in both modes;
        // only the reveal transition consults the seed.
        if !matches!(role, WideRole::Dark) {
            return None;
        }
        match seed {
            None => home_role(from),
            Some(s) => seeded_reveal(s, color, from),
        }
    }

    fn carries_reveal_seed() -> bool {
        // Jieqi carries its optional stochastic-reveal seed in the state and as a
        // trailing FEN field. Absent ⇒ the home-role baseline (a six-field FEN is
        // backward-compatible and byte-identical).
        true
    }

    fn redact_board_for<const R: usize>(
        board: &Board<Xiangqi9x10, R>,
        _state: &GenericState<Xiangqi9x10, R>,
        _perspective: Color,
    ) -> Option<Board<Xiangqi9x10, R>> {
        // Jieqi's hidden state is **symmetric**: neither player knows a face-down
        // piece's true identity before it flips, and a flip is public. The board
        // itself already renders every concealed piece as a generic `Dark` token
        // and every revealed piece as its true identity, so the *board* leaks
        // nothing — the only secret is the assignment seed, which the redaction FEN
        // path (`to_fen_with_board`) strips. Returning `Some(*board)` (unchanged)
        // routes `view_for` / `spectator_view` through that seed-stripping path, so
        // a client receives an identical board with no seed. Both colors' views
        // agree on the hidden part, exactly as the symmetric rule requires.
        Some(*board)
    }
}

/// Jieqi (hidden Xiangqi) as a [`GenericPosition`] over the 9x10 [`Xiangqi9x10`]
/// geometry.
///
/// Construct the all-dark start with
/// [`Jieqi::startpos`](GenericPosition::startpos) or parse a FEN (mcr dialect,
/// face-down pieces as `=D`/`=d`) with
/// [`Jieqi::from_fen`](GenericPosition::from_fen). See the [module docs](self) for
/// the hidden movement, the reveal model, and how correctness is validated.
pub type Jieqi = GenericPosition<
    Xiangqi9x10,
    JieqiRules,
    { <JieqiRules as WideVariant<Xiangqi9x10>>::ROLE_SPAN },
>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{perft as gperft, Geometry, Xiangqi};

    /// Every square of the 9x10 board, in file-major order.
    fn all_squares() -> impl Iterator<Item = Square<Xiangqi9x10>> {
        (0..Xiangqi9x10::HEIGHT).flat_map(|rank| {
            (0..Xiangqi9x10::WIDTH).filter_map(move |file| Square::from_file_rank(file, rank))
        })
    }

    /// The all-dark startpos parses and has the General face-up on e1/e10 with
    /// every other start square holding a face-down Dark piece.
    #[test]
    fn startpos_is_two_generals_and_thirty_dark() {
        let board = *Jieqi::startpos().board();
        let mut dark = 0;
        let mut kings = 0;
        for sq in all_squares() {
            if let Some(p) = board.piece_at(sq) {
                match p.role {
                    WideRole::Dark => dark += 1,
                    WideRole::King => kings += 1,
                    other => panic!("unexpected start role {other:?} at {sq:?}"),
                }
            }
        }
        assert_eq!(kings, 2, "two face-up Generals");
        assert_eq!(dark, 2 * HIDDEN_POOL_SIZE, "30 face-down pieces");
    }

    /// `home_role` names exactly the standard Xiangqi piece on every start square,
    /// and `None` everywhere a dark piece never stands (the General's square and
    /// all empty squares).
    #[test]
    fn home_role_matches_the_xiangqi_start_array() {
        // Compare against the actual Xiangqi starting board: every non-General
        // occupied square's piece equals home_role; every other square is None.
        let board = *Xiangqi::startpos().board();
        for sq in all_squares() {
            match board.piece_at(sq) {
                Some(p) if p.role == WideRole::King => {
                    assert_eq!(home_role(sq), None, "General square {sq:?} is not dark");
                }
                Some(p) => {
                    assert_eq!(
                        home_role(sq),
                        Some(p.role),
                        "home_role at {sq:?} must equal the Xiangqi piece there",
                    );
                }
                None => assert_eq!(home_role(sq), None, "empty square {sq:?} has no home role"),
            }
        }
    }

    /// A face-down piece's pseudo-attacks equal its home piece's pseudo-attacks on
    /// the all-dark startpos — i.e. Dark movement is exactly the home-square mover.
    #[test]
    fn dark_attacks_equal_home_role_attacks() {
        let occ = Jieqi::startpos().board().occupied();
        for sq in all_squares() {
            let Some(home) = home_role(sq) else { continue };
            for color in [Color::White, Color::Black] {
                let dark = JieqiRules::role_attacks(WideRole::Dark, color, sq, occ);
                let direct = XiangqiRules::role_attacks(home, color, sq, occ);
                assert_eq!(dark, direct, "dark vs {home:?} at {sq:?} ({color:?})");
            }
        }
    }

    /// A fresh pool is the army minus the General: 2/2/2/2/2/5 = 15 pieces.
    #[test]
    fn full_pool_is_army_minus_general() {
        let pool = Pool::full();
        assert_eq!(pool.remaining(), HIDDEN_POOL_SIZE);
        assert_eq!(pool.count(WideRole::Rook), 2);
        assert_eq!(pool.count(WideRole::Horse), 2);
        assert_eq!(pool.count(WideRole::XiangqiElephant), 2);
        assert_eq!(pool.count(WideRole::Advisor), 2);
        assert_eq!(pool.count(WideRole::Cannon), 2);
        assert_eq!(pool.count(WideRole::Soldier), 5);
        // The General is never hidden.
        assert_eq!(pool.count(WideRole::King), 0);
    }

    /// `draw_at` draws without replacement and conserves the multiset: drawing
    /// every index `remaining-1 .. 0` (always the last expansion slot) empties the
    /// pool and yields exactly the full army.
    #[test]
    fn draw_at_exhausts_and_conserves_the_multiset() {
        let mut pool = Pool::full();
        let mut tally = [0u8; 6];
        while !pool.is_empty() {
            let last = pool.remaining() - 1;
            let role = pool.draw_at(last).expect("draw within range");
            let i = HIDDEN_ARMY.iter().position(|&(r, _)| r == role).unwrap();
            tally[i] += 1;
        }
        // Every identity drawn exactly its starting count; pool now empty.
        for (i, &(_, n)) in HIDDEN_ARMY.iter().enumerate() {
            assert_eq!(tally[i], n, "kind {i} drawn the right number of times");
        }
        assert!(pool.is_empty());
        // An out-of-range index draws nothing.
        assert_eq!(Pool::full().draw_at(HIDDEN_POOL_SIZE), None);
    }

    /// The seeded `draw` is deterministic (same seed → same sequence), exhausts the
    /// pool in exactly `HIDDEN_POOL_SIZE` draws, and the drawn sequence is always a
    /// permutation of the full army (multiset conservation) for every seed.
    #[test]
    fn seeded_draw_is_deterministic_and_conserves_the_army() {
        let run = |seed_init: u64| -> Vec<WideRole> {
            let mut pool = Pool::full();
            let mut seed = seed_init;
            let mut out = Vec::new();
            while let Some(r) = pool.draw(&mut seed) {
                out.push(r);
            }
            out
        };
        for seed in [0u64, 1, 2, 42, 0xDEAD_BEEF, u64::MAX, 0x1234_5678_9ABC_DEF0] {
            let a = run(seed);
            let b = run(seed);
            assert_eq!(a, b, "seed {seed:#x}: draw is deterministic");
            assert_eq!(
                a.len(),
                HIDDEN_POOL_SIZE,
                "seed {seed:#x}: exhausts the pool"
            );
            // Conservation: the drawn multiset equals the full army.
            let mut tally = [0u8; 6];
            for r in &a {
                let i = HIDDEN_ARMY.iter().position(|&(x, _)| x == *r).unwrap();
                tally[i] += 1;
            }
            for (i, &(_, n)) in HIDDEN_ARMY.iter().enumerate() {
                assert_eq!(tally[i], n, "seed {seed:#x}: kind {i} conserved");
            }
        }
    }

    /// Different seeds generally produce different reveal orders (the draw actually
    /// consumes the seed) — a sanity check that the seed is not ignored.
    #[test]
    fn distinct_seeds_diverge() {
        let run = |mut seed: u64| -> Vec<WideRole> {
            let mut pool = Pool::full();
            let mut out = Vec::new();
            while let Some(r) = pool.draw(&mut seed) {
                out.push(r);
            }
            out
        };
        assert_ne!(run(1), run(2), "distinct seeds give distinct reveal orders");
    }

    /// The all-dark startpos generates exactly the standard Xiangqi startpos moves
    /// (every dark piece moves as the Xiangqi piece native to its square), so its
    /// depth-1 move count equals the FSF-confirmed Xiangqi value.
    #[test]
    fn all_dark_startpos_depth1_equals_xiangqi() {
        let jq = gperft::<Xiangqi9x10, JieqiRules, _>(&Jieqi::startpos(), 1);
        let xq = gperft::<Xiangqi9x10, XiangqiRules, _>(&Xiangqi::startpos(), 1);
        assert_eq!(jq, xq, "Jieqi all-dark depth-1 == Xiangqi depth-1");
    }

    /// Under the engine's identity reveal the whole Jieqi tree collapses to
    /// Xiangqi: perft of the all-dark startpos equals perft of the Xiangqi startpos
    /// at depth 2 and 3 (the FSF-confirmed values), validating dark movement *and*
    /// the reveal transition together.
    #[test]
    fn all_dark_tree_equals_xiangqi_to_depth_3() {
        for depth in 1..=3 {
            let jq = gperft::<Xiangqi9x10, JieqiRules, _>(&Jieqi::startpos(), depth);
            let xq = gperft::<Xiangqi9x10, XiangqiRules, _>(&Xiangqi::startpos(), depth);
            assert_eq!(jq, xq, "Jieqi vs Xiangqi perft at depth {depth}");
        }
    }

    // -- The hidden-piece reveal layer (issue #501) -------------------------

    /// [`WideVariant::reveal_on_move`] reveals **only** a face-down [`WideRole::Dark`]
    /// piece, and reveals it to exactly its [`home_role`]; an already-concrete
    /// (face-up) role never re-reveals (`None`). This is the reveal transition the
    /// make-move path applies, checked over every square.
    #[test]
    fn reveal_on_move_reveals_dark_to_home_role_only() {
        for sq in all_squares() {
            // No seed (the baseline): a face-down piece reveals to the Xiangqi piece
            // native to its square, independent of the color argument.
            for color in [Color::White, Color::Black] {
                assert_eq!(
                    JieqiRules::reveal_on_move(WideRole::Dark, sq, color, None),
                    home_role(sq),
                    "Dark at {sq:?} reveals to its home role",
                );
                // An already-revealed (concrete) piece is never re-revealed.
                for role in [
                    WideRole::Rook,
                    WideRole::Horse,
                    WideRole::XiangqiElephant,
                    WideRole::Advisor,
                    WideRole::Cannon,
                    WideRole::Soldier,
                    WideRole::King,
                ] {
                    assert_eq!(
                        JieqiRules::reveal_on_move(role, sq, color, None),
                        None,
                        "concrete {role:?} at {sq:?} does not re-reveal",
                    );
                }
            }
        }
    }

    /// Playing a face-down piece's **first move** reveals it on the board: the
    /// piece that lands on the destination is its concrete [`home_role`], no longer
    /// [`WideRole::Dark`]. Checked for *every* legal first move of a dark piece
    /// from the all-dark startpos.
    #[test]
    fn first_move_reveals_the_dark_piece_on_the_board() {
        let start = Jieqi::startpos();
        let mut revealed_any = false;
        for mv in start.legal_moves() {
            let from = mv.from::<Xiangqi9x10>();
            let Some(piece) = start.board().piece_at(from) else {
                continue;
            };
            if piece.role != WideRole::Dark {
                continue;
            }
            let expected = home_role(from).expect("a dark piece stands on a home square");
            let after = start.play(&mv);
            let to = mv.to::<Xiangqi9x10>();
            let landed = after
                .board()
                .piece_at(to)
                .expect("the moved piece occupies its destination");
            assert_eq!(
                landed.role, expected,
                "dark piece at {from:?} reveals to {expected:?} on its first move",
            );
            assert_ne!(
                landed.role,
                WideRole::Dark,
                "the piece at {to:?} is face-up after moving",
            );
            revealed_any = true;
        }
        assert!(
            revealed_any,
            "the all-dark startpos has dark first moves to reveal",
        );
    }

    /// The identity reveals of a side's face-down army are a **legal
    /// draw-without-replacement** from that side's hidden [`Pool`]: each revealed
    /// role is still present in the remaining pool when it is drawn, and drawing
    /// every reveal empties the pool exactly (multiset conservation). This ties the
    /// on-board reveal to the seeded pool model — a revealed piece is always a legal
    /// draw from the remaining pool.
    #[test]
    fn identity_reveals_are_legal_draws_that_exhaust_the_pool() {
        // Remove `role` from `pool` via the first expansion slot of its kind (the
        // cumulative remaining count of the kinds that precede it), asserting it was
        // drawable — i.e. that the reveal is a legal draw from the remaining pool.
        fn draw_role(pool: &mut Pool, role: WideRole) -> bool {
            let mut index = 0usize;
            for &(kind, _) in HIDDEN_ARMY.iter() {
                if kind == role {
                    break;
                }
                index += pool.count(kind);
            }
            pool.count(role) > 0 && pool.draw_at(index) == Some(role)
        }

        let board = *Jieqi::startpos().board();
        for color in [Color::White, Color::Black] {
            let mut pool = Pool::full();
            let mut drawn = 0usize;
            for sq in all_squares() {
                let Some(piece) = board.piece_at(sq) else {
                    continue;
                };
                if piece.role != WideRole::Dark || piece.color != color {
                    continue;
                }
                let reveal = home_role(sq).expect("a dark piece has a home role");
                assert!(
                    pool.count(reveal) > 0,
                    "reveal {reveal:?} for {color:?} must be a legal draw from the remaining pool",
                );
                assert!(draw_role(&mut pool, reveal), "drawing {reveal:?} succeeds");
                drawn += 1;
            }
            assert_eq!(drawn, HIDDEN_POOL_SIZE, "{color:?} hides a full army");
            assert!(
                pool.is_empty(),
                "the identity reveals exhaust {color:?}'s pool exactly",
            );
        }
    }

    // -- The opt-in seeded-shuffle reveal (issue #609) ----------------------

    /// The all-dark startpos FEN with a trailing reveal-seed field (mcr dialect).
    const SEEDED_STARTPOS: &str =
        "=d=d=d=dk=d=d=d=d/9/1=d5=d1/=d1=d1=d1=d1=d/9/9/=D1=D1=D1=D1=D/1=D5=D1/9/=D=D=D=DK=D=D=D=D w - - 0 1 42";

    /// The home squares of `color`, in the same canonical order `seeded_reveal`
    /// draws them.
    fn home_squares_of(color: Color) -> Vec<Square<Xiangqi9x10>> {
        all_squares()
            .filter(|&sq| is_home_square_of(color, sq))
            .collect()
    }

    /// Each side has exactly its 15 home squares, and they are disjoint by color.
    #[test]
    fn home_squares_partition_the_two_armies() {
        let white = home_squares_of(Color::White);
        let black = home_squares_of(Color::Black);
        assert_eq!(white.len(), HIDDEN_POOL_SIZE, "White hides a full army");
        assert_eq!(black.len(), HIDDEN_POOL_SIZE, "Black hides a full army");
        for sq in &white {
            assert!(!black.contains(sq), "square {sq:?} is not shared");
        }
    }

    /// The seeded assignment is a **bijection** from a side's 15 home squares onto
    /// its 15-piece hidden army: every home square gets a defined reveal, and the
    /// multiset of reveals is exactly the army (conservation — no duplication or
    /// loss). Checked for several seeds and both colors.
    #[test]
    fn seeded_reveal_is_a_bijection_conserving_the_army() {
        for seed in [0u64, 1, 42, 0xDEAD_BEEF, u64::MAX] {
            for color in [Color::White, Color::Black] {
                let mut tally = [0u8; 6];
                for sq in home_squares_of(color) {
                    let role =
                        seeded_reveal(seed, color, sq).expect("every home square has a reveal");
                    let i = HIDDEN_ARMY
                        .iter()
                        .position(|&(r, _)| r == role)
                        .expect("a reveal is an army role");
                    tally[i] += 1;
                }
                for (i, &(_, n)) in HIDDEN_ARMY.iter().enumerate() {
                    assert_eq!(tally[i], n, "seed {seed:#x} {color:?}: kind {i} conserved");
                }
            }
            // A non-home square (the General's e1) has no seeded reveal.
            let e1 = Square::from_file_rank(4, 0).unwrap();
            assert_eq!(seeded_reveal(seed, Color::White, e1), None);
        }
    }

    /// Determinism: the same seed yields the same assignment every time, and (a
    /// sanity check that the seed is consumed) different seeds generally differ.
    #[test]
    fn seeded_reveal_is_deterministic() {
        let map = |seed: u64, color: Color| -> Vec<WideRole> {
            home_squares_of(color)
                .into_iter()
                .map(|sq| seeded_reveal(seed, color, sq).unwrap())
                .collect()
        };
        for seed in [0u64, 7, 42, u64::MAX] {
            assert_eq!(map(seed, Color::White), map(seed, Color::White));
        }
        assert_ne!(
            map(1, Color::White),
            map(2, Color::White),
            "distinct seeds give distinct assignments"
        );
    }

    /// With **no seed** the startpos is byte-identical to the baseline: its state
    /// carries no seed and `to_fen` emits the plain six-field FEN (no seventh
    /// field), so a round-trip is unchanged.
    #[test]
    fn no_seed_startpos_is_byte_identical() {
        let start = Jieqi::startpos();
        let fen = start.to_fen();
        assert_eq!(fen.split(' ').count(), 6, "no seed ⇒ six-field FEN: {fen}");
        // A freshly parsed baseline FEN carries no seed and round-trips unchanged.
        let reparsed = Jieqi::from_fen(&fen).expect("baseline FEN parses");
        assert_eq!(reparsed.to_fen(), fen, "baseline FEN round-trips");
    }

    /// A seeded FEN round-trips through `from_fen`/`to_fen`, preserving the trailing
    /// seed field.
    #[test]
    fn seeded_fen_round_trips() {
        let pos = Jieqi::from_fen(SEEDED_STARTPOS).expect("seeded FEN parses");
        assert_eq!(
            pos.to_fen(),
            SEEDED_STARTPOS,
            "seed survives the round-trip"
        );
        assert_eq!(
            pos.to_fen().split(' ').count(),
            7,
            "a seeded FEN has a seventh field"
        );
    }

    /// Under a seed a face-down piece's first move reveals its **seed-derived** true
    /// identity (not necessarily its home role), and at least one such reveal
    /// actually differs from the home role — proving the seed changes the game.
    #[test]
    fn seeded_first_move_reveals_the_seed_identity() {
        let start = Jieqi::from_fen(SEEDED_STARTPOS).expect("seeded FEN parses");
        let seed = 42u64;
        let mut diverged = false;
        for mv in start.legal_moves() {
            let from = mv.from::<Xiangqi9x10>();
            let Some(piece) = start.board().piece_at(from) else {
                continue;
            };
            if piece.role != WideRole::Dark {
                continue;
            }
            let expected =
                seeded_reveal(seed, piece.color, from).expect("a dark piece has a seeded reveal");
            let after = start.play(&mv);
            let landed = after
                .board()
                .piece_at(mv.to::<Xiangqi9x10>())
                .expect("the moved piece occupies its destination");
            assert_eq!(
                landed.role, expected,
                "dark piece at {from:?} reveals to its seeded identity",
            );
            if Some(expected) != home_role(from) {
                diverged = true;
            }
        }
        assert!(
            diverged,
            "seed 42 makes at least one reveal differ from the home role",
        );
    }

    /// Determinism at the game level: two positions from the same seeded FEN, played
    /// through the same move, reach byte-identical FENs (same reveal).
    #[test]
    fn same_seed_same_game() {
        let a = Jieqi::from_fen(SEEDED_STARTPOS).expect("parses");
        let b = Jieqi::from_fen(SEEDED_STARTPOS).expect("parses");
        let mv = a.legal_moves()[0];
        assert_eq!(a.play(&mv).to_fen(), b.play(&mv).to_fen());
    }

    /// `view_for` never leaks the seed or an unflipped identity: the redacted FEN
    /// has no seventh (seed) field, both colors' views agree on the (symmetric)
    /// board, and every concealed piece still renders as a generic Dark token.
    #[test]
    fn view_for_strips_seed_and_stays_symmetric() {
        let pos = Jieqi::from_fen(SEEDED_STARTPOS).expect("seeded FEN parses");
        let seed_field = "42";
        for color in [Color::White, Color::Black] {
            let view = pos.view_for(color);
            let fields: Vec<&str> = view.fen.split(' ').collect();
            assert_eq!(
                fields.len(),
                6,
                "redacted view drops the seed field: {}",
                view.fen
            );
            assert!(
                !view.fen.split(' ').any(|f| f == seed_field),
                "the seed must never appear in a redacted view: {}",
                view.fen
            );
            // Concealed pieces still render as generic Dark tokens.
            let placement = fields[0];
            assert!(
                placement.contains("=d") && placement.contains("=D"),
                "concealed pieces stay Dark in the view: {}",
                view.fen
            );
        }
        // Symmetric hidden state: both colors see the same board placement.
        let white_placement = pos
            .view_for(Color::White)
            .fen
            .split(' ')
            .next()
            .unwrap()
            .to_string();
        let black_placement = pos
            .view_for(Color::Black)
            .fen
            .split(' ')
            .next()
            .unwrap()
            .to_string();
        assert_eq!(
            white_placement, black_placement,
            "symmetric rule: both views agree on the concealed board"
        );
        // The spectator view likewise carries no seed field.
        let spectator = pos.spectator_view();
        assert_eq!(
            spectator.fen.split(' ').count(),
            6,
            "spectator drops the seed"
        );
    }

    /// A seeded game still leaves each side's revealed identities a legal exhaustive
    /// draw from its pool: playing out every first reveal under the seed conserves
    /// the army exactly (the seed only permutes which square gets which identity).
    #[test]
    fn seeded_reveals_conserve_each_army() {
        for color in [Color::White, Color::Black] {
            let mut tally = [0u8; 6];
            for sq in home_squares_of(color) {
                let role = seeded_reveal(42, color, sq).unwrap();
                let i = HIDDEN_ARMY.iter().position(|&(r, _)| r == role).unwrap();
                tally[i] += 1;
            }
            for (i, &(_, n)) in HIDDEN_ARMY.iter().enumerate() {
                assert_eq!(tally[i], n, "{color:?} kind {i} conserved under the seed");
            }
        }
    }
}
