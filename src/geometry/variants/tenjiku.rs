//! Tenjiku Shogi (天竺将棋, "exotic/Indian shogi", 16x16) on the generic engine.
//!
//! Tenjiku Shogi is the **largest and most complex variant in the project**: a
//! **sixteen-by-sixteen board** (256 squares — the [`U256`](crate::geometry::U256)
//! backing's exact ceiling), **no hand and no drops** (captured pieces are
//! removed), and an army of ~36 piece types on the [`Tenjiku16x16`] geometry. It
//! extends Dai / Chu Shogi with the famous **jump-capturing Generals** (Great,
//! Vice, Rook, Bishop) and the **area-burning Fire Demon**.
//!
//! ## Relationship to Chu Shogi
//!
//! Tenjiku reuses almost the whole Chu rule layer. Every Chu ranging slider, the
//! Lion and the two lion-power promoted pieces (Soaring Eagle, Horned Falcon), the
//! generals, Kirin, Phoenix, the Drunk Elephant / Prince pair and the ranging
//! promoted forms behave exactly as in [`ChuRules`](super::chu::ChuRules); the
//! shared [`IronGeneral`](WideRole::IronGeneral) and
//! [`ShogiKnight`](WideRole::ShogiKnight) come from the Dai army. The differences
//! are the fourteen genuinely-new movers ([`WideRole::FireDemon`] …
//! [`WideRole::Dog`], the fifth-tier `****` overflow roles), a **five-rank**
//! promotion zone, and a set of Tenjiku-specific promotions (e.g. the Soaring
//! Eagle promotes to a Rook General here, the Lion to a Lion-Hawk, the Free King to
//! a Free Eagle).
//!
//! ## Oracle and validation status — be precise about what is validated
//!
//! The reference engine for Tenjiku Shogi is **HaChu** (H. G. Muller), driven as a
//! GPL subprocess oracle by the `compare-fairy` harness (issue #379); Tenjiku is
//! **not** a Fairy-Stockfish variant. HaChu has no native perft, so the harness
//! walks the move tree externally and questions HaChu per node.
//!
//! **What is machine-validated against HaChu:** the start-position move set at
//! **perft(1)** node-for-node, and **perft(2)** as a per-root divide. At the start
//! the two armies are separated by empty ranks, so every move at these depths is a
//! **non-capture** — and in HaChu the jump-capture, Fire-Demon burn, and Lion
//! multi-capture powers fire **only on captures** (`GenCapts`); for non-captures
//! (`GenNonCapts`) even the Generals and Fire Demon slide as ordinary blockable
//! sliders. The ordinary movement of every piece is therefore exercised and pinned
//! at these depths. See `tests/perft_tenjiku.rs` for the validated counts.
//!
//! ## What is modelled vs. approximated (the honest partial)
//!
//! Tenjiku's hardest powers are **captures**, out of reach at the machine-validated
//! start-position depths; they are modelled to the tractable bar and documented
//! here:
//!
//! * **Fire Demon** ([`WideRole::FireDemon`]) — **fully modelled** (issue #477). It
//!   moves as a Flying Ox (any distance vertically or diagonally — exact) and then
//!   **burns** (captures) *every* enemy on the up-to-eight squares adjacent to its
//!   destination, and it may **igui** (burn in place without moving). Because the
//!   burn victim set is deterministic from the destination + board (all adjacent
//!   enemies), it is not stored in the move: the Fire Demon's slides and its igui
//!   are emitted as
//!   [`WideMoveKind::FireDemonMove`](crate::geometry::WideMoveKind::FireDemonMove)
//!   moves, and the burn is recomputed at apply-time. Igui is `from == to`, and it
//!   is emitted only when there is an adjacent enemy to burn. There is **no machine
//!   oracle** for the burn (HaChu exercises captures only at shallow depth and
//!   segfaults on Tenjiku), so it is validated by **hand-derived perft** on
//!   constructed capture positions — see `tests/perft_tenjiku.rs`.
//! * **Jump-capturing Generals** ([`WideRole::GreatGeneral`],
//!   [`WideRole::ViceGeneral`], [`WideRole::RookGeneral`],
//!   [`WideRole::BishopGeneral`]) — **fully modelled** (issue #478). Each slides as
//!   its base piece (Free King / Bishop / Rook / Bishop) and, **when capturing**, may
//!   jump over any number of *consecutive* **strictly lower-ranked** pieces (friend or
//!   foe) in a straight line to capture an enemy beyond, stopped by the first
//!   equal-or-higher-ranked piece. The ranking hierarchy is King / Prince = 4, Great
//!   General = 3, Vice General = 2, Rook / Bishop General = 1, every other piece = 0
//!   ([`TenjikuRules::role_jump_rank`]). The **Great General is un-capturable except
//!   by another Great General** ([`TenjikuRules::role_is_capture_immune`]), enforced
//!   for both ordinary and jump captures. A jump-capture is a single-victim
//!   `from → to` [`Capture`](crate::geometry::WideMoveKind::Capture) (only the landing
//!   square is taken; the jumped pieces are untouched), so it needs no new move
//!   representation. Emitted by the multi-royal generator's `gen_jump_general_moves`
//!   pass, gated behind [`WideVariant::has_jump_captures`]. The two facets deferred
//!   from #478 are now modelled (issue #491): a General's jump-*check* through a
//!   screen **is** in the attack model — king-safety folds the jump into the
//!   royal-attack query (`jump_general_checks`), so a move leaving one's own king in a
//!   jump-check is illegal and a jump-check must be answered — and a Great General
//!   removed as a Lion double-capture's / Fire Demon area-burn's *secondary* victim is
//!   made immune too (those separate capture paths now honour
//!   [`role_is_capture_immune`](TenjikuRules::role_is_capture_immune)). There is **no
//!   machine oracle** (HaChu segfaults on Tenjiku), so the jump-captures, jump-checks,
//!   and secondary-victim immunity are validated by **hand-derived perft** — see
//!   `tests/perft_tenjiku.rs`.
//! * **Lion / Lion-Hawk multi-move captures** — the Lion double-step, igui and
//!   pass are modelled via the shared [`gen_lion_moves`](crate::geometry::GenericPosition)
//!   pass exactly as in Chu/Dai. The Lion-Hawk ([`WideRole::LionHawk`]) adds
//!   unlimited Bishop range along its diagonals (modelled) atop full Lion power.
//!
//! ## Two royals (King + Prince), count-thresholded
//!
//! As in Chu/Dai the Drunk Elephant promotes to a **Prince**
//! ([`WideRole::CrownPrince`]), a second royal handled by the multi-royal
//! machinery: a side is lost only when both King and Prince are gone, and the
//! king-safety constraint is active only while a side holds at most one royal.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{attacks, Bitboard, Board, PromotionConfig, Square, WideRole, WideVariant};
use crate::Color;

use super::super::Tenjiku16x16;

/// The Tenjiku Shogi rule layer: a zero-sized [`WideVariant`] over
/// [`Tenjiku16x16`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TenjikuRules;

/// The Tenjiku Shogi starting placement (mcr dialect), reproducing the HaChu
/// oracle's `variant tenjiku` board **exactly** — including its hand-written
/// asymmetries (White's second rank is one file short of Black's, and the central
/// General / Free-Eagle pieces sit mirrored between the colours). The board is set
/// left-to-right the way HaChu's `SetUp` places it, so mcr's start-position legal
/// moves match HaChu's node-for-node (see `perft_tenjiku`). New Tenjiku pieces
/// render with the quadrupled `****` fifth-tier prefix; Dragon Kings as `+R`,
/// Dragon Horses as `+B`, the Drunk Elephant as `**E`, and the Chu/Dai overflow
/// pieces with the `***` prefix.
const TENJIKU_PLACEMENT: &str = concat!(
    // rank 16 (Black back)
    "l*n***l***u***csg**ekgs***c***u***l*nl/",
    // rank 15 (Black)
    "***r1****c****c1***t***pq***n***k***t1****c****c1***r/",
    // rank 14 (Black)
    "****s****lb+b+r****w****i****g****h****i****w+r+bb****l****s/",
    // rank 13 (Black)
    "***i***vr***h***e****b****r****v****g****r****b***e***hr***v***i/",
    "pppppppppppppppp/", // rank 12 (Black pawns)
    "4+r6+r4/",          // rank 11 (Black free Dragon Kings, files e, l)
    "16/16/16/16/",      // ranks 10..7 (empty)
    "4+R6+R4/",          // rank 6  (White free Dragon Kings)
    "PPPPPPPPPPPPPPPP/", // rank 5  (White pawns)
    // rank 4  (White)
    "***I***VR***H***E****B****R****G****V****R****B***E***HR***V***I/",
    // rank 3  (White)
    "****S****LB+B+R****W****I****H****E****I****W+R+BB****L****S/",
    // rank 2  (White; one file short — HaChu's asymmetry, faithfully reproduced)
    "***R1****C****C***T***K***NQ***P***T1****C****C1***R1/",
    // rank 1  (White back)
    "L*N***L***U***CSGK**EGS***C***U***L*NL"
);

impl TenjikuRules {
    /// Rotates a White-orientation step `(df, dr)` into `color`'s orientation:
    /// White keeps it, Black takes its vertical mirror. Every Tenjiku piece is
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
        sq: Square<Tenjiku16x16>,
        white_offsets: &[(i8, i8)],
    ) -> Bitboard<Tenjiku16x16> {
        let mut bb = Bitboard::<Tenjiku16x16>::EMPTY;
        for &off in white_offsets {
            let (df, dr) = Self::orient(off, color);
            if let Some(dest) = sq.offset(df, dr) {
                bb.set(dest);
            }
        }
        bb
    }

    /// Slides from `sq` along each White-orientation direction in `white_dirs`
    /// (oriented for `color`) until the first blocker (the capture square is
    /// included).
    fn ray(
        color: Color,
        sq: Square<Tenjiku16x16>,
        occupancy: Bitboard<Tenjiku16x16>,
        white_dirs: &[(i8, i8)],
    ) -> Bitboard<Tenjiku16x16> {
        Self::ray_limited(color, sq, occupancy, white_dirs, u8::MAX)
    }

    /// Like [`ray`](Self::ray) but stops after at most `max_steps` squares in each
    /// direction. A range-2 slider (the Soldiers' sideways / vertical reach) passes
    /// `2`: it reaches one or two squares along a line and is blocked by an
    /// intervening piece (it cannot jump).
    fn ray_limited(
        color: Color,
        sq: Square<Tenjiku16x16>,
        occupancy: Bitboard<Tenjiku16x16>,
        white_dirs: &[(i8, i8)],
        max_steps: u8,
    ) -> Bitboard<Tenjiku16x16> {
        let mut bb = Bitboard::<Tenjiku16x16>::EMPTY;
        for &dir in white_dirs {
            let (df, dr) = Self::orient(dir, color);
            let mut cur = sq.offset(df, dr);
            let mut steps = 0u8;
            while let Some(dest) = cur {
                bb.set(dest);
                steps += 1;
                if steps >= max_steps || occupancy.contains(dest) {
                    break;
                }
                cur = dest.offset(df, dr);
            }
        }
        bb
    }
}

// White-orientation offset groups shared by several pieces.
const FERZ: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
const WAZIR: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
const VERT: [(i8, i8); 2] = [(0, 1), (0, -1)];
const HORIZ: [(i8, i8); 2] = [(1, 0), (-1, 0)];
/// All eight King directions — the Great General's slide / range-jump lines.
const EIGHT_DIRS: [(i8, i8); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

impl WideVariant<Tenjiku16x16> for TenjikuRules {
    /// The tightest prefix of [`WideRole::ALL`] that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 146;

    fn starting_position() -> (Board<Tenjiku16x16>, GenericState<Tenjiku16x16>) {
        let board = Board::<Tenjiku16x16>::from_fen_placement(TENJIKU_PLACEMENT)
            .expect("the Tenjiku Shogi starting placement is valid on a 16x16 board");
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

    #[allow(clippy::too_many_lines)]
    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Tenjiku16x16>,
        occ: Bitboard<Tenjiku16x16>,
    ) -> Bitboard<Tenjiku16x16> {
        match role {
            // --- royals -----------------------------------------------------
            WideRole::King | WideRole::CrownPrince => attacks::king_attacks::<Tenjiku16x16>(sq),

            // --- step generals (shared with Chu / Dai) ----------------------
            WideRole::Gold => Self::leaper(
                color,
                sq,
                &[(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, 1)],
            ),
            WideRole::Silver => {
                Self::leaper(color, sq, &[(0, 1), (1, 1), (-1, 1), (1, -1), (-1, -1)])
            }
            WideRole::CopperGeneral => Self::leaper(color, sq, &[(0, 1), (1, 1), (-1, 1), (0, -1)]),
            WideRole::IronGeneral => Self::leaper(color, sq, &[(0, 1), (1, 1), (-1, 1)]),
            WideRole::FerociousLeopard => Self::leaper(
                color,
                sq,
                &[(0, 1), (1, 1), (-1, 1), (0, -1), (1, -1), (-1, -1)],
            ),
            WideRole::BlindTiger => Self::leaper(
                color,
                sq,
                &[(0, -1), (1, 0), (-1, 0), (1, 1), (-1, 1), (1, -1), (-1, -1)],
            ),
            WideRole::DrunkElephant => Self::leaper(
                color,
                sq,
                &[(0, 1), (1, 0), (-1, 0), (1, 1), (-1, 1), (1, -1), (-1, -1)],
            ),
            WideRole::GoBetween => Self::leaper(color, sq, &VERT),
            WideRole::Pawn => Self::leaper(color, sq, &[(0, 1)]),
            WideRole::ShogiKnight => Self::leaper(color, sq, &[(1, 2), (-1, 2)]),
            // Dog: one step straight forward or to either backward diagonal.
            WideRole::Dog => Self::leaper(color, sq, &[(0, 1), (1, -1), (-1, -1)]),

            // --- jumpers (Kirin / Phoenix) ----------------------------------
            WideRole::Kirin => Self::leaper(
                color,
                sq,
                &[
                    (0, 2),
                    (0, -2),
                    (2, 0),
                    (-2, 0),
                    (1, 1),
                    (1, -1),
                    (-1, 1),
                    (-1, -1),
                ],
            ),
            WideRole::Phoenix => Self::leaper(
                color,
                sq,
                &[
                    (2, 2),
                    (2, -2),
                    (-2, 2),
                    (-2, -2),
                    (1, 0),
                    (-1, 0),
                    (0, 1),
                    (0, -1),
                ],
            ),

            // --- full sliders -----------------------------------------------
            WideRole::Rook => attacks::rook_attacks::<Tenjiku16x16>(sq, occ),
            WideRole::Bishop => attacks::bishop_attacks::<Tenjiku16x16>(sq, occ),
            // Free King (奔王), and the Great General / Free Eagle whose ordinary
            // ride is the same eight-way slide. This is only the Generals' *ordinary*
            // ride; their jump-capture (issue #478) rides the dedicated
            // `gen_jump_general_moves` path, gated behind `has_jump_captures`.
            WideRole::Queen | WideRole::GreatGeneral | WideRole::FreeEagle => {
                attacks::rook_attacks::<Tenjiku16x16>(sq, occ)
                    | attacks::bishop_attacks::<Tenjiku16x16>(sq, occ)
            }
            // Rook General: ordinary Rook ride (the jump-capture rides the generator's
            // jump pass, not this attack set).
            WideRole::RookGeneral => attacks::rook_attacks::<Tenjiku16x16>(sq, occ),
            // Vice / Bishop General: ordinary Bishop ride (jump-capture in the jump
            // pass).
            WideRole::ViceGeneral | WideRole::BishopGeneral => {
                attacks::bishop_attacks::<Tenjiku16x16>(sq, occ)
            }
            // Dragon King: Rook slide + one-step diagonals.
            WideRole::Dragon => {
                attacks::rook_attacks::<Tenjiku16x16>(sq, occ) | Self::leaper(color, sq, &FERZ)
            }
            // Dragon Horse: Bishop slide + one-step orthogonals.
            WideRole::DragonHorse => {
                attacks::bishop_attacks::<Tenjiku16x16>(sq, occ) | Self::leaper(color, sq, &WAZIR)
            }

            // --- directional / partial sliders (shared with Chu) ------------
            WideRole::Lance => Self::ray(color, sq, occ, &[(0, 1)]),
            WideRole::ReverseChariot => Self::ray(color, sq, occ, &VERT),
            WideRole::SideMover => {
                Self::ray(color, sq, occ, &HORIZ) | Self::leaper(color, sq, &VERT)
            }
            WideRole::VerticalMover => {
                Self::ray(color, sq, occ, &VERT) | Self::leaper(color, sq, &HORIZ)
            }
            WideRole::WhiteHorse => Self::ray(color, sq, occ, &[(0, 1), (0, -1), (1, 1), (-1, 1)]),
            WideRole::Whale => Self::ray(color, sq, occ, &[(0, 1), (0, -1), (1, -1), (-1, -1)]),
            WideRole::FlyingStag => {
                Self::ray(color, sq, occ, &VERT) | attacks::king_attacks::<Tenjiku16x16>(sq)
            }
            // Flying Ox (promoted Vertical Mover) *and* the Fire Demon share the
            // vertical-Rook + Bishop ride. This is the Fire Demon's *ride*; its
            // area burn on arrival (and its igui) rides the dedicated
            // `FireDemonMove` path in the generator and is applied on top of this
            // target set (issue #477; see the module docs).
            WideRole::FlyingOx | WideRole::FireDemon => {
                Self::ray(color, sq, occ, &VERT) | attacks::bishop_attacks::<Tenjiku16x16>(sq, occ)
            }
            WideRole::FreeBoar => {
                Self::ray(color, sq, occ, &HORIZ) | attacks::bishop_attacks::<Tenjiku16x16>(sq, occ)
            }

            // --- Tenjiku Soldiers (mixed slide / range-2 / step) ------------
            // Chariot Soldier / Heavenly Tetrarch: vertical Rook + Bishop + range-2
            // sideways.
            WideRole::ChariotSoldier | WideRole::HeavenlyTetrarch => {
                Self::ray(color, sq, occ, &VERT)
                    | attacks::bishop_attacks::<Tenjiku16x16>(sq, occ)
                    | Self::ray_limited(color, sq, occ, &HORIZ, 2)
            }
            // Water Buffalo: horizontal Rook + Bishop + range-2 vertical.
            WideRole::WaterBuffalo => {
                Self::ray(color, sq, occ, &HORIZ)
                    | attacks::bishop_attacks::<Tenjiku16x16>(sq, occ)
                    | Self::ray_limited(color, sq, occ, &VERT, 2)
            }
            // Vertical Soldier: forward Rook, one step back, range-2 sideways.
            WideRole::VerticalSoldier => {
                Self::ray(color, sq, occ, &[(0, 1)])
                    | Self::leaper(color, sq, &[(0, -1)])
                    | Self::ray_limited(color, sq, occ, &HORIZ, 2)
            }
            // Side Soldier: horizontal Rook, one step back, range-2 forward.
            WideRole::SideSoldier => {
                Self::ray(color, sq, occ, &HORIZ)
                    | Self::leaper(color, sq, &[(0, -1)])
                    | Self::ray_limited(color, sq, occ, &[(0, 1)], 2)
            }
            // Multi-General (promoted Dog): forward Rook + both backward diagonals.
            WideRole::MultiGeneral => Self::ray(color, sq, occ, &[(0, 1), (1, -1), (-1, -1)]),

            // --- lion-power pieces ------------------------------------------
            // Lion: any square within two King steps, jumping intervening pieces.
            WideRole::ChuLion => Self::leaper(color, sq, &LION_OFFSETS),
            // Lion-Hawk: full Lion power (the within-two jumps here) plus unlimited
            // Bishop range along the diagonals. The extra igui / double-step moves
            // come from the shared lion pass.
            WideRole::LionHawk => {
                Self::leaper(color, sq, &LION_OFFSETS)
                    | attacks::bishop_attacks::<Tenjiku16x16>(sq, occ)
            }
            // Horned Falcon (promoted Dragon Horse): slides every direction except
            // straight forward, plus a two-step forward reach.
            WideRole::HornedFalcon => {
                Self::ray(
                    color,
                    sq,
                    occ,
                    &[(1, 0), (-1, 0), (0, -1), (1, 1), (-1, 1), (1, -1), (-1, -1)],
                ) | Self::leaper(color, sq, &[(0, 1), (0, 2)])
            }
            // Soaring Eagle (promoted Dragon King): slides every direction except
            // the two forward diagonals, plus a two-step forward-diagonal reach.
            WideRole::SoaringEagle => {
                Self::ray(
                    color,
                    sq,
                    occ,
                    &[(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, -1)],
                ) | Self::leaper(color, sq, &[(1, 1), (2, 2), (-1, 1), (-2, 2)])
            }

            _ => Bitboard::EMPTY,
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // Forward-biased pieces: their attack set is not symmetric under a vertical
        // flip, so an attacker of one colour is found by reverse-projecting the
        // opposite colour's pattern. Every Tenjiku piece is left-right symmetric, so
        // this is exact. The symmetric big pieces (Generals, Free Eagle, Fire Demon,
        // Chariot Soldier, Water Buffalo, Lion-Hawk) are *not* directional.
        matches!(
            role,
            WideRole::Gold
                | WideRole::Silver
                | WideRole::CopperGeneral
                | WideRole::IronGeneral
                | WideRole::FerociousLeopard
                | WideRole::BlindTiger
                | WideRole::DrunkElephant
                | WideRole::Pawn
                | WideRole::ShogiKnight
                | WideRole::Lance
                | WideRole::WhiteHorse
                | WideRole::Whale
                | WideRole::HornedFalcon
                | WideRole::SoaringEagle
                | WideRole::Dog
                | WideRole::MultiGeneral
                | WideRole::VerticalSoldier
                | WideRole::SideSoldier
        )
    }

    fn role_is_slider(role: WideRole) -> bool {
        // Every piece with an *unbounded* rook/bishop ride can pin / be pinned. As
        // in Dai this is advisory only (Tenjiku is multi-royal and rides the
        // make/unmake king-safety path), so the range-2 reaches are excluded.
        matches!(
            role,
            WideRole::Rook
                | WideRole::Bishop
                | WideRole::Queen
                | WideRole::Dragon
                | WideRole::DragonHorse
                | WideRole::Lance
                | WideRole::ReverseChariot
                | WideRole::SideMover
                | WideRole::VerticalMover
                | WideRole::WhiteHorse
                | WideRole::Whale
                | WideRole::FlyingStag
                | WideRole::FlyingOx
                | WideRole::FreeBoar
                | WideRole::HornedFalcon
                | WideRole::SoaringEagle
                | WideRole::FireDemon
                | WideRole::GreatGeneral
                | WideRole::ViceGeneral
                | WideRole::RookGeneral
                | WideRole::BishopGeneral
                | WideRole::LionHawk
                | WideRole::FreeEagle
                | WideRole::HeavenlyTetrarch
                | WideRole::ChariotSoldier
                | WideRole::WaterBuffalo
                | WideRole::VerticalSoldier
                | WideRole::SideSoldier
                | WideRole::MultiGeneral
        )
    }

    // --- Lion powers (igui, double capture, jitto pass) -------------------

    fn has_lion_moves() -> bool {
        true
    }

    fn role_is_full_lion(role: WideRole) -> bool {
        // The Lion and the Lion-Hawk have full (all-direction) Lion power.
        matches!(role, WideRole::ChuLion | WideRole::LionHawk)
    }

    // --- Fire Demon area burn (igui + arrival burn) -----------------------

    fn has_area_burn() -> bool {
        true
    }

    fn role_is_area_burner(role: WideRole) -> bool {
        // The Fire Demon is the sole area-burner: it slides as a Flying Ox and then
        // burns every enemy adjacent to its destination, or igui-burns in place.
        matches!(role, WideRole::FireDemon)
    }

    // --- range-jumping Generals (issue #478) ------------------------------

    fn has_jump_captures() -> bool {
        true
    }

    fn role_jump_rank(role: WideRole) -> u8 {
        // The Tenjiku range-jump hierarchy (chessvariants / Wikipedia "Tenjiku
        // shogi"): a General jumps only over *strictly lower*-ranked pieces and is
        // stopped by any equal-or-higher one. Royals sit at the top and are never
        // jumped (they also outrank every General, so no General can leap them).
        match role {
            WideRole::King | WideRole::CrownPrince => 4,
            WideRole::GreatGeneral => 3,
            WideRole::ViceGeneral => 2,
            WideRole::RookGeneral | WideRole::BishopGeneral => 1,
            _ => 0,
        }
    }

    fn role_is_jump_capturer(role: WideRole) -> bool {
        // The four range-jumping Generals; the Fire Demon is **not** one (it uses the
        // area-burn path instead).
        matches!(
            role,
            WideRole::GreatGeneral
                | WideRole::ViceGeneral
                | WideRole::RookGeneral
                | WideRole::BishopGeneral
        )
    }

    fn role_jump_dirs(role: WideRole) -> &'static [(i8, i8)] {
        // The jump travels along the same lines as the General's ordinary slide
        // (see `role_attacks`): the Great General queen-wise, the Rook General
        // orthogonally, the Vice and Bishop Generals diagonally.
        match role {
            WideRole::GreatGeneral => &EIGHT_DIRS,
            WideRole::RookGeneral => &WAZIR,
            WideRole::ViceGeneral | WideRole::BishopGeneral => &FERZ,
            _ => &[],
        }
    }

    fn role_is_capture_immune(role: WideRole) -> bool {
        // The Great General ("dai-dai-shō") is un-capturable except by another Great
        // General.
        matches!(role, WideRole::GreatGeneral)
    }

    fn role_lion_lines(role: WideRole) -> &'static [(i8, i8)] {
        // The two lion-power promoted pieces carry a *partial* (straight-line) lion
        // power: the Horned Falcon straight forward, the Soaring Eagle along each
        // forward diagonal.
        match role {
            WideRole::HornedFalcon => &[(0, 1)],
            WideRole::SoaringEagle => &[(1, 1), (-1, 1)],
            _ => &[],
        }
    }

    fn lion_style_promotion() -> bool {
        true
    }

    fn role_promotion_forced(role: WideRole, color: Color, to_rank: u8) -> bool {
        // A Pawn or Lance reaching the furthest rank, or a Knight reaching either of
        // the furthest two ranks, can no longer move (all three only advance), so
        // each must promote there even on a non-capturing move.
        let furthest = match color {
            Color::White => 15,
            Color::Black => 0,
        };
        match role {
            WideRole::Pawn | WideRole::Lance => to_rank == furthest,
            WideRole::ShogiKnight => match color {
                Color::White => to_rank >= 14,
                Color::Black => to_rank <= 1,
            },
            _ => false,
        }
    }

    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        // The furthest five ranks (HaChu's Tenjiku zoneDepth = 5): ranks 12–16
        // (indices 11–15) for White, ranks 1–5 (indices 0–4) for Black.
        match color {
            Color::White => rank >= 11,
            Color::Black => rank <= 4,
        }
    }

    fn has_castling() -> bool {
        false
    }

    // --- per-piece promotion (no hand) ------------------------------------

    fn has_piece_promotion() -> bool {
        true
    }

    fn pawn_is_stepper() -> bool {
        true
    }

    fn role_can_promote(role: WideRole) -> bool {
        matches!(
            role,
            // Shared Chu / Dai promoters.
            WideRole::Pawn
                | WideRole::ShogiKnight
                | WideRole::IronGeneral
                | WideRole::GoBetween
                | WideRole::FerociousLeopard
                | WideRole::CopperGeneral
                | WideRole::Silver
                | WideRole::Gold
                | WideRole::Lance
                | WideRole::ReverseChariot
                | WideRole::SideMover
                | WideRole::VerticalMover
                | WideRole::Bishop
                | WideRole::Rook
                | WideRole::DragonHorse
                | WideRole::Dragon
                | WideRole::BlindTiger
                | WideRole::Kirin
                | WideRole::Phoenix
                | WideRole::DrunkElephant
                // Tenjiku-specific promoters.
                | WideRole::SoaringEagle
                | WideRole::HornedFalcon
                | WideRole::ChuLion
                | WideRole::Queen
                | WideRole::ChariotSoldier
                | WideRole::WaterBuffalo
                | WideRole::VerticalSoldier
                | WideRole::SideSoldier
                | WideRole::RookGeneral
                | WideRole::BishopGeneral
                | WideRole::Dog
        )
    }

    fn role_promoted_to(role: WideRole) -> WideRole {
        match role {
            // Shared Chu promotions.
            WideRole::Pawn => WideRole::Gold,
            WideRole::GoBetween => WideRole::DrunkElephant,
            WideRole::FerociousLeopard => WideRole::Bishop,
            WideRole::CopperGeneral => WideRole::SideMover,
            WideRole::Silver => WideRole::VerticalMover,
            WideRole::Gold => WideRole::Rook,
            WideRole::Lance => WideRole::WhiteHorse,
            WideRole::ReverseChariot => WideRole::Whale,
            WideRole::SideMover => WideRole::FreeBoar,
            WideRole::VerticalMover => WideRole::FlyingOx,
            WideRole::Bishop => WideRole::DragonHorse,
            WideRole::Rook => WideRole::Dragon,
            WideRole::DragonHorse => WideRole::HornedFalcon,
            WideRole::Dragon => WideRole::SoaringEagle,
            WideRole::BlindTiger => WideRole::FlyingStag,
            WideRole::Kirin => WideRole::ChuLion,
            WideRole::Phoenix => WideRole::Queen,
            WideRole::DrunkElephant => WideRole::CrownPrince,
            // Tenjiku-specific promotions (overriding some Chu ones).
            WideRole::ShogiKnight => WideRole::SideSoldier,
            WideRole::IronGeneral => WideRole::VerticalSoldier,
            WideRole::SoaringEagle => WideRole::RookGeneral,
            WideRole::HornedFalcon => WideRole::BishopGeneral,
            WideRole::ChuLion => WideRole::LionHawk,
            WideRole::Queen => WideRole::FreeEagle,
            WideRole::ChariotSoldier => WideRole::HeavenlyTetrarch,
            WideRole::WaterBuffalo => WideRole::FireDemon,
            WideRole::VerticalSoldier => WideRole::ChariotSoldier,
            WideRole::SideSoldier => WideRole::WaterBuffalo,
            WideRole::RookGeneral => WideRole::GreatGeneral,
            WideRole::BishopGeneral => WideRole::ViceGeneral,
            WideRole::Dog => WideRole::MultiGeneral,
            other => other,
        }
    }

    fn promotion_config() -> PromotionConfig {
        // Tenjiku has no pawn-path promotion (every promotion rides the per-piece
        // path); this static set is unused, but the trait requires it.
        PromotionConfig {
            roles: alloc::vec![WideRole::Gold],
        }
    }

    // --- two royals: King + Prince, count-thresholded ---------------------

    fn multi_royal() -> bool {
        true
    }

    fn royal_squares(board: &Board<Tenjiku16x16>, color: Color) -> Bitboard<Tenjiku16x16> {
        board.kings_of(color) | board.pieces(color, WideRole::CrownPrince)
    }

    fn royals_all_must_survive() -> bool {
        true
    }

    fn royal_constraint_active(board: &Board<Tenjiku16x16>, color: Color) -> bool {
        let royals = board.kings_of(color) | board.pieces(color, WideRole::CrownPrince);
        royals.count() <= 1
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

    // `attack_repetition_loses` is deliberately left at its default `false` (issue
    // #485). Chu and Dai enable the "attacked pieces however futile → attacker must
    // deviate or lose" rule, but Tenjiku's repetition convention is, per Wikipedia's
    // *Tenjiku shogi* article, "debated and uncertain": the historical sources give
    // no ruling and applying the modern Chu / JCSA rule to Tenjiku is explicitly only
    // *presumed*, not confirmed. There is also no machine oracle (HaChu segfaults on
    // Tenjiku). Enabling it would be guessing a disputed rule, so Tenjiku keeps only
    // the base sennichite / perpetual-check of issue #471 and draws a non-checking
    // repetition. See the [`GenericGame`](crate::geometry::game) module docs.
}

/// The Lion's reachable squares on an empty board: every square within two King
/// steps (Chebyshev distance 1 or 2), which it jumps to over any intervening piece.
const LION_OFFSETS: [(i8, i8); 24] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
    (-2, -2),
    (-1, -2),
    (0, -2),
    (1, -2),
    (2, -2),
    (-2, -1),
    (2, -1),
    (-2, 0),
    (2, 0),
    (-2, 1),
    (2, 1),
    (-2, 2),
    (-1, 2),
    (0, 2),
    (1, 2),
    (2, 2),
];

/// Tenjiku Shogi (exotic shogi, 16x16) as a [`GenericPosition`] over
/// [`Tenjiku16x16`].
///
/// Construct the starting position with
/// [`Tenjiku::startpos`](GenericPosition::startpos) or parse a FEN (mcr dialect)
/// with [`Tenjiku::from_fen`](GenericPosition::from_fen). See the [module
/// docs](self) for the army, the two-royal rule, the five-rank promotion zone, and
/// which powers are modelled vs. approximated.
pub type Tenjiku = GenericPosition<Tenjiku16x16, TenjikuRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Square as Sq;
    use alloc::vec::Vec;

    fn targets_from(fen: &str, file: u8, rank: u8) -> Vec<u8> {
        let pos = Tenjiku::from_fen(fen).expect("valid Tenjiku FEN");
        let from = Sq::<Tenjiku16x16>::from_file_rank(file, rank).expect("on board");
        let mut got: Vec<u8> = pos
            .legal_moves()
            .iter()
            .filter(|m| m.from::<Tenjiku16x16>() == from)
            .map(|m| m.to::<Tenjiku16x16>().index())
            .collect();
        got.sort_unstable();
        got.dedup();
        got
    }

    fn indices(coords: &[(u8, u8)]) -> Vec<u8> {
        let mut v: Vec<u8> = coords
            .iter()
            .map(|&(f, r)| {
                Sq::<Tenjiku16x16>::from_file_rank(f, r)
                    .expect("on board")
                    .index()
            })
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    #[test]
    fn startpos_round_trips() {
        let pos = Tenjiku::startpos();
        let fen = pos.to_fen();
        // The placement round-trips through FEN unchanged.
        assert!(fen.starts_with(TENJIKU_PLACEMENT));
        let reparsed = Tenjiku::from_fen(&fen).expect("re-parse");
        assert_eq!(reparsed.to_fen(), fen);
    }

    /// A lone Dog steps one square forward or to either backward diagonal.
    #[test]
    fn dog_steps_forward_and_back_diagonals() {
        let got = targets_from(
            "16/16/16/16/16/16/16/16/7****D8/16/16/16/16/16/16/7K8 w - - 0 1",
            7,
            7,
        );
        let want = indices(&[(7, 8), (8, 6), (6, 6)]);
        assert_eq!(got, want);
    }

    /// A Chariot Soldier slides forward/back and diagonally (Rook-vertical +
    /// Bishop) and reaches one or two squares sideways.
    #[test]
    fn chariot_soldier_moves() {
        let got = targets_from(
            "16/16/16/16/16/16/16/16/7****C8/16/16/16/16/16/16/7K8 w - - 0 1",
            7,
            7,
        );
        let mut want: Vec<(u8, u8)> = Vec::new();
        // Vertical Rook (blocked only by the board edge / friendly King on rank 0).
        for r in 8..16 {
            want.push((7, r));
        }
        for r in (1..7).rev() {
            want.push((7, r));
        }
        // Both full diagonals.
        for d in 1..16i16 {
            for (sf, sr) in [(1, 1), (-1, 1), (1, -1), (-1, -1)] {
                let f = 7 + sf * d;
                let r = 7 + sr * d;
                if (0..16).contains(&f) && (0..16).contains(&r) {
                    want.push((f as u8, r as u8));
                }
            }
        }
        // Range-2 sideways.
        for (f, r) in [(8, 7), (9, 7), (6, 7), (5, 7)] {
            want.push((f, r));
        }
        assert_eq!(got, indices(&want));
    }

    /// A Vertical Soldier: forward Rook, one step back, range-2 sideways.
    #[test]
    fn vertical_soldier_moves() {
        let got = targets_from(
            "16/16/16/16/16/16/16/16/7****L8/16/16/16/16/16/16/7K8 w - - 0 1",
            7,
            7,
        );
        let mut want: Vec<(u8, u8)> = Vec::new();
        for r in 8..16 {
            want.push((7, r));
        }
        want.push((7, 6)); // one step back
        for (f, r) in [(8, 7), (9, 7), (6, 7), (5, 7)] {
            want.push((f, r));
        }
        assert_eq!(got, indices(&want));
    }

    /// A Great General slides freely in all eight directions (ordinary ride; the
    /// jump-capture power is documented-unmodelled).
    #[test]
    fn great_general_rides_like_a_queen() {
        // King on b1 (file 1, rank 0) sits off all eight rays from h8, so the
        // General's ride is unobstructed to the board edges.
        let got = targets_from(
            "16/16/16/16/16/16/16/16/7****G8/16/16/16/16/16/16/1K14 w - - 0 1",
            7,
            7,
        );
        let mut want: Vec<(u8, u8)> = Vec::new();
        for (sf, sr) in [
            (0, 1),
            (0, -1),
            (1, 0),
            (-1, 0),
            (1, 1),
            (-1, 1),
            (1, -1),
            (-1, -1),
        ] {
            for d in 1..16i16 {
                let f = 7 + sf * d;
                let r = 7 + sr * d;
                if (0..16).contains(&f) && (0..16).contains(&r) {
                    want.push((f as u8, r as u8));
                }
            }
        }
        assert_eq!(got, indices(&want));
    }

    /// The Lion-Hawk reaches every square within two King steps (full Lion power),
    /// jumping intervening pieces, plus unlimited Bishop range along the diagonals,
    /// plus the jitto pass to its own square.
    #[test]
    fn lion_hawk_is_lion_plus_bishop() {
        let got = targets_from(
            "16/16/16/16/16/16/16/16/7****H8/16/16/16/16/16/16/7K8 w - - 0 1",
            7,
            7,
        );
        let mut want: Vec<(u8, u8)> = Vec::new();
        for &(df, dr) in LION_OFFSETS.iter() {
            want.push(((7 + df) as u8, (7 + dr) as u8));
        }
        want.push((7, 7)); // jitto pass
        for (sf, sr) in [(1, 1), (-1, 1), (1, -1), (-1, -1)] {
            for d in 1..16i16 {
                let f = 7 + sf * d;
                let r = 7 + sr * d;
                if (0..16).contains(&f) && (0..16).contains(&r) {
                    want.push((f as u8, r as u8));
                }
            }
        }
        assert_eq!(got, indices(&want));
    }

    /// A Fire Demon moves as a Flying Ox: vertical Rook plus Bishop, never sideways.
    /// (Its area burn adds captures but no new *destinations*, so with no enemy on
    /// the board the reachable-square set is exactly the Flying-Ox ride; the burn
    /// itself is exercised by the dedicated burn tests below and in
    /// `tests/perft_tenjiku.rs`.)
    #[test]
    fn fire_demon_moves_as_flying_ox() {
        let got = targets_from(
            "16/16/16/16/16/16/16/16/7****I8/16/16/16/16/16/16/7K8 w - - 0 1",
            7,
            7,
        );
        let mut want: Vec<(u8, u8)> = Vec::new();
        for r in 8..16 {
            want.push((7, r));
        }
        for r in (1..7).rev() {
            want.push((7, r));
        }
        for (sf, sr) in [(1, 1), (-1, 1), (1, -1), (-1, -1)] {
            for d in 1..16i16 {
                let f = 7 + sf * d;
                let r = 7 + sr * d;
                if (0..16).contains(&f) && (0..16).contains(&r) {
                    want.push((f as u8, r as u8));
                }
            }
        }
        // No sideways moves.
        assert_eq!(got, indices(&want));
    }
}
