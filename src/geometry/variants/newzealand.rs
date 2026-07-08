//! New Zealand chess (8x8) on the generic engine — **standard chess with the Rook
//! and Knight swapped for two capture-swap pieces**. A Fairy-Stockfish built-in
//! (`UCI_Variant newzealand`, the standard chess base with the Rook / Knight
//! removed and a ROOKNI / KNIROO added). Validated square-for-square against
//! Fairy-Stockfish.
//!
//! ## The capture-swap pieces
//!
//! Both pieces separate their **move** geometry from their **capture** geometry —
//! the same move≠capture split the Orda cavalry uses, and expressed with the same
//! engine hooks:
//!
//! * **ROOKNI** ([`WideRole::Rookni`], FSF `rookni` letter `r`, mcr `****k`, Betza
//!   `mRcN`) — **moves** like a rook (any distance along a rank or file to an
//!   **empty** square) but **captures** like a knight (only the eight 2-1 leaps).
//!   Its quiet rook slides ride
//!   [`quiet_only_targets`](WideVariant::quiet_only_targets); its
//!   [`role_attacks`](WideVariant::role_attacks) — the squares it captures, checks,
//!   and threatens on — is the knight pattern. Being a knight-capturer it gives
//!   check by a knight-attack and **cannot pin** (a leaper attack is not a line).
//! * **KNIROO** ([`WideRole::Lancer`], FSF `kniroo` letter `n`, mcr `f`, Betza
//!   `mNcR`) — the inverse: **moves** like a knight to an empty square but
//!   **captures** like a rook (an orthogonal slider capture). This is exactly the
//!   Orda [`WideRole::Lancer`], reused unchanged — its
//!   [`quiet_only_targets`](WideVariant::quiet_only_targets) is the knight pattern
//!   and its [`role_attacks`](WideVariant::role_attacks) the rook slide, so it
//!   checks and **pins** along a rook line.
//!
//! For each, [`role_attacks_are_capture_only`](WideVariant::role_attacks_are_capture_only)
//! marks the [`role_attacks`](WideVariant::role_attacks) set capture-only (its
//! squares are reachable *only* by capturing an enemy there), and the quiet moves
//! come solely from [`quiet_only_targets`](WideVariant::quiet_only_targets). King
//! safety therefore sees the ROOKNI's knight-checks and the KNIROO's rook-line
//! checks / pins exactly, on the ordinary single-king mask path — no full-verify is
//! needed. The KNIROO is a slider for pin purposes
//! ([`role_is_slider`](WideVariant::role_is_slider)); the ROOKNI, a knight-capturer,
//! is not.
//!
//! ## Castling — the ROOKNI is the "rook"
//!
//! The two back-rank corner pieces are ROOKNIs, and castling uses them exactly as
//! standard chess uses its rooks (FSF `castlingRookPiece = ROOKNI`). The king
//! castles on the standard files — kingside king e1 -> **g1** with the ROOKNI to
//! **f1**; queenside king e1 -> **c1** with the ROOKNI to **d1** — so
//! [`castle_rook_role`](WideVariant::castle_rook_role) returns
//! [`WideRole::Rookni`] on **both** sides.
//!
//! ## Promotion
//!
//! A pawn of either colour reaching the last rank promotes to a **Queen**,
//! **ROOKNI**, **Bishop**, or **KNIROO** (FSF `promotionPieceTypes = q r b n`, where
//! `r` is the ROOKNI and `n` the KNIROO) — never an ordinary Rook or Knight (there
//! are none in this army).
//!
//! ## Confirmed starting FEN
//!
//! New Zealand chess is a FSF **built-in** derived from the standard chess base, so
//! its position is the standard chess start with the rooks being ROOKNIs and the
//! knights KNIROOs:
//!
//! ```text
//! FSF dialect: rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! mcr dialect: ****kfbqkbf****k/pppppppp/8/8/8/8/PPPPPPPP/****KFBQKBF****K w KQkq - 0 1
//! ```
//!
//! In FSF the back rank's `r` is the ROOKNI and `n` the KNIROO — so the FSF FEN is
//! spelled identically to standard chess. mcr already names `r` the Rook and `n` the
//! Knight; the KNIROO reuses the Orda [`WideRole::Lancer`] (bare letter `f`), and the
//! ROOKNI, a fifth-tier overflow role (every single-letter base and the `*` / `**` /
//! `=` / `***` banks are exhausted), takes the token `****k`. So the standard back
//! rank `r n b q k b n r` becomes `****k f b q k b f ****k`. The two FENs are the
//! same position; the `compare-fairy/` harness rewrites mcr's `****k → r` and
//! `f → n` when driving FSF. Both sides have full castling rights (`KQkq`).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Chess8x8, Geometry, PromotionConfig, Square, StandardChess, WideRole,
    WideVariant,
};
use crate::Color;

/// The confirmed New Zealand starting placement in mcr's role letters: standard
/// chess with the two rooks replaced by ROOKNIs (`****k`) and the two knights by
/// KNIROOs (`f`), so each back rank is `****k f b q k b f ****k` and the pawns /
/// king / bishops / queen are standard.
const NEWZEALAND_START_PLACEMENT: &str =
    "****kfbqkbf****k/pppppppp/8/8/8/8/PPPPPPPP/****KFBQKBF****K";

/// The New Zealand-chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides the two capture-swap movements (the ROOKNI's rook-move /
/// knight-capture and the KNIROO's knight-move / rook-capture), the `q r b n`
/// promotion set (`r` the ROOKNI, `n` the KNIROO), and the castling "rook" role (the
/// ROOKNI on both sides). Every other piece, the double pawn step, and en passant
/// are the standard-chess trait defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NewzealandRules;

impl WideVariant<Chess8x8> for NewzealandRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field (the start army Pawn / Bishop / Queen / King, the KNIROO
    /// [`WideRole::Lancer`], and the ROOKNI [`WideRole::Rookni`] at index `148`, both
    /// also promotion targets); the movegen loops iterate this far. See
    /// [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = WideRole::Rookni.index() + 1;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(NEWZEALAND_START_PLACEMENT)
            .expect("the New Zealand starting placement is valid on an 8x8 board");
        // Standard chess castling rights for both sides: the kingside ROOKNI sits on
        // the last file, the queenside ROOKNI on file 0 (the per-side castle piece
        // role is read from `castle_rook_role`).
        let mut castling = GenericCastling::NONE;
        for color in Color::ALL {
            castling.set(color, 0, Some(Chess8x8::WIDTH - 1));
            castling.set(color, 1, Some(0));
        }
        let state = GenericState {
            turn: Color::White,
            castling,
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
        };
        (board, state)
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Chess8x8>,
        occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        match role {
            // ROOKNI: captures like a knight (its only capturing / checking squares).
            // Its non-capturing rook slides are quiet-only (see `quiet_only_targets`),
            // so they are NOT in the attack set.
            WideRole::Rookni => attacks::knight_attacks::<Chess8x8>(sq),
            // KNIROO (Orda Lancer): captures like a rook (its only capturing /
            // checking squares); its knight jumps are quiet-only.
            WideRole::Lancer => attacks::rook_attacks::<Chess8x8>(sq, occupancy),
            // Everything else (pawn, bishop, queen, king) is standard chess.
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn quiet_only_targets(
        role: WideRole,
        _color: Color,
        sq: Square<Chess8x8>,
        occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        // The ROOKNI **moves** like a rook (sliding to empty squares) and the KNIROO
        // like a knight, but neither captures on those squares — their capture sets
        // (knight / rook) live in `role_attacks`. The generic generator filters these
        // by emptiness, so the move pattern is emitted only as a quiet move.
        match role {
            WideRole::Rookni => attacks::rook_attacks::<Chess8x8>(sq, occupancy),
            WideRole::Lancer => attacks::knight_attacks::<Chess8x8>(sq),
            _ => Bitboard::EMPTY,
        }
    }

    fn role_attacks_are_capture_only(role: WideRole) -> bool {
        // The ROOKNI (knight capture) and KNIROO (rook capture) reach their
        // `role_attacks` squares only by capturing; their quiet moves come solely
        // from `quiet_only_targets` (rook slide / knight jump respectively).
        matches!(role, WideRole::Rookni | WideRole::Lancer)
    }

    fn role_is_slider(role: WideRole) -> bool {
        match role {
            // The KNIROO captures along a rook line, so it can pin and be pinned.
            WideRole::Lancer => true,
            // The ROOKNI captures by a knight leap — a leaper, so it cannot pin (and
            // its checks cannot be interposed). It can still be pinned as a blocker,
            // which the generic pin scan handles regardless of this flag.
            WideRole::Rookni => false,
            _ => <StandardChess as WideVariant<Chess8x8>>::role_is_slider(role),
        }
    }

    fn castle_rook_role(_side: usize) -> WideRole {
        // FSF `castlingRookPiece = ROOKNI`: both the kingside and queenside castle
        // pieces are the ROOKNI on the corner squares.
        WideRole::Rookni
    }

    // --- promotion: pawns -> Queen / ROOKNI / Bishop / KNIROO ------------------

    fn promotion_config() -> PromotionConfig {
        // FSF `promotionPieceTypes = q r b n`, where `r` is the ROOKNI and `n` the
        // KNIROO (there is no ordinary Rook or Knight in this army). Order affects
        // only enumeration order, not the perft leaf count.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Queen,
                WideRole::Rookni,
                WideRole::Bishop,
                WideRole::Lancer,
            ],
        }
    }

    /// The western **fifty-move rule**: a position whose halfmove clock has reached
    /// 100 plies (50 full moves with no capture or pawn move) is a
    /// [`WideEndReason::MoveRule`](crate::geometry::WideEndReason::MoveRule) draw,
    /// matching Fairy-Stockfish's default `nMoveRule = 50` for its standard-chess
    /// base. Adjudication-only (the clock never gates move generation), so perft
    /// stays byte-identical.
    fn move_rule_plies() -> Option<u16> {
        Some(100)
    }
}

/// New Zealand chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position (standard chess with ROOKNIs on the rook squares
/// and KNIROOs on the knight squares) with
/// [`Newzealand::startpos`](GenericPosition::startpos) or parse a FEN (mcr dialect)
/// with [`Newzealand::from_fen`](GenericPosition::from_fen). See the [module
/// docs](self) for the capture-swap movements, the ROOKNI castling, and the
/// `q r b n` promotion.
pub type Newzealand = GenericPosition<
    Chess8x8,
    NewzealandRules,
    { <NewzealandRules as WideVariant<Chess8x8>>::ROLE_SPAN },
>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::WideMoveKind;

    /// The canonical start FEN round-trips (mcr `****k` / `f` dialect) and opens with
    /// the FSF-confirmed 20 moves: 16 pawn pushes + 4 KNIROO knight-hops (the ROOKNIs
    /// are boxed in by the pawns, sliding nowhere).
    #[test]
    fn startpos_round_trips() {
        let pos = Newzealand::startpos();
        assert_eq!(
            pos.to_fen(),
            "****kfbqkbf****k/pppppppp/8/8/8/8/PPPPPPPP/****KFBQKBF****K w KQkq - 0 1"
        );
        assert_eq!(pos.turn(), Color::White);
        // Matches Fairy-Stockfish `UCI_Variant newzealand` startpos perft(1).
        assert_eq!(pos.legal_move_count(), 20);
    }

    /// A ROOKNI on an open board **moves** like a rook (the 14 rank/file squares from
    /// d4) but every one is a quiet slide — none can capture. It **captures** only via
    /// its eight knight jumps: with a lone enemy exactly a knight-hop away and its rook
    /// lines otherwise open, the enemy is taken by the leap, and the rook slide up to
    /// (but not onto) a same-file enemy never captures.
    #[test]
    fn rookni_moves_like_rook_captures_like_knight() {
        // A white ROOKNI on d4, a black pawn on e6 (a knight-hop away) and a black
        // pawn on d7 (straight up the file). The ROOKNI captures e6 (knight) but only
        // slides up to d6 on the file — d7 is not a capture, and it may not land on it.
        let pos = Newzealand::from_fen("4k3/3p4/4p3/8/3****K4/8/8/4K3 w - - 0 1").expect("valid");
        let from = Square::<Chess8x8>::from_file_rank(3, 3).unwrap(); // d4
        let e6 = Square::<Chess8x8>::from_file_rank(4, 5).unwrap();
        let d7 = Square::<Chess8x8>::from_file_rank(3, 6).unwrap();
        let d6 = Square::<Chess8x8>::from_file_rank(3, 5).unwrap();
        let dests: alloc::vec::Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Chess8x8>() == from)
            .map(|m| (m.to::<Chess8x8>(), m.is_capture()))
            .collect();
        // The knight-hop onto e6 is a capture.
        assert!(dests.contains(&(e6, true)), "ROOKNI knight-captures e6");
        // The rook slide reaches d6 (empty, quiet) but never d7 (the file enemy).
        assert!(dests.contains(&(d6, false)), "ROOKNI slides quietly to d6");
        assert!(
            !dests.iter().any(|(sq, _)| *sq == d7),
            "ROOKNI cannot capture up its own file"
        );
        // No rook-line move is ever a capture.
        assert!(
            dests.iter().all(|(sq, cap)| !*cap || *sq == e6),
            "the ROOKNI captures only on knight squares"
        );
    }

    /// A KNIROO (Orda Lancer) **moves** like a knight to empty squares but **captures**
    /// like a rook: with an enemy along a rank/file it takes by sliding, while its
    /// knight destinations are quiet only.
    #[test]
    fn kniroo_moves_like_knight_captures_like_rook() {
        // A white KNIROO on d4, a black pawn on d7 up the file (a rook capture) and a
        // knight-hop landing e6 that is empty (a quiet move).
        let pos = Newzealand::from_fen("4k3/3p4/8/8/3F4/8/8/4K3 w - - 0 1").expect("valid");
        let from = Square::<Chess8x8>::from_file_rank(3, 3).unwrap(); // d4
        let d7 = Square::<Chess8x8>::from_file_rank(3, 6).unwrap();
        let e6 = Square::<Chess8x8>::from_file_rank(4, 5).unwrap();
        let dests: alloc::vec::Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Chess8x8>() == from)
            .map(|m| (m.to::<Chess8x8>(), m.is_capture()))
            .collect();
        assert!(dests.contains(&(d7, true)), "KNIROO rook-captures d7");
        assert!(dests.contains(&(e6, false)), "KNIROO knight-moves to e6");
        // Every capture is on a rook line (never a knight square).
        assert!(
            dests.iter().all(|(sq, cap)| !*cap || *sq == d7),
            "the KNIROO captures only along rook lines"
        );
    }

    /// A KNIROO **pins** a friendly piece to the king along a rook line: a black KNIROO
    /// on e8, a white bishop on e4, and the white king on e1 (the file e8-e4-e1), with
    /// the black king out of the way on a8. The bishop is frozen — it may not leave the
    /// e-file, and being a bishop it has no e-file move, so only king steps are legal.
    /// Matches Fairy-Stockfish perft(1) = 5.
    #[test]
    fn kniroo_pins_along_a_rook_line() {
        let pos = Newzealand::from_fen("k3f3/8/8/8/4B3/8/8/4K3 w - - 0 1").expect("valid");
        assert_eq!(pos.legal_move_count(), 5);
        // Every legal move is a king move; the pinned bishop contributes none.
        let king = Square::<Chess8x8>::from_file_rank(4, 0).unwrap(); // e1
        assert!(pos
            .legal_moves()
            .into_iter()
            .all(|m| m.from::<Chess8x8>() == king));
    }

    /// A ROOKNI gives **check** by a knight-attack (a leap, not a line): the black king
    /// on e6 is checked by a white ROOKNI on d4 (the knight hop d4-e6). Being a leaper
    /// check it cannot be interposed — only a king move or capturing the ROOKNI
    /// answers it. Matches Fairy-Stockfish perft(1) = 7.
    #[test]
    fn rookni_gives_knight_check() {
        let pos = Newzealand::from_fen("8/8/4k3/8/3****K4/8/8/4K3 b - - 0 1").expect("valid");
        assert_eq!(pos.legal_move_count(), 7);
    }

    /// Castling slides the corner **ROOKNI** (not a Rook) beside the king on both
    /// wings: kingside king e1 -> g1 with the h1 ROOKNI to f1, queenside king e1 -> c1
    /// with the a1 ROOKNI to d1. Both king destinations and both ROOKNI landings match
    /// Fairy-Stockfish.
    #[test]
    fn castling_uses_the_rookni_on_both_wings() {
        let pos = Newzealand::from_fen(
            "****k3k2****k/pppppppp/8/8/8/8/PPPPPPPP/****K3K2****K w KQkq - 0 1",
        )
        .expect("valid");
        let mut saw_kingside = false;
        let mut saw_queenside = false;
        for mv in pos.legal_moves() {
            match mv.kind() {
                WideMoveKind::CastleKingside => {
                    saw_kingside = true;
                    assert_eq!(mv.to_uci::<Chess8x8>(), "e1g1");
                    let next = pos.play(&mv);
                    assert_eq!(
                        next.board().king_of(Color::White),
                        Square::<Chess8x8>::from_file_rank(6, 0),
                    );
                    assert_eq!(
                        next.board()
                            .piece_at(Square::<Chess8x8>::from_file_rank(5, 0).unwrap())
                            .map(|p| p.role),
                        Some(WideRole::Rookni),
                    );
                }
                WideMoveKind::CastleQueenside => {
                    saw_queenside = true;
                    assert_eq!(mv.to_uci::<Chess8x8>(), "e1c1");
                    let next = pos.play(&mv);
                    assert_eq!(
                        next.board().king_of(Color::White),
                        Square::<Chess8x8>::from_file_rank(2, 0),
                    );
                    assert_eq!(
                        next.board()
                            .piece_at(Square::<Chess8x8>::from_file_rank(3, 0).unwrap())
                            .map(|p| p.role),
                        Some(WideRole::Rookni),
                    );
                }
                _ => {}
            }
        }
        assert!(
            saw_kingside && saw_queenside,
            "both castles available (the ROOKNI is the castle piece)"
        );
    }

    /// A pawn promotes to Queen / ROOKNI / Bishop / KNIROO — never an ordinary Rook or
    /// Knight (there are none in this army). The `q r b n` set offers exactly four
    /// targets on the last rank.
    #[test]
    fn pawn_promotes_to_the_capture_swap_set() {
        let pos = Newzealand::from_fen("4k3/P7/8/8/8/8/8/4K3 w - - 0 1").expect("valid");
        let mut promos: alloc::vec::Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter_map(|m| m.promotion())
            .collect();
        promos.sort();
        promos.dedup();
        let mut want = alloc::vec![
            WideRole::Queen,
            WideRole::Rookni,
            WideRole::Bishop,
            WideRole::Lancer,
        ];
        want.sort();
        assert_eq!(promos, want);
        assert!(!promos.contains(&WideRole::Rook));
        assert!(!promos.contains(&WideRole::Knight));
    }
}
