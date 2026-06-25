//! Racing Kings as a [`Variant`]: a race to get your king to the eighth rank,
//! played on a pawnless board where no move may ever give or stand in check.
//!
//! # Rules
//!
//! The starting placement is `8/8/8/8/8/8/krbnNBRK/qrbnNBRQ w - - 0 1`: the two
//! back ranks hold both armies' pieces (queens, rooks, bishops, knights, and the
//! kings) with **no pawns** anywhere, so promotion and en passant never arise.
//! There is no castling.
//!
//! Both sides share one over-riding restriction: **no move may leave either king
//! in check.** A side may not move into check (standard) *and* may not give
//! check to the opponent either. This is the only king-safety departure from
//! standard chess, so the variant turns off the fast-legality sentinel and
//! filters pseudo-legal moves through [`Variant::is_legal_after`] (H2).
//!
//! # Winning, and the both-reach draw
//!
//! A king that reaches the eighth rank wins. The one subtlety is fairness to the
//! second player: because White moves first, if White's king reaches rank 8 then
//! Black is given one final move; if Black's king can also reach rank 8 on that
//! move the game is a **draw** (both kings finished), otherwise White has won.
//!
//! ## How the history-free hooks implement this
//!
//! Both the terminal hook [`Variant::extra_terminal`] (H1) and the move-filter
//! hook [`Variant::filter_forced`] (H7) see only a single [`Position`], not the
//! move history, so "Black gets one more move" is expressed positionally by the
//! [`race_over`] predicate:
//!
//! - With **no** king on rank 8, the race is still on.
//! - With it **White** to move, or a **Black** king on rank 8, the race is over.
//! - Otherwise it is **Black** to move with only **White** home: the race is over
//!   (White won) unless Black's king can also step onto rank 8 this move, in which
//!   case play continues for that one answering move. After it, either both kings
//!   are home (a **draw**) or Black failed and it is White's turn with White home
//!   (**White won**) — both terminal.
//!
//! This is exactly the standard Racing Kings rule and matches the reference
//! engine's move counts. When the race is over [`Variant::filter_forced`] empties
//! the legal-move list, so a finished position is a leaf for movegen, outcome
//! detection, and perft even though pieces could still physically move.
//!
//! The draw / decisive outcomes are encoded through the shared [`EndReason`].
//! A lone king home is reported as [`EndReason::RaceFinished`], whose
//! [`EndReason::outcome`] awards the win to the side *not* to move (always the
//! side that just moved, so the win is awarded correctly), and the both-home
//! case as [`EndReason::RaceDraw`] (an automatic draw).

use super::{Variant, VariantId};
use crate::board::Board;
use crate::position::CastlingRights;
use crate::{Color, EndReason, Move, Position, Rank, Role, Square};

/// The rank a king must reach to finish the race.
const GOAL_RANK: Rank = Rank::Eighth;

/// The Racing Kings rule layer: a pawnless race to the eighth rank where no move
/// may give or stand in check. A zero-sized marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RacingKingsRules;

impl Variant for RacingKingsRules {
    type State = ();
    const ID: VariantId = VariantId::RacingKings;

    /// King safety differs from standard chess (neither king may be checked), so
    /// the fast core generator cannot be used; generation runs the pseudo-legal +
    /// [`Variant::is_legal_after`] make-move filter instead.
    const USES_FAST_LEGALITY: bool = false;

    /// H2: a move is legal iff, in the resulting position, **neither** king is in
    /// check — the mover is not left in check (standard) *and* the opponent is not
    /// put in check (the Racing Kings restriction).
    fn is_legal_after(_parent: &Position, _mv: &Move, child: &Position) -> bool {
        !either_king_in_check(child)
    }

    /// H1: report the race outcome once the race has ended (see `race_over` for
    /// when that is, including the both-reach draw subtlety).
    ///
    /// The mapping mirrors the standard Racing Kings outcome:
    /// - both kings on rank 8 → draw;
    /// - only White's king on rank 8 → White wins;
    /// - only Black's king on rank 8 → Black wins.
    ///
    /// In every position reachable by legal play the home king belongs to the
    /// side that just moved — i.e. the side *not* to move — so a lone-king win is
    /// reported as [`EndReason::RaceFinished`], whose outcome awards the win to
    /// exactly that side. The both-home case is reported as the draw
    /// [`EndReason::RaceDraw`].
    ///
    /// (A FEN can construct the degenerate "the winning king is home and it is
    /// that same side to move" position, which never arises from play because
    /// racing onto rank 8 passes the move to the opponent; `RaceFinished` would
    /// name the wrong winner there. Such a position is not produced by
    /// `race_over` during play and is documented as out of scope.)
    fn extra_terminal(core: &Position, _state: &Self::State) -> Option<EndReason> {
        if !race_over(core) {
            return None;
        }
        let both_home = king_on_goal(core, Color::White) && king_on_goal(core, Color::Black);
        Some(if both_home {
            // Both kings finished: a drawn race.
            EndReason::RaceDraw
        } else {
            // Exactly one king home, belonging to the side that just moved (the
            // side not to move): a decisive win for that side.
            EndReason::RaceFinished
        })
    }

    /// H7: once the race is over no further moves are legal, even though pieces
    /// could still physically move. Emptying the list here makes the game-over
    /// position a leaf for movegen, outcome detection, and perft, matching the
    /// reference Racing Kings semantics.
    fn filter_forced(core: &Position, _state: &Self::State, moves: &mut Vec<Move>) {
        if race_over(core) {
            moves.clear();
        }
    }

    /// H3: the king is royal — check fully applies (and, uniquely here, the
    /// opponent's king may not be checked either, enforced in `is_legal_after`).
    fn king_is_royal() -> bool {
        true
    }

    /// Racing Kings has no insufficient-material draw: a lone king still races to
    /// the goal rank, so sparse material never ends the game on its own. Only the
    /// race finish, stalemate, and the move-count draws apply.
    fn insufficient_material_is_draw() -> bool {
        false
    }

    /// H9: Racing Kings has no castling.
    fn castling_allowed() -> bool {
        false
    }

    /// H11: the pawnless racing start with no castling rights and unit state.
    fn starting_board() -> (Board, CastlingRights, Self::State) {
        (racing_start_board(), CastlingRights::NONE, ())
    }
}

/// The back-rank role layout of the Racing Kings start, file `a` through `h`.
///
/// Rank 1 (`qrbnNBRQ`) and rank 2 (`krbnNBRK`) share this shape; the colours and
/// the king/queen swap are applied per square in [`racing_start_board`].
const RACING_BACK_RANK: [Role; 8] = [
    Role::Queen,
    Role::Rook,
    Role::Bishop,
    Role::Knight,
    Role::Knight,
    Role::Bishop,
    Role::Rook,
    Role::Queen,
];

/// Builds the Racing Kings starting board `8/8/8/8/8/8/krbnNBRK/qrbnNBRQ`.
///
/// The lower half of each back rank (files a–d) is Black, the upper half (files
/// e–h) is White; rank 2 carries the kings on the a-/h-files where rank 1 carries
/// the queens. No pawns are placed.
fn racing_start_board() -> Board {
    use crate::Piece;

    let mut board = Board::empty();
    for (file_index, &role) in RACING_BACK_RANK.iter().enumerate() {
        let file = crate::File::new(file_index as u8).expect("file index in 0..8");
        // Files a–d are Black, files e–h are White.
        let color = if file_index < 4 {
            Color::Black
        } else {
            Color::White
        };

        // Rank 1 holds the role from `RACING_BACK_RANK`; rank 2 is identical
        // except the outer queens become kings (`qrbnNBRQ` -> `krbnNBRK`).
        let rank1_sq = Square::from_file_rank(file, Rank::First);
        board.set_piece(rank1_sq, Piece::new(color, role));

        let rank2_role = if role == Role::Queen {
            Role::King
        } else {
            role
        };
        let rank2_sq = Square::from_file_rank(file, Rank::Second);
        board.set_piece(rank2_sq, Piece::new(color, rank2_role));
    }
    board
}

/// Whether `color`'s king stands on the goal rank in `core`.
fn king_on_goal(core: &Position, color: Color) -> bool {
    core.board()
        .king_of(color)
        .is_some_and(|sq| sq.rank() >= GOAL_RANK)
}

/// Whether either king is currently in check in `pos` — the Racing Kings legality
/// predicate. A king is "in check" if it is attacked by the opposing side.
fn either_king_in_check(pos: &Position) -> bool {
    let board = pos.board();
    for color in Color::ALL {
        if let Some(king) = board.king_of(color) {
            if pos.is_attacked(king, color.opposite()) {
                return true;
            }
        }
    }
    false
}

/// Whether the race has ended in `core`, i.e. no further move is legal.
///
/// This is the Racing Kings game-over test, used both to report the outcome
/// ([`RacingKingsRules::extra_terminal`]) and to empty the legal-move list
/// ([`RacingKingsRules::filter_forced`]). The rule, expressed positionally:
///
/// - If no king is on the goal rank, the race is still on.
/// - If it is **White** to move, or a **Black** king is on the goal rank, the
///   race is over (White's king on rank 8 with White to move is the settled
///   White win; a Black king on rank 8 is the settled Black win or both-home
///   draw).
/// - Otherwise it is **Black** to move with only **White's** king on the goal
///   rank — White has just arrived and Black gets one answering move. The race is
///   over (White wins) **unless** Black's king can step onto an unoccupied
///   goal-rank square that White does not attack, in which case Black may yet
///   draw and the race continues.
///
/// The Black-catch-up test mirrors the reference engine: a candidate target is a
/// king-step square on the goal rank, not occupied by a Black piece, that is not
/// attacked by White under the current occupancy (the Black king still on its
/// origin, exactly as when the answering move is made).
fn race_over(core: &Position) -> bool {
    let white_home = king_on_goal(core, Color::White);
    let black_home = king_on_goal(core, Color::Black);
    if !white_home && !black_home {
        return false;
    }
    if core.turn() == Color::White || black_home {
        return true;
    }

    // White's king is home and it is Black to move: the race continues only if
    // Black's king can also reach the goal rank this move.
    let Some(black_king) = core.board().king_of(Color::Black) else {
        return true;
    };
    for target in crate::attacks::king_attacks(black_king) {
        if target.rank() < GOAL_RANK {
            continue;
        }
        if core.board().color_at(target) == Some(Color::Black) {
            continue;
        }
        if !core.is_attacked(target, Color::White) {
            // Black can step to an undefended goal square and draw: not over yet.
            return false;
        }
    }
    true
}

/// Racing Kings as a [`VariantPosition`](super::VariantPosition).
///
/// Movegen runs the slow pseudo-legal + make-move filter (king safety differs:
/// neither king may be checked), there is no castling, and the race-to-rank-8 win
/// — including the both-reach draw — is reported through
/// [`VariantPosition::outcome`](super::VariantPosition::outcome).
pub type RacingKings = super::VariantPosition<RacingKingsRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variant::perft_variant;
    use crate::{Color, EndReason, Outcome, VariantId};

    const START_FEN: &str = "8/8/8/8/8/8/krbnNBRK/qrbnNBRQ w - - 0 1";

    fn play_line(mut pos: RacingKings, ucis: &[&str]) -> RacingKings {
        for uci in ucis {
            let mv = pos.parse_uci(uci).expect("legal uci move");
            pos = pos.play(&mv);
        }
        pos
    }

    #[test]
    fn startpos_is_the_racing_layout() {
        let pos = RacingKings::startpos();
        assert_eq!(pos.variant_id(), VariantId::RacingKings);
        assert_eq!(pos.turn(), Color::White);
        assert_eq!(pos.to_fen(), START_FEN);
        // Parsing the canonical FEN yields the same position.
        let parsed = RacingKings::from_fen(START_FEN).unwrap();
        assert_eq!(parsed, pos);
        // Published shakmaty start perft(1).
        assert_eq!(pos.legal_moves().len(), 21);
        assert!(pos.outcome().is_none());
    }

    #[test]
    fn no_castling_rights_in_start() {
        let pos = RacingKings::startpos();
        assert_eq!(pos.core().castling_rights(), CastlingRights::NONE);
        assert!(!RacingKingsRules::castling_allowed());
    }

    #[test]
    fn no_legal_move_gives_or_stands_in_check() {
        // Exhaustively to a shallow depth: no position reachable by legal play
        // ever has either king in check.
        fn walk(pos: &RacingKings, depth: u32) {
            assert!(
                !either_king_in_check(pos.core()),
                "reached a position with a king in check: {}",
                pos.to_fen()
            );
            if depth == 0 {
                return;
            }
            for mv in pos.legal_moves() {
                walk(&pos.play(&mv), depth - 1);
            }
        }
        walk(&RacingKings::startpos(), 3);
    }

    #[test]
    fn moving_into_check_is_illegal() {
        // White rook on a1, black king on c7 (no current check). Ra1-a7 would
        // attack the black king along the seventh rank, giving check, which is
        // illegal in Racing Kings even though it is not a self-check.
        let pos: RacingKings = "8/2k5/8/8/8/8/7K/R7 w - - 0 1".parse().unwrap();
        let ucis: Vec<String> = pos.legal_moves().iter().map(|m| m.to_uci()).collect();
        assert!(
            !ucis.iter().any(|m| m == "a1a7"),
            "Ra7+ gives check and must be illegal: {ucis:?}"
        );
        // The rook may still move where it gives no check, e.g. b1.
        assert!(ucis.iter().any(|m| m == "a1b1"), "Rb1 should be legal");
    }

    #[test]
    fn white_king_reaching_rank_eight_wins() {
        // White king one step from the goal with Black unable to follow: White
        // races in and wins outright.
        let pos: RacingKings = "8/6K1/8/8/8/8/8/k7 w - - 0 1".parse().unwrap();
        let after = play_line(pos, &["g7g8"]);
        assert!(king_on_goal(after.core(), Color::White));
        assert_eq!(after.turn(), Color::Black);
        assert_eq!(
            after.outcome(),
            Some(Outcome::Decisive {
                winner: Color::White
            })
        );
        assert_eq!(after.end_reason(), Some(EndReason::RaceFinished));
    }

    #[test]
    fn black_king_reaching_rank_eight_wins() {
        // Symmetric: Black king steps onto rank 8 and wins immediately.
        let pos: RacingKings = "8/6k1/8/8/8/8/8/K7 b - - 0 1".parse().unwrap();
        let after = play_line(pos, &["g7g8"]);
        assert!(king_on_goal(after.core(), Color::Black));
        assert_eq!(after.turn(), Color::White);
        assert_eq!(
            after.outcome(),
            Some(Outcome::Decisive {
                winner: Color::Black
            })
        );
        assert_eq!(after.end_reason(), Some(EndReason::RaceFinished));
    }

    #[test]
    fn both_kings_on_rank_eight_is_a_draw() {
        // White already home on a8; Black to move with its king on g7 able to
        // step to g8 (or f8/h8). Under the "one more move" rule the race is NOT
        // yet over: Black still gets to play, so the position has no outcome and
        // offers Black its catch-up moves.
        let pos: RacingKings = "K7/6k1/8/8/8/8/8/8 b - - 0 1".parse().unwrap();
        assert!(
            pos.outcome().is_none(),
            "Black still has its answering move"
        );
        let ucis: Vec<String> = pos.legal_moves().iter().map(|m| m.to_uci()).collect();
        assert!(
            ucis.iter().any(|m| m == "g7g8"),
            "Black must be offered the catch-up move: {ucis:?}"
        );

        // After Black actually steps onto rank 8, both kings are home: a draw.
        let after = play_line(pos, &["g7g8"]);
        assert!(king_on_goal(after.core(), Color::White));
        assert!(king_on_goal(after.core(), Color::Black));
        assert_eq!(after.outcome(), Some(Outcome::Draw));
        assert_eq!(after.end_reason(), Some(EndReason::RaceDraw));
    }

    #[test]
    fn white_home_but_black_cannot_follow_is_white_win() {
        // White on a8; Black king on a1 cannot reach rank 8 in one move -> White
        // has won despite Black's answering move.
        let pos: RacingKings = "K7/8/8/8/8/8/8/k7 b - - 0 1".parse().unwrap();
        assert_eq!(
            pos.outcome(),
            Some(Outcome::Decisive {
                winner: Color::White
            })
        );
        assert_eq!(pos.end_reason(), Some(EndReason::RaceFinished));
    }

    #[test]
    fn start_perft_shallow() {
        // Published shakmaty `racingkings.perft` start counts (cheap depths).
        let pos = RacingKings::startpos();
        assert_eq!(perft_variant(&pos, 1), 21);
        assert_eq!(perft_variant(&pos, 2), 421);
        assert_eq!(perft_variant(&pos, 3), 11264);
    }

    #[test]
    fn fen_round_trip() {
        for fen in [
            START_FEN,
            "8/6K1/8/8/8/8/8/k7 w - - 0 1",
            "4brn1/2K2k2/8/8/8/8/8/8 w - - 0 1",
        ] {
            let pos: RacingKings = fen.parse().unwrap();
            assert_eq!(pos.to_fen(), fen, "round trip for {fen}");
        }
    }
}
