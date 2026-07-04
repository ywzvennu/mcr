//! Human move notation (SAN) and whole-game notation (PGN) for the generic
//! fairy-variant layer, alongside a position-aware UCI parser.
//!
//! The geometry layer already renders a [`WideMove`] to UCI long algebraic
//! notation ([`WideMove::to_uci`]) and round-trips its FEN, but UCI is not a
//! human move language and there is no PGN. This module adds, purely
//! additively (no perft / move-generation behaviour changes):
//!
//! - **A UCI parse audit.** [`WideMove::to_uci`] is, by itself, not invertible:
//!   like standard chess UCI it drops the move *kind* (a quiet, a capture, a
//!   double push, an en-passant, and a castling all render `from``to`), so a
//!   string is resolved *against the legal moves of a position*. The geometry
//!   counterpart of [`crate::Position::parse_uci`] is
//!   [`GenericPosition::parse_uci`], which returns the unique legal
//!   [`WideMove`] whose [`to_uci`](WideMove::to_uci) equals the input. The
//!   audit (see the tests) walks the pinned perft corpora and checks that every
//!   legal move round-trips `move -> UCI -> move` losslessly.
//!
//! - **SAN generation and parsing** for fairy pieces, drops, passes, gating,
//!   and promotion, via [`GenericPosition::san`] and
//!   [`GenericPosition::parse_san`]. Role letters follow the same alphabet as
//!   the board FEN: the standard `p n b r q k`, the fairy single letters, and
//!   the prefixed *overflow* (`*` / `**` / `=`) and Shogi *promoted* (`+`)
//!   tokens (see [`WideRole`]). A drop is `P@e4`, a Janggi pass is `--`, a
//!   Seirawan / S-House gate is the FSF-style `/H` suffix, and a promotion is
//!   `=` plus the promoted role's token.
//!
//! - **PGN export / import** of a whole fairy game, [`WidePgn`], carrying a
//!   `[Variant "..."]` header (the [`WideVariantId`] name) and a
//!   `[SetUp "1"]` / `[FEN "..."]` pair for a non-standard start position, with
//!   movetext written in the SAN above. A game exported with
//!   [`WidePgn::to_pgn`] re-imports with [`WidePgn::from_pgn`] to the identical
//!   move list and positions.

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;

use super::{
    AnyWideVariant, GateRole, GateSquare, GenericPosition, Geometry, Square, WideMove,
    WideMoveKind, WideRole, WideVariant, WideVariantId,
};
use crate::Color;

// ===========================================================================
// Role tokens
// ===========================================================================

/// Writes the uppercase SAN/FEN token for `role`: the bare role letter, or the
/// prefixed form for a Shogi promoted role (`+P`) or an overflow role
/// (`*U` / `**E` / `=A`). The case (always upper here) is meaningful only in
/// board FEN; in SAN the token always uses the uppercase base letter.
fn push_role_token(out: &mut String, role: WideRole) {
    if role.is_promoted() {
        out.push('+');
    } else if role.is_overflow5() {
        out.push_str("****");
    } else if role.is_overflow4() {
        out.push_str("***");
    } else if role.is_overflow2() {
        out.push_str("**");
    } else if role.is_overflow() {
        out.push('*');
    } else if role.is_overflow3() {
        out.push('=');
    }
    out.push(role.upper_char());
}

/// Parses a *leading* role token from `body`, returning the role and the number
/// of bytes it consumed. A lowercase first byte (a file letter) means a pawn
/// move, consuming nothing. Returns `None` only if a prefix is present but names
/// no role.
fn parse_leading_role(body: &str) -> Option<(WideRole, usize)> {
    let b = body.as_bytes();
    if b.is_empty() {
        return None;
    }
    if body.starts_with("****") {
        let letter = *b.get(4)? as char;
        return Some((WideRole::overflow5_from_base(letter)?, 5));
    }
    if body.starts_with("***") {
        let letter = *b.get(3)? as char;
        return Some((WideRole::overflow4_from_base(letter)?, 4));
    }
    if body.starts_with("**") {
        let letter = *b.get(2)? as char;
        return Some((WideRole::overflow2_from_base(letter)?, 3));
    }
    match b[0] {
        b'+' => {
            let letter = *b.get(1)? as char;
            let promoted = WideRole::from_char(letter)?.promoted_form();
            if promoted.is_promoted() {
                Some((promoted, 2))
            } else {
                None
            }
        }
        b'*' => {
            let letter = *b.get(1)? as char;
            Some((WideRole::overflow_from_base(letter)?, 2))
        }
        b'=' => {
            let letter = *b.get(1)? as char;
            Some((WideRole::overflow3_from_base(letter)?, 2))
        }
        c if c.is_ascii_uppercase() => Some((WideRole::from_char(c as char)?, 1)),
        _ => Some((WideRole::Pawn, 0)),
    }
}

/// Parses a whole role token that must occupy the entire string `tok` (used for
/// a drop's piece, which is the full token before the `@`).
fn parse_full_role_token(tok: &str) -> Option<WideRole> {
    let (role, consumed) = parse_leading_role(tok)?;
    if consumed == tok.len() && consumed != 0 {
        Some(role)
    } else {
        None
    }
}

// ===========================================================================
// Square coordinates
// ===========================================================================

/// Appends the algebraic coordinate of square `index` over geometry `G`: a file
/// letter (`a`, `b`, …) and a 1-based rank number (so a ten-rank board reaches
/// `a10`). Mirrors [`WideMove::to_uci`]'s square rendering exactly.
fn push_square<G: Geometry>(out: &mut String, index: u8) {
    let file = index % G::WIDTH;
    let rank = index / G::WIDTH;
    out.push((b'a' + file) as char);
    let rank_no = rank as u32 + 1;
    if rank_no >= 10 {
        out.push((b'0' + (rank_no / 10) as u8) as char);
    }
    out.push((b'0' + (rank_no % 10) as u8) as char);
}

/// Splits a trailing algebraic square (file letter + rank digits) off the end of
/// `s`, returning the prefix before it and the parsed square. Variable-width
/// ranks (one or two digits) are handled.
fn split_trailing_square<G: Geometry>(s: &str) -> Option<(&str, Square<G>)> {
    let bytes = s.as_bytes();
    let mut i = bytes.len();
    while i > 0 && bytes[i - 1].is_ascii_digit() {
        i -= 1;
    }
    if i == bytes.len() || i == 0 {
        return None; // no rank digits, or no room for a file letter
    }
    let rank: u32 = s[i..].parse().ok()?;
    if rank == 0 {
        return None;
    }
    let file_ch = bytes[i - 1];
    if !file_ch.is_ascii_lowercase() {
        return None;
    }
    let file = file_ch - b'a';
    let sq = Square::<G>::from_file_rank(file, (rank - 1) as u8)?;
    Some((&s[..i - 1], sq))
}

// ===========================================================================
// SAN errors
// ===========================================================================

/// The error returned when a SAN string cannot be resolved against a position by
/// [`GenericPosition::parse_san`]. Mirrors the concrete [`crate::SanError`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WideSanError {
    /// The string was empty or not recognizable as SAN at all.
    Empty,
    /// The move-text was syntactically malformed.
    Malformed,
    /// The SAN named no legal move in this position.
    Illegal,
    /// The SAN matched more than one legal move; more disambiguation was needed.
    Ambiguous,
}

impl fmt::Display for WideSanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            WideSanError::Empty => "empty SAN string",
            WideSanError::Malformed => "malformed SAN string",
            WideSanError::Illegal => "SAN names no legal move in this position",
            WideSanError::Ambiguous => "SAN matches more than one legal move",
        })
    }
}

#[cfg(feature = "std")]
impl std::error::Error for WideSanError {}

// ===========================================================================
// Parsed SAN description
// ===========================================================================

/// The "shape" of a base (non-suffix) SAN move, used to filter legal moves.
enum SanShape {
    Pass,
    CastleKingside,
    CastleQueenside,
    /// A drop of `role` on `to`.
    Drop {
        role: WideRole,
        to: u8,
    },
    /// A normal move: a role reaching `to`, with optional origin hints and an
    /// optional promotion role.
    Move {
        role: WideRole,
        to: u8,
        from_file: Option<u8>,
        from_rank: Option<u8>,
        promotion: Option<WideRole>,
    },
}

/// A fully parsed SAN move: the base shape plus the gate and duck addenda.
struct ParsedSan {
    shape: SanShape,
    gate_role: Option<WideRole>,
    gate_on_rook: bool,
    duck_to: Option<u8>,
}

/// Parses a `/`-suffix gate token (`H` or `H@r`) into its role and whether it
/// lands on the castling rook's vacated square.
fn parse_gate(tok: &str) -> Option<(WideRole, bool)> {
    let bytes = tok.as_bytes();
    let letter = *bytes.first()? as char;
    let role = WideRole::from_char(letter)?;
    let on_rook = match &tok[1..] {
        "" => false,
        "@r" => true,
        _ => return None,
    };
    Some((role, on_rook))
}

// ===========================================================================
// SAN + UCI on GenericPosition
// ===========================================================================

impl<G: Geometry, V: WideVariant<G>> GenericPosition<G, V> {
    /// Resolves a UCI move string to the unique legal [`WideMove`] whose
    /// [`to_uci`](WideMove::to_uci) equals `uci`, or `None` if it names no legal
    /// move.
    ///
    /// This is the geometry-layer analogue of [`crate::Position::parse_uci`].
    /// Because UCI does not encode the move kind, a string is matched against the
    /// position's own legal-move renderings, so the result is guaranteed legal
    /// and to render back to the same string.
    #[must_use]
    pub fn parse_uci(&self, uci: &str) -> Option<WideMove> {
        self.legal_moves()
            .into_iter()
            .find(|m| m.to_uci::<G>() == uci)
    }

    /// Renders the legal move `mv` as Standard Algebraic Notation for this
    /// position's variant.
    ///
    /// The move must be legal (as returned by [`legal_moves`]). Role letters use
    /// the FEN alphabet (including the `*` / `**` / `=` overflow and `+` Shogi
    /// promoted tokens); a drop is `P@e4`, a pass is `--`, a gate is the `/H`
    /// suffix, a promotion is `=` plus the promoted role's token, and a `+` / `#`
    /// suffix marks check / mate where the variant has a royal.
    ///
    /// [`legal_moves`]: GenericPosition::legal_moves
    #[must_use]
    pub fn san(&self, mv: &WideMove) -> String {
        let mut s = String::with_capacity(8);
        let from = mv.from_index();
        let to = mv.to_index();

        if mv.is_castle() {
            s.push_str(if matches!(mv.kind(), WideMoveKind::CastleKingside) {
                "O-O"
            } else {
                "O-O-O"
            });
        } else if let WideMoveKind::LionMove {
            first_capture,
            second_capture,
        } = mv.kind()
        {
            // A Chu Lion multi-step move. A pure net-zero non-capture is the jitto
            // pass; otherwise the role token leads, an intermediate capture is shown
            // as `x<mid>`, and the second leg (when the piece actually moves) as
            // `-<to>` or `x<to>`. Examples: `Nxg6` (igui on g6), `Nxg6xh6` (double
            // capture), `Nxg6-h5` (capture g6, land empty h5).
            let role = self
                .board()
                .role_at(Square::<G>::new(from))
                .unwrap_or(WideRole::Pawn);
            if from == to && !first_capture {
                return "--".to_string();
            }
            push_role_token(&mut s, role);
            // Always render **both legs**, naming the intermediate square: the mid
            // is what distinguishes a two-step area move from the leaper's direct
            // jump to the same square (and one area path from another), and spelling
            // out the second leg keeps an igui (`***Nxi5-f6`: capture i5, return to
            // f6) distinct from the ordinary step-capture (`***Nxi5`).
            let mid = mv.lion_mid_index().unwrap_or(to);
            s.push(if first_capture { 'x' } else { '-' });
            push_square::<G>(&mut s, mid);
            s.push(if second_capture { 'x' } else { '-' });
            push_square::<G>(&mut s, to);
        } else if from == to && !mv.is_drop() {
            // A Janggi pass: a legal null move. It carries no gate / promotion /
            // check decoration, so it renders as the bare token.
            return "--".to_string();
        } else if let Some(role) = mv.drop_role() {
            push_role_token(&mut s, role);
            s.push('@');
            push_square::<G>(&mut s, to);
        } else {
            let role = self
                .board()
                .role_at(Square::<G>::new(from))
                .unwrap_or(WideRole::Pawn);
            if role == WideRole::Pawn {
                self.write_pawn_san(&mut s, mv);
            } else {
                self.write_piece_san(&mut s, mv, role);
            }
            if let Some(promo) = mv.promotion() {
                s.push('=');
                push_role_token(&mut s, promo);
            }
        }

        // Gate (Seirawan reserve or S-House hand piece): the FSF-style `/PIECE`
        // suffix, with `@r` distinguishing a castling gate onto the rook square.
        if let Some(gate) = mv.gate() {
            s.push('/');
            s.push(gate.role().upper_char());
            if matches!(mv.gate_square(), GateSquare::RookOrigin) {
                s.push_str("@r");
            }
        } else if let Some(role) = mv.hand_gate() {
            s.push('/');
            push_role_token(&mut s, role);
            if matches!(mv.hand_gate_square(), GateSquare::RookOrigin) {
                s.push_str("@r");
            }
        }

        // Duck placement (Duck chess): the FSF-style `,from to` duck sub-move.
        if let Some(duck_to) = mv.duck_to_index() {
            s.push(',');
            push_square::<G>(&mut s, to);
            push_square::<G>(&mut s, duck_to);
        }

        // Check / mate suffix, computed by playing the move. Variants whose king
        // is not royal never report check, so no suffix is appended.
        let after = self.play(mv);
        if after.is_check() {
            s.push(if after.legal_move_count() == 0 {
                '#'
            } else {
                '+'
            });
        }
        s
    }

    /// Writes the body of a pawn (`WideRole::Pawn`) move. A capture leads with the
    /// origin file (and an `x`); a non-capture is the bare destination — but, for
    /// pawn-role pieces that can reach a square from several origins without
    /// capturing (e.g. a Cannon Shogi soldier stepping sideways), the minimal
    /// file/rank disambiguation is added.
    fn write_pawn_san(&self, s: &mut String, mv: &WideMove) {
        let from = mv.from_index();
        let (same_file, same_rank, any_other) = self.disambig_flags(mv, WideRole::Pawn);
        if mv.is_capture() {
            // SAN convention: a pawn capture always names the origin file.
            s.push((b'a' + from % G::WIDTH) as char);
            // A second pawn capturing the same square from the same file (only
            // possible for sideways-capturing fairy soldiers) needs the rank too.
            if any_other && same_file {
                s.push_str(&(from / G::WIDTH + 1).to_string());
            }
            s.push('x');
        } else if any_other {
            self.push_disambiguation(s, from, same_file, same_rank);
        }
        push_square::<G>(s, mv.to_index());
    }

    /// Writes the body of a non-pawn, non-castling, non-drop move: the role token,
    /// minimal disambiguation, an `x` for captures, then the destination.
    fn write_piece_san(&self, s: &mut String, mv: &WideMove, role: WideRole) {
        push_role_token(s, role);
        let (same_file, same_rank, any_other) = self.disambig_flags(mv, role);
        if any_other {
            self.push_disambiguation(s, mv.from_index(), same_file, same_rank);
        }
        if mv.is_capture() {
            s.push('x');
        }
        push_square::<G>(s, mv.to_index());
    }

    /// Computes whether any *other* legal move of `role` reaches `mv`'s
    /// destination, and whether such a rival shares `mv`'s origin file / rank.
    fn disambig_flags(&self, mv: &WideMove, role: WideRole) -> (bool, bool, bool) {
        let from = mv.from_index();
        let to = mv.to_index();
        let from_file = from % G::WIDTH;
        let from_rank = from / G::WIDTH;

        let mut same_file = false;
        let mut same_rank = false;
        let mut any_other = false;
        for other in self.legal_moves() {
            if other.is_drop() || other.to_index() != to || other.from_index() == from {
                continue;
            }
            if other.from_index() == other.to_index() {
                continue; // a pass
            }
            if self.board().role_at(Square::<G>::new(other.from_index())) != Some(role) {
                continue;
            }
            any_other = true;
            if other.from_index() % G::WIDTH == from_file {
                same_file = true;
            }
            if other.from_index() / G::WIDTH == from_rank {
                same_rank = true;
            }
        }
        (same_file, same_rank, any_other)
    }

    /// Appends the minimal file / rank / both disambiguation for origin `from`,
    /// given the shared-file / shared-rank flags from [`disambig_flags`].
    ///
    /// [`disambig_flags`]: GenericPosition::disambig_flags
    fn push_disambiguation(&self, s: &mut String, from: u8, same_file: bool, same_rank: bool) {
        let from_file = from % G::WIDTH;
        let from_rank = from / G::WIDTH;
        if !same_file {
            s.push((b'a' + from_file) as char);
        } else if !same_rank {
            s.push_str(&(from_rank as u32 + 1).to_string());
        } else {
            s.push((b'a' + from_file) as char);
            s.push_str(&(from_rank as u32 + 1).to_string());
        }
    }

    /// Resolves a SAN string to the unique legal [`WideMove`] in this position.
    ///
    /// Parsing emits canonical SAN but tolerates trailing `+`/`#`/`!`/`?` glyphs
    /// and `0-0`/`0-0-0` for castling.
    ///
    /// # Errors
    ///
    /// Returns [`WideSanError`] if the string is empty, malformed, names no legal
    /// move, or is ambiguous between several legal moves.
    pub fn parse_san(&self, s: &str) -> Result<WideMove, WideSanError> {
        // The structured parse handles every move shape *except* a Chu Lion
        // multi-step move, whose two-square / igui / pass notation has no
        // `ParsedSan` form. Try the structured path first (unchanged for every other
        // variant); fall back to a direct canonical-SAN match, which resolves Lion
        // moves (and is only reached when the structured path names nothing).
        match parse_san_str::<G>(s) {
            Ok(parsed) => {
                let mut found: Option<WideMove> = None;
                for mv in self.legal_moves() {
                    if !self.san_matches(&mv, &parsed) {
                        continue;
                    }
                    if found.is_some() {
                        return Err(WideSanError::Ambiguous);
                    }
                    found = Some(mv);
                }
                if let Some(mv) = found {
                    return Ok(mv);
                }
            }
            Err(WideSanError::Ambiguous) => return Err(WideSanError::Ambiguous),
            Err(_) => {}
        }
        self.direct_san_match(s)
    }

    /// Resolves `s` by comparing it against the canonical SAN of each legal move —
    /// the fallback that resolves Chu Lion multi-step moves (which have no
    /// structured `ParsedSan` shape). Trailing check / annotation glyphs are
    /// ignored on both sides.
    fn direct_san_match(&self, s: &str) -> Result<WideMove, WideSanError> {
        let want = s.trim_end_matches(['+', '#', '!', '?']);
        let mut found: Option<WideMove> = None;
        for mv in self.legal_moves() {
            let cand = self.san(&mv);
            if cand.trim_end_matches(['+', '#', '!', '?']) == want {
                if found.is_some() {
                    return Err(WideSanError::Ambiguous);
                }
                found = Some(mv);
            }
        }
        found.ok_or(WideSanError::Illegal)
    }

    /// Whether the legal move `mv` is described by the parsed SAN `p`.
    fn san_matches(&self, mv: &WideMove, p: &ParsedSan) -> bool {
        // Gate addendum.
        if let Some(gate_role) = p.gate_role {
            let placed = mv
                .gate()
                .map(GateRole::role)
                .or_else(|| mv.hand_gate())
                .filter(|&r| r == gate_role);
            if placed.is_none() {
                return false;
            }
            let on_rook = matches!(mv.gate_square(), GateSquare::RookOrigin)
                || matches!(mv.hand_gate_square(), GateSquare::RookOrigin);
            if on_rook != p.gate_on_rook {
                return false;
            }
        } else if mv.gate().is_some() || mv.hand_gate().is_some() {
            return false;
        }
        // Duck addendum.
        if mv.duck_to_index() != p.duck_to {
            return false;
        }

        match p.shape {
            // A bare `--` pass is a non-capturing null move (a Janggi pass or the
            // Chu Lion's jitto pass); an igui also has `from == to` but captures, so
            // it is excluded here and resolved by its own SAN.
            SanShape::Pass => mv.from_index() == mv.to_index() && !mv.is_drop() && !mv.is_capture(),
            SanShape::CastleKingside => matches!(mv.kind(), WideMoveKind::CastleKingside),
            SanShape::CastleQueenside => matches!(mv.kind(), WideMoveKind::CastleQueenside),
            SanShape::Drop { role, to } => mv.drop_role() == Some(role) && mv.to_index() == to,
            SanShape::Move {
                role,
                to,
                from_file,
                from_rank,
                promotion,
            } => {
                if mv.is_castle() || mv.is_drop() {
                    return false;
                }
                // A Chu Lion multi-step move has its own SAN (`direct_san_match`);
                // it must not be matched here, or a plain capture and a same-square
                // Lion area capture would collide.
                if matches!(mv.kind(), WideMoveKind::LionMove { .. }) {
                    return false;
                }
                if mv.from_index() == mv.to_index() {
                    return false; // a pass
                }
                if mv.to_index() != to || mv.promotion() != promotion {
                    return false;
                }
                if self.board().role_at(Square::<G>::new(mv.from_index())) != Some(role) {
                    return false;
                }
                if let Some(file) = from_file {
                    if mv.from_index() % G::WIDTH != file {
                        return false;
                    }
                }
                if let Some(rank) = from_rank {
                    if mv.from_index() / G::WIDTH != rank {
                        return false;
                    }
                }
                true
            }
        }
    }
}

/// Parses a SAN string into a [`ParsedSan`] without a position (the position is
/// only consulted to resolve it to a concrete legal move).
fn parse_san_str<G: Geometry>(s: &str) -> Result<ParsedSan, WideSanError> {
    let core = s.trim().trim_end_matches(['+', '#', '!', '?']);
    if core.is_empty() {
        return Err(WideSanError::Empty);
    }
    if !core.is_ascii() {
        return Err(WideSanError::Malformed);
    }

    if core == "--" {
        return Ok(ParsedSan {
            shape: SanShape::Pass,
            gate_role: None,
            gate_on_rook: false,
            duck_to: None,
        });
    }

    // Peel the duck sub-move (`,from to`) off the end, keeping only the duck's
    // destination (the second square; the first is cosmetic).
    let (core, duck_to) = match core.find(',') {
        Some(i) => {
            let (_, duck_sq) =
                split_trailing_square::<G>(&core[i + 1..]).ok_or(WideSanError::Malformed)?;
            (&core[..i], Some(duck_sq.index()))
        }
        None => (core, None),
    };

    // Peel the gate suffix (`/PIECE` or `/PIECE@r`).
    let (core, gate) = match core.rfind('/') {
        Some(i) => {
            let g = parse_gate(&core[i + 1..]).ok_or(WideSanError::Malformed)?;
            (&core[..i], Some(g))
        }
        None => (core, None),
    };
    let (gate_role, gate_on_rook) = match gate {
        Some((role, on_rook)) => (Some(role), on_rook),
        None => (None, false),
    };

    let shape = parse_san_shape::<G>(core)?;
    Ok(ParsedSan {
        shape,
        gate_role,
        gate_on_rook,
        duck_to,
    })
}

/// Parses the base (gate/duck-stripped) part of a SAN move into a [`SanShape`].
fn parse_san_shape<G: Geometry>(core: &str) -> Result<SanShape, WideSanError> {
    if core == "O-O" || core == "0-0" {
        return Ok(SanShape::CastleKingside);
    }
    if core == "O-O-O" || core == "0-0-0" {
        return Ok(SanShape::CastleQueenside);
    }

    // Drop: `TOKEN@square`.
    if let Some(at) = core.find('@') {
        let role = parse_full_role_token(&core[..at]).ok_or(WideSanError::Malformed)?;
        let (prefix, to) =
            split_trailing_square::<G>(&core[at + 1..]).ok_or(WideSanError::Malformed)?;
        if !prefix.is_empty() {
            return Err(WideSanError::Malformed);
        }
        return Ok(SanShape::Drop {
            role,
            to: to.index(),
        });
    }

    // Leading role token (or pawn).
    let (role, consumed) = parse_leading_role(core).ok_or(WideSanError::Malformed)?;
    let body = &core[consumed..];

    // Trailing promotion `=token`.
    let (body, promotion) = match body.find('=') {
        Some(i) => {
            let promo = parse_full_role_token(&body[i + 1..]).ok_or(WideSanError::Malformed)?;
            (&body[..i], Some(promo))
        }
        None => (body, None),
    };

    let (prefix, to) = split_trailing_square::<G>(body).ok_or(WideSanError::Malformed)?;

    // The prefix holds optional origin file / rank disambiguation and a capture
    // marker (`x` / `:`).
    let prefix = prefix
        .strip_suffix('x')
        .or_else(|| prefix.strip_suffix(':'))
        .unwrap_or(prefix);
    let (from_file, from_rank) = parse_origin_hint::<G>(prefix)?;

    Ok(SanShape::Move {
        role,
        to: to.index(),
        from_file,
        from_rank,
        promotion,
    })
}

/// Parses an origin-hint prefix (an optional file letter then optional rank
/// digits) into file and rank indices.
fn parse_origin_hint<G: Geometry>(prefix: &str) -> Result<(Option<u8>, Option<u8>), WideSanError> {
    if prefix.is_empty() {
        return Ok((None, None));
    }
    let bytes = prefix.as_bytes();
    let mut j = 0;
    let mut from_file = None;
    if bytes[j].is_ascii_lowercase() {
        from_file = Some(bytes[j] - b'a');
        j += 1;
    }
    let mut from_rank = None;
    if j < prefix.len() {
        let rank: u32 = prefix[j..].parse().map_err(|_| WideSanError::Malformed)?;
        if rank == 0 {
            return Err(WideSanError::Malformed);
        }
        from_rank = Some((rank - 1) as u8);
    }
    let _ = core::marker::PhantomData::<G>;
    Ok((from_file, from_rank))
}

// `AnyWideVariant::san` / `parse_san` are generated by the `wide_variants!`
// macro in `any.rs` (each arm forwards to the inherent `GenericPosition`
// methods above), mirroring the existing `to_uci` / `parse_uci` forwarding.

// ===========================================================================
// PGN
// ===========================================================================

/// A game result token in PGN (`1-0`, `0-1`, `1/2-1/2`, `*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WidePgnResult {
    /// White won (`1-0`).
    WhiteWins,
    /// Black won (`0-1`).
    BlackWins,
    /// The game was drawn (`1/2-1/2`).
    Draw,
    /// The result is unknown or the game is in progress (`*`).
    Unknown,
}

impl WidePgnResult {
    /// The PGN token for this result.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            WidePgnResult::WhiteWins => "1-0",
            WidePgnResult::BlackWins => "0-1",
            WidePgnResult::Draw => "1/2-1/2",
            WidePgnResult::Unknown => "*",
        }
    }

    /// Parses a result token, returning `None` for anything else.
    #[must_use]
    pub fn from_token(s: &str) -> Option<WidePgnResult> {
        match s {
            "1-0" => Some(WidePgnResult::WhiteWins),
            "0-1" => Some(WidePgnResult::BlackWins),
            "1/2-1/2" => Some(WidePgnResult::Draw),
            "*" => Some(WidePgnResult::Unknown),
            _ => None,
        }
    }
}

impl fmt::Display for WidePgnResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.token())
    }
}

/// The error returned when a fairy PGN string cannot be parsed by
/// [`WidePgn::from_pgn`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WidePgnError {
    /// A `[Name "Value"]` tag line was malformed.
    MalformedTag,
    /// No `[Variant "..."]` tag, or it named an unknown fairy variant.
    UnknownVariant(String),
    /// A `[FEN "..."]` start position could not be parsed.
    InvalidFen,
    /// A movetext token could not be resolved to a legal move in turn.
    IllegalMove(String),
}

impl fmt::Display for WidePgnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WidePgnError::MalformedTag => f.write_str("malformed PGN tag"),
            WidePgnError::UnknownVariant(s) => write!(f, "unknown variant tag: {s:?}"),
            WidePgnError::InvalidFen => f.write_str("invalid FEN in PGN setup"),
            WidePgnError::IllegalMove(s) => write!(f, "illegal move in movetext: {s:?}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for WidePgnError {}

/// A whole fairy game in PGN form: the variant, an optional non-standard start
/// position, the validated mainline of moves, and the result.
///
/// Build one from a played game with [`WidePgn::from_game`], serialize it with
/// [`to_pgn`](WidePgn::to_pgn), and parse one back with
/// [`from_pgn`](WidePgn::from_pgn). A round trip preserves the variant, start
/// position, and every move.
#[derive(Debug, Clone)]
pub struct WidePgn {
    variant: WideVariantId,
    /// The start FEN when it differs from the variant's own start position;
    /// `None` for the standard start.
    start_fen: Option<String>,
    tags: Vec<(String, String)>,
    moves: Vec<WideMove>,
    sans: Vec<String>,
    result: WidePgnResult,
}

impl WidePgn {
    /// Builds a [`WidePgn`] from a start position and a list of moves, validating
    /// each move in turn and recording its canonical SAN.
    ///
    /// `extra_tags` are written verbatim after the mandatory `Variant` / setup
    /// tags. The result is taken from a `Result` entry in `extra_tags` if present,
    /// otherwise derived from the final position's outcome.
    ///
    /// # Errors
    ///
    /// Returns [`WidePgnError::IllegalMove`] (carrying the offending move's UCI)
    /// if any move is not legal in turn.
    pub fn from_game(
        start: &AnyWideVariant,
        moves: &[WideMove],
        extra_tags: Vec<(String, String)>,
    ) -> Result<WidePgn, WidePgnError> {
        let variant = start.variant_id();
        let standard_fen = AnyWideVariant::startpos(variant).to_fen();
        let start_fen = if start.to_fen() != standard_fen {
            Some(start.to_fen())
        } else {
            None
        };

        let mut pos = start.clone();
        let mut recorded = Vec::with_capacity(moves.len());
        let mut sans = Vec::with_capacity(moves.len());
        for mv in moves {
            if !pos.legal_moves().iter().any(|legal| legal == mv) {
                return Err(WidePgnError::IllegalMove(pos.to_uci(mv)));
            }
            sans.push(pos.san(mv));
            recorded.push(*mv);
            pos = pos.play(mv);
        }

        let result = extra_tags
            .iter()
            .find(|(k, _)| k == "Result")
            .and_then(|(_, v)| WidePgnResult::from_token(v))
            .unwrap_or_else(|| match pos.outcome() {
                Some(super::WideOutcome::Decisive { winner }) => {
                    if winner.is_white() {
                        WidePgnResult::WhiteWins
                    } else {
                        WidePgnResult::BlackWins
                    }
                }
                Some(super::WideOutcome::Draw) => WidePgnResult::Draw,
                None => WidePgnResult::Unknown,
            });

        Ok(WidePgn {
            variant,
            start_fen,
            tags: extra_tags,
            moves: recorded,
            sans,
            result,
        })
    }

    /// The variant this game is played under.
    #[must_use]
    pub fn variant(&self) -> WideVariantId {
        self.variant
    }

    /// The extra header tag pairs (everything other than the structural
    /// `Variant` / `SetUp` / `FEN` tags), in stored order.
    #[must_use]
    pub fn tags(&self) -> &[(String, String)] {
        &self.tags
    }

    /// The validated mainline moves, in play order.
    #[must_use]
    pub fn moves(&self) -> &[WideMove] {
        &self.moves
    }

    /// The recorded SAN strings, parallel to [`moves`](WidePgn::moves).
    #[must_use]
    pub fn sans(&self) -> &[String] {
        &self.sans
    }

    /// The game result.
    #[must_use]
    pub fn result(&self) -> WidePgnResult {
        self.result
    }

    /// The start position (the `[FEN ...]` setup, or the variant start).
    #[must_use]
    pub fn start_position(&self) -> AnyWideVariant {
        match &self.start_fen {
            Some(fen) => AnyWideVariant::from_fen(self.variant, fen)
                .unwrap_or_else(|_| AnyWideVariant::startpos(self.variant)),
            None => AnyWideVariant::startpos(self.variant),
        }
    }

    /// The final position reached after playing the whole mainline.
    #[must_use]
    pub fn final_position(&self) -> AnyWideVariant {
        let mut pos = self.start_position();
        for mv in &self.moves {
            pos = pos.play(mv);
        }
        pos
    }

    /// Serializes this game to PGN text: the `Variant` tag, an optional
    /// `SetUp`/`FEN` pair, the `Result` tag, any extra tags, then the SAN
    /// movetext and the result token.
    #[must_use]
    pub fn to_pgn(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("[Variant \"{}\"]\n", self.variant.as_str()));
        if let Some(fen) = &self.start_fen {
            out.push_str("[SetUp \"1\"]\n");
            out.push_str(&format!("[FEN \"{fen}\"]\n"));
        }
        out.push_str(&format!("[Result \"{}\"]\n", self.result.token()));
        for (k, v) in &self.tags {
            if k == "Result" {
                continue;
            }
            out.push_str(&format!("[{k} \"{v}\"]\n"));
        }
        out.push('\n');

        // Movetext: a fullmove number before each White move (and a `N...`
        // ellipsis when the game starts on a Black move).
        let mut pos = self.start_position();
        let mut fullmove = pos.fullmove_number().max(1);
        let mut white_to_move = pos.turn() == Color::White;
        let mut line = String::new();
        let mut first = true;
        for (mv, san) in self.moves.iter().zip(&self.sans) {
            if white_to_move {
                line.push_str(&format!("{fullmove}. "));
            } else if first {
                line.push_str(&format!("{fullmove}... "));
            }
            line.push_str(san);
            line.push(' ');
            first = false;
            if !white_to_move {
                fullmove += 1;
            }
            white_to_move = !white_to_move;
            pos = pos.play(mv);
        }
        line.push_str(self.result.token());
        out.push_str(line.trim_end());
        out.push('\n');
        out
    }

    /// Parses a fairy game from PGN text.
    ///
    /// The `[Variant "..."]` tag is mandatory (it selects the geometry); a
    /// `[FEN "..."]` tag supplies a non-standard start. Every movetext SAN token
    /// is resolved against the running position.
    ///
    /// # Errors
    ///
    /// Returns [`WidePgnError`] for a malformed tag, an unknown variant, an
    /// invalid setup FEN, or a movetext token that names no legal move.
    pub fn from_pgn(text: &str) -> Result<WidePgn, WidePgnError> {
        let mut tags: Vec<(String, String)> = Vec::new();
        let mut body = String::new();
        for raw in text.lines() {
            let line = raw.trim();
            if line.starts_with('[') {
                let (k, v) = parse_tag_line(line)?;
                tags.push((k, v));
            } else {
                body.push_str(raw);
                body.push('\n');
            }
        }

        let variant_name = tags
            .iter()
            .find(|(k, _)| k == "Variant")
            .map(|(_, v)| v.clone())
            .ok_or_else(|| WidePgnError::UnknownVariant(String::new()))?;
        let variant: WideVariantId = variant_name
            .parse()
            .map_err(|_| WidePgnError::UnknownVariant(variant_name.clone()))?;

        let start_fen = tags
            .iter()
            .find(|(k, _)| k == "FEN")
            .map(|(_, v)| v.clone());
        let mut pos = match &start_fen {
            Some(fen) => {
                AnyWideVariant::from_fen(variant, fen).map_err(|_| WidePgnError::InvalidFen)?
            }
            None => AnyWideVariant::startpos(variant),
        };

        let mut moves = Vec::new();
        let mut sans = Vec::new();
        let mut result = WidePgnResult::Unknown;
        for token in body.split_whitespace() {
            if let Some(r) = WidePgnResult::from_token(token) {
                result = r;
                continue;
            }
            if is_move_number(token) {
                continue;
            }
            // Strip a leading `N.` / `N...` move-number that is glued to the SAN.
            let san = strip_move_number_prefix(token);
            if san.is_empty() {
                continue;
            }
            let mv = pos
                .parse_san(san)
                .ok_or_else(|| WidePgnError::IllegalMove(san.to_string()))?;
            sans.push(pos.san(&mv));
            moves.push(mv);
            pos = pos.play(&mv);
        }

        // The retained extra tags exclude the structural ones.
        let extra: Vec<(String, String)> = tags
            .into_iter()
            .filter(|(k, _)| k != "Variant" && k != "FEN" && k != "SetUp")
            .collect();

        // Normalize the stored start FEN exactly as `from_game` would, so a
        // re-export is byte-stable.
        let normalized_start = if let Some(fen) = &start_fen {
            let standard = AnyWideVariant::startpos(variant).to_fen();
            if AnyWideVariant::from_fen(variant, fen)
                .map(|p| p.to_fen())
                .ok()
                .as_deref()
                == Some(standard.as_str())
            {
                None
            } else {
                Some(
                    AnyWideVariant::from_fen(variant, fen)
                        .map(|p| p.to_fen())
                        .map_err(|_| WidePgnError::InvalidFen)?,
                )
            }
        } else {
            None
        };

        Ok(WidePgn {
            variant,
            start_fen: normalized_start,
            tags: extra,
            moves,
            sans,
            result,
        })
    }
}

/// Parses a `[Name "Value"]` tag line.
fn parse_tag_line(line: &str) -> Result<(String, String), WidePgnError> {
    let inner = line
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or(WidePgnError::MalformedTag)?;
    let q = inner.find('"').ok_or(WidePgnError::MalformedTag)?;
    let name = inner[..q].trim().to_string();
    if name.is_empty() {
        return Err(WidePgnError::MalformedTag);
    }
    let rest = &inner[q + 1..];
    let end = rest.find('"').ok_or(WidePgnError::MalformedTag)?;
    Ok((name, rest[..end].to_string()))
}

/// Whether `token` is a bare move number such as `1.` or `12...`.
fn is_move_number(token: &str) -> bool {
    let trimmed = token.trim_end_matches('.');
    !trimmed.is_empty() && trimmed.bytes().all(|b| b.is_ascii_digit())
}

/// Strips a `N.` / `N...` move-number prefix glued onto a SAN token.
fn strip_move_number_prefix(token: &str) -> &str {
    match token.rfind('.') {
        Some(i) if token[..i].bytes().all(|b| b.is_ascii_digit()) && i > 0 => &token[i + 1..],
        _ => token,
    }
}

#[cfg(test)]
mod tests {
    include!("notation_tests.rs");
}
