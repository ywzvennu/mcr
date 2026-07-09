//! Mansindam (만신담, "Pantheon tale", 9x9) on the generic engine — a **shogi-chess
//! hybrid** in the Korean-mythology theme: the full crazyhouse **captures-to-hand
//! with drops**, a **mandatory** far-three-ranks promotion zone where every
//! promotable piece *must* upgrade, and a **campmate** flag win (a King that
//! reaches the opponent's back rank wins). Validated against Fairy-Stockfish
//! `UCI_Variant mansindam`.
//!
//! Structurally Mansindam is crazyhouse on the 9x9 [`Shogi9x9`] board: a captured
//! piece flips to the captor's side and banks (reverted to its base role) into the
//! **hand**, from which it may be **dropped** onto any empty square (a Pawn not on
//! the last rank, and not onto a file already holding a friendly unpromoted Pawn —
//! FSF's `dropNoDoubled = p`). A move that **starts or ends** in the far three
//! ranks promotes the moving piece, and — unlike Shogi / Shogun — promotion is
//! **mandatory** (FSF `mandatoryPiecePromotion`): the non-promoting alternative is
//! never offered.
//!
//! ## Pieces (confirmed against FSF; promoted forms in parentheses)
//!
//! | piece (hanja) | moves like | mcr role | promotes to |
//! |---------------|-----------|----------|-------------|
//! | Pawn 步 (P)   | one step straight forward (Shogi pawn) | [`WideRole::Pawn`] | Guard |
//! | Knight 騎 (N) | a chess Knight                          | [`WideRole::Knight`] | Centaur |
//! | Bishop 角 (B) | a Bishop                                | [`WideRole::Bishop`] | Archer |
//! | Rook 方 (R)   | a Rook                                  | [`WideRole::Rook`] | Tiger |
//! | Cardinal 猊 (C) | Bishop + Knight                       | [`WideRole::Hawk`] (`a`) | Rhino |
//! | Marshal 首 (M)  | Rook + Knight                         | [`WideRole::Elephant`] (`e`) | Ship |
//! | Queen 奔 (Q)  | Bishop + Rook                           | [`WideRole::Queen`] | — (never) |
//! | Angel 天 (A)  | Bishop + Rook + Knight                  | [`WideRole::Angel`] (`**a`) | — (never) |
//! | King          | a King (royal)                          | [`WideRole::King`] | — (never) |
//!
//! The promoted forms (none promote again; each reverts to its base in hand):
//!
//! | promoted (hanja) | moves like | mcr role |
//! |------------------|-----------|----------|
//! | Guard 哨 (G)   | a King (eight one-steps, non-royal) | [`WideRole::Commoner`] (`*u`) |
//! | Centaur 衛 (E) | King + Knight                        | [`WideRole::Kheshig`] (`w`) |
//! | Archer 馬 (H)  | Bishop + Wazir                       | [`WideRole::DragonHorse`] (`+B`) |
//! | Tiger 龍 (T)   | Rook + Ferz                          | [`WideRole::Dragon`] (`+R`) |
//! | Rhino 聖 (I)   | Bishop + Knight + Wazir              | [`WideRole::Rhino`] (`**i`) |
//! | Ship 名 (S)    | Rook + Knight + Ferz                 | [`WideRole::Ship`] (`**s`) |
//!
//! Because the Guard / Centaur add the King-step component to the Bishop / Rook,
//! the Archer is just Bishop + Wazir and the Tiger Rook + Ferz (the diagonal /
//! orthogonal steps the slide already covers fold away), so they reuse the Shogi
//! Dragon Horse / Dragon King exactly.
//!
//! ## Win conditions
//!
//! Checkmate, stalemate (the side that cannot move *or* drop loses — FSF
//! `stalemateValue = loss`, which never changes a perft node count), and
//! **campmate**: a King that reaches the opponent's back rank (rank 9 for White,
//! rank 1 for Black — FSF `flagPiece = k`, `flagRegion *9` / `*1`) wins
//! immediately, truncating that subtree. The reach is purely positional (FSF sets
//! no `flagPieceSafe`); a King may not step onto a square the enemy attacks, but
//! that is the ordinary king-safety rule, so no contested-flag handling is needed.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant mansindam`, `position startpos`) renders the start as
//!
//! ```text
//! rnbakqcnm/9/ppppppppp/9/9/9/PPPPPPPPP/9/MNCQKABNR[] w - - 0 1
//! ```
//!
//! with FSF's letters `a` (Angel/amazon), `c` (Cardinal/archbishop) and `m`
//! (Marshal/chancellor). mcr reuses `a` (Hawk = Cardinal) and `e` (Elephant =
//! Marshal) and spells the Angel with the second-bank overflow token `**a`, so its
//! canonical start FEN is
//!
//! ```text
//! rnb**akqane/9/ppppppppp/9/9/9/PPPPPPPPP/9/ENAQK**ABNR[] w - - 0 1
//! ```
//!
//! The two are the same position (note the **asymmetric** back ranks — White's
//! Marshal/Cardinal sit on the opposite side of the King from Black's, a 180°
//! rotation); the `compare-fairy/` harness translates the tokens when driving FSF.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Geometry, PromotionConfig, Square, WideRole, WideVariant,
};
use crate::Color;

use super::super::Shogi9x9;

/// The Mansindam rule layer: a zero-sized [`WideVariant`] over [`Shogi9x9`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MansindamRules;

/// The confirmed Mansindam starting placement (the hand is empty at the start).
/// In mcr dialect: the Cardinal is the Hawk `a`/`A`, the Marshal the Elephant
/// `e`/`E`, and the Angel the second-bank overflow `**a`/`**A`.
const MANSINDAM_PLACEMENT: &str = "rnb**akqane/9/ppppppppp/9/9/9/PPPPPPPPP/9/ENAQK**ABNR";

/// The four diagonal one-step (ferz) offsets — the Tiger's diagonal component.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// The four orthogonal one-step (wazir) offsets — the Archer's orthogonal
/// component.
const WAZIR_OFFSETS: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// The depth of the promotion zone: the furthest three ranks from each side.
const ZONE_DEPTH: u8 = 3;

impl MansindamRules {
    /// The Shogi Pawn's attack/movement square: the single square straight forward
    /// (it both moves and captures there, unlike a chess pawn).
    fn pawn_attacks(color: Color, sq: Square<Shogi9x9>) -> Bitboard<Shogi9x9> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = Bitboard::<Shogi9x9>::EMPTY;
        if let Some(dest) = sq.offset(0, fwd) {
            bb.set(dest);
        }
        bb
    }

    /// The last rank for `color` (rank 9 white / rank 1 black) — a Pawn there has
    /// no further move (so it may not be dropped on it; on a move it is forced to
    /// promote before reaching it).
    fn last_rank(color: Color) -> u8 {
        match color {
            Color::White => Shogi9x9::HEIGHT - 1,
            Color::Black => 0,
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

    /// The mask of every square on `file`.
    fn file_mask(file: u8) -> Bitboard<Shogi9x9> {
        let mut bb = Bitboard::<Shogi9x9>::EMPTY;
        for rank in 0..Shogi9x9::HEIGHT {
            if let Some(sq) = Square::<Shogi9x9>::from_file_rank(file, rank) {
                bb.set(sq);
            }
        }
        bb
    }
}

impl WideVariant<Shogi9x9> for MansindamRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 70;

    fn starting_position() -> (Board<Shogi9x9>, GenericState<Shogi9x9>) {
        let board = Board::<Shogi9x9>::from_fen_placement(MANSINDAM_PLACEMENT)
            .expect("the Mansindam starting placement is valid on a 9x9 board");
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
            checks_against: [0, 0],
            jieqi_seed: None,
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
            // Pawn (步): one square straight forward (move and capture alike).
            WideRole::Pawn => Self::pawn_attacks(color, sq),
            // Knight (騎): a standard chess Knight.
            WideRole::Knight => attacks::knight_attacks::<Shogi9x9>(sq),
            WideRole::Bishop => attacks::bishop_attacks::<Shogi9x9>(sq, occupancy),
            WideRole::Rook => attacks::rook_attacks::<Shogi9x9>(sq, occupancy),
            // Queen (奔): Bishop + Rook.
            WideRole::Queen => {
                attacks::bishop_attacks::<Shogi9x9>(sq, occupancy)
                    | attacks::rook_attacks::<Shogi9x9>(sq, occupancy)
            }
            WideRole::King => attacks::king_attacks::<Shogi9x9>(sq),
            // Cardinal (猊) = Hawk: Bishop + Knight.
            WideRole::Hawk => {
                attacks::bishop_attacks::<Shogi9x9>(sq, occupancy)
                    | attacks::knight_attacks::<Shogi9x9>(sq)
            }
            // Marshal (首) = Elephant: Rook + Knight.
            WideRole::Elephant => {
                attacks::rook_attacks::<Shogi9x9>(sq, occupancy)
                    | attacks::knight_attacks::<Shogi9x9>(sq)
            }
            // Angel (天): Bishop + Rook + Knight.
            WideRole::Angel => {
                attacks::bishop_attacks::<Shogi9x9>(sq, occupancy)
                    | attacks::rook_attacks::<Shogi9x9>(sq, occupancy)
                    | attacks::knight_attacks::<Shogi9x9>(sq)
            }
            // Guard (哨) = Commoner: a King's eight one-steps, non-royal.
            WideRole::Commoner => attacks::king_attacks::<Shogi9x9>(sq),
            // Centaur (衛) = Kheshig: King + Knight.
            WideRole::Kheshig => {
                attacks::king_attacks::<Shogi9x9>(sq) | attacks::knight_attacks::<Shogi9x9>(sq)
            }
            // Archer (馬) = Dragon Horse: Bishop + Wazir.
            WideRole::DragonHorse => {
                attacks::bishop_attacks::<Shogi9x9>(sq, occupancy)
                    | attacks::leaper_attacks::<Shogi9x9>(sq, &WAZIR_OFFSETS)
            }
            // Tiger (龍) = Dragon King: Rook + Ferz.
            WideRole::Dragon => {
                attacks::rook_attacks::<Shogi9x9>(sq, occupancy)
                    | attacks::leaper_attacks::<Shogi9x9>(sq, &FERZ_OFFSETS)
            }
            // Rhino (聖): Bishop + Knight + Wazir.
            WideRole::Rhino => {
                attacks::bishop_attacks::<Shogi9x9>(sq, occupancy)
                    | attacks::knight_attacks::<Shogi9x9>(sq)
                    | attacks::leaper_attacks::<Shogi9x9>(sq, &WAZIR_OFFSETS)
            }
            // Ship (名): Rook + Knight + Ferz.
            WideRole::Ship => {
                attacks::rook_attacks::<Shogi9x9>(sq, occupancy)
                    | attacks::knight_attacks::<Shogi9x9>(sq)
                    | attacks::leaper_attacks::<Shogi9x9>(sq, &FERZ_OFFSETS)
            }
            _ => Bitboard::EMPTY,
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // Only the Pawn is forward-biased (it captures straight ahead); every other
        // Mansindam piece — including the standard chess Knight and the compounds —
        // is colour-symmetric, so the attacker scan reverse-projects them directly.
        matches!(role, WideRole::Pawn)
    }

    fn role_is_slider(role: WideRole) -> bool {
        // Every piece with a ray-slide component (so it can pin / be pinned). The
        // Pawn, Knight, King, Guard (Commoner) and Centaur (Kheshig) are pure
        // steppers / leapers.
        matches!(
            role,
            WideRole::Bishop
                | WideRole::Rook
                | WideRole::Queen
                | WideRole::Hawk
                | WideRole::Elephant
                | WideRole::Angel
                | WideRole::DragonHorse
                | WideRole::Dragon
                | WideRole::Rhino
                | WideRole::Ship
        )
    }

    fn promotion_config() -> PromotionConfig {
        // Mansindam's promotions are per-piece (each promotable base role has one
        // promoted form, handled by the generic per-piece promotion path); this
        // static set is unused, but the trait requires it.
        PromotionConfig {
            roles: alloc::vec![WideRole::Commoner],
        }
    }

    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        match color {
            Color::White => rank >= Shogi9x9::HEIGHT - ZONE_DEPTH,
            Color::Black => rank < ZONE_DEPTH,
        }
    }

    fn promotion_mandatory_in_zone() -> bool {
        // FSF `mandatoryPiecePromotion = true`: a promotable piece whose move starts
        // or ends in the zone *must* promote — the non-promoting alternative is
        // never offered.
        true
    }

    fn has_castling() -> bool {
        false
    }

    fn has_flag_win() -> bool {
        // FSF `flagPiece = k`, `flagRegionWhite = *9`, `flagRegionBlack = *1`: a
        // King reaching the opponent's back rank wins ("campmate"). The default
        // `flag_rank` (rank `HEIGHT-1` for White, `0` for Black) is exactly that
        // region, and the win is purely positional (`flag_win_requires_safe` stays
        // at its `false` default — FSF sets no `flagPieceSafe`).
        true
    }

    // --- hand / drops + per-piece promotion -------------------------------

    fn has_hand() -> bool {
        true
    }

    fn role_can_promote(role: WideRole) -> bool {
        // The promotable base pieces. The Queen, Angel and King never promote, and
        // a piece already promoted does not promote again.
        matches!(
            role,
            WideRole::Pawn
                | WideRole::Knight
                | WideRole::Bishop
                | WideRole::Rook
                | WideRole::Hawk
                | WideRole::Elephant
        )
    }

    fn role_promoted_to(role: WideRole) -> WideRole {
        match role {
            WideRole::Pawn => WideRole::Commoner,      // Guard
            WideRole::Knight => WideRole::Kheshig,     // Centaur
            WideRole::Bishop => WideRole::DragonHorse, // Archer
            WideRole::Rook => WideRole::Dragon,        // Tiger
            WideRole::Hawk => WideRole::Rhino,         // Cardinal → Rhino
            WideRole::Elephant => WideRole::Ship,      // Marshal → Ship
            other => other,
        }
    }

    fn role_hand_base(role: WideRole) -> WideRole {
        // A captured promoted piece sheds its promotion before entering the hand
        // (FSF banks the unpromoted base): Guard → Pawn, Centaur → Knight,
        // Archer → Bishop, Tiger → Rook, Rhino → Cardinal, Ship → Marshal. Every
        // base piece (including the never-promoting Queen and Angel) banks as
        // itself.
        match role {
            WideRole::Commoner => WideRole::Pawn,
            WideRole::Kheshig => WideRole::Knight,
            WideRole::DragonHorse => WideRole::Bishop,
            WideRole::Dragon => WideRole::Rook,
            WideRole::Rhino => WideRole::Hawk,
            WideRole::Ship => WideRole::Elephant,
            other => other,
        }
    }

    fn drop_targets<const R: usize>(
        role: WideRole,
        color: Color,
        board: &Board<Shogi9x9, R>,
    ) -> Bitboard<Shogi9x9> {
        let mut mask = !board.occupied();
        if role == WideRole::Pawn {
            // A dropped Pawn may not land on the last rank (it would be immobile —
            // FSF `immobilityIllegal`), nor onto a file already holding a friendly
            // unpromoted Pawn (FSF `dropNoDoubled = p`; a promoted Guard does not
            // count). Every other piece may drop on any empty square — a chess
            // Knight always has a move, so no last-rank ban applies to it.
            mask &= !Self::rank_mask(Self::last_rank(color));
            for pawn in board.pieces(color, WideRole::Pawn) {
                mask &= !Self::file_mask(pawn.file());
            }
        }
        mask
    }
}

/// Mansindam (Korean "Pantheon tale", 9x9 shogi-chess hybrid) as a
/// [`GenericPosition`] over the 9x9 [`Shogi9x9`] geometry.
///
/// Construct the starting position with
/// [`Mansindam::startpos`](GenericPosition::startpos) or parse a FEN — the
/// placement may carry the hand as a `[..]` holdings bracket — with
/// [`Mansindam::from_fen`](GenericPosition::from_fen). See the [module docs](self)
/// for the army, the mandatory promotion zone, the crazyhouse hand and drops, and
/// the campmate flag win.
pub type Mansindam = GenericPosition<
    Shogi9x9,
    MansindamRules,
    { <MansindamRules as WideVariant<Shogi9x9>>::ROLE_SPAN },
>;
