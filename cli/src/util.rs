//! Shared helpers: the error type, FEN/variant parsing, board rendering, and a
//! tiny deterministic RNG used by `mcr play`.

use std::fmt;

use mcr::geometry::{AnyWideVariant, WideVariantId};
use mcr::{AnyVariant, CastleSide, Color, File, Position, Rank, Square, VariantId};

/// The standard chess starting FEN, used as the expansion of `startpos`.
pub const STARTPOS_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// A flat error carrying a human-readable message. Every command returns this;
/// `main` prints it to stderr and exits nonzero, so bad input never panics.
#[derive(Debug)]
pub struct CliError(pub String);

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CliError {}

impl CliError {
    /// Builds a `CliError` from anything displayable.
    pub fn new(msg: impl fmt::Display) -> CliError {
        CliError(msg.to_string())
    }
}

/// The `Result` alias every command uses.
pub type CliResult<T = ()> = Result<T, CliError>;

/// Expands the `startpos` shorthand to the standard starting FEN; otherwise
/// returns the input unchanged.
pub fn resolve_fen(fen: &str) -> &str {
    if fen == "startpos" {
        STARTPOS_FEN
    } else {
        fen
    }
}

/// Parses a variant name (anything [`VariantId`] accepts: `standard`, `chess`,
/// `atomic`, `koth`, `3check`, ...). `None` means standard chess.
pub fn parse_variant(name: Option<&str>) -> CliResult<VariantId> {
    match name {
        None => Ok(VariantId::Standard),
        Some(name) => name.parse::<VariantId>().map_err(CliError::new),
    }
}

/// Whether a variant id is plain standard chess (the only arm with full SAN /
/// `Position` access in this tool).
pub fn is_standard(id: VariantId) -> bool {
    id == VariantId::Standard
}

/// Loads a standard [`Position`] from a (already `startpos`-resolved) FEN.
pub fn load_position(fen: &str) -> CliResult<Position> {
    Position::from_fen(fen).map_err(|err| CliError::new(format!("bad FEN: {err}")))
}

/// Loads an [`AnyVariant`] for a runtime-chosen variant from a FEN.
pub fn load_variant(id: VariantId, fen: &str) -> CliResult<AnyVariant> {
    AnyVariant::from_fen(id, fen).map_err(|err| CliError::new(format!("bad FEN for {id}: {err}")))
}

/// Parses a geometry-layer fairy variant name (anything [`WideVariantId`]
/// accepts: `xiangqi`, `shogi`, `janggi`, `orda`, ... plus their aliases).
pub fn parse_wide_variant(name: &str) -> CliResult<WideVariantId> {
    name.parse::<WideVariantId>().map_err(CliError::new)
}

/// Loads an [`AnyWideVariant`] for a runtime-chosen fairy variant. Unlike the
/// concrete engine, every fairy variant has its own start array, so `startpos`
/// resolves to that variant's opening rather than a single shared FEN.
pub fn load_wide_variant(id: WideVariantId, fen: &str) -> CliResult<AnyWideVariant> {
    if fen == "startpos" {
        Ok(AnyWideVariant::startpos(id))
    } else {
        AnyWideVariant::from_fen(id, fen)
            .map_err(|err| CliError::new(format!("bad FEN for {id}: {err}")))
    }
}

/// The human-readable side name.
pub fn side_name(color: Color) -> &'static str {
    if color.is_white() {
        "white"
    } else {
        "black"
    }
}

/// Renders castling rights in FEN order (`KQkq`), or `-` when none remain.
pub fn castling_string(pos: &Position) -> String {
    let rights = pos.castling_rights();
    let mut s = String::new();
    for (color, side, letter) in [
        (Color::White, CastleSide::King, 'K'),
        (Color::White, CastleSide::Queen, 'Q'),
        (Color::Black, CastleSide::King, 'k'),
        (Color::Black, CastleSide::Queen, 'q'),
    ] {
        if rights.has(color, side) {
            s.push(letter);
        }
    }
    if s.is_empty() {
        s.push('-');
    }
    s
}

/// Renders a bitboard of squares as a space-separated list, or `-` when empty.
pub fn squares_string(bb: mcr::Bitboard) -> String {
    if bb.is_empty() {
        return "-".to_owned();
    }
    bb.into_iter()
        .map(|sq| sq.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Prints a FEN board field as an 8x8 grid, rank 8 at the top, dots for empty
/// squares, with file and rank labels. Works for any variant — the piece
/// placement field of a FEN has the same shape across all of them.
pub fn print_board_fen(fen: &str) {
    // The first space-separated field is the piece placement; render it.
    let placement = fen.split(' ').next().unwrap_or("");
    match mcr::Board::from_fen_placement(placement) {
        Ok(board) => print_board(&board),
        // Fall back to a literal echo if the placement is unexpectedly shaped;
        // callers only pass FENs the library already accepted.
        Err(_) => println!("{placement}"),
    }
}

/// Prints an [`mcr::Board`] as an 8x8 grid with labels.
pub fn print_board(board: &mcr::Board) {
    for rank_idx in (0..8).rev() {
        let rank = Rank::new(rank_idx).expect("0..8 is a valid rank index");
        print!("{}  ", rank.char());
        for file_idx in 0..8 {
            let file = File::new(file_idx).expect("0..8 is a valid file index");
            let sq = Square::from_file_rank(file, rank);
            match board.piece_at(sq) {
                Some(piece) => print!("{} ", piece.char()),
                None => print!(". "),
            }
        }
        println!();
    }
    print!("   ");
    for file_idx in 0..8 {
        let file = File::new(file_idx).expect("0..8 is a valid file index");
        print!("{} ", file.char());
    }
    println!();
}

/// A minimal `splitmix64` PRNG — deterministic, fast, good enough for picking
/// moves (not for cryptography). Same seed ⇒ same game.
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Seeds the generator.
    pub fn new(seed: u64) -> Self {
        SplitMix64 { state: seed }
    }

    /// Returns the next 64-bit output and advances the state.
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A uniform value in `0..bound` via Lemire's multiply-high method.
    pub fn below(&mut self, bound: u64) -> u64 {
        ((u128::from(self.next_u64()) * u128::from(bound)) >> 64) as u64
    }
}
