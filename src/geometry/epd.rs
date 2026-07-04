//! Extended Position Description (EPD) for the fairy-variant layer.
//!
//! EPD is a relative of FEN: it shares FEN's leading position fields but drops
//! the move clocks, replacing them with a free-form list of *operations* — an
//! `opcode`, zero or more operands, and a terminating semicolon. It is the
//! lingua franca of test suites (best-move / avoid-move suites) and analysis
//! interchange. The concrete [`Epd`](crate::Epd) covers standard chess; this
//! module adds the wide-layer counterpart, [`WideEpd`], purely additively (no
//! move-generation or perft behaviour changes).
//!
//! EPD carries no variant field, so [`WideEpd::parse`] takes a [`WideVariantId`]
//! alongside the line. The position fields are the variant's structural FEN
//! fields (the move clocks dropped) — the same split [`WideEpd::to_epd`] writes,
//! so a record always round-trips. The common opcodes are:
//!
//! - `bm` / `am` — *best move(s)* / *avoid move(s)*, in SAN, resolved against the
//!   position into concrete [`WideMove`]s ([`WideEpd::best_moves`],
//!   [`WideEpd::avoid_moves`]).
//! - `id` — a quoted identifier string ([`WideEpd::id`]).
//! - any other opcode — kept verbatim as raw string operands and surfaced through
//!   [`WideEpd::operation`], so an [`WideEpd`] round-trips through
//!   [`WideEpd::to_epd`] without losing operations it does not interpret.
//!
//! ```
//! use mcr::geometry::{WideEpd, WideVariantId};
//!
//! let epd = WideEpd::parse(
//!     WideVariantId::Makruk,
//!     "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w - - bm Kc2; id \"makruk-start\";",
//! )
//! .unwrap();
//! assert_eq!(epd.id(), Some("makruk-start"));
//! assert_eq!(epd.best_moves().unwrap().unwrap().len(), 1);
//! // The record round-trips through its EPD text.
//! let reparsed = WideEpd::parse(WideVariantId::Makruk, &epd.to_epd()).unwrap();
//! assert_eq!(reparsed.to_epd(), epd.to_epd());
//! ```

use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use super::{AnyWideVariant, WideFenError, WideMove, WideSanError, WideVariantId};

/// A parsed fairy-variant EPD record: an [`AnyWideVariant`] position plus its
/// list of operations.
///
/// Build one with [`WideEpd::parse`] (passing the variant, since EPD has no
/// variant field) and serialize it back with [`WideEpd::to_epd`]. Operations are
/// stored in input order; query them with [`WideEpd::operation`], or use the
/// typed helpers [`WideEpd::id`], [`WideEpd::best_moves`], and
/// [`WideEpd::avoid_moves`].
#[derive(Debug, Clone)]
pub struct WideEpd {
    position: AnyWideVariant,
    /// Operations in input order: `(opcode, operands)`, each operand the raw
    /// token text with any surrounding double quotes already stripped.
    operations: Vec<(String, Vec<String>)>,
}

impl WideEpd {
    /// Parses an EPD record of the variant named by `variant`: the variant's
    /// structural position fields followed by zero or more `opcode operand... ;`
    /// operations.
    ///
    /// # Errors
    ///
    /// Returns [`WideEpdError`] if the input is not valid ASCII, the position
    /// fields are missing or describe an impossible position, or an operation is
    /// malformed (for example an unterminated quoted string).
    pub fn parse(variant: WideVariantId, input: &str) -> Result<WideEpd, WideEpdError> {
        if !input.is_ascii() {
            return Err(WideEpdError::NonAscii);
        }

        // EPD's position is the variant's structural FEN fields (the move clocks
        // dropped). Reattach the variant's default clock fields so the full FEN
        // parser accepts the line.
        let (field_count, clocks) = position_layout(variant);
        let mut fields = input.split_whitespace();
        let mut position_fields: Vec<&str> = Vec::with_capacity(field_count);
        for _ in 0..field_count {
            position_fields.push(fields.next().ok_or(WideEpdError::MissingPosition)?);
        }
        let mut fen = position_fields.join(" ");
        if !clocks.is_empty() {
            fen.push(' ');
            fen.push_str(&clocks);
        }
        let position = AnyWideVariant::from_fen(variant, &fen).map_err(WideEpdError::Position)?;

        let ops_str = remainder_after_fields(input, field_count);
        let operations = parse_operations(ops_str)?;

        Ok(WideEpd {
            position,
            operations,
        })
    }

    /// The position described by this record.
    #[must_use]
    pub fn position(&self) -> &AnyWideVariant {
        &self.position
    }

    /// Consumes the record and returns its position.
    #[must_use]
    pub fn into_position(self) -> AnyWideVariant {
        self.position
    }

    /// The variant of this record's position.
    #[must_use]
    pub fn variant(&self) -> WideVariantId {
        self.position.variant_id()
    }

    /// The operands of the first operation with the given `opcode`, or `None` if
    /// the record has no such operation.
    #[must_use]
    pub fn operation(&self, opcode: &str) -> Option<&[String]> {
        self.operations
            .iter()
            .find(|(op, _)| op == opcode)
            .map(|(_, operands)| operands.as_slice())
    }

    /// Every operation in the record, in input order, as `(opcode, operands)`.
    #[must_use]
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
    /// Returns `None` if the record has no `bm` operation, or `Some(Err(..))` if
    /// a `bm` operand names no legal move in this position.
    #[must_use = "the result reports whether every best move resolved"]
    pub fn best_moves(&self) -> Option<Result<Vec<WideMove>, WideSanError>> {
        self.resolve_moves("bm")
    }

    /// The avoid moves (`am` operation), resolved against the position from their
    /// SAN operands. See [`WideEpd::best_moves`].
    #[must_use = "the result reports whether every avoid move resolved"]
    pub fn avoid_moves(&self) -> Option<Result<Vec<WideMove>, WideSanError>> {
        self.resolve_moves("am")
    }

    /// Resolves every operand of `opcode` as a SAN move against the position.
    fn resolve_moves(&self, opcode: &str) -> Option<Result<Vec<WideMove>, WideSanError>> {
        let operands = self.operation(opcode)?;
        Some(
            operands
                .iter()
                .map(|san| self.position.parse_san(san).ok_or(WideSanError::Illegal))
                .collect(),
        )
    }

    /// Serializes this record back into an EPD string: the structural position
    /// fields followed by each operation, terminated by `;`.
    ///
    /// The move clocks are dropped. Operations are written in stored order; an
    /// `id` operand (and any operand that needs it) is re-quoted so the output
    /// re-parses.
    #[must_use]
    pub fn to_epd(&self) -> String {
        let (field_count, _) = position_layout(self.position.variant_id());
        let fen = self.position.to_fen();
        let mut out = String::with_capacity(fen.len() + 16);
        for field in fen.split_whitespace().take(field_count) {
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

/// The number of structural (clock-free) position fields in `variant`'s FEN, and
/// the variant's default trailing clock fields (joined by spaces).
///
/// The clocks are the trailing purely-numeric FEN fields; everything before them
/// is the EPD position. Reattaching the defaults lets the full FEN parser read a
/// clockless EPD line.
fn position_layout(variant: WideVariantId) -> (usize, String) {
    let fen = AnyWideVariant::startpos(variant).to_fen();
    let fields: Vec<&str> = fen.split_whitespace().collect();
    let mut count = fields.len();
    while count > 1 && fields[count - 1].bytes().all(|b| b.is_ascii_digit()) {
        count -= 1;
    }
    let clocks = fields[count..].join(" ");
    (count, clocks)
}

/// Returns the slice of `input` that follows its first `n` whitespace-separated
/// fields, with the leading whitespace trimmed.
fn remainder_after_fields(input: &str, n: usize) -> &str {
    let bytes = input.as_bytes();
    let mut idx = 0;
    let mut fields_seen = 0;

    while idx < bytes.len() && fields_seen < n {
        while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
            idx += 1;
        }
        if idx >= bytes.len() {
            break;
        }
        while idx < bytes.len() && !bytes[idx].is_ascii_whitespace() {
            idx += 1;
        }
        fields_seen += 1;
    }

    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }

    &input[idx..]
}

/// Parses the operation list into `(opcode, operands)` pairs. A trailing
/// operation without its `;` is accepted leniently.
fn parse_operations(s: &str) -> Result<Vec<(String, Vec<String>)>, WideEpdError> {
    let bytes = s.as_bytes();
    let mut idx = 0;
    let mut ops = Vec::new();

    loop {
        while idx < bytes.len() && (bytes[idx].is_ascii_whitespace() || bytes[idx] == b';') {
            idx += 1;
        }
        if idx >= bytes.len() {
            break;
        }

        let opcode = match next_token(s, &mut idx)? {
            Some(tok) => tok,
            None => break,
        };

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

/// Reads the next token starting at `*idx`, advancing past it. A token is either
/// a double-quoted string (quotes stripped, closing quote required) or a run of
/// non-whitespace, non-semicolon characters.
fn next_token(s: &str, idx: &mut usize) -> Result<Option<String>, WideEpdError> {
    let bytes = s.as_bytes();
    if *idx >= bytes.len() {
        return Ok(None);
    }

    if bytes[*idx] == b'"' {
        *idx += 1;
        let start = *idx;
        while *idx < bytes.len() && bytes[*idx] != b'"' {
            *idx += 1;
        }
        if *idx >= bytes.len() {
            return Err(WideEpdError::UnterminatedString);
        }
        let token = s[start..*idx].to_owned();
        *idx += 1;
        return Ok(Some(token));
    }

    let start = *idx;
    while *idx < bytes.len() && !bytes[*idx].is_ascii_whitespace() && bytes[*idx] != b';' {
        *idx += 1;
    }
    if *idx == start {
        return Ok(None);
    }
    Ok(Some(s[start..*idx].to_owned()))
}

/// Writes one operand of `opcode` to `out`, quoting it when a bare token cannot
/// represent it so the output re-parses to the same operand.
fn write_operand(out: &mut String, opcode: &str, operand: &str) {
    let needs_quote = opcode == "id"
        || operand.is_empty()
        || operand
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b == b';' || b == b'"');
    if needs_quote {
        out.push('"');
        for ch in operand.chars().filter(|&c| c != '"') {
            out.push(ch);
        }
        out.push('"');
    } else {
        out.push_str(operand);
    }
}

/// The error returned when an EPD record cannot be parsed by [`WideEpd::parse`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WideEpdError {
    /// The input contained non-ASCII bytes (EPD is an ASCII format).
    NonAscii,
    /// Fewer than the variant's required position fields were present.
    MissingPosition,
    /// The position fields did not describe a valid position.
    Position(WideFenError),
    /// A quoted operand string was opened but never closed.
    UnterminatedString,
}

impl fmt::Display for WideEpdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WideEpdError::NonAscii => f.write_str("EPD input is not valid ASCII"),
            WideEpdError::MissingPosition => {
                f.write_str("EPD is missing a required position field")
            }
            WideEpdError::Position(e) => write!(f, "invalid EPD position: {e}"),
            WideEpdError::UnterminatedString => {
                f.write_str("unterminated quoted string in EPD operand")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for WideEpdError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WideEpdError::Position(e) => Some(e),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bm_and_id() {
        let line = "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w - - bm Kc2 e4; id \"makruk\";";
        let epd = WideEpd::parse(WideVariantId::Makruk, line).unwrap();
        assert_eq!(epd.variant(), WideVariantId::Makruk);
        assert_eq!(epd.id(), Some("makruk"));
        let bm = epd.best_moves().unwrap().unwrap();
        assert_eq!(bm.len(), 2);
        let pos = epd.position();
        assert_eq!(bm[0], pos.parse_san("Kc2").unwrap());
        assert_eq!(bm[1], pos.parse_san("e4").unwrap());
    }

    #[test]
    fn resolves_avoid_moves() {
        let line = "rn*xkm*xnr/pppppppp/8/8/8/8/PPPPPPPP/RN*XKM*XNR w - - am Na3;";
        let epd = WideEpd::parse(WideVariantId::Shatranj, line).unwrap();
        let am = epd.avoid_moves().unwrap().unwrap();
        assert_eq!(am.len(), 1);
        assert_eq!(am[0], epd.position().parse_san("Na3").unwrap());
    }

    #[test]
    fn round_trips_makruk() {
        let line = "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w - - bm Kc2; id \"start\";";
        let epd = WideEpd::parse(WideVariantId::Makruk, line).unwrap();
        let written = epd.to_epd();
        let reparsed = WideEpd::parse(WideVariantId::Makruk, &written).unwrap();
        assert_eq!(reparsed.to_epd(), written);
        assert_eq!(reparsed.id(), Some("start"));
        assert_eq!(reparsed.best_moves().unwrap().unwrap().len(), 1);
        assert_eq!(reparsed.position().to_fen(), epd.position().to_fen());
    }

    #[test]
    fn round_trips_xiangqi_across_geometry() {
        let line =
            "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/9/RJOUKUOJR w - - bm Ra2; id \"xq\";";
        let epd = WideEpd::parse(WideVariantId::Xiangqi, line).unwrap();
        let reparsed = WideEpd::parse(WideVariantId::Xiangqi, &epd.to_epd()).unwrap();
        assert_eq!(reparsed.to_epd(), epd.to_epd());
        assert_eq!(reparsed.id(), Some("xq"));
        assert_eq!(epd.best_moves().unwrap().unwrap().len(), 1);
    }

    #[test]
    fn no_operations() {
        let epd = WideEpd::parse(
            WideVariantId::Makruk,
            "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w - -",
        )
        .unwrap();
        assert!(epd.operations().is_empty());
        assert!(epd.id().is_none());
        assert!(epd.best_moves().is_none());
    }

    #[test]
    fn generic_operands_preserved() {
        let line = "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w - - acd 14; ce 25;";
        let epd = WideEpd::parse(WideVariantId::Makruk, line).unwrap();
        assert_eq!(epd.operation("acd"), Some(&["14".to_owned()][..]));
        assert_eq!(epd.operation("ce"), Some(&["25".to_owned()][..]));
        let reparsed = WideEpd::parse(WideVariantId::Makruk, &epd.to_epd()).unwrap();
        assert_eq!(reparsed.to_epd(), epd.to_epd());
    }

    #[test]
    fn rejects_truncated_position() {
        assert_eq!(
            WideEpd::parse(WideVariantId::Makruk, "rnsmksnr/8/pppppppp w").unwrap_err(),
            WideEpdError::MissingPosition
        );
    }

    #[test]
    fn rejects_bad_board() {
        assert!(matches!(
            WideEpd::parse(WideVariantId::Makruk, "not_a_board w - - bm Kc2;").unwrap_err(),
            WideEpdError::Position(_)
        ));
    }

    #[test]
    fn rejects_non_ascii() {
        assert_eq!(
            WideEpd::parse(
                WideVariantId::Makruk,
                "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w - - id \"caf\u{e9}\";"
            )
            .unwrap_err(),
            WideEpdError::NonAscii
        );
    }

    #[test]
    fn rejects_unterminated_string() {
        assert_eq!(
            WideEpd::parse(
                WideVariantId::Makruk,
                "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w - - id \"oops;"
            )
            .unwrap_err(),
            WideEpdError::UnterminatedString
        );
    }

    #[test]
    fn illegal_bm_resolves_to_error() {
        let epd = WideEpd::parse(
            WideVariantId::Makruk,
            "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w - - bm Qh5;",
        )
        .unwrap();
        assert!(epd.best_moves().unwrap().is_err());
    }

    #[test]
    fn parse_never_panics_on_arbitrary_input() {
        let inputs = [
            "",
            " ",
            ";",
            "\"",
            "x",
            "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR",
            "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w - - bm",
            "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w - - \"\"\"\";;;",
        ];
        for input in inputs {
            let _ = WideEpd::parse(WideVariantId::Makruk, input);
        }
    }
}
