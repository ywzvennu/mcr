//! Portable Game Notation (PGN) reading and writing for whole games.
//!
//! Where [`Position::san`](crate::Position::san) handles a *single* move, this
//! module handles a *game*: the `[Tag "Value"]` header pairs, the move-number /
//! SAN movetext (with `+`/`#` suffixes, `$n` NAGs, `{...}` comments, and
//! `(...)` recursive variations), and the game-terminating result token. It is a
//! hand-written parser and writer with no extra dependencies.
//!
//! The central type is [`Pgn`]: a parsed game holding its tag pairs, the start
//! position (which may be a `[SetUp "1"][FEN "..."]` custom position and/or a
//! `[Variant "..."]` non-standard variant), the validated mainline of moves, and
//! the result.
//!
//! - [`Pgn::from_pgn`] parses one game from a string, validating every move in
//!   the mainline against the running position. It is panic-free on arbitrary,
//!   malformed, non-ASCII, or truncated input: it always returns either an
//!   [`Pgn`] or a [`PgnError`].
//! - [`Pgn::to_pgn`] serializes a game back to canonical PGN text.
//!
//! # Scope
//!
//! Only the *mainline* is replayed and stored as moves. Recursive variations
//! (`(...)`) are tolerated — they are skipped over by balanced-paren scanning so
//! they never corrupt the mainline — but their contents are not retained. Tag
//! pairs and per-move annotations (NAGs, comments) are preserved across a
//! round-trip.
//!
//! ```
//! use mce::Pgn;
//!
//! let text = "[Event \"Test\"]\n[Result \"1-0\"]\n\n1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6 4. Qxf7# 1-0\n";
//! let pgn = Pgn::from_pgn(text).unwrap();
//! assert_eq!(pgn.tag("Event"), Some("Test"));
//! assert_eq!(pgn.moves().len(), 7);
//! // The final position is checkmate.
//! assert!(pgn.final_position().outcome().is_some());
//! ```

use core::fmt;

use crate::{AnyVariant, Move, Position, SanError, VariantId};

/// One `[Name "Value"]` header tag pair.
type TagPair = (String, String);

/// The error returned when a PGN string cannot be parsed by [`Pgn::from_pgn`].
///
/// Every malformed, truncated, or non-ASCII input yields one of these rather
/// than panicking, so the parser is safe to run over untrusted or fuzzed data.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PgnError {
    /// A tag-pair line was opened with `[` but was not a well-formed
    /// `[Name "Value"]` pair (missing name, unquoted or unterminated value, or a
    /// missing closing bracket).
    MalformedTag,
    /// A `[Variant "..."]` tag named a variant this engine does not recognize.
    UnknownVariant(String),
    /// A `[SetUp "1"]`/`[FEN "..."]` start position (or a variant start position)
    /// could not be parsed as a valid FEN.
    InvalidFen,
    /// A `{...}` comment or `(...)` variation was opened but never closed before
    /// the end of the input.
    UnterminatedComment,
    /// A movetext token that should have been SAN did not resolve to a legal move
    /// in the running position. Carries the offending token and the reason.
    IllegalMove {
        /// The SAN token that failed to resolve.
        san: String,
        /// Why it failed (no legal move, ambiguous, malformed, ...).
        reason: SanError,
    },
    /// A token in the movetext was neither a move number, SAN, NAG, comment,
    /// variation, nor a recognized result, and could not be skipped safely.
    UnexpectedToken(String),
}

impl fmt::Display for PgnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PgnError::MalformedTag => f.write_str("malformed PGN tag pair"),
            PgnError::UnknownVariant(v) => write!(f, "unknown variant {v:?} in [Variant] tag"),
            PgnError::InvalidFen => f.write_str("invalid FEN in start position"),
            PgnError::UnterminatedComment => {
                f.write_str("unterminated comment or variation in movetext")
            }
            PgnError::IllegalMove { san, reason } => {
                write!(f, "illegal SAN move {san:?} in movetext: {reason}")
            }
            PgnError::UnexpectedToken(t) => write!(f, "unexpected token {t:?} in movetext"),
        }
    }
}

impl std::error::Error for PgnError {}

/// The result of a PGN game, taken from the `[Result "..."]` tag and the
/// game-terminating movetext token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PgnResult {
    /// White won (`1-0`).
    WhiteWins,
    /// Black won (`0-1`).
    BlackWins,
    /// Draw (`1/2-1/2`).
    Draw,
    /// Unknown / game in progress (`*`).
    #[default]
    Unknown,
}

impl PgnResult {
    /// The canonical PGN token for this result (`1-0`, `0-1`, `1/2-1/2`, `*`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            PgnResult::WhiteWins => "1-0",
            PgnResult::BlackWins => "0-1",
            PgnResult::Draw => "1/2-1/2",
            PgnResult::Unknown => "*",
        }
    }

    /// Parses a result token, returning `None` if it is not one of the four
    /// recognized PGN result strings.
    #[must_use]
    pub fn from_token(s: &str) -> Option<PgnResult> {
        match s {
            "1-0" => Some(PgnResult::WhiteWins),
            "0-1" => Some(PgnResult::BlackWins),
            "1/2-1/2" => Some(PgnResult::Draw),
            "*" => Some(PgnResult::Unknown),
            _ => None,
        }
    }
}

impl fmt::Display for PgnResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A single mainline move together with the annotations attached to it.
///
/// The move is stored both as the concrete legal [`Move`] (resolved against the
/// position it was played in) and as the canonical SAN string, so writing can
/// re-emit identical movetext without needing the position again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PgnMove {
    /// The concrete legal move, resolved against the position before it.
    mv: Move,
    /// The canonical SAN for the move, as rendered in the position before it.
    san: String,
    /// Numeric Annotation Glyphs (`$n`) attached to this move, in order.
    nags: Vec<u16>,
    /// `{...}` comments attached after this move, in order, without the braces.
    comments: Vec<String>,
}

impl PgnMove {
    /// The concrete legal move.
    #[must_use]
    pub const fn mv(&self) -> Move {
        self.mv
    }

    /// The canonical SAN string for the move.
    #[must_use]
    pub fn san(&self) -> &str {
        &self.san
    }

    /// The NAGs (`$n`) attached to this move, in order.
    #[must_use]
    pub fn nags(&self) -> &[u16] {
        &self.nags
    }

    /// The comments attached after this move, in order (brace-free).
    #[must_use]
    pub fn comments(&self) -> &[String] {
        &self.comments
    }
}

/// A parsed PGN game: tag pairs, start position, mainline moves, and result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pgn {
    /// Tag pairs in file order. Order is preserved across a round-trip; the
    /// Seven-Tag Roster is moved to the front on write if missing-but-derivable.
    tags: Vec<(String, String)>,
    /// The variant this game is played in (from a `[Variant "..."]` tag, or
    /// [`VariantId::Standard`]).
    variant: VariantId,
    /// The start position. For a standard game from the initial array this is the
    /// variant start position; with `[SetUp "1"][FEN "..."]` it is the parsed
    /// FEN.
    start: AnyVariant,
    /// Whether the start position came from an explicit `[FEN]` tag (as opposed to
    /// the default start array), which controls whether `to_pgn` re-emits SetUp/FEN.
    explicit_fen: bool,
    /// The mainline moves, in order.
    moves: Vec<PgnMove>,
    /// Free-text comments appearing before the first move (game-level preamble).
    initial_comments: Vec<String>,
    /// The game result.
    result: PgnResult,
}

impl Pgn {
    /// The tag pairs in file order.
    #[must_use]
    pub fn tags(&self) -> &[(String, String)] {
        &self.tags
    }

    /// The value of the named tag, if present (first match wins).
    #[must_use]
    pub fn tag(&self, name: &str) -> Option<&str> {
        self.tags
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// The variant this game is played in.
    #[must_use]
    pub const fn variant(&self) -> VariantId {
        self.variant
    }

    /// The start position of the game.
    #[must_use]
    pub const fn start_position(&self) -> &AnyVariant {
        &self.start
    }

    /// Whether the start position came from an explicit `[FEN]` tag.
    #[must_use]
    pub const fn has_explicit_fen(&self) -> bool {
        self.explicit_fen
    }

    /// The mainline moves.
    #[must_use]
    pub fn moves(&self) -> &[PgnMove] {
        &self.moves
    }

    /// The game result.
    #[must_use]
    pub const fn result(&self) -> PgnResult {
        self.result
    }

    /// The game-level comments appearing before the first move.
    #[must_use]
    pub fn initial_comments(&self) -> &[String] {
        &self.initial_comments
    }

    /// Replays the mainline from the start and returns the final position.
    ///
    /// Every stored move is legal by construction (it was validated during
    /// parsing or when the game was built), so the replay never fails.
    #[must_use]
    pub fn final_position(&self) -> AnyVariant {
        let mut pos = self.start.clone();
        for m in &self.moves {
            pos = pos.play(&m.mv);
        }
        pos
    }

    /// Parses a single PGN game from `text`.
    ///
    /// The header tag pairs are read first; `[Variant "..."]`, `[SetUp "1"]`, and
    /// `[FEN "..."]` together choose the start position. The movetext is then
    /// tokenized and each SAN move validated against the running position. NAGs,
    /// comments, and the result are retained; recursive variations are skipped.
    ///
    /// # Errors
    ///
    /// Returns a [`PgnError`] for any malformed tag, unknown variant, invalid
    /// start FEN, unterminated comment/variation, illegal move, or unrecognized
    /// token. The function never panics, including on non-ASCII or truncated
    /// input.
    pub fn from_pgn(text: &str) -> Result<Pgn, PgnError> {
        let (tags, rest) = parse_tags(text)?;

        // Resolve variant and start position from the header.
        let variant = match tags.iter().find(|(k, _)| k == "Variant") {
            Some((_, v)) => v
                .parse::<VariantId>()
                .map_err(|_| PgnError::UnknownVariant(v.clone()))?,
            None => VariantId::Standard,
        };

        let setup = tags
            .iter()
            .find(|(k, _)| k == "SetUp")
            .map(|(_, v)| v.as_str());
        let fen = tags
            .iter()
            .find(|(k, _)| k == "FEN")
            .map(|(_, v)| v.as_str());

        // A FEN tag (optionally gated by SetUp "1") defines the start position.
        let (start, explicit_fen) = match fen {
            Some(fen) if setup != Some("0") => (
                AnyVariant::from_fen(variant, fen).map_err(|_| PgnError::InvalidFen)?,
                true,
            ),
            _ => (AnyVariant::startpos(variant), false),
        };

        let header_result = tags
            .iter()
            .find(|(k, _)| k == "Result")
            .and_then(|(_, v)| PgnResult::from_token(v));

        let mut game = Pgn {
            tags,
            variant,
            start: start.clone(),
            explicit_fen,
            moves: Vec::new(),
            initial_comments: Vec::new(),
            result: header_result.unwrap_or_default(),
        };

        let movetext_result = parse_movetext(rest, &start, &mut game)?;
        if let Some(r) = movetext_result {
            game.result = r;
        }
        Ok(game)
    }

    /// Serializes this game to canonical PGN text.
    ///
    /// The header is the tag pairs in their stored order (Seven-Tag Roster
    /// fields are emitted as stored). A `[SetUp "1"][FEN "..."]` pair is present
    /// in the tags when the game started from a custom position. The movetext is
    /// wrapped to a conventional column width, with move numbers, SAN, NAGs,
    /// comments, and the trailing result token.
    #[must_use]
    pub fn to_pgn(&self) -> String {
        let mut out = String::new();

        // --- Header ---
        for (k, v) in &self.tags {
            out.push('[');
            out.push_str(k);
            out.push_str(" \"");
            // Escape backslashes and quotes per the PGN spec.
            for ch in v.chars() {
                if ch == '\\' || ch == '"' {
                    out.push('\\');
                }
                out.push(ch);
            }
            out.push_str("\"]\n");
        }
        out.push('\n');

        // --- Movetext ---
        let mut line = String::new();
        let push_token = |out: &mut String, line: &mut String, tok: &str| {
            if line.is_empty() {
                line.push_str(tok);
            } else if line.len() + 1 + tok.len() > 80 {
                out.push_str(line);
                out.push('\n');
                line.clear();
                line.push_str(tok);
            } else {
                line.push(' ');
                line.push_str(tok);
            }
        };

        for c in &self.initial_comments {
            push_token(&mut out, &mut line, &format!("{{{c}}}"));
        }

        // Determine the side to move and starting move number from the start FEN.
        let start_fen = self.start.to_fen();
        let (mut fullmove, mut white_to_move) = parse_side_and_number(&start_fen);

        for m in &self.moves {
            if white_to_move {
                push_token(&mut out, &mut line, &format!("{fullmove}."));
            } else if line.is_empty() {
                // Black move at the start of a wrapped line / after a comment:
                // re-emit the move number with the "..." continuation marker.
                push_token(&mut out, &mut line, &format!("{fullmove}..."));
            }
            push_token(&mut out, &mut line, &m.san);
            for &nag in &m.nags {
                push_token(&mut out, &mut line, &format!("${nag}"));
            }
            for c in &m.comments {
                push_token(&mut out, &mut line, &format!("{{{c}}}"));
            }
            if !white_to_move {
                fullmove += 1;
            }
            white_to_move = !white_to_move;
        }

        push_token(&mut out, &mut line, self.result.as_str());
        if !line.is_empty() {
            out.push_str(&line);
        }
        out.push('\n');
        out
    }
}

/// Renders SAN for a move in an [`AnyVariant`] position, using the crazyhouse
/// drop-aware renderer where relevant and the core renderer otherwise.
fn variant_san(pos: &AnyVariant, mv: &Move) -> String {
    match pos {
        AnyVariant::Crazyhouse(p) => p.san(mv),
        AnyVariant::Chess(p) => p.core().san(mv),
        AnyVariant::Chess960(p) => p.core().san(mv),
        AnyVariant::KingOfTheHill(p) => p.core().san(mv),
        AnyVariant::ThreeCheck(p) => p.core().san(mv),
        AnyVariant::RacingKings(p) => p.core().san(mv),
        AnyVariant::Horde(p) => p.core().san(mv),
        AnyVariant::Atomic(p) => p.core().san(mv),
        AnyVariant::Antichess(p) => p.core().san(mv),
    }
}

/// Resolves a SAN token to a legal move in an [`AnyVariant`] position, using the
/// crazyhouse drop-aware parser where relevant and the core parser otherwise.
fn variant_parse_san(pos: &AnyVariant, san: &str) -> Result<Move, SanError> {
    match pos {
        AnyVariant::Crazyhouse(p) => p.parse_san(san),
        AnyVariant::Chess(p) => p.core().parse_san(san),
        AnyVariant::Chess960(p) => p.core().parse_san(san),
        AnyVariant::KingOfTheHill(p) => p.core().parse_san(san),
        AnyVariant::ThreeCheck(p) => p.core().parse_san(san),
        AnyVariant::RacingKings(p) => p.core().parse_san(san),
        AnyVariant::Horde(p) => p.core().parse_san(san),
        AnyVariant::Atomic(p) => p.core().parse_san(san),
        AnyVariant::Antichess(p) => p.core().parse_san(san),
    }
}

/// Parses the `[Name "Value"]` tag-pair header at the front of `text`, returning
/// the pairs (in order) and the remaining movetext.
///
/// Lines outside tag pairs, blank lines, and `%`-escaped lines are skipped until
/// the first non-tag, non-blank content, which begins the movetext.
fn parse_tags(text: &str) -> Result<(Vec<TagPair>, &str), PgnError> {
    let mut tags = Vec::new();
    let mut rest = text;

    loop {
        // Skip leading whitespace and blank/escaped lines, but stop at content.
        let trimmed = rest.trim_start_matches([' ', '\t', '\r', '\n']);
        // `%`-escape: a line beginning with `%` is ignored entirely.
        if let Some(after_pct) = trimmed.strip_prefix('%') {
            rest = after_pct.find('\n').map_or("", |nl| &after_pct[nl + 1..]);
            continue;
        }
        if !trimmed.starts_with('[') {
            return Ok((tags, trimmed));
        }
        // Parse one tag pair: [Name "Value"].
        let close = trimmed.find(']').ok_or(PgnError::MalformedTag)?;
        let inner = &trimmed[1..close];
        let (name, value) = parse_tag_inner(inner)?;
        tags.push((name, value));
        rest = &trimmed[close + 1..];
    }
}

/// Parses the inside of one tag pair (`Name "Value"`), unescaping `\\` and `\"`.
fn parse_tag_inner(inner: &str) -> Result<(String, String), PgnError> {
    let inner = inner.trim();
    let q = inner.find('"').ok_or(PgnError::MalformedTag)?;
    let name = inner[..q].trim();
    if name.is_empty() || name.contains(|c: char| c.is_whitespace()) {
        return Err(PgnError::MalformedTag);
    }
    // The value runs from the opening quote to the matching unescaped quote.
    let after = &inner[q + 1..];
    let mut value = String::new();
    let mut escaped = false;
    let mut closed = false;
    let mut tail_start = after.len();
    for (i, ch) in after.char_indices() {
        if escaped {
            value.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            closed = true;
            tail_start = i + ch.len_utf8();
            break;
        } else {
            value.push(ch);
        }
    }
    if !closed {
        return Err(PgnError::MalformedTag);
    }
    // Anything after the closing quote (other than whitespace) is malformed.
    if !after[tail_start..].trim().is_empty() {
        return Err(PgnError::MalformedTag);
    }
    Ok((name.to_string(), value))
}

/// Parses (and replays) the movetext, populating `game.moves`,
/// `game.initial_comments`, and returning the result token if one was found.
fn parse_movetext(
    text: &str,
    start: &AnyVariant,
    game: &mut Pgn,
) -> Result<Option<PgnResult>, PgnError> {
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut pos = start.clone();
    let mut result = None;
    // NAGs/comments seen before the first move attach to `initial_comments`.
    let mut have_first_move = false;

    while i < bytes.len() {
        let b = bytes[i];
        match b {
            // Whitespace.
            b' ' | b'\t' | b'\r' | b'\n' => {
                i += 1;
            }
            // Comment to end of line.
            b';' => {
                i = text[i..].find('\n').map_or(bytes.len(), |nl| i + nl + 1);
            }
            // Brace comment.
            b'{' => {
                let end = text[i + 1..]
                    .find('}')
                    .ok_or(PgnError::UnterminatedComment)?;
                let comment = text[i + 1..i + 1 + end].trim().to_string();
                if have_first_move {
                    if let Some(last) = game.moves.last_mut() {
                        last.comments.push(comment);
                    } else {
                        game.initial_comments.push(comment);
                    }
                } else {
                    game.initial_comments.push(comment);
                }
                i += 1 + end + 1;
            }
            // Recursive variation: skip with balanced-paren scanning.
            b'(' => {
                i = skip_variation(text, i)?;
            }
            // A stray `)` without a matching `(` — tolerate by skipping it.
            b')' => {
                i += 1;
            }
            // NAG.
            b'$' => {
                let mut j = i + 1;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if j > i + 1 {
                    let nag: u16 = text[i + 1..j].parse().unwrap_or(0);
                    if let Some(last) = game.moves.last_mut() {
                        last.nags.push(nag);
                    }
                }
                i = j;
            }
            // A move number or a token starting with a digit (move number,
            // result like `1-0`, or `1/2-1/2`).
            _ => {
                let tok_end = token_end(bytes, i);
                let token = &text[i..tok_end];
                i = tok_end;

                // Pure move-number tokens (e.g. `1.`, `12...`, `1`) are skipped.
                if is_move_number(token) {
                    continue;
                }
                // Result tokens end the game.
                if let Some(r) = PgnResult::from_token(token) {
                    result = Some(r);
                    break;
                }
                // Otherwise it must be SAN. A token may carry leading move-number
                // digits glued to the SAN (e.g. `1.e4`); strip them.
                let san = strip_leading_move_number(token);
                if san.is_empty() {
                    continue;
                }
                let mv = variant_parse_san(&pos, san).map_err(|reason| PgnError::IllegalMove {
                    san: san.to_string(),
                    reason,
                })?;
                let canonical = variant_san(&pos, &mv);
                pos = pos.play(&mv);
                have_first_move = true;
                game.moves.push(PgnMove {
                    mv,
                    san: canonical,
                    nags: Vec::new(),
                    comments: Vec::new(),
                });
            }
        }
    }
    Ok(result)
}

/// Returns the byte index just past a whitespace/structure-delimited token that
/// starts at `i`.
fn token_end(bytes: &[u8], i: usize) -> usize {
    let mut j = i;
    while j < bytes.len() {
        match bytes[j] {
            b' ' | b'\t' | b'\r' | b'\n' | b'{' | b'}' | b'(' | b')' | b';' | b'$' => break,
            _ => j += 1,
        }
    }
    j
}

/// Skips a balanced `(...)` variation starting at `start` (the `(`), returning
/// the index just past the matching `)`. Brace comments inside are respected so
/// a `)` inside a comment does not close the variation.
fn skip_variation(text: &str, start: usize) -> Result<usize, PgnError> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    return Ok(i);
                }
            }
            b'{' => {
                let end = text[i + 1..]
                    .find('}')
                    .ok_or(PgnError::UnterminatedComment)?;
                i += 1 + end + 1;
            }
            b';' => {
                i = text[i..].find('\n').map_or(bytes.len(), |nl| i + nl + 1);
            }
            _ => i += 1,
        }
    }
    Err(PgnError::UnterminatedComment)
}

/// Whether `token` is purely a move-number indicator: digits optionally followed
/// by one or more `.` (e.g. `1`, `12.`, `7...`).
fn is_move_number(token: &str) -> bool {
    let mut seen_digit = false;
    let mut chars = token.chars();
    for ch in chars.by_ref() {
        if ch.is_ascii_digit() {
            seen_digit = true;
        } else {
            // The first non-digit must begin a run of dots to the end.
            if ch != '.' {
                return false;
            }
            return token
                .chars()
                .skip_while(char::is_ascii_digit)
                .all(|c| c == '.')
                && seen_digit;
        }
    }
    // All digits, no dots: still a (bare) move number.
    seen_digit
}

/// Strips a leading `<digits>.` or `<digits>...` move-number prefix glued to a
/// SAN token, returning the SAN tail (which may be empty).
fn strip_leading_move_number(token: &str) -> &str {
    let trimmed = token.trim_start_matches(|c: char| c.is_ascii_digit());
    if trimmed.len() == token.len() {
        // No leading digits at all; nothing was a move number.
        return token;
    }
    // Only treat the digit prefix as a move number if it is followed by dot(s).
    let after_dots = trimmed.trim_start_matches('.');
    if after_dots.len() == trimmed.len() {
        // Digits not followed by a dot — not a glued move number (could be part
        // of nothing meaningful in SAN, but leave the token intact for parsing).
        return token;
    }
    after_dots
}

/// Reads the side-to-move and fullmove number out of a FEN string, defaulting to
/// white-to-move / move 1 if the FEN is too short to contain them.
fn parse_side_and_number(fen: &str) -> (u32, bool) {
    let mut fields = fen.split_whitespace();
    let _placement = fields.next();
    let side = fields.next();
    // Skip castling and en-passant fields.
    let _ = fields.next();
    let _ = fields.next();
    let _halfmove = fields.next();
    let fullmove = fields
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(1);
    let white = side != Some("b");
    (fullmove, white)
}

/// Builds a [`Pgn`] from a sequence of UCI/`Move`s replayed from `start`, used
/// for round-trip and writer construction without going through text.
impl Pgn {
    /// Constructs a [`Pgn`] from a start position and a list of moves, validating
    /// each move and capturing its canonical SAN.
    ///
    /// The tags are taken verbatim; if no `Result` tag is present, the result is
    /// derived from the final position's outcome (or `*` if the game is still in
    /// progress).
    ///
    /// # Errors
    ///
    /// Returns [`PgnError::IllegalMove`] if any move is not legal in turn.
    pub fn from_moves(
        start: AnyVariant,
        moves: &[Move],
        tags: Vec<(String, String)>,
    ) -> Result<Pgn, PgnError> {
        let variant = start.variant_id();
        let explicit_fen = start.to_fen() != AnyVariant::startpos(variant).to_fen();
        let mut pos = start.clone();
        let mut recorded = Vec::with_capacity(moves.len());
        for mv in moves {
            if !pos.legal_moves().iter().any(|legal| legal == mv) {
                return Err(PgnError::IllegalMove {
                    san: pos.to_uci(mv),
                    reason: SanError::Illegal,
                });
            }
            let san = variant_san(&pos, mv);
            pos = pos.play(mv);
            recorded.push(PgnMove {
                mv: *mv,
                san,
                nags: Vec::new(),
                comments: Vec::new(),
            });
        }

        let result = tags
            .iter()
            .find(|(k, _)| k == "Result")
            .and_then(|(_, v)| PgnResult::from_token(v))
            .unwrap_or_else(|| match pos.outcome() {
                Some(crate::Outcome::Decisive { winner }) => {
                    if winner.is_white() {
                        PgnResult::WhiteWins
                    } else {
                        PgnResult::BlackWins
                    }
                }
                Some(crate::Outcome::Draw) => PgnResult::Draw,
                None => PgnResult::Unknown,
            });

        Ok(Pgn {
            tags,
            variant,
            start,
            explicit_fen,
            moves: recorded,
            initial_comments: Vec::new(),
            result,
        })
    }
}

/// Convenience for building a standard-chess [`Pgn`] from a position's FEN.
///
/// The starting position of a plain `Pgn` parse uses [`Position`] indirectly via
/// [`AnyVariant`]; this helper is exposed for tests and callers that already
/// have a [`Position`].
impl From<Position> for Pgn {
    fn from(position: Position) -> Pgn {
        let fen = position.to_fen();
        // A standard position always parses as the Standard variant.
        let start = AnyVariant::from_fen(VariantId::Standard, &fen)
            .unwrap_or_else(|_| AnyVariant::startpos(VariantId::Standard));
        let explicit_fen = fen != Position::startpos().to_fen();
        Pgn {
            tags: Vec::new(),
            variant: VariantId::Standard,
            start,
            explicit_fen,
            moves: Vec::new(),
            initial_comments: Vec::new(),
            result: PgnResult::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_game_to_mate() {
        let text = "[Event \"Scholar\"]\n[Site \"?\"]\n[Result \"1-0\"]\n\n\
                    1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6?? 4. Qxf7# 1-0\n";
        let pgn = Pgn::from_pgn(text).unwrap();
        assert_eq!(pgn.tag("Event"), Some("Scholar"));
        assert_eq!(pgn.result(), PgnResult::WhiteWins);
        assert_eq!(pgn.moves().len(), 7);
        assert_eq!(pgn.moves()[6].san(), "Qxf7#");
        let final_pos = pgn.final_position();
        assert!(final_pos.outcome().is_some());
    }

    #[test]
    fn parses_nags_and_comments() {
        let text = "1. e4 {best by test} e5 $1 2. Nf3 $2 {develops} Nc6 *\n";
        let pgn = Pgn::from_pgn(text).unwrap();
        assert_eq!(pgn.moves().len(), 4);
        assert_eq!(pgn.moves()[0].comments(), &["best by test".to_string()]);
        assert_eq!(pgn.moves()[1].nags(), &[1]);
        assert_eq!(pgn.moves()[2].nags(), &[2]);
        assert_eq!(pgn.moves()[2].comments(), &["develops".to_string()]);
        assert_eq!(pgn.result(), PgnResult::Unknown);
    }

    #[test]
    fn skips_recursive_variations() {
        // The mainline is e4 e5 Nf3; the (1... c5 ...) variation is skipped.
        let text = "1. e4 e5 (1... c5 2. Nf3 d6) 2. Nf3 Nc6 *\n";
        let pgn = Pgn::from_pgn(text).unwrap();
        let sans: Vec<&str> = pgn.moves().iter().map(PgnMove::san).collect();
        assert_eq!(sans, vec!["e4", "e5", "Nf3", "Nc6"]);
    }

    #[test]
    fn skips_nested_variations_with_comment_paren() {
        // Nested variations and a `)` inside a comment must not confuse the scan.
        let text = "1. e4 e5 (1... c5 (1... e6 {the French )}) 2. Nf3) 2. Nf3 *\n";
        let pgn = Pgn::from_pgn(text).unwrap();
        let sans: Vec<&str> = pgn.moves().iter().map(PgnMove::san).collect();
        assert_eq!(sans, vec!["e4", "e5", "Nf3"]);
    }

    #[test]
    fn supports_setup_fen_start() {
        // A King-and-pawn endgame start.
        let text = "[SetUp \"1\"]\n[FEN \"4k3/8/8/8/8/8/4P3/4K3 w - - 0 1\"]\n\n\
                    1. e4 Kd7 2. e5 *\n";
        let pgn = Pgn::from_pgn(text).unwrap();
        assert!(pgn.has_explicit_fen());
        assert_eq!(pgn.moves().len(), 3);
        assert_eq!(pgn.moves()[0].san(), "e4");
    }

    #[test]
    fn supports_variant_tag() {
        let text = "[Variant \"Atomic\"]\n[Result \"*\"]\n\n1. e4 e5 *\n";
        let pgn = Pgn::from_pgn(text).unwrap();
        assert_eq!(pgn.variant(), VariantId::Atomic);
        assert_eq!(pgn.moves().len(), 2);
    }

    #[test]
    fn unknown_variant_is_rejected() {
        let text = "[Variant \"Frobnicate\"]\n\n1. e4 *\n";
        assert_eq!(
            Pgn::from_pgn(text),
            Err(PgnError::UnknownVariant("Frobnicate".to_string()))
        );
    }

    #[test]
    fn invalid_fen_is_rejected() {
        let text = "[SetUp \"1\"]\n[FEN \"not a fen\"]\n\n1. e4 *\n";
        assert_eq!(Pgn::from_pgn(text), Err(PgnError::InvalidFen));
    }

    #[test]
    fn illegal_move_is_rejected() {
        let text = "1. e4 e5 2. Ke2 Qh4 3. Kxq *\n";
        let err = Pgn::from_pgn(text).unwrap_err();
        assert!(matches!(err, PgnError::IllegalMove { .. }));
    }

    #[test]
    fn round_trip_simple_game() {
        let text = "[Event \"RT\"]\n[Result \"1-0\"]\n\n\
                    1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6 4. Qxf7# 1-0\n";
        let pgn = Pgn::from_pgn(text).unwrap();
        let written = pgn.to_pgn();
        let reparsed = Pgn::from_pgn(&written).unwrap();
        assert_eq!(pgn.tags(), reparsed.tags());
        assert_eq!(pgn.result(), reparsed.result());
        let a: Vec<&str> = pgn.moves().iter().map(PgnMove::san).collect();
        let b: Vec<&str> = reparsed.moves().iter().map(PgnMove::san).collect();
        assert_eq!(a, b);
    }

    #[test]
    fn round_trip_preserves_nags_and_comments() {
        let text = "[Result \"*\"]\n\n1. e4 {a comment} e5 $1 2. Nf3 *\n";
        let pgn = Pgn::from_pgn(text).unwrap();
        let reparsed = Pgn::from_pgn(&pgn.to_pgn()).unwrap();
        assert_eq!(reparsed.moves()[0].comments(), &["a comment".to_string()]);
        assert_eq!(reparsed.moves()[1].nags(), &[1]);
    }

    #[test]
    fn round_trip_setup_fen() {
        let text =
            "[SetUp \"1\"]\n[FEN \"4k3/8/8/8/8/8/4P3/4K3 w - - 0 1\"]\n\n1. e4 Kd7 2. e5 *\n";
        let pgn = Pgn::from_pgn(text).unwrap();
        let reparsed = Pgn::from_pgn(&pgn.to_pgn()).unwrap();
        assert!(reparsed.has_explicit_fen());
        let a: Vec<&str> = pgn.moves().iter().map(PgnMove::san).collect();
        let b: Vec<&str> = reparsed.moves().iter().map(PgnMove::san).collect();
        assert_eq!(a, b);
        assert_eq!(
            pgn.final_position().to_fen(),
            reparsed.final_position().to_fen()
        );
    }

    #[test]
    fn from_moves_round_trips() {
        let start = AnyVariant::startpos(VariantId::Standard);
        let mut pos = start.clone();
        let mut moves = Vec::new();
        for uci in ["e2e4", "e7e5", "g1f3", "b8c6"] {
            let mv = pos.parse_uci(uci).unwrap();
            moves.push(mv);
            pos = pos.play(&mv);
        }
        let pgn = Pgn::from_moves(start, &moves, vec![]).unwrap();
        let sans: Vec<&str> = pgn.moves().iter().map(PgnMove::san).collect();
        assert_eq!(sans, vec!["e4", "e5", "Nf3", "Nc6"]);
        // Replay the written PGN.
        let reparsed = Pgn::from_pgn(&pgn.to_pgn()).unwrap();
        let sans2: Vec<&str> = reparsed.moves().iter().map(PgnMove::san).collect();
        assert_eq!(sans, sans2);
    }

    #[test]
    fn black_to_move_start_writes_continuation_number() {
        // A start FEN with black to move should render `1...` before the first
        // (black) move, and re-parse to the same mainline.
        let text =
            "[SetUp \"1\"]\n[FEN \"rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1\"]\n\n\
             1... Nc6 2. Nf3 *\n";
        let pgn = Pgn::from_pgn(text).unwrap();
        assert_eq!(pgn.moves()[0].san(), "Nc6");
        let written = pgn.to_pgn();
        assert!(written.contains("1..."), "written: {written}");
        let reparsed = Pgn::from_pgn(&written).unwrap();
        let a: Vec<&str> = pgn.moves().iter().map(PgnMove::san).collect();
        let b: Vec<&str> = reparsed.moves().iter().map(PgnMove::san).collect();
        assert_eq!(a, b);
    }

    #[test]
    fn empty_input_is_an_empty_game() {
        let pgn = Pgn::from_pgn("").unwrap();
        assert!(pgn.tags().is_empty());
        assert!(pgn.moves().is_empty());
        assert_eq!(pgn.result(), PgnResult::Unknown);
    }

    #[test]
    fn malformed_tags_are_rejected() {
        for bad in [
            "[Event \"unterminated\n\n1. e4 *",
            "[Event no-quotes]\n\n*",
            "[ \"value only\"]\n\n*",
            "[Event]\n\n*",
        ] {
            assert_eq!(
                Pgn::from_pgn(bad),
                Err(PgnError::MalformedTag),
                "should reject {bad:?}"
            );
        }
    }

    #[test]
    fn unterminated_comment_is_rejected() {
        assert_eq!(
            Pgn::from_pgn("1. e4 {never closed\n"),
            Err(PgnError::UnterminatedComment)
        );
        assert_eq!(
            Pgn::from_pgn("1. e4 (1... e5"),
            Err(PgnError::UnterminatedComment)
        );
    }

    #[test]
    fn non_ascii_and_truncated_inputs_do_not_panic() {
        // None of these may panic; each must yield Ok or Err.
        let inputs = [
            "\u{1f600}",                                    // lone emoji
            "[Event \"\u{e9}\u{e9}\u{e9}\"]\n\n1. e4 e5 *", // non-ASCII tag value
            "1. \u{e9}4 e5 *",                              // non-ASCII inside movetext
            "1. e",                                         // truncated SAN
            "1.",                                           // truncated move number
            "[",                                            // truncated tag
            "[Event",                                       // truncated tag name
            "{",                                            // truncated comment
            "(",                                            // truncated variation
            "1. e4 \u{301}\u{301} e5 *",                    // combining marks
            "\u{e9}",                                       // single non-ASCII char
            "1-0 0-1 1/2-1/2 *",                            // multiple result tokens
            "1. O-O *",                                     // illegal castling from startpos
            "$",                                            // bare NAG marker
            "$abc",                                         // non-numeric NAG
            ";comment only\n",                              // line comment only
        ];
        for input in inputs {
            // The contract is only "no panic"; result may be Ok or Err.
            let _ = Pgn::from_pgn(input);
        }
    }

    #[test]
    fn tag_value_with_escapes_round_trips() {
        let text = "[Event \"He said \\\"hi\\\" \\\\ ok\"]\n\n*\n";
        let pgn = Pgn::from_pgn(text).unwrap();
        assert_eq!(pgn.tag("Event"), Some("He said \"hi\" \\ ok"));
        let reparsed = Pgn::from_pgn(&pgn.to_pgn()).unwrap();
        assert_eq!(reparsed.tag("Event"), Some("He said \"hi\" \\ ok"));
    }

    #[test]
    fn replays_full_immortal_game_to_final_position() {
        // The Immortal Game (Anderssen vs Kieseritzky, 1851), mainline.
        let text = "[Event \"Immortal\"]\n[Result \"1-0\"]\n\n\
            1. e4 e5 2. f4 exf4 3. Bc4 Qh4+ 4. Kf1 b5 5. Bxb5 Nf6 6. Nf3 Qh6 \
            7. d3 Nh5 8. Nh4 Qg5 9. Nf5 c6 10. g4 Nf6 11. Rg1 cxb5 12. h4 Qg6 \
            13. h5 Qg5 14. Qf3 Ng8 15. Bxf4 Qf6 16. Nc3 Bc5 17. Nd5 Qxb2 \
            18. Bd6 Bxg1 19. e5 Qxa1+ 20. Ke2 Na6 21. Nxg7+ Kd8 22. Qf6+ Nxf6 \
            23. Be7# 1-0\n";
        let pgn = Pgn::from_pgn(text).unwrap();
        assert_eq!(pgn.moves().len(), 45);
        assert_eq!(pgn.moves().last().unwrap().san(), "Be7#");
        let final_pos = pgn.final_position();
        assert!(final_pos.outcome().is_some());
        // Round-trip the whole game.
        let reparsed = Pgn::from_pgn(&pgn.to_pgn()).unwrap();
        let a: Vec<&str> = pgn.moves().iter().map(PgnMove::san).collect();
        let b: Vec<&str> = reparsed.moves().iter().map(PgnMove::san).collect();
        assert_eq!(a, b);
        assert_eq!(
            pgn.final_position().to_fen(),
            reparsed.final_position().to_fen()
        );
    }

    #[test]
    fn from_position_helper() {
        let pos = Position::from_fen("4k3/8/8/8/8/8/4P3/4K3 w - - 0 1").unwrap();
        let pgn = Pgn::from(pos);
        assert!(pgn.has_explicit_fen());
        assert_eq!(pgn.variant(), VariantId::Standard);
    }
}
