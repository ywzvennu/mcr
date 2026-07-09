//! Empire (8x8) on the generic engine — a standard chess **Black** army against
//! an asymmetric White **Empire** (Roman) army of long-range "move-far /
//! capture-close" pieces, plus the **flag-win** (campmate) terminal rule and the
//! broadened **flying-general** king-faceoff. Asymmetric like Orda / Synochess.
//! Validated node-for-node against Fairy-Stockfish `UCI_Variant empire` (from its
//! `variants.ini`).
//!
//! ## Armies
//!
//! * **Black = standard chess.** The six standard pieces (`rnbqkbnr` / `pppppppp`)
//!   with standard castling (the only side that castles), standard pawns (double
//!   step, en passant), and the Empire promotion rule below. Every Black movement
//!   is the trait default.
//! * **White = the Empire (Roman army).** Four distinctive long-range pieces, each
//!   of which **moves like a Queen** to an empty square but **captures only on a
//!   short fixed pattern** — the long-range mirror of the Orda cavalry
//!   (knight-move / slider-capture). Confirmed square-for-square against FSF (its
//!   `customPiece` Betza strings `mQc?` — *move* Queen, *capture* `?`):
//!   * **Eagle** ([`WideRole::Eagle`], FSF `e:mQcN`, mcr `*e`) — moves like a
//!     Queen; **captures like a Knight** (the eight 2-1 leaps).
//!   * **Cardinal** ([`WideRole::Cardinal`], FSF `c:mQcB`, mcr `*c`) — moves like a
//!     Queen; **captures like a Bishop** (a diagonal slide).
//!   * **Tower** ([`WideRole::Tower`], FSF `t:mQcR`, mcr `*t`) — moves like a
//!     Queen; **captures like a Rook** (an orthogonal slide).
//!   * **Duke** ([`WideRole::Duke`], FSF `d:mQcK`, mcr `*d`) — moves like a Queen;
//!     **captures like a King** (the eight one-step squares).
//!   * **Soldier** ([`WideRole::Soldier`], FSF `s`, mcr `z`) — steps one square
//!     **forward or sideways** (toward Black, never backward, never diagonal, no
//!     promotion). Identical to the Synochess / Xiangqi soldier mover, colour
//!     White (so its forward is toward rank 8). The two Empire soldiers begin on
//!     White's d3 / e3 (rank 3), pushed one ahead of the pawn shield.
//!   * **King ("Emperor")** ([`WideRole::King`], `k`) — a plain royal king (one),
//!     subject to the flag-win and flying-general rules below. White does not
//!     castle.
//!   * **Pawns** ([`WideRole::Pawn`], `p`) — standard White pawns (double step, en
//!     passant), six of them (the d/e files carry Soldiers instead).
//!
//! ## Promotion
//!
//! A pawn of **either** colour reaching the last rank promotes only to a **Queen**
//! (FSF `promotionPieceTypes = q`) — never to a Rook/Bishop/Knight, and never to an
//! Empire piece. The Empire pieces and the Soldier themselves never promote.
//!
//! ## Special rules
//!
//! * **Flag win (campmate)** ([`WideVariant::has_flag_win`]): White wins the
//!   instant its king reaches the **last rank**; Black wins the instant its king
//!   reaches the **first rank** (FSF `flagPiece = k`, `flagRegionWhite = *8`,
//!   `flagRegionBlack = *1`). The goal ranks are exactly the generic
//!   [`flag_rank`](WideVariant::flag_rank) default, so the engine's shared
//!   `flag_win_reached` test truncates a flag node to a perft leaf, matching FSF.
//! * **King faceoff** ([`WideVariant::has_flying_general`]): the two kings may not
//!   see each other down an open file **or rank** (FSF `flyingGeneral = true`,
//!   the broad file+rank rule, exactly as Synochess). This rides the per-move
//!   verify path the flying general already requires.
//! * **Stalemate is a loss** for the stalemated side
//!   ([`WideVariant::stalemate_is_loss`], FSF `stalemateValue = loss`); this
//!   affects only the reported outcome, not perft.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `variants.ini` (`[empire:chess]`, `startFen`):
//!
//! ```text
//! FSF dialect: rnbqkbnr/pppppppp/8/8/8/PPPSSPPP/8/TECDKCET w kq - 0 1
//! mcr dialect: rnbqkbnr/pppppppp/8/8/8/PPPZZPPP/8/*T*E*C*DK*C*E*T w kq - 0 1
//! ```
//!
//! Black is ordinary chess on ranks 7-8. White's Empire back rank (rank 1) is
//! `T E C D K C E T` (Tower, Eagle, Cardinal, Duke, King, Cardinal, Eagle, Tower)
//! and its pawn shield on rank 3 is `PPP S S PPP` (six pawns plus two Soldiers on
//! the d/e files); rank 2 is **empty** — the Empire asymmetry. mcr spells the four
//! Empire pieces with `*`-prefixed overflow tokens (`*t *e *c *d`, recycling the
//! FSF mnemonics) and the Soldier as `z`; the two FENs are the same position, and
//! the `compare-fairy/` harness rewrites mcr's tokens (`*e → e`, `*c → c`,
//! `*t → t`, `*d → d`, `z → s`) when driving FSF. Only Black has castling rights
//! (`kq`).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Chess8x8, Geometry, PromotionConfig, Square, StandardChess, WideRole,
    WideVariant,
};
use crate::Color;

/// The confirmed Empire starting placement, mcr dialect (see the [module
/// docs](self)): Black standard chess on ranks 7-8, White's Empire back rank
/// `*T *E *C *D K *C *E *T` on rank 1, a `PPP *Z *Z PPP` pawn-and-Soldier shield
/// on rank 3, and an empty rank 2.
const EMPIRE_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/PPPZZPPP/8/*T*E*C*DK*C*E*T";

/// The Empire rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides only what Empire changes from the generic engine: the four White
/// "move-Queen / capture-short" Empire pieces (Eagle / Cardinal / Tower / Duke),
/// the forward/sideways White Soldier, the Queen-only promotion target set, the
/// flag-win campmate, the broadened (file + rank) flying general, and
/// stalemate-as-loss. Black stays standard chess (pawns, castling, promotion, en
/// passant).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct EmpireRules;

impl EmpireRules {
    /// Whether `role` is one of the four White Empire "move-Queen / capture-short"
    /// pieces. Each moves like a Queen to an empty square but captures only on its
    /// own short pattern (returned by [`capture_pattern`](Self::capture_pattern)).
    fn is_empire_piece(role: WideRole) -> bool {
        matches!(
            role,
            WideRole::Eagle | WideRole::Cardinal | WideRole::Tower | WideRole::Duke
        )
    }

    /// The short **capture** pattern of an Empire piece on `sq` under `occupancy`
    /// — the only squares it threatens / checks / captures on:
    /// * Eagle — the Knight leaps,
    /// * Cardinal — the Bishop slide,
    /// * Tower — the Rook slide,
    /// * Duke — the King steps.
    ///
    /// Returns [`Bitboard::EMPTY`] for any non-Empire role (callers gate on
    /// [`is_empire_piece`](Self::is_empire_piece)).
    fn capture_pattern(
        role: WideRole,
        sq: Square<Chess8x8>,
        occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        match role {
            WideRole::Eagle => attacks::knight_attacks::<Chess8x8>(sq),
            WideRole::Cardinal => attacks::bishop_attacks::<Chess8x8>(sq, occupancy),
            WideRole::Tower => attacks::rook_attacks::<Chess8x8>(sq, occupancy),
            WideRole::Duke => attacks::king_attacks::<Chess8x8>(sq),
            _ => Bitboard::EMPTY,
        }
    }

    /// The White Soldier's reachable squares from `sq`: one step forward (toward
    /// rank 8, the enemy back rank) and one step to either side. Black never fields
    /// a Soldier, but this honours `color` for completeness (Black's forward is
    /// toward rank 1).
    fn soldier_targets(color: Color, sq: Square<Chess8x8>) -> Bitboard<Chess8x8> {
        let forward: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = Bitboard::<Chess8x8>::EMPTY;
        if let Some(dest) = sq.offset(0, forward) {
            bb.set(dest);
        }
        for df in [-1i8, 1] {
            if let Some(dest) = sq.offset(df, 0) {
                bb.set(dest);
            }
        }
        bb
    }
}

impl WideVariant<Chess8x8> for EmpireRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 40;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(EMPIRE_START_PLACEMENT)
            .expect("the Empire starting placement is valid on an 8x8 board");
        // Only Black (the standard army) has castling rights; White's Empire back
        // rank never castles. The kingside rook sits on the last file, the
        // queenside rook on file 0.
        let mut castling = GenericCastling::NONE;
        castling.set(Color::Black, 0, Some(Chess8x8::WIDTH - 1));
        castling.set(Color::Black, 1, Some(0));
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
            jieqi_seed: None,
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
            // The four Empire pieces: their *attack* (capture / check) set is the
            // short capture pattern — never the Queen move. The Queen move to an
            // empty square is a quiet-only step (see `role_attacks_board` /
            // `quiet_only_targets`) and is not in the attack relation. Empire is on
            // the per-move verify path (flying general), which projects the
            // board-aware set, so this occupancy-only fallback returns the bare
            // capture pattern for any incidental query.
            r if Self::is_empire_piece(r) => Self::capture_pattern(r, sq, occupancy),
            // White's Soldier: forward / sideways one step.
            WideRole::Soldier => Self::soldier_targets(color, sq),
            // Black's whole army and the king are standard chess.
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn uses_board_attacks() -> bool {
        // The Empire pieces' generated move set folds two distinct patterns — quiet
        // Queen slides onto empty squares plus short-pattern captures onto enemy
        // pieces — so the verify-path generator and the king-safety / attacker
        // projection take the board-aware set below. Every other role returns
        // `None` and falls back to the occupancy-only `role_attacks`.
        true
    }

    fn role_attacks_board<const R: usize>(
        role: WideRole,
        color: Color,
        sq: Square<Chess8x8>,
        board: &Board<Chess8x8, R>,
    ) -> Option<Bitboard<Chess8x8>> {
        // Only the Empire pieces need the whole board: their move set is the union
        // of the Queen slide onto *empty* squares and the short capture pattern
        // landing on *enemy* pieces. `emit_targets` splits the returned set by
        // enemy occupancy, so a quiet Queen square becomes a Quiet move and a
        // capture-pattern enemy becomes a Capture — exactly matching FSF's `mQc?`
        // semantics. The king sits on an occupied (enemy) square, so it falls only
        // in the capture portion: the same set serves king-safety and
        // `attackers_to` (both gated via `role_attack_is_leg_asymmetric`).
        if !Self::is_empire_piece(role) {
            return None;
        }
        let occupied = board.occupied();
        let enemies = board.by_color(color.opposite());
        let quiet_queen = attacks::queen_attacks::<Chess8x8>(sq, occupied) & !occupied;
        let captures = Self::capture_pattern(role, sq, occupied) & enemies;
        Some(quiet_queen | captures)
    }

    fn role_threats_board<const R: usize>(
        role: WideRole,
        color: Color,
        sq: Square<Chess8x8>,
        board: &Board<Chess8x8, R>,
    ) -> Option<Bitboard<Chess8x8>> {
        // The pure **threat** set of an Empire piece is just its short capture
        // pattern — never its Queen move. `role_attacks_board` (the *move* set) folds
        // in the quiet Queen slides onto empty squares, which are reachable but not
        // attacked: a piece an Empire mover could only ever *step* to (not capture) is
        // not under threat. The threat-detection paths (`attackers_to`, king-safety)
        // must therefore project only the capture pattern, or an empty square on the
        // Queen-move diagonal of, say, a Tower (which captures only like a Rook) would
        // be flagged as attacked — wrongly forbidding a king from castling onto or
        // through it (issue #359). The king-safety caller sits the king on an
        // occupied square, where this agrees with the move set; only an empty query
        // square (a castling transit / destination) exposed the divergence. Returns
        // the bare capture pattern (not masked by enemy occupancy): a threat covers
        // any square the piece could capture on, occupied or not.
        if !Self::is_empire_piece(role) {
            let _ = color;
            return None;
        }
        Some(Self::capture_pattern(role, sq, board.occupied()))
    }

    fn role_attack_is_leg_asymmetric(role: WideRole) -> bool {
        // The Empire pieces' threat set is their short capture pattern, which is not
        // the same as their (Queen) move pattern — so `attackers_to` /
        // `king_safe_after` must project each piece's board-aware set *forward* from
        // its own origin (as the move generator does) rather than reverse-projecting
        // a single symmetric pattern. The Eagle (Knight) / Cardinal (Bishop) / Tower
        // (Rook) / Duke (King) capture patterns are each geometrically symmetric,
        // but only the board-aware `role_attacks_board` correctly isolates the
        // capture portion (an empty Queen-square must never count as a threat to a
        // king there), so they ride this forward-projection path.
        Self::is_empire_piece(role)
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // The Soldier's forward step is colour-directional (forward flips with the
        // side), so a reverse projection must use the opposite colour. The Empire
        // pieces are handled by the leg-asymmetric forward path above, and Black's
        // pawns are the standard directional case.
        matches!(role, WideRole::Pawn | WideRole::Soldier)
    }

    fn role_is_slider(role: WideRole) -> bool {
        // The Empire pieces capture along (Cardinal/Tower) or leap to (Eagle/Duke)
        // their short pattern, but their *move* is a Queen slide; they are routed
        // through the board-aware verify path, not the standard pin/slider machinery,
        // so they need no slider flag here. The Soldier is a stepper. Black's army is
        // standard.
        <StandardChess as WideVariant<Chess8x8>>::role_is_slider(role)
    }

    // --- promotion: pawns -> Queen only (both colours) ---------------------

    fn promotion_config() -> PromotionConfig {
        PromotionConfig {
            roles: alloc::vec![WideRole::Queen],
        }
    }

    // --- flying general (file + rank king faceoff) ------------------------

    fn has_flying_general() -> bool {
        // White fields no cannons, but the flying-general rule alone routes the
        // engine onto the per-move verify path — exactly what the Empire pieces'
        // board-aware move set and the contested-flag rule also need.
        true
    }

    fn extra_royal_attack<const R: usize>(
        board: &Board<Chess8x8, R>,
        sq: Square<Chess8x8>,
        by: Color,
        occupied: Bitboard<Chess8x8>,
    ) -> bool {
        // The king faceoff: `by`'s king attacks the enemy royal square `sq` iff they
        // share a file **or a rank** with no piece strictly between them (FSF's broad
        // `flyingGeneral = true`, identical to Synochess).
        let Some(king) = board.king_of(by) else {
            return false;
        };
        if king == sq {
            return false;
        }
        if king.file() != sq.file() && king.rank() != sq.rank() {
            return false;
        }
        (attacks::between::<Chess8x8>(king, sq) & occupied).is_empty()
    }

    // --- flag win (campmate) + stalemate scoring --------------------------

    fn has_flag_win() -> bool {
        true
    }

    // The flag goal ranks (White's last rank, Black's first) are exactly the
    // generic `flag_rank` default, so Empire does not override it.

    fn stalemate_is_loss() -> bool {
        true
    }

    fn has_castling() -> bool {
        true
    }
}

/// Empire as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position (standard Black vs the White Empire army) with
/// [`Empire::startpos`](GenericPosition::startpos) or parse a FEN (mcr dialect)
/// with [`Empire::from_fen`](GenericPosition::from_fen). See the [module
/// docs](self) for the piece movements, the Queen-only promotion, and the
/// flag-win / king-faceoff rules.
pub type Empire =
    GenericPosition<Chess8x8, EmpireRules, { <EmpireRules as WideVariant<Chess8x8>>::ROLE_SPAN }>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::position::WideOutcome;

    /// The canonical start FEN round-trips.
    #[test]
    fn startpos_round_trips() {
        let pos = Empire::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/PPPZZPPP/8/*T*E*C*DK*C*E*T w kq - 0 1"
        );
    }

    /// Campmate: a White king on rank 8 has won (the position is terminal and
    /// Black, to move, has no reply), and the win is credited to White.
    #[test]
    fn campmate_white_king_on_rank_eight_wins() {
        let pos = Empire::from_fen("3K1k2/8/8/8/8/8/8/8 b - - 0 1").expect("valid FEN");
        assert!(pos.legal_moves().is_empty(), "won position has no moves");
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::White
            })
        );
    }

    /// Campmate for Black: a Black king on rank 1 has won.
    #[test]
    fn campmate_black_king_on_rank_one_wins() {
        let pos = Empire::from_fen("8/8/8/8/8/8/4K3/3k4 w - - 0 1").expect("valid FEN");
        assert!(pos.legal_moves().is_empty());
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::Black
            })
        );
    }

    /// Stalemate is scored as a **loss** for the side to move (issue #498). The
    /// White king on a1 (its own back rank, not the rank-8 flag goal) has no legal
    /// move and is not in check — boxed by Black's standard-army queen b3 and king
    /// c2 — so White loses and Black wins. (Standard chess would call this a draw.)
    #[test]
    fn stalemate_is_a_loss() {
        let pos = Empire::from_fen("8/8/8/8/8/1q6/2k5/K7 w - - 0 1").expect("valid FEN");
        assert!(pos.legal_moves().is_empty(), "White has no legal move");
        assert!(!pos.is_check(), "White is not in check — a true stalemate");
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::Black
            })
        );
    }
}
