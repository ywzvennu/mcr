//! Standard Algebraic Notation (SAN) for [`Position`].
//!
//! SAN is the move notation used by the PGN standard and by virtually all human
//! game records: `e4`, `Nf3`, `exd5`, `O-O`, `e8=Q`, `Qxh7#`. Unlike UCI long
//! algebraic notation, SAN is *minimal* — it names only what is needed to
//! identify the move unambiguously in the position it is played in, so both
//! rendering and parsing are inherently methods on a [`Position`].
//!
//! This module adds two inherent methods:
//!
//! - [`Position::san`] renders a legal move as canonical SAN.
//! - [`Position::parse_san`] resolves a SAN string to the concrete legal
//!   [`Move`] in this position.
//!
//! # Rendering rules
//!
//! - **Castling** is `O-O` (king-side) and `O-O-O` (queen-side), using the
//!   letter `O`.
//! - **Pawn moves** use no piece letter: a push is the destination square
//!   (`e4`), a capture is the origin file, `x`, and the destination (`exd5`).
//!   En-passant captures render exactly like an ordinary pawn capture (`exd6`).
//!   Promotions append `=` and the uppercase promotion letter (`e8=Q`,
//!   `exd8=N`).
//! - **Other pieces** use the uppercase piece letter, optional disambiguation,
//!   an `x` for captures, and the destination (`Nf3`, `Qxd7`, `R1a3`).
//! - **Disambiguation** is added only when more than one legal piece of that
//!   role can reach the destination: the origin file is used when it suffices,
//!   otherwise the origin rank, otherwise both.
//! - A trailing `+` marks a move that gives check and `#` a move that gives
//!   checkmate; both are computed by playing the move.
//!
//! # Parsing
//!
//! Parsing always *emits* the canonical forms above but is lenient on input: it
//! accepts `0-0`/`0-0-0` for castling, a bare promotion letter without the `=`
//! (`e8Q`), and tolerates trailing check/mate (`+`/`#`) and annotation
//! (`!`/`?`) glyphs. It resolves the SAN against the position's legal moves and
//! rejects anything illegal or ambiguous with a [`SanError`].

use core::fmt;

use crate::{File, Move, MoveKind, Position, Rank, Role, Square};

/// The error returned when a SAN string cannot be resolved against a position by
/// [`Position::parse_san`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SanError {
    /// The string was empty or not recognizable as SAN at all.
    Empty,
    /// The move-text was syntactically malformed (bad squares, stray
    /// characters, a promotion where none is possible, and so on).
    Malformed,
    /// The SAN named no legal move in this position.
    Illegal,
    /// The SAN was well-formed but matched more than one legal move; more
    /// disambiguation was required.
    Ambiguous,
}

impl fmt::Display for SanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SanError::Empty => f.write_str("empty SAN string"),
            SanError::Malformed => f.write_str("malformed SAN string"),
            SanError::Illegal => f.write_str("SAN names no legal move in this position"),
            SanError::Ambiguous => f.write_str("SAN matches more than one legal move"),
        }
    }
}

impl std::error::Error for SanError {}

impl Position {
    /// Renders the legal move `mv` as canonical Standard Algebraic Notation.
    ///
    /// The move must be legal in this position (as returned by
    /// [`Position::legal_moves`]); the rendering reads the moving piece from the
    /// board and computes the check/mate suffix by playing the move.
    ///
    /// ```
    /// use mce::Position;
    /// let pos = Position::startpos();
    /// let e4 = pos.parse_uci("e2e4").unwrap();
    /// assert_eq!(pos.san(&e4), "e4");
    /// let nf3 = pos.parse_uci("g1f3").unwrap();
    /// assert_eq!(pos.san(&nf3), "Nf3");
    /// ```
    #[must_use]
    pub fn san(&self, mv: &Move) -> String {
        let mut s = String::with_capacity(8);

        match mv.kind() {
            MoveKind::CastleKingside => s.push_str("O-O"),
            MoveKind::CastleQueenside => s.push_str("O-O-O"),
            _ => {
                let role = self
                    .board()
                    .piece_at(mv.from())
                    .map_or(Role::Pawn, |p| p.role);
                if role == Role::Pawn {
                    self.write_pawn_san(&mut s, mv);
                } else {
                    self.write_piece_san(&mut s, mv, role);
                }
            }
        }

        // Check / checkmate suffix, computed by playing the move.
        let after = self.play(mv);
        if after.is_check() {
            let suffix = if after.legal_move_count() == 0 {
                '#'
            } else {
                '+'
            };
            s.push(suffix);
        }

        s
    }

    /// Writes the body (everything but the check suffix) of a pawn move.
    fn write_pawn_san(&self, s: &mut String, mv: &Move) {
        let is_capture = mv.is_capture();
        if is_capture {
            // Captures (ordinary, en passant, capturing promotion) lead with the
            // origin file and an `x`.
            s.push(mv.from().file().char());
            s.push('x');
        }
        s.push_str(&mv.to().to_string());
        if let Some(role) = mv.promotion() {
            s.push('=');
            s.push(role.upper_char());
        }
    }

    /// Writes the body of a non-pawn, non-castling move.
    fn write_piece_san(&self, s: &mut String, mv: &Move, role: Role) {
        s.push(role.upper_char());
        self.write_disambiguation(s, mv, role);
        if mv.is_capture() {
            s.push('x');
        }
        s.push_str(&mv.to().to_string());
    }

    /// Appends the minimal disambiguation (file, rank, or both) needed to single
    /// out `mv` among legal moves of the same `role` reaching the same square.
    fn write_disambiguation(&self, s: &mut String, mv: &Move, role: Role) {
        let from = mv.from();
        let to = mv.to();

        // Origins of other legal moves of this role that also reach `to`.
        let mut same_file = false;
        let mut same_rank = false;
        let mut any_other = false;
        for other in self.legal_moves() {
            if other.to() != to || other.from() == from {
                continue;
            }
            if self.board().piece_at(other.from()).map(|p| p.role) != Some(role) {
                continue;
            }
            any_other = true;
            if other.from().file() == from.file() {
                same_file = true;
            }
            if other.from().rank() == from.rank() {
                same_rank = true;
            }
        }

        if !any_other {
            return;
        }
        // Prefer the file; if a rival shares the file, the rank distinguishes;
        // if rivals share both, use the full origin square.
        if !same_file {
            s.push(from.file().char());
        } else if !same_rank {
            s.push(from.rank().char());
        } else {
            s.push(from.file().char());
            s.push(from.rank().char());
        }
    }

    /// Resolves a SAN string to the concrete legal [`Move`] in this position.
    ///
    /// Parsing emits canonical SAN but accepts several common input variants:
    /// `0-0`/`0-0-0` for castling, a bare promotion letter without `=` (`e8Q`),
    /// and trailing `+`/`#`/`!`/`?` glyphs.
    ///
    /// ```
    /// use mce::Position;
    /// let pos = Position::startpos();
    /// let nf3 = pos.parse_san("Nf3").unwrap();
    /// assert_eq!(nf3.to_uci(), "g1f3");
    /// // Lenient input still resolves.
    /// assert_eq!(pos.parse_san("Ng1f3+").unwrap(), nf3);
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`SanError`] if the string is empty, malformed, names no legal
    /// move, or is ambiguous between several legal moves.
    pub fn parse_san(&self, s: &str) -> Result<Move, SanError> {
        // Strip trailing annotation and check/mate glyphs; they are advisory and
        // not needed to identify the move.
        let core = s.trim().trim_end_matches(['+', '#', '!', '?']);
        if core.is_empty() {
            return Err(SanError::Empty);
        }

        // Castling, accepting both the letter `O` and the digit `0`.
        if core == "O-O-O" || core == "0-0-0" {
            return self.find_castle(MoveKind::CastleQueenside);
        }
        if core == "O-O" || core == "0-0" {
            return self.find_castle(MoveKind::CastleKingside);
        }

        let parsed = parse_target_san(core)?;
        self.resolve(&parsed)
    }

    /// Finds the unique legal castling move of the given kind.
    fn find_castle(&self, kind: MoveKind) -> Result<Move, SanError> {
        self.legal_moves()
            .into_iter()
            .find(|m| m.kind() == kind)
            .ok_or(SanError::Illegal)
    }

    /// Matches a parsed SAN description against the legal moves, enforcing
    /// uniqueness.
    fn resolve(&self, p: &ParsedSan) -> Result<Move, SanError> {
        let mut found: Option<Move> = None;
        for mv in self.legal_moves() {
            if !self.matches(&mv, p) {
                continue;
            }
            if found.is_some() {
                return Err(SanError::Ambiguous);
            }
            found = Some(mv);
        }
        found.ok_or(SanError::Illegal)
    }

    /// Whether the legal move `mv` is described by the parsed SAN `p`.
    fn matches(&self, mv: &Move, p: &ParsedSan) -> bool {
        if mv.is_castle() {
            return false;
        }
        if mv.to() != p.to {
            return false;
        }
        let role = match self.board().piece_at(mv.from()) {
            Some(piece) => piece.role,
            None => return false,
        };
        if role != p.role {
            return false;
        }
        // Promotion role must match exactly when specified, and a promotion in
        // the SAN requires a promoting move (and vice versa).
        if mv.promotion() != p.promotion {
            return false;
        }
        // A capture indicator (`x`, or a pawn origin-file before a different
        // destination file) must agree for pawns; for pieces `x` is advisory but
        // we still honor an explicit capture/no-capture mismatch only loosely:
        // SAN consumers rely on origin hints, which we check below.
        if let Some(file) = p.from_file {
            if mv.from().file() != file {
                return false;
            }
        }
        if let Some(rank) = p.from_rank {
            if mv.from().rank() != rank {
                return false;
            }
        }
        true
    }
}

/// A SAN move parsed into its constituent constraints, used to filter the legal
/// moves of a position.
struct ParsedSan {
    role: Role,
    to: Square,
    from_file: Option<File>,
    from_rank: Option<Rank>,
    promotion: Option<Role>,
}

/// Parses the "target" form of SAN (everything except castling) into a
/// [`ParsedSan`]. The string must already have its trailing glyphs stripped.
fn parse_target_san(core: &str) -> Result<ParsedSan, SanError> {
    let bytes = core.as_bytes();
    let mut i = 0;

    // Leading piece letter (uppercase). Absent means a pawn move.
    let role = match bytes[0] {
        b'N' | b'B' | b'R' | b'Q' | b'K' => {
            let r = Role::from_char(bytes[0] as char).ok_or(SanError::Malformed)?;
            i += 1;
            r
        }
        _ => Role::Pawn,
    };

    // Optional promotion suffix: `=X` or a bare trailing role letter `X`.
    let mut promotion = None;
    let mut end = core.len();
    if let Some(stripped) = core.strip_suffix(['=']) {
        // A dangling `=` with nothing after it is malformed.
        let _ = stripped;
        return Err(SanError::Malformed);
    }
    // Look for `=X`.
    if core.len() >= 2 && bytes[core.len() - 2] == b'=' {
        let role_ch = bytes[core.len() - 1] as char;
        promotion = Some(promo_role(role_ch)?);
        end = core.len() - 2;
    } else if role == Role::Pawn {
        // Bare promotion letter (e.g. `e8Q`): only for pawn move-text, and only
        // when the final character is a promotable role letter following a rank
        // digit.
        let last = bytes[core.len() - 1];
        if matches!(last, b'N' | b'B' | b'R' | b'Q' | b'n' | b'b' | b'r' | b'q') {
            promotion = Some(promo_role(last as char)?);
            end = core.len() - 1;
        }
    }

    // The remaining body, `core[i..end]`, is: optional origin file, optional
    // origin rank, optional `x`, then the mandatory destination square.
    let body = &core[i..end];
    let bb = body.as_bytes();
    if bb.len() < 2 {
        return Err(SanError::Malformed);
    }

    // The destination is always the final two characters (file + rank).
    let to: Square = body[bb.len() - 2..]
        .parse()
        .map_err(|_| SanError::Malformed)?;

    // Everything before the destination is the disambiguation / capture marker.
    let prefix = &bb[..bb.len() - 2];
    let mut from_file = None;
    let mut from_rank = None;
    for (j, &ch) in prefix.iter().enumerate() {
        match ch {
            b'a'..=b'h' if from_file.is_none() && from_rank.is_none() => {
                from_file = File::from_char(ch as char);
            }
            b'1'..=b'8' if from_rank.is_none() => {
                from_rank = Rank::from_char(ch as char);
            }
            // The capture marker is allowed only as the last prefix character.
            b'x' | b':' if j + 1 == prefix.len() => {}
            _ => return Err(SanError::Malformed),
        }
    }

    // Promotions are only meaningful for pawn moves.
    if promotion.is_some() && role != Role::Pawn {
        return Err(SanError::Malformed);
    }

    Ok(ParsedSan {
        role,
        to,
        from_file,
        from_rank,
        promotion,
    })
}

/// Parses a promotion role letter, rejecting pawn and king.
fn promo_role(ch: char) -> Result<Role, SanError> {
    match Role::from_char(ch) {
        Some(r @ (Role::Knight | Role::Bishop | Role::Rook | Role::Queen)) => Ok(r),
        _ => Err(SanError::Malformed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Position;

    fn pos(fen: &str) -> Position {
        Position::from_fen(fen).unwrap()
    }

    /// Every legal move of `pos` must round-trip through SAN.
    fn assert_round_trips(p: &Position) {
        for mv in p.legal_moves() {
            let san = p.san(&mv);
            let parsed = p
                .parse_san(&san)
                .unwrap_or_else(|e| panic!("parse_san({san:?}) failed: {e} in {}", p.to_fen()));
            assert_eq!(
                parsed,
                mv,
                "round-trip mismatch for {san:?} (uci {}) in {}",
                mv.to_uci(),
                p.to_fen()
            );
        }
    }

    #[test]
    fn round_trip_startpos() {
        assert_round_trips(&Position::startpos());
    }

    #[test]
    fn round_trip_kiwipete() {
        assert_round_trips(&pos(
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        ));
    }

    #[test]
    fn round_trip_promotions() {
        // White pawns ready to promote, with capture targets on the 8th rank.
        assert_round_trips(&pos("1n2k3/P1P5/8/8/8/8/8/4K3 w - - 0 1"));
        // Black to move with promotions.
        assert_round_trips(&pos("4k3/8/8/8/8/8/p1p5/1N2K3 b - - 0 1"));
    }

    #[test]
    fn round_trip_disambiguation() {
        // Two knights both reach d2 / f3 etc.; two rooks share a file.
        assert_round_trips(&pos("4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1"));
        assert_round_trips(&pos("8/8/8/8/8/2N1N3/8/4K2k w - - 0 1"));
        // Three queens able to reach a shared square (file+rank disambiguation).
        assert_round_trips(&pos("8/8/2k5/Q6Q/8/8/8/Q3K3 w - - 0 1"));
    }

    #[test]
    fn round_trip_en_passant() {
        let p = pos("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1");
        assert_round_trips(&p);
        // The en-passant capture renders as a normal pawn capture.
        let ep = p.parse_uci("e5d6").unwrap();
        assert_eq!(ep.kind(), MoveKind::EnPassant);
        assert_eq!(p.san(&ep), "exd6");
    }

    #[test]
    fn golden_piece_moves() {
        let p = Position::startpos();
        assert_eq!(p.san(&p.parse_uci("g1f3").unwrap()), "Nf3");
        assert_eq!(p.san(&p.parse_uci("e2e4").unwrap()), "e4");

        let p = pos("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2");
        // exd5 pawn capture.
        assert_eq!(p.san(&p.parse_uci("e4d5").unwrap()), "exd5");
    }

    #[test]
    fn golden_knight_disambiguation() {
        // Knights on b1 and f3 can both reach d2.
        let p = pos("4k3/8/8/8/8/5N2/8/1N2K3 w - - 0 1");
        let nbd2 = p.parse_uci("b1d2").unwrap();
        let ngd2 = p.parse_uci("f3d2").unwrap();
        assert_eq!(p.san(&nbd2), "Nbd2");
        assert_eq!(p.san(&ngd2), "Nfd2");
        // Parse them back.
        assert_eq!(p.parse_san("Nbd2").unwrap(), nbd2);
        assert_eq!(p.parse_san("Nfd2").unwrap(), ngd2);
        // The bare `Nd2` is ambiguous.
        assert_eq!(p.parse_san("Nd2").unwrap_err(), SanError::Ambiguous);
    }

    #[test]
    fn golden_rank_disambiguation() {
        // Rooks on a1 and a3 share the a-file, so the rank disambiguates moves to
        // a2.
        let p = pos("4k3/8/8/8/8/R7/8/R3K3 w - - 0 1");
        let r1a2 = p.parse_uci("a1a2").unwrap();
        let r3a2 = p.parse_uci("a3a2").unwrap();
        assert_eq!(p.san(&r1a2), "R1a2");
        assert_eq!(p.san(&r3a2), "R3a2");
        assert_eq!(p.parse_san("R1a2").unwrap(), r1a2);
        assert_eq!(p.parse_san("R3a2").unwrap(), r3a2);
    }

    #[test]
    fn golden_both_origin_square() {
        // Three white queens all reach d4: d1 (file d), d8 (file d), a1
        // (diagonal a1-d4, rank 1). The queen on d1 shares its file with d8 and
        // its rank with a1, so neither a file nor a rank alone disambiguates and
        // the SAN must spell the full origin square `Qd1d4`.
        let p = pos("3Q4/8/8/8/8/8/8/Q2QK2k w - - 0 1");
        let mv = p.parse_uci("d1d4").unwrap();
        assert_eq!(p.san(&mv), "Qd1d4");
        assert_eq!(p.parse_san("Qd1d4").unwrap(), mv);
    }

    #[test]
    fn golden_promotion() {
        // Black king off the a-file and rank 8 so the promotion is quiet.
        let p = pos("8/P7/4k3/8/8/8/8/4K3 w - - 0 1");
        let q = p.parse_uci("a7a8q").unwrap();
        assert_eq!(p.san(&q), "a8=Q");
        assert_eq!(p.parse_san("a8=Q").unwrap(), q);
        // Underpromotion and the no-`=` input form.
        let n = p.parse_uci("a7a8n").unwrap();
        assert_eq!(p.san(&n), "a8=N");
        assert_eq!(p.parse_san("a8N").unwrap(), n);
    }

    #[test]
    fn golden_promotion_capture() {
        // White pawn on b7 can capture the knight on a8 or c8 and promote.
        let p = pos("n1n1k3/1P6/8/8/8/8/8/4K3 w - - 0 1");
        let cap = p.parse_uci("b7a8q").unwrap();
        assert_eq!(p.san(&cap), "bxa8=Q");
        assert_eq!(p.parse_san("bxa8=Q").unwrap(), cap);
        // Without the `=` too.
        assert_eq!(p.parse_san("bxa8Q").unwrap(), cap);
    }

    #[test]
    fn golden_castling() {
        let p = pos("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
        let oo = p.parse_uci("e1g1").unwrap();
        let ooo = p.parse_uci("e1c1").unwrap();
        assert_eq!(p.san(&oo), "O-O");
        assert_eq!(p.san(&ooo), "O-O-O");
        assert_eq!(p.parse_san("O-O").unwrap(), oo);
        assert_eq!(p.parse_san("O-O-O").unwrap(), ooo);
        // Lenient zero forms.
        assert_eq!(p.parse_san("0-0").unwrap(), oo);
        assert_eq!(p.parse_san("0-0-0").unwrap(), ooo);
    }

    #[test]
    fn golden_check_suffix() {
        // A bishop check: white bishop delivers check on the black king.
        let p = pos("4k3/8/8/8/8/8/8/4KB2 w - - 0 1");
        let check = p.parse_uci("f1b5").unwrap();
        assert_eq!(p.san(&check), "Bb5+");
        assert_eq!(p.parse_san("Bb5+").unwrap(), check);
        // Parsing without the `+` still resolves.
        assert_eq!(p.parse_san("Bb5").unwrap(), check);
    }

    #[test]
    fn golden_back_rank_mate() {
        // White rook delivers back-rank mate; king boxed in by its own pawns.
        let p = pos("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1");
        let mate = p.parse_uci("a1a8").unwrap();
        let after = p.play(&mate);
        assert!(after.is_checkmate());
        assert_eq!(p.san(&mate), "Ra8#");
        assert_eq!(p.parse_san("Ra8#").unwrap(), mate);
    }

    #[test]
    fn rejects_illegal_and_ambiguous() {
        let p = Position::startpos();
        // No piece can reach e5 as a queen.
        assert_eq!(p.parse_san("Qe5").unwrap_err(), SanError::Illegal);
        // Both knights reach d2? In startpos only b1 and g1 -> only b1 reaches
        // d2 (a3/c3). c3 reachable by b1 only; d2 by b1 only. Use a clearer
        // ambiguous case in a custom position below.
        assert_eq!(p.parse_san("e9").unwrap_err(), SanError::Malformed);
        assert_eq!(p.parse_san("").unwrap_err(), SanError::Empty);
        assert_eq!(p.parse_san("Zd4").unwrap_err(), SanError::Malformed);

        // Ambiguous knight move.
        let amb = pos("4k3/8/8/8/8/5N2/8/1N2K3 w - - 0 1");
        assert_eq!(amb.parse_san("Nd2").unwrap_err(), SanError::Ambiguous);
        // Wrong disambiguation hint -> illegal, not a panic.
        assert_eq!(amb.parse_san("Nhd2").unwrap_err(), SanError::Illegal);
    }

    #[test]
    fn parse_tolerates_annotations() {
        let p = Position::startpos();
        let e4 = p.parse_uci("e2e4").unwrap();
        assert_eq!(p.parse_san("e4!").unwrap(), e4);
        assert_eq!(p.parse_san("e4?!").unwrap(), e4);
        assert_eq!(p.parse_san("  e4  ").unwrap(), e4);
    }

    #[test]
    fn deep_round_trip_perft_like() {
        // Walk a few plies from the start, round-tripping every move at each node.
        fn walk(p: &Position, depth: u32) {
            assert_round_trips(p);
            if depth == 0 {
                return;
            }
            for mv in p.legal_moves() {
                walk(&p.play(&mv), depth - 1);
            }
        }
        walk(&Position::startpos(), 2);
    }
}
