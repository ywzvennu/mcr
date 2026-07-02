//! Reusable, rules-only **analysis primitives** built on the validated attack
//! internals (issue #373).
//!
//! This module adds no new rules and changes no move generation. It exposes a
//! small, cohesive public surface over machinery that already exists and is
//! validated elsewhere:
//!
//! * the reverse-projecting [`attackers_to`](GenericPosition::attackers_to) — the
//!   per-variant, board-aware, directional / leg-asymmetric-correct "who attacks
//!   this square" query (issues #198, #201, #202, #359), and
//! * the per-role forward attack projection ([`WideVariant::role_attacks`] with
//!   the board-aware [`WideVariant::role_threats_board`] preference) the move
//!   generator itself uses.
//!
//! Everything here is pure geometry: there is **no evaluation, no scoring, and
//! no search**. The primitives are *queries* over the current position. Because
//! they delegate to the already-validated attack relation, they are variant-
//! correct for every variant the crate supports — including the directional
//! Soldiers/Pawns, the leg-asymmetric Xiangqi Horse, and the board-aware
//! cannons (Xiangqi / Janggi / Cannon Shogi) and capture-short Empire pieces —
//! without reimplementing any of that logic.
//!
//! # What each primitive answers
//!
//! * [`attackers_of`](GenericPosition::attackers_of) — the set of `side` pieces
//!   that attack a square, under the live board occupancy. A one-argument-lighter
//!   convenience over [`attackers_to`](GenericPosition::attackers_to).
//! * [`is_attacked`](GenericPosition::is_attacked) (defined on the position
//!   itself) — whether a square is attacked by a side.
//! * [`attack_map`](GenericPosition::attack_map) — the per-side attack bitboard:
//!   every square some piece of `side` attacks. Equal, square for square, to
//!   "`attackers_of` is non-empty", so the forward union and the reverse query
//!   agree by construction.
//! * [`defense_map`](GenericPosition::defense_map) — the squares of `side`'s own
//!   pieces that `side` also attacks (i.e. that are defended).
//! * [`piece_attacks`](GenericPosition::piece_attacks) /
//!   [`piece_mobility`](GenericPosition::piece_mobility) — the attack set, and its
//!   size, of the single piece standing on a square.
//! * [`attack_count`](GenericPosition::attack_count) — the number of squares a
//!   side attacks.
//!
//! [`WideVariant::role_attacks`]: crate::geometry::WideVariant::role_attacks
//! [`WideVariant::role_threats_board`]: crate::geometry::WideVariant::role_threats_board

use alloc::vec::Vec;

use super::bitboard::Bitboard;
use super::role::WideRole;
use super::square::Square;
use super::variant::WideVariant;
use super::wide_move::WideMove;
use super::GenericPosition;
use super::Geometry;
use crate::Color;

impl<G: Geometry, V: WideVariant<G>> GenericPosition<G, V> {
    /// The **attack (threat) set** of a `role` piece of `color` standing on
    /// `sq`, computed exactly as [`attackers_to`](Self::attackers_to) computes
    /// the forward relation it must reproduce.
    ///
    /// For an occupancy-only role this is [`WideVariant::role_attacks`]. For a
    /// board-aware variant (the screen cannons; Empire's capture-short pieces)
    /// it prefers the board-aware **threat** set
    /// [`WideVariant::role_threats_board`] — which for Empire excludes the
    /// move-only quiet Queen slides that are not attacks — falling back to the
    /// occupancy-only set when the hook returns `None`. This is the same
    /// preference the reverse-projecting attack query uses, so the two never
    /// disagree.
    fn role_threat_set(&self, role: WideRole, color: Color, sq: Square<G>) -> Bitboard<G> {
        let board = self.board();
        let occupied = board.occupied();
        if V::uses_board_attacks() {
            V::role_threats_board(role, color, sq, board)
                .unwrap_or_else(|| V::role_attacks(role, color, sq, occupied))
        } else {
            V::role_attacks(role, color, sq, occupied)
        }
    }

    /// Returns the set of `side` pieces that attack `square` under the current
    /// board occupancy.
    ///
    /// A convenience over [`attackers_to`](Self::attackers_to) that fills in the
    /// live occupancy for you; it reuses that validated, per-variant, board-aware
    /// reverse projection verbatim (directional Pawns/Soldiers, the leg-asymmetric
    /// Xiangqi Horse, and the screen cannons are all handled there). To detect
    /// attackers *through* a would-be-moved king — so the king does not shield
    /// itself — call [`attackers_to`](Self::attackers_to) directly with a custom
    /// occupancy.
    #[must_use]
    #[inline]
    pub fn attackers_of(&self, square: Square<G>, side: Color) -> Bitboard<G> {
        self.attackers_to(square, side, self.board().occupied())
    }

    /// The **attack (threat) set** of the piece standing on `square`, or `None`
    /// if `square` is empty.
    ///
    /// This is the single-piece forward projection the move generator and the
    /// attack relation use: the squares this piece attacks under the live
    /// occupancy, board-aware where the variant requires it (see
    /// [`WideVariant::role_threats_board`]). It is a *threat* set — for a piece
    /// that moves and captures differently (a cannon, an Empire capture-short
    /// piece) it reports the squares the piece could capture on, not its quiet
    /// move targets.
    #[must_use]
    pub fn piece_attacks(&self, square: Square<G>) -> Option<Bitboard<G>> {
        let board = self.board();
        let role = board.role_at(square)?;
        let color = board.color_at(square)?;
        Some(self.role_threat_set(role, color, square))
    }

    /// The number of squares the piece standing on `square` attacks (its
    /// mobility), or `0` if `square` is empty.
    ///
    /// The population count of [`piece_attacks`](Self::piece_attacks). This is a
    /// pure geometric mobility measure — the count of a piece's threatened
    /// squares — not an evaluation term.
    #[must_use]
    #[inline]
    pub fn piece_mobility(&self, square: Square<G>) -> u32 {
        self.piece_attacks(square).map_or(0, |bb| bb.count())
    }

    /// The **per-side attack bitboard**: every square attacked by at least one
    /// piece of `side`, under the current occupancy.
    ///
    /// Built as the forward union of each `side` piece's
    /// [`piece_attacks`](Self::piece_attacks) set, board-aware where the variant
    /// requires it. Equivalent, square for square, to "`attackers_of(sq, side)`
    /// is non-empty" (and hence to [`is_attacked`](Self::is_attacked)) — the
    /// forward union and the reverse query agree by construction, since both use
    /// the same per-role threat sets. Excludes the Xiangqi flying-general
    /// confrontation, which is a royal-only rule handled separately.
    #[must_use]
    pub fn attack_map(&self, side: Color) -> Bitboard<G> {
        let board = self.board();
        let mut acc = Bitboard::EMPTY;
        for role in WideRole::ALL {
            let pieces = board.pieces(side, role);
            if pieces.is_empty() {
                continue;
            }
            for from in pieces {
                acc |= self.role_threat_set(role, side, from);
            }
        }
        acc
    }

    /// The **per-side defense bitboard**: the squares of `side`'s *own* pieces
    /// that `side` also attacks — i.e. the pieces `side` defends.
    ///
    /// The intersection of [`attack_map`](Self::attack_map) with `side`'s
    /// occupancy. Purely geometric: a square is "defended" iff some friendly
    /// piece's threat set covers it, exactly the relation used for attacker /
    /// defender bookkeeping. No value judgement is made.
    #[must_use]
    #[inline]
    pub fn defense_map(&self, side: Color) -> Bitboard<G> {
        self.attack_map(side) & self.board().by_color(side)
    }

    /// The number of distinct squares `side` attacks — the population count of
    /// [`attack_map`](Self::attack_map).
    ///
    /// A cheap, geometry-only mobility / pressure measure. It is *not* a move
    /// count (a square attacked by several pieces counts once, and an attacked
    /// square need not be a legal move target); for the legal-move count of the
    /// side to move use [`legal_move_count`](Self::legal_move_count).
    #[must_use]
    #[inline]
    pub fn attack_count(&self, side: Color) -> u32 {
        self.attack_map(side).count()
    }

    /// The **royal (king) squares** of `color` — the squares whose occupant is
    /// royal, i.e. whose safety the check / checkmate rules protect.
    ///
    /// A thin, board-aware view of [`WideVariant::royal_squares`]: one square in
    /// standard chess and the single-king variants, possibly several for a
    /// multi-king variant (Spartan), and **empty** for a variant whose king is
    /// non-royal (Duck, Dobutsu's captured-king extinction rule). This is the
    /// anchor the other check queries project from — [`checkers_of`](Self::checkers_of)
    /// reports who attacks these squares and [`is_in_check`](Self::is_in_check)
    /// folds the variant's royal-attack discipline over them.
    #[must_use]
    #[inline]
    pub fn royal_squares(&self, color: Color) -> Bitboard<G> {
        V::royal_squares(self.board(), color)
    }

    /// The **checkers of `color`**: the enemy pieces that attack one of `color`'s
    /// royal squares under the live occupancy.
    ///
    /// The union, over every royal square of `color`, of
    /// [`attackers_of`](Self::attackers_of) by the opposing side — so the
    /// directional Pawns/Soldiers, the leg-asymmetric Xiangqi Horse, and the
    /// screen cannons are all handled by the validated reverse projection. Every
    /// square it reports holds an enemy piece whose threat set covers a royal.
    ///
    /// This is the *piece* relation: it does **not** include the royal-only Xiangqi
    /// flying-general confrontation (an open file between the two generals, which
    /// is not a piece attack); use [`is_in_check`](Self::is_in_check) for the full
    /// variant-correct in-check verdict. For a single-royal side a non-empty
    /// checker set means the side is in check; under multi-king duple-check
    /// (Spartan) a side is in check only when *every* king is attacked, so a
    /// non-empty set alone need not be a check there.
    #[must_use]
    pub fn checkers_of(&self, color: Color) -> Bitboard<G> {
        let them = color.opposite();
        let mut acc = Bitboard::EMPTY;
        for royal in self.royal_squares(color) {
            acc |= self.attackers_of(royal, them);
        }
        acc
    }

    /// The **checkers of the side to move** — [`checkers_of`](Self::checkers_of)
    /// for [`turn`](Self::turn). The pieces the mover must answer.
    #[must_use]
    #[inline]
    pub fn checkers(&self) -> Bitboard<G> {
        self.checkers_of(self.turn())
    }

    /// The legal moves of the side to move whose **origin is `square`** — the
    /// per-piece slice of [`legal_moves`](Self::legal_moves).
    ///
    /// A filter over the validated legal-move list, so it never invents a move the
    /// generator would not: every legality rule (pins, check evasion, multi-royal,
    /// cannon screens, gating) is already applied. Because each legal move has
    /// exactly one origin, the lists returned for the distinct board squares
    /// partition [`legal_moves`](Self::legal_moves). A **drop** is packed with its
    /// origin equal to its target, so it is grouped under the square it drops onto.
    #[must_use]
    pub fn legal_moves_from(&self, square: Square<G>) -> Vec<WideMove> {
        self.legal_moves()
            .into_iter()
            .filter(|mv| mv.from::<G>() == square)
            .collect()
    }
}
