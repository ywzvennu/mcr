//! Dobutsu (どうぶつしょうぎ, "animal shogi", #233) on the generic engine — a
//! 3x4 educational shogi variant with **drops**, a non-royal **Lion**, and a
//! **try** (flag) win. It reuses the Shogi (#190) / Minishogi (#195) persistent
//! **hand**, **drops**, and far-rank **promotion** machinery on a new
//! three-by-four (12-square) [`Dobutsu3x4`] geometry, plus the unified flag-win
//! hooks with the added [`flag_win_requires_safe`](WideVariant::flag_win_requires_safe)
//! "safe" condition. Validated against Fairy-Stockfish `UCI_Variant dobutsu`.
//!
//! ## Pieces (confirmed against FSF; the captor's hand demotes a Hen to a Chick)
//!
//! One of each per side at the start (the Hen appears only by promotion):
//!
//! * **Lion (L, ライオン)** — a king-stepping piece: one square in any of the
//!   eight directions. Unlike a Shogi King it is **non-royal** — there is no
//!   check, and a side may step its Lion into capture or leave it attacked. A
//!   side **loses** when its Lion is captured (extinction), or the **opponent**
//!   wins by the try rule below. Modelled as the [`WideRole::King`] role with an
//!   **empty royal set**, so the generic king-safety machinery never filters a
//!   self-check.
//! * **Jiraffe (G, キリン)** — a **wazir**: one step orthogonally (four
//!   directions). The [`WideRole::Wazir`] overflow role (FEN token `*j`).
//! * **Elephant (E, ゾウ)** — a **ferz**: one step diagonally (four directions).
//!   The [`WideRole::Met`] role (`m`).
//! * **Chick (C, ひよこ → Hen)** — one step straight **forward** (it both moves
//!   and captures there, like a Shogi pawn). On reaching the far rank it is
//!   **forced** to promote to a **Hen**. The [`WideRole::Pawn`] role (`p`).
//! * **Hen (H, にわとり, `+C`)** — the promoted Chick. Moves as a **Jold
//!   General** — one step orthogonally (four directions) or one step diagonally
//!   **forward** (two directions): six squares. The [`WideRole::Tokin`] promoted
//!   role (`+p`); captured, it reverts to a Chick in the captor's hand.
//!
//! ## Promotion zone
//!
//! The promotion zone is the **furthest rank only** (rank 4 / 0-based 3 for
//! White, rank 1 / 0-based 0 for Black). A Chick reaching it is **forced** to
//! promote (it would otherwise have no further move) — confirmed against FSF.
//! Only the Chick promotes; the Lion, Giraffe, and Elephant never promote, and a
//! dropped or already-promoted piece does not promote.
//!
//! ## Hand and drops
//!
//! A captured piece is banked **unpromoted** (a captured Hen enters the hand as a
//! Chick) and flipped to the captor's side. On a turn a side may, instead of a
//! board move, **drop** a held piece onto **any empty square** — with **no**
//! restrictions: a Chick may be dropped on the last rank (FSF `immobilityIllegal
//! = false`, so an immobile dropped Chick there is legal), and there is **no
//! nifu** (FSF `dropNoDoubled = -`), so the trait-default [`drop_targets`](crate::geometry::WideVariant::drop_targets) (all
//! empty squares) applies unchanged. A dropped piece is always unpromoted.
//!
//! ## Try / flag win
//!
//! A side wins the instant its **Lion reaches the far rank** (rank 4 for White,
//! rank 1 for Black) **and is safe there** — i.e. not attacked by the opponent
//! (FSF `flagPiece = l`, `flagRegion = far rank`, `flagPieceSafe = true`). A Lion
//! that reaches the far rank but **can be captured** has **not** won: the game
//! continues and the opponent may capture it. This is the unified flag-win hook
//! ([`has_flag_win`](WideVariant::has_flag_win)) with the
//! [`flag_win_requires_safe`](WideVariant::flag_win_requires_safe) condition; FSF
//! adjudicates it on the **loser's** turn, so a node where the side to move's
//! opponent already stands safely on its goal rank is terminal (no children),
//! which is what makes mcr's perft match FSF's `go perft` at a try node.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant dobutsu`, `position startpos`) renders the start as
//!
//! ```text
//! gle/1c1/1C1/ELG[-] w 0 1
//! ```
//!
//! with FSF's letters `g l e c` (Giraffe, Lion, Elephant, Chick). mcr reuses the
//! Lion as a King (`k`), the Chick as a Pawn (`p`), the Elephant as a Met (`m`),
//! and the Giraffe as the Wazir overflow role (`*j`):
//!
//! ```text
//! *jkm/1p1/1P1/MK*J[] w - - 0 1
//! ```
//!
//! The two FENs are the same position; the `compare-fairy/` harness translates the
//! mcr letters (and the empty `[]` holdings bracket) when driving FSF.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Geometry, PromotionConfig, Square, WideRole, WideVariant,
};
use crate::Color;

use super::super::Dobutsu3x4;

/// The Dobutsu rule layer: a zero-sized [`WideVariant`] over [`Dobutsu3x4`].
///
/// It overrides the animal-piece movements (Lion / Giraffe / Elephant / Chick →
/// Hen), the non-royal Lion (an empty royal set, so there is no check), the
/// Shogi hand / drops / forced Chick promotion, and the safe-try flag win.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DobutsuRules;

/// The confirmed Dobutsu starting placement in mcr's role letters: the empty hand
/// is carried by the `[]` bracket parsed separately. White's back rank is Elephant
/// (Met `m`), Lion (King `k`), Giraffe (Wazir `*j`); its Chick (Pawn `p`) sits on
/// b2. Black mirrors it. Matches FSF's `gle/1c1/1C1/ELG`.
const DOBUTSU_PLACEMENT: &str = "*jkm/1p1/1P1/MK*J";

/// The four diagonal one-step (ferz) offsets — the Elephant's move set.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// The four orthogonal one-step (wazir) offsets — the Giraffe's move set.
const WAZIR_OFFSETS: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

impl DobutsuRules {
    /// The Hen's (promoted Chick's) attack/move set: a **Jold General** — one step
    /// orthogonally (four directions) plus one step diagonally **forward** (two
    /// directions), six squares. Color-directional (the forward diagonals flip
    /// with the side), so it is also listed in
    /// [`role_attack_is_directional`](WideVariant::role_attack_is_directional).
    fn gold_attacks(color: Color, sq: Square<Dobutsu3x4>) -> Bitboard<Dobutsu3x4> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        let offsets = [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            // The two forward diagonals.
            (1, fwd),
            (-1, fwd),
        ];
        attacks::leaper_attacks::<Dobutsu3x4>(sq, &offsets)
    }

    /// The Chick's attack/movement square: the single square straight forward (it
    /// both moves and captures there, like a Shogi pawn).
    fn chick_attacks(color: Color, sq: Square<Dobutsu3x4>) -> Bitboard<Dobutsu3x4> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = Bitboard::<Dobutsu3x4>::EMPTY;
        if let Some(dest) = sq.offset(0, fwd) {
            bb.set(dest);
        }
        bb
    }

    /// The last (far) rank for `color` — rank 3 white / rank 0 black. It is both
    /// the Chick's forced-promotion rank and the Lion's flag (try) goal rank.
    fn last_rank(color: Color) -> u8 {
        match color {
            Color::White => Dobutsu3x4::HEIGHT - 1,
            Color::Black => 0,
        }
    }
}

impl WideVariant<Dobutsu3x4> for DobutsuRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 24;

    fn starting_position() -> (Board<Dobutsu3x4>, GenericState<Dobutsu3x4>) {
        let board = Board::<Dobutsu3x4>::from_fen_placement(DOBUTSU_PLACEMENT)
            .expect("the Dobutsu starting placement is valid on a 3x4 board");
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
            board_b: crate::geometry::Bitboard::EMPTY,
        };
        (board, state)
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Dobutsu3x4>,
        _occupancy: Bitboard<Dobutsu3x4>,
    ) -> Bitboard<Dobutsu3x4> {
        match role {
            // Chick: one step straight forward (moves and captures there).
            WideRole::Pawn => Self::chick_attacks(color, sq),
            // Hen (promoted Chick): moves as a Gold General.
            WideRole::Tokin => Self::gold_attacks(color, sq),
            // Elephant: a ferz — one step in each diagonal.
            WideRole::Met => attacks::leaper_attacks::<Dobutsu3x4>(sq, &FERZ_OFFSETS),
            // Giraffe: a wazir — one step in each orthogonal.
            WideRole::Wazir => attacks::leaper_attacks::<Dobutsu3x4>(sq, &WAZIR_OFFSETS),
            // Lion: a king-stepper (non-royal; see `royal_squares`).
            WideRole::King => attacks::king_attacks::<Dobutsu3x4>(sq),
            _ => Bitboard::EMPTY,
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // The forward-biased pieces: the Chick (forward move/capture) and the Hen
        // (Gold General's forward diagonals). Their attack sets point forward, so
        // the attacker scan reverse-projects the opposite color from the target.
        // The Lion (king), Giraffe (wazir), and Elephant (ferz) are
        // color-symmetric and not directional.
        matches!(role, WideRole::Pawn | WideRole::Tokin)
    }

    fn role_is_slider(_role: WideRole) -> bool {
        // Every Dobutsu piece is a one-step leaper / stepper; none slides, so none
        // can pin or be pinned along a ray.
        false
    }

    fn promotion_config() -> PromotionConfig {
        // Dobutsu's only promotion is per-piece (Chick → Hen, handled by the
        // generic per-piece promotion path); this static set is unused, but the
        // trait requires it.
        PromotionConfig {
            roles: alloc::vec![WideRole::Tokin],
        }
    }

    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        // The zone is the single furthest rank.
        rank == Self::last_rank(color)
    }

    fn has_castling() -> bool {
        false
    }

    // --- hand / drops + per-piece promotion -------------------------------

    fn has_hand() -> bool {
        true
    }

    fn role_can_promote(role: WideRole) -> bool {
        // Only the Chick promotes (to a Hen); the Lion, Giraffe, and Elephant
        // never promote, and a piece already promoted does not promote again.
        matches!(role, WideRole::Pawn)
    }

    fn role_promotion_forced(role: WideRole, color: Color, to_rank: u8) -> bool {
        match role {
            // A Chick on the far rank has no further move, so its promotion is
            // forced (confirmed against FSF: the only move is `...+`).
            WideRole::Pawn => to_rank == Self::last_rank(color),
            _ => false,
        }
    }

    // The trait-default `drop_targets` (every empty square) is exactly Dobutsu's
    // rule: a Chick may be dropped on the last rank (FSF `immobilityIllegal =
    // false`) and there is no nifu (FSF `dropNoDoubled = -`), so no override.

    // --- non-royal Lion ---------------------------------------------------

    fn non_royal_king() -> bool {
        // The Lion is non-royal: a side loses by extinction (its Lion captured) or
        // by the opponent's try, never by checkmate. This routes the hand path
        // through the non-royal branch (every pseudo-legal move and drop is legal).
        true
    }

    fn royal_squares(_board: &Board<Dobutsu3x4>, _color: Color) -> Bitboard<Dobutsu3x4> {
        // The Lion is **not royal**: there is no check, pin, or self-check filter.
        // An empty royal set makes the generic king-safety machinery report "never
        // in check". A side loses by extinction (its Lion captured) or by the
        // opponent's try.
        Bitboard::EMPTY
    }

    // --- try / flag win ---------------------------------------------------

    fn has_flag_win() -> bool {
        true
    }

    fn flag_win_requires_safe() -> bool {
        // The try wins only when the Lion on the far rank is **safe** (unattacked):
        // a capturable Lion on the goal rank has not won.
        true
    }

    // The flag goal ranks (White's far rank, Black's first) are exactly the
    // generic `flag_rank` default, so Dobutsu does not override it.
}

/// Dobutsu (3x4 animal shogi) as a [`GenericPosition`] over the [`Dobutsu3x4`]
/// geometry.
///
/// Construct the starting position with
/// [`Dobutsu::startpos`](GenericPosition::startpos) or parse a FEN — the placement
/// may carry the hand as a `[..]` holdings bracket — with
/// [`Dobutsu::from_fen`](GenericPosition::from_fen). See the [module docs](self)
/// for the animal moves, the non-royal Lion, the hand / drops, the forced Chick
/// promotion, and the safe-try flag win.
pub type Dobutsu = GenericPosition<Dobutsu3x4, DobutsuRules>;
