//! Wa Shogi (和将棋, animal shogi, 11x11) on the generic engine — a Shogi-family
//! variant with a **persistent hand**, **drops**, and **per-piece promotion**, but
//! an animal-and-bird army of thirty-one piece kinds in place of the usual Shogi
//! pieces, on the 11x11 [`Washogi11x11`] geometry.
//!
//! ## Oracle: rules-validated (no perft oracle)
//!
//! Wa Shogi is **not** a Fairy-Stockfish variant (FSF's shogi family is
//! checkshogi / euroshogi / kyotoshogi / manchu / minishogi / okisakishogi / shogi
//! / shoshogi / torishogi / yarishogi — Wa is absent), and the only engine that
//! implements it, **HaChu**, has an unreliable perft (its `perft` of standard
//! Shogi already disagrees with the known 30 / 900 / 25470 sequence, returning
//! 30 / 930 / 12891). There is therefore **no trustworthy perft oracle**, so this
//! variant is **rules-validated** — exactly the policy used for Alice / Fog-of-War
//! / Bughouse / Ataxx. The piece moves, the start position, the promotion zone and
//! the drop rules below are taken from the documented rules
//! (<https://www.chessvariants.com/rules/wa-shogi> and
//! <http://www.shogi.net/rjhare/wa-shogi/>) cross-checked against the HaChu
//! engine's Betza piece definitions, and verified by the hand-derived low-depth
//! perft and the property / unit tests in `tests/perft_washogi.rs` plus the
//! attacker-consistency playouts in `tests/attackers_consistency.rs`.
//!
//! ## Pieces
//!
//! Directions are given from White's side (forward = up the board); every Wa piece
//! is left-right symmetric, so Black's move set is the vertical mirror. The royal
//! piece is the **Crane King**, a plain [`WideRole::King`]; the game is won by
//! capturing it. Sixteen non-royal base pieces and fourteen promoted forms make up
//! the rest of the army (Betza notation in parentheses):
//!
//! * **Sparrow Pawn** (`fW` → Golden Bird) — one step straight forward.
//! * **Oxcart** (`fR` → Plodding Ox) — a forward rook (lance).
//! * **Liberated Horse** (`fRbW2` → Heavenly Horse) — forward rook, or up to two
//!   straight back.
//! * **Strutting Crow** (`fWbF` → Flying Falcon) — one forward, or one back-diagonal.
//! * **Swooping Owl** (`fWbF` → Cloud Eagle) — same move, different promotion.
//! * **Climbing Monkey** (`vWfF` → Violent Stag) — one straight forward/back, or one
//!   forward-diagonal.
//! * **Flying Goose** (`vWfF` → Swallow's Wings) — same move, different promotion.
//! * **Flying Cock** (`sWfF` → Raiding Falcon) — one sideways, or one forward-diagonal.
//! * **Blind Dog** (`fFsbW` → Violent Wolf) — one forward-diagonal, or one
//!   sideways/back orthogonal.
//! * **Violent Stag** (`FfW` → Roaming Boar) — one diagonal, or one straight forward.
//! * **Violent Wolf** (`WfF` → Bear's Eyes) — one orthogonal, or one forward-diagonal.
//! * **Swallow's Wings** (`sRvW` → Gliding Swallow) — sideways rook, or one straight
//!   forward/back.
//! * **Running Rabbit** (`fRFbW` → Treacherous Fox) — forward rook, one diagonal, or
//!   one straight back.
//! * **Flying Falcon** (`BfW` → Tenacious Falcon) — bishop, or one straight forward.
//! * **Treacherous Fox** (`FAvWvD`, never promotes) — one/two diagonal and one/two
//!   straight forward/back, the two-square steps being jumps.
//! * **Cloud Eagle** (`vRsWfF3bF`, never promotes) — vertical rook, one sideways, one
//!   to three forward-diagonal, one back-diagonal.
//!
//! The promoted forms repeat several base moves: the Golden Bird and Promoted Blind
//! Dog move as a Violent Wolf (`WfF`); the Plodding Ox and Bear's Eyes as a King;
//! the Promoted Climbing Monkey as a Violent Stag (`FfW`); the Promoted Strutting
//! Crow as a Flying Falcon (`BfW`); the Promoted Swooping Owl as a Cloud Eagle; the
//! Promoted Flying Goose as a Swallow's Wings (`sRvW`); the Promoted Running Rabbit
//! as a Treacherous Fox. The Heavenly Horse (`vN`), Raiding Falcon (`vRsWfF`),
//! Roaming Boar (`FfsW`), Gliding Swallow (`R`) and Tenacious Falcon (`BvRsW`) are
//! distinct movers. Each promoted piece is a **distinct role** from its base: it
//! keeps its promoted move on the board but reverts to the base in the captor's
//! hand (the [`role_hand_base`](WideVariant::role_hand_base) hook), like the Tori
//! Shogi birds.
//!
//! ## Promotion zone
//!
//! The promotion zone is the **furthest three ranks** from each side (ranks 9-11 /
//! 0-based 8-10 for White, ranks 1-3 / 0-based 0-2 for Black). A move that starts
//! or ends in the zone *may* promote (optional), except a Sparrow Pawn or Oxcart
//! reaching the last rank — where it would have no further move — **must** promote.
//! The Treacherous Fox, Cloud Eagle and Crane King never promote.
//!
//! ## Hand and drops
//!
//! Wa Shogi is documented both with and without drops; this implementation uses the
//! **with-drops** form (FSF-style captures-to-hand), exercising the shared Shogi
//! drop machinery. A captured piece is banked **unpromoted** and flipped to the
//! captor's side; a side may instead drop a held piece onto an empty square, except
//! a Sparrow Pawn or Oxcart may not be dropped on the last rank (it would have no
//! move) and a dropped piece is always unpromoted.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Geometry, PromotionConfig, Square, WideRole, WideVariant,
};
use crate::Color;

use super::super::Washogi11x11;

/// The Wa Shogi rule layer: a zero-sized [`WideVariant`] over [`Washogi11x11`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct WashogiRules;

/// The Wa Shogi starting placement (the hand is empty at the start), in mcr's
/// overflow spelling. White's ranks 1-3 hold the army; Black's ranks 9-11 are the
/// 180° reflection. Reading the back rank a..k: Oxcart, Blind Dog, Strutting Crow,
/// Flying Goose, Violent Wolf, Crane King, Violent Stag, Flying Cock, Swooping Owl,
/// Climbing Monkey, Liberated Horse.
const WASHOGI_PLACEMENT: &str = concat!(
    "**f**j**h**l**nk**o**k**g**m**d/",   // rank 11 (Black back)
    "1**v3**q3**t1/",                     // rank 10
    "**b**b**b**r**b**b**b**u**b**b**b/", // rank 9
    "11/11/11/11/11/",                    // ranks 8..4
    "**B**B**B**U**B**B**B**R**B**B**B/", // rank 3
    "1**T3**Q3**V1/",                     // rank 2
    "**D**M**G**K**OK**N**L**H**J**F"     // rank 1 (White back)
);

/// The depth of the promotion zone: the furthest three ranks from each side.
const ZONE_DEPTH: u8 = 3;

/// The four diagonal one-step (Ferz) offsets, White orientation.
const FERZ: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

impl WashogiRules {
    /// Rotates a White-orientation step into `color`'s orientation: White keeps it,
    /// Black takes its vertical mirror (the rank axis negated). Every Wa piece is
    /// left-right symmetric, so the vertical mirror is its Black move set.
    const fn orient((df, dr): (i8, i8), color: Color) -> (i8, i8) {
        match color {
            Color::White => (df, dr),
            Color::Black => (df, -dr),
        }
    }

    /// The leaper attack set for `white_offsets`, oriented for `color`.
    fn leaper(
        color: Color,
        sq: Square<Washogi11x11>,
        white_offsets: &[(i8, i8)],
    ) -> Bitboard<Washogi11x11> {
        let mut bb = Bitboard::<Washogi11x11>::EMPTY;
        for &off in white_offsets {
            let (df, dr) = Self::orient(off, color);
            if let Some(dest) = sq.offset(df, dr) {
                bb.set(dest);
            }
        }
        bb
    }

    /// Slides from `sq` along each White-orientation direction in `white_dirs`
    /// (oriented for `color`) for at most `max` squares (`0` = unlimited), stopping
    /// on the first blocker (a capture square is included).
    fn ray(
        color: Color,
        sq: Square<Washogi11x11>,
        occupancy: Bitboard<Washogi11x11>,
        white_dirs: &[(i8, i8)],
        max: u8,
    ) -> Bitboard<Washogi11x11> {
        let mut bb = Bitboard::<Washogi11x11>::EMPTY;
        for &dir in white_dirs {
            let (df, dr) = Self::orient(dir, color);
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

    /// Violent Wolf (`WfF`): the four orthogonal steps plus the two forward
    /// diagonals. Shared by the Golden Bird and Promoted Blind Dog.
    fn violent_wolf(color: Color, sq: Square<Washogi11x11>) -> Bitboard<Washogi11x11> {
        Self::leaper(
            color,
            sq,
            &[(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, 1)],
        )
    }

    /// Violent Stag (`FfW`): the four diagonals plus one straight forward. Shared by
    /// the Promoted Climbing Monkey.
    fn violent_stag(color: Color, sq: Square<Washogi11x11>) -> Bitboard<Washogi11x11> {
        Self::leaper(color, sq, &[(1, 1), (1, -1), (-1, 1), (-1, -1), (0, 1)])
    }

    /// Treacherous Fox (`FAvWvD`): one or two squares diagonally, and one or two
    /// squares straight forward/back, the two-square steps being jumps (Alfil /
    /// Dabbaba). A pure leaper, shared by the Promoted Running Rabbit.
    fn fox(color: Color, sq: Square<Washogi11x11>) -> Bitboard<Washogi11x11> {
        Self::leaper(
            color,
            sq,
            &[
                // Ferz (one diagonal) + Alfil (two diagonal jump).
                (1, 1),
                (1, -1),
                (-1, 1),
                (-1, -1),
                (2, 2),
                (2, -2),
                (-2, 2),
                (-2, -2),
                // Vertical Wazir (one) + vertical Dabbaba (two jump).
                (0, 1),
                (0, -1),
                (0, 2),
                (0, -2),
            ],
        )
    }

    /// Flying Falcon (`BfW`): a bishop slide plus one straight forward step. Shared
    /// by the Promoted Strutting Crow.
    fn flying_falcon(
        color: Color,
        sq: Square<Washogi11x11>,
        occ: Bitboard<Washogi11x11>,
    ) -> Bitboard<Washogi11x11> {
        let mut bb = attacks::bishop_attacks::<Washogi11x11>(sq, occ);
        bb |= Self::leaper(color, sq, &[(0, 1)]);
        bb
    }

    /// Cloud Eagle (`vRsWfF3bF`): a vertical rook, one sideways step, a one-to-three
    /// forward-diagonal slide, and one back-diagonal step. Shared by the Promoted
    /// Swooping Owl.
    fn cloud_eagle(
        color: Color,
        sq: Square<Washogi11x11>,
        occ: Bitboard<Washogi11x11>,
    ) -> Bitboard<Washogi11x11> {
        // Vertical rook (unlimited forward + backward).
        let mut bb = Self::ray(color, sq, occ, &[(0, 1), (0, -1)], 0);
        // One sideways step + one back-diagonal step.
        bb |= Self::leaper(color, sq, &[(1, 0), (-1, 0), (1, -1), (-1, -1)]);
        // Forward-diagonal slide of up to three squares.
        bb |= Self::ray(color, sq, occ, &[(1, 1), (-1, 1)], 3);
        bb
    }

    /// Swallow's Wings (`sRvW`): a sideways rook plus one straight forward/back step.
    /// Shared by the Promoted Flying Goose.
    fn swallows_wings(
        color: Color,
        sq: Square<Washogi11x11>,
        occ: Bitboard<Washogi11x11>,
    ) -> Bitboard<Washogi11x11> {
        let mut bb = Self::ray(color, sq, occ, &[(1, 0), (-1, 0)], 0);
        bb |= Self::leaper(color, sq, &[(0, 1), (0, -1)]);
        bb
    }

    /// The last rank for `color` (rank 10 white / rank 0 black) — a Sparrow Pawn or
    /// Oxcart there has no further move.
    fn last_rank(color: Color) -> u8 {
        match color {
            Color::White => Washogi11x11::HEIGHT - 1,
            Color::Black => 0,
        }
    }

    /// The mask of every square on `rank`.
    fn rank_mask(rank: u8) -> Bitboard<Washogi11x11> {
        let mut bb = Bitboard::<Washogi11x11>::EMPTY;
        for file in 0..Washogi11x11::WIDTH {
            if let Some(sq) = Square::<Washogi11x11>::from_file_rank(file, rank) {
                bb.set(sq);
            }
        }
        bb
    }
}

impl WideVariant<Washogi11x11> for WashogiRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 106;

    fn starting_position() -> (Board<Washogi11x11>, GenericState<Washogi11x11>) {
        let board = Board::<Washogi11x11>::from_fen_placement(WASHOGI_PLACEMENT)
            .expect("the Wa Shogi starting placement is valid on an 11x11 board");
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
        sq: Square<Washogi11x11>,
        occupancy: Bitboard<Washogi11x11>,
    ) -> Bitboard<Washogi11x11> {
        match role {
            // --- Royal -------------------------------------------------------
            WideRole::King => attacks::king_attacks::<Washogi11x11>(sq),

            // --- Base pieces -------------------------------------------------
            WideRole::SparrowPawn => Self::leaper(color, sq, &[(0, 1)]),
            WideRole::Oxcart => Self::ray(color, sq, occupancy, &[(0, 1)], 0),
            WideRole::LiberatedHorse => {
                // Forward rook + up to two straight back.
                Self::ray(color, sq, occupancy, &[(0, 1)], 0)
                    | Self::ray(color, sq, occupancy, &[(0, -1)], 2)
            }
            // Strutting Crow and Swooping Owl share `fWbF` (different promotions).
            WideRole::StruttingCrow | WideRole::SwoopingOwl => {
                Self::leaper(color, sq, &[(0, 1), (1, -1), (-1, -1)])
            }
            // Climbing Monkey and Flying Goose share `vWfF` (different promotions).
            WideRole::ClimbingMonkey | WideRole::FlyingGoose => {
                Self::leaper(color, sq, &[(0, 1), (0, -1), (1, 1), (-1, 1)])
            }
            WideRole::FlyingCock => Self::leaper(color, sq, &[(1, 0), (-1, 0), (1, 1), (-1, 1)]),
            WideRole::BlindDog => {
                // Forward Ferz + sideways/back Wazir.
                Self::leaper(color, sq, &[(1, 1), (-1, 1), (1, 0), (-1, 0), (0, -1)])
            }
            WideRole::ViolentStag | WideRole::PromotedClimbingMonkey => {
                Self::violent_stag(color, sq)
            }
            WideRole::ViolentWolf | WideRole::GoldenBird | WideRole::PromotedBlindDog => {
                Self::violent_wolf(color, sq)
            }
            WideRole::SwallowsWings | WideRole::PromotedFlyingGoose => {
                Self::swallows_wings(color, sq, occupancy)
            }
            WideRole::RunningRabbit => {
                // Forward rook + Ferz + one straight back.
                Self::ray(color, sq, occupancy, &[(0, 1)], 0)
                    | Self::leaper(color, sq, &FERZ)
                    | Self::leaper(color, sq, &[(0, -1)])
            }
            WideRole::FlyingFalcon | WideRole::PromotedStruttingCrow => {
                Self::flying_falcon(color, sq, occupancy)
            }
            WideRole::TreacherousFox | WideRole::PromotedRunningRabbit => Self::fox(color, sq),
            WideRole::CloudEagle | WideRole::PromotedSwoopingOwl => {
                Self::cloud_eagle(color, sq, occupancy)
            }

            // --- Promoted forms with distinct movement -----------------------
            WideRole::PloddingOx | WideRole::BearsEyes => attacks::king_attacks::<Washogi11x11>(sq),
            WideRole::HeavenlyHorse => {
                // Vertical knight: the four `(±1, ±2)` leaps, forward and back.
                Self::leaper(color, sq, &[(1, 2), (-1, 2), (1, -2), (-1, -2)])
            }
            WideRole::RaidingFalcon => {
                // Vertical rook + one sideways + one forward-diagonal.
                Self::ray(color, sq, occupancy, &[(0, 1), (0, -1)], 0)
                    | Self::leaper(color, sq, &[(1, 0), (-1, 0), (1, 1), (-1, 1)])
            }
            WideRole::RoamingBoar => {
                // Ferz + forward + sideways (every King step except straight back).
                Self::leaper(
                    color,
                    sq,
                    &[(1, 1), (1, -1), (-1, 1), (-1, -1), (0, 1), (1, 0), (-1, 0)],
                )
            }
            WideRole::GlidingSwallow => attacks::rook_attacks::<Washogi11x11>(sq, occupancy),
            WideRole::TenaciousFalcon => {
                // Bishop + vertical rook + one sideways.
                attacks::bishop_attacks::<Washogi11x11>(sq, occupancy)
                    | Self::ray(color, sq, occupancy, &[(0, 1), (0, -1)], 0)
                    | Self::leaper(color, sq, &[(1, 0), (-1, 0)])
            }

            _ => Bitboard::EMPTY,
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // The forward-biased Wa pieces — their attack sets are not symmetric under a
        // vertical flip, so an attacker of one colour is found by projecting the
        // opposite colour's pattern back from the target square (as a Shogi Gold or
        // Lance is). Every Wa piece is left-right symmetric, so this colour-flipped
        // reverse projection is exact. The fully-symmetric pieces (Crane King,
        // Heavenly Horse, Treacherous Fox, Plodding Ox, Bear's Eyes, Gliding
        // Swallow, Swallow's Wings, Tenacious Falcon, and their role-sharing
        // promotions) take the default reverse projection.
        matches!(
            role,
            WideRole::SparrowPawn
                | WideRole::Oxcart
                | WideRole::LiberatedHorse
                | WideRole::StruttingCrow
                | WideRole::SwoopingOwl
                | WideRole::ClimbingMonkey
                | WideRole::FlyingGoose
                | WideRole::FlyingCock
                | WideRole::BlindDog
                | WideRole::ViolentStag
                | WideRole::ViolentWolf
                | WideRole::RunningRabbit
                | WideRole::FlyingFalcon
                | WideRole::CloudEagle
                | WideRole::GoldenBird
                | WideRole::PromotedStruttingCrow
                | WideRole::PromotedSwoopingOwl
                | WideRole::PromotedClimbingMonkey
                | WideRole::RaidingFalcon
                | WideRole::PromotedBlindDog
                | WideRole::RoamingBoar
        )
    }

    fn role_is_slider(role: WideRole) -> bool {
        // Every Wa piece with an unbounded rook/bishop ride can pin / be pinned
        // along a ray.
        matches!(
            role,
            WideRole::Oxcart
                | WideRole::LiberatedHorse
                | WideRole::SwallowsWings
                | WideRole::RunningRabbit
                | WideRole::FlyingFalcon
                | WideRole::CloudEagle
                | WideRole::GlidingSwallow
                | WideRole::PromotedStruttingCrow
                | WideRole::PromotedSwoopingOwl
                | WideRole::PromotedFlyingGoose
                | WideRole::RaidingFalcon
                | WideRole::TenaciousFalcon
        )
    }

    fn confine_pins_to_segment() -> bool {
        // Wa fields *jumping* leapers — chiefly the Treacherous Fox / Promoted
        // Running Rabbit (`FAvWvD`), whose Alfil (two-square diagonal) and vertical
        // Dabbaba (two-square straight) steps *jump* the intervening square, and the
        // Heavenly Horse's knight leaps. A pinned Fox can leap **past** the pinning
        // slider (or past its own king) onto a collinear square that no longer
        // shields the king: e.g. a Fox pinned on a file by a forward-sliding Oxcart
        // directly ahead can jump two squares forward, over the Oxcart, vacating the
        // shielding square and exposing the king (issue #426, the illegal `d2d4`).
        // The default full-line pin mask wrongly permits such a jump. Confining a
        // pinned piece to the king-to-pinner segment (inclusive of the pinner's
        // square) keeps exactly the moves that remain a blocker or capture the
        // pinner. For Wa's sliders (Oxcart, Cloud Eagle, Flying Falcon, the rooks,
        // …) the segment and the full line are equivalent, so this is byte-identical
        // for every previously-validated Wa position and touches no other variant.
        true
    }

    fn promotion_config() -> PromotionConfig {
        // Wa's promotions are per-piece (each promotable base has exactly one
        // promoted form, handled by the generic per-piece promotion path); this
        // static set is unused, but the trait requires it.
        PromotionConfig {
            roles: alloc::vec![WideRole::GoldenBird],
        }
    }

    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        match color {
            Color::White => rank >= Washogi11x11::HEIGHT - ZONE_DEPTH,
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
        // The sixteen base pieces minus the two non-promoting ones (Treacherous Fox,
        // Cloud Eagle) and the royal Crane King; a promoted piece never re-promotes.
        matches!(
            role,
            WideRole::SparrowPawn
                | WideRole::Oxcart
                | WideRole::LiberatedHorse
                | WideRole::StruttingCrow
                | WideRole::SwoopingOwl
                | WideRole::ClimbingMonkey
                | WideRole::FlyingGoose
                | WideRole::FlyingCock
                | WideRole::BlindDog
                | WideRole::ViolentStag
                | WideRole::ViolentWolf
                | WideRole::SwallowsWings
                | WideRole::RunningRabbit
                | WideRole::FlyingFalcon
        )
    }

    fn role_promoted_to(role: WideRole) -> WideRole {
        match role {
            WideRole::SparrowPawn => WideRole::GoldenBird,
            WideRole::Oxcart => WideRole::PloddingOx,
            WideRole::LiberatedHorse => WideRole::HeavenlyHorse,
            WideRole::StruttingCrow => WideRole::PromotedStruttingCrow,
            WideRole::SwoopingOwl => WideRole::PromotedSwoopingOwl,
            WideRole::ClimbingMonkey => WideRole::PromotedClimbingMonkey,
            WideRole::FlyingGoose => WideRole::PromotedFlyingGoose,
            WideRole::FlyingCock => WideRole::RaidingFalcon,
            WideRole::BlindDog => WideRole::PromotedBlindDog,
            WideRole::ViolentStag => WideRole::RoamingBoar,
            WideRole::ViolentWolf => WideRole::BearsEyes,
            WideRole::SwallowsWings => WideRole::GlidingSwallow,
            WideRole::RunningRabbit => WideRole::PromotedRunningRabbit,
            WideRole::FlyingFalcon => WideRole::TenaciousFalcon,
            other => other,
        }
    }

    fn role_promotion_forced(role: WideRole, color: Color, to_rank: u8) -> bool {
        // A Sparrow Pawn (forward step) or Oxcart (forward slide) reaching the last
        // rank has no further move, so it must promote. Every other piece keeps a
        // legal move on the far rank.
        match role {
            WideRole::SparrowPawn | WideRole::Oxcart => to_rank == Self::last_rank(color),
            _ => false,
        }
    }

    fn role_hand_base(role: WideRole) -> WideRole {
        // A captured promoted piece sheds its promotion before entering the hand;
        // every base piece banks as itself.
        match role {
            WideRole::GoldenBird => WideRole::SparrowPawn,
            WideRole::PloddingOx => WideRole::Oxcart,
            WideRole::HeavenlyHorse => WideRole::LiberatedHorse,
            WideRole::PromotedStruttingCrow => WideRole::StruttingCrow,
            WideRole::PromotedSwoopingOwl => WideRole::SwoopingOwl,
            WideRole::PromotedClimbingMonkey => WideRole::ClimbingMonkey,
            WideRole::PromotedFlyingGoose => WideRole::FlyingGoose,
            WideRole::RaidingFalcon => WideRole::FlyingCock,
            WideRole::PromotedBlindDog => WideRole::BlindDog,
            WideRole::RoamingBoar => WideRole::ViolentStag,
            WideRole::BearsEyes => WideRole::ViolentWolf,
            WideRole::GlidingSwallow => WideRole::SwallowsWings,
            WideRole::PromotedRunningRabbit => WideRole::RunningRabbit,
            WideRole::TenaciousFalcon => WideRole::FlyingFalcon,
            other => other,
        }
    }

    fn drop_targets<const R: usize>(
        role: WideRole,
        color: Color,
        board: &Board<Washogi11x11, R>,
    ) -> Bitboard<Washogi11x11> {
        let mut mask = !board.occupied();
        // Dead-piece rule: a dropped Sparrow Pawn or Oxcart may not land on the last
        // rank (it would then have no move).
        if matches!(role, WideRole::SparrowPawn | WideRole::Oxcart) {
            mask &= !Self::rank_mask(Self::last_rank(color));
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

/// Wa Shogi (animal shogi) as a [`GenericPosition`] over the 11x11 geometry.
///
/// Construct the starting position with
/// [`Washogi::startpos`](GenericPosition::startpos) or parse a FEN — the placement
/// may carry the hand as a `[..]` holdings bracket — with
/// [`Washogi::from_fen`](GenericPosition::from_fen). See the [module docs](self)
/// for the animal army, the hand, drops, and the promotion zone.
pub type Washogi = GenericPosition<
    Washogi11x11,
    WashogiRules,
    { <WashogiRules as WideVariant<Washogi11x11>>::ROLE_SPAN },
>;
