//! Dai Shogi (大将棋, "large shogi", 15x15) on the generic engine.
//!
//! Dai Shogi is **Chu Shogi widened to a fifteen-by-fifteen board** (225 squares):
//! **no hand and no drops** (captured pieces are removed), an army of ~29 piece
//! types — including the famous **Lion** double-move piece — with per-piece
//! promotion on entering the **far five ranks**. It is played on the [`Dai15x15`]
//! geometry.
//!
//! ## Relationship to Chu Shogi
//!
//! Dai reuses almost the whole Chu rule layer. Every ranging slider, the Lion and
//! the two lion-power promoted pieces, the generals, Kirin, Phoenix, the Drunk
//! Elephant / Prince pair and the ranging promoted forms behave exactly as in
//! [`ChuRules`](super::chu::ChuRules). The differences are:
//!
//! * a **15x15 board** with a **five-rank** promotion zone (Chu's is four);
//! * five **extra short-range movers** — Violent Ox ([`WideRole::ViolentOx`], a
//!   range-2 rook), Flying Dragon ([`WideRole::FlyingDragon`], a range-2 bishop),
//!   Evil Wolf ([`WideRole::EvilWolf`]), Iron General ([`WideRole::IronGeneral`])
//!   and Stone General ([`WideRole::StoneGeneral`]);
//! * three more pieces that reuse existing roles — the **Angry Boar** (one
//!   orthogonal step) is a [`WideRole::Wazir`], the **Cat Sword** (one diagonal
//!   step) a [`WideRole::Met`] (Ferz), and the **Knight** a forward 2-1
//!   [`WideRole::ShogiKnight`];
//! * **Kirin and Phoenix do not promote** in Dai (in Chu they become the Lion and
//!   the Free King); the Gold general does not promote either. Every weak piece
//!   (Pawn, Knight, Angry Boar, Cat Sword, Evil Wolf, Iron/Stone General, Violent
//!   Ox, Flying Dragon) promotes to a **Gold general**, and the remaining
//!   promotions match Chu.
//!
//! ## Oracle and validation status
//!
//! The reference engine for Dai Shogi is **HaChu** (H. G. Muller), driven as a GPL
//! subprocess oracle by the `compare-fairy` harness (issue #379); Dai is **not** a
//! Fairy-Stockfish variant. HaChu has no native perft, so the harness walks the
//! move tree externally and questions HaChu per node. See `tests/perft_dai.rs` for
//! the validated start-position perft pins and the per-piece movement checks, and
//! the report on issue #401 for the depth reached against HaChu.
//!
//! ## Two royals (King + Prince), count-thresholded
//!
//! As in Chu, the Drunk Elephant promotes to a **Prince**
//! ([`WideRole::CrownPrince`]), a second royal: a side is lost only when both its
//! King and Prince are gone, and the king-safety constraint is active only while a
//! side holds at most one royal. Because the variant is multi-royal, legality rides
//! the make/unmake king-safety path rather than the pin fast-path, so the range-2
//! Violent Ox / Flying Dragon (which are not flagged as unbounded sliders) are
//! handled correctly by their occupancy-aware [`role_attacks`](WideVariant::role_attacks).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{attacks, Bitboard, Board, PromotionConfig, Square, WideRole, WideVariant};
use crate::Color;

use super::super::Dai15x15;

/// The Dai Shogi rule layer: a zero-sized [`WideVariant`] over [`Dai15x15`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DaiRules;

/// The Dai Shogi starting placement (mcr dialect). White (uppercase) holds ranks
/// 1–6; Black is the 180° rotation on ranks 10–15. Reading White's back rank a..o:
/// Lance, Knight, Stone General, Iron General, Copper, Silver, Gold, King, Gold,
/// Silver, Copper, Iron General, Stone General, Knight, Lance — the King on the
/// central h-file. Rank 2 carries the Reverse Chariots, Cat Swords, Ferocious
/// Leopards, Blind Tigers and the central Drunk Elephant; rank 3 the Violent Oxen,
/// Angry Boars, Evil Wolves and the central Kirin / Lion / Phoenix trio (Kirin on
/// the King's left, matching the HaChu oracle); rank 4 the
/// ranging pieces (Rooks, Flying Dragons, Side/Vertical Movers, Bishops, Dragon
/// Horses, Dragon Kings and the central Free King). Rank 5 is the fifteen Pawns,
/// rank 6 the two Go-Betweens (files e and k). Dragon Kings render as `+R`, Dragon
/// Horses as `+B`, the Drunk Elephant as `**E`, and the Dai/Chu overflow pieces
/// with the tripled `***` prefix.
const DAI_PLACEMENT: &str = concat!(
    // rank 15 (Black back): Lance, Knight, Stone, Iron, Copper, Silver, Gold, King …
    "l*n***z***u***csgkgs***c***u***z*nl/",
    // rank 14 (Black): Reverse Chariot / Cat Sword / Leopard / Blind Tiger / Elephant
    "***r1m1***l1***t**e***t1***l1m1***r/",
    // rank 13 (Black): Violent Ox / Angry Boar / Evil Wolf / Phoenix-Lion-Kirin
    "1***x1*j1***f***p***n***k***f1*j1***x1/",
    // rank 12 (Black): Rook / Flying Dragon / Side & Vertical Mover / Bishop / Dragons / Free King
    "r***d***i***vb+b+rq+r+bb***v***i***dr/",
    "ppppppppppppppp/",                        // rank 11 (Black pawns)
    "4***g5***g4/",                            // rank 10 (Black go-betweens: files e, k)
    "15/15/15/",                               // ranks 9, 8, 7 (empty)
    "4***G5***G4/",                            // rank 6  (White go-betweens: files e, k)
    "PPPPPPPPPPPPPPP/",                        // rank 5  (White pawns)
    "R***D***I***VB+B+RQ+R+BB***V***I***DR/",  // rank 4 (White ranging pieces)
    "1***X1*J1***F***K***N***P***F1*J1***X1/", // rank 3 (White: Kirin-Lion-Phoenix)
    "***R1M1***L1***T**E***T1***L1M1***R/",    // rank 2 (White)
    "L*N***Z***U***CSGKGS***C***U***Z*NL"      // rank 1 (White back)
);

impl DaiRules {
    /// Rotates a White-orientation step `(df, dr)` into `color`'s orientation:
    /// White keeps it, Black takes its vertical mirror. Every Dai piece is
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
        sq: Square<Dai15x15>,
        white_offsets: &[(i8, i8)],
    ) -> Bitboard<Dai15x15> {
        let mut bb = Bitboard::<Dai15x15>::EMPTY;
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
        sq: Square<Dai15x15>,
        occupancy: Bitboard<Dai15x15>,
        white_dirs: &[(i8, i8)],
    ) -> Bitboard<Dai15x15> {
        Self::ray_limited(color, sq, occupancy, white_dirs, u8::MAX)
    }

    /// Like [`ray`](Self::ray) but stops after at most `max_steps` squares in each
    /// direction. A range-2 slider (Violent Ox, Flying Dragon) passes `2`: it
    /// reaches one or two squares along a line and is blocked by an intervening
    /// piece (it cannot jump).
    fn ray_limited(
        color: Color,
        sq: Square<Dai15x15>,
        occupancy: Bitboard<Dai15x15>,
        white_dirs: &[(i8, i8)],
        max_steps: u8,
    ) -> Bitboard<Dai15x15> {
        let mut bb = Bitboard::<Dai15x15>::EMPTY;
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
const ORTHO: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
const DIAG: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

impl WideVariant<Dai15x15> for DaiRules {
    fn starting_position() -> (Board<Dai15x15>, GenericState<Dai15x15>) {
        let board = Board::<Dai15x15>::from_fen_placement(DAI_PLACEMENT)
            .expect("the Dai Shogi starting placement is valid on a 15x15 board");
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
        sq: Square<Dai15x15>,
        occ: Bitboard<Dai15x15>,
    ) -> Bitboard<Dai15x15> {
        match role {
            // --- royals -----------------------------------------------------
            WideRole::King | WideRole::CrownPrince => attacks::king_attacks::<Dai15x15>(sq),

            // --- step generals ---------------------------------------------
            // Gold: orthogonals + forward diagonals (six).
            WideRole::Gold => Self::leaper(
                color,
                sq,
                &[(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, 1)],
            ),
            // Silver: straight forward + four diagonals (five).
            WideRole::Silver => {
                Self::leaper(color, sq, &[(0, 1), (1, 1), (-1, 1), (1, -1), (-1, -1)])
            }
            // Copper: straight forward, two forward diagonals, straight back (four).
            WideRole::CopperGeneral => Self::leaper(color, sq, &[(0, 1), (1, 1), (-1, 1), (0, -1)]),
            // Iron General: straight forward + two forward diagonals (three).
            WideRole::IronGeneral => Self::leaper(color, sq, &[(0, 1), (1, 1), (-1, 1)]),
            // Stone General: the two forward diagonals only.
            WideRole::StoneGeneral => Self::leaper(color, sq, &[(1, 1), (-1, 1)]),
            // Evil Wolf: forward, sideways, and forward diagonals (five).
            WideRole::EvilWolf => {
                Self::leaper(color, sq, &[(0, 1), (1, 0), (-1, 0), (1, 1), (-1, 1)])
            }
            // Ferocious Leopard: every King step except the two sideways ones (six).
            WideRole::FerociousLeopard => Self::leaper(
                color,
                sq,
                &[(0, 1), (1, 1), (-1, 1), (0, -1), (1, -1), (-1, -1)],
            ),
            // Blind Tiger: every King step except straight forward (seven).
            WideRole::BlindTiger => Self::leaper(
                color,
                sq,
                &[(0, -1), (1, 0), (-1, 0), (1, 1), (-1, 1), (1, -1), (-1, -1)],
            ),
            // Drunk Elephant: every King step except straight back (seven).
            WideRole::DrunkElephant => Self::leaper(
                color,
                sq,
                &[(0, 1), (1, 0), (-1, 0), (1, 1), (-1, 1), (1, -1), (-1, -1)],
            ),
            // Angry Boar (Wazir): one orthogonal step (four).
            WideRole::Wazir => Self::leaper(color, sq, &WAZIR),
            // Cat Sword (Met / Ferz): one diagonal step (four).
            WideRole::Met => Self::leaper(color, sq, &FERZ),
            // Go-Between: one step straight forward or back.
            WideRole::GoBetween => Self::leaper(color, sq, &[(0, 1), (0, -1)]),
            // Pawn: one step straight forward (moves and captures forward only).
            WideRole::Pawn => Self::leaper(color, sq, &[(0, 1)]),
            // Knight (Shogi Knight): the two forward 2-1 jumps.
            WideRole::ShogiKnight => Self::leaper(color, sq, &[(1, 2), (-1, 2)]),

            // --- range-2 sliders (blockable, cannot jump) -------------------
            // Violent Ox: one or two squares orthogonally.
            WideRole::ViolentOx => Self::ray_limited(color, sq, occ, &ORTHO, 2),
            // Flying Dragon: one or two squares diagonally.
            WideRole::FlyingDragon => Self::ray_limited(color, sq, occ, &DIAG, 2),

            // --- jumpers ----------------------------------------------------
            // Kirin: two-square orthogonal jumps + one-step diagonals.
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
            // Phoenix: two-square diagonal jumps + one-step orthogonals.
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
            WideRole::Rook => attacks::rook_attacks::<Dai15x15>(sq, occ),
            WideRole::Bishop => attacks::bishop_attacks::<Dai15x15>(sq, occ),
            // Free King (奔王): slides in all eight directions.
            WideRole::Queen => {
                attacks::rook_attacks::<Dai15x15>(sq, occ)
                    | attacks::bishop_attacks::<Dai15x15>(sq, occ)
            }
            // Dragon King: Rook slide + one-step diagonals.
            WideRole::Dragon => {
                attacks::rook_attacks::<Dai15x15>(sq, occ) | Self::leaper(color, sq, &FERZ)
            }
            // Dragon Horse: Bishop slide + one-step orthogonals.
            WideRole::DragonHorse => {
                attacks::bishop_attacks::<Dai15x15>(sq, occ) | Self::leaper(color, sq, &WAZIR)
            }

            // --- directional / partial sliders ------------------------------
            // Lance: slides straight forward only.
            WideRole::Lance => Self::ray(color, sq, occ, &[(0, 1)]),
            // Reverse Chariot: slides straight forward or back.
            WideRole::ReverseChariot => Self::ray(color, sq, occ, &[(0, 1), (0, -1)]),
            // Side Mover: slides sideways + one step straight forward/back.
            WideRole::SideMover => {
                Self::ray(color, sq, occ, &[(1, 0), (-1, 0)])
                    | Self::leaper(color, sq, &[(0, 1), (0, -1)])
            }
            // Vertical Mover: slides straight forward/back + one step sideways.
            WideRole::VerticalMover => {
                Self::ray(color, sq, occ, &[(0, 1), (0, -1)])
                    | Self::leaper(color, sq, &[(1, 0), (-1, 0)])
            }
            // White Horse (promoted Lance): slides forward/back + forward-diagonals.
            WideRole::WhiteHorse => Self::ray(color, sq, occ, &[(0, 1), (0, -1), (1, 1), (-1, 1)]),
            // Whale (promoted Reverse Chariot): slides forward/back + back-diagonals.
            WideRole::Whale => Self::ray(color, sq, occ, &[(0, 1), (0, -1), (1, -1), (-1, -1)]),
            // Flying Stag (promoted Blind Tiger): vertical slide + King step.
            WideRole::FlyingStag => {
                Self::ray(color, sq, occ, &[(0, 1), (0, -1)])
                    | attacks::king_attacks::<Dai15x15>(sq)
            }
            // Flying Ox (promoted Vertical Mover): vertical Rook + Bishop.
            WideRole::FlyingOx => {
                Self::ray(color, sq, occ, &[(0, 1), (0, -1)])
                    | attacks::bishop_attacks::<Dai15x15>(sq, occ)
            }
            // Free Boar (promoted Side Mover): horizontal Rook + Bishop.
            WideRole::FreeBoar => {
                Self::ray(color, sq, occ, &[(1, 0), (-1, 0)])
                    | attacks::bishop_attacks::<Dai15x15>(sq, occ)
            }

            // --- lion-power pieces (jumping-leaper model; see Chu module docs) ---
            // Lion: any square within two King steps, jumping intervening pieces.
            WideRole::ChuLion => Self::leaper(color, sq, &LION_OFFSETS),
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
        // The forward-biased pieces: their attack set is not symmetric under a
        // vertical flip, so an attacker of one colour is found by reverse-projecting
        // the opposite colour's pattern from the target square. Every Dai piece is
        // left-right symmetric, so this is exact.
        matches!(
            role,
            WideRole::Gold
                | WideRole::Silver
                | WideRole::CopperGeneral
                | WideRole::IronGeneral
                | WideRole::StoneGeneral
                | WideRole::EvilWolf
                | WideRole::BlindTiger
                | WideRole::DrunkElephant
                | WideRole::Pawn
                | WideRole::ShogiKnight
                | WideRole::Lance
                | WideRole::WhiteHorse
                | WideRole::Whale
                | WideRole::HornedFalcon
                | WideRole::SoaringEagle
        )
    }

    fn role_is_slider(role: WideRole) -> bool {
        // Every piece with an *unbounded* rook/bishop ride can pin / be pinned. The
        // range-2 Violent Ox / Flying Dragon are deliberately excluded (as Chak's
        // range-2 Divine Lord is): Dai is multi-royal and rides the make/unmake
        // king-safety path, so this classification is advisory only.
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
        )
    }

    // --- Lion powers (igui, double capture, jitto pass) -------------------

    fn has_lion_moves() -> bool {
        true
    }

    fn role_is_full_lion(role: WideRole) -> bool {
        // The Lion has lion power in all eight directions.
        matches!(role, WideRole::ChuLion)
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
        // each must take its "second chance" to promote there even on a
        // non-capturing move. Every other Dai piece can always move from the deepest
        // rank, so promotion stays optional for it.
        let furthest = match color {
            Color::White => 14,
            Color::Black => 0,
        };
        match role {
            WideRole::Pawn | WideRole::Lance => to_rank == furthest,
            WideRole::ShogiKnight => match color {
                Color::White => to_rank >= 13,
                Color::Black => to_rank <= 1,
            },
            _ => false,
        }
    }

    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        // The furthest five ranks: ranks 11–15 (indices 10–14) for White, ranks 1–5
        // (indices 0–4) for Black.
        match color {
            Color::White => rank >= 10,
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
        // The Pawn is a forward stepper (moves and captures straight ahead), not a
        // chess pawn; the multi-royal generator routes it through the per-piece
        // promotion path on this flag.
        true
    }

    fn role_can_promote(role: WideRole) -> bool {
        // Every promotable Dai piece. Unlike Chu, the Kirin, Phoenix and Gold do
        // **not** promote here.
        matches!(
            role,
            WideRole::Pawn
                | WideRole::ShogiKnight
                | WideRole::Wazir
                | WideRole::Met
                | WideRole::EvilWolf
                | WideRole::IronGeneral
                | WideRole::StoneGeneral
                | WideRole::ViolentOx
                | WideRole::FlyingDragon
                | WideRole::GoBetween
                | WideRole::FerociousLeopard
                | WideRole::CopperGeneral
                | WideRole::Silver
                | WideRole::Lance
                | WideRole::ReverseChariot
                | WideRole::SideMover
                | WideRole::VerticalMover
                | WideRole::Bishop
                | WideRole::Rook
                | WideRole::DragonHorse
                | WideRole::Dragon
                | WideRole::BlindTiger
                | WideRole::DrunkElephant
        )
    }

    fn role_promoted_to(role: WideRole) -> WideRole {
        match role {
            // Weak pieces promote to a Gold general.
            WideRole::Pawn
            | WideRole::ShogiKnight
            | WideRole::Wazir
            | WideRole::Met
            | WideRole::EvilWolf
            | WideRole::IronGeneral
            | WideRole::StoneGeneral
            | WideRole::ViolentOx
            | WideRole::FlyingDragon => WideRole::Gold,
            // The remaining promotions match Chu.
            WideRole::GoBetween => WideRole::DrunkElephant,
            WideRole::FerociousLeopard => WideRole::Bishop,
            WideRole::CopperGeneral => WideRole::SideMover,
            WideRole::Silver => WideRole::VerticalMover,
            WideRole::Lance => WideRole::WhiteHorse,
            WideRole::ReverseChariot => WideRole::Whale,
            WideRole::SideMover => WideRole::FreeBoar,
            WideRole::VerticalMover => WideRole::FlyingOx,
            WideRole::Bishop => WideRole::DragonHorse,
            WideRole::Rook => WideRole::Dragon,
            WideRole::DragonHorse => WideRole::HornedFalcon,
            WideRole::Dragon => WideRole::SoaringEagle,
            WideRole::BlindTiger => WideRole::FlyingStag,
            WideRole::DrunkElephant => WideRole::CrownPrince,
            other => other,
        }
    }

    fn promotion_config() -> PromotionConfig {
        // Dai has no pawn-path promotion (every promotion rides the per-piece path);
        // this static set is unused, but the trait requires it.
        PromotionConfig {
            roles: alloc::vec![WideRole::Gold],
        }
    }

    // --- two royals: King + Prince, count-thresholded ---------------------

    fn multi_royal() -> bool {
        true
    }

    fn royal_squares(board: &Board<Dai15x15>, color: Color) -> Bitboard<Dai15x15> {
        board.kings_of(color) | board.pieces(color, WideRole::CrownPrince)
    }

    fn royals_all_must_survive() -> bool {
        true
    }

    fn royal_constraint_active(board: &Board<Dai15x15>, color: Color) -> bool {
        // A royal (King or Prince) is royal only while the side holds at most one of
        // them; with two, neither is royal and the constraint is off.
        let royals = board.kings_of(color) | board.pieces(color, WideRole::CrownPrince);
        royals.count() <= 1
    }

    // --- Sennichite / perpetual check / attack repetition (draw rules) -----
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

    fn attack_repetition_loses() -> bool {
        // Dai shares Chu's large-shogi attack-repetition rule: at the fourth
        // occurrence, a side that attacked enemy pieces (however futile) through the
        // cycle while the other attacked nothing must deviate or lose. Adjudicated in
        // [`GenericGame`]; see that module.
        true
    }
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

/// Dai Shogi (large shogi, 15x15) as a [`GenericPosition`] over [`Dai15x15`].
///
/// Construct the starting position with
/// [`Dai::startpos`](GenericPosition::startpos) or parse a FEN (mcr dialect) with
/// [`Dai::from_fen`](GenericPosition::from_fen). See the [module docs](self) for the
/// army, the two-royal rule, the five-rank promotion zone, and the validation
/// status.
pub type Dai = GenericPosition<Dai15x15, DaiRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Square as Sq;
    use alloc::vec::Vec;

    fn targets_from(fen: &str, file: u8, rank: u8) -> Vec<u8> {
        let pos = Dai::from_fen(fen).expect("valid Dai FEN");
        let from = Sq::<Dai15x15>::from_file_rank(file, rank).expect("on board");
        let mut got: Vec<u8> = pos
            .legal_moves()
            .iter()
            .filter(|m| m.from::<Dai15x15>() == from)
            .map(|m| m.to::<Dai15x15>().index())
            .collect();
        got.sort_unstable();
        got.dedup();
        got
    }

    fn indices(coords: &[(u8, u8)]) -> Vec<u8> {
        let mut v: Vec<u8> = coords
            .iter()
            .map(|&(f, r)| {
                Sq::<Dai15x15>::from_file_rank(f, r)
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
        let pos = Dai::startpos();
        assert_eq!(
            pos.to_fen(),
            "l*n***z***u***csgkgs***c***u***z*nl/***r1m1***l1***t**e***t1***l1m1***r/1***x1*j1***f***p***n***k***f1*j1***x1/r***d***i***vb+b+rq+r+bb***v***i***dr/ppppppppppppppp/4***g5***g4/15/15/15/4***G5***G4/PPPPPPPPPPPPPPP/R***D***I***VB+B+RQ+R+BB***V***I***DR/1***X1*J1***F***K***N***P***F1*J1***X1/***R1M1***L1***T**E***T1***L1M1***R/L*N***Z***U***CSGKGS***C***U***Z*NL w - - 0 1"
        );
    }

    /// A lone Violent Ox slides one or two squares orthogonally, blocked by a piece.
    #[test]
    fn violent_ox_is_range_two_rook() {
        // Violent Ox on h8 (file 7, rank 7), board otherwise empty of blockers near it.
        let got = targets_from(
            "7k7/15/15/15/15/15/15/7***X7/15/15/15/15/15/15/7K7 w - - 0 1",
            7,
            7,
        );
        let want = indices(&[
            (7, 8),
            (7, 9), // north 1, 2
            (7, 6),
            (7, 5), // south 1, 2
            (8, 7),
            (9, 7), // east 1, 2
            (6, 7),
            (5, 7), // west 1, 2
        ]);
        assert_eq!(got, want);
    }

    /// A Flying Dragon slides one or two squares diagonally.
    #[test]
    fn flying_dragon_is_range_two_bishop() {
        let got = targets_from(
            "7k7/15/15/15/15/15/15/7***D7/15/15/15/15/15/15/7K7 w - - 0 1",
            7,
            7,
        );
        let want = indices(&[
            (8, 8),
            (9, 9),
            (6, 8),
            (5, 9),
            (8, 6),
            (9, 5),
            (6, 6),
            (5, 5),
        ]);
        assert_eq!(got, want);
    }

    /// An Iron General steps straight forward and to the two forward diagonals.
    #[test]
    fn iron_general_steps() {
        let got = targets_from(
            "7k7/15/15/15/15/15/15/7***U7/15/15/15/15/15/15/7K7 w - - 0 1",
            7,
            7,
        );
        let want = indices(&[(7, 8), (8, 8), (6, 8)]);
        assert_eq!(got, want);
    }

    /// A Stone General steps only to the two forward diagonals.
    #[test]
    fn stone_general_steps() {
        let got = targets_from(
            "7k7/15/15/15/15/15/15/7***Z7/15/15/15/15/15/15/7K7 w - - 0 1",
            7,
            7,
        );
        let want = indices(&[(8, 8), (6, 8)]);
        assert_eq!(got, want);
    }

    /// An Evil Wolf steps forward, sideways, and to the two forward diagonals.
    #[test]
    fn evil_wolf_steps() {
        let got = targets_from(
            "7k7/15/15/15/15/15/15/7***F7/15/15/15/15/15/15/7K7 w - - 0 1",
            7,
            7,
        );
        let want = indices(&[(7, 8), (8, 8), (6, 8), (8, 7), (6, 7)]);
        assert_eq!(got, want);
    }

    /// The Lion reaches every square within two King steps, jumping intervening
    /// pieces, plus its own square via the jitto pass.
    #[test]
    fn lion_reaches_two_king_steps() {
        let got = targets_from(
            "k14/15/15/15/15/15/15/7***N7/15/15/15/15/15/15/7K7 w - - 0 1",
            7,
            7,
        );
        let mut want_coords: Vec<(u8, u8)> = LION_OFFSETS
            .iter()
            .map(|&(df, dr)| ((7 + df) as u8, (7 + dr) as u8))
            .collect();
        want_coords.push((7, 7)); // jitto pass
        assert_eq!(got, indices(&want_coords));
    }
}
