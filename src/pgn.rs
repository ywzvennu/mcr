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
//! The whole recursive variation *tree* is parsed and retained. The game is a
//! list of [`PgnNode`]s walking the mainline; each node carries its SAN-resolved
//! move, its NAGs and comments, and a list of *alternative* sub-lines (the RAV
//! `(...)` variations that branch from the position *before* that node). Each
//! variation is itself a list of nodes and may nest to any depth. The flat
//! [`Pgn::moves`] accessor is preserved (it is the mainline = walking the
//! top-level / first-child nodes); [`Pgn::mainline`] and per-node
//! [`PgnNode::variations`] expose the tree. Tag pairs, NAGs, `{...}`/`;`
//! comments (including embedded `[%clk ...]`/`[%emt ...]`/`[%eval ...]` command
//! tokens, preserved verbatim), and variations all round-trip.
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
//!
//! ```
//! use mce::Pgn;
//!
//! // A mainline with a variation branching at Black's first reply.
//! let pgn = Pgn::from_pgn("1. e4 e5 (1... c5 2. Nf3 d6) 2. Nf3 *\n").unwrap();
//! // The mainline is still e4 e5 Nf3.
//! let sans: Vec<&str> = pgn.moves().iter().map(|m| m.san()).collect();
//! assert_eq!(sans, ["e4", "e5", "Nf3"]);
//! // The second mainline node (1... e5) has one alternative: 1... c5 2. Nf3 d6.
//! let alt = &pgn.mainline()[1].variations()[0];
//! let alt_sans: Vec<&str> = alt.iter().map(|n| n.san()).collect();
//! assert_eq!(alt_sans, ["c5", "Nf3", "d6"]);
//! ```

use alloc::format;
#[cfg(test)]
use alloc::vec;
use alloc::{string::String, string::ToString, vec::Vec};
use core::fmt;

use crate::{AnyVariant, Move, Position, SanError, VariantId};

/// One `[Name "Value"]` header tag pair.
type TagPair = (String, String);

/// The product of parsing one movetext line: the parsed nodes, any preamble
/// comments (only non-empty for a top-level line), and a terminating result
/// token (only set when a top-level line ended on a result).
type ParsedLine = (Vec<PgnNode>, Vec<String>, Option<PgnResult>);

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

#[cfg(feature = "std")]
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

/// One node in the recursive variation tree.
///
/// A node holds the move played (as a concrete [`Move`] plus its canonical SAN),
/// the annotations attached to it (NAGs and comments), and the list of
/// *alternative* sub-lines that branch from the position *before* this move — the
/// RAV `(...)` variations. Each alternative is itself a `Vec<PgnNode>` and may
/// nest to any depth.
///
/// A line (the mainline, or any variation) is a `&[PgnNode]`: walking it forward
/// replays the moves in order, and at each step `variations()` gives the lines
/// that could have been played *instead of* that node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PgnNode {
    /// The concrete legal move, resolved against the position before it.
    mv: Move,
    /// The canonical SAN for the move, as rendered in the position before it.
    san: String,
    /// Numeric Annotation Glyphs (`$n`) attached to this move, in order.
    nags: Vec<u16>,
    /// `{...}`/`;` comments attached to this move, in order, without delimiters.
    comments: Vec<String>,
    /// Alternative sub-lines branching from the position *before* this move.
    /// Each is a full line of nodes; the order is the source `(...)` order.
    variations: Vec<Vec<PgnNode>>,
}

impl PgnNode {
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

    /// The comments attached to this move, in order (delimiter-free). Embedded
    /// `[%clk ...]`/`[%emt ...]`/`[%eval ...]` command tokens are kept verbatim.
    #[must_use]
    pub fn comments(&self) -> &[String] {
        &self.comments
    }

    /// The alternative sub-lines (RAV `(...)` variations) that branch from the
    /// position *before* this node, in source order. Empty if this node has no
    /// alternatives. Each sub-line is itself a slice of [`PgnNode`]s.
    #[must_use]
    pub fn variations(&self) -> &[Vec<PgnNode>] {
        &self.variations
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
    /// The mainline moves, in order. This is a flattened, annotation-bearing
    /// view of the top-level (first-child) walk of [`Pgn::tree`], kept in sync
    /// with it and preserved for the #106 API.
    moves: Vec<PgnMove>,
    /// The full recursive variation tree. The top-level vec is the mainline;
    /// each node may carry alternative sub-lines via [`PgnNode::variations`].
    tree: Vec<PgnNode>,
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

    /// The mainline as variation-tree nodes.
    ///
    /// This is the top-level line of the tree: walking it forward replays the
    /// mainline, and each node's `PgnNode::variations` gives the alternative
    /// sub-lines (RAV `(...)`) branching from the position before that node.
    #[must_use]
    pub fn mainline(&self) -> &[PgnNode] {
        &self.tree
    }

    /// Visits every node in the variation tree in depth-first, pre-order order,
    /// calling `visit(depth, node)` for each. The mainline is depth `0`; a node
    /// inside one of a node's `variations` is one deeper, and so on recursively.
    pub fn walk_tree<F: FnMut(usize, &PgnNode)>(&self, mut visit: F) {
        fn go<F: FnMut(usize, &PgnNode)>(line: &[PgnNode], depth: usize, visit: &mut F) {
            for node in line {
                visit(depth, node);
                for variation in &node.variations {
                    go(variation, depth + 1, visit);
                }
            }
        }
        go(&self.tree, 0, &mut visit);
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
            tree: Vec::new(),
            initial_comments: Vec::new(),
            result: header_result.unwrap_or_default(),
        };

        let mut parser = MovetextParser::new(rest);
        let (tree, initial_comments, movetext_result) = parser.parse_line(&start, true)?;
        game.tree = tree;
        game.initial_comments = initial_comments;
        game.moves = flatten_mainline(&game.tree);
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
        let (fullmove, white_to_move) = parse_side_and_number(&start_fen);

        // After the preamble comments a black-to-move start needs a continuation
        // marker before its first move; signal that with `force_number`.
        let force_number = !self.initial_comments.is_empty();
        write_line(
            &mut out,
            &mut line,
            &push_token,
            &self.tree,
            fullmove,
            white_to_move,
            force_number,
            false,
        );

        push_token(&mut out, &mut line, self.result.as_str());
        if !line.is_empty() {
            out.push_str(&line);
        }
        out.push('\n');
        out
    }
}

/// Emits one line of movetext (the mainline or a `(...)` variation body) into the
/// running `line`/`out` buffers, recursing into each node's alternative
/// variations. `fullmove`/`white_to_move` are the move number and side at the
/// start of `nodes`; `force_number` forces a move-number (or `N...`) marker
/// before the first move even when it is black's, used after a preamble or at the
/// start of a variation.
#[allow(clippy::too_many_arguments)]
fn write_line(
    out: &mut String,
    line: &mut String,
    push_token: &impl Fn(&mut String, &mut String, &str),
    nodes: &[PgnNode],
    mut fullmove: u32,
    mut white_to_move: bool,
    mut force_number: bool,
    mut glue: bool,
) {
    for node in nodes {
        // Render the move-number marker, gluing it onto a just-opened `(` when
        // this is the first token of a variation body.
        let number = if white_to_move {
            Some(format!("{fullmove}."))
        } else if force_number || line.is_empty() {
            // Black move after an interruption (variation, comment, wrap, or the
            // start of a variation line): re-emit the number with `...`.
            Some(format!("{fullmove}..."))
        } else {
            None
        };
        if let Some(num) = number {
            if glue {
                line.push_str(&num);
            } else {
                push_token(out, line, &num);
            }
            glue = false;
        }
        force_number = false;
        if glue {
            line.push_str(&node.san);
            glue = false;
        } else {
            push_token(out, line, &node.san);
        }
        for &nag in &node.nags {
            push_token(out, line, &format!("${nag}"));
        }
        for c in &node.comments {
            push_token(out, line, &format!("{{{c}}}"));
            // A comment interrupts move flow; the next move needs a number.
            force_number = true;
        }
        // Alternative sub-lines branch from the position *before* this node, so
        // they are emitted with this node's own move number / side.
        for variation in &node.variations {
            push_token(out, line, "(");
            write_line(
                out,
                line,
                push_token,
                variation,
                fullmove,
                white_to_move,
                true,
                true,
            );
            // Append `)` directly onto the last token without a leading space.
            if line.is_empty() {
                out.push(')');
            } else {
                line.push(')');
            }
            // After a variation, the next mainline move needs a number marker.
            force_number = true;
        }
        if !white_to_move {
            fullmove += 1;
        }
        white_to_move = !white_to_move;
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

/// Flattens the top-level (mainline) walk of a node tree into the legacy
/// [`PgnMove`] list, copying each mainline node's move, SAN, NAGs and comments.
fn flatten_mainline(tree: &[PgnNode]) -> Vec<PgnMove> {
    tree.iter()
        .map(|n| PgnMove {
            mv: n.mv,
            san: n.san.clone(),
            nags: n.nags.clone(),
            comments: n.comments.clone(),
        })
        .collect()
}

/// A recursive-descent parser over PGN movetext that builds the full variation
/// tree. A single shared byte cursor (`i`) walks the text; `parse_line` consumes
/// one line of moves (the mainline, or the body of one `(...)` variation),
/// recursing into nested variations and stopping at the `)` that closes its own
/// line (or at a result token / end of input for the top-level line).
struct MovetextParser<'a> {
    text: &'a str,
    bytes: &'a [u8],
    i: usize,
}

impl<'a> MovetextParser<'a> {
    fn new(text: &'a str) -> Self {
        MovetextParser {
            text,
            bytes: text.as_bytes(),
            i: 0,
        }
    }

    /// Parses one line of movetext played against `start`. When `top_level` is
    /// true, comments before the first move are collected as game preamble and a
    /// result token terminates the line; otherwise (inside a `(...)`) the line is
    /// terminated by its closing `)` and any result token is ignored.
    ///
    /// Returns the parsed nodes, the preamble comments (only non-empty at the top
    /// level), and the result token if one ended a top-level line.
    fn parse_line(&mut self, start: &AnyVariant, top_level: bool) -> Result<ParsedLine, PgnError> {
        let mut nodes: Vec<PgnNode> = Vec::new();
        let mut preamble: Vec<String> = Vec::new();
        let mut result = None;
        // Position before the most-recently-pushed node, so a `(...)` opened
        // after that node can be replayed from the same point.
        let mut pos_before_last = start.clone();
        // Position after the most-recently-pushed node (the running position).
        let mut pos = start.clone();

        while self.i < self.bytes.len() {
            let b = self.bytes[self.i];
            match b {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    self.i += 1;
                }
                // Comment to end of line.
                b';' => {
                    let rest = &self.text[self.i..];
                    let comment = rest
                        .find('\n')
                        .map_or(&rest[1..], |nl| &rest[1..nl])
                        .trim()
                        .to_string();
                    self.i = rest
                        .find('\n')
                        .map_or(self.bytes.len(), |nl| self.i + nl + 1);
                    Self::attach_comment(&mut nodes, &mut preamble, comment);
                }
                // Brace comment (may contain `[%clk ...]` etc.; kept verbatim).
                b'{' => {
                    let end = self.text[self.i + 1..]
                        .find('}')
                        .ok_or(PgnError::UnterminatedComment)?;
                    let comment = self.text[self.i + 1..self.i + 1 + end].trim().to_string();
                    self.i += 1 + end + 1;
                    Self::attach_comment(&mut nodes, &mut preamble, comment);
                }
                // Recursive variation: parse against the position before the last
                // move and attach the resulting line to that node as an alternative.
                b'(' => {
                    self.i += 1;
                    let (line, _, _) = self.parse_line(&pos_before_last, false)?;
                    if let Some(last) = nodes.last_mut() {
                        last.variations.push(line);
                    }
                    // else: a `(...)` with no preceding move; the parsed line is
                    // discarded (nothing legal to attach it to).
                }
                // Closing paren: end of this variation line.
                b')' => {
                    self.i += 1;
                    if top_level {
                        // A stray `)` at the top level — tolerate by ignoring it.
                        continue;
                    }
                    return Ok((nodes, preamble, result));
                }
                // NAG.
                b'$' => {
                    let mut j = self.i + 1;
                    while j < self.bytes.len() && self.bytes[j].is_ascii_digit() {
                        j += 1;
                    }
                    if j > self.i + 1 {
                        let nag: u16 = self.text[self.i + 1..j].parse().unwrap_or(0);
                        if let Some(last) = nodes.last_mut() {
                            last.nags.push(nag);
                        }
                    }
                    self.i = j;
                }
                _ => {
                    let tok_end = token_end(self.bytes, self.i);
                    let token = &self.text[self.i..tok_end];
                    self.i = tok_end;

                    if is_move_number(token) {
                        continue;
                    }
                    if let Some(r) = PgnResult::from_token(token) {
                        if top_level {
                            result = Some(r);
                            break;
                        }
                        // Result tokens inside a variation are ignored.
                        continue;
                    }
                    let san = strip_leading_move_number(token);
                    if san.is_empty() {
                        continue;
                    }
                    let mv =
                        variant_parse_san(&pos, san).map_err(|reason| PgnError::IllegalMove {
                            san: san.to_string(),
                            reason,
                        })?;
                    let canonical = variant_san(&pos, &mv);
                    pos_before_last = pos.clone();
                    pos = pos.play(&mv);
                    nodes.push(PgnNode {
                        mv,
                        san: canonical,
                        nags: Vec::new(),
                        comments: Vec::new(),
                        variations: Vec::new(),
                    });
                }
            }
        }

        if !top_level {
            // Reached end of input without the closing `)`.
            return Err(PgnError::UnterminatedComment);
        }
        Ok((nodes, preamble, result))
    }

    /// Attaches a comment to the last parsed node, or to the preamble when no
    /// move has been seen yet on this line.
    fn attach_comment(nodes: &mut [PgnNode], preamble: &mut Vec<String>, comment: String) {
        if let Some(last) = nodes.last_mut() {
            last.comments.push(comment);
        } else {
            preamble.push(comment);
        }
    }
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
        let mut tree = Vec::with_capacity(moves.len());
        for mv in moves {
            if !pos.legal_moves().iter().any(|legal| legal == mv) {
                return Err(PgnError::IllegalMove {
                    san: pos.to_uci(mv),
                    reason: SanError::Illegal,
                });
            }
            let san = variant_san(&pos, mv);
            pos = pos.play(mv);
            tree.push(PgnNode {
                mv: *mv,
                san,
                nags: Vec::new(),
                comments: Vec::new(),
                variations: Vec::new(),
            });
        }
        let recorded = flatten_mainline(&tree);

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
            tree,
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
            tree: Vec::new(),
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
        // An unterminated `(...)` variation (legal move, missing `)`) is caught
        // as the same "unterminated comment or variation" error.
        assert_eq!(
            Pgn::from_pgn("1. e4 (1. d4 d5"),
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

    /// Collects the SAN of a line of nodes.
    fn line_sans(line: &[PgnNode]) -> Vec<&str> {
        line.iter().map(PgnNode::san).collect()
    }

    #[test]
    fn parses_variation_tree_structure() {
        // Mainline e4 e5 Nf3 Nc6, with one alternative at each black reply and a
        // nested variation inside the first alternative.
        let text = "1. e4 e5 (1... c5 2. Nf3 (2. Nc3 Nc6) d6) 2. Nf3 Nc6 \
                     (2... d6 3. d4) *\n";
        let pgn = Pgn::from_pgn(text).unwrap();

        // Mainline is unchanged and flat-accessible.
        let sans: Vec<&str> = pgn.moves().iter().map(PgnMove::san).collect();
        assert_eq!(sans, ["e4", "e5", "Nf3", "Nc6"]);
        assert_eq!(line_sans(pgn.mainline()), ["e4", "e5", "Nf3", "Nc6"]);

        // The 1... e5 node (index 1) carries the Sicilian alternative.
        let e5 = &pgn.mainline()[1];
        assert_eq!(e5.san(), "e5");
        assert_eq!(e5.variations().len(), 1);
        let sicilian = &e5.variations()[0];
        assert_eq!(line_sans(sicilian), ["c5", "Nf3", "d6"]);

        // Inside the Sicilian, the Nf3 node has a nested 2. Nc3 alternative.
        let v_nf3 = &sicilian[1];
        assert_eq!(v_nf3.san(), "Nf3");
        assert_eq!(v_nf3.variations().len(), 1);
        assert_eq!(line_sans(&v_nf3.variations()[0]), ["Nc3", "Nc6"]);

        // The 2... Nc6 mainline node carries the 2... d6 alternative.
        let nc6 = &pgn.mainline()[3];
        assert_eq!(nc6.variations().len(), 1);
        assert_eq!(line_sans(&nc6.variations()[0]), ["d6", "d4"]);
    }

    #[test]
    fn walk_tree_visits_every_node_with_depth() {
        let text = "1. e4 e5 (1... c5 2. Nf3 (2. Nc3 Nc6) d6) 2. Nf3 *\n";
        let pgn = Pgn::from_pgn(text).unwrap();
        let mut seen: Vec<(usize, String)> = Vec::new();
        pgn.walk_tree(|depth, node| seen.push((depth, node.san().to_string())));
        // Pre-order: mainline e4, e5, then its depth-1 variation, with the
        // depth-2 nested line inside it, then mainline Nf3.
        let depths: Vec<usize> = seen.iter().map(|(d, _)| *d).collect();
        let sans: Vec<&str> = seen.iter().map(|(_, s)| s.as_str()).collect();
        assert_eq!(sans, ["e4", "e5", "c5", "Nf3", "Nc3", "Nc6", "d6", "Nf3"]);
        assert_eq!(depths, [0, 0, 1, 1, 2, 2, 1, 0]);
    }

    #[test]
    fn round_trips_annotated_game_with_variations() {
        let text = "[Event \"Annotated\"]\n[Result \"*\"]\n\n\
            1. e4 {good} e5 $1 (1... c5 {Sicilian} 2. Nf3 d6) \
            2. Nf3 Nc6 (2... d6 $6 {passive}) 3. Bb5 *\n";
        let pgn = Pgn::from_pgn(text).unwrap();
        let written = pgn.to_pgn();
        let reparsed = Pgn::from_pgn(&written).unwrap();

        // Mainline preserved.
        let a: Vec<&str> = pgn.moves().iter().map(PgnMove::san).collect();
        let b: Vec<&str> = reparsed.moves().iter().map(PgnMove::san).collect();
        assert_eq!(a, b);
        assert_eq!(a, ["e4", "e5", "Nf3", "Nc6", "Bb5"]);

        // The full tree (moves, NAGs, comments, variations) survives the round-trip.
        assert_eq!(pgn.mainline(), reparsed.mainline());

        // Spot-check annotations on the re-parsed tree.
        assert_eq!(reparsed.mainline()[0].comments(), &["good".to_string()]);
        assert_eq!(reparsed.mainline()[1].nags(), &[1]);
        let sicilian = &reparsed.mainline()[1].variations()[0];
        assert_eq!(sicilian[0].comments(), &["Sicilian".to_string()]);
        assert_eq!(line_sans(sicilian), ["c5", "Nf3", "d6"]);
    }

    #[test]
    fn preserves_clk_emt_eval_command_tokens_in_comments() {
        let text = "[Result \"*\"]\n\n\
            1. e4 {[%clk 0:01:23] [%eval 0.21]} e5 {[%emt 0:00:05]} 2. Nf3 *\n";
        let pgn = Pgn::from_pgn(text).unwrap();
        assert_eq!(
            pgn.mainline()[0].comments(),
            &["[%clk 0:01:23] [%eval 0.21]".to_string()]
        );
        assert_eq!(
            pgn.mainline()[1].comments(),
            &["[%emt 0:00:05]".to_string()]
        );
        // The command tokens are preserved verbatim across a round-trip.
        let reparsed = Pgn::from_pgn(&pgn.to_pgn()).unwrap();
        assert_eq!(reparsed.mainline(), pgn.mainline());
        assert!(reparsed.to_pgn().contains("[%clk 0:01:23]"));
        assert!(reparsed.to_pgn().contains("[%emt 0:00:05]"));
    }

    #[test]
    fn deeply_nested_variations_round_trip() {
        // Variations nested four deep, each branching from the position before
        // its parent move.
        let text = "1. e4 e5 (1... c5 2. Nf3 (2. Nc3 Nc6 (2... a6 3. g3)) d6) 2. Nf3 *\n";
        let pgn = Pgn::from_pgn(text).unwrap();
        let reparsed = Pgn::from_pgn(&pgn.to_pgn()).unwrap();
        assert_eq!(pgn.mainline(), reparsed.mainline());
        // Drill to the depth-3 line: e5 -> c5 -> Nf3 -> Nc3 -> Nc6 -> a6.
        let l1 = &pgn.mainline()[1].variations()[0]; // c5 Nf3 d6
        let l2 = &l1[1].variations()[0]; // Nc3 Nc6
        let l3 = &l2[1].variations()[0]; // a6 g3
        assert_eq!(line_sans(l3), ["a6", "g3"]);
    }

    #[test]
    fn variation_with_illegal_move_is_rejected() {
        // The (1... Ke7) variation, branching from after 1. e4, plays an illegal
        // king move (blocked by the pawn on e7).
        let text = "1. e4 e5 (1... Ke7) 2. Nf3 *\n";
        let err = Pgn::from_pgn(text).unwrap_err();
        assert!(matches!(err, PgnError::IllegalMove { .. }));
    }

    #[test]
    fn malformed_variation_inputs_do_not_panic() {
        let inputs = [
            "1. e4 (",                       // truncated variation
            "1. e4 ()",                      // empty variation
            "1. e4 (((",                     // many unterminated opens
            "1. e4 )))",                     // stray closes
            "1. e4 ( \u{1f600} )",           // emoji inside variation
            "1. e4 (1... \u{e9}5)",          // non-ASCII SAN in variation
            "(1. e4) 1. e4 *",               // variation before any mainline move
            "1. e4 (1... e5 {[%clk }) e5 *", // brace command split across paren
        ];
        for input in inputs {
            let _ = Pgn::from_pgn(input);
        }
    }
}
