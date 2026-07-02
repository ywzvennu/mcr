//! Chu Shogi (中将棋, "middle shogi", 12x12) on the generic engine.
//!
//! Chu Shogi is the largest historically-popular shogi variant: a **twelve-by-twelve**
//! board, **no hand and no drops** (captured pieces are removed), and an army of
//! twenty-one piece types — including the famous **Lion** double-move piece — with
//! per-piece promotion on entering the far four ranks. It is played on the
//! [`Chu12x12`] geometry.
//!
//! ## Oracle and validation status (be precise about what is validated)
//!
//! The reference engine for Chu Shogi is **HaChu** (H. G. Muller), driven as a
//! GPL subprocess oracle by the `compare-fairy` harness (issue #379).
//!
//! **What is machine-validated against HaChu:**
//!
//! * HaChu has **no native perft**, but its move generation can be read externally:
//!   the `ddugovic/hachu 0.23` build accepts `usermove` only after a `memory N` hash
//!   allocation (otherwise it segfaults on the first move), and with `debug=1` it
//!   prints its full generated move list for a position when handed an illegal
//!   `usermove`. Driving it that way (a fresh subprocess per node, replaying the move
//!   sequence, reading the move-list dump — deduping killer/hash prefixes and
//!   suppressed entries) gives an external HaChu perft; `setboard` drives arbitrary
//!   positions for isolated Lion / promotion checks.
//! * From the start position: **perft(1) = 36** matches HaChu **byte-for-byte**, and
//!   **perft(2) = 1296** matches **node-for-node**. At **perft(3)** mce counts 48319
//!   and HaChu 48317; the trees agree at every node **except one** (after
//!   `1. f3f5 d8d7`), where HaChu 0.23 fails to generate the two legal
//!   *anti-diagonal* distance-two Lion captures of the Black Go-Between on d7 — a
//!   **HaChu bug** (its Lion captures a distance-two enemy on the a1–l12 diagonal
//!   but not the opposite one, shown in isolation via `setboard`). mce is correct.
//!
//! **What this module implements:**
//!
//! * Every non-Lion piece — the sliding generals/chariots/movers, the step
//!   generals, the Kirin/Phoenix jumpers, and the promoted forms — via
//!   [`role_attacks`](WideVariant::role_attacks).
//! * The **Lion** ([`WideRole::ChuLion`]) and the two lion-power promoted pieces
//!   (**Horned Falcon** forward, **Soaring Eagle** forward-diagonally) in full: the
//!   single steps and distance-two jumps come from the leaper `role_attacks`, and
//!   the moves a leaper cannot express — the **igui** (stationary capture), the
//!   **double capture**, the two-step **area move** (one per capturing intermediate
//!   path), and the **jitto pass** — come from the dedicated
//!   [`gen_lion_moves`](crate::geometry::GenericPosition) pass via the new
//!   [`WideMoveKind::LionMove`](crate::geometry::WideMoveKind) (an intermediate
//!   square packed into the `WideMove` addendum, default-off for every other
//!   variant). The Chu **lion-trading** restrictions are **not** modelled because
//!   HaChu does not enforce them in its move generation either (its `setboard` dumps
//!   let a Lion capture a *protected* enemy Lion), so matching the oracle means
//!   leaving them off.
//! * The Chu **promotion** rule as HaChu applies it (its default "promote on
//!   entry"): promotion is **mandatory** on a move that *enters* the zone from
//!   outside, and is never offered on a move that stays within, leaves, or captures
//!   within it ([`WideVariant::lion_style_promotion`]).
//!
//! See `tests/perft_chu.rs` for the HaChu-cross-checked perft counts and the
//! Lion / promotion / per-piece movement unit tests.
//!
//! ## The army (White orientation; forward = up the board)
//!
//! Reused existing roles: King, Gold, Silver, Rook, Bishop, Lance, Queen (the Free
//! King 奔王), Drunk Elephant, Crown Prince (the Prince 太子), Dragon (the Dragon
//! King 龍王 = Rook + Ferz) and Dragon Horse (龍馬 = Bishop + Wazir). The genuinely
//! new pieces are the fourth-tier-overflow roles ([`WideRole::CopperGeneral`] …
//! [`WideRole::SoaringEagle`]).
//!
//! ## Two royals (King + Prince), count-thresholded
//!
//! The Drunk Elephant promotes to a **Prince** ([`WideRole::CrownPrince`]), a second
//! royal. As in Sho Shogi this is expressed with the multi-royal machinery: a side
//! is lost only when both its King and Prince are gone, and the king-safety
//! constraint is active only while a side holds at most one royal.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{attacks, Bitboard, Board, PromotionConfig, Square, WideRole, WideVariant};
use crate::Color;

use super::super::Chu12x12;

/// The Chu Shogi rule layer: a zero-sized [`WideVariant`] over [`Chu12x12`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ChuRules;

/// The confirmed Chu Shogi starting placement (mce dialect), matching the HaChu
/// oracle's `variant chu` board. The chirality is cross-checked against HaChu: its
/// King sits on **f1** (verified — the piece there is the one that may step straight
/// back, which only the King may) and its Lion on **f3**, and mce reproduces HaChu's
/// start-position legal-move set exactly (see `perft_chu`). White (uppercase) holds
/// ranks 1–5; Black is the 180° rotation on ranks 8–12. Reading White's back rank
/// a..l: Lance, Ferocious Leopard, Copper, Silver, Gold, King, Drunk Elephant, Gold,
/// Silver, Copper, Ferocious Leopard, Lance — the King on the f-file, the Drunk
/// Elephant on g. The three central files line up by promotion: f holds King (r1) /
/// Kirin (r2) / Lion (r3), and g holds Elephant (r1) / Phoenix (r2) / Free King
/// (r3), so each Kirin sits behind its Lion and each Phoenix behind its Free King.
/// Dragon Kings render as `+R`, Dragon Horses as `+B`.
const CHU_PLACEMENT: &str = concat!(
    "l***l***csg**ekgs***c***ll/", // rank 12 (Black back: Elephant f, King g)
    "***r1b1***t***p***k***t1b1***r/", // rank 11 (Black: Phoenix f, Kirin g)
    "***i***vr+b+rq***n+r+br***v***i/", // rank 10 (Black: Free King f, Lion g)
    "pppppppppppp/",               // rank 9  (Black pawns)
    "3***g4***g3/",                // rank 8  (Black go-betweens)
    "12/12/",                      // ranks 7,6 (empty)
    "3***G4***G3/",                // rank 5  (White go-betweens)
    "PPPPPPPPPPPP/",               // rank 4  (White pawns)
    "***I***VR+B+R***NQ+R+BR***V***I/", // rank 3  (White: Lion f, Free King g)
    "***R1B1***T***K***P***T1B1***R/", // rank 2  (White: Kirin f, Phoenix g)
    "L***L***CSGK**EGS***C***LL"   // rank 1  (White back: King f, Elephant g)
);

impl ChuRules {
    /// Rotates a White-orientation step `(df, dr)` into `color`'s orientation:
    /// White keeps it, Black takes its vertical mirror. Every Chu piece is
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
        sq: Square<Chu12x12>,
        white_offsets: &[(i8, i8)],
    ) -> Bitboard<Chu12x12> {
        let mut bb = Bitboard::<Chu12x12>::EMPTY;
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
        sq: Square<Chu12x12>,
        occupancy: Bitboard<Chu12x12>,
        white_dirs: &[(i8, i8)],
    ) -> Bitboard<Chu12x12> {
        let mut bb = Bitboard::<Chu12x12>::EMPTY;
        for &dir in white_dirs {
            let (df, dr) = Self::orient(dir, color);
            let mut cur = sq.offset(df, dr);
            while let Some(dest) = cur {
                bb.set(dest);
                if occupancy.contains(dest) {
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

impl WideVariant<Chu12x12> for ChuRules {
    fn starting_position() -> (Board<Chu12x12>, GenericState<Chu12x12>) {
        let board = Board::<Chu12x12>::from_fen_placement(CHU_PLACEMENT)
            .expect("the Chu Shogi starting placement is valid on a 12x12 board");
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
        sq: Square<Chu12x12>,
        occ: Bitboard<Chu12x12>,
    ) -> Bitboard<Chu12x12> {
        match role {
            // --- royals -----------------------------------------------------
            WideRole::King | WideRole::CrownPrince => attacks::king_attacks::<Chu12x12>(sq),

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
            // Go-Between: one step straight forward or back.
            WideRole::GoBetween => Self::leaper(color, sq, &[(0, 1), (0, -1)]),
            // Pawn: one step straight forward (moves and captures forward only).
            WideRole::Pawn => Self::leaper(color, sq, &[(0, 1)]),

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
            WideRole::Rook => attacks::rook_attacks::<Chu12x12>(sq, occ),
            WideRole::Bishop => attacks::bishop_attacks::<Chu12x12>(sq, occ),
            // Free King (奔王): slides in all eight directions.
            WideRole::Queen => {
                attacks::rook_attacks::<Chu12x12>(sq, occ)
                    | attacks::bishop_attacks::<Chu12x12>(sq, occ)
            }
            // Dragon King: Rook slide + one-step diagonals.
            WideRole::Dragon => {
                attacks::rook_attacks::<Chu12x12>(sq, occ) | Self::leaper(color, sq, &FERZ)
            }
            // Dragon Horse: Bishop slide + one-step orthogonals.
            WideRole::DragonHorse => {
                attacks::bishop_attacks::<Chu12x12>(sq, occ) | Self::leaper(color, sq, &WAZIR)
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
                    | attacks::king_attacks::<Chu12x12>(sq)
            }
            // Flying Ox (promoted Vertical Mover): vertical Rook + Bishop.
            WideRole::FlyingOx => {
                Self::ray(color, sq, occ, &[(0, 1), (0, -1)])
                    | attacks::bishop_attacks::<Chu12x12>(sq, occ)
            }
            // Free Boar (promoted Side Mover): horizontal Rook + Bishop.
            WideRole::FreeBoar => {
                Self::ray(color, sq, occ, &[(1, 0), (-1, 0)])
                    | attacks::bishop_attacks::<Chu12x12>(sq, occ)
            }

            // --- lion-power pieces (jumping-leaper model; see module docs) ---
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
        // the opposite colour's pattern from the target square. Every Chu piece is
        // left-right symmetric, so this is exact.
        matches!(
            role,
            WideRole::Gold
                | WideRole::Silver
                | WideRole::CopperGeneral
                | WideRole::BlindTiger
                | WideRole::DrunkElephant
                | WideRole::Pawn
                | WideRole::Lance
                | WideRole::WhiteHorse
                | WideRole::Whale
                | WideRole::HornedFalcon
                | WideRole::SoaringEagle
        )
    }

    fn role_is_slider(role: WideRole) -> bool {
        // Every piece with an unbounded rook/bishop ride can pin / be pinned.
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
        // forward diagonal. (Their sliding/leaping moves in the other directions
        // are the ordinary `role_attacks` set.)
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
        // A Pawn or Lance reaching the furthest rank can no longer move (both only
        // advance straight forward), so it must take its "second chance" to promote
        // there even on a non-capturing move. Every other Chu piece can always move
        // from the deepest rank, so promotion stays optional for it.
        let furthest = match color {
            Color::White => 11,
            Color::Black => 0,
        };
        to_rank == furthest && matches!(role, WideRole::Pawn | WideRole::Lance)
    }

    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        // The furthest four ranks: ranks 9–12 (indices 8–11) for White, ranks 1–4
        // (indices 0–3) for Black.
        match color {
            Color::White => rank >= 8,
            Color::Black => rank <= 3,
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
        matches!(
            role,
            WideRole::Pawn
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
        )
    }

    fn role_promoted_to(role: WideRole) -> WideRole {
        match role {
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
            other => other,
        }
    }

    fn promotion_config() -> PromotionConfig {
        // Chu has no pawn-path promotion (every promotion rides the per-piece path);
        // this static set is unused, but the trait requires it.
        PromotionConfig {
            roles: alloc::vec![WideRole::Gold],
        }
    }

    // --- two royals: King + Prince, count-thresholded ---------------------

    fn multi_royal() -> bool {
        true
    }

    fn royal_squares(board: &Board<Chu12x12>, color: Color) -> Bitboard<Chu12x12> {
        board.kings_of(color) | board.pieces(color, WideRole::CrownPrince)
    }

    fn royals_all_must_survive() -> bool {
        true
    }

    fn royal_constraint_active(board: &Board<Chu12x12>, color: Color) -> bool {
        // A royal (King or Prince) is royal only while the side holds at most one of
        // them; with two, neither is royal and the constraint is off.
        let royals = board.kings_of(color) | board.pieces(color, WideRole::CrownPrince);
        royals.count() <= 1
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

/// Chu Shogi (middle shogi, 12x12) as a [`GenericPosition`] over [`Chu12x12`].
///
/// Construct the starting position with
/// [`Chu::startpos`](GenericPosition::startpos) or parse a FEN (mce dialect) with
/// [`Chu::from_fen`](GenericPosition::from_fen). See the [module docs](self) for the
/// army, the two-royal rule, the promotion zone, and the Lion's validation status.
pub type Chu = GenericPosition<Chu12x12, ChuRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Square as Sq;
    use alloc::vec::Vec;

    fn targets_from(fen: &str, file: u8, rank: u8) -> Vec<u8> {
        let pos = Chu::from_fen(fen).expect("valid Chu FEN");
        let from = Sq::<Chu12x12>::from_file_rank(file, rank).expect("on board");
        let mut got: Vec<u8> = pos
            .legal_moves()
            .iter()
            .filter(|m| m.from::<Chu12x12>() == from)
            .map(|m| m.to::<Chu12x12>().index())
            .collect();
        got.sort_unstable();
        got.dedup();
        got
    }

    fn indices(coords: &[(u8, u8)]) -> Vec<u8> {
        let mut v: Vec<u8> = coords
            .iter()
            .map(|&(f, r)| {
                Sq::<Chu12x12>::from_file_rank(f, r)
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
        let pos = Chu::startpos();
        assert_eq!(
            pos.to_fen(),
            "l***l***csg**ekgs***c***ll/***r1b1***t***p***k***t1b1***r/***i***vr+b+rq***n+r+br***v***i/pppppppppppp/3***g4***g3/12/12/3***G4***G3/PPPPPPPPPPPP/***I***VR+B+R***NQ+R+BR***V***I/***R1B1***T***K***P***T1B1***R/L***L***CSGK**EGS***C***LL w - - 0 1"
        );
    }

    /// The Lion reaches every square within two King steps (24 targets), jumping
    /// intervening pieces, plus its own square via the jitto pass (25 destinations).
    /// From f6 (file 5, rank 5) on an otherwise near-empty board.
    #[test]
    fn lion_reaches_two_king_steps() {
        let got = targets_from("k11/12/12/12/12/12/5***N6/12/12/12/12/5K6 w - - 0 1", 5, 5);
        let mut want_coords: Vec<(u8, u8)> = LION_OFFSETS
            .iter()
            .map(|&(df, dr)| ((5 + df) as u8, (5 + dr) as u8))
            .collect();
        // The jitto pass returns the Lion to its own square (a `from == to` move).
        want_coords.push((5, 5));
        assert_eq!(got, indices(&want_coords));
    }

    /// A Kirin jumps to the second orthogonal square and steps one diagonally.
    #[test]
    fn kirin_jumps_and_steps() {
        let got = targets_from("k11/12/12/12/12/5***K6/12/12/12/12/12/5K6 w - - 0 1", 5, 6);
        let want = indices(&[
            (5, 8),
            (5, 4),
            (7, 6),
            (3, 6),
            (6, 7),
            (4, 7),
            (6, 5),
            (4, 5),
        ]);
        assert_eq!(got, want);
    }

    /// A Copper General steps forward, forward-diagonally, and straight back.
    #[test]
    fn copper_general_steps() {
        let got = targets_from("k11/12/12/12/12/5***C6/12/12/12/12/12/5K6 w - - 0 1", 5, 6);
        let want = indices(&[(5, 7), (6, 7), (4, 7), (5, 5)]);
        assert_eq!(got, want);
    }
}
