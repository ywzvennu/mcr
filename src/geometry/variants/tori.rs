//! Tori Shogi (bird shogi, 7x7) on the generic engine — a Shogi-family variant
//! with a **persistent hand**, **drops**, and **per-piece promotion**, but a
//! seven-bird army in place of the usual Shogi pieces. Validated square-for-square
//! and node-for-node against Fairy-Stockfish `UCI_Variant torishogi`.
//!
//! Tori Shogi reuses every piece of the Shogi machinery proven by [`Shogi`](crate::geometry::Shogi) /
//! [`Minishogi`](super::Minishogi) — a captured piece flips colour and enters the
//! captor's **hand**, from which it may be **dropped** back onto an empty square;
//! a piece **promotes** when a move starts or ends in the far-board zone — but on
//! a 7x7 [`Tori7x7`] board with the bird army below.
//!
//! ## Pieces (confirmed against FSF; promoted forms in parentheses)
//!
//! Directions are given from White's side (forward = up the board). The Black
//! pieces are the vertical mirror.
//!
//! * **Swallow (`*Y`, 燕 → Goose, `*G`)** — one step straight **forward** (it both
//!   moves and captures there, exactly the Shogi pawn). Promotes to a Goose.
//! * **Goose (`*G`)** — the promoted Swallow: leaps two squares diagonally
//!   **forward** (a forward Alfil, jumping the middle square) or two squares
//!   straight **backward** (a backward Dabbaba jump). Three target squares.
//! * **Falcon (`*A`, 鷹 → Eagle, `*I`)** — the four diagonal steps (Ferz) plus one
//!   step **forward** or **sideways** orthogonally — every King step except the
//!   backward orthogonal one (seven squares). Promotes to an Eagle.
//! * **Eagle (`*I`)** — the promoted Falcon: a King step in every direction, a
//!   **backward** Rook slide, a **forward** Bishop slide, and a **backward**
//!   diagonal slide of up to two squares.
//! * **Crane (`*K`, 鶴)** — the four diagonal steps (Ferz) plus one step straight
//!   **forward** or **backward** orthogonally — every King step except the two
//!   sideways ones (six squares). Never promotes.
//! * **Left Quail (`*V`, 鶉)** — an **asymmetric** bird: a **forward** Rook slide,
//!   a **right-backward** Bishop slide, and one step **left-backward** diagonally.
//!   Not left-right symmetric (it is the mirror of the Right Quail).
//! * **Right Quail (`*R`, 鶉)** — the mirror of the Left Quail: a **forward** Rook
//!   slide, a **left-backward** Bishop slide, and one step **right-backward**
//!   diagonally.
//! * **Pheasant (`*Z`, 雉)** — leaps two squares straight **forward** (a forward
//!   Dabbaba jump) and steps one square **backward** diagonally (a backward Ferz).
//!   Three target squares. Never promotes.
//! * **King (K, 玉)** — a standard king.
//!
//! ## Promotion zone
//!
//! The promotion zone is the **furthest two ranks** from each side: ranks 6-7
//! (0-based 5-6) for White, ranks 1-2 (0-based 0-1) for Black. Promotion is
//! **mandatory** (FSF `mandatoryPiecePromotion = true`): a move that starts or
//! ends in the zone always promotes — there is no non-promoting alternative. Only
//! the Swallow and the Falcon promote (to Goose / Eagle); every other bird, and a
//! dropped or already-promoted piece, does not.
//!
//! ## Hand and drops
//!
//! A captured piece is banked **unpromoted** (a captured Goose enters the hand as
//! a Swallow, a captured Eagle as a Falcon) and flipped to the captor's side. On a
//! turn a side may, instead of a board move, **drop** a held piece onto any empty
//! square, subject to the Swallow drop rules (FSF `dropNoDoubled = s`,
//! `dropNoDoubledCount = 2`, `shogiPawnDropMateIllegal = true`):
//!
//! 1. **Dead-piece** — a Swallow may not be dropped on the last rank (it would
//!    then have no move).
//! 2. **No more than two Swallows per file** — a Swallow may not be dropped onto a
//!    file that already holds **two** friendly (unpromoted) Swallows.
//! 3. A dropped piece is always unpromoted.
//!
//! The Swallow-drop-mate rule (FSF `shogiPawnDropMateIllegal`) is **not** applied
//! here: like Shogi against FSF (see [`Shogi`](crate::geometry::Shogi)), FSF's `torishogi` move generation
//! lists a mating swallow drop in its `go perft` divide, so mcr matches FSF and
//! leaves [`pawn_drop_mate_forbidden`](WideVariant::pawn_drop_mate_forbidden) at
//! its `false` default.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant torishogi`, `position startpos`) renders the start as
//!
//! ```text
//! rpckcpl/3f3/sssssss/2s1S2/SSSSSSS/3F3/LPCKCPR[-] w 0 1
//! ```
//!
//! In mcr's overflow spelling (each bird is a `*`-prefixed overflow role) this is
//! the `TORI_PLACEMENT` below; the `compare-fairy/` harness rewrites each
//! `*<base>` token to FSF's letter when driving FSF.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Geometry, PromotionConfig, Square, WideRole, WideVariant,
};
use crate::Color;

use super::super::Tori7x7;

/// The Tori Shogi rule layer: a zero-sized [`WideVariant`] over [`Tori7x7`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ToriRules;

/// The confirmed Tori Shogi starting placement (the hand is empty at the start),
/// in mcr's overflow spelling of the FSF start `rpckcpl/3f3/sssssss/2s1S2/SSSSSSS/3F3/LPCKCPR`.
const TORI_PLACEMENT: &str =
    "*r*z*kk*k*z*v/3*a3/*y*y*y*y*y*y*y/2*y1*Y2/*Y*Y*Y*Y*Y*Y*Y/3*A3/*V*Z*KK*K*Z*R";

/// The depth of the promotion zone: the furthest two ranks from each side.
const ZONE_DEPTH: u8 = 2;

/// Rotates a `(file, rank)` step from White's orientation into `color`'s: White
/// keeps it, Black takes its **180° rotation** (both axes negated). Every Tori
/// bird's Black move set is the 180° rotation of its White set — *not* a mere
/// vertical flip — because the asymmetric quails are not left-right symmetric (a
/// Black Left Quail's right-backward Bishop slide runs up-**left** on the board,
/// the full point-reflection of the White slide). For the left-right-symmetric
/// birds the two transforms coincide, so this is correct for all of them.
const fn orient((df, dr): (i8, i8), color: Color) -> (i8, i8) {
    match color {
        Color::White => (df, dr),
        Color::Black => (-df, -dr),
    }
}

impl ToriRules {
    /// The leaper attack set for a piece whose **White-orientation** step offsets
    /// are `white_offsets`, oriented for `color` (180°-rotated for Black).
    fn oriented_leaper(
        color: Color,
        sq: Square<Tori7x7>,
        white_offsets: &[(i8, i8)],
    ) -> Bitboard<Tori7x7> {
        let mut bb = Bitboard::<Tori7x7>::EMPTY;
        for &off in white_offsets {
            let (df, dr) = orient(off, color);
            if let Some(dest) = sq.offset(df, dr) {
                bb.set(dest);
            }
        }
        bb
    }

    /// Slides from `sq` along each **White-orientation** direction in `white_dirs`
    /// (oriented for `color`) for at most `max` squares, stopping on the first
    /// blocker (a capture). `max == 0` means unlimited (a full rider).
    fn oriented_ray(
        color: Color,
        sq: Square<Tori7x7>,
        occupancy: Bitboard<Tori7x7>,
        white_dirs: &[(i8, i8)],
        max: u8,
    ) -> Bitboard<Tori7x7> {
        let mut bb = Bitboard::<Tori7x7>::EMPTY;
        for &dir in white_dirs {
            let (df, dr) = orient(dir, color);
            let mut cur = sq.offset(df, dr);
            let mut steps = 0u8;
            while let Some(dest) = cur {
                bb.set(dest);
                steps += 1;
                if (max != 0 && steps >= max) || occupancy.contains(dest) {
                    break;
                }
                cur = dest.offset(df, dr);
            }
        }
        bb
    }

    /// Falcon (`FsfW`): the four diagonals plus forward / left / right orthogonal —
    /// every King step except the backward orthogonal one.
    fn falcon_attacks(color: Color, sq: Square<Tori7x7>) -> Bitboard<Tori7x7> {
        const WHITE: [(i8, i8); 7] = [
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1), // four diagonals (Ferz)
            (0, 1),   // forward orthogonal
            (1, 0),
            (-1, 0), // sideways orthogonal
        ];
        Self::oriented_leaper(color, sq, &WHITE)
    }

    /// Crane (`FvW`): the four diagonals plus forward / backward orthogonal — every
    /// King step except the two sideways ones. (Left-right symmetric.)
    fn crane_attacks(color: Color, sq: Square<Tori7x7>) -> Bitboard<Tori7x7> {
        const WHITE: [(i8, i8); 6] = [
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1), // four diagonals (Ferz)
            (0, 1),
            (0, -1), // forward / backward orthogonal (vertical Wazir)
        ];
        Self::oriented_leaper(color, sq, &WHITE)
    }

    /// Pheasant (`bFfD`): a forward Dabbaba jump (two straight ahead) plus the two
    /// backward diagonal steps.
    fn pheasant_attacks(color: Color, sq: Square<Tori7x7>) -> Bitboard<Tori7x7> {
        const WHITE: [(i8, i8); 3] = [
            (0, 2), // forward Dabbaba (jumps the middle square)
            (1, -1),
            (-1, -1), // backward diagonals (backward Ferz)
        ];
        Self::oriented_leaper(color, sq, &WHITE)
    }

    /// Goose (`fAbD`, promoted Swallow): a forward Alfil (two-diagonal jump) plus a
    /// backward Dabbaba jump.
    fn goose_attacks(color: Color, sq: Square<Tori7x7>) -> Bitboard<Tori7x7> {
        const WHITE: [(i8, i8); 3] = [
            (2, 2),
            (-2, 2), // forward Alfil
            (0, -2), // backward Dabbaba
        ];
        Self::oriented_leaper(color, sq, &WHITE)
    }

    /// Eagle (`KbRfBbF2`, promoted Falcon): a full King, a backward Rook slide, a
    /// forward Bishop slide, and a backward diagonal slide of up to two squares.
    fn eagle_attacks(
        color: Color,
        sq: Square<Tori7x7>,
        occupancy: Bitboard<Tori7x7>,
    ) -> Bitboard<Tori7x7> {
        let mut bb = attacks::king_attacks::<Tori7x7>(sq);
        // Backward Rook slide.
        bb |= Self::oriented_ray(color, sq, occupancy, &[(0, -1)], 0);
        // Forward Bishop slide (both forward diagonals).
        bb |= Self::oriented_ray(color, sq, occupancy, &[(1, 1), (-1, 1)], 0);
        // Backward diagonal slide of up to two squares.
        bb |= Self::oriented_ray(color, sq, occupancy, &[(1, -1), (-1, -1)], 2);
        bb
    }

    /// Left Quail (`fRrbBlbF`): a forward Rook slide, a right-backward Bishop
    /// slide, and one left-backward diagonal step.
    fn left_quail_attacks(
        color: Color,
        sq: Square<Tori7x7>,
        occupancy: Bitboard<Tori7x7>,
    ) -> Bitboard<Tori7x7> {
        // Forward Rook slide + right-backward Bishop slide (unlimited).
        let mut bb = Self::oriented_ray(color, sq, occupancy, &[(0, 1), (1, -1)], 0);
        // One left-backward diagonal step.
        bb |= Self::oriented_leaper(color, sq, &[(-1, -1)]);
        bb
    }

    /// Right Quail (`fRlbBrbF`): the mirror of the Left Quail — a forward Rook
    /// slide, a left-backward Bishop slide, and one right-backward diagonal step.
    fn right_quail_attacks(
        color: Color,
        sq: Square<Tori7x7>,
        occupancy: Bitboard<Tori7x7>,
    ) -> Bitboard<Tori7x7> {
        // Forward Rook slide + left-backward Bishop slide (unlimited).
        let mut bb = Self::oriented_ray(color, sq, occupancy, &[(0, 1), (-1, -1)], 0);
        // One right-backward diagonal step.
        bb |= Self::oriented_leaper(color, sq, &[(1, -1)]);
        bb
    }

    /// The Swallow's attack/movement square: the single square straight forward (it
    /// both moves and captures there, like the Shogi pawn).
    fn swallow_attacks(color: Color, sq: Square<Tori7x7>) -> Bitboard<Tori7x7> {
        Self::oriented_leaper(color, sq, &[(0, 1)])
    }

    /// The last rank for `color` (rank 6 white / rank 0 black) — a Swallow there has
    /// no further move (so it cannot be dropped there).
    fn last_rank(color: Color) -> u8 {
        match color {
            Color::White => Tori7x7::HEIGHT - 1,
            Color::Black => 0,
        }
    }

    /// The mask of every square on `rank`.
    fn rank_mask(rank: u8) -> Bitboard<Tori7x7> {
        let mut bb = Bitboard::<Tori7x7>::EMPTY;
        for file in 0..Tori7x7::WIDTH {
            if let Some(sq) = Square::<Tori7x7>::from_file_rank(file, rank) {
                bb.set(sq);
            }
        }
        bb
    }

    /// The mask of every square on `file`.
    fn file_mask(file: u8) -> Bitboard<Tori7x7> {
        let mut bb = Bitboard::<Tori7x7>::EMPTY;
        for rank in 0..Tori7x7::HEIGHT {
            if let Some(sq) = Square::<Tori7x7>::from_file_rank(file, rank) {
                bb.set(sq);
            }
        }
        bb
    }
}

impl WideVariant<Tori7x7> for ToriRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 57;

    fn starting_position() -> (Board<Tori7x7>, GenericState<Tori7x7>) {
        let board = Board::<Tori7x7>::from_fen_placement(TORI_PLACEMENT)
            .expect("the Tori Shogi starting placement is valid on a 7x7 board");
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
            petrified: crate::geometry::Bitboard::EMPTY,
        };
        (board, state)
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Tori7x7>,
        occupancy: Bitboard<Tori7x7>,
    ) -> Bitboard<Tori7x7> {
        match role {
            WideRole::Swallow => Self::swallow_attacks(color, sq),
            WideRole::Goose => Self::goose_attacks(color, sq),
            WideRole::ToriFalcon => Self::falcon_attacks(color, sq),
            WideRole::ToriEagle => Self::eagle_attacks(color, sq, occupancy),
            WideRole::Crane => Self::crane_attacks(color, sq),
            WideRole::LeftQuail => Self::left_quail_attacks(color, sq, occupancy),
            WideRole::RightQuail => Self::right_quail_attacks(color, sq, occupancy),
            WideRole::Pheasant => Self::pheasant_attacks(color, sq),
            WideRole::King => attacks::king_attacks::<Tori7x7>(sq),
            _ => Bitboard::EMPTY,
        }
    }

    fn role_attack_is_leg_asymmetric(role: WideRole) -> bool {
        // Every bird's attack set is geometrically asymmetric (forward-biased, and
        // the quails are not even left-right symmetric), so it is not recovered by
        // reverse-projecting the role's own pattern from the target. Detect such
        // attackers the way the generator does — forward-project from each candidate
        // origin and keep those that hit the square. (The King is symmetric and
        // takes the default reverse-projection path.)
        matches!(
            role,
            WideRole::Swallow
                | WideRole::Goose
                | WideRole::ToriFalcon
                | WideRole::ToriEagle
                | WideRole::Crane
                | WideRole::LeftQuail
                | WideRole::RightQuail
                | WideRole::Pheasant
        )
    }

    fn role_is_slider(role: WideRole) -> bool {
        // The pieces with an unbounded ride that can pin / be pinned along a ray:
        // the Eagle (rook + bishop rays) and both quails (forward lance ray + a
        // backward bishop ray). Every other bird is a leaper.
        matches!(
            role,
            WideRole::ToriEagle | WideRole::LeftQuail | WideRole::RightQuail
        )
    }

    fn confine_pins_to_segment() -> bool {
        // Tori fields *jumping* leapers — the Pheasant (a two-square forward
        // Dabbaba jump) and the Goose (forward Alfil / backward Dabbaba jumps) —
        // that can leap **past** the pinning slider (or past their own king) onto a
        // collinear square that no longer shields the king. The default full-line
        // pin mask would wrongly permit such a jump: e.g. a Pheasant pinned on a
        // file by a forward-sliding Quail behind it can jump two squares forward,
        // over the Quail, vacating the shielding square and exposing the king
        // (issue #416, the illegal `f5f3`). Confining a pinned piece to the
        // king-to-pinner segment (inclusive of the pinner's square) keeps exactly
        // the moves that remain a blocker or capture the pinner. For Tori's sliders
        // (Eagle, quails) the segment and the full line are equivalent, so this is
        // byte-identical for every previously-validated Tori position and touches
        // no other variant.
        true
    }

    fn promotion_config() -> PromotionConfig {
        // Tori's promotions are per-piece (Swallow → Goose, Falcon → Eagle), handled
        // by the generic per-piece promotion path; this static set is unused, but the
        // trait requires it.
        PromotionConfig {
            roles: alloc::vec![WideRole::Goose],
        }
    }

    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        match color {
            Color::White => rank >= Tori7x7::HEIGHT - ZONE_DEPTH,
            Color::Black => rank < ZONE_DEPTH,
        }
    }

    fn has_castling() -> bool {
        false
    }

    // --- hand / drops + per-piece promotion -------------------------------

    fn has_hand() -> bool {
        true
    }

    fn role_can_promote(role: WideRole) -> bool {
        // Only the Swallow and the Falcon promote (to Goose / Eagle); every other
        // bird, the King, and an already-promoted piece never promote.
        matches!(role, WideRole::Swallow | WideRole::ToriFalcon)
    }

    fn role_promoted_to(role: WideRole) -> WideRole {
        match role {
            WideRole::Swallow => WideRole::Goose,
            WideRole::ToriFalcon => WideRole::ToriEagle,
            other => other,
        }
    }

    fn promotion_mandatory_in_zone() -> bool {
        // FSF `mandatoryPiecePromotion = true`: a move that starts or ends in the
        // zone always promotes — there is no non-promoting alternative.
        true
    }

    fn role_hand_base(role: WideRole) -> WideRole {
        // A captured promoted bird sheds its promotion before entering the hand: a
        // Goose banks as a Swallow, an Eagle as a Falcon. Every base bird banks as
        // itself.
        match role {
            WideRole::Goose => WideRole::Swallow,
            WideRole::ToriEagle => WideRole::ToriFalcon,
            other => other,
        }
    }

    fn pawn_drop_role() -> WideRole {
        // The Swallow is Tori's "pawn" for the no-doubled / drop-mate rules.
        WideRole::Swallow
    }

    fn drop_targets(role: WideRole, color: Color, board: &Board<Tori7x7>) -> Bitboard<Tori7x7> {
        let mut mask = !board.occupied();
        if role == WideRole::Swallow {
            // Dead-piece rule: a dropped Swallow may not land on the last rank (it
            // would then have no move).
            mask &= !Self::rank_mask(Self::last_rank(color));
            // No more than two Swallows per file (FSF `dropNoDoubledCount = 2`): a
            // Swallow may not be dropped onto a file that already holds two friendly
            // (unpromoted) Swallows.
            let own_swallows = board.pieces(color, WideRole::Swallow);
            let mut per_file = [0u8; Tori7x7::WIDTH as usize];
            for swallow in own_swallows {
                per_file[swallow.file() as usize] += 1;
            }
            for (file, &count) in per_file.iter().enumerate() {
                if count >= 2 {
                    mask &= !Self::file_mask(file as u8);
                }
            }
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

    fn stalemate_is_loss() -> bool {
        // Stalemate is a loss for the stalemated side (FSF `stalemateValue =
        // -VALUE_MATE`); adjudication only, so perft is byte-identical.
        true
    }
}

/// Tori Shogi (bird shogi) as a [`GenericPosition`] over the 7x7 geometry.
///
/// Construct the starting position with
/// [`Tori::startpos`](GenericPosition::startpos) or parse a FEN — the placement
/// may carry the hand as a `[..]` holdings bracket — with
/// [`Tori::from_fen`](GenericPosition::from_fen). See the [module docs](self) for
/// the bird army, the hand, drops, and the promotion zone.
///
/// [`Shogi`]: super::Shogi
pub type Tori = GenericPosition<Tori7x7, ToriRules>;
