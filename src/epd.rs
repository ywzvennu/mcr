//! Extended Position Description (EPD).
//!
//! EPD is a close relative of [FEN][crate::Position::from_fen]: it shares FEN's
//! first four fields — piece placement, side to move, castling availability, and
//! the en-passant target — but **drops** the halfmove clock and fullmove number,
//! replacing them with a free-form, extensible list of *operations*. Each
//! operation is an `opcode` followed by zero or more operands and terminated by
//! a semicolon, for example:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - bm e4; id "startpos";
//! ```
//!
//! EPD is the lingua franca of test suites (the *perftsuite* and *WAC*-style
//! collections), opening books, and analysis interchange. The common opcodes
//! are:
//!
//! - `bm` / `am` — *best move(s)* / *avoid move(s)*, given in SAN and resolved
//!   here against the position into concrete [`Move`]s ([`Epd::best_moves`],
//!   [`Epd::avoid_moves`]).
//! - `id` — a quoted identifier string ([`Epd::id`]).
//! - `Dn` / `cn` — perft node counts to depth *n* / generic comment slots; their
//!   operands are kept as raw strings and surfaced through [`Epd::operation`].
//!
//! Any other opcode is preserved verbatim as a list of string operands, so an
//! [`Epd`] round-trips through [`Epd::to_epd`] without losing operations it does
//! not interpret.
//!
//! # Robustness
//!
//! [`Epd::parse`] is total and panic-free: every malformed input — a truncated
//! record, a bad position field, an unterminated quoted string, or non-ASCII
//! bytes — yields an [`EpdError`] rather than a panic.

use core::fmt;

use crate::{FenError, Move, Position, SanError};

/// A parsed EPD record: a [`Position`] plus its list of operations.
///
/// Build one with [`Epd::parse`] and serialize it back with [`Epd::to_epd`].
/// The position is a full [`Position`] — EPD omits the two move clocks, so they
/// default to a halfmove clock of `0` and fullmove number `1`. Operations are
/// stored in input order; query them with [`Epd::operation`], or use the typed
/// helpers [`Epd::id`], [`Epd::best_moves`], and [`Epd::avoid_moves`].
///
/// ```
/// use mce::Epd;
/// let epd = Epd::parse(
///     "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - bm e4; id \"start\";",
/// )
/// .unwrap();
/// assert_eq!(epd.id(), Some("start"));
/// assert_eq!(epd.best_moves().unwrap().unwrap().len(), 1);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Epd {
    position: Position,
    /// Operations in input order: `(opcode, operands)`. Each operand is stored
    /// as the raw token text, with any surrounding double quotes already
    /// stripped.
    operations: Vec<(String, Vec<String>)>,
}

impl Epd {
    /// Parses an EPD record: the four position fields followed by zero or more
    /// `opcode operand... ;` operations.
    ///
    /// # Errors
    ///
    /// Returns [`EpdError`] if the input is not valid ASCII, the four position
    /// fields are missing or describe an impossible position, or an operation is
    /// malformed (for example an unterminated quoted string).
    pub fn parse(input: &str) -> Result<Epd, EpdError> {
        if !input.is_ascii() {
            return Err(EpdError::NonAscii);
        }

        // Split off the four leading position fields. EPD requires exactly four
        // space-separated fields before the operations begin.
        let mut fields = input.split_whitespace();
        let board = fields.next().ok_or(EpdError::MissingPosition)?;
        let turn = fields.next().ok_or(EpdError::MissingPosition)?;
        let castling = fields.next().ok_or(EpdError::MissingPosition)?;
        let ep = fields.next().ok_or(EpdError::MissingPosition)?;

        // Reuse the six-field FEN parser by supplying default clocks; EPD's
        // first four fields are exactly FEN's first four.
        let fen = format!("{board} {turn} {castling} {ep} 0 1");
        let position = Position::from_fen(&fen).map_err(EpdError::Position)?;

        // The operations are whatever follows the en-passant field. Recover that
        // tail from the original input by locating the end of the fourth field,
        // so we tokenize over the real characters (quotes included) rather than
        // the whitespace-collapsed iterator.
        let ops_str = remainder_after_fields(input, 4);
        let operations = parse_operations(ops_str)?;

        Ok(Epd {
            position,
            operations,
        })
    }

    /// The position described by this record.
    #[must_use]
    #[inline]
    pub fn position(&self) -> &Position {
        &self.position
    }

    /// Consumes the record and returns its [`Position`].
    #[must_use]
    #[inline]
    pub fn into_position(self) -> Position {
        self.position
    }

    /// The operands of the first operation with the given `opcode`, or `None` if
    /// the record has no such operation.
    ///
    /// Operands are the raw token strings (quotes already stripped), in input
    /// order.
    #[must_use]
    pub fn operation(&self, opcode: &str) -> Option<&[String]> {
        self.operations
            .iter()
            .find(|(op, _)| op == opcode)
            .map(|(_, operands)| operands.as_slice())
    }

    /// Every operation in the record, in input order, as `(opcode, operands)`.
    #[must_use]
    #[inline]
    pub fn operations(&self) -> &[(String, Vec<String>)] {
        &self.operations
    }

    /// The `id` operation's string value, if present.
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        self.operation("id")
            .and_then(|operands| operands.first())
            .map(String::as_str)
    }

    /// The best moves (`bm` operation), resolved against the position from their
    /// SAN operands.
    ///
    /// Returns `None` if the record has no `bm` operation. Returns
    /// `Some(Err(..))` if a `bm` operand is not a legal SAN move in this
    /// position.
    #[must_use = "the result reports whether every best move resolved"]
    pub fn best_moves(&self) -> Option<Result<Vec<Move>, SanError>> {
        self.resolve_moves("bm")
    }

    /// The avoid moves (`am` operation), resolved against the position from
    /// their SAN operands. See [`Epd::best_moves`].
    #[must_use = "the result reports whether every avoid move resolved"]
    pub fn avoid_moves(&self) -> Option<Result<Vec<Move>, SanError>> {
        self.resolve_moves("am")
    }

    /// Resolves every operand of `opcode` as a SAN move against the position.
    fn resolve_moves(&self, opcode: &str) -> Option<Result<Vec<Move>, SanError>> {
        let operands = self.operation(opcode)?;
        Some(
            operands
                .iter()
                .map(|san| self.position.parse_san(san))
                .collect(),
        )
    }

    /// Serializes this record back into an EPD string: the four position fields
    /// followed by each operation, terminated by `;`.
    ///
    /// The position fields are exactly FEN's first four; the move clocks are
    /// dropped. Operations are written in stored order. An `id` operand (and any
    /// operand that needs it) is re-quoted so the output re-parses.
    #[must_use]
    pub fn to_epd(&self) -> String {
        // Take the four position fields off the front of the FEN rendering.
        let fen = self.position.to_fen();
        let mut out = String::with_capacity(fen.len() + 16);
        for field in fen.split_whitespace().take(4) {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(field);
        }

        for (opcode, operands) in &self.operations {
            out.push(' ');
            out.push_str(opcode);
            for operand in operands {
                out.push(' ');
                write_operand(&mut out, opcode, operand);
            }
            out.push(';');
        }

        out
    }
}

/// Returns the slice of `input` that follows its first `n` whitespace-separated
/// fields, with the leading whitespace trimmed. If `input` has fewer than `n`
/// fields, returns an empty string.
fn remainder_after_fields(input: &str, n: usize) -> &str {
    let bytes = input.as_bytes();
    let mut idx = 0;
    let mut fields_seen = 0;

    while idx < bytes.len() && fields_seen < n {
        // Skip leading whitespace.
        while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
            idx += 1;
        }
        if idx >= bytes.len() {
            break;
        }
        // Consume one field.
        while idx < bytes.len() && !bytes[idx].is_ascii_whitespace() {
            idx += 1;
        }
        fields_seen += 1;
    }

    // Trim the whitespace between the last field and the operations.
    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }

    &input[idx..]
}

/// Parses the operation list — everything after the four position fields — into
/// `(opcode, operands)` pairs.
///
/// The grammar is a sequence of operations, each an opcode token, then zero or
/// more operands (bare tokens or double-quoted strings), terminated by `;`.
/// Tokens are separated by ASCII whitespace. A final operation without a
/// trailing `;` is accepted leniently (real-world perftsuite lines sometimes
/// omit it).
fn parse_operations(s: &str) -> Result<Vec<(String, Vec<String>)>, EpdError> {
    let bytes = s.as_bytes();
    let mut idx = 0;
    let mut ops = Vec::new();

    loop {
        // Skip whitespace and any stray semicolons between operations.
        while idx < bytes.len() && (bytes[idx].is_ascii_whitespace() || bytes[idx] == b';') {
            idx += 1;
        }
        if idx >= bytes.len() {
            break;
        }

        // The opcode is the first token of the operation.
        let opcode = match next_token(s, &mut idx)? {
            Some(tok) => tok,
            None => break,
        };

        // Operands run until the terminating semicolon (or end of input).
        let mut operands = Vec::new();
        loop {
            while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
                idx += 1;
            }
            if idx >= bytes.len() || bytes[idx] == b';' {
                break;
            }
            match next_token(s, &mut idx)? {
                Some(tok) => operands.push(tok),
                None => break,
            }
        }

        ops.push((opcode, operands));
    }

    Ok(ops)
}

/// Reads the next token starting at `*idx`, advancing `*idx` past it.
///
/// A token is either a double-quoted string (quotes stripped, the closing quote
/// required) or a run of non-whitespace, non-semicolon characters. Leading
/// whitespace is assumed already skipped by the caller. Returns `Ok(None)` only
/// at end of input.
fn next_token(s: &str, idx: &mut usize) -> Result<Option<String>, EpdError> {
    let bytes = s.as_bytes();
    if *idx >= bytes.len() {
        return Ok(None);
    }

    if bytes[*idx] == b'"' {
        // Quoted string: scan to the next unescaped quote.
        *idx += 1;
        let start = *idx;
        while *idx < bytes.len() && bytes[*idx] != b'"' {
            *idx += 1;
        }
        if *idx >= bytes.len() {
            return Err(EpdError::UnterminatedString);
        }
        let token = s[start..*idx].to_owned();
        *idx += 1; // consume the closing quote
        return Ok(Some(token));
    }

    // Bare token: up to the next whitespace or semicolon.
    let start = *idx;
    while *idx < bytes.len() && !bytes[*idx].is_ascii_whitespace() && bytes[*idx] != b';' {
        *idx += 1;
    }
    if *idx == start {
        return Ok(None);
    }
    Ok(Some(s[start..*idx].to_owned()))
}

/// Writes one operand of `opcode` to `out`, quoting it when necessary so the
/// output re-parses to the same operand.
///
/// An operand is quoted when its opcode is one of the conventionally
/// string-valued ones (`id`), or when it contains whitespace, a semicolon, or a
/// quote, or is empty — any of which a bare token cannot represent.
fn write_operand(out: &mut String, opcode: &str, operand: &str) {
    let needs_quote = opcode == "id"
        || operand.is_empty()
        || operand
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b == b';' || b == b'"');
    if needs_quote {
        out.push('"');
        // Drop embedded quotes; EPD quoted strings have no escape mechanism, so
        // an embedded quote cannot round-trip and is elided to keep the output
        // re-parseable.
        for ch in operand.chars().filter(|&c| c != '"') {
            out.push(ch);
        }
        out.push('"');
    } else {
        out.push_str(operand);
    }
}

/// The error returned when an EPD record cannot be parsed by [`Epd::parse`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EpdError {
    /// The input contained non-ASCII bytes (EPD is an ASCII format).
    NonAscii,
    /// Fewer than the four required position fields were present.
    MissingPosition,
    /// The four position fields did not describe a valid position.
    Position(FenError),
    /// A quoted operand string was opened but never closed.
    UnterminatedString,
}

impl fmt::Display for EpdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EpdError::NonAscii => f.write_str("EPD input is not valid ASCII"),
            EpdError::MissingPosition => {
                f.write_str("EPD is missing one of the four position fields")
            }
            EpdError::Position(e) => write!(f, "invalid EPD position: {e}"),
            EpdError::UnterminatedString => {
                f.write_str("unterminated quoted string in EPD operand")
            }
        }
    }
}

impl std::error::Error for EpdError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            EpdError::Position(e) => Some(e),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_perftsuite_line() {
        // A standard perftsuite-style record: position plus Dn perft counts.
        let line = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - ;D1 20 ;D2 400 ;D3 8902 ;D4 197281";
        let epd = Epd::parse(line).unwrap();
        assert_eq!(epd.position(), &Position::startpos());
        assert_eq!(epd.operation("D1"), Some(&["20".to_owned()][..]));
        assert_eq!(epd.operation("D2"), Some(&["400".to_owned()][..]));
        assert_eq!(epd.operation("D4"), Some(&["197281".to_owned()][..]));
        assert_eq!(epd.operation("D9"), None);
    }

    #[test]
    fn parses_bm_and_id() {
        let line =
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - bm e4 Nf3; id \"opening test\";";
        let epd = Epd::parse(line).unwrap();
        assert_eq!(epd.id(), Some("opening test"));

        let bm = epd.best_moves().unwrap().unwrap();
        assert_eq!(bm.len(), 2);
        let pos = epd.position();
        assert_eq!(bm[0], pos.parse_uci("e2e4").unwrap());
        assert_eq!(bm[1], pos.parse_uci("g1f3").unwrap());
    }

    #[test]
    fn resolves_avoid_moves() {
        let line = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - am a4;";
        let epd = Epd::parse(line).unwrap();
        let am = epd.avoid_moves().unwrap().unwrap();
        assert_eq!(am.len(), 1);
        assert_eq!(am[0], epd.position().parse_uci("a2a4").unwrap());
    }

    #[test]
    fn no_operations() {
        let epd = Epd::parse("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -").unwrap();
        assert!(epd.operations().is_empty());
        assert!(epd.id().is_none());
        assert!(epd.best_moves().is_none());
    }

    #[test]
    fn round_trips() {
        let line =
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - bm O-O; id \"position\";";
        let epd = Epd::parse(line).unwrap();
        let written = epd.to_epd();
        let reparsed = Epd::parse(&written).unwrap();
        assert_eq!(epd, reparsed);
        assert_eq!(reparsed.id(), Some("position"));
        assert_eq!(reparsed.best_moves().unwrap().unwrap().len(), 1);
    }

    #[test]
    fn round_trips_perft_counts() {
        let line = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - ;D1 20 ;D2 400";
        let epd = Epd::parse(line).unwrap();
        let reparsed = Epd::parse(&epd.to_epd()).unwrap();
        assert_eq!(epd, reparsed);
    }

    #[test]
    fn position_field_only_no_clocks() {
        // EPD drops the FEN clocks; the position still parses with defaults.
        let epd = Epd::parse("4k3/8/8/8/8/8/8/4K3 w - -").unwrap();
        assert_eq!(epd.position().halfmove_clock(), 0);
        assert_eq!(epd.position().fullmove_number(), 1);
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(Epd::parse("").unwrap_err(), EpdError::MissingPosition);
    }

    #[test]
    fn rejects_truncated_position() {
        // Only three of the four required fields.
        assert_eq!(
            Epd::parse("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq").unwrap_err(),
            EpdError::MissingPosition
        );
    }

    #[test]
    fn rejects_bad_board() {
        assert!(matches!(
            Epd::parse("not_a_board w KQkq - bm e4;").unwrap_err(),
            EpdError::Position(_)
        ));
    }

    #[test]
    fn rejects_non_ascii() {
        assert_eq!(
            Epd::parse("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - id \"café\";")
                .unwrap_err(),
            EpdError::NonAscii
        );
    }

    #[test]
    fn rejects_unterminated_string() {
        assert_eq!(
            Epd::parse("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - id \"oops;")
                .unwrap_err(),
            EpdError::UnterminatedString
        );
    }

    #[test]
    fn illegal_bm_resolves_to_error() {
        // A syntactically valid SAN that names no legal move in the position.
        let epd =
            Epd::parse("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - bm Qh5;").unwrap();
        assert!(epd.best_moves().unwrap().is_err());
    }

    #[test]
    fn parse_never_panics_on_arbitrary_input() {
        // A spread of malformed, truncated, and adversarial inputs: none may
        // panic; each must yield Ok or Err.
        let inputs = [
            "",
            " ",
            ";",
            "\"",
            "x",
            "8/8/8/8/8/8/8/8 w - -",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - bm",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - \"\"\"\";;;",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - ;;;;;;",
        ];
        for input in inputs {
            let _ = Epd::parse(input);
        }
    }

    #[test]
    fn generic_operands_preserved() {
        let epd = Epd::parse("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - acd 14; ce 25;")
            .unwrap();
        assert_eq!(epd.operation("acd"), Some(&["14".to_owned()][..]));
        assert_eq!(epd.operation("ce"), Some(&["25".to_owned()][..]));
    }
}
