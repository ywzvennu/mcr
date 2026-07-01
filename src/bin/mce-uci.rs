//! `mce-uci` — a thin UCI-ish adapter that drives the mce library over the
//! stdin/stdout text protocol, the mirror image of the `compare-fairy` harness
//! (which speaks the same protocol *to* a Fairy-Stockfish subprocess).
//!
//! This is a rules/movegen adapter, **not** a search engine: it deliberately
//! implements no `go` search or evaluation. It exists so external tooling that
//! already drives UCI perft engines (e.g. the Fairy-Stockfish `go perft`
//! divide format) can drive mce the same way, for differential perft.
//!
//! Supported commands:
//!
//! * `uci` — prints `id name`/`id author`, one
//!   `option name UCI_Variant type combo default chess var …` line listing
//!   every registered variant, then `uciok`.
//! * `setoption name UCI_Variant value <name>` — selects the variant (resetting
//!   to its start position). Any other option is accepted and ignored.
//! * `isready` — replies `readyok`.
//! * `ucinewgame` — accepted and ignored.
//! * `position startpos [moves …]` / `position fen <FEN> [moves …]` — sets the
//!   position from the variant's start array or an mce-dialect FEN, then applies
//!   the trailing UCI moves.
//! * `d` — prints the board grid and `Fen: <fen>`.
//! * `go perft <N>` — prints the per-root-move divide (`<uci>: <nodes>`), a
//!   blank line, then `Nodes searched: <total>` (the Fairy-Stockfish shape, so
//!   the same parsers read it).
//! * `quit` — exits.
//!
//! The adapter spans both variant families the library ships: the concrete
//! 8x8-engine variants reached through [`mce::AnyVariant`] / [`mce::VariantId`]
//! (standard chess, chess960, atomic, …) and the geometry-layer fairy variants
//! reached through [`mce::geometry::AnyWideVariant`] /
//! [`mce::geometry::WideVariantId`] (xiangqi, shogi, janggi, …). Every fairy
//! variant is enumerated from `WideVariantId::ALL`, so the combo option always
//! matches whatever the library currently registers.

use std::io::{self, BufRead, Write};

use mce::geometry::{AnyWideVariant, WideVariantId};
use mce::{AnyVariant, VariantId};

/// The concrete-engine variants, paired with the `UCI_Variant` name the combo
/// advertises. Each name round-trips through `VariantId::from_str`, and none
/// collides with a `WideVariantId` name, so selection is unambiguous.
const CLASSIC_VARIANTS: &[(&str, VariantId)] = &[
    ("chess", VariantId::Standard),
    ("chess960", VariantId::Chess960),
    ("atomic", VariantId::Atomic),
    ("antichess", VariantId::Antichess),
    ("crazyhouse", VariantId::Crazyhouse),
    ("kingofthehill", VariantId::KingOfTheHill),
    ("3check", VariantId::ThreeCheck),
    ("racingkings", VariantId::RacingKings),
    ("horde", VariantId::Horde),
];

/// Which variant family a selection resolves to.
#[derive(Debug, Clone, Copy)]
enum VariantKind {
    /// A concrete 8x8-engine variant (`AnyVariant`).
    Classic(VariantId),
    /// A geometry-layer fairy variant (`AnyWideVariant`).
    Wide(WideVariantId),
}

/// A runtime position of either family, behind one uniform surface.
// The `Wide` arm holds an `AnyWideVariant`, which stores positions of up to
// 10x10 inline (it carries its own `#[allow(clippy::large_enum_variant)]` for
// exactly this reason); mirror that choice here rather than box a short-lived,
// stack-only value.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
enum GamePos {
    Classic(AnyVariant),
    Wide(AnyWideVariant),
}

impl GamePos {
    /// The start position of `kind`.
    fn startpos(kind: VariantKind) -> Self {
        match kind {
            VariantKind::Classic(id) => GamePos::Classic(AnyVariant::startpos(id)),
            VariantKind::Wide(id) => GamePos::Wide(AnyWideVariant::startpos(id)),
        }
    }

    /// The position parsed from an mce-dialect `fen`, or an error message.
    fn from_fen(kind: VariantKind, fen: &str) -> Result<Self, String> {
        match kind {
            VariantKind::Classic(id) => AnyVariant::from_fen(id, fen)
                .map(GamePos::Classic)
                .map_err(|e| e.to_string()),
            VariantKind::Wide(id) => AnyWideVariant::from_fen(id, fen)
                .map(GamePos::Wide)
                .map_err(|e| e.to_string()),
        }
    }

    /// The FEN for the current position.
    fn to_fen(&self) -> String {
        match self {
            GamePos::Classic(p) => p.to_fen(),
            GamePos::Wide(p) => p.to_fen(),
        }
    }

    /// Applies the UCI move `uci`, returning the successor, or `None` if it names
    /// no legal move.
    fn play_uci(&self, uci: &str) -> Option<Self> {
        match self {
            GamePos::Classic(p) => p
                .parse_uci(uci)
                .ok()
                .map(|mv| GamePos::Classic(p.play(&mv))),
            GamePos::Wide(p) => p.play_uci(uci).map(GamePos::Wide),
        }
    }

    /// The `(uci, child)` pair for each legal root move — the raw material of a
    /// `go perft` divide.
    fn root_moves(&self) -> Vec<(String, GamePos)> {
        match self {
            GamePos::Classic(p) => p
                .legal_moves()
                .iter()
                .map(|mv| (p.to_uci(mv), GamePos::Classic(p.play(mv))))
                .collect(),
            GamePos::Wide(p) => p
                .legal_moves()
                .iter()
                .map(|mv| (p.to_uci(mv), GamePos::Wide(p.play(mv))))
                .collect(),
        }
    }

    /// The perft node count to `depth`.
    fn perft(&self, depth: u32) -> u64 {
        match self {
            GamePos::Classic(p) => p.perft(depth),
            GamePos::Wide(p) => p.perft(depth),
        }
    }
}

/// Resolves a `UCI_Variant` value to a [`VariantKind`], trying the concrete
/// engine first and the geometry layer second. Returns `None` for an unknown
/// name.
fn resolve_variant(value: &str) -> Option<VariantKind> {
    if let Ok(id) = value.parse::<VariantId>() {
        return Some(VariantKind::Classic(id));
    }
    if let Ok(id) = value.parse::<WideVariantId>() {
        return Some(VariantKind::Wide(id));
    }
    None
}

/// The `UCI_Variant` names advertised in the combo option, concrete variants
/// first and then every registered fairy variant, in declaration order.
fn all_variant_names() -> Vec<String> {
    let mut names: Vec<String> = CLASSIC_VARIANTS
        .iter()
        .map(|(name, _)| (*name).to_string())
        .collect();
    names.extend(WideVariantId::ALL.iter().map(|id| id.as_str().to_string()));
    names
}

/// The full `option name UCI_Variant type combo …` line.
fn uci_variant_option() -> String {
    let vars = all_variant_names()
        .iter()
        .map(|n| format!("var {n}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!("option name UCI_Variant type combo default chess {vars}")
}

/// Splits a `setoption` command into its option name and value, e.g.
/// `setoption name UCI_Variant value xiangqi` → `("UCI_Variant", "xiangqi")`.
/// The name is everything between `name` and `value`; the value is the rest.
fn parse_setoption(tokens: &[&str]) -> Option<(String, String)> {
    let name_at = tokens.iter().position(|t| *t == "name")?;
    let value_at = tokens.iter().position(|t| *t == "value");
    let name_end = value_at.unwrap_or(tokens.len());
    let name = tokens.get(name_at + 1..name_end)?.join(" ");
    let value = match value_at {
        Some(i) => tokens.get(i + 1..).unwrap_or(&[]).join(" "),
        None => String::new(),
    };
    if name.is_empty() {
        return None;
    }
    Some((name, value))
}

/// Prints the placement field of `fen` as a plain grid, one rank per line, then
/// the `Fen:` line — a compact `d` that works for every board geometry (any
/// rank/file count) since it reads the FEN placement directly.
fn print_board<W: Write>(out: &mut W, fen: &str) -> io::Result<()> {
    let placement = fen.split(' ').next().unwrap_or("");
    // Drop a crazyhouse/shogi-style pocket suffix; it is not part of the grid.
    let placement = placement.split('[').next().unwrap_or(placement);
    for rank in placement.split('/') {
        let mut cells: Vec<String> = Vec::new();
        let mut empties = 0u32;
        let mut pending = String::new();
        for ch in rank.chars() {
            if ch.is_ascii_digit() {
                empties = empties * 10 + ch.to_digit(10).unwrap_or(0);
                continue;
            }
            if empties > 0 {
                for _ in 0..empties {
                    cells.push(".".to_string());
                }
                empties = 0;
            }
            // A leading `+` (promoted shogi piece) binds to the next letter.
            if ch == '+' {
                pending.push(ch);
                continue;
            }
            pending.push(ch);
            cells.push(std::mem::take(&mut pending));
        }
        for _ in 0..empties {
            cells.push(".".to_string());
        }
        if !pending.is_empty() {
            cells.push(pending);
        }
        writeln!(out, "{}", cells.join(" "))?;
    }
    writeln!(out, "Fen: {fen}")
}

/// Runs one `go perft <depth>` on `pos`, printing the divide and total in the
/// Fairy-Stockfish shape.
fn go_perft<W: Write>(out: &mut W, pos: &GamePos, depth: u32) -> io::Result<()> {
    let total = if depth == 0 {
        1
    } else {
        let mut total = 0u64;
        for (uci, child) in pos.root_moves() {
            let count = child.perft(depth - 1);
            writeln!(out, "{uci}: {count}")?;
            total += count;
        }
        total
    };
    writeln!(out)?;
    writeln!(out, "Nodes searched: {total}")
}

/// The adapter's mutable state: the selected variant and the current position.
#[derive(Debug)]
struct Adapter {
    kind: VariantKind,
    pos: GamePos,
}

impl Adapter {
    /// A fresh adapter defaulting to standard chess at the start position.
    fn new() -> Self {
        let kind = VariantKind::Classic(VariantId::Standard);
        Adapter {
            kind,
            pos: GamePos::startpos(kind),
        }
    }

    /// Handles `setoption`, selecting a new variant on `UCI_Variant`.
    fn set_option(&mut self, tokens: &[&str], err: &mut impl Write) -> io::Result<()> {
        let Some((name, value)) = parse_setoption(tokens) else {
            return Ok(());
        };
        if !name.eq_ignore_ascii_case("UCI_Variant") {
            // Silently accept unknown options (e.g. UCI_Chess960), like a real
            // engine, so drivers that always set them do not error out.
            return Ok(());
        }
        match resolve_variant(&value) {
            Some(kind) => {
                self.kind = kind;
                self.pos = GamePos::startpos(kind);
            }
            None => writeln!(err, "info string unknown variant: {value}")?,
        }
        Ok(())
    }

    /// Handles `position startpos|fen … [moves …]`.
    fn set_position(&mut self, tokens: &[&str], err: &mut impl Write) -> io::Result<()> {
        // Everything up to a `moves` token is the position spec; the rest are
        // moves to apply in order.
        let moves_at = tokens.iter().position(|t| *t == "moves");
        let spec_end = moves_at.unwrap_or(tokens.len());
        let spec = &tokens[1..spec_end];

        let base = match spec.first().copied() {
            Some("startpos") => GamePos::startpos(self.kind),
            Some("fen") => {
                let fen = spec[1..].join(" ");
                match GamePos::from_fen(self.kind, &fen) {
                    Ok(p) => p,
                    Err(e) => {
                        writeln!(err, "info string bad fen: {e}")?;
                        return Ok(());
                    }
                }
            }
            _ => {
                writeln!(err, "info string malformed position command")?;
                return Ok(());
            }
        };

        let mut pos = base;
        if let Some(i) = moves_at {
            for uci in &tokens[i + 1..] {
                match pos.play_uci(uci) {
                    Some(next) => pos = next,
                    None => {
                        writeln!(err, "info string illegal move: {uci}")?;
                        return Ok(());
                    }
                }
            }
        }
        self.pos = pos;
        Ok(())
    }

    /// Dispatches one protocol line. Returns `false` on `quit`.
    fn handle_line<W: Write, E: Write>(
        &mut self,
        line: &str,
        out: &mut W,
        err: &mut E,
    ) -> io::Result<bool> {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let Some(&cmd) = tokens.first() else {
            return Ok(true);
        };
        match cmd {
            "uci" => {
                writeln!(out, "id name mce-uci {}", env!("CARGO_PKG_VERSION"))?;
                writeln!(out, "id author {}", env!("CARGO_PKG_AUTHORS"))?;
                writeln!(out, "{}", uci_variant_option())?;
                writeln!(out, "uciok")?;
            }
            "isready" => writeln!(out, "readyok")?,
            "ucinewgame" => {}
            "setoption" => self.set_option(&tokens, err)?,
            "position" => self.set_position(&tokens, err)?,
            "d" => print_board(out, &self.pos.to_fen())?,
            "go" => {
                // Rules/movegen only: the sole supported `go` is `go perft <N>`.
                match (
                    tokens.get(1).copied(),
                    tokens.get(2).and_then(|d| d.parse().ok()),
                ) {
                    (Some("perft"), Some(depth)) => go_perft(out, &self.pos, depth)?,
                    _ => writeln!(err, "info string only 'go perft <depth>' is supported")?,
                }
            }
            "quit" => return Ok(false),
            _ => {}
        }
        Ok(true)
    }
}

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let mut out = io::stdout().lock();
    let mut err = io::stderr().lock();
    let mut adapter = Adapter::new();

    for line in stdin.lock().lines() {
        let line = line?;
        let keep_going = adapter.handle_line(line.trim(), &mut out, &mut err)?;
        // Flush every turn: the driver reads line-by-line, so buffered output
        // must reach it before we block on the next read.
        out.flush()?;
        err.flush()?;
        if !keep_going {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_both_families() {
        assert!(matches!(
            resolve_variant("chess"),
            Some(VariantKind::Classic(VariantId::Standard))
        ));
        assert!(matches!(
            resolve_variant("standard"),
            Some(VariantKind::Classic(VariantId::Standard))
        ));
        assert!(matches!(
            resolve_variant("xiangqi"),
            Some(VariantKind::Wide(WideVariantId::Xiangqi))
        ));
        assert!(matches!(
            resolve_variant("korean"),
            Some(VariantKind::Wide(WideVariantId::Janggi))
        ));
        assert!(resolve_variant("not-a-variant").is_none());
    }

    #[test]
    fn combo_lists_every_registered_variant() {
        let names = all_variant_names();
        assert!(names.iter().any(|n| n == "chess"));
        for id in WideVariantId::ALL {
            assert!(names.iter().any(|n| n == id.as_str()), "combo missing {id}");
        }
        // Concrete family plus the whole geometry family.
        assert_eq!(
            names.len(),
            CLASSIC_VARIANTS.len() + WideVariantId::ALL.len()
        );
    }

    #[test]
    fn classic_and_wide_names_never_collide() {
        for (name, _) in CLASSIC_VARIANTS {
            assert!(
                name.parse::<WideVariantId>().is_err(),
                "{name} collides with a fairy variant"
            );
        }
    }

    #[test]
    fn setoption_splits_name_and_value() {
        let toks: Vec<&str> = "setoption name UCI_Variant value xiangqi"
            .split(' ')
            .collect();
        assert_eq!(
            parse_setoption(&toks),
            Some(("UCI_Variant".to_string(), "xiangqi".to_string()))
        );
        let toks: Vec<&str> = "setoption name UCI_Chess960 value true"
            .split(' ')
            .collect();
        assert_eq!(
            parse_setoption(&toks),
            Some(("UCI_Chess960".to_string(), "true".to_string()))
        );
    }

    #[test]
    fn standard_startpos_perft() {
        let pos = GamePos::startpos(VariantKind::Classic(VariantId::Standard));
        assert_eq!(pos.perft(4), 197_281);
    }

    #[test]
    fn xiangqi_startpos_perft() {
        let pos = GamePos::startpos(VariantKind::Wide(WideVariantId::Xiangqi));
        assert_eq!(pos.perft(3), 79_666);
    }

    #[test]
    fn position_moves_apply() {
        let mut adapter = Adapter::new();
        let mut out = Vec::new();
        let mut err = Vec::new();
        adapter
            .handle_line("position startpos moves e2e4 e7e5", &mut out, &mut err)
            .unwrap();
        assert!(adapter
            .pos
            .to_fen()
            .starts_with("rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR"));
    }

    #[test]
    fn go_perft_output_shape() {
        let pos = GamePos::startpos(VariantKind::Classic(VariantId::Standard));
        let mut out = Vec::new();
        go_perft(&mut out, &pos, 1).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("e2e4: 1"));
        assert!(text.trim_end().ends_with("Nodes searched: 20"));
    }
}
