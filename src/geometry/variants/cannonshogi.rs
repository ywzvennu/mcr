//! Cannon Shogi (大砲将棋, 9x9) on the generic engine — the first variant to
//! combine a Shogi-style **hand / drops** with **cannon-type** pieces (it rides
//! both the `has_hand` drop machinery and the `has_cannons` pseudo-legal +
//! per-move-verify king-safety path at once). Validated against Fairy-Stockfish
//! `UCI_Variant cannonshogi`.
//!
//! Cannon Shogi is the standard 9x9 Shogi army with two changes: the Pawn is
//! replaced by a **Soldier** (which also steps sideways) and **five CANNON-type
//! pieces** are added, each of which (with its promoted form) can also be dropped
//! from the hand. The geometry, promotion zone, and hand/drop mechanic are the
//! [`Shogi`](super::Shogi) ones; only the new movers differ.
//!
//! ## Pieces (confirmed square-for-square against FSF `cannonshogi`)
//!
//! Unchanged from Shogi: **King**, **Gold**, **Silver** (→ promoted), **Knight**
//! (→ promoted), **Lance** (→ promoted), **Rook** (→ Dragon), **Bishop** (→ Dragon
//! Horse). New:
//!
//! * **Soldier (歩, FSF `p`)** — the [`WideRole::Pawn`] reused as a forward **and
//!   sideways** one-step mover (`fsW`: it both moves and captures one square
//!   straight forward or one square to either side, never backward or diagonally).
//!   Promotes to a Tokin (Gold mover). Because it always has a sideways move it is
//!   never force-promoted and may be dropped on any rank (no last-rank dead-piece
//!   rule, and **no** *nifu*: FSF `dropNoDoubled = -`).
//! * **Cannon (砲 → `+U`, FSF `u`, Betza `mRcpR`)** — the Xiangqi rook-cannon
//!   reused as [`WideRole::Cannon`]: slides quietly like a rook and captures by
//!   jumping exactly one screen on a rook line. Promotes to a
//!   [`WideRole::PromotedCannon`].
//! * **Rook-cannon ([`WideRole::RookCannon`], FSF `a`, Betza `pR`)** — moves **and**
//!   captures only by jumping one screen on a rook line, sliding any distance
//!   beyond it. Promotes to a [`WideRole::PromotedRookCannon`].
//! * **Bishop-cannon ([`WideRole::BishopCannon`], FSF `c`, Betza `mBcpB`)** — the
//!   diagonal cannon: quiet bishop slide plus an over-one-diagonal-screen capture.
//!   Promotes to a [`WideRole::PromotedBishopCannon`].
//! * **Bishop-hopper ([`WideRole::BishopHopper`], FSF `i`, Betza `pB`)** — moves
//!   **and** captures only by jumping one diagonal screen. Promotes to a
//!   [`WideRole::PromotedBishopHopper`].
//!
//! The four promoted cannon forms share two movements: `+U` / `+A`
//! (`mRpRmFpB2` — a full rook line of quiet slides and unlimited over-screen hops,
//! plus a one-step diagonal quiet move and a range-2 diagonal hop) and `+C` / `+I`
//! (`mBpBmWpR2` — the orthogonal/diagonal mirror image). Each keeps its **distinct
//! base identity**, so a captured promoted piece reverts to the right unpromoted
//! role in hand (`+U → u`, `+A → a`, `+C → c`, `+I → i`), exactly as FSF banks it.
//!
//! ## Confirmed starting FEN
//!
//! FSF renders the start as
//!
//! ```text
//! lnsgkgsnl/1rci1uab1/p1p1p1p1p/9/9/9/P1P1P1P1P/1BAU1ICR1/LNSGKGSNL[-] w 0 1
//! ```
//!
//! mcr uses the same board with its own dialect for the cannon pieces — the
//! Cannon is `c`, and the three new movers spell themselves with the second
//! overflow prefix `=` (`=a` rook-cannon, `=c` bishop-cannon, `=i` bishop-hopper),
//! the promoted forms `=u` / `=w` / `=f` / `=e` — and an empty `[]` holdings
//! bracket. The `compare-fairy/` harness reconciles the dialect when driving FSF.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Geometry, PromotionConfig, Square, WideRole, WideVariant,
};
use crate::Color;

use super::super::Shogi9x9;
use super::shogi::ShogiRules;

/// The Cannon Shogi rule layer: a zero-sized [`WideVariant`] over [`Shogi9x9`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CannonShogiRules;

/// The confirmed Cannon Shogi starting placement (the hand is empty at the start).
/// FSF `lnsgkgsnl/1rci1uab1/p1p1p1p1p/9/9/9/P1P1P1P1P/1BAU1ICR1/LNSGKGSNL`, rewritten
/// into the mcr dialect: FSF `u`→Cannon `c`, `a`→`=a`, `c`→`=c`, `i`→`=i`, `p`→`p`.
const CANNONSHOGI_PLACEMENT: &str =
    "lnsgkgsnl/1r=c=i1c=ab1/p1p1p1p1p/9/9/9/P1P1P1P1P/1B=AC1=I=CR1/LNSGKGSNL";

/// The four diagonal one-step (ferz) offsets `(file, rank)`.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// The four orthogonal one-step (wazir) offsets `(file, rank)`.
const WAZIR_OFFSETS: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// The depth of the promotion zone: the furthest three ranks from each side.
const ZONE_DEPTH: u8 = 3;

impl CannonShogiRules {
    /// The Soldier's move-and-attack set: one step straight **forward** or one step
    /// to **either side** (`fsW`; it both moves and captures on all three). Never
    /// backward or diagonal. Replaces the Shogi Pawn (`fW`).
    fn soldier_attacks(color: Color, sq: Square<Shogi9x9>) -> Bitboard<Shogi9x9> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        attacks::leaper_attacks::<Shogi9x9>(sq, &[(0, fwd), (1, 0), (-1, 0)])
    }

    /// The Cannon's (`u`, `mRcpR`) full set: quiet rook slides plus over-one-screen
    /// captures on a rook line. Both are folded together — the generator's
    /// `emit_targets` splits them by enemy occupancy (empty quiet-ray squares are
    /// moves; the occupied target beyond a screen is a capture), and the over-screen
    /// capture lands only on an occupied square, so it never adds a phantom step.
    fn cannon_attacks(sq: Square<Shogi9x9>, occ: Bitboard<Shogi9x9>) -> Bitboard<Shogi9x9> {
        attacks::cannon_quiet_moves::<Shogi9x9>(sq, occ)
            | attacks::cannon_capture_targets::<Shogi9x9>(sq, occ)
    }

    /// The Rook-cannon's (`a`, `pR`) full set: jump exactly one screen on a rook
    /// line, then every empty square beyond it (a move) and the first piece past it
    /// (a capture). No non-jumping quiet slide. Built from the (no-cannon-restriction)
    /// Janggi orthogonal ray primitives with an empty cannon set.
    fn rook_cannon_attacks(sq: Square<Shogi9x9>, occ: Bitboard<Shogi9x9>) -> Bitboard<Shogi9x9> {
        attacks::janggi_cannon_quiet::<Shogi9x9>(sq, occ, Bitboard::EMPTY)
            | attacks::janggi_cannon_capture::<Shogi9x9>(sq, occ, Bitboard::EMPTY)
    }

    /// The Bishop-cannon's (`c`, `mBcpB`) full set: quiet bishop slides plus an
    /// over-one-diagonal-screen capture.
    fn bishop_cannon_attacks(sq: Square<Shogi9x9>, occ: Bitboard<Shogi9x9>) -> Bitboard<Shogi9x9> {
        (attacks::bishop_attacks::<Shogi9x9>(sq, occ) & !occ)
            | attacks::diag_cannon_capture_targets::<Shogi9x9>(sq, occ)
    }

    /// The Bishop-hopper's (`i`, `pB`) full set: jump exactly one diagonal screen,
    /// then every empty square beyond it (a move) and the first piece past it (a
    /// capture). The diagonal analogue of [`rook_cannon_attacks`](Self::rook_cannon_attacks).
    fn bishop_hopper_attacks(sq: Square<Shogi9x9>, occ: Bitboard<Shogi9x9>) -> Bitboard<Shogi9x9> {
        attacks::diag_cannon_quiet_jumps::<Shogi9x9>(sq, occ)
            | attacks::diag_cannon_capture_targets::<Shogi9x9>(sq, occ)
    }

    /// A range-2 hop over an **adjacent** screen along the four `dirs`: for each
    /// direction whose immediate neighbour is occupied (any piece, the screen), the
    /// square two steps away (if on the board) is a destination — empty (a move) or
    /// enemy (a capture; friendly squares are masked out by the generator). This is
    /// the `pB2` (diagonal) / `pR2` (orthogonal) component of the promoted cannons.
    fn short_hop(
        sq: Square<Shogi9x9>,
        occ: Bitboard<Shogi9x9>,
        dirs: &[(i8, i8)],
    ) -> Bitboard<Shogi9x9> {
        let mut bb = Bitboard::<Shogi9x9>::EMPTY;
        for &(df, dr) in dirs {
            let Some(screen) = sq.offset(df, dr) else {
                continue;
            };
            if !occ.contains(screen) {
                continue;
            }
            if let Some(target) = screen.offset(df, dr) {
                bb.set(target);
            }
        }
        bb
    }

    /// The promoted Cannon / Rook-cannon (`+U` / `+A`, `mRpRmFpB2`) capture-and-slide
    /// set: a full rook line (quiet slide *and* unlimited over-screen hop) plus the
    /// range-2 diagonal hop. The one-step diagonal **quiet** move (`mF`) is a
    /// move-only step, emitted from [`quiet_only_targets`](CannonShogiRules::quiet_only_targets).
    fn promoted_rook_cannon_attacks(
        sq: Square<Shogi9x9>,
        occ: Bitboard<Shogi9x9>,
    ) -> Bitboard<Shogi9x9> {
        Self::cannon_attacks(sq, occ)
            | attacks::janggi_cannon_quiet::<Shogi9x9>(sq, occ, Bitboard::EMPTY)
            | Self::short_hop(sq, occ, &FERZ_OFFSETS)
    }

    /// The promoted Bishop-cannon / Bishop-hopper (`+C` / `+I`, `mBpBmWpR2`)
    /// capture-and-slide set: a full bishop line (quiet slide *and* unlimited
    /// over-screen hop) plus the range-2 orthogonal hop. The one-step orthogonal
    /// **quiet** move (`mW`) is emitted from `quiet_only_targets`.
    fn promoted_bishop_cannon_attacks(
        sq: Square<Shogi9x9>,
        occ: Bitboard<Shogi9x9>,
    ) -> Bitboard<Shogi9x9> {
        Self::bishop_cannon_attacks(sq, occ)
            | attacks::diag_cannon_quiet_jumps::<Shogi9x9>(sq, occ)
            | Self::short_hop(sq, occ, &WAZIR_OFFSETS)
    }

    /// The last rank for `color` (rank 8 white / rank 0 black) — a Lance there has
    /// no further move (forced promotion / no drop).
    fn last_rank(color: Color) -> u8 {
        match color {
            Color::White => Shogi9x9::HEIGHT - 1,
            Color::Black => 0,
        }
    }

    /// `true` if `rank` is in the last two ranks for `color` — a Knight there has
    /// no further move.
    fn in_last_two(color: Color, rank: u8) -> bool {
        match color {
            Color::White => rank >= Shogi9x9::HEIGHT - 2,
            Color::Black => rank <= 1,
        }
    }

    /// The mask of every square on `rank`.
    fn rank_mask(rank: u8) -> Bitboard<Shogi9x9> {
        let mut bb = Bitboard::<Shogi9x9>::EMPTY;
        for file in 0..Shogi9x9::WIDTH {
            if let Some(sq) = Square::<Shogi9x9>::from_file_rank(file, rank) {
                bb.set(sq);
            }
        }
        bb
    }

    /// The mask of the last two ranks for `color` (where a Knight has no move).
    fn last_two_mask(color: Color) -> Bitboard<Shogi9x9> {
        let (a, b) = match color {
            Color::White => (Shogi9x9::HEIGHT - 1, Shogi9x9::HEIGHT - 2),
            Color::Black => (0, 1),
        };
        Self::rank_mask(a) | Self::rank_mask(b)
    }
}

impl WideVariant<Shogi9x9> for CannonShogiRules {
    /// The tightest prefix of [`WideRole::ALL`] that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 67;

    fn starting_position() -> (Board<Shogi9x9>, GenericState<Shogi9x9>) {
        let board = Board::<Shogi9x9>::from_fen_placement(CANNONSHOGI_PLACEMENT)
            .expect("the Cannon Shogi starting placement is valid on a 9x9 board");
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::NONE,
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

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Shogi9x9>,
        occupancy: Bitboard<Shogi9x9>,
    ) -> Bitboard<Shogi9x9> {
        match role {
            // The Soldier replaces the Shogi forward-only Pawn with a forward/
            // sideways stepper.
            WideRole::Pawn => Self::soldier_attacks(color, sq),
            // The five cannon-type movers and their promoted forms. Every set is
            // occupancy-only (Cannon Shogi cannons hop over and capture *any* piece,
            // with no Janggi-style cannon restriction), so the plain `role_attacks`
            // computes the full move-and-attack set; `has_cannons` routes king
            // safety through the per-move verify path.
            WideRole::Cannon => Self::cannon_attacks(sq, occupancy),
            WideRole::RookCannon => Self::rook_cannon_attacks(sq, occupancy),
            WideRole::BishopCannon => Self::bishop_cannon_attacks(sq, occupancy),
            WideRole::BishopHopper => Self::bishop_hopper_attacks(sq, occupancy),
            WideRole::PromotedCannon | WideRole::PromotedRookCannon => {
                Self::promoted_rook_cannon_attacks(sq, occupancy)
            }
            WideRole::PromotedBishopCannon | WideRole::PromotedBishopHopper => {
                Self::promoted_bishop_cannon_attacks(sq, occupancy)
            }
            // Every Shogi piece (King, Gold, Silver, Knight, Lance, Rook, Bishop,
            // Dragon, Dragon Horse, and the Gold-moving promoted minors) is exactly
            // the Shogi mover.
            _ => ShogiRules::role_attacks(role, color, sq, occupancy),
        }
    }

    fn quiet_only_targets(
        role: WideRole,
        _color: Color,
        sq: Square<Shogi9x9>,
        occupancy: Bitboard<Shogi9x9>,
    ) -> Bitboard<Shogi9x9> {
        // The promoted cannons have a one-step **move-only** component (a piece they
        // may step onto but never capture): the diagonal Ferz step (`mF`) of the
        // promoted rook-cannons and the orthogonal Wazir step (`mW`) of the promoted
        // bishop-cannons. These are emitted as quiet-only moves (the generator masks
        // them to empty squares), so they never appear in the attack relation and a
        // friendly/enemy piece on such a square is never captured there. Every other
        // role (and the Shogi pieces) has no quiet-only step.
        let _ = occupancy;
        match role {
            WideRole::PromotedCannon | WideRole::PromotedRookCannon => {
                attacks::leaper_attacks::<Shogi9x9>(sq, &FERZ_OFFSETS)
            }
            WideRole::PromotedBishopCannon | WideRole::PromotedBishopHopper => {
                attacks::leaper_attacks::<Shogi9x9>(sq, &WAZIR_OFFSETS)
            }
            _ => Bitboard::EMPTY,
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // The Soldier is forward-biased (its forward step is colour-directional; the
        // two sideways steps are symmetric, so the reverse projection with the
        // opposite colour still recovers it). Every other forward-biased Shogi piece
        // (Silver, Gold, Knight, Lance, and the Gold-moving promoted minors) keeps
        // the Shogi classification; the cannons are colour-symmetric.
        ShogiRules::role_attack_is_directional(role)
    }

    fn role_attack_is_leg_asymmetric(role: WideRole) -> bool {
        // Every cannon-type mover's attack set depends on a screen under the live
        // occupancy, so it is not reverse-projectable: `attackers_to` must
        // forward-project it from each origin (exactly as the generator computes it),
        // and the per-move verify path re-tests king safety after each move. (The
        // Cannon Shogi cannons hop over / capture any piece, so the set is
        // occupancy-only — no piece-type board hook is needed.)
        matches!(
            role,
            WideRole::Cannon
                | WideRole::RookCannon
                | WideRole::BishopCannon
                | WideRole::BishopHopper
                | WideRole::PromotedCannon
                | WideRole::PromotedRookCannon
                | WideRole::PromotedBishopCannon
                | WideRole::PromotedBishopHopper
        )
    }

    fn role_is_slider(role: WideRole) -> bool {
        // The plain line sliders (Rook, Bishop, their promoted Dragon / Dragon Horse,
        // and the forward-sliding Lance) pin and are pinned along a ray. The cannons'
        // slide is screen-gated, so they are not pin-classified — Cannon Shogi runs
        // the per-move verify path (`has_cannons`), which re-checks king safety
        // directly rather than via pins.
        matches!(
            role,
            WideRole::Rook
                | WideRole::Bishop
                | WideRole::Dragon
                | WideRole::DragonHorse
                | WideRole::Lance
        )
    }

    fn promotion_config() -> PromotionConfig {
        // Per-piece promotions (each base role has one promoted form, handled by the
        // generic per-piece promotion path); this static set is unused but required.
        PromotionConfig {
            roles: alloc::vec![WideRole::Gold],
        }
    }

    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        match color {
            Color::White => rank >= Shogi9x9::HEIGHT - ZONE_DEPTH,
            Color::Black => rank < ZONE_DEPTH,
        }
    }

    fn has_castling() -> bool {
        false
    }

    fn has_cannons() -> bool {
        // Cannon Shogi fields five cannon-type movers (and their promoted forms),
        // whose screen-dependent captures and king checks need the pseudo-legal +
        // per-move-verify king-safety path, exactly as Xiangqi / Janggi. This is the
        // first variant to combine that path with a hand (the verify path generates
        // and re-verifies hand drops alongside board moves).
        true
    }

    // --- hand / drops + per-piece promotion -------------------------------

    fn has_hand() -> bool {
        true
    }

    fn role_can_promote(role: WideRole) -> bool {
        // The promotable base pieces; Gold and King never promote, and a piece
        // already promoted does not promote again.
        matches!(
            role,
            WideRole::Pawn
                | WideRole::Lance
                | WideRole::Knight
                | WideRole::Silver
                | WideRole::Rook
                | WideRole::Bishop
                | WideRole::Cannon
                | WideRole::RookCannon
                | WideRole::BishopCannon
                | WideRole::BishopHopper
        )
    }

    fn role_promoted_to(role: WideRole) -> WideRole {
        // The four cannon promotions are distinct roles that share two movements but
        // keep their base identity for hand reversion; the Shogi pieces use the
        // global Shogi promoted-form mapping (Pawn → Tokin, Rook → Dragon, …).
        match role {
            WideRole::Cannon => WideRole::PromotedCannon,
            WideRole::RookCannon => WideRole::PromotedRookCannon,
            WideRole::BishopCannon => WideRole::PromotedBishopCannon,
            WideRole::BishopHopper => WideRole::PromotedBishopHopper,
            other => other.promoted_form(),
        }
    }

    fn role_hand_base(role: WideRole) -> WideRole {
        // A captured promoted cannon sheds its promotion before entering the hand,
        // reverting to its distinct base (`+U → u`, `+A → a`, `+C → c`, `+I → i`),
        // matching FSF. Every Shogi promoted form (and base) banks via the global
        // Shogi mapping.
        match role {
            WideRole::PromotedCannon => WideRole::Cannon,
            WideRole::PromotedRookCannon => WideRole::RookCannon,
            WideRole::PromotedBishopCannon => WideRole::BishopCannon,
            WideRole::PromotedBishopHopper => WideRole::BishopHopper,
            other => other.promoted_base(),
        }
    }

    fn role_promotion_forced(role: WideRole, color: Color, to_rank: u8) -> bool {
        // Only a Lance (last rank) and a Knight (last two ranks) have no further move
        // and so must promote. The Soldier always has a sideways move, so — unlike
        // the Shogi Pawn — it is never force-promoted; the cannons can always slide
        // or hop, so they never are either.
        match role {
            WideRole::Lance => to_rank == Self::last_rank(color),
            WideRole::Knight => Self::in_last_two(color, to_rank),
            _ => false,
        }
    }

    fn drop_targets(role: WideRole, color: Color, board: &Board<Shogi9x9>) -> Bitboard<Shogi9x9> {
        let mut mask = !board.occupied();
        // Dead-piece rule: a dropped Lance may not land on the last rank, nor a
        // Knight on the last two ranks (it would then have no move). The Soldier has
        // a sideways move on every rank, so it has no last-rank restriction, and
        // Cannon Shogi has **no** *nifu* (FSF `dropNoDoubled = -`): a Soldier may be
        // doubled on a file. The cannons can always move, so they drop anywhere.
        match role {
            WideRole::Lance => {
                mask &= !Self::rank_mask(Self::last_rank(color));
            }
            WideRole::Knight => {
                mask &= !Self::last_two_mask(color);
            }
            _ => {}
        }
        mask
    }

    // --- Sennichite / perpetual check (default-off draw rules) -------------
    //
    // These affect only terminal adjudication in [`GenericGame`], never move
    // generation, so perft is byte-identical.

    fn tracks_repetition() -> bool {
        true
    }

    fn repetition_fold() -> usize {
        // Sennichite: the same position (including both hands) occurring a fourth
        // time is a draw.
        4
    }

    fn repetition_draw_reason() -> crate::geometry::WideEndReason {
        crate::geometry::WideEndReason::Sennichite
    }

    fn perpetual_check_loses() -> bool {
        // A sennichite brought about by perpetual check is a loss for the checking
        // side.
        true
    }
}

/// Cannon Shogi (大砲将棋) as a [`GenericPosition`] over the 9x9 geometry.
///
/// Construct the starting position with
/// [`CannonShogi::startpos`](GenericPosition::startpos) or parse a FEN — the
/// placement may carry the hand as a `[..]` holdings bracket — with
/// [`CannonShogi::from_fen`](GenericPosition::from_fen). See the [module
/// docs](self) for the cannon army, the hand, drops, and the promotion zone.
pub type CannonShogi = GenericPosition<Shogi9x9, CannonShogiRules>;
